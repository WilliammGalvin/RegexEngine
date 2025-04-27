use crate::nfa_builder::StatePtr;

use std::rc::Rc;

fn find_e_closure(state: StatePtr, closure: &mut Vec<StatePtr>) -> &mut Vec<StatePtr> {
    if !closure.iter().any(|s| Rc::ptr_eq(s, &state)) {
        closure.push(Rc::clone(&state));
    }

    for e_state in state.borrow().get_e_transitions().iter() {
        if !closure.iter().any(|s| Rc::ptr_eq(s, &e_state)) {
            closure.push(Rc::clone(e_state));
            find_e_closure(Rc::clone(&e_state), closure);
        }
    }

    closure
}

pub fn match_regex(nfa: StatePtr, input: &str) -> bool {
    let mut current_states = find_e_closure(Rc::clone(&nfa), &mut Vec::new()).to_vec();

    for c in input.chars() {
        let mut next_states = Vec::new();

        for state in current_states.iter() {
            let state_borrow = state.borrow();

            if let Some(target_state) = state_borrow.get_transitions().get(&c) {
                find_e_closure(Rc::clone(&target_state), &mut next_states);
            }
        }

        current_states = next_states;
    }

    current_states.iter().any(|s| s.borrow().is_accepting())
}
