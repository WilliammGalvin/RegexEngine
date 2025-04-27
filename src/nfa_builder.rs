use std::cell::{RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use crate::parser::{RegexAST, RegexASTModifier};

pub struct State {
    id: usize,
    transitions: HashMap<char, StatePtr>,
    e_transitions: Vec<StatePtr>,
    is_accepting: bool,
}

impl State {
    fn new(id: usize) -> Self {
        State {
            id,
            transitions: HashMap::new(),
            e_transitions: Vec::new(),
            is_accepting: false,
        }
    }
    
    pub fn get_id(&self) -> usize {
        self.id
    }

    pub fn get_transitions(&self) -> &HashMap<char, StatePtr> {
        &self.transitions
    }
    
    pub fn get_e_transitions(&self) -> &Vec<StatePtr> {
        &self.e_transitions
    }

    pub fn is_accepting(&self) -> bool {
        self.is_accepting
    }
}

pub struct StateMod {
    start: StatePtr,
    end: StatePtr,
}

impl StateMod {
    pub fn get_start_state(&self) -> StatePtr {
        Rc::clone(&self.start)
    }
}

pub type StatePtr = Rc<RefCell<State>>;

thread_local! {
    static STATE_ID: RefCell<usize> = RefCell::new(0);
}

fn new_state() -> StatePtr {
    let id = STATE_ID.with(|id| {
        let ret = *id.borrow();
        *id.borrow_mut() += 1;
        ret
    });
    
    Rc::new(RefCell::new(State::new(id)))
}

fn clone_state(state: &StatePtr) -> StatePtr {
    state.clone()
}

pub fn build_nfa(ast: &RegexAST) -> StateMod {
    let nfa = build_nfa_inner(ast);
    nfa.end.borrow_mut().is_accepting = true;
    nfa
}

fn build_nfa_inner(ast: &RegexAST) -> StateMod {
    match ast {
        RegexAST::Const(_) => build_const_nfa(ast),
        RegexAST::Concat(_, _) => build_concat_nfa(ast),
        RegexAST::Alternation(_, _) => build_alternation_nfa(ast),
        RegexAST::Modifier(_, _) => build_modifier_nfa(ast),
        RegexAST::Group(inner) => build_nfa_inner(inner),
    }
}

fn build_concat_nfa(ast: &RegexAST) -> StateMod {
    if let RegexAST::Concat(b1, b2) = ast {
        let nfa_1 = build_nfa_inner(b1);
        let nfa_2 = build_nfa_inner(b2);

        nfa_1.end.borrow_mut().e_transitions.push(clone_state(&nfa_2.start));

        StateMod {
            start: nfa_1.start,
            end: nfa_2.end,
        }
    } else {
        panic!("Expected a concat AST");
    }
}

fn build_alternation_nfa(ast: &RegexAST) -> StateMod {
    if let RegexAST::Alternation(b1, b2) = ast {
        let nfa_1 = build_nfa_inner(b1);
        let nfa_2 = build_nfa_inner(b2);
        let start = new_state();
        let end = new_state();

        start.borrow_mut().e_transitions.push(clone_state(&nfa_1.start));
        start.borrow_mut().e_transitions.push(clone_state(&nfa_2.start));
        nfa_1.end.borrow_mut().e_transitions.push(clone_state(&end));
        nfa_2.end.borrow_mut().e_transitions.push(clone_state(&end));

        StateMod { start, end }
    } else {
        panic!("Expected an alternation AST");
    }
}

fn build_modifier_nfa(ast: &RegexAST) -> StateMod {
    if let RegexAST::Modifier(base, modifier) = ast {
        let start = new_state();
        let end = new_state();

        let base_nfa = build_nfa_inner(base);

        start.borrow_mut().e_transitions.push(clone_state(&base_nfa.start));

        match modifier {
            RegexASTModifier::Star => {
                start.borrow_mut().e_transitions.push(clone_state(&end));
                base_nfa.end.borrow_mut().e_transitions.push(clone_state(&base_nfa.start));
                base_nfa.end.borrow_mut().e_transitions.push(clone_state(&end));
            }
            RegexASTModifier::Plus => {
                base_nfa.end.borrow_mut().e_transitions.push(clone_state(&base_nfa.start));
                base_nfa.end.borrow_mut().e_transitions.push(clone_state(&end));
            }
            RegexASTModifier::Optional => {
                start.borrow_mut().e_transitions.push(clone_state(&end));
                base_nfa.end.borrow_mut().e_transitions.push(clone_state(&end));
            }
        }

        StateMod { start, end }
    } else {
        panic!("Expected a modifier AST");
    }
}

fn build_const_nfa(ast: &RegexAST) -> StateMod {
    if let RegexAST::Const(c) = ast {
        let start = new_state();
        let end = new_state();

        start.borrow_mut().transitions.insert(*c, clone_state(&end));

        StateMod { start, end }
    } else {
        panic!("Expected a constant AST");
    }
}