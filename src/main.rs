use std::cell::RefCell;
use std::collections::HashMap;
use std::process::exit;
use std::rc::Rc;

struct State {
    id: i16,
    is_accepting: bool,
    transitions: HashMap<char, StatePtr>,
    e_transitions: Vec<StatePtr>,
}

impl State {
    fn new(id: i16) -> Self {
        Self {
            id,
            is_accepting: false,
            transitions: HashMap::new(),
            e_transitions: Vec::new(),
        }
    }
}

struct StateModule {
    input: StatePtr,
    output: StatePtr,
}

type StatePtr = Rc<RefCell<State>>;

fn construct_const_nfa(c: char, id_counter: &mut i16) -> StateModule {
    let input = Rc::new(RefCell::new(State::new(*id_counter)));
    let output = Rc::new(RefCell::new(State::new(*id_counter + 1)));
    *id_counter += 1;

    input.borrow_mut().transitions.insert(c, Rc::clone(&output));
    StateModule { input, output }
}

fn construct_kleene_plus_nfa(c: char, id_counter: &mut i16) -> StateModule {
    let input = Rc::new(RefCell::new(State::new(*id_counter)));
    let output = Rc::new(RefCell::new(State::new(*id_counter + 1)));
    *id_counter += 1;

    input.borrow_mut().transitions.insert(c, Rc::clone(&output));
    output.borrow_mut().e_transitions.push(Rc::clone(&input));

    StateModule { input, output }
}

fn construct_nfa(regex: &str) -> StatePtr {
    let init_state = Rc::new(RefCell::new(State::new(0)));
    let mut last_state = Rc::clone(&init_state);

    let mut id_counter = 1;
    let mut chars = regex.chars().peekable();

    while let Some(c) = chars.next() {
        let state_mod = match chars.peek() {
            Some(&'+') => {
                chars.next();
                construct_kleene_plus_nfa(c, &mut id_counter)
            }
            _ if c.is_alphabetic() => construct_const_nfa(c, &mut id_counter),
            _ => {
                println!("Error evaluating regex character '{}'", c);
                exit(1);
            }
        };

        id_counter += 1;

        last_state
            .borrow_mut()
            .e_transitions
            .push(Rc::clone(&state_mod.input));

        last_state = Rc::clone(&state_mod.output);
    }

    let accept_state = Rc::new(RefCell::new(State::new(id_counter)));
    accept_state.borrow_mut().is_accepting = true;
    last_state
        .borrow_mut()
        .e_transitions
        .push(Rc::clone(&accept_state));

    Rc::clone(&init_state)
}

fn find_e_closure(state: StatePtr, closure: &mut Vec<StatePtr>) -> &mut Vec<StatePtr> {
    if !closure.iter().any(|s| Rc::ptr_eq(s, &state)) {
        closure.push(Rc::clone(&state));
    }

    for e_state in state.borrow().e_transitions.iter() {
        if !closure.iter().any(|s| Rc::ptr_eq(s, &e_state)) {
            closure.push(Rc::clone(e_state));
            find_e_closure(Rc::clone(&e_state), closure);
        }
    }

    closure
}

fn match_regex(nfa: StatePtr, input: &str) -> bool {
    let mut current_states = find_e_closure(Rc::clone(&nfa), &mut Vec::new()).to_vec();

    for c in input.chars() {
        let mut next_states = Vec::new();

        for state in current_states.iter() {
            let state_borrow = state.borrow();

            if let Some(target_state) = state_borrow.transitions.get(&c) {
                find_e_closure(Rc::clone(&target_state), &mut next_states);
            }
        }

        current_states = next_states;
    }

    current_states.iter().any(|s| s.borrow().is_accepting)
}

fn main() {
    let regex = "aaba+b";
    let input = "aabaaaaaab";
    let init_state = construct_nfa(regex);
    println!("{}", match_regex(Rc::clone(&init_state), input))
}
