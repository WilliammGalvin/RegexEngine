use std::fmt;

use crate::ast::{Assertion, Ast, Class, ClassItem, Repeat};
use crate::lex::{LexError, Span, Spanned, Token, tokenize};

const MAX_DEPTH: u32 = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParseErrorKind {
    Lex(LexError),
    UnexpectedToken(Token),
    NothingToRepeat,
    NestedQuantifier,
    UnmatchedRightParen,
    UnclosedGroup,
    UnclosedClass,
    InvalidClassRange,
    InvalidRepeat,
    InvalidRepeatRange,
    RepeatTooLarge,
    UnknownEscape(char),
    UnsupportedEscape(char),
    NestingTooDeep,
}

impl fmt::Display for ParseErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lex(e) => write!(f, "{e}"),
            Self::UnexpectedToken(t) => write!(f, "unexpected token `{t}`"),
            Self::NothingToRepeat => f.write_str("quantifier has nothing to repeat"),
            Self::NestedQuantifier => f.write_str("quantifier cannot follow another quantifier"),
            Self::UnmatchedRightParen => f.write_str("unmatched `)`"),
            Self::UnclosedGroup => f.write_str("unclosed group: missing `)`"),
            Self::UnclosedClass => f.write_str("unclosed character class: missing `]`"),
            Self::InvalidClassRange => f.write_str("invalid character class range"),
            Self::InvalidRepeat => {
                f.write_str("invalid repetition: expected `{n}`, `{n,}` or `{n,m}`")
            }
            Self::InvalidRepeatRange => {
                f.write_str("invalid repetition: minimum is greater than maximum")
            }
            Self::RepeatTooLarge => f.write_str("repetition count is too large"),
            Self::UnknownEscape(c) => write!(f, "unknown escape sequence `\\{c}`"),
            Self::UnsupportedEscape(c) => {
                write!(
                    f,
                    "escape `\\{c}` is not supported inside a character class"
                )
            }
            Self::NestingTooDeep => f.write_str("groups are nested too deeply"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    kind: ParseErrorKind,
    span: Span,
}

impl ParseError {
    fn new(kind: ParseErrorKind, span: Span) -> Self {
        Self { kind, span }
    }

    #[must_use]
    pub fn kind(&self) -> &ParseErrorKind {
        &self.kind
    }

    #[must_use]
    pub fn span(&self) -> Span {
        self.span
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} (at bytes {}..{})",
            self.kind, self.span.start, self.span.end
        )
    }
}

impl std::error::Error for ParseError {}

impl From<LexError> for ParseError {
    fn from(e: LexError) -> Self {
        let span = e.span();
        Self::new(ParseErrorKind::Lex(e), span)
    }
}

struct Parser<'t> {
    tokens: &'t [Spanned],
    pos: usize,
    depth: u32,
}

