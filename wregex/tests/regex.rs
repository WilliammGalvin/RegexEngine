use proptest::prelude::*;
use wregex::{Error, Regex};

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).unwrap_or_else(|e| panic!("pattern {pattern:?} failed to compile: {e}"))
}

fn check_full(pattern: &str, cases: &[(&str, bool)]) {
    let r = re(pattern);
    for &(text, expected) in cases {
        assert_eq!(
            r.is_full_match(text),
            expected,
            "is_full_match: pattern {pattern:?} on {text:?}"
        );
    }
}

fn check_search(pattern: &str, cases: &[(&str, bool)]) {
    let r = re(pattern);
    for &(text, expected) in cases {
        assert_eq!(
            r.is_match(text),
            expected,
            "is_match: pattern {pattern:?} on {text:?}"
        );
    }
}

#[test]
fn literals() {
    check_full(
        "abc",
        &[("abc", true), ("ab", false), ("abcd", false), ("", false)],
    );
}

#[test]
fn empty_pattern() {
    check_full("", &[("", true), ("a", false)]);
    check_search("", &[("", true), ("anything", true)]);
}

#[test]
fn alternation() {
    check_full(
        "cat|dog|bird",
        &[
            ("cat", true),
            ("dog", true),
            ("bird", true),
            ("cow", false),
            ("catdog", false),
        ],
    );
    check_full("a|", &[("a", true), ("", true), ("b", false)]);
}

#[test]
fn quantifiers() {
    check_full("a*", &[("", true), ("aaaa", true), ("b", false)]);
    check_full("a+", &[("", false), ("a", true), ("aaaa", true)]);
    check_full(
        "colou?r",
        &[("color", true), ("colour", true), ("colouur", false)],
    );
    check_full("a{3}", &[("aa", false), ("aaa", true), ("aaaa", false)]);
    check_full("a{2,}", &[("a", false), ("aa", true), ("aaaaaa", true)]);
    check_full(
        "a{2,4}",
        &[("a", false), ("aa", true), ("aaaa", true), ("aaaaa", false)],
    );
    check_full("a{0}", &[("", true), ("a", false)]);
}

#[test]
fn groups() {
    check_full(
        "abc(d*)+",
        &[("abc", true), ("abcddd", true), ("abd", false)],
    );
    check_full("(ab|cd)*", &[("", true), ("abcdab", true), ("abc", false)]);
    check_full(
        "(ab){2}",
        &[("abab", true), ("ab", false), ("ababab", false)],
    );
    check_full("((a))", &[("a", true)]);
}

#[test]
fn classes() {
    check_full("[a-c]+x", &[("abcax", true), ("x", false), ("abdx", false)]);
    check_full("[^a-c]", &[("d", true), ("a", false), ("c", false)]);
    check_full("[]a]", &[("]", true), ("a", true), ("b", false)]);
    check_full("[a-]", &[("a", true), ("-", true), ("b", false)]);
    check_full(
        "[.+*]",
        &[(".", true), ("+", true), ("*", true), ("a", false)],
    );
}

#[test]
fn escapes() {
    check_full(r"a\.b", &[("a.b", true), ("axb", false)]);
    check_full(
        r"\d+-\d+",
        &[("10-200", true), ("10-", false), ("a-1", false)],
    );
    check_full(r"\w+", &[("foo_Bar9", true), ("foo bar", false)]);
    check_full(r"\s", &[(" ", true), ("\t", true), ("a", false)]);
    check_full(r"\D", &[("a", true), ("5", false)]);
    check_full(r"\n", &[("\n", true), ("n", false)]);
    check_full(r"\\", &[("\\", true), ("/", false)]);
    check_full(r"[\w.]+", &[("foo.bar_1", true), ("foo-bar", false)]);
}

#[test]
fn dot_excludes_newline() {
    check_full(".", &[("x", true), ("é", true), ("\n", false), ("", false)]);
    check_full("a.c", &[("abc", true), ("a\nc", false)]);
}

#[test]
fn anchors() {
    check_search("^ab$", &[("ab", true), ("xab", false), ("abx", false)]);
    check_search("^ab", &[("abc", true), ("cab", false)]);
    check_search("ab$", &[("cab", true), ("abc", false)]);
    check_search("^$", &[("", true), ("a", false)]);
}

