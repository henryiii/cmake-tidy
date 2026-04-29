mod cursor;
mod lexer;
mod token;

pub use lexer::tokenize;
pub use token::{Token, TokenKind};

#[cfg(test)]
mod tests {
    use super::{TokenKind, tokenize};

    #[test]
    fn preserves_comments_and_newlines() {
        let tokens = tokenize("project(example) # trailing\n");

        assert!(matches!(tokens[0].kind, TokenKind::Identifier(_)));
        assert!(matches!(tokens[1].kind, TokenKind::LeftParen));
        assert!(matches!(tokens[2].kind, TokenKind::Identifier(_)));
        assert!(matches!(tokens[3].kind, TokenKind::RightParen));
        assert!(matches!(tokens[4].kind, TokenKind::Whitespace(_)));
        assert!(matches!(tokens[5].kind, TokenKind::Comment(_)));
        assert!(matches!(tokens[6].kind, TokenKind::Newline));
    }

    #[test]
    fn tokenizes_bracket_arguments() {
        let tokens = tokenize("message([=[hello]=])");
        assert!(matches!(tokens[2].kind, TokenKind::BracketArgument(_)));
    }

    #[test]
    fn normalizes_crlf_to_newline_tokens() {
        let tokens = tokenize("project(example)\r\nmessage(STATUS hi)\r\n");
        assert_eq!(
            tokens
                .iter()
                .filter(|token| matches!(token.kind, TokenKind::Newline))
                .count(),
            2
        );
    }

    #[test]
    fn tokenizes_quoted_arguments_with_escapes() {
        let tokens = tokenize("message(\"a \\\"quoted\\\" value\")");
        let TokenKind::QuotedArgument(text) = &tokens[2].kind else {
            panic!("expected quoted argument");
        };
        assert_eq!(text, "\"a \\\"quoted\\\" value\"");
    }

    #[test]
    fn tokenizes_non_identifier_bare_words_as_unquoted_arguments() {
        let tokens = tokenize("set(VAR foo-bar)");
        assert!(matches!(tokens[4].kind, TokenKind::UnquotedArgument(_)));
    }
}