impl Parser<'_> {
    fn peek(&self) -> Option<Spanned> {
        self.tokens.get(self.pos).copied()
    }

    fn peek_token(&self) -> Option<Token> {
        self.peek().map(|t| t.token)
    }

    fn bump(&mut self) -> Option<Spanned> {
        let tok = self.peek()?;
        self.pos += 1;
        Some(tok)
    }

    fn eat(&mut self, token: Token) -> Option<Spanned> {
        if self.peek_token() == Some(token) {
            self.bump()
        } else {
            None
        }
    }

    fn alternation(&mut self) -> Result<Ast, ParseError> {
        let mut branches = vec![self.concat()?];
        while self.eat(Token::Pipe).is_some() {
            branches.push(self.concat()?);
        }

        Ok(if branches.len() == 1 {
            branches.remove(0)
        } else {
            Ast::Alternate(branches)
        })
    }

    fn concat(&mut self) -> Result<Ast, ParseError> {
        let mut items = Vec::new();
        while let Some(t) = self.peek() {
            if matches!(t.token, Token::Pipe | Token::RightParen) {
                break;
            }
            items.push(self.repeat()?);
        }
        Ok(match items.len() {
            0 => Ast::Empty,
            1 => items.remove(0),
            _ => Ast::Concat(items),
        })
    }

    fn repeat(&mut self) -> Result<Ast, ParseError> {
        let (node, repeatable) = self.atom()?;

        if let Some(t) = self.peek() {
            if !repeatable && is_quantifier(t.token) {
                return Err(ParseError::new(ParseErrorKind::NothingToRepeat, t.span));
            }
        }

        let Some((min, max)) = self.quantifier()? else {
            return Ok(node);
        };

        if let Some(t) = self.peek() {
            if is_quantifier(t.token) {
                return Err(ParseError::new(ParseErrorKind::NestedQuantifier, t.span));
            }
        }

        Ok(Ast::Repeat(Repeat {
            node: Box::new(node),
            min,
            max,
        }))
    }

    fn atom(&mut self) -> Result<(Ast, bool), ParseError> {
        let Some(t) = self.bump() else {
            return Err(ParseError::new(
                ParseErrorKind::InvalidRepeat,
                Span::new(0, 0),
            ));
        };

        let ast = match t.token {
            Token::Literal(c) => Ast::Literal(c),
            Token::Escaped(c) => escape(c, t.span)?,
            Token::Period => Ast::Any,
            Token::Caret => return Ok((Ast::Assertion(Assertion::Start), false)),
            Token::DollarSign => return Ok((Ast::Assertion(Assertion::End), false)),
            Token::LeftParen => return Ok((self.group(t.span)?, true)),
            Token::LeftBracket => Ast::Class(self.class(t.span)?),

            Token::Plus | Token::Star | Token::QuestionMark | Token::LeftBrace => {
                return Err(ParseError::new(ParseErrorKind::NothingToRepeat, t.span));
            }

            Token::RightBrace | Token::RightBracket | Token::Dash => {
                Ast::Literal(token_char(t.token))
            }

            Token::Pipe | Token::RightParen => {
                return Err(ParseError::new(
                    ParseErrorKind::UnexpectedToken(t.token),
                    t.span,
                ));
            }
        };
        Ok((ast, true))
    }

    fn group(&mut self, open: Span) -> Result<Ast, ParseError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(ParseError::new(ParseErrorKind::NestingTooDeep, open));
        }

        let inner = self.alternation()?;
        self.depth -= 1;

        match self.eat(Token::RightParen) {
            Some(_) => Ok(inner),
            None => Err(ParseError::new(ParseErrorKind::UnclosedGroup, open)),
        }
    }

    fn class(&mut self, open: Span) -> Result<Class, ParseError> {
        let negated = self.eat(Token::Caret).is_some();
        let mut items = Vec::new();
        let mut first = true;

        loop {
            let Some(t) = self.bump() else {
                return Err(ParseError::new(ParseErrorKind::UnclosedClass, open));
            };

            if t.token == Token::RightBracket && !first {
                break;
            }
            first = false;

            let lo = match t.token {
                Token::Escaped(c) => match class_escape(c, t.span)? {
                    ClassEscape::Char(ch) => ch,
                    ClassEscape::Items(more) => {
                        items.extend(more);
                        continue;
                    }
                },
                other => token_char(other),
            };

            let is_range = self.peek_token() == Some(Token::Dash)
                && self
                    .tokens
                    .get(self.pos + 1)
                    .is_some_and(|n| n.token != Token::RightBracket);

            if !is_range {
                items.push(ClassItem::Char(lo));
                continue;
            }

            self.pos += 1;
            let hi_tok = self
                .bump()
                .ok_or_else(|| ParseError::new(ParseErrorKind::UnclosedClass, open))?;
            let range_span = Span::new(t.span.start, hi_tok.span.end);

            let hi = match hi_tok.token {
                Token::Escaped(c) => match class_escape(c, hi_tok.span)? {
                    ClassEscape::Char(ch) => ch,
                    ClassEscape::Items(_) => {
                        return Err(ParseError::new(
                            ParseErrorKind::InvalidClassRange,
                            range_span,
                        ));
                    }
                },
                other => token_char(other),
            };

            if lo > hi {
                return Err(ParseError::new(
                    ParseErrorKind::InvalidClassRange,
                    range_span,
                ));
            }

            items.push(ClassItem::Range(lo, hi));
        }

        Ok(Class { negated, items })
    }

    fn quantifier(&mut self) -> Result<Option<(u32, Option<u32>)>, ParseError> {
        let Some(t) = self.peek() else {
            return Ok(None);
        };

        let bounds = match t.token {
            Token::Star => (0, None),
            Token::Plus => (1, None),
            Token::QuestionMark => (0, Some(1)),
            Token::LeftBrace => {
                self.pos += 1;
                return self.counted(t.span).map(Some);
            }
            _ => return Ok(None),
        };

        self.pos += 1;
        Ok(Some(bounds))
    }

    fn counted(&mut self, open: Span) -> Result<(u32, Option<u32>), ParseError> {
        let min = self.number(open)?;
        let max = if self.eat(Token::Literal(',')).is_some() {
            if self.peek_token() == Some(Token::RightBrace) {
                None
            } else {
                Some(self.number(open)?)
            }
        } else {
            Some(min)
        };

        let Some(close) = self.eat(Token::RightBrace) else {
            return Err(ParseError::new(ParseErrorKind::InvalidRepeat, open));
        };

        if max.is_some_and(|m| m < min) {
            return Err(ParseError::new(
                ParseErrorKind::InvalidRepeatRange,
                Span::new(open.start, close.span.end),
            ));
        }

        Ok((min, max))
    }

    fn number(&mut self, open: Span) -> Result<u32, ParseError> {
        let mut value: Option<u32> = None;
        while let Some(Spanned {
            token: Token::Literal(c),
            span,
        }) = self.peek()
        {
            let Some(digit) = c.to_digit(10) else { break };
            let next = value
                .unwrap_or(0)
                .checked_mul(10)
                .and_then(|v| v.checked_add(digit))
                .ok_or_else(|| ParseError::new(ParseErrorKind::RepeatTooLarge, span))?;
            value = Some(next);
            self.pos += 1;
        }

        value.ok_or_else(|| ParseError::new(ParseErrorKind::InvalidRepeat, open))
    }
}

