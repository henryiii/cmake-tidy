pub struct Cursor<'a> {
    source: &'a str,
    offset: usize,
}

impl<'a> Cursor<'a> {
    pub(crate) const fn new(source: &'a str) -> Self {
        Self { source, offset: 0 }
    }

    pub(crate) const fn is_eof(&self) -> bool {
        self.offset >= self.source.len()
    }

    pub(crate) const fn offset(&self) -> usize {
        self.offset
    }

    pub(crate) fn peek_char(&self) -> Option<char> {
        self.source[self.offset..].chars().next()
    }

    pub(crate) fn starts_with(&self, pattern: &str) -> bool {
        self.source[self.offset..].starts_with(pattern)
    }

    pub(crate) fn slice(&self, start: usize, end: usize) -> &'a str {
        &self.source[start..end]
    }

    pub(crate) fn remaining(&self) -> &'a str {
        &self.source[self.offset..]
    }

    pub(crate) fn bump_char(&mut self) -> Option<char> {
        let character = self.peek_char()?;
        self.offset += character.len_utf8();
        Some(character)
    }

    pub(crate) const fn advance_bytes(&mut self, byte_count: usize) {
        self.offset += byte_count;
    }
}
