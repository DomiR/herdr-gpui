//! Linear rejects a query whose complexity passes 10,000 points, so each
//! query's estimate is computed here by Linear's published rules: 0.1 a
//! scalar field, 1 an object, and a connection's children counted `first`
//! times (50 when unset).

use crate::linear::{CAP, OPTIONS, QUERY, STATES};

const LIMIT: u64 = 10_000;

enum Token {
    Name(String),
    Arguments(String),
    Open,
    Close,
}

fn tokens(query: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = query.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => tokens.push(Token::Open),
            '}' => tokens.push(Token::Close),
            '(' => {
                let (mut depth, mut text) = (1, String::new());
                for c in chars.by_ref() {
                    depth += match c {
                        '(' => 1,
                        ')' => -1,
                        _ => 0,
                    };
                    if depth == 0 {
                        break;
                    }
                    text.push(c);
                }
                tokens.push(Token::Arguments(text));
            }
            c if c.is_alphanumeric() || c == '_' => {
                let mut name = c.to_string();
                while let Some(&c) = chars.peek().filter(|c| c.is_alphanumeric() || **c == '_') {
                    name.push(c);
                    chars.next();
                }
                tokens.push(Token::Name(name));
            }
            _ => {}
        }
    }
    tokens
}

/// `arguments`' `first`.
fn first(arguments: &str) -> Option<u64> {
    let after = &arguments[arguments.find("first:")? + "first:".len()..];
    let digits: String = after
        .trim_start()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

/// The cost in tenths of a point of the fields up to the selection's
/// closing brace, and whether they hold a connection's `nodes`.
fn selection(tokens: &mut std::iter::Peekable<std::vec::IntoIter<Token>>) -> (u64, bool) {
    let (mut cost, mut nodes) = (0, false);
    while let Some(token) = tokens.next() {
        let name = match token {
            Token::Close => break,
            Token::Name(name) => name,
            Token::Arguments(_) | Token::Open => panic!("unexpected token"),
        };
        nodes |= name == "nodes";
        let arguments = match tokens.peek() {
            Some(Token::Arguments(_)) => match tokens.next() {
                Some(Token::Arguments(text)) => Some(text),
                _ => None,
            },
            _ => None,
        };
        if !matches!(tokens.peek(), Some(Token::Open)) {
            cost += 1;
            continue;
        }
        tokens.next();
        let (children, connection) = selection(tokens);
        let times = match arguments.as_deref().and_then(first) {
            Some(first) => first,
            None if connection => 50,
            None => 1,
        };
        cost += 10 + times * children;
    }
    (cost, nodes)
}

/// Linear's estimate of `query`'s complexity, rounded up.
fn complexity(query: &str) -> u64 {
    let body = &query[query.find('{').unwrap()..];
    let mut tokens = tokens(body).into_iter().peekable();
    assert!(matches!(tokens.next(), Some(Token::Open)));
    selection(&mut tokens).0.div_ceil(10)
}

#[test]
fn the_estimate_follows_linears_rules() {
    assert_eq!(complexity("query { viewer { id name } }"), 2);
    // 1 for the connection, then 10 times (1 for nodes + 0.2).
    assert_eq!(
        complexity("query { teams(first: 10) { nodes { id name } } }"),
        13
    );
    // Unset, a connection's `first` is 50.
    assert_eq!(complexity("query { teams { nodes { id } } }"), 56);
}

#[test]
fn a_page_of_issues_costs_well_under_linears_limit() {
    // With up to ten sub-issues each, a page costs a little over half the
    // limit; the margin keeps one page far from a refusal.
    let cost = complexity(QUERY);
    assert!(cost <= LIMIT * 6 / 10, "{cost}");
    assert!(QUERY.contains("issues(first: 50,"), "pages of 50 issues");
    assert_eq!(CAP % 50, 0, "a scope's cap is whole pages");
}

#[test]
fn the_other_queries_stay_under_linears_limit() {
    for (name, query) in [("STATES", STATES), ("OPTIONS", OPTIONS)] {
        let cost = complexity(query);
        assert!(cost < LIMIT, "{name}: {cost}");
    }
}
