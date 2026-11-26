//! The PHP reader: a lexer that finds the code in a file, then the statements `php-imports` takes from it, the class
//! `use` items and the static `require` and `include` paths (spec #647, "The reader" and "Requires"). It is a text
//! reader and runs no `php` (ADR 0002).

/// A token of PHP code.
#[derive(Debug, PartialEq)]
enum Tok {
    /// A name or keyword: `Foo`, `Foo\Bar`, `\Foo`, `namespace\Foo`, one token as PHP 8 lexes it.
    Name(String),
    /// A string literal that holds no interpolation, its value.
    Str(String),
    /// A decimal integer literal.
    Int(u64),
    /// `->` or `?->`.
    Arrow,
    /// `::`.
    Scope,
    /// One byte of punctuation. A `?>` close tag is a `;`, as PHP counts it.
    Punct(u8),
    /// Anything else: a variable, an interpolated string, a number, a heredoc.
    Other,
}

/// Tokens, each with the line it starts on.
type Tokens = [(Tok, u32)];

/// How a run of code ended.
enum End {
    /// `?>`: PHP mode is left.
    CloseTag,
    /// The end of the file.
    Eof,
    /// `__halt_compiler`: the rest of the file is data.
    Halt,
    /// The `}` that closes the code a string nests.
    CloseBrace,
}

struct Lexer<'t> {
    bytes: &'t [u8],
    at: usize,
    /// The offsets of the `\n` bytes, to turn an offset into a line.
    newlines: Vec<usize>,
    /// How many strings' nested code the lexer is inside.
    nesting: usize,
}

/// The deepest nesting of string code the reader lexes: a file nested deeper is one it cannot lex, so a crafted file
/// cannot exhaust the stack.
const MAX_NESTING: usize = 256;

fn label_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b >= 0x80
}

fn label_char(b: u8) -> bool {
    label_start(b) || b.is_ascii_digit()
}

/// The tokens of the PHP code in `text`, or none when the reader cannot lex it: a string, heredoc or `/* */` comment
/// that is never closed, or string code nested deeper than [`MAX_NESTING`].
fn tokens(text: &str) -> Option<Vec<(Tok, u32)>> {
    let bytes = text.as_bytes();
    let mut lexer = Lexer {
        bytes,
        at: 0,
        newlines: bytes
            .iter()
            .enumerate()
            .filter(|(_, b)| **b == b'\n')
            .map(|(at, _)| at)
            .collect(),
        nesting: 0,
    };
    let mut out = Vec::new();
    while lexer.open_tag() {
        match lexer.code(false, &mut out)? {
            End::CloseTag => {}
            End::Eof | End::Halt | End::CloseBrace => break,
        }
    }
    Some(out)
}

