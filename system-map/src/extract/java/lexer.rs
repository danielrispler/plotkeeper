//! Java's tokens, as far as the reader needs them (spec #590, "The reader"): identifiers, punctuation and literals,
//! with whitespace and comments skipped. Lines are counted on the raw text, `\r\n`, `\n` and a lone `\r` each ending
//! one.

#[derive(Debug, PartialEq)]
pub(super) enum Tok {
    /// A word: a keyword, an identifier or a number.
    Ident(String),
    Punct(u8),
    /// A string literal, a char literal or a text block; its text is never read.
    Literal,
}

pub(super) struct Lexer<'a> {
    text: &'a [u8],
    at: usize,
    line: u32,
}

impl<'a> Lexer<'a> {
    pub(super) fn new(text: &'a str) -> Lexer<'a> {
        Lexer {
            text: text.as_bytes(),
            at: 0,
            line: 1,
        }
    }

    /// The length of the line terminator at `at`: 2 for `\r\n`, 1 for `\n` or a lone `\r`, 0 for none.
    fn terminator(&self, at: usize) -> usize {
        match (self.text.get(at), self.text.get(at + 1)) {
            (Some(b'\r'), Some(b'\n')) => 2,
            (Some(b'\r' | b'\n'), _) => 1,
            _ => 0,
        }
    }

    /// Moves to `end`, counting the lines of the text it passes.
    fn move_to(&mut self, end: usize) {
        let end = end.min(self.text.len());
        let mut at = self.at;
        while at < end {
            match self.terminator(at) {
                0 => at += 1,
                len => {
                    self.line += 1;
                    at += len;
                }
            }
        }
        self.at = end;
    }

    /// The end of a `//` comment starting at `from`: the next line terminator, or the end of the text.
    fn line_end(&self, from: usize) -> usize {
        (from..self.text.len())
            .find(|&i| self.terminator(i) > 0)
            .unwrap_or(self.text.len())
    }

    /// The end of a literal starting at `from` that `quote` closes: past the closing quote, or at a line terminator or
    /// the end of the text, where an unclosed one just ends. A backslash takes the byte after it.
    fn quoted_end(&self, from: usize, quote: u8) -> usize {
        let mut i = from + 1;
        while let Some(&byte) = self.text.get(i) {
            if byte == quote {
                return i + 1;
            }
            if self.terminator(i) > 0 {
                return i;
            }
            i += if byte == b'\\' { 2 } else { 1 };
        }
        self.text.len()
    }

    /// The end of a text block starting at `from`: past its closing `"""`, which `\"""` does not make, or the end of
    /// the text.
    fn text_block_end(&self, from: usize) -> usize {
        let mut i = from + 3;
        while i < self.text.len() {
            if self.text[i] == b'\\' {
                i += 2;
            } else if self.text[i..].starts_with(b"\"\"\"") {
                return i + 3;
            } else {
                i += 1;
            }
        }
        self.text.len()
    }
}

impl Iterator for Lexer<'_> {
    type Item = (Tok, u32);

    /// The next token with the line it starts on, or none at the end of the text.
    fn next(&mut self) -> Option<(Tok, u32)> {
        loop {
            let &byte = self.text.get(self.at)?;
            let rest = &self.text[self.at..];
            if byte.is_ascii_whitespace() {
                self.move_to(self.at + self.terminator(self.at).max(1));
            } else if rest.starts_with(b"//") {
                self.move_to(self.line_end(self.at));
            } else if rest.starts_with(b"/*") {
                let end = rest[2..]
                    .windows(2)
                    .position(|w| w == b"*/")
                    .map_or(self.text.len(), |len| self.at + len + 4);
                self.move_to(end);
            } else {
                break;
            }
        }
        let line = self.line;
        let start = self.at;
        let byte = self.text[start];
        let rest = &self.text[start..];
        let tok = if rest.starts_with(b"\"\"\"") {
            self.move_to(self.text_block_end(start));
            Tok::Literal
        } else if byte == b'"' || byte == b'\'' {
            self.move_to(self.quoted_end(start, byte));
            Tok::Literal
        } else if is_word(byte) {
            let len = rest.iter().position(|&b| !is_word(b)).unwrap_or(rest.len());
            self.at = start + len;
            Tok::Ident(String::from_utf8_lossy(&rest[..len]).into_owned())
        } else {
            self.at += 1;
            Tok::Punct(byte)
        };
        Some((tok, line))
    }
}

/// Whether a byte can be part of a word: an ASCII letter or digit, `_`, `$`, or any byte of a non-ASCII character.
fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$' || byte >= 0x80
}
