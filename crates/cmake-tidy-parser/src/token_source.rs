use cmake_tidy_lexer::Token;

pub(crate) struct TokenSource<'a> {
    tokens: &'a [Token],
    significant: Vec<usize>,
    position: usize,
}

impl<'a> TokenSource<'a> {
    pub(crate) fn new(tokens: &'a [Token]) -> Self {
        let significant = tokens
            .iter()
            .enumerate()
            .filter_map(|(index, token)| (!token.kind.is_trivia()).then_some(index))
            .collect();

        Self {
            tokens,
            significant,
            position: 0,
        }
    }

    pub(crate) fn current(&self) -> Option<&'a Token> {
        let token_index = *self.significant.get(self.position)?;
        self.tokens.get(token_index)
    }

    pub(crate) fn bump(&mut self) -> Option<&'a Token> {
        let token_index = *self.significant.get(self.position)?;
        self.position += 1;
        self.tokens.get(token_index)
    }
}