impl Lexer<'_> {
    fn line(&self, at: usize) -> u32 {
        1 + self.newlines.partition_point(|&n| n < at) as u32
    }

    /// The byte at `at`, or 0 past the end.
    fn byte(&self, at: usize) -> u8 {
        self.bytes.get(at).copied().unwrap_or(0)
    }

    fn starts_with_ignore_case(&self, at: usize, word: &str) -> bool {
        self.bytes
            .get(at..at + word.len())
            .is_some_and(|b| b.eq_ignore_ascii_case(word.as_bytes()))
    }

    /// Skips inline HTML to the next tag that opens PHP mode: `<?php` and a whitespace byte or the end of the file, or
    /// `<?=`, in any case. A bare `<?` stays HTML. False when the file ends first.
    fn open_tag(&mut self) -> bool {
        let mut i = self.at;
        while let Some(found) = self.bytes[i..].windows(2).position(|w| w == b"<?") {
            let tag = i + found;
            if self.starts_with_ignore_case(tag + 2, "php")
                && self
                    .bytes
                    .get(tag + 5)
                    .is_none_or(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
            {
                self.at = tag + 5;
                return true;
            }
            if self.byte(tag + 2) == b'=' {
                self.at = tag + 3;
                return true;
            }
            i = tag + 1;
        }
        self.at = self.bytes.len();
        false
    }

    /// Reads code from `self.at` into `out`: to a `?>`, the end of the file or `__halt_compiler`, or, when `nested`
    /// (the code of a `{$` in a string), to its closing `}`. None when something is never closed.
    fn code(&mut self, nested: bool, out: &mut Vec<(Tok, u32)>) -> Option<End> {
        let mut depth = 0usize;
        while self.at < self.bytes.len() {
            let at = self.at;
            let line = self.line(at);
            let b = self.bytes[at];
            let next = self.byte(at + 1);
            match b {
                b' ' | b'\t' | b'\r' | b'\n' | 0x0b | 0x0c => self.at += 1,
                b'?' if next == b'-' && self.byte(at + 2) == b'>' => {
                    out.push((Tok::Arrow, line));
                    self.at += 3;
                }
                b'?' if next == b'>' => {
                    if nested {
                        return None;
                    }
                    out.push((Tok::Punct(b';'), line));
                    self.at += 2;
                    return Some(End::CloseTag);
                }
                // `#[` opens an attribute, read as code; any other `#` is a comment.
                b'#' if next == b'[' => self.at += 1,
                b'#' => self.line_comment(),
                b'/' if next == b'/' => self.line_comment(),
                b'/' if next == b'*' => {
                    let close = self.bytes[at + 2..].windows(2).position(|w| w == b"*/")?;
                    self.at = at + 2 + close + 2;
                }
                b'\'' => {
                    let value = self.single_quoted()?;
                    out.push((Tok::Str(value), line));
                }
                b'"' => {
                    let tok = self.interpolated(b'"')?;
                    out.push((tok, line));
                }
                b'`' => {
                    self.interpolated(b'`')?;
                    out.push((Tok::Other, line));
                }
                b'<' if self.heredoc()? => out.push((Tok::Other, line)),
                b'$' if label_start(next) => {
                    self.at += 1;
                    while label_char(self.byte(self.at)) {
                        self.at += 1;
                    }
                    out.push((Tok::Other, line));
                }
                b'0'..=b'9' => {
                    while label_char(self.byte(self.at)) {
                        self.at += 1;
                    }
                    let value = std::str::from_utf8(&self.bytes[at..self.at])
                        .ok()
                        .and_then(|digits| digits.parse().ok());
                    out.push((value.map_or(Tok::Other, Tok::Int), line));
                }
                _ if label_start(b) || (b == b'\\' && label_start(next)) => {
                    let name = self.name();
                    let member = matches!(out.last(), Some((Tok::Arrow | Tok::Scope, _)));
                    if !member && name.eq_ignore_ascii_case("__halt_compiler") {
                        return Some(End::Halt);
                    }
                    out.push((Tok::Name(name), line));
                }
                b'-' if next == b'>' => {
                    out.push((Tok::Arrow, line));
                    self.at += 2;
                }
                b':' if next == b':' => {
                    out.push((Tok::Scope, line));
                    self.at += 2;
                }
                b'{' => {
                    depth += 1;
                    out.push((Tok::Punct(b), line));
                    self.at += 1;
                }
                b'}' if nested && depth == 0 => {
                    self.at += 1;
                    return Some(End::CloseBrace);
                }
                b'}' => {
                    depth = depth.saturating_sub(1);
                    out.push((Tok::Punct(b), line));
                    self.at += 1;
                }
                _ => {
                    out.push((Tok::Punct(b), line));
                    self.at += 1;
                }
            }
        }
        if nested { None } else { Some(End::Eof) }
    }

    /// Skips a `//` or `#` comment, which ends at the end of the line, a `\n` or a `\r`, or at `?>`, whichever comes
    /// first (`scanner.l:2489-2511`).
    fn line_comment(&mut self) {
        while self.at < self.bytes.len() {
            match self.bytes[self.at] {
                b'\n' | b'\r' => return,
                b'?' if self.byte(self.at + 1) == b'>' => return,
                _ => self.at += 1,
            }
        }
    }

    /// Reads a name from `self.at`: segments of label characters joined by `\`, with an optional leading `\`.
    fn name(&mut self) -> String {
        let start = self.at;
        if self.byte(self.at) == b'\\' {
            self.at += 1;
        }
        loop {
            while label_char(self.byte(self.at)) {
                self.at += 1;
            }
            if self.byte(self.at) == b'\\' && label_start(self.byte(self.at + 1)) {
                self.at += 1;
            } else {
                break;
            }
        }
        String::from_utf8_lossy(&self.bytes[start..self.at]).into_owned()
    }

    /// Reads the single-quoted string at `self.at`, which takes only `\\` and `\'` as escapes. None when it is never
    /// closed.
    fn single_quoted(&mut self) -> Option<String> {
        let mut value = Vec::new();
        let mut i = self.at + 1;
        loop {
            match *self.bytes.get(i)? {
                b'\'' => break,
                b'\\' if matches!(self.byte(i + 1), b'\\' | b'\'') => {
                    value.push(self.bytes[i + 1]);
                    i += 2;
                }
                b => {
                    value.push(b);
                    i += 1;
                }
            }
        }
        self.at = i + 1;
        Some(String::from_utf8_lossy(&value).into_owned())
    }

    /// Reads the double-quoted string or backtick string at `self.at`, closed by `quote`. A double-quoted one is a
    /// string token only when it holds no `$` and no backslash; the code a `{$` or `${` nests is read with its own
    /// strings. None when it is never closed.
    fn interpolated(&mut self, quote: u8) -> Option<Tok> {
        let start = self.at + 1;
        let mut plain = true;
        let mut i = start;
        loop {
            let b = *self.bytes.get(i)?;
            let next = self.byte(i + 1);
            if b == quote {
                break;
            }
            match b {
                b'\\' => {
                    plain = false;
                    i += 2;
                }
                b'$' if next == b'{' => {
                    plain = false;
                    i = self.nested(i + 2)?;
                }
                b'{' if next == b'$' => {
                    plain = false;
                    i = self.nested(i + 1)?;
                }
                b'$' => {
                    plain = false;
                    i += 1;
                }
                _ => i += 1,
            }
        }
        let value = String::from_utf8_lossy(&self.bytes[start..i]).into_owned();
        self.at = i + 1;
        Some(if plain && quote == b'"' {
            Tok::Str(value)
        } else {
            Tok::Other
        })
    }

    /// Reads the code that starts at `from` to its closing `}`, and gives the offset after it. None past
    /// [`MAX_NESTING`].
    fn nested(&mut self, from: usize) -> Option<usize> {
        if self.nesting == MAX_NESTING {
            return None;
        }
        self.nesting += 1;
        self.at = from;
        let end = self.code(true, &mut Vec::new());
        self.nesting -= 1;
        end?;
        Some(self.at)
    }

    /// Reads a heredoc or nowdoc that starts at `self.at`, a `<`: true when one starts there, with `self.at` after its
    /// closing label. A `<` that opens none is left alone and gives false. None when the body is never closed.
    ///
    /// The closing label is at the start of a line, after optional spaces or tabs, and no label character follows it
    /// (PHP 7.3, `scanner.l:3031` and `:3155`).
    fn heredoc(&mut self) -> Option<bool> {
        let mut i = self.at;
        if !self.bytes[i..].starts_with(b"<<<") {
            return Some(false);
        }
        i += 3;
        while matches!(self.byte(i), b' ' | b'\t') {
            i += 1;
        }
        let quote = match self.byte(i) {
            q @ (b'\'' | b'"') => {
                i += 1;
                Some(q)
            }
            _ => None,
        };
        if !label_start(self.byte(i)) {
            return Some(false);
        }
        let label_at = i;
        while label_char(self.byte(i)) {
            i += 1;
        }
        let label = &self.bytes[label_at..i];
        if let Some(q) = quote {
            if self.byte(i) != q {
                return Some(false);
            }
            i += 1;
        }
        match self.byte(i) {
            b'\n' => i += 1,
            b'\r' => i += if self.byte(i + 1) == b'\n' { 2 } else { 1 },
            _ => return Some(false),
        }
        let interpolates = quote != Some(b'\'');
        let mut line_start = true;
        loop {
            if line_start {
                let mut j = i;
                while matches!(self.byte(j), b' ' | b'\t') {
                    j += 1;
                }
                if self.bytes[j..].starts_with(label) && !label_char(self.byte(j + label.len())) {
                    self.at = j + label.len();
                    return Some(true);
                }
                line_start = false;
            }
            let b = *self.bytes.get(i)?;
            let next = self.byte(i + 1);
            match b {
                b'\n' => {
                    i += 1;
                    line_start = true;
                }
                b'\r' => {
                    i += 1;
                    line_start = next != b'\n';
                }
                b'\\' if interpolates && next != b'\n' => i += 2,
                b'$' if interpolates && next == b'{' => i = self.nested(i + 2)?,
                b'{' if interpolates && next == b'$' => i = self.nested(i + 1)?,
                _ => i += 1,
            }
        }
    }
}

