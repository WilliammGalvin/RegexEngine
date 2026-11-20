use std::fmt;
use std::mem;

use crate::ast::{Assertion, Ast, Class, Repeat};

const MAX_WORK: usize = 100_000;

type StateId = usize;

#[derive(Debug, Clone)]
enum State {
    Char { c: char, next: StateId },
    Any { next: StateId },
    Class { class: Class, next: StateId },
    Split { left: StateId, right: StateId },
    Assert { kind: Assertion, next: StateId },
    Match,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum BuildError {
    TooLarge { limit: usize },
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge { limit } => {
                write!(f, "pattern is too large to compile (limit {limit})")
            }
        }
    }
}

impl std::error::Error for BuildError {}

#[derive(Debug, Clone)]
pub struct Nfa {
    states: Vec<State>,
    start: StateId,
    accept: StateId,
}

impl Nfa {
    pub fn from_ast(ast: &Ast) -> Result<Self, BuildError> {
        let mut builder = Builder {
            states: Vec::new(),
            budget: MAX_WORK,
        };

        let accept = builder.push(State::Match);
        let start = builder.compile(ast, accept)?;
        Ok(Self {
            states: builder.states,
            start,
            accept,
        })
    }

    #[must_use]
    pub fn state_count(&self) -> usize {
        self.states.len()
    }

    #[must_use]
    pub fn is_match(&self, text: &str) -> bool {
        self.run(text, true)
    }

    #[must_use]
    pub fn is_full_match(&self, text: &str) -> bool {
        self.run(text, false)
    }

    fn run(&self, text: &str, unanchored: bool) -> bool {
        let len = text.len();
        let mut cur = StateSet::new(self.states.len());
        let mut next = StateSet::new(self.states.len());
        let mut stack = Vec::new();
        let mut pos = 0;

        self.add(&mut cur, self.start, pos, len, &mut stack);
        let mut chars = text.chars();

        loop {
            if unanchored && cur.contains(self.accept) {
                return true;
            }

            let Some(c) = chars.next() else {
                return cur.contains(self.accept);
            };

            if !unanchored && cur.is_empty() {
                return false;
            }

            let new_pos = pos + c.len_utf8();
            next.clear();
            for &id in cur.ids() {
                if let Some(target) = self.step(id, c) {
                    self.add(&mut next, target, new_pos, len, &mut stack);
                }
            }

            if unanchored {
                self.add(&mut next, self.start, new_pos, len, &mut stack);
            }

            pos = new_pos;
            mem::swap(&mut cur, &mut next);
        }
    }

    fn step(&self, id: StateId, c: char) -> Option<StateId> {
        match &self.states[id] {
            State::Char { c: want, next } if *want == c => Some(*next),
            State::Any { next } if c != '\n' => Some(*next),
            State::Class { class, next } if class.matches(c) => Some(*next),
            _ => None,
        }
    }

    fn add(
        &self,
        set: &mut StateSet,
        start: StateId,
        pos: usize,
        len: usize,
        stack: &mut Vec<StateId>,
    ) {
        stack.push(start);
        while let Some(id) = stack.pop() {
            if !set.insert(id) {
                continue;
            }

            match &self.states[id] {
                State::Split { left, right } => {
                    stack.push(*right);
                    stack.push(*left);
                }
                State::Assert { kind, next } if holds(*kind, pos, len) => {
                    stack.push(*next);
                }
                _ => {}
            }
        }
    }
}

fn holds(kind: Assertion, pos: usize, len: usize) -> bool {
    match kind {
        Assertion::Start => pos == 0,
        Assertion::End => pos == len,
    }
}

struct StateSet {
    list: Vec<StateId>,
    seen: Vec<bool>,
}

impl StateSet {
    fn new(capacity: usize) -> Self {
        Self {
            list: Vec::with_capacity(capacity),
            seen: vec![false; capacity],
        }
    }

    fn insert(&mut self, id: StateId) -> bool {
        if self.seen[id] {
            return false;
        }

        self.seen[id] = true;
        self.list.push(id);
        true
    }

