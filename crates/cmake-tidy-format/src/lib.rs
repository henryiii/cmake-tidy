use cmake_tidy_ast::TextRange;
use cmake_tidy_lexer::{TokenKind, tokenize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatResult {
    pub output: String,
    pub changed: bool,
}

#[must_use]
pub fn format_source(source: &str) -> FormatResult {
    let protected_ranges = tokenize(source)
        .into_iter()
        .filter_map(|token| match token.kind {
            TokenKind::BracketArgument(_) => Some(token.range),
            _ => None,
        })
        .collect::<Vec<_>>();

    let bytes = source.as_bytes();
    let mut output = String::with_capacity(source.len());
    let mut offset = 0;

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
        let trailing_range = TextRange::new(trim_end, line_end);
        let preserve_trailing = trim_end != line_end && overlaps_protected_range(trailing_range, &protected_ranges);

        if trim_end != line_end && !preserve_trailing {
            output.push_str(&source[offset..trim_end]);
        } else {
            output.push_str(&source[offset..line_end]);
        }
        output.push_str(&source[line_end..newline_end]);

        offset = newline_end;
    }

    let changed = output != source;
    FormatResult { output, changed }
}

fn trimmed_line_end(bytes: &[u8], line_start: usize, line_end: usize) -> usize {
    let mut trim_end = line_end;
    while trim_end > line_start && matches!(bytes[trim_end - 1], b' ' | b'\t') {
        trim_end -= 1;
    }
    trim_end
}

fn overlaps_protected_range(range: TextRange, protected_ranges: &[TextRange]) -> bool {
    protected_ranges.iter().any(|protected| {
        range.start < protected.end && protected.start < range.end
    })
}

#[cfg(test)]
mod tests {
    use super::format_source;

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
}