/// A class named by a top-level `use`, fully qualified with no leading `\`.
#[derive(Debug, PartialEq)]
pub(super) struct ClassUse {
    pub(super) name: String,
    /// The line of the item's name.
    pub(super) line: u32,
}

/// A `require` or `include` of a static path.
#[derive(Debug, PartialEq)]
pub(super) struct Require {
    /// The file it names, from the repository root, cleaned.
    pub(super) path: String,
    /// The line of its keyword.
    pub(super) line: u32,
}

/// What the reader takes from a PHP file.
#[derive(Debug, Default, PartialEq)]
pub(super) struct Reading {
    pub(super) uses: Vec<ClassUse>,
    pub(super) requires: Vec<Require>,
}

impl Reading {
    /// What `file` holds, given its `text`; none when the reader cannot lex it.
    pub(super) fn read(file: &str, text: &str) -> Option<Reading> {
        let tokens = tokens(text)?;
        let mut reading = Reading::default();
        // The brace depth, and the depth of a braced namespace's body, where its `use` lines are top-level.
        let mut depth = 0usize;
        let mut top = 0usize;
        let mut namespace_brace = None;
        let mut i = 0;
        while i < tokens.len() {
            let (tok, line) = &tokens[i];
            match tok {
                Tok::Punct(b'{') => {
                    depth += 1;
                    if namespace_brace == Some(i) {
                        top = depth;
                    }
                }
                Tok::Punct(b'}') => {
                    depth = depth.saturating_sub(1);
                    if depth < top {
                        top = 0;
                    }
                }
                Tok::Name(word) if !is_member(&tokens, i) => {
                    if word.eq_ignore_ascii_case("namespace") {
                        // `namespace Name {` and `namespace {`, a braced block.
                        let after = i
                            + 1
                            + usize::from(matches!(tokens.get(i + 1), Some((Tok::Name(_), _))));
                        if depth == 0 && is_punct(&tokens, after, b'{') {
                            namespace_brace = Some(after);
                        }
                    } else if word.eq_ignore_ascii_case("use") && depth == top {
                        // A closure's `use (` reads as no item.
                        let (uses, next) = read_use(&tokens, i + 1);
                        reading.uses.extend(uses);
                        i = next;
                        continue;
                    } else if ["require", "require_once", "include", "include_once"]
                        .iter()
                        .any(|keyword| word.eq_ignore_ascii_case(keyword))
                        && let Some(path) = static_path(&tokens, i + 1, file)
                    {
                        reading.requires.push(Require { path, line: *line });
                    }
                }
                _ => {}
            }
            i += 1;
        }
        Some(reading)
    }
}

