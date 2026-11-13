#[derive(Debug, Clone)]
pub enum Atom {
    Simple(String),
    Group {
        flag: String,
        negated: bool,
        items: Vec<Atom>,
    },
    Any(Vec<Atom>),
}

pub fn split_atoms(text: &str) -> Vec<Atom> {
    let tokens = tokenize(text);
    let mut pos = 0usize;
    let mut out = Vec::new();
    while pos < tokens.len() {
        let (atom, next) = parse_atom(&tokens, pos);
        if let Some(a) = atom {
            out.push(a);
        }
        pos = next;
    }
    out
}

fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut bracket = 0usize;
    for c in text.chars() {
        match c {
            '[' if quote.is_none() => {
                bracket += 1;
                cur.push(c);
            }
            ']' if quote.is_none() && bracket > 0 => {
                bracket -= 1;
                cur.push(c);
            }
            '\'' | '"' => {
                quote = if quote.is_none() { Some(c) } else { None };
                cur.push(c);
            }
            '(' | ')' if quote.is_none() && bracket == 0 => {
                if !cur.is_empty() {
                    tokens.push(cur.clone());
                    cur.clear();
                }
                tokens.push(c.to_string());
            }
            c if c.is_whitespace() && quote.is_none() && bracket == 0 => {
                if !cur.is_empty() {
                    tokens.push(cur.clone());
                    cur.clear();
                }
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() {
        tokens.push(cur);
    }
    tokens
}

fn parse_atom(tokens: &[String], pos: usize) -> (Option<Atom>, usize) {
    let t = &tokens[pos];
    if t == "(" {
        let (items, next) = parse_group_body(tokens, pos + 1);
        return (Some(Atom::Any(items)), next);
    }
    if t == ")" || t == "||" {
        return (None, pos + 1);
    }
    if let Some(flag) = t.strip_suffix('?') {
        if tokens.get(pos + 1).map(|s| s.as_str()) == Some("(") {
            let (items, next) = parse_group_body(tokens, pos + 2);
            let negated = flag.starts_with('!');
            let flag = flag.trim_start_matches('!').to_string();
            return (
                Some(Atom::Group {
                    flag,
                    negated,
                    items,
                }),
                next,
            );
        }
    }
    (Some(Atom::Simple(t.clone())), pos + 1)
}

fn parse_group_body(tokens: &[String], mut pos: usize) -> (Vec<Atom>, usize) {
    let mut items = Vec::new();
    while pos < tokens.len() {
        if tokens[pos] == ")" {
            return (items, pos + 1);
        }
        let (atom, next) = parse_atom(tokens, pos);
        if let Some(a) = atom {
            items.push(a);
        }
        pos = next;
    }
    (items, pos)
}

pub fn flatten_leaves(atoms: &[Atom]) -> Vec<String> {
    let mut out = Vec::new();
    for a in atoms {
        match a {
            Atom::Simple(s) => out.push(s.clone()),
            Atom::Group { items, .. } => out.extend(flatten_leaves(items)),
            Atom::Any(_) => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_atoms() {
        let atoms = split_atoms("dev-libs/openssl zlib>=1.2 ");
        assert_eq!(atoms.len(), 2);
        match &atoms[0] {
            Atom::Simple(s) => assert_eq!(s, "dev-libs/openssl"),
            _ => panic!(),
        }
    }

    #[test]
    fn use_conditional() {
        let atoms = split_atoms("ssl? ( dev-libs/openssl ) !gnome? ( gtk+ )");
        assert_eq!(atoms.len(), 2);
        match &atoms[0] {
            Atom::Group {
                flag,
                negated,
                items,
            } => {
                assert_eq!(flag, "ssl");
                assert!(!negated);
                assert_eq!(items.len(), 1);
            }
            _ => panic!(),
        }
        match &atoms[1] {
            Atom::Group { flag, negated, .. } => {
                assert_eq!(flag, "gnome");
                assert!(negated);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn any_of() {
        let atoms = split_atoms("|| ( dev-lang/python:3.12 dev-lang/python:3.11 )");
        assert_eq!(atoms.len(), 1);
        match &atoms[0] {
            Atom::Any(items) => assert_eq!(items.len(), 2),
            _ => panic!(),
        }
    }

    #[test]
    fn nested_groups() {
        let atoms = split_atoms("a? ( b? ( x ) y )");
        match &atoms[0] {
            Atom::Group { items, .. } => assert_eq!(items.len(), 2),
            _ => panic!(),
        }
    }
}

#[cfg(test)]
mod bracket_tests {
    use super::*;

    #[test]
    fn use_brackets_stay_whole() {
        let atoms = split_atoms("sys-libs/ncurses:=[unicode(+)?] zlib");
        assert_eq!(atoms.len(), 2);
        match &atoms[0] {
            Atom::Simple(s) => assert_eq!(s, "sys-libs/ncurses:=[unicode(+)?]"),
            _ => panic!(),
        }
    }
}
