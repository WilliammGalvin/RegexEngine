use crate::parser::RegexToken;

pub fn tokenize_regex(regex: &str) -> Vec<RegexToken> {
    let mut result: Vec<RegexToken> = Vec::new();

    for c in regex.chars() {
        let token = match c {
            '|' => RegexToken::Alternation,
            '*' => RegexToken::Star,
            '+' => RegexToken::Plus,
            '?' => RegexToken::Optional,
            '(' => RegexToken::OpenBracket,
            ')' => RegexToken::CloseBracket,
            _ if c.is_alphanumeric() => RegexToken::Const(c),
            _ => panic!("Unrecognized character: {:?}", c)
        };

        result.push(token);
    }

    result
}