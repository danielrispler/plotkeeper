//! The header reader: the directives at the top of a Dart file, read as the SDK's `tokenizeDirectives` does (spec #621,
//! "The reader"). It reads in this order: a byte-order mark, a `#!` line, then, repeated, whitespace, comments,
//! metadata and directives, up to the first token that starts none of them. A file's later declarations are never
//! lexed.

/// What a directive does to the file it is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Import,
    Export,
    Part,
}

/// One `import`, `export` or `part` directive. `library` and `part of` give none.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Directive {
    pub(super) kind: Kind,
    /// The line of the directive's keyword.
    pub(super) line: u32,
    /// Each URI it names, the first and every `if (…)` one, left out when it holds an interpolation.
    pub(super) uris: Vec<String>,
}

/// A string literal: its text outside any interpolation, and whether it holds one.
struct Literal {
    value: String,
    interpolated: bool,
}

/// The URI of adjacent string literals.
enum Uri {
    Text(String),
    /// One of the literals holds an interpolation, a compile error in a directive: it names nothing.
    Interpolated,
}

impl Uri {
    /// The text of a URI that names a file.
    fn text(self) -> Option<String> {
        match self {
            Uri::Text(text) => Some(text),
            Uri::Interpolated => None,
        }
    }
}

/// The rest of an `import` or `export` after its first URI.
enum Configurable {
    /// It ends at its `;`, naming these URIs.
    Uris(Vec<String>),
    /// The text ends before its `;`.
    Unended,
}

enum Token<'a> {
    Ident(&'a str),
    Str(Literal),
    Punct(char),
    Eof,
}

struct Lexer<'a> {
    text: &'a str,
    pos: usize,
    /// Where the last token began.
    start: usize,
}

impl<'a> Lexer<'a> {
    fn byte(&self, at: usize) -> Option<u8> {
        self.text.as_bytes().get(at).copied()
    }

    /// Skips whitespace and comments; none when a block comment is never closed.
    fn skip_trivia(&mut self) -> Option<()> {
        loop {
            match (self.byte(self.pos), self.byte(self.pos + 1)) {
                (Some(b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c), _) => self.pos += 1,
                (Some(b'/'), Some(b'/')) => {
                    while self
                        .byte(self.pos)
                        .is_some_and(|b| b != b'\n' && b != b'\r')
                    {
                        self.pos += 1;
                    }
                }
                (Some(b'/'), Some(b'*')) => {
                    // Block comments nest.
                    let mut depth = 0;
                    loop {
                        match (self.byte(self.pos)?, self.byte(self.pos + 1)) {
                            (b'/', Some(b'*')) => {
                                depth += 1;
                                self.pos += 2;
                            }
                            (b'*', Some(b'/')) => {
                                depth -= 1;
                                self.pos += 2;
                                if depth == 0 {
                                    break;
                                }
                            }
                            _ => self.pos += 1,
                        }
                    }
                }
                _ => return Some(()),
            }
        }
    }