fn is_quantifier(token: Token) -> bool {
    matches!(
        token,
        Token::Plus | Token::Star | Token::QuestionMark | Token::LeftBrace
    )
}

fn token_char(token: Token) -> char {
    match token {
        Token::Literal(c) | Token::Escaped(c) => c,
        Token::Plus => '+',
        Token::Star => '*',
        Token::QuestionMark => '?',
        Token::LeftBrace => '{',
        Token::RightBrace => '}',
        Token::LeftParen => '(',
        Token::RightParen => ')',
        Token::LeftBracket => '[',
        Token::RightBracket => ']',
        Token::Caret => '^',
        Token::DollarSign => '$',
        Token::Period => '.',
        Token::Pipe => '|',
        Token::Dash => '-',
    }
}

fn simple_escape(c: char, span: Span) -> Result<char, ParseError> {
    match c {
        'n' => Ok('\n'),
        't' => Ok('\t'),
        'r' => Ok('\r'),

        c if c.is_ascii_alphanumeric() => {
            Err(ParseError::new(ParseErrorKind::UnknownEscape(c), span))
        }

        other => Ok(other),
    }
}

fn perl_items(c: char) -> Vec<ClassItem> {
    match c {
        'd' => vec![ClassItem::Range('0', '9')],
        'w' => vec![
            ClassItem::Range('a', 'z'),
            ClassItem::Range('A', 'Z'),
            ClassItem::Range('0', '9'),
            ClassItem::Char('_'),
        ],
        _ => [' ', '\t', '\n', '\r', '\x0B', '\x0C']
            .into_iter()
            .map(ClassItem::Char)
            .collect(),
    }
}

