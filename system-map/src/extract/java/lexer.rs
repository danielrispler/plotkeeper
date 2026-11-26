//! Java's tokens, as far as the reader needs them (spec #590, "The reader"): identifiers, punctuation and literals,
//! with whitespace and comments skipped. A pass before the lexer translates `\uXXXX` escapes as JLS §3.3 has it. Lines
//! are counted on the raw text, `\r\n`, `\n` and a lone `\r` each ending one, so a translated line break counts for
//! none.

#[derive(Debug, PartialEq)]
pub(super) enum Tok {
    /// A word: a keyword, an identifier or a number.
    Ident(String),
    Punct(u8),
    /// A string literal, a char literal or a text block; its text is never read.
    Literal,
}

/// The tokens of `text` with the raw line each starts on, or none when the file cannot be lexed: a malformed escape, or
/// a comment, string, char literal or text block never closed (JLS §3.3, §3.7, §3.10).
pub(super) fn tokens(text: &str) -> Option<Vec<(Tok, u32)>> {
    let mut lexer = Lexer::new(text)?;
    let tokens: Vec<(Tok, u32)> = lexer.by_ref().collect();
    (!lexer.unclosed).then_some(tokens)
}

/// The text with its `\uXXXX` escapes translated (JLS §3.3), and the offsets, in the translated text, that follow each
/// raw line terminator. None for a malformed escape.
///
/// A backslash is eligible when an even number of backslashes precede it; one or more `u`s may follow, and the four hex
/// digits after the last `u` give the character. A translated character takes part in no further escape. One that is no
/// character (half a surrogate pair) becomes U+FFFD, which no token reads.
fn translate(text: &str) -> Option<(Vec<u8>, Vec<usize>)> {
    let raw = text.as_bytes();
    let mut out = Vec::with_capacity(raw.len());
    let mut breaks = Vec::new();
    let mut backslashes = 0usize;
    let mut i = 0;
    while i < raw.len() {
        let byte = raw[i];
        if byte == b'\\' && backslashes.is_multiple_of(2) && raw.get(i + 1) == Some(&b'u') {
            let mut digits = i + 1;
            while raw.get(digits) == Some(&b'u') {
                digits += 1;
            }
            let hex = raw.get(digits..digits + 4)?;
            let mut code = 0;
            for &digit in hex {
                code = code * 16 + char::from(digit).to_digit(16)?;
            }
            let ch = char::from_u32(code).unwrap_or(char::REPLACEMENT_CHARACTER);
            out.extend_from_slice(ch.encode_utf8(&mut [0; 4]).as_bytes());
            backslashes = 0;
            i = digits + 4;
            continue;
        }
        backslashes = if byte == b'\\' { backslashes + 1 } else { 0 };
        out.push(byte);
        i += 1;
        if byte == b'\r' && raw.get(i) == Some(&b'\n') {
            out.push(b'\n');
            i += 1;
        }
        if byte == b'\r' || byte == b'\n' {
            breaks.push(out.len());
        }
    }
    Some((out, breaks))
}

struct Lexer {
    /// The text after the escape pass.
    text: Vec<u8>,
    /// The offsets in `text` just past each raw line terminator, ascending.
    breaks: Vec<usize>,
    at: usize,
    /// Whether a comment, string, char literal or text block ran to a line break or the end of the text unclosed.
    unclosed: bool,
}

impl Lexer {
    /// A lexer over `text`, or none when an escape in it is malformed.
    fn new(text: &str) -> Option<Lexer> {
        let (text, breaks) = translate(text)?;
        Some(Lexer {
            text,
            breaks,
            at: 0,
            unclosed: false,
        })
    }

    /// The raw line of the translated offset `at`.
    fn line(&self, at: usize) -> u32 {
        1 + self.breaks.partition_point(|&end| end <= at) as u32
    }

    /// The length of the line terminator at `at` in the translated text: 2 for `\r\n`, 1 for `\n` or a lone `\r`, 0 for
    /// none.
    fn terminator(&self, at: usize) -> usize {
        match (self.text.get(at), self.text.get(at + 1)) {
            (Some(b'\r'), Some(b'\n')) => 2,
            (Some(b'\r' | b'\n'), _) => 1,
            _ => 0,
        }
    }

    /// The end of a `//` comment starting at `from`: the next line terminator, or the end of the text.
    fn line_end(&self, from: usize) -> usize {
        (from..self.text.len())
            .find(|&i| self.terminator(i) > 0)
            .unwrap_or(self.text.len())
    }

    /// The end of a literal starting at `from` that `quote` closes: past the closing quote, or none when a line
    /// terminator or the end of the text comes first. A backslash takes the byte after it.
    fn quoted_end(&self, from: usize, quote: u8) -> Option<usize> {
        let mut i = from + 1;
        while let Some(&byte) = self.text.get(i) {
            if byte == quote {
                return Some(i + 1);
            }
            if self.terminator(i) > 0 || (byte == b'\\' && self.terminator(i + 1) > 0) {
                return None;
            }
            i += if byte == b'\\' { 2 } else { 1 };
        }
        None
    }

    /// The end of a text block starting at `from`: past its closing `"""`, which `\"""` does not make, or none at the
    /// end of the text.
    fn text_block_end(&self, from: usize) -> Option<usize> {
        let mut i = from + 3;
        while i < self.text.len() {
            if self.text[i] == b'\\' {
                i += 2;
            } else if self.text[i..].starts_with(b"\"\"\"") {
                return Some(i + 3);
            } else {
                i += 1;
            }
        }
        None
    }

    /// Ends the text at an unclosed construct.
    fn give_up(&mut self) -> Option<(Tok, u32)> {
        self.unclosed = true;
        self.at = self.text.len();
        None
    }
}

impl Iterator for Lexer {
    type Item = (Tok, u32);

    /// The next token with the line it starts on, or none at the end of the text or at an unclosed construct.
    fn next(&mut self) -> Option<(Tok, u32)> {
        loop {
            let &byte = self.text.get(self.at)?;
            let rest = &self.text[self.at..];
            if byte.is_ascii_whitespace() {
                self.at += self.terminator(self.at).max(1);
            } else if rest.starts_with(b"//") {
                self.at = self.line_end(self.at);
            } else if rest.starts_with(b"/*") {
                match rest[2..].windows(2).position(|w| w == b"*/") {
                    Some(len) => self.at += len + 4,
                    None => return self.give_up(),
                }
            } else {
                break;
            }
        }
        let line = self.line(self.at);
        let start = self.at;
        let byte = self.text[start];
        let rest = &self.text[start..];
        let tok = if rest.starts_with(b"\"\"\"") {
            match self.text_block_end(start) {
                Some(end) => self.at = end,
                None => return self.give_up(),
            }
            Tok::Literal
        } else if byte == b'"' || byte == b'\'' {
            match self.quoted_end(start, byte) {
                Some(end) => self.at = end,
                None => return self.give_up(),
            }
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