#[test]
fn search_vs_full() {
    let r = re("b+c");
    assert!(r.is_match("aabbbcdd"));
    assert!(!r.is_full_match("aabbbcdd"));
    assert!(r.is_full_match("bbbc"));
    assert!(!r.is_match("aabbdd"));
}

#[test]
fn unicode() {
    check_full("é+", &[("ééé", true), ("e", false)]);
    check_full("[^a]+", &[("é", true), ("\u{10FFFF}", true), ("a", false)]);
    check_full("日本語", &[("日本語", true), ("日本", false)]);
    check_search("本", &[("日本語", true), ("日語", false)]);
}

#[test]
fn regex_is_reusable_and_cloneable() {
    let r = re("a+b");
    let c = r.clone();
    for _ in 0..3 {
        assert!(r.is_full_match("aaab"));
        assert!(c.is_full_match("aaab"));
        assert!(!r.is_full_match("aaa"));
    }
}

#[test]
fn types_are_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Regex>();
    assert_send_sync::<Error>();
}

#[test]
fn parse_errors_surface_as_error_parse() {
    for pattern in [
        "(", "a)", "*a", "a**", "[a", "[z-a]", "a{3,1}", r"\q", r"ab\",
    ] {
        assert!(
            matches!(Regex::new(pattern), Err(Error::Parse(_))),
            "pattern {pattern:?}"
        );
    }
}

#[test]
fn error_messages_are_readable() {
    let e = Regex::new("ab(c").unwrap_err();
    let msg = e.to_string();
    assert!(msg.contains("unclosed group"), "message was: {msg}");
    assert!(msg.contains("2..3"), "message was: {msg}");
}

#[test]
fn error_implements_std_error() {
    fn takes_error(_: &dyn std::error::Error) {}
    let e = Regex::new("(").unwrap_err();
    takes_error(&e);
}

#[test]
fn oversized_patterns_are_rejected_not_hung() {
    let e = Regex::new("((a{1000}){1000}){1000}").unwrap_err();
    assert!(matches!(e, Error::Build(_)), "got {e:?}");
}

#[test]
fn state_explosion_is_an_error() {
    let e = Regex::new("(a|b)*a(a|b){15}").unwrap_err();
    assert!(matches!(e, Error::Dfa(_)), "got {e:?}");
}

#[test]
fn deeply_nested_groups_do_not_overflow_the_stack() {
    let pattern = "(".repeat(10_000);
    assert!(matches!(Regex::new(&pattern), Err(Error::Parse(_))));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn new_never_panics_on_arbitrary_patterns(s in any::<String>()) {
        let _ = Regex::new(&s);
    }

    #[test]
    fn new_never_panics_on_metacharacter_soup(s in r"[ab()|*+?.^$\[\]{},0-9\\-]{0,16}") {
        let _ = Regex::new(&s);
    }

    #[test]
    fn matching_never_panics(text in any::<String>()) {
        for pattern in ["a*", "^(ab|cd)+$", "[^a-c]+x", r"\w+\d", "(a|b)*c", "é.$"] {
            let r = re(pattern);
            let _ = r.is_match(&text);
            let _ = r.is_full_match(&text);
        }
    }

    #[test]
    fn full_match_implies_search_match(
        pattern in r"[ab()|*+?.^$]{0,12}",
        text in "[abc]{0,8}",
    ) {
        if let Ok(r) = Regex::new(&pattern) {
            if r.is_full_match(&text) {
                prop_assert!(r.is_match(&text), "pattern {:?} text {:?}", pattern, text);
            }
        }
    }

    #[test]
    fn escaped_text_matches_itself(text in r"\PC{0,20}") {
        let mut pattern = String::new();
        for c in text.chars() {
            if !c.is_ascii_alphanumeric() {
                pattern.push('\\');
            }
            pattern.push(c);
        }
        let r = Regex::new(&pattern).unwrap();
        prop_assert!(r.is_full_match(&text));
        prop_assert!(r.is_match(&text));
    }

    #[test]
    fn search_finds_embedded_literals(
        prefix in "[xyz]{0,5}",
        needle in "[abc]{1,5}",
        suffix in "[xyz]{0,5}",
    ) {
        let r = Regex::new(&needle).unwrap();
        let hay = format!("{prefix}{needle}{suffix}");
        prop_assert!(r.is_match(&hay));
        prop_assert!(!r.is_full_match(&hay) || (prefix.is_empty() && suffix.is_empty()));
    }
}
