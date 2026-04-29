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

#[cfg(test)]
mod tests {
    use super::Cursor;

    #[test]
    fn cursor_advances_through_unicode_and_reports_remaining_text() {
        let mut cursor = Cursor::new("aé\n");

        assert_eq!(cursor.peek_char(), Some('a'));
        assert_eq!(cursor.bump_char(), Some('a'));
        assert_eq!(cursor.offset(), 1);
        assert_eq!(cursor.peek_char(), Some('é'));
        assert_eq!(cursor.remaining(), "é\n");
        assert_eq!(cursor.bump_char(), Some('é'));
        assert_eq!(cursor.offset(), "aé".len());
        assert_eq!(cursor.slice(0, cursor.offset()), "aé");
    }

    #[test]
    fn cursor_supports_prefix_checks_and_byte_advances() {
        let mut cursor = Cursor::new("\r\nrest");

        assert!(cursor.starts_with("\r\n"));
        cursor.advance_bytes(2);
        assert_eq!(cursor.remaining(), "rest");
        assert!(!cursor.is_eof());
        cursor.advance_bytes(4);
        assert!(cursor.is_eof());
        assert_eq!(cursor.peek_char(), None);
    }
}
