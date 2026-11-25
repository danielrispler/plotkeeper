//! Go's build constraints, judged as `go mod tidy` judges them: `ignore` is false and every other tag is both true and
//! false, so a file is left out only when its constraint can never hold with `ignore` off (spec #569, "`reads` and
//! `skips`"). Ported from golang/go's `cmd/go/internal/imports/build.go` (`ShouldBuild`, `parseFileHeader`, `eval`,
//! `matchTag`) and `go/build/constraint/expr.go` (the `//go:build` and `// +build` parsers).

/// Whether a file's build constraint leaves it out of every build `go mod tidy` considers. As `ShouldBuild` has it, a
/// `//go:build` line in the leading comments controls; with none, every `// +build` line of the header (the leading
/// comments up to their last blank line) must hold. Two `//go:build` lines, or one that does not parse, leave the file
/// out, as Go rejects it.
pub(super) fn never_builds(text: &str) -> bool {
    let Some((header, go_build)) = file_header(text) else {
        return true;
    };
    match go_build {
        Some(line) => parse_go_build(line).is_none_or(|x| !eval(&x, true)),
        None => header
            .lines()
            .map(str::trim)
            .filter_map(plus_build)
            .any(|x| !eval(&x, true)),
    }
}

/// The header (the leading run of `//` comments and blank lines, cut at its last blank line) and the `//go:build` line
/// of the leading comments, as `parseFileHeader` reads them; none when there are two `//go:build` lines.
fn file_header(text: &str) -> Option<(&str, Option<&str>)> {
    let mut end = 0;
    let mut ended = false;
    let mut in_slash_star = false;
    let mut go_build = None;
    let mut at = 0;
    'lines: while at < text.len() {
        let next = text[at..].find('\n').map_or(text.len(), |i| at + i + 1);
        let mut line = text[at..next].trim();
        at = next;
        if line.is_empty() && !ended {
            end = at;
            continue;
        }
        if !line.starts_with("//") {
            ended = true;
        }
        if !in_slash_star && is_go_build(line) {
            if go_build.is_some() {
                return None;
            }
            go_build = Some(line);
        }
        while !line.is_empty() {
            if in_slash_star {
                let Some(i) = line.find("*/") else {
                    continue 'lines;
                };
                in_slash_star = false;
                line = line[i + 2..].trim();
                continue;
            }
            if line.starts_with("//") {
                continue 'lines;
            }
            if let Some(rest) = line.strip_prefix("/*") {
                in_slash_star = true;
                line = rest.trim();
                continue;
            }
            break 'lines;
        }
    }
    Some((&text[..end], go_build))
}

/// Whether a trimmed line is a `//go:build` comment: the prefix, then nothing or white space.
fn is_go_build(line: &str) -> bool {
    line.strip_prefix("//go:build")
        .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '\t']))
}

#[derive(Debug)]
enum Expr {
    Tag(String),
    Not(Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
}

/// `eval` with `go mod tidy`'s tags: `prefer` is what any tag but `ignore` (and the empty one) is, flipped under a `!`.
fn eval(x: &Expr, prefer: bool) -> bool {
    match x {
        Expr::Tag(name) => match_tag(name, prefer),
        Expr::Not(x) => !eval(x, !prefer),
        Expr::And(a, b) => eval(a, prefer) && eval(b, prefer),
        Expr::Or(a, b) => eval(a, prefer) || eval(b, prefer),
    }
}

/// `matchTag` with the tags `{"*": true}` that `go mod tidy` passes: every tag but `ignore` matches as `prefer` asks.
fn match_tag(name: &str, prefer: bool) -> bool {
    !name.is_empty() && name != "ignore" && prefer
}

/// Whether `s` is a valid tag: letters, digits, `_` and `.`, at least one.
fn is_tag(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
}

/// A `// +build` line's expression: its fields ORed, each field's comma-separated terms ANDed, a bad term read as
/// `ignore`, and an empty line as `ignore`. None when the line is not a `+build` line.
fn plus_build(line: &str) -> Option<Expr> {
    let rest = line.strip_prefix("//")?.trim();
    let rest = rest.strip_prefix("+build")?;
    if !(rest.is_empty() || rest.starts_with([' ', '\t'])) {
        return None;
    }
    let ignore = || Expr::Tag("ignore".into());
    let mut or: Option<Expr> = None;
    for clause in rest.split_whitespace() {
        let mut and: Option<Expr> = None;
        for lit in clause.split(',') {
            let z = if lit.starts_with("!!") || lit == "!" {
                ignore()
            } else {
                let (neg, lit) = lit
                    .strip_prefix('!')
                    .map_or((false, lit), |rest| (true, rest));
                let z = if is_tag(lit) {
                    Expr::Tag(lit.into())
                } else {
                    ignore()
                };
                if neg { Expr::Not(Box::new(z)) } else { z }
            };
            and = Some(match and {
                Some(y) => Expr::And(Box::new(y), Box::new(z)),
                None => z,
            });
        }
        let y = and.unwrap_or_else(ignore);
        or = Some(match or {
            Some(x) => Expr::Or(Box::new(x), Box::new(y)),
            None => y,
        });
    }
    Some(or.unwrap_or_else(ignore))
}

/// A `//go:build` line's expression, `||` binding looser than `&&`, with `!` and parentheses; none when it does not
/// parse.
fn parse_go_build(line: &str) -> Option<Expr> {
    let mut p = Parser {
        text: line.strip_prefix("//go:build")?,
        at: 0,
    };
    let x = p.or()?;
    p.skip_space();
    (p.at == p.text.len()).then_some(x)
}

struct Parser<'a> {
    text: &'a str,
    at: usize,
}

impl Parser<'_> {
    fn skip_space(&mut self) {
        let rest = &self.text[self.at..];
        self.at += rest.len() - rest.trim_start_matches([' ', '\t']).len();
    }

    /// Takes `token` when it comes next.
    fn eat(&mut self, token: &str) -> bool {
        self.skip_space();
        let found = self.text[self.at..].starts_with(token);
        if found {
            self.at += token.len();
        }
        found
    }

    fn or(&mut self) -> Option<Expr> {
        let mut x = self.and()?;
        while self.eat("||") {
            x = Expr::Or(Box::new(x), Box::new(self.and()?));
        }
        Some(x)
    }

    fn and(&mut self) -> Option<Expr> {
        let mut x = self.not()?;
        while self.eat("&&") {
            x = Expr::And(Box::new(x), Box::new(self.not()?));
        }
        Some(x)
    }

    fn not(&mut self) -> Option<Expr> {
        if self.eat("!") {
            return Some(Expr::Not(Box::new(self.not()?)));
        }
        if self.eat("(") {
            let x = self.or()?;
            return self.eat(")").then_some(x);
        }
        self.skip_space();
        let rest = &self.text[self.at..];
        let len = rest
            .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.'))
            .unwrap_or(rest.len());
        self.at += len;
        (len > 0).then(|| Expr::Tag(rest[..len].into()))
    }
}