/// Whether the token at `at` is a member name: after `->`, `?->` or `::`, or after `function` in a declaration, so
/// `$this->include(…)` and `function require()` are no keyword.
fn is_member(tokens: &Tokens, at: usize) -> bool {
    match at.checked_sub(1).and_then(|p| tokens.get(p)) {
        Some((Tok::Arrow | Tok::Scope, _)) => true,
        Some((Tok::Name(w), _)) => w.eq_ignore_ascii_case("function"),
        _ => false,
    }
}

fn is_word(tokens: &Tokens, at: usize, word: &str) -> bool {
    matches!(tokens.get(at), Some((Tok::Name(w), _)) if w.eq_ignore_ascii_case(word))
}

fn is_punct(tokens: &Tokens, at: usize, byte: u8) -> bool {
    matches!(tokens.get(at), Some((Tok::Punct(b), _)) if *b == byte)
}

/// Reads the `use` declaration whose items start at `at`, after the `use`, and gives its class items and the token after
/// it. `use function` and `use const` items, and those of a mixed group, give nothing. A declaration that does not read
/// as one ends where it stops.
fn read_use(tokens: &Tokens, mut at: usize) -> (Vec<ClassUse>, usize) {
    let mut found = Vec::new();
    let typed = is_word(tokens, at, "function") || is_word(tokens, at, "const");
    if typed {
        at += 1;
    }
    loop {
        let Some((Tok::Name(name), line)) = tokens.get(at) else {
            return (found, at);
        };
        let name = name.trim_start_matches('\\');
        at += 1;
        if is_punct(tokens, at, b'\\') && is_punct(tokens, at + 1, b'{') {
            at += 2;
            loop {
                let item_typed =
                    !typed && (is_word(tokens, at, "function") || is_word(tokens, at, "const"));
                if item_typed {
                    at += 1;
                }
                let Some((Tok::Name(item), line)) = tokens.get(at) else {
                    return (found, at);
                };
                at += 1;
                if is_word(tokens, at, "as") {
                    at += 2;
                }
                if !typed && !item_typed {
                    found.push(ClassUse {
                        name: format!("{name}\\{item}"),
                        line: *line,
                    });
                }
                if is_punct(tokens, at, b',') {
                    at += 1;
                    if is_punct(tokens, at, b'}') {
                        at += 1;
                        break;
                    }
                } else if is_punct(tokens, at, b'}') {
                    at += 1;
                    break;
                } else {
                    return (found, at);
                }
            }
        } else {
            if is_word(tokens, at, "as") {
                at += 2;
            }
            if !typed {
                found.push(ClassUse {
                    name: name.to_string(),
                    line: *line,
                });
            }
        }
        if is_punct(tokens, at, b',') {
            at += 1;
        } else if is_punct(tokens, at, b';') {
            return (found, at + 1);
        } else {
            return (found, at);
        }
    }
}

