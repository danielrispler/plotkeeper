//! The reader: the imports of a Python file, read as text with a lexer of its own (spec #529, "Lexing is per language";
//! spec #589, "The reader"). It tokenises the file the way Python's tokenizer splits it into logical lines (comments,
//! backslash joins, bracket depth, every string form with its prefixes, the nested fields of f- and t-strings), then
//! keys on each `import` keyword and looks back for `from` and `lazy` only as far as the statement's start.

/// What an import statement brings in from its module.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Imported {
    /// `import a.b`: the module itself.
    Module,
    /// `from a.b import c, d`: these names, each an attribute or a submodule.
    Names(Vec<String>),
    /// `from a.b import *`.
    Star,
}

/// One imported module of an import statement.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Import {
    /// The line the statement starts on.
    pub(super) line: u32,
    /// The leading dots of a relative import, `...` counting three; none for an absolute import.
    pub(super) dots: u32,
    /// The module's dotted name, split; empty for `from . import x`.
    pub(super) module: Vec<String>,
    pub(super) imported: Imported,
}

/// The imports of a Python file, in the order they are written. Text that is not valid Python ends the statement it
/// is in (and an unterminated triple-quoted string ends the file), with no finding. `from __future__ import …` is a
/// compiler directive and gives nothing.
pub(super) fn imports(text: &str) -> Vec<Import> {
    let toks = lex(text);
    let mut found = Vec::new();
    // The index of the current statement's first token, and the bracket depth.
    let (mut start, mut depth, mut i) = (0, 0_u32, 0);
    while i < toks.len() {
        match &toks[i].0 {
            Tok::Open => depth += 1,
            Tok::Close => depth = depth.saturating_sub(1),
            Tok::Newline | Tok::Semi | Tok::Colon if depth == 0 => start = i + 1,
            Tok::Bad => {
                depth = 0;
                start = i + 1;
            }
            Tok::Name(word) if word == "import" && depth == 0 => {
                if let Some((imports, end)) = statement(&toks, start, i) {
                    found.extend(imports);
                    i = end;
                    continue;
                }
            }
            _ => {}
        }
        i += 1;
    }
    found
}

#[derive(Debug, PartialEq)]
enum Tok {
    Name(String),
    Dot,
    /// `...`, which counts as three dots in a relative import.
    Ellipsis,
    Comma,
    Star,
    Open,
    Close,
    Semi,
    Colon,
    /// The end of a logical line: a newline outside brackets and not escaped.
    Newline,
    /// A string, a number or an operator: nothing an import statement holds.
    Other,
    /// Text Python's tokenizer rejects: an unterminated string, a stray backslash. It ends the statement.
    Bad,
}

type Token = (Tok, u32);

/// The tokens of `text`, each with the line it starts on.
fn lex(text: &str) -> Vec<Token> {
    let bytes = text.as_bytes();
    let mut lexer = Lexer {
        text: bytes,
        at: if bytes.starts_with(b"\xEF\xBB\xBF") {
            3
        } else {
            0
        },
        line: 1,
        depth: 0,
        toks: Vec::new(),
    };
    while lexer.at < bytes.len() {
        lexer.next();
    }
    lexer.toks
}

struct Lexer<'a> {
    text: &'a [u8],
    at: usize,
    line: u32,
    depth: u32,
    toks: Vec<Token>,
}

