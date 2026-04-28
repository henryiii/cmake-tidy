#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TextRange {
    pub start: usize,
    pub end: usize,
}

impl TextRange {
    #[must_use]
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct File {
    pub items: Vec<Statement>,
    pub range: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Statement {
    Command(CommandInvocation),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandInvocation {
    pub name: Identifier,
    pub arguments: Vec<Argument>,
    pub range: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identifier {
    pub text: String,
    pub range: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Argument {
    Unquoted(UnquotedArgument),
    Quoted(QuotedArgument),
    Bracket(BracketArgument),
    ParenGroup(ParenGroup),
}

impl Argument {
    #[must_use]
    pub const fn range(&self) -> TextRange {
        match self {
            Self::Unquoted(argument) => argument.range,
            Self::Quoted(argument) => argument.range,
            Self::Bracket(argument) => argument.range,
            Self::ParenGroup(group) => group.range,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnquotedArgument {
    pub text: String,
    pub range: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotedArgument {
    pub text: String,
    pub range: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BracketArgument {
    pub text: String,
    pub range: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParenGroup {
    pub items: Vec<Argument>,
    pub range: TextRange,
}

