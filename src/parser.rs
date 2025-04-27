#[derive(Debug, PartialEq)]
pub enum RegexToken {
    Const(char),
    Alternation,
    Star,
    Plus,
    Optional,
    OpenBracket,
    CloseBracket
}

pub enum RegexAST {
    Const(char),
    Concat(Box<RegexAST>, Box<RegexAST>),
    Alternation(Box<RegexAST>, Box<RegexAST>),
    Group(Box<RegexAST>),
    Modifier(Box<RegexAST>, RegexASTModifier),
}

pub enum RegexASTModifier {
    Star,
    Plus,
    Optional
}

fn build_expr(tokens: &[RegexToken], pos: &mut usize) -> RegexAST {
    let mut left = build_term(tokens, pos);
    
    while *pos < tokens.len() {
        match tokens[*pos] {
            RegexToken::Alternation => {
                *pos += 1;
                let right = build_term(tokens, pos);
                left = RegexAST::Alternation(Box::new(left), Box::new(right));
            },
            _ => break,
        }
    }
    
    left
}

fn build_term(tokens: &[RegexToken], pos: &mut usize) -> RegexAST {
    let mut left = build_factor(tokens, pos);
    
    while *pos < tokens.len() {
        match tokens[*pos] {
            RegexToken::Const(_) |
            RegexToken::OpenBracket => {
                let right = build_factor(tokens, pos);
                left = RegexAST::Concat(Box::new(left), Box::new(right));
            },
            _ => break,
        }
    }
    
    left
}

fn build_factor(tokens: &[RegexToken], pos: &mut usize) -> RegexAST {
    let base = build_base(tokens, pos);

    if *pos <= tokens.len() - 1 {
        match tokens[*pos] {
            RegexToken::Star |
            RegexToken::Plus |
            RegexToken::Optional => {
                RegexAST::Modifier(Box::new(base), build_quantifier(tokens, pos))
            }
            _ => base
        }
    } else {
        base
    }
}

fn build_base(tokens: &[RegexToken], pos: &mut usize) -> RegexAST {
    let token = &tokens[*pos];
    
    if token == &RegexToken::OpenBracket {
        *pos += 1;
        let expr = build_expr(tokens, &mut *pos);
        
        if *pos >= tokens.len() || tokens[*pos] != RegexToken::CloseBracket {
            panic!("Closing bracket not found")
        }
        
        *pos += 1;
        return RegexAST::Group(Box::new(expr));
    }
    
    match token {
        RegexToken::Const(c) => {
            *pos += 1;
            RegexAST::Const(*c)
        },
        _ => panic!("Unidentified token: {:?}", tokens[*pos])
    }
}

fn build_quantifier(tokens: &[RegexToken], pos: &mut usize) -> RegexASTModifier {
    let ast_mod = match tokens[*pos] {
        RegexToken::Star => RegexASTModifier::Star,
        RegexToken::Plus => RegexASTModifier::Plus,
        RegexToken::Optional => RegexASTModifier::Optional,
        _ => panic!("Unidentified token: {:?}", tokens[*pos])
    };

    *pos += 1;
    ast_mod
}

pub fn build_ast(tokens: &Vec<RegexToken>) -> RegexAST {
    let mut pos: usize = 0;
    let ast = build_expr(&tokens, &mut pos);

    if pos != tokens.len() {
        panic!("Unable to parse expression");
    }

    ast
}