impl Lexer<'_> {
    fn push(&mut self, tok: Tok) {
        self.toks.push((tok, self.line));
    }

    /// Moves to `end`, counting the lines it passes: each `\n`, and each `\r` not followed by one.
    fn advance_to(&mut self, end: usize) {
        let end = end.min(self.text.len());
        self.line += (self.at..end)
            .filter(|&i| match self.text[i] {
                b'\n' => true,
                b'\r' => self.text.get(i + 1) != Some(&b'\n'),
                _ => false,
            })
            .count() as u32;
        self.at = end;
    }

    /// Reads one token or one piece of skipped text. A line ends at `\n`, `\r\n` or a lone `\r`, as in Python.
    fn next(&mut self) {
        let rest = &self.text[self.at..];
        let byte = rest[0];
        match byte {
            b'\r' if rest.get(1) == Some(&b'\n') => self.at += 1,
            b'\n' | b'\r' => {
                if self.depth == 0 {
                    self.push(Tok::Newline);
                }
                self.advance_to(self.at + 1);
            }
            b'#' => {
                self.at += rest
                    .iter()
                    .copied()
                    .position(is_newline)
                    .unwrap_or(rest.len());
            }
            b'\\' => self.backslash(),
            b'\'' | b'"' => self.string(self.at, false, false),
            b'0'..=b'9' => self.number(),
            b'.' if rest.starts_with(b"...") => self.single(Tok::Ellipsis, 3),
            b'.' if rest.get(1).is_some_and(u8::is_ascii_digit) => self.number(),
            b'.' => self.single(Tok::Dot, 1),
            b'(' | b'[' | b'{' => {
                self.depth += 1;
                self.single(Tok::Open, 1);
            }
            b')' | b']' | b'}' => {
                self.depth = self.depth.saturating_sub(1);
                self.single(Tok::Close, 1);
            }
            b',' => self.single(Tok::Comma, 1),
            b';' => self.single(Tok::Semi, 1),
            b':' => self.single(Tok::Colon, 1),
            b'*' => self.single(Tok::Star, 1),
            _ if is_ident(byte) => self.word(),
            b' ' | b'\t' | 0x0c => self.at += 1,
            _ => self.single(Tok::Other, 1),
        }
    }

    fn single(&mut self, tok: Tok, len: usize) {
        self.push(tok);
        self.at += len;
    }

    /// A backslash joins the next line to this one; any other use of it is not Python.
    fn backslash(&mut self) {
        let rest = &self.text[self.at + 1..];
        if rest.starts_with(b"\n") {
            self.advance_to(self.at + 2);
        } else if rest.starts_with(b"\r\n") {
            self.advance_to(self.at + 3);
        } else if rest.starts_with(b"\r") {
            self.advance_to(self.at + 2);
        } else {
            self.bad(self.at + 1);
        }
    }

    /// Ends the statement: the text up to `end` is not Python.
    fn bad(&mut self, end: usize) {
        self.push(Tok::Bad);
        self.depth = 0;
        self.advance_to(end);
    }

    /// A number, with its dots, exponent signs, underscores and suffix letters.
    fn number(&mut self) {
        self.push(Tok::Other);
        let mut i = self.at + 1;
        while let Some(&b) = self.text.get(i) {
            let exponent = matches!(b, b'+' | b'-') && matches!(self.text[i - 1], b'e' | b'E');
            if b.is_ascii_alphanumeric() || b == b'_' || b == b'.' || exponent {
                i += 1;
            } else {
                break;
            }
        }
        self.at = i;
    }

    /// A name, or the prefix of a string.
    fn word(&mut self) {
        let end = ident_end(self.text, self.at);
        if let Some(&quote) = self.text.get(end)
            && matches!(quote, b'\'' | b'"')
            && let Some((raw, interpolated)) = prefix(&self.text[self.at..end])
        {
            self.string(end, raw, interpolated);
            return;
        }
        let word = String::from_utf8_lossy(&self.text[self.at..end]).into_owned();
        self.push(Tok::Name(word));
        self.at = end;
    }

    /// A string whose opening quote is at `quote_at`, the token starting at the cursor.
    fn string(&mut self, quote_at: usize, raw: bool, interpolated: bool) {
        match skip_string(self.text, quote_at, raw, interpolated) {
            Ok(end) => {
                self.push(Tok::Other);
                self.advance_to(end);
            }
            Err(stop) => self.bad(stop),
        }
    }
}

/// Whether a byte ends a line: `\n`, or `\r` alone or before `\n`.
fn is_newline(byte: u8) -> bool {
    matches!(byte, b'\n' | b'\r')
}

fn is_ident(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphabetic() || byte >= 0x80
}

/// The end of the identifier starting at `at`.
fn ident_end(text: &[u8], at: usize) -> usize {
    let len = text[at..]
        .iter()
        .position(|&b| !(is_ident(b) || b.is_ascii_digit()))
        .unwrap_or(text.len() - at);
    at + len
}

/// What a string prefix says, in any case: whether it is raw, and whether it holds fields (`f`, `t`). None for a word
/// that is no prefix.
fn prefix(word: &[u8]) -> Option<(bool, bool)> {
    match word.to_ascii_lowercase().as_slice() {
        b"r" | b"br" | b"rb" => Some((true, false)),
        b"u" | b"b" => Some((false, false)),
        b"f" | b"t" => Some((false, true)),
        b"fr" | b"rf" | b"tr" | b"rt" => Some((true, true)),
        _ => None,
    }
}

