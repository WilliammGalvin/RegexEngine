use crate::lexer::tokenize_regex;
use crate::matcher::match_regex;
use crate::nfa_builder::build_nfa;
use crate::parser::build_ast;

mod lexer;
mod parser;
mod nfa_builder;
mod matcher;

fn main() {
    let pattern = "(a|b)*c?d+";
    let tokens = tokenize_regex(pattern);
    let ast = build_ast(&tokens);
    let nfa = build_nfa(&ast);

    let inputs = vec![
        "aacd", // True
        "bddd", // True
        "d", // True
        "abb", // False
        "ab", // False
        "ac", // False
    ];

    for input in inputs {
        let is_match = match_regex(nfa.get_start_state(), input);
        println!("{}: {}", input, is_match)
    }
}