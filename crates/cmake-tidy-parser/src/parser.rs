use cmake_tidy_ast::{
    Argument, BracketArgument, CommandInvocation, File, Identifier, ParenGroup, QuotedArgument,
    Statement, TextRange, UnquotedArgument,
};
use cmake_tidy_lexer::{Token, TokenKind, tokenize};

use crate::parsed::{ParseError, Parsed};
use crate::token_source::TokenSource;

#[must_use]
pub fn parse_file(source: &str) -> Parsed<File> {
    let tokens = tokenize(source);
    let (syntax, errors) = {
        let mut parser = Parser::new(&tokens, source.len());
        let syntax = parser.parse_file();
        (syntax, parser.errors)
    };

    Parsed {
        syntax,
        tokens,
        errors,
    }
}

struct Parser<'a> {
    tokens: TokenSource<'a>,
    errors: Vec<ParseError>,
    source_len: usize,
}

impl<'a> Parser<'a> {
    fn new(tokens: &'a [Token], source_len: usize) -> Self {
        Self {
            tokens: TokenSource::new(tokens),
            errors: Vec::new(),
            source_len,
        }
    }

    fn parse_file(&mut self) -> File {
        let mut items = Vec::new();

        while self.tokens.current().is_some() {
            if let Some(statement) = self.parse_statement() {
                items.push(statement);
            }
        }

        File {
            items,
            range: TextRange::new(0, self.source_len),
        }
    }

    fn parse_statement(&mut self) -> Option<Statement> {
        let token = self.tokens.current()?;

        if let TokenKind::Identifier(_) = &token.kind {
            self.parse_command_invocation().map(Statement::Command)
        } else {
            self.error_here("expected a command name");
            self.tokens.bump();
            None
        }
    }

    fn parse_command_invocation(&mut self) -> Option<CommandInvocation> {
        let name_token = self.tokens.bump()?;
        let TokenKind::Identifier(text) = &name_token.kind else {
            self.error_at(name_token.range, "expected a command name");
            return None;
        };

        let name = Identifier {
            text: text.clone(),
            range: name_token.range,
        };

        let Some(next_token) = self.tokens.current() else {
            self.error_at(name.range, "expected `(` after command name");
            return Some(CommandInvocation {
                name,
                arguments: Vec::new(),
                range: name_token.range,
            });
        };

        if !matches!(next_token.kind, TokenKind::LeftParen) {
            self.error_at(next_token.range, "expected `(` after command name");
            return Some(CommandInvocation {
                name,
                arguments: Vec::new(),
                range: TextRange::new(name_token.range.start, name_token.range.end),
            });
        }

        let left_paren = self.tokens.bump().expect("left paren must exist");
        let (arguments, end) = self.parse_argument_list(left_paren.range.end);

        Some(CommandInvocation {
            name,
            arguments,
            range: TextRange::new(name_token.range.start, end),
        })
    }

    fn parse_argument_list(&mut self, fallback_end: usize) -> (Vec<Argument>, usize) {
        let mut arguments = Vec::new();
        let mut end = fallback_end;

        loop {
            let Some(token) = self.tokens.current() else {
                self.errors.push(ParseError {
                    message: "expected `)` to close command invocation".to_owned(),
                    range: TextRange::new(end, end),
                });
                break;
            };

            if matches!(token.kind, TokenKind::RightParen) {
                end = token.range.end;
                self.tokens.bump();
                break;
            }

            let Some(argument) = self.parse_argument() else {
                self.error_here("expected a command argument");
                self.tokens.bump();
                continue;
            };

            end = argument.range().end;
            arguments.push(argument);
        }

        (arguments, end)
    }

    fn parse_argument(&mut self) -> Option<Argument> {
        let token = self.tokens.current()?;

        match &token.kind {
            TokenKind::Identifier(text) | TokenKind::UnquotedArgument(text) => {
                let range = token.range;
                let text = text.clone();
                self.tokens.bump();
                Some(Argument::Unquoted(UnquotedArgument { text, range }))
            }
            TokenKind::QuotedArgument(text) => {
                let range = token.range;
                let text = text.clone();
                self.tokens.bump();
                Some(Argument::Quoted(QuotedArgument { text, range }))
            }
            TokenKind::BracketArgument(text) => {
                let range = token.range;
                let text = text.clone();
                self.tokens.bump();
                Some(Argument::Bracket(BracketArgument { text, range }))
            }
            TokenKind::LeftParen => self.parse_paren_group().map(Argument::ParenGroup),
            _ => None,
        }
    }

    fn parse_paren_group(&mut self) -> Option<ParenGroup> {
        let left_paren = self.tokens.bump()?;
        let start = left_paren.range.start;
        let (items, end) = self.parse_argument_list(left_paren.range.end);

        Some(ParenGroup {
            items,
            range: TextRange::new(start, end),
        })
    }

    fn error_here(&mut self, message: &str) {
        let range = self.tokens.current().map_or_else(
            || TextRange::new(self.source_len, self.source_len),
            |token| token.range,
        );
        self.error_at(range, message);
    }

    fn error_at(&mut self, range: TextRange, message: &str) {
        self.errors.push(ParseError {
            message: message.to_owned(),
            range,
        });
    }
}