    /// The next token; none when a comment, string or interpolation in the way is never closed.
    fn next(&mut self) -> Option<Token<'a>> {
        self.skip_trivia()?;
        self.start = self.pos;
        let Some(b) = self.byte(self.pos) else {
            return Some(Token::Eof);
        };
        if matches!(b, b'\'' | b'"') {
            return self.string(false);
        }
        if b == b'r' && matches!(self.byte(self.pos + 1), Some(b'\'' | b'"')) {
            self.pos += 1;
            return self.string(true);
        }
        if b.is_ascii_alphabetic() || b == b'_' || b == b'$' {
            let len = self.text[self.pos..]
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '$'))
                .unwrap_or(self.text.len() - self.pos);
            let ident = &self.text[self.pos..self.pos + len];
            self.pos += len;
            return Some(Token::Ident(ident));
        }
        let c = self.text[self.pos..].chars().next()?;
        self.pos += c.len_utf8();
        Some(Token::Punct(c))
    }

    /// The token `next` would give, left unread.
    fn peek(&mut self) -> Option<Token<'a>> {
        let (pos, start) = (self.pos, self.start);
        let token = self.next();
        (self.pos, self.start) = (pos, start);
        token
    }

    /// The string literal whose opening quote is at the position, `raw` for `r'…'`; none when it is never closed.
    /// Its `${…}` interpolations are lexed as code, with their own strings, braces and comments.
    fn string(&mut self, raw: bool) -> Option<Token<'a>> {
        let quote = self.byte(self.pos)?;
        let triple =
            self.byte(self.pos + 1) == Some(quote) && self.byte(self.pos + 2) == Some(quote);
        self.pos += if triple { 3 } else { 1 };
        let mut literal = Literal {
            value: String::new(),
            interpolated: false,
        };
        loop {
            let b = self.byte(self.pos)?;
            if b == quote {
                if !triple {
                    self.pos += 1;
                    return Some(Token::Str(literal));
                }
                if self.byte(self.pos + 1) == Some(quote) && self.byte(self.pos + 2) == Some(quote)
                {
                    self.pos += 3;
                    return Some(Token::Str(literal));
                }
            } else if (b == b'\n' || b == b'\r') && !triple {
                return None;
            } else if b == b'\\' && !raw {
                self.pos += 1;
            } else if b == b'$' && !raw {
                let next = self.byte(self.pos + 1);
                if next == Some(b'{') {
                    literal.interpolated = true;
                    self.pos += 2;
                    self.skip_interpolation()?;
                    continue;
                }
                if next.is_some_and(|n| n.is_ascii_alphabetic() || n == b'_') {
                    literal.interpolated = true;
                }
            }
            let c = self.text[self.pos..].chars().next()?;
            literal.value.push(c);
            self.pos += c.len_utf8();
        }
    }

    /// Skips the code of a `${…}`, its opening brace read, to its closing brace.
    fn skip_interpolation(&mut self) -> Option<()> {
        let mut depth = 1;
        loop {
            match self.next()? {
                Token::Punct('{') => depth += 1,
                Token::Punct('}') => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(());
                    }
                }
                Token::Eof => return None,
                _ => {}
            }
        }
    }

    /// Skips to the closing bracket of the group whose opening one was just read; none when it is never closed or
    /// another kind of bracket closes it.
    fn skip_group(&mut self, open: char) -> Option<()> {
        let mut open = vec![open];
        while !open.is_empty() {
            match self.next()? {
                Token::Punct(c @ ('(' | '[' | '{')) => open.push(c),
                Token::Punct(c @ (')' | ']' | '}')) => {
                    let want = match c {
                        ')' => '(',
                        ']' => '[',
                        _ => '{',
                    };
                    if open.pop() != Some(want) {
                        return None;
                    }
                }
                Token::Eof => return None,
                _ => {}
            }
        }
        Some(())
    }

    /// Reads tokens up to and including the next `;`, or to the end of the text.
    fn skip_to_semicolon(&mut self) -> Option<()> {
        loop {
            if matches!(self.next()?, Token::Punct(';') | Token::Eof) {
                return Some(());
            }
        }
    }

    /// The metadata whose `@` was just read: a dotted name, type arguments with an optional `.constructor`, and arguments. Whether it was metadata.
    fn skip_metadata(&mut self) -> Option<bool> {
        if !matches!(self.next()?, Token::Ident(_)) {
            return Some(false);
        }
        while matches!(self.peek()?, Token::Punct('.')) {
            self.next()?;
            if !matches!(self.next()?, Token::Ident(_)) {
                return Some(false);
            }
        }
        if matches!(self.peek()?, Token::Punct('<')) {
            self.next()?;
            let mut depth = 1;
            while depth > 0 {
                match self.next()? {
                    Token::Punct('<') => depth += 1,
                    Token::Punct('>') => depth -= 1,
                    Token::Punct(';') | Token::Eof => return None,
                    _ => {}
                }
            }
            // A named constructor after the type arguments: `@C<T>.named(…)`.
            if matches!(self.peek()?, Token::Punct('.')) {
                self.next()?;
                if !matches!(self.next()?, Token::Ident(_)) {
                    return Some(false);
                }
            }
        }
        if matches!(self.peek()?, Token::Punct('(')) {
            self.next()?;
            self.skip_group('(')?;
        }
        Some(true)
    }

    /// The URI of adjacent string literals, `first` read: their texts joined.
    fn uri(&mut self, first: Literal) -> Option<Uri> {
        let Literal {
            mut value,
            mut interpolated,
        } = first;
        while let Token::Str(next) = self.peek()? {
            self.next()?;
            value.push_str(&next.value);
            interpolated |= next.interpolated;
        }
        Some(if interpolated {
            Uri::Interpolated
        } else {
            Uri::Text(value)
        })
    }

    /// The rest of an `import` or `export`, its keyword and first token read, up to its `;`: the URIs it names.
    fn configurable(&mut self, first: Literal) -> Option<Configurable> {
        let mut uris: Vec<String> = self.uri(first)?.text().into_iter().collect();
        loop {
            match self.next()? {
                Token::Punct(';') => return Some(Configurable::Uris(uris)),
                Token::Eof => return Some(Configurable::Unended),
                Token::Ident("if") => {
                    if matches!(self.next()?, Token::Punct('(')) {
                        self.skip_group('(')?;
                        if let Token::Str(literal) = self.peek()? {
                            self.next()?;
                            uris.extend(self.uri(literal)?.text());
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// The 1-based line of the last token's start. A line ends at `\n`, `\r\n` or a lone `\r`, as in Dart.
    fn line(&self) -> u32 {
        let bytes = &self.text.as_bytes()[..self.start];
        let breaks = bytes
            .iter()
            .enumerate()
            .filter(|&(at, &b)| b == b'\n' || (b == b'\r' && bytes.get(at + 1) != Some(&b'\n')))
            .count();
        1 + breaks as u32
    }
}

/// The directives of `text`'s header, or none when the header cannot be lexed: a comment, string or bracket in it is
/// never closed.
pub(super) fn read(text: &str) -> Option<Vec<Directive>> {
    let mut lexer = Lexer {
        text,
        pos: if text.starts_with('\u{feff}') { 3 } else { 0 },
        start: 0,
    };
    if text[lexer.pos..].starts_with("#!") {
        while lexer
            .byte(lexer.pos)
            .is_some_and(|b| b != b'\n' && b != b'\r')
        {
            lexer.pos += 1;
        }
    }
    let mut found = Vec::new();
    loop {
        let token = lexer.next()?;
        let line = lexer.line();
        let kind = match token {
            Token::Punct('@') => {
                if lexer.skip_metadata()? {
                    continue;
                }
                return Some(found);
            }
            Token::Ident("library") => {
                lexer.skip_to_semicolon()?;
                continue;
            }
            Token::Ident("import") => Kind::Import,
            Token::Ident("export") => Kind::Export,
            Token::Ident("part") => Kind::Part,
            _ => return Some(found),
        };
        let uris = match (kind, lexer.next()?) {
            (Kind::Part, Token::Ident("of")) => {
                lexer.skip_to_semicolon()?;
                continue;
            }
            (Kind::Part, Token::Str(literal)) => {
                let uri = lexer.uri(literal)?;
                lexer.skip_to_semicolon()?;
                uri.text().into_iter().collect()
            }
            // An unquoted import, not yet shipped: no pair.
            (Kind::Import | Kind::Export, Token::Ident(_)) => {
                lexer.skip_to_semicolon()?;
                continue;
            }
            (Kind::Import | Kind::Export, Token::Str(literal)) => {
                match lexer.configurable(literal)? {
                    Configurable::Uris(uris) => uris,
                    Configurable::Unended => return Some(found),
                }
            }
            _ => return Some(found),
        };
        found.push(Directive { kind, line, uris });
    }
}
