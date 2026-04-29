use cmake_tidy_ast::TextRange;
use cmake_tidy_lexer::Token;
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct Parsed<T> {
    pub syntax: T,
    pub tokens: Vec<Token>,
    pub errors: Vec<ParseError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{message}")]
pub struct ParseError {
    pub message: String,
    pub range: TextRange,
}