    fn contains(&self, id: StateId) -> bool {
        self.seen[id]
    }

    fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    fn ids(&self) -> &[StateId] {
        &self.list
    }

    fn clear(&mut self) {
        for &id in &self.list {
            self.seen[id] = false;
        }

        self.list.clear();
    }
}

struct Builder {
    states: Vec<State>,
    budget: usize,
}

impl Builder {
    fn push(&mut self, state: State) -> StateId {
        self.states.push(state);
        self.states.len() - 1
    }

    fn compile(&mut self, ast: &Ast, next: StateId) -> Result<StateId, BuildError> {
        self.budget = self
            .budget
            .checked_sub(1)
            .ok_or(BuildError::TooLarge { limit: MAX_WORK })?;

        Ok(match ast {
            Ast::Empty => next,
            Ast::Literal(c) => self.push(State::Char { c: *c, next }),
            Ast::Any => self.push(State::Any { next }),

            Ast::Class(class) => self.push(State::Class {
                class: class.clone(),
                next,
            }),

            Ast::Assertion(kind) => self.push(State::Assert { kind: *kind, next }),

            Ast::Concat(items) => {
                let mut entry = next;
                for item in items.iter().rev() {
                    entry = self.compile(item, entry)?;
                }
                entry
            }

            Ast::Alternate(branches) => {
                let mut starts = Vec::with_capacity(branches.len());
                for branch in branches {
                    starts.push(self.compile(branch, next)?);
                }

                let mut rev = starts.into_iter().rev();
                match rev.next() {
                    None => next,
                    Some(last) => {
                        rev.fold(last, |right, left| self.push(State::Split { left, right }))
                    }
                }
            }

            Ast::Repeat(rep) => self.compile_repeat(rep, next)?,
        })
    }

