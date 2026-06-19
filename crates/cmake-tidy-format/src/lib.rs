use cmake_tidy_ast::{Statement, TextRange};
use cmake_tidy_config::FormatConfiguration;
use cmake_tidy_lexer::{Token, TokenKind, tokenize};
use cmake_tidy_parser::parse_file;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatResult {
    pub output: String,
    pub changed: bool,
}

#[must_use]
pub fn format_source(source: &str) -> FormatResult {
    format_source_with_options(source, &FormatConfiguration::default())
}

#[must_use]
pub fn format_source_with_options(source: &str, options: &FormatConfiguration) -> FormatResult {
    let initial_protected_ranges = protected_ranges(source);
    let normalized_parens = normalize_space_before_paren(
        source,
        &initial_protected_ranges,
        options.space_before_paren,
    );
    let parens_protected_ranges = protected_ranges(&normalized_parens);
    let indented = normalize_indentation(&normalized_parens, &parens_protected_ranges, options);
    let indented_protected_ranges = protected_ranges(&indented);
    let output = normalize_lines(&indented, &indented_protected_ranges, options);
    let changed = output != source;
    FormatResult { output, changed }
}

fn protected_ranges(source: &str) -> Vec<TextRange> {
    let tokens = tokenize(source);
    let mut ranges = tokens
        .iter()
        .filter_map(|token| match token.kind {
            TokenKind::BracketArgument(_) => Some(token.range),
            _ => None,
        })
        .collect::<Vec<_>>();
    ranges.extend(format_disabled_ranges(source, &tokens));
    merge_ranges(ranges)
}

fn format_disabled_ranges(source: &str, tokens: &[Token]) -> Vec<TextRange> {
    let mut ranges = Vec::new();
    let mut disabled_start = None;

    for token in tokens {
        let TokenKind::Comment(text) = &token.kind else {
            continue;
        };

        if text.trim() == "# cmake-format: off" {
            if disabled_start.is_none() {
                disabled_start = Some(line_start(source, token.range.start));
            }
        } else if text.trim() == "# cmake-format: on"
            && let Some(start) = disabled_start.take()
        {
            ranges.push(TextRange::new(start, line_end(source, token.range.end)));
        }
    }

    if let Some(start) = disabled_start {
        ranges.push(TextRange::new(start, source.len()));
    }

    ranges
}

fn merge_ranges(mut ranges: Vec<TextRange>) -> Vec<TextRange> {
    if ranges.len() < 2 {
        return ranges;
    }

    ranges.sort_by_key(|range| (range.start, range.end));
    let mut merged: Vec<TextRange> = Vec::with_capacity(ranges.len());

    for range in ranges {
        if let Some(last) = merged.last_mut()
            && range.start <= last.end
        {
            last.end = last.end.max(range.end);
            continue;
        }

        merged.push(range);
    }

    merged
}

fn line_start(source: &str, offset: usize) -> usize {
    let bytes = source.as_bytes();
    let mut start = offset.min(bytes.len());

    while start > 0 && !matches!(bytes[start - 1], b'\n' | b'\r') {
        start -= 1;
    }

    start
}

fn line_end(source: &str, offset: usize) -> usize {
    let bytes = source.as_bytes();
    let mut end = offset.min(bytes.len());

    while end < bytes.len() && !matches!(bytes[end], b'\n' | b'\r') {
        end += 1;
    }

    if bytes.get(end) == Some(&b'\r') && bytes.get(end + 1) == Some(&b'\n') {
        end + 2
    } else if bytes.get(end).is_some() {
        end + 1
    } else {
        end
    }
}

