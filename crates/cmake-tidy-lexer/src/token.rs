use cmake_tidy_ast::TextRange;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub range: TextRange,
}

impl Token {
    #[must_use]
    pub const fn new(kind: TokenKind, range: TextRange) -> Self {
        Self { kind, range }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Identifier(String),
    UnquotedArgument(String),
    QuotedArgument(String),
    BracketArgument(String),
    LeftParen,
    RightParen,
    Comment(String),
    Whitespace(String),
    Newline,
}

impl TokenKind {
    #[must_use]
    pub const fn is_trivia(&self) -> bool {
        matches!(
            self,
            Self::Comment(_) | Self::Whitespace(_) | Self::Newline
        )
    }
}

