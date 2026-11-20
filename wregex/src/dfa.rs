use std::collections::{BTreeSet, HashMap};
use std::fmt;

use crate::ast::{Assertion, ClassItem};
use crate::nfa::{Nfa, State, StateId};

const MAX_STATES: usize = 10_000;
const CHAR_LIMIT: u32 = 0x11_0000;
const SURROGATE_START: u32 = 0xD800;
const SURROGATE_END: u32 = 0xE000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Full,
    Search,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DfaError {
    TooManyStates { limit: usize },
}

impl fmt::Display for DfaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyStates { limit } => {
                write!(f, "automaton needs more than {limit} states")
            }
        }
    }
}

impl std::error::Error for DfaError {}

#[derive(Debug, Clone)]
pub struct Dfa {
    mode: Mode,
    starts: Vec<u32>,
    stride: usize,
    trans: Vec<usize>,
    accepting: Vec<bool>,
    accepting_at_end: Vec<bool>,
    dead: Vec<bool>,
}

impl Dfa {
    pub fn new(nfa: &Nfa, mode: Mode) -> Result<Self, DfaError> {
        let starts = alphabet_starts(nfa);
        let reps = representatives(&starts);
        let stride = starts.len();
        let search = mode == Mode::Search;

        let start_set = closure(nfa, &[nfa.start], true, false);
        let mut sets: Vec<Vec<StateId>> = vec![start_set];
        let mut map: HashMap<Vec<StateId>, usize> = HashMap::new();
        let mut trans: Vec<usize> = Vec::new();

        let mut i = 0;
        while i < sets.len() {
            let current = sets[i].clone();
            for rep in &reps {
                let target = match rep {
                    None => Vec::new(),
                    Some(c) => {
                        let mut seeds: Vec<StateId> =
                            current.iter().filter_map(|&id| nfa.step(id, *c)).collect();
                        if search {
                            seeds.push(nfa.start);
                        }

                        closure(nfa, &seeds, false, false)
                    }
                };

                trans.push(intern(&mut map, &mut sets, target)?);
            }

            i += 1;
        }

        let accepting = sets.iter().map(|s| contains(s, nfa.accept)).collect();
        let accepting_at_end = sets
            .iter()
            .enumerate()
            .map(|(i, s)| contains(&closure(nfa, s, i == 0, true), nfa.accept))
            .collect();
        let dead = sets.iter().map(Vec::is_empty).collect();

        Ok(Self {
            mode,
            starts,
            stride,
            trans,
            accepting,
            accepting_at_end,
            dead,
        })
    }

    #[must_use]
    pub fn state_count(&self) -> usize {
        self.accepting.len()
    }

    #[must_use]
    pub fn class_count(&self) -> usize {
        self.stride
    }

    #[must_use]
    pub fn is_match(&self, text: &str) -> bool {
        let search = self.mode == Mode::Search;
        let mut state = 0;

        for c in text.chars() {
            if search && self.accepting[state] {
                return true;
            }
            if self.dead[state] {
                return false;
            }
            state = self.trans[state * self.stride + self.class_of(c)];
        }

        self.accepting_at_end[state]
    }

    fn class_of(&self, c: char) -> usize {
        self.starts.partition_point(|&s| s <= u32::from(c)) - 1
    }
}

fn intern(
    map: &mut HashMap<Vec<StateId>, usize>,
    sets: &mut Vec<Vec<StateId>>,
    set: Vec<StateId>,
) -> Result<usize, DfaError> {
    if let Some(&idx) = map.get(&set) {
        return Ok(idx);
    }

    if sets.len() >= MAX_STATES {
        return Err(DfaError::TooManyStates { limit: MAX_STATES });
    }

    let idx = sets.len();
    sets.push(set.clone());
    map.insert(set, idx);
    Ok(idx)
}

fn contains(set: &[StateId], id: StateId) -> bool {
    set.binary_search(&id).is_ok()
}

fn closure(nfa: &Nfa, seeds: &[StateId], at_start: bool, at_end: bool) -> Vec<StateId> {
    let mut seen = vec![false; nfa.states.len()];
    let mut stack = seeds.to_vec();
    let mut out = Vec::new();

    while let Some(id) = stack.pop() {
        if seen[id] {
            continue;
        }
        seen[id] = true;

        match &nfa.states[id] {
            State::Split { left, right } => {
                stack.push(*right);
                stack.push(*left);
            }
            State::Assert { kind, next } => {
                let holds = match kind {
                    Assertion::Start => at_start,
                    Assertion::End => at_end,
                };
                if holds {
                    stack.push(*next);
                }
                if *kind == Assertion::End {
                    out.push(id);
                }
            }
            State::Char { .. } | State::Any { .. } | State::Class { .. } | State::Match => {
                out.push(id);
            }
        }
    }

    out.sort_unstable();
    out
}