fn normalize_space_before_paren(
    source: &str,
    protected_ranges: &[TextRange],
    enabled: bool,
) -> String {
    let tokens = tokenize(source);
    let mut output = String::with_capacity(source.len());
    let mut offset = 0;
    let mut index = 0;

    while index < tokens.len() {
        if index + 2 < tokens.len() {
            let first = &tokens[index];
            let second = &tokens[index + 1];
            let third = &tokens[index + 2];

            if matches!(first.kind, TokenKind::Identifier(_))
                && matches!(second.kind, TokenKind::Whitespace(_))
                && matches!(third.kind, TokenKind::LeftParen)
                && !source[second.range.start..second.range.end].contains(['\n', '\r'])
                && !overlaps_protected_range(
                    TextRange::new(first.range.start, third.range.end),
                    protected_ranges,
                )
            {
                output.push_str(&source[offset..second.range.start]);
                if enabled {
                    output.push(' ');
                }
                offset = second.range.end;
                index += 2;
                continue;
            }
        }

        if index + 1 < tokens.len()
            && matches!(tokens[index].kind, TokenKind::Identifier(_))
            && matches!(tokens[index + 1].kind, TokenKind::LeftParen)
            && enabled
            && !overlaps_protected_range(
                TextRange::new(tokens[index].range.start, tokens[index + 1].range.end),
                protected_ranges,
            )
        {
            let insert_at = tokens[index + 1].range.start;
            output.push_str(&source[offset..insert_at]);
            output.push(' ');
            offset = insert_at;
            index += 1;
            continue;
        }

        index += 1;
    }

    output.push_str(&source[offset..]);
    output
}

/// A `CMake` command that increases block depth for the statements that follow it.
fn is_block_opener(name: &str) -> bool {
    matches!(
        name,
        "if" | "foreach" | "while" | "function" | "macro" | "block"
    )
}

/// A `CMake` command that closes the most recent block.
fn is_block_closer(name: &str) -> bool {
    matches!(
        name,
        "endif" | "endforeach" | "endwhile" | "endfunction" | "endmacro" | "endblock"
    )
}

/// A `CMake` command that sits one level out from its block body without changing depth.
fn is_block_midpoint(name: &str) -> bool {
    matches!(name, "else" | "elseif")
}

/// What to do with the leading whitespace of a single physical line.
#[derive(Debug, Clone)]
enum IndentAction {
    /// Leave the line untouched (protected ranges, or lines we cannot reason about).
    Keep,
    /// Replace the leading whitespace with `level` indent units (blank lines stay empty).
    Set(usize),
    /// Continuation line of a multi-line command: swap the first line's old indent
    /// prefix for its new one, preserving any deeper hand-alignment.
    Shift { from: String, to: String },
}

/// Re-indent each statement and standalone comment to match `CMake` block nesting.
///
/// Depth is derived from block-opening/closing command names because the AST is a flat
/// list of statements rather than a nested block tree. Bracket-argument and
/// format-disabled lines are left verbatim.
fn normalize_indentation(
    source: &str,
    protected_ranges: &[TextRange],
    options: &FormatConfiguration,
) -> String {
    let parsed = parse_file(source);
    let line_starts = line_start_offsets(source);
    let line_of = |offset: usize| match line_starts.binary_search(&offset) {
        Ok(index) => index,
        Err(index) => index.saturating_sub(1),
    };

    let mut actions = vec![IndentAction::Keep; line_starts.len()];
    let unit = options.indent_unit();
    let mut depth: usize = 0;
    let mut assigned_through = 0;

    for item in &parsed.syntax.items {
        let Statement::Command(command) = item;

        // Disabled regions are rendered verbatim and must not perturb depth tracking.
        if overlaps_protected_range(command.range, protected_ranges) {
            continue;
        }

        let name = command.name.text.to_ascii_lowercase();
        let first_line = line_of(command.range.start);
        let last_line = line_of(command.range.end.saturating_sub(1).max(command.range.start));

        // Standalone comment/blank lines before this statement follow the surrounding depth.
        if first_line >= assigned_through {
            for action in &mut actions[assigned_through..first_line] {
                *action = IndentAction::Set(depth);
            }
        }

        let render_depth = if is_block_midpoint(&name) {
            depth.saturating_sub(1)
        } else if is_block_closer(&name) {
            depth = depth.saturating_sub(1);
            depth
        } else {
            depth
        };

        if first_line >= assigned_through {
            let first_lead = leading_whitespace(source, &line_starts, first_line).to_owned();
            actions[first_line] = IndentAction::Set(render_depth);
            let new_lead = unit.repeat(render_depth);
            for action in &mut actions[first_line + 1..=last_line] {
                *action = IndentAction::Shift {
                    from: first_lead.clone(),
                    to: new_lead.clone(),
                };
            }
        }

        if is_block_opener(&name) {
            depth += 1;
        }

        assigned_through = (last_line + 1).max(assigned_through);
    }

    for action in &mut actions[assigned_through..] {
        *action = IndentAction::Set(depth);
    }

    render_indentation(source, &line_starts, &actions, protected_ranges, &unit)
}