/// The end of the string literal whose opening quote is at `quote_at`, or where it stops being Python: the end of the
/// line for a one-line string, the end of the text for a triple-quoted one.
fn skip_string(
    text: &[u8],
    quote_at: usize,
    raw: bool,
    interpolated: bool,
) -> Result<usize, usize> {
    let quote = text[quote_at];
    let triple = text[quote_at + 1..].starts_with(&[quote, quote]);
    let mut i = quote_at + if triple { 3 } else { 1 };
    loop {
        let Some(&b) = text.get(i) else {
            return Err(text.len());
        };
        match b {
            b'\\' => i = escape(text, i, raw, interpolated)?,
            b'\n' | b'\r' if !triple => return Err(i),
            _ if b == quote && !triple => return Ok(i + 1),
            _ if b == quote && text[i..].starts_with(&[quote; 3]) => return Ok(i + 3),
            b'{' if interpolated && text.get(i + 1) == Some(&b'{') => i += 2,
            b'{' if interpolated => i = field(text, i + 1)?,
            _ => i += 1,
        }
    }
}

/// The index after the escape at `i`, a backslash. In a string with fields a backslash leaves a brace to the field
/// rule, and `\N{name}` is one escape (not in a raw string).
fn escape(text: &[u8], i: usize, raw: bool, interpolated: bool) -> Result<usize, usize> {
    match (text.get(i + 1), text.get(i + 2)) {
        (Some(b'{' | b'}'), _) if interpolated => Ok(i + 1),
        (Some(b'N'), Some(b'{')) if interpolated && !raw => text[i + 3..]
            .iter()
            .position(|&b| b == b'}')
            .map(|at| i + 3 + at + 1)
            .ok_or(text.len()),
        (Some(b'\r'), Some(b'\n')) => Ok(i + 3),
        (Some(_), _) => Ok(i + 2),
        (None, _) => Err(text.len()),
    }
}

/// The end of a replacement field whose first byte after its `{` is at `i`: code, in which a string is skipped
/// whole, a `#` comment ends at its line, and brackets nest, up to the `}` at depth zero or to a `:` that starts the
/// format spec.
fn field(text: &[u8], mut i: usize) -> Result<usize, usize> {
    let mut depth = 0_u32;
    loop {
        let Some(&b) = text.get(i) else {
            return Err(text.len());
        };
        match b {
            b'\'' | b'"' => i = skip_string(text, i, false, false)?,
            b'#' => i += text[i..].iter().take_while(|&&c| !is_newline(c)).count(),
            b'(' | b'[' | b'{' => {
                depth += 1;
                i += 1;
            }
            b')' | b']' => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            b'}' if depth == 0 => return Ok(i + 1),
            b'}' => {
                depth -= 1;
                i += 1;
            }
            b':' if depth == 0 => return spec(text, i + 1),
            _ if is_ident(b) => i = word_in_field(text, i)?,
            _ => i += 1,
        }
    }
}

/// A name in a field at `i`, or a prefixed string: the index after it.
fn word_in_field(text: &[u8], i: usize) -> Result<usize, usize> {
    let end = ident_end(text, i);
    match (text.get(end), prefix(&text[i..end])) {
        (Some(b'\'' | b'"'), Some((raw, interpolated))) => {
            skip_string(text, end, raw, interpolated)
        }
        _ => Ok(end),
    }
}

/// The end of a format spec starting at `i`: text with nested fields, up to the `}` that closes the field.
fn spec(text: &[u8], mut i: usize) -> Result<usize, usize> {
    loop {
        match text.get(i) {
            None => return Err(text.len()),
            Some(b'}') => return Ok(i + 1),
            Some(b'{') => i = field(text, i + 1)?,
            Some(_) => i += 1,
        }
    }
}

fn is_name(tok: Option<&Token>, word: &str) -> bool {
    matches!(tok, Some((Tok::Name(w), _)) if w == word)
}

