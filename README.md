# Regex NFA Engine
A toy non-deterministic finite automaton (NFA) based regular expression matcher built in Rust. I focused on the process of
understanding how languages are built and interpreted in code.

## Features
- Supports literal characters (`a`–`z`, `A`–`Z`, `0`–`9`)
- Concatenation
- Alternation (`|`)
- Kleene star (`*`), plus (`+`), and optional (`?`) modifiers
- Grouping with parentheses

## Project Structure
```text
src/
├── main.rs         // Example usage
├── lexer.rs        // Tokenizes the pattern into RegexToken
├── parser.rs       // Builds a RegexAST from tokens using defined grammar
├── nfa_builder.rs  // Constructs the NFA from the AST
└── matcher.rs      // Executes the NFA matching algorithm
```

## Examples
Matching `(a|b)*c?d+` against:

- `aacd` → `true`
- `bddd` → `true`
- `d`    → `true`
- `abb`  → `false`
- `ab`   → `false`
- `ac`   → `false`