    fn compile_repeat(&mut self, rep: &Repeat, next: StateId) -> Result<StateId, BuildError> {
        debug_assert!(rep.max.is_none_or(|max| rep.min <= max));

        let mut entry = match rep.max {
            None => {
                let split = self.push(State::Split {
                    left: next,
                    right: next,
                });
                let body = self.compile(&rep.node, split)?;
                self.states[split] = State::Split {
                    left: body,
                    right: next,
                };
                split
            }
            Some(max) => {
                let mut entry = next;
                for _ in rep.min..max {
                    let body = self.compile(&rep.node, entry)?;
                    entry = self.push(State::Split {
                        left: body,
                        right: next,
                    });
                }
                entry
            }
        };

        for _ in 0..rep.min {
            entry = self.compile(&rep.node, entry)?;
        }

        Ok(entry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::ClassItem;
    use crate::parse::parse_str;

    fn lit(s: &str) -> Ast {
        Ast::Concat(s.chars().map(Ast::Literal).collect())
    }

    fn rep(node: Ast, min: u32, max: Option<u32>) -> Ast {
        Ast::Repeat(Repeat {
            node: Box::new(node),
            min,
            max,
        })
    }

    fn nfa(ast: &Ast) -> Nfa {
        Nfa::from_ast(ast).unwrap()
    }

    fn compiled(pattern: &str) -> Nfa {
        nfa(&parse_str(pattern).unwrap())
    }

    fn full(pattern: &str, text: &str) -> bool {
        compiled(pattern).is_full_match(text)
    }

    #[test]
    fn literal_sequence() {
        let n = nfa(&lit("abc"));
        assert!(n.is_full_match("abc"));
        assert!(!n.is_full_match("ab"));
        assert!(!n.is_full_match("abcd"));
    }

    #[test]
    fn empty_pattern_matches_empty_string() {
        let n = nfa(&Ast::Empty);
        assert!(n.is_full_match(""));
        assert!(!n.is_full_match("a"));
        assert!(n.is_match("anything"));
    }

    #[test]
    fn alternation() {
        let n = nfa(&Ast::Alternate(vec![lit("cat"), lit("dog"), lit("")]));
        assert!(n.is_full_match("cat"));
        assert!(n.is_full_match("dog"));
        assert!(n.is_full_match(""));
        assert!(!n.is_full_match("cow"));
    }

    #[test]
    fn star_plus_question() {
        let a = || Ast::Literal('a');
        let star = nfa(&rep(a(), 0, None));
        let plus = nfa(&rep(a(), 1, None));
        let opt = nfa(&rep(a(), 0, Some(1)));

        assert!(star.is_full_match("") && star.is_full_match("aaaa"));
        assert!(!plus.is_full_match("") && plus.is_full_match("aaaa"));
        assert!(opt.is_full_match("") && opt.is_full_match("a") && !opt.is_full_match("aa"));
    }

    #[test]
    fn bounded_repeat() {
        let n = nfa(&rep(Ast::Literal('a'), 2, Some(3)));
        assert!(!n.is_full_match("a"));
        assert!(n.is_full_match("aa"));
        assert!(n.is_full_match("aaa"));
        assert!(!n.is_full_match("aaaa"));
    }

    #[test]
    fn nested_empty_loop_terminates() {
        let n = nfa(&rep(rep(Ast::Literal('a'), 0, None), 0, None));
        assert!(n.is_full_match("aaa"));
        assert!(!n.is_full_match("b"));
    }

    #[test]
    fn classes() {
        let class = |negated| {
            Ast::Class(Class {
                negated,
                items: vec![ClassItem::Range('a', 'c'), ClassItem::Char('z')],
            })
        };
        let pos = nfa(&class(false));
        let neg = nfa(&class(true));
        assert!(pos.is_full_match("b") && pos.is_full_match("z") && !pos.is_full_match("d"));
        assert!(neg.is_full_match("d") && !neg.is_full_match("b"));
    }

    #[test]
    fn dot_excludes_newline() {
        let n = nfa(&Ast::Any);
        assert!(n.is_full_match("x"));
        assert!(n.is_full_match("é"));
        assert!(!n.is_full_match("\n"));
    }

    #[test]
    fn anchors() {
        let n = compiled("^ab$");
        assert!(n.is_match("ab"));
        assert!(!n.is_match("xab"));
        assert!(!n.is_match("abx"));
    }

    #[test]
    fn unanchored_search_finds_inner_matches() {
        let n = compiled("b+c");
        assert!(n.is_match("aabbbcdd"));
        assert!(!n.is_match("aabbdd"));
    }

    #[test]
    fn end_to_end_patterns() {
        assert!(full("abc(d*)+", "abcddd"));
        assert!(!full("abc(d*)+", "abd"));
        assert!(full("colou?r", "color") && full("colou?r", "colour"));
        assert!(full("[a-c]+x", "abcax") && !full("[a-c]+x", "abdx"));
        assert!(full("a{2,3}", "aa") && full("a{2,3}", "aaa"));
        assert!(!full("a{2,3}", "a") && !full("a{2,3}", "aaaa"));
        assert!(full(r"a\.b", "a.b") && !full(r"a\.b", "axb"));
        assert!(full(r"\d+-\d+", "10-200"));
        assert!(full(r"[\w.]+", "foo.bar_1"));
        assert!(full("(ab|cd)*", "abcdab"));
        assert!(full("", ""));
    }

    #[test]
    fn unicode_text() {
        assert!(full("é+", "ééé"));
        assert!(full("a.c", "aéc"));
        assert!(full("[^a]", "é"));
    }

    #[test]
    fn huge_repetition_is_rejected() {
        let a = Ast::Literal('a');
        let ast = rep(
            rep(rep(a, 1000, Some(1000)), 1000, Some(1000)),
            1000,
            Some(1000),
        );
        assert!(matches!(
            Nfa::from_ast(&ast),
            Err(BuildError::TooLarge { .. })
        ));
    }

    #[test]
    fn state_count_is_reasonable() {
        assert!(compiled("abc").state_count() <= 4);
    }
}
