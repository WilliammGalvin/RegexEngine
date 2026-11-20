#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Ast {
    Empty,
    Literal(char),
    Any,
    Class(Class),
    Assertion(Assertion),
    Concat(Vec<Ast>),
    Alternate(Vec<Ast>),
    Repeat(Repeat),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Assertion {
    Start,
    End,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repeat {
    pub node: Box<Ast>,
    pub min: u32,
    pub max: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Class {
    pub negated: bool,
    pub items: Vec<ClassItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassItem {
    Char(char),
    Range(char, char),
}

impl Class {
    #[must_use]
    pub fn matches(&self, c: char) -> bool {
        let found = self.items.iter().any(|item| match *item {
            ClassItem::Char(x) => x == c,
            ClassItem::Range(lo, hi) => (lo..hi).contains(&c),
        });

        found != self.negated
    }
}