fn alphabet_starts(nfa: &Nfa) -> Vec<u32> {
    let mut points = BTreeSet::new();
    points.insert(0);

    let mut add = |p: u32| {
        if p < CHAR_LIMIT {
            points.insert(p);
        }
    };

    for state in &nfa.states {
        match state {
            State::Char { c, .. } => {
                add(u32::from(*c));
                add(u32::from(*c) + 1);
            }
            State::Any { .. } => {
                add(u32::from('\n'));
                add(u32::from('\n') + 1);
            }
            State::Class { class, .. } => {
                for item in &class.items {
                    match *item {
                        ClassItem::Char(c) => {
                            add(u32::from(c));
                            add(u32::from(c) + 1);
                        }
                        ClassItem::Range(lo, hi) => {
                            add(u32::from(lo));
                            add(u32::from(hi) + 1);
                        }
                    }
                }
            }
            State::Split { .. } | State::Assert { .. } | State::Match => {}
        }
    }

    points.into_iter().collect()
}

fn representatives(starts: &[u32]) -> Vec<Option<char>> {
    starts
        .iter()
        .enumerate()
        .map(|(i, &start)| {
            let end = starts.get(i + 1).copied().unwrap_or(CHAR_LIMIT);
            if (SURROGATE_START..SURROGATE_END).contains(&start) {
                if end > SURROGATE_END {
                    char::from_u32(SURROGATE_END)
                } else {
                    None
                }
            } else {
                char::from_u32(start)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_str;

    fn nfa(pattern: &str) -> Nfa {
        Nfa::from_ast(&parse_str(pattern).unwrap()).unwrap()
    }

    fn strings(alphabet: &[char], max_len: usize) -> Vec<String> {
        let mut all = vec![String::new()];
        let mut frontier = vec![String::new()];
        for _ in 0..max_len {
            let mut next = Vec::new();
            for s in &frontier {
                for &c in alphabet {
                    let mut t = s.clone();
                    t.push(c);
                    next.push(t);
                }
            }
            all.extend(next.iter().cloned());
            frontier = next;
        }

        all
    }

    const PATTERNS: &[&str] = &[
        "",
        "a",
        "abc",
        "a*",
        "a+",
        "a?",
        "a{2,3}",
        "a{2,}",
        "(a|b)*c",
        "(ab|ba)+",
        "[a-b]+c",
        "[^a]b",
        "[^a-b]+",
        ".",
        "a.c",
        "^a",
        "a$",
        "^a*$",
        "^(a|b)+$",
        "(a*)*",
        "(a|)b",
        "^",
        "$",
        "^$",
        "$^",
        "x|^a",
        "a$|b",
        "é+",
        r"\n",
        "(a?){3}a{3}",
    ];

    #[test]
    fn agrees_with_nfa_on_full_matches() {
        let inputs = strings(&['a', 'b', 'c', '\n', 'é'], 5);
        for pattern in PATTERNS {
            let n = nfa(pattern);
            let d = Dfa::new(&n, Mode::Full).unwrap();
            for s in &inputs {
                assert_eq!(
                    d.is_match(s),
                    n.is_full_match(s),
                    "pattern {pattern:?} input {s:?}"
                );
            }
        }
    }

    #[test]
    fn agrees_with_nfa_on_searches() {
        let inputs = strings(&['a', 'b', 'c', '\n', 'é'], 5);
        for pattern in PATTERNS {
            let n = nfa(pattern);
            let d = Dfa::new(&n, Mode::Search).unwrap();
            for s in &inputs {
                assert_eq!(
                    d.is_match(s),
                    n.is_match(s),
                    "pattern {pattern:?} input {s:?}"
                );
            }
        }
    }

    #[test]
    fn class_upper_bound_is_inclusive() {
        let d = Dfa::new(&nfa("[a-c]"), Mode::Full).unwrap();
        assert!(d.is_match("a") && d.is_match("b") && d.is_match("c"));
        assert!(!d.is_match("d") && !d.is_match("`"));
    }

    #[test]
    fn unicode_and_high_code_points() {
        let d = Dfa::new(&nfa("[^a]+"), Mode::Full).unwrap();
        assert!(d.is_match("é"));
        assert!(d.is_match("\u{10FFFF}"));
        assert!(d.is_match("\u{D7FF}\u{E000}"));
        assert!(!d.is_match("a"));
    }

    #[test]
    fn small_pattern_has_few_states() {
        let d = Dfa::new(&nfa("abc"), Mode::Full).unwrap();
        assert!(d.state_count() <= 6);
    }

    #[test]
    fn state_explosion_is_rejected() {
        let n = nfa("(a|b)*a(a|b){15}");
        assert!(matches!(
            Dfa::new(&n, Mode::Full),
            Err(DfaError::TooManyStates { .. })
        ));
    }
}
