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
}

