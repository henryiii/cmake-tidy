use cmake_tidy_ast::TextRange;

use crate::cursor::Cursor;
use crate::token::{Token, TokenKind};

#[must_use]
pub fn tokenize(source: &str) -> Vec<Token> {
    let mut lexer = Lexer::new(source);
    lexer.tokenize()
}

struct Lexer<'a> {
    cursor: Cursor<'a>,
}

impl<'a> Lexer<'a> {
    const fn new(source: &'a str) -> Self {
        Self {
            cursor: Cursor::new(source),
        }
    }

    fn tokenize(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();

        while !self.cursor.is_eof() {
            let start = self.cursor.offset();
            let token = match self.cursor.peek_char() {
                Some('(') => {
                    self.cursor.bump_char();
                    Token::new(
                        TokenKind::LeftParen,
                        TextRange::new(start, self.cursor.offset()),
                    )
                }
                Some(')') => {
                    self.cursor.bump_char();
                    Token::new(
                        TokenKind::RightParen,
                        TextRange::new(start, self.cursor.offset()),
                    )
                }
                Some('\n') => {
                    self.cursor.bump_char();
                    Token::new(
                        TokenKind::Newline,
                        TextRange::new(start, self.cursor.offset()),
                    )
                }
                Some('\r') => {
                    if self.cursor.starts_with("\r\n") {
                        self.cursor.advance_bytes(2);
                    } else {
                        self.cursor.bump_char();
                    }
                    Token::new(
                        TokenKind::Newline,
                        TextRange::new(start, self.cursor.offset()),
                    )
                }
                Some(' ' | '\t' | '\u{0C}') => self.lex_whitespace(start),
                Some('#') => self.lex_comment(start),
                Some('"') => self.lex_quoted_argument(start),
                Some('[') => self.lex_bracket_or_unquoted(start),
                Some(_) => self.lex_bare(start),
                None => break,
            };

            tokens.push(token);
        }

        tokens
    }

    fn lex_whitespace(&mut self, start: usize) -> Token {
        while let Some(character) = self.cursor.peek_char() {
            if matches!(character, ' ' | '\t' | '\u{0C}') {
                self.cursor.bump_char();
            } else {
                break;
            }
        }

        let text = self.cursor.slice(start, self.cursor.offset()).to_owned();
        Token::new(
            TokenKind::Whitespace(text),
            TextRange::new(start, self.cursor.offset()),
        )
    }

    fn lex_comment(&mut self, start: usize) -> Token {
        while let Some(character) = self.cursor.peek_char() {
            if matches!(character, '\n' | '\r') {
                break;
            }
            self.cursor.bump_char();
        }

        let text = self.cursor.slice(start, self.cursor.offset()).to_owned();
        Token::new(
            TokenKind::Comment(text),
            TextRange::new(start, self.cursor.offset()),
        )
    }

    fn lex_quoted_argument(&mut self, start: usize) -> Token {
        self.cursor.bump_char();

        while let Some(character) = self.cursor.peek_char() {
            self.cursor.bump_char();

            if character == '\\' {
                let _ = self.cursor.bump_char();
                continue;
            }

            if character == '"' {
                break;
            }
        }

        let text = self.cursor.slice(start, self.cursor.offset()).to_owned();
        Token::new(
            TokenKind::QuotedArgument(text),
            TextRange::new(start, self.cursor.offset()),
        )
    }

    fn lex_bracket_or_unquoted(&mut self, start: usize) -> Token {
        if let Some(open_len) = bracket_open_len(self.cursor.remaining()) {
            let eq_count = open_len.saturating_sub(2);
            let closing = format!("]{}]", "=".repeat(eq_count));

            self.cursor.advance_bytes(open_len);

            if let Some(relative_end) = self.cursor.remaining().find(&closing) {
                self.cursor.advance_bytes(relative_end + closing.len());
            } else {
                self.cursor.advance_bytes(self.cursor.remaining().len());
            }

            let text = self.cursor.slice(start, self.cursor.offset()).to_owned();
            return Token::new(
                TokenKind::BracketArgument(text),
                TextRange::new(start, self.cursor.offset()),
            );
        }

        self.lex_bare(start)
    }

    fn lex_bare(&mut self, start: usize) -> Token {
        while let Some(character) = self.cursor.peek_char() {
            if is_bare_terminator(character) {
                break;
            }
            self.cursor.bump_char();
        }

        let text = self.cursor.slice(start, self.cursor.offset()).to_owned();
        let kind = if is_identifier(&text) {
            TokenKind::Identifier(text)
        } else {
            TokenKind::UnquotedArgument(text)
        };

        Token::new(kind, TextRange::new(start, self.cursor.offset()))
    }
}

fn bracket_open_len(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    if bytes.first() != Some(&b'[') {
        return None;
    }

    let mut offset = 1;
    while bytes.get(offset) == Some(&b'=') {
        offset += 1;
    }

    if bytes.get(offset) == Some(&b'[') {
        Some(offset + 1)
    } else {
        None
    }
}

const fn is_bare_terminator(character: char) -> bool {
    matches!(
        character,
        '(' | ')' | '#' | ' ' | '\t' | '\n' | '\r' | '\u{0C}'
    )
}

fn is_identifier(text: &str) -> bool {
    let mut characters = text.chars();
    let Some(first) = characters.next() else {
        return false;
    };

    if !(first == '_' || first.is_ascii_alphabetic()) {
        return false;
    }

    characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
}
