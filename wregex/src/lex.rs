use std::fmt;
use std::iter::FusedIterator;
use std::ops::Range;
use std::str::CharIndices;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Token {
    Literal(char),
    Escaped(char),

    Plus,
    Star,
    QuestionMark,
    LeftBrace,
    RightBrace,
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    Caret,
    DollarSign,
    Period,
    Pipe,
    Dash,
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Literal(c) => write!(f, "{c}"),
            Self::Escaped(c) => write!(f, "\\{c}"),

            Self::Plus => f.write_str("+"),
            Self::Star => f.write_str("*"),
            Self::QuestionMark => f.write_str("?"),
            Self::LeftBrace => f.write_str("{"),
            Self::RightBrace => f.write_str("}"),
            Self::LeftParen => f.write_str("("),
            Self::RightParen => f.write_str(")"),
            Self::LeftBracket => f.write_str("["),
            Self::RightBracket => f.write_str("]"),
            Self::Caret => f.write_str("^"),
            Self::DollarSign => f.write_str("$"),
            Self::Period => f.write_str("."),
            Self::Pipe => f.write_str("|"),
            Self::Dash => f.write_str("-"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    #[must_use]
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    #[must_use]
    pub const fn len(self) -> usize {
        self.end - self.start
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }

    #[must_use]
    pub const fn range(self) -> Range<usize> {
        self.start..self.end
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Spanned {
    pub token: Token,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LexError {
    TrailingBackslash { pos: usize },
}

impl LexError {
    #[must_use]
    pub const fn span(&self) -> Span {
        match self {
            Self::TrailingBackslash { pos } => Span::new(*pos, *pos + 1),
        }
    }
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LexError::TrailingBackslash { pos } => {
                write!(f, "pattern ends with an unescaped backslash at byte {pos}")
            }
        }
    }
}

impl std::error::Error for LexError {}

pub struct Lexer<'a> {
    chars: CharIndices<'a>,
}

impl<'a> Lexer<'a> {
    #[must_use]
    pub fn new(pattern: &'a str) -> Self {
        Self {
            chars: pattern.char_indices(),
        }
    }
}

impl Iterator for Lexer<'_> {
    type Item = Result<Spanned, LexError>;

    fn next(&mut self) -> Option<Self::Item> {
        let (start, c) = self.chars.next()?;
        let token = match c {
            '\\' => match self.chars.next() {
                Some((_, escaped)) => Token::Escaped(escaped),
                None => return Some(Err(LexError::TrailingBackslash { pos: start })),
            },

            '+' => Token::Plus,
            '*' => Token::Star,
            '?' => Token::QuestionMark,
            '(' => Token::LeftParen,
            ')' => Token::RightParen,
            '{' => Token::LeftBrace,
            '}' => Token::RightBrace,
            '[' => Token::LeftBracket,
            ']' => Token::RightBracket,
            '^' => Token::Caret,
            '$' => Token::DollarSign,
            '.' => Token::Period,
            '|' => Token::Pipe,
            '-' => Token::Dash,

            other => Token::Literal(other),
        };

        let span = Span::new(start, self.chars.offset());
        Some(Ok(Spanned { token, span }))
    }
}

impl FusedIterator for Lexer<'_> {}

pub fn tokenize(pattern: &str) -> Result<Vec<Spanned>, LexError> {
    Lexer::new(pattern).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(pattern: &str) -> Vec<Token> {
        tokenize(pattern)
            .unwrap()
            .into_iter()
            .map(|t| t.token)
            .collect()
    }

    #[test]
    fn empty_pattern() {
        assert_eq!(tokenize("").unwrap(), vec![]);
    }

    #[test]
    fn group_with_quantifiers() {
        assert_eq!(
            kinds("abc(d*)+"),
            [
                Token::Literal('a'),
                Token::Literal('b'),
                Token::Literal('c'),
                Token::LeftParen,
                Token::Literal('d'),
                Token::Star,
                Token::RightParen,
                Token::Plus,
            ]
        );
    }

    #[test]
    fn every_symbol() {
        let cases = [
            ("+", Token::Plus),
            ("*", Token::Star),
            ("?", Token::QuestionMark),
            ("{", Token::LeftBrace),
            ("}", Token::RightBrace),
            ("(", Token::LeftParen),
            (")", Token::RightParen),
            ("[", Token::LeftBracket),
            ("]", Token::RightBracket),
            ("^", Token::Caret),
            ("$", Token::DollarSign),
            (".", Token::Period),
            ("|", Token::Pipe),
            ("-", Token::Dash),
        ];
        for (input, expected) in cases {
            assert_eq!(kinds(input), [expected], "input: {input:?}");
        }
    }

    #[test]
    fn escapes() {
        assert_eq!(
            kinds(r"a\+\d\\"),
            [
                Token::Literal('a'),
                Token::Escaped('+'),
                Token::Escaped('d'),
                Token::Escaped('\\'),
            ]
        );
    }

    #[test]
    fn spaces_and_punctuation_are_literals() {
        assert_eq!(
            kinds("a @_"),
            [
                Token::Literal('a'),
                Token::Literal(' '),
                Token::Literal('@'),
                Token::Literal('_'),
            ]
        );
    }

    #[test]
    fn spans_are_byte_offsets() {
        let tokens = tokenize("é+").unwrap();
        assert_eq!(tokens[0].span, Span::new(0, 2));
        assert_eq!(tokens[1].span, Span::new(2, 3));
    }

    #[test]
    fn escape_span_covers_backslash_and_char() {
        let tokens = tokenize("\\é").unwrap();
        assert_eq!(tokens[0].span, Span::new(0, 3));
    }

    #[test]
    fn trailing_backslash() {
        let err = tokenize(r"ab\").unwrap_err();
        assert_eq!(err, LexError::TrailingBackslash { pos: 2 });
        assert_eq!(err.span(), Span::new(2, 3));
        assert_eq!(
            err.to_string(),
            "pattern ends with an unescaped backslash at byte 2"
        );
    }

    #[test]
    fn escaped_backslash_at_end_is_fine() {
        assert_eq!(kinds(r"a\\"), [Token::Literal('a'), Token::Escaped('\\')]);
    }

    #[test]
    fn lexer_is_lazy_and_fused() {
        let mut lexer = Lexer::new("a");
        assert!(lexer.next().is_some());
        assert!(lexer.next().is_none());
        assert!(lexer.next().is_none());
    }

    #[test]
    fn display_round_trips() {
        let pattern = r"a\+(b|c)*[^x-z]{2}$";
        let rebuilt: String = tokenize(pattern)
            .unwrap()
            .iter()
            .map(|t| t.token.to_string())
            .collect();

        assert_eq!(rebuilt, pattern);
    }
}