fn render_indentation(
    source: &str,
    line_starts: &[usize],
    actions: &[IndentAction],
    protected_ranges: &[TextRange],
    unit: &str,
) -> String {
    let mut output = String::with_capacity(source.len());

    for (index, &start) in line_starts.iter().enumerate() {
        let end = line_starts.get(index + 1).copied().unwrap_or(source.len());
        let line = &source[start..end];

        if overlaps_protected_range(TextRange::new(start, end), protected_ranges) {
            output.push_str(line);
            continue;
        }

        let lead_len = leading_whitespace_len(line);
        let lead = &line[..lead_len];
        let rest = &line[lead_len..];
        let is_blank = rest.is_empty() || rest.starts_with(['\n', '\r']);

        match &actions[index] {
            IndentAction::Keep => output.push_str(line),
            IndentAction::Set(level) => {
                if !is_blank {
                    output.push_str(&unit.repeat(*level));
                }
                output.push_str(rest);
            }
            IndentAction::Shift { from, to } => {
                if is_blank {
                    output.push_str(rest);
                } else if let Some(extra) = lead.strip_prefix(from.as_str()) {
                    output.push_str(to);
                    output.push_str(extra);
                    output.push_str(rest);
                } else {
                    output.push_str(line);
                }
            }
        }
    }

    output
}

/// Byte offsets where each physical line begins, handling `\n` and `\r\n`.
fn line_start_offsets(source: &str) -> Vec<usize> {
    let bytes = source.as_bytes();
    let mut starts = vec![0];
    let mut offset = 0;

    while offset < bytes.len() {
        match bytes[offset] {
            b'\n' => {
                starts.push(offset + 1);
                offset += 1;
            }
            b'\r' => {
                let step = if bytes.get(offset + 1) == Some(&b'\n') {
                    2
                } else {
                    1
                };
                starts.push(offset + step);
                offset += step;
            }
            _ => offset += 1,
        }
    }

    starts
}

fn leading_whitespace<'a>(source: &'a str, line_starts: &[usize], line: usize) -> &'a str {
    let start = line_starts[line];
    let end = line_starts.get(line + 1).copied().unwrap_or(source.len());
    let text = &source[start..end];
    &text[..leading_whitespace_len(text)]
}

fn leading_whitespace_len(line: &str) -> usize {
    line.bytes()
        .take_while(|byte| matches!(byte, b' ' | b'\t'))
        .count()
}

fn normalize_lines(
    source: &str,
    protected_ranges: &[TextRange],
    options: &FormatConfiguration,
) -> String {
    let bytes = source.as_bytes();
    let mut kept_lines = Vec::new();
    let mut offset = 0;
    let mut blank_run = 0;

    while offset < bytes.len() {
        let mut line_end = offset;
        while line_end < bytes.len() && !matches!(bytes[line_end], b'\n' | b'\r') {
            line_end += 1;
        }

        let newline_end = if line_end == bytes.len() {
            line_end
        } else if bytes[line_end] == b'\r' && bytes.get(line_end + 1) == Some(&b'\n') {
            line_end + 2
        } else {
            line_end + 1
        };

        let trim_end = trimmed_line_end(bytes, offset, line_end);
        let line_range = TextRange::new(offset, line_end);
        let line_protected = overlaps_protected_range(line_range, protected_ranges);
        let preserve_trailing = trim_end != line_end
            && overlaps_protected_range(TextRange::new(trim_end, line_end), protected_ranges);

        let content = if trim_end != line_end && !preserve_trailing {
            &source[offset..trim_end]
        } else {
            &source[offset..line_end]
        };
        let newline = &source[line_end..newline_end];
        let is_blank = !line_protected && content.is_empty();

        if is_blank {
            blank_run += 1;
            if blank_run <= options.max_blank_lines {
                kept_lines.push((content.to_owned(), newline.to_owned(), line_protected));
            }
        } else {
            blank_run = 0;
            kept_lines.push((content.to_owned(), newline.to_owned(), line_protected));
        }

        offset = newline_end;
    }

    while kept_lines
        .last()
        .is_some_and(|(content, _newline, protected)| !*protected && content.is_empty())
    {
        kept_lines.pop();
    }

    let preferred_newline = preferred_newline(source);
    let mut output = String::with_capacity(source.len());
    for (index, (content, newline, _protected)) in kept_lines.iter().enumerate() {
        output.push_str(content);

        let is_last = index + 1 == kept_lines.len();
        if is_last {
            if !newline.is_empty() && options.final_newline {
                output.push_str(preferred_newline);
            }
        } else if newline.is_empty() {
            output.push_str(preferred_newline);
        } else {
            output.push_str(newline);
        }
    }

    let protected_to_eof = protected_ranges
        .iter()
        .any(|range| range.end == source.len());
    if options.final_newline
        && !protected_to_eof
        && (kept_lines.is_empty() || (!output.ends_with('\n') && !output.ends_with("\r\n")))
    {
        output.push_str(preferred_newline);
    }

    output
}