/// The import statement whose `import` keyword is at `at`, the statement starting at `start`: its imports and the
/// index of the token that ends it. None when the text before `import` is no `from …` or `lazy`, or the statement is
/// malformed.
fn statement(toks: &[Token], start: usize, at: usize) -> Option<(Vec<Import>, usize)> {
    let mut head = &toks[start..at];
    if is_name(head.first(), "lazy") {
        head = &head[1..];
    }
    let line = toks[start].1;
    match head {
        [] => plain(toks, at + 1, line),
        [(Tok::Name(from), _), module @ ..] if from == "from" => {
            let (dots, module) = relative(module)?;
            let (imported, end) = names(toks, at + 1)?;
            let future = dots == 0 && module == ["__future__"];
            let import = Import {
                line,
                dots,
                module,
                imported,
            };
            Some((if future { Vec::new() } else { vec![import] }, end))
        }
        _ => None,
    }
}

/// The index after a statement's last token when the token at `at` ends it: a newline, a `;` or the end of the file.
fn ends(toks: &[Token], at: usize) -> Option<usize> {
    matches!(toks.get(at), None | Some((Tok::Newline | Tok::Semi, _))).then_some(at)
}

/// A dotted name `a.b.c` at `at`: its segments and the index after it.
fn dotted(toks: &[Token], mut at: usize) -> Option<(Vec<String>, usize)> {
    let mut segments = Vec::new();
    loop {
        let Some((Tok::Name(word), _)) = toks.get(at) else {
            return None;
        };
        segments.push(word.clone());
        at += 1;
        if !matches!(toks.get(at), Some((Tok::Dot, _))) {
            return Some((segments, at));
        }
        at += 1;
    }
}

/// `import a.b as c, d` after its keyword, at `at`: one import per module.
fn plain(toks: &[Token], mut at: usize, line: u32) -> Option<(Vec<Import>, usize)> {
    let mut found = Vec::new();
    loop {
        let (module, next) = dotted(toks, at)?;
        found.push(Import {
            line,
            dots: 0,
            module,
            imported: Imported::Module,
        });
        at = alias(toks, next)?;
        if !matches!(toks.get(at), Some((Tok::Comma, _))) {
            return Some((found, ends(toks, at)?));
        }
        at += 1;
    }
}

/// The index after an optional `as name` at `at`.
fn alias(toks: &[Token], at: usize) -> Option<usize> {
    if !is_name(toks.get(at), "as") {
        return Some(at);
    }
    matches!(toks.get(at + 1), Some((Tok::Name(_), _))).then_some(at + 2)
}

/// A `from` statement's module, between `from` and `import`: its dots and its dotted name, either one empty but not
/// both.
fn relative(toks: &[Token]) -> Option<(u32, Vec<String>)> {
    let dots: u32 = toks
        .iter()
        .map_while(|(tok, _)| match tok {
            Tok::Dot => Some(1),
            Tok::Ellipsis => Some(3),
            _ => None,
        })
        .sum();
    let after = toks
        .iter()
        .take_while(|(tok, _)| matches!(tok, Tok::Dot | Tok::Ellipsis))
        .count();
    if after == toks.len() {
        return (dots > 0).then_some((dots, Vec::new()));
    }
    let (module, end) = dotted(toks, after)?;
    (end == toks.len()).then_some((dots, module))
}

/// The names a `from` statement imports, after its `import` keyword, at `at`, and the index of the token that ends the
/// statement: `*`, a parenthesised list with an optional trailing comma, or a bare list.
fn names(toks: &[Token], at: usize) -> Option<(Imported, usize)> {
    match toks.get(at) {
        Some((Tok::Star, _)) => Some((Imported::Star, ends(toks, at + 1)?)),
        Some((Tok::Open, _)) => {
            let (names, next) = name_list(toks, at + 1, true)?;
            matches!(toks.get(next), Some((Tok::Close, _))).then_some(())?;
            Some((Imported::Names(names), ends(toks, next + 1)?))
        }
        _ => {
            let (names, next) = name_list(toks, at, false)?;
            Some((Imported::Names(names), ends(toks, next)?))
        }
    }
}

/// `a as b, c` from `at`: the imported names and the index after the list. A trailing comma is allowed in
/// parentheses only.
fn name_list(toks: &[Token], mut at: usize, parenthesised: bool) -> Option<(Vec<String>, usize)> {
    let mut names = Vec::new();
    loop {
        let Some((Tok::Name(name), _)) = toks.get(at) else {
            return None;
        };
        names.push(name.clone());
        at = alias(toks, at + 1)?;
        if !matches!(toks.get(at), Some((Tok::Comma, _))) {
            return Some((names, at));
        }
        at += 1;
        if parenthesised && matches!(toks.get(at), Some((Tok::Close, _))) {
            return Some((names, at));
        }
    }
}
