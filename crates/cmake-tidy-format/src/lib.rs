use cmake_tidy_ast::TextRange;
use cmake_tidy_config::FormatConfiguration;
use cmake_tidy_lexer::{Token, TokenKind, tokenize};

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
    let normalized_protected_ranges = protected_ranges(&normalized_parens);
    let output = normalize_lines(&normalized_parens, &normalized_protected_ranges, options);
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
    use cmake_tidy_config::FormatConfiguration;

    use super::{format_source, format_source_with_options};

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