fn trimmed_line_end(bytes: &[u8], line_start: usize, line_end: usize) -> usize {
    let mut trim_end = line_end;
    while trim_end > line_start && matches!(bytes[trim_end - 1], b' ' | b'\t') {
        trim_end -= 1;
    }
    trim_end
}

fn overlaps_protected_range(range: TextRange, protected_ranges: &[TextRange]) -> bool {
    protected_ranges
        .iter()
        .any(|protected| range.start < protected.end && protected.start < range.end)
}

fn preferred_newline(source: &str) -> &str {
    if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

#[cfg(test)]
mod tests {
    use cmake_tidy_config::{FormatConfiguration, IndentStyle};

    use super::{format_source, format_source_with_options};

    #[test]
    fn indents_nested_blocks() {
        let source = concat!(
            "if(A)\n",
            "message(STATUS \"hi\")\n",
            "if(B)\n",
            "foo()\n",
            "endif()\n",
            "endif()\n",
        );
        let result = format_source(source);
        assert_eq!(
            result.output,
            concat!(
                "if(A)\n",
                "  message(STATUS \"hi\")\n",
                "  if(B)\n",
                "    foo()\n",
                "  endif()\n",
                "endif()\n",
            )
        );
        assert!(result.changed);
    }

    #[test]
    fn dedents_else_and_elseif_to_block_level() {
        let source = concat!(
            "if(A)\n",
            "foo()\n",
            "elseif(B)\n",
            "bar()\n",
            "else()\n",
            "baz()\n",
            "endif()\n",
        );
        let result = format_source(source);
        assert_eq!(
            result.output,
            concat!(
                "if(A)\n",
                "  foo()\n",
                "elseif(B)\n",
                "  bar()\n",
                "else()\n",
                "  baz()\n",
                "endif()\n",
            )
        );
    }

    #[test]
    fn reindents_overindented_blocks() {
        let source = concat!(
            "foreach(item IN LISTS items)\n",
            "        message(${item})\n",
            "endforeach()\n",
        );
        let result = format_source(source);
        assert_eq!(
            result.output,
            concat!(
                "foreach(item IN LISTS items)\n",
                "  message(${item})\n",
                "endforeach()\n",
            )
        );
    }

    #[test]
    fn shifts_continuation_lines_with_their_command() {
        let source = concat!(
            "if(A)\n",
            "target_link_libraries(mylib\n",
            "    PUBLIC foo\n",
            ")\n",
            "endif()\n",
        );
        let result = format_source(source);
        assert_eq!(
            result.output,
            concat!(
                "if(A)\n",
                "  target_link_libraries(mylib\n",
                "      PUBLIC foo\n",
                "  )\n",
                "endif()\n",
            )
        );
    }

    #[test]
    fn honors_configured_indent_width_and_style() {
        let source = "if(A)\nfoo()\nendif()\n";

        let four_spaces = format_source_with_options(
            source,
            &FormatConfiguration {
                indent_width: 4,
                ..FormatConfiguration::default()
            },
        );
        assert_eq!(four_spaces.output, "if(A)\n    foo()\nendif()\n");

        let tabs = format_source_with_options(
            source,
            &FormatConfiguration {
                indent_style: IndentStyle::Tab,
                ..FormatConfiguration::default()
            },
        );
        assert_eq!(tabs.output, "if(A)\n\tfoo()\nendif()\n");
    }

    #[test]
    fn indents_standalone_comments_to_surrounding_depth() {
        let source = concat!("if(A)\n", "# inside the block\n", "foo()\n", "endif()\n",);
        let result = format_source(source);
        assert_eq!(
            result.output,
            concat!(
                "if(A)\n",
                "  # inside the block\n",
                "  foo()\n",
                "endif()\n",
            )
        );
    }

    #[test]
    fn leaves_disabled_regions_unindented() {
        let source = concat!(
            "if(A)\n",
            "# cmake-format: off\n",
            "foo()\n",
            "# cmake-format: on\n",
            "bar()\n",
            "endif()\n",
        );
        let result = format_source(source);
        assert_eq!(
            result.output,
            concat!(
                "if(A)\n",
                "# cmake-format: off\n",
                "foo()\n",
                "# cmake-format: on\n",
                "  bar()\n",
                "endif()\n",
            )
        );
    }

    #[test]
    fn trims_trailing_spaces() {
        let result = format_source("project(example)   \nadd_subdirectory(src)\t\n");
        assert_eq!(result.output, "project(example)\nadd_subdirectory(src)\n");
        assert!(result.changed);
    }

    #[test]
    fn preserves_trailing_spaces_inside_multiline_bracket_arguments() {
        let source = "message([=[\nfirst line    \nsecond line\t\n]=])\n";
        let result = format_source(source);
        assert_eq!(result.output, source);
        assert!(!result.changed);
    }

    #[test]
    fn removes_space_before_paren() {
        let result = format_source("message (STATUS \"hi\")\n");
        assert_eq!(result.output, "message(STATUS \"hi\")\n");
        assert!(result.changed);
    }

    #[test]
    fn can_enforce_space_before_paren() {
        let result = format_source_with_options(
            "message(STATUS \"hi\")\n",
            &FormatConfiguration {
                space_before_paren: true,
                ..FormatConfiguration::default()
            },
        );
        assert_eq!(result.output, "message (STATUS \"hi\")\n");
        assert!(result.changed);
    }

    #[test]
    fn ensures_single_final_newline_and_trims_eof_blank_lines() {
        let result = format_source("project(example)\n\n\n");
        assert_eq!(result.output, "project(example)\n");
        assert!(result.changed);
    }

    #[test]
    fn respects_max_blank_lines_setting() {
        let result = format_source_with_options(
            "project(example)\n\n\n\nadd_subdirectory(src)\n",
            &FormatConfiguration {
                max_blank_lines: 2,
                ..FormatConfiguration::default()
            },
        );
        assert_eq!(
            result.output,
            "project(example)\n\n\nadd_subdirectory(src)\n"
        );
    }

    #[test]
    fn preserves_disabled_regions_verbatim() {
        let source = concat!(
            "project(example)\n",
            "# cmake-format: off\n",
            "message (STATUS \"hi\")   \n",
            "\n",
            "\n",
            "# cmake-format: on\n",
            "add_subdirectory(src)   \n",
        );
        let result = format_source(source);
        assert_eq!(
            result.output,
            concat!(
                "project(example)\n",
                "# cmake-format: off\n",
                "message (STATUS \"hi\")   \n",
                "\n",
                "\n",
                "# cmake-format: on\n",
                "add_subdirectory(src)\n",
            )
        );
        assert!(result.changed);
    }

    #[test]
    fn preserves_unterminated_disabled_region_to_eof() {
        let source = concat!(
            "project(example)\n",
            "# cmake-format: off\n",
            "message (STATUS \"hi\")   "
        );
        let result = format_source(source);
        assert_eq!(result.output, source);
        assert!(!result.changed);
    }
}
