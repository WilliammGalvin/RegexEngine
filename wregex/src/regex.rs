use std::fmt;

use crate::ast::Ast;
use crate::dfa::{Dfa, DfaError, Mode};
use crate::nfa::{BuildError, Nfa};
use crate::parse::{ParseError, parse_str};

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    Parse(ParseError),
    Build(BuildError),
    Dfa(DfaError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(e) => e.fmt(f),
            Self::Build(e) => e.fmt(f),
            Self::Dfa(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for Error {}

impl From<ParseError> for Error {
    fn from(e: ParseError) -> Self {
        Self::Parse(e)
    }
}

impl From<BuildError> for Error {
    fn from(e: BuildError) -> Self {
        Self::Build(e)
    }
}

impl From<DfaError> for Error {
    fn from(e: DfaError) -> Self {
        Self::Dfa(e)
    }
}

#[derive(Debug, Clone)]
pub struct Regex {
    nfa: Nfa,
    full: Dfa,
    search: Dfa,
}

impl Regex {
    pub fn new(pattern: &str) -> Result<Self, Error> {
        let ast: Ast = parse_str(pattern)?;
        let nfa = Nfa::from_ast(&ast)?;
        let full = Dfa::new(&nfa, Mode::Full)?;
        let search = Dfa::new(&nfa, Mode::Search)?;
        Ok(Self { nfa, full, search })
    }

    #[must_use]
    pub fn is_match(&self, text: &str) -> bool {
        self.search.is_match(text)
    }

    #[must_use]
    pub fn is_full_match(&self, text: &str) -> bool {
        self.full.is_match(text)
    }
}