/// One term of a path expression: its text, and whether it starts from a base. A based term's text is the path under
/// the repository root, `""` for the root and `/a/b` for the folder `a/b`.
struct Term {
    text: String,
    based: bool,
}

/// The file a `require` expression names, when it is static: the tokens from `at`, after the keyword, are one base and
/// one or more string literals joined by `.` (spec #647, "Requires"). None when the expression is dynamic, is relative
/// to the working directory or leaves the repository.
fn static_path(tokens: &Tokens, at: usize, file: &str) -> Option<String> {
    let (term, end) = path(tokens, at, file)?;
    let ends = match tokens.get(end) {
        None => true,
        Some((Tok::Punct(b), _)) => b";),]?:|&".contains(b),
        Some((Tok::Name(w), _)) => ["or", "and", "xor"]
            .iter()
            .any(|operator| w.eq_ignore_ascii_case(operator)),
        Some(_) => false,
    };
    if !ends || !term.based {
        return None;
    }
    super::clean(&term.text).filter(|path| !path.is_empty())
}

/// `term { . term }`: the first term may be a base, the rest are string literals.
fn path(tokens: &Tokens, at: usize, file: &str) -> Option<(Term, usize)> {
    let (mut joined, mut end) = term(tokens, at, file)?;
    while is_punct(tokens, end, b'.') {
        let (next, after) = term(tokens, end + 1, file)?;
        if next.based {
            return None;
        }
        joined.text.push_str(&next.text);
        end = after;
    }
    Some((joined, end))
}

fn term(tokens: &Tokens, at: usize, file: &str) -> Option<(Term, usize)> {
    match &tokens.get(at)?.0 {
        Tok::Str(text) => Some((
            Term {
                text: text.clone(),
                based: false,
            },
            at + 1,
        )),
        Tok::Punct(b'(') => {
            let (inner, end) = path(tokens, at + 1, file)?;
            is_punct(tokens, end, b')').then_some((inner, end + 1))
        }
        Tok::Name(word) if is_function(word, "realpath") && is_punct(tokens, at + 1, b'(') => {
            let (mut inner, end) = path(tokens, at + 2, file)?;
            // `realpath` goes around a base only, and resolves `.` and `..` in it.
            if !is_punct(tokens, end, b')') || !inner.based {
                return None;
            }
            let cleaned = super::clean(&inner.text)?;
            inner.text = if cleaned.is_empty() {
                String::new()
            } else {
                format!("/{cleaned}")
            };
            Some((inner, end + 1))
        }
        Tok::Name(_) => {
            let (folders, end) = directory(tokens, at, file)?;
            Some((
                Term {
                    text: folders.iter().map(|folder| format!("/{folder}")).collect(),
                    based: true,
                },
                end,
            ))
        }
        _ => None,
    }
}

/// The folders of a base, `__DIR__` or `dirname(…)` of one, and the token after it.
fn directory(tokens: &Tokens, at: usize, file: &str) -> Option<(Vec<String>, usize)> {
    let (Tok::Name(word), _) = tokens.get(at)? else {
        return None;
    };
    let mut folders: Vec<String> = file.split('/').map(String::from).collect();
    if word.eq_ignore_ascii_case("__DIR__") {
        folders.pop();
        return Some((folders, at + 1));
    }
    if !is_function(word, "dirname") || !is_punct(tokens, at + 1, b'(') {
        return None;
    }
    let (mut folders, mut end) = if is_word(tokens, at + 2, "__FILE__") {
        (folders, at + 3)
    } else {
        directory(tokens, at + 2, file)?
    };
    let mut levels = 1;
    if is_punct(tokens, end, b',') {
        let Some((Tok::Int(n), _)) = tokens.get(end + 1) else {
            return None;
        };
        levels = *n;
        end += 2;
    }
    if !is_punct(tokens, end, b')') || levels == 0 {
        return None;
    }
    // A level past the repository root leaves it.
    for _ in 0..levels {
        folders.pop()?;
    }
    Some((folders, end + 1))
}

/// Whether `word` is the function `name`, written bare or with a leading `\`, in any case.
fn is_function(word: &str, name: &str) -> bool {
    word.strip_prefix('\\')
        .unwrap_or(word)
        .eq_ignore_ascii_case(name)
}