fn escape(c: char, span: Span) -> Result<Ast, ParseError> {
    Ok(match c {
        'd' | 'w' | 's' => Ast::Class(Class {
            negated: false,
            items: perl_items(c),
        }),
        'D' | 'W' | 'S' => Ast::Class(Class {
            negated: true,
            items: perl_items(c.to_ascii_lowercase()),
        }),
        _ => Ast::Literal(simple_escape(c, span)?),
    })
}

enum ClassEscape {
    Char(char),
    Items(Vec<ClassItem>),
}

fn class_escape(c: char, span: Span) -> Result<ClassEscape, ParseError> {
    match c {
        'd' | 'w' | 's' => Ok(ClassEscape::Items(perl_items(c))),
        'D' | 'W' | 'S' => Err(ParseError::new(ParseErrorKind::UnsupportedEscape(c), span)),
        _ => Ok(ClassEscape::Char(simple_escape(c, span)?)),
    }
}

pub fn parse(tokens: &[Spanned]) -> Result<Ast, ParseError> {
    let mut parser = Parser {
        tokens,
        pos: 0,
        depth: 0,
    };

    let ast = parser.alternation()?;
    match parser.peek() {
        None => Ok(ast),
        Some(t) => Err(ParseError::new(ParseErrorKind::UnmatchedRightParen, t.span)),
    }
}

pub fn parse_str(pattern: &str) -> Result<Ast, ParseError> {
    let tokens = tokenize(pattern)?;
    parse(&tokens)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lex::tokenize;

    fn ok(pattern: &str) -> Ast {
        parse(&tokenize(pattern).unwrap()).unwrap()
    }

    fn err(pattern: &str) -> ParseError {
        parse(&tokenize(pattern).unwrap()).unwrap_err()
    }

    fn lit(c: char) -> Ast {
        Ast::Literal(c)
    }

    fn rep(node: Ast, min: u32, max: Option<u32>) -> Ast {
        Ast::Repeat(Repeat {
            node: Box::new(node),
            min,
            max,
        })
    }

    fn class(negated: bool, items: Vec<ClassItem>) -> Ast {
        Ast::Class(Class { negated, items })
    }

    #[test]
    fn empty_token_slice_is_empty_ast() {
        assert_eq!(parse(&[]).unwrap(), Ast::Empty);
    }

    #[test]
    fn parse_str_matches_lex_then_parse() {
        for pattern in ["", "a", "abc(d*)+", "[a-c]{2,3}|x", r"\d+\.\d*"] {
            assert_eq!(
                parse_str(pattern).unwrap(),
                parse(&tokenize(pattern).unwrap()).unwrap(),
                "pattern: {pattern:?}"
            );
        }
    }

    #[test]
    fn lex_errors_are_wrapped() {
        let e = parse_str(r"ab\").unwrap_err();
        assert_eq!(
            e.kind(),
            &ParseErrorKind::Lex(LexError::TrailingBackslash { pos: 2 })
        );
        assert_eq!(e.span(), Span::new(2, 3));
    }

    #[test]
    fn parse_works_on_a_hand_built_slice() {
        let tokens = [
            Spanned {
                token: Token::Literal('a'),
                span: Span::new(0, 1),
            },
            Spanned {
                token: Token::Plus,
                span: Span::new(1, 2),
            },
        ];
        assert_eq!(parse(&tokens).unwrap(), rep(lit('a'), 1, None));
    }

    #[test]
    fn empty_forms() {
        assert_eq!(ok(""), Ast::Empty);
        assert_eq!(ok("()"), Ast::Empty);
        assert_eq!(ok("a|"), Ast::Alternate(vec![lit('a'), Ast::Empty]));
        assert_eq!(ok("|a"), Ast::Alternate(vec![Ast::Empty, lit('a')]));
    }

    #[test]
    fn single_literal_is_not_wrapped() {
        assert_eq!(ok("a"), lit('a'));
        assert_eq!(ok("(a)"), lit('a'));
        assert_eq!(ok("((a))"), lit('a'));
    }

    #[test]
    fn group_with_quantifiers() {
        assert_eq!(
            ok("abc(d*)+"),
            Ast::Concat(vec![
                lit('a'),
                lit('b'),
                lit('c'),
                rep(rep(lit('d'), 0, None), 1, None),
            ])
        );
    }

    #[test]
    fn precedence() {
        assert_eq!(
            ok("ab*"),
            Ast::Concat(vec![lit('a'), rep(lit('b'), 0, None)])
        );
        assert_eq!(
            ok("ab|cd"),
            Ast::Alternate(vec![
                Ast::Concat(vec![lit('a'), lit('b')]),
                Ast::Concat(vec![lit('c'), lit('d')]),
            ])
        );
        assert_eq!(
            ok("a(b|c)"),
            Ast::Concat(vec![lit('a'), Ast::Alternate(vec![lit('b'), lit('c')])])
        );
        assert_eq!(
            ok("a|b|c"),
            Ast::Alternate(vec![lit('a'), lit('b'), lit('c')])
        );
    }

    #[test]
    fn quantifier_forms() {
        assert_eq!(ok("a*"), rep(lit('a'), 0, None));
        assert_eq!(ok("a+"), rep(lit('a'), 1, None));
        assert_eq!(ok("a?"), rep(lit('a'), 0, Some(1)));
        assert_eq!(ok("a{3}"), rep(lit('a'), 3, Some(3)));
        assert_eq!(ok("a{2,}"), rep(lit('a'), 2, None));
        assert_eq!(ok("a{2,4}"), rep(lit('a'), 2, Some(4)));
        assert_eq!(ok("a{0}"), rep(lit('a'), 0, Some(0)));
        assert_eq!(
            ok("(ab){2}"),
            rep(Ast::Concat(vec![lit('a'), lit('b')]), 2, Some(2))
        );
    }

    #[test]
    fn classes() {
        assert_eq!(ok("[a-c]"), class(false, vec![ClassItem::Range('a', 'c')]));
        assert_eq!(
            ok("[^a-c_]"),
            class(true, vec![ClassItem::Range('a', 'c'), ClassItem::Char('_')])
        );
        assert_eq!(
            ok("[]a]"),
            class(false, vec![ClassItem::Char(']'), ClassItem::Char('a')])
        );
        assert_eq!(
            ok("[^]a]"),
            class(true, vec![ClassItem::Char(']'), ClassItem::Char('a')])
        );
        assert_eq!(
            ok("[-a]"),
            class(false, vec![ClassItem::Char('-'), ClassItem::Char('a')])
        );
        assert_eq!(
            ok("[a-]"),
            class(false, vec![ClassItem::Char('a'), ClassItem::Char('-')])
        );
        assert_eq!(
            ok("[.+*]"),
            class(
                false,
                vec![
                    ClassItem::Char('.'),
                    ClassItem::Char('+'),
                    ClassItem::Char('*')
                ]
            )
        );
        assert_eq!(
            ok(r"[\d_]"),
            class(
                false,
                vec![ClassItem::Range('0', '9'), ClassItem::Char('_')]
            )
        );
        assert_eq!(ok(r"[\]]"), class(false, vec![ClassItem::Char(']')]));
        assert_eq!(
            ok(r"[\d-z]"),
            class(
                false,
                vec![
                    ClassItem::Range('0', '9'),
                    ClassItem::Char('-'),
                    ClassItem::Char('z'),
                ]
            )
        );
    }

    #[test]
    fn escapes() {
        assert_eq!(ok(r"\."), lit('.'));
        assert_eq!(ok(r"\n"), lit('\n'));
        assert_eq!(ok(r"\\"), lit('\\'));
        assert_eq!(ok(r"\d"), class(false, vec![ClassItem::Range('0', '9')]));
        assert_eq!(ok(r"\D"), class(true, vec![ClassItem::Range('0', '9')]));
    }

    #[test]
    fn stray_closers_and_dashes_are_literals() {
        assert_eq!(
            ok("a-b}]"),
            Ast::Concat(vec![lit('a'), lit('-'), lit('b'), lit('}'), lit(']')])
        );
    }

    #[test]
    fn anchors_and_dot() {
        assert_eq!(
            ok("^a$"),
            Ast::Concat(vec![
                Ast::Assertion(Assertion::Start),
                lit('a'),
                Ast::Assertion(Assertion::End),
            ])
        );
        assert_eq!(ok("."), Ast::Any);
    }

    #[test]
    fn errors_have_kind_and_span() {
        let e = err("*a");
        assert_eq!(e.kind(), &ParseErrorKind::NothingToRepeat);
        assert_eq!(e.span(), Span::new(0, 1));

        let e = err("a**");
        assert_eq!(e.kind(), &ParseErrorKind::NestedQuantifier);
        assert_eq!(e.span(), Span::new(2, 3));

        let e = err("a)");
        assert_eq!(e.kind(), &ParseErrorKind::UnmatchedRightParen);
        assert_eq!(e.span(), Span::new(1, 2));

        let e = err("ab(c");
        assert_eq!(e.kind(), &ParseErrorKind::UnclosedGroup);
        assert_eq!(e.span(), Span::new(2, 3));

        let e = err("a[bc");
        assert_eq!(e.kind(), &ParseErrorKind::UnclosedClass);
        assert_eq!(e.span(), Span::new(1, 2));

        let e = err("[z-a]");
        assert_eq!(e.kind(), &ParseErrorKind::InvalidClassRange);
        assert_eq!(e.span(), Span::new(1, 4));

        let e = err("a{3,1}");
        assert_eq!(e.kind(), &ParseErrorKind::InvalidRepeatRange);
        assert_eq!(e.span(), Span::new(1, 6));
    }

    #[test]
    fn more_errors() {
        assert_eq!(err("^*").kind(), &ParseErrorKind::NothingToRepeat);
        assert_eq!(err("$+").kind(), &ParseErrorKind::NothingToRepeat);
        assert_eq!(err("(*)").kind(), &ParseErrorKind::NothingToRepeat);
        assert_eq!(err("|*").kind(), &ParseErrorKind::NothingToRepeat);
        assert_eq!(err("a{").kind(), &ParseErrorKind::InvalidRepeat);
        assert_eq!(err("a{x}").kind(), &ParseErrorKind::InvalidRepeat);
        assert_eq!(err("a{,3}").kind(), &ParseErrorKind::InvalidRepeat);
        assert_eq!(err("a{2").kind(), &ParseErrorKind::InvalidRepeat);
        assert_eq!(
            err("a{99999999999}").kind(),
            &ParseErrorKind::RepeatTooLarge
        );
        assert_eq!(err("[a-").kind(), &ParseErrorKind::UnclosedClass);
        assert_eq!(err("[").kind(), &ParseErrorKind::UnclosedClass);
        assert_eq!(err("(a|b").kind(), &ParseErrorKind::UnclosedGroup);
        assert_eq!(err(r"\q").kind(), &ParseErrorKind::UnknownEscape('q'));
        assert_eq!(err(r"\1").kind(), &ParseErrorKind::UnknownEscape('1'));
        assert_eq!(err(r"[\D]").kind(), &ParseErrorKind::UnsupportedEscape('D'));
        assert_eq!(err(r"[a-\d]").kind(), &ParseErrorKind::InvalidClassRange);
    }

    #[test]
    fn deep_nesting_is_rejected_not_overflowed() {
        let pattern = "(".repeat(10_000);
        assert_eq!(err(&pattern).kind(), &ParseErrorKind::NestingTooDeep);
    }

    #[test]
    fn nesting_just_under_the_limit_is_fine() {
        let n = MAX_DEPTH as usize;
        let pattern = format!("{}a{}", "(".repeat(n), ")".repeat(n));
        assert_eq!(ok(&pattern), lit('a'));
    }

    #[test]
    fn display_includes_location() {
        assert_eq!(
            err("a**").to_string(),
            "quantifier cannot follow another quantifier (at bytes 2..3)"
        );
    }
}
