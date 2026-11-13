use std::collections::BTreeMap;

#[derive(Debug, Default)]
pub struct Doc {
    pub vars: Vec<(String, String)>,
    pub arrays: Vec<(String, Vec<String>)>,
    pub funcs: BTreeMap<String, Vec<String>>,
}

pub fn parse(text: &str) -> Doc {
    let mut doc = Doc::default();
    let mut lines = text.lines().peekable();
    while let Some(raw) = lines.next() {
        let line = strip_comment(raw).trim_end().to_string();
        if line.trim().is_empty() {
            continue;
        }
        if let Some(func) = parse_func_header(&line) {
            let mut depth = line.matches('{').count() as i32 - line.matches('}').count() as i32;
            let mut body: Vec<String> = Vec::new();
            if depth > 0 {
                for l in lines.by_ref() {
                    depth += brace_delta(&strip_comment(l));
                    if depth <= 0 {
                        break;
                    }
                    body.push(l.to_string());
                }
            }
            doc.funcs.insert(func, body);
            continue;
        }
        if let Some((name, item)) = parse_assign(&line) {
            if item == "(" {
                let mut acc = String::new();
                for l in lines.by_ref() {
                    let clean = strip_comment(l).trim().to_string();
                    if clean.contains(')') {
                        let head = clean.split(')').next().unwrap_or("").to_string();
                        acc.push_str(&head);
                        break;
                    }
                    acc.push_str(&clean);
                    acc.push(' ');
                }
                let items = split_array_items(&acc);
                doc.arrays.push((name, items));
                continue;
            }
            if let Some(full) = close_multiline(&item, &mut lines) {
                doc.vars.push((name, full));
            } else {
                doc.vars.push((name, unquote(&item)));
            }
            continue;
        }
        if let Some(cont) = continuation(&line) {
            let mut acc = cont;
            for l in lines.by_ref() {
                let clean = strip_comment(l).trim_end().to_string();
                let done = !clean.ends_with('\\');
                acc.push_str(clean.trim_end_matches('\\').trim());
                if done {
                    break;
                }
                acc.push(' ');
            }
            if let Some((name, item)) = parse_assign(&acc) {
                doc.vars.push((name, item));
            }
        }
    }
    doc
}

fn continuation(line: &str) -> Option<String> {
    if !line.trim_end().ends_with('\\') {
        return None;
    }
    let trimmed = line.trim_end().trim_end_matches('\\').trim();
    if trimmed.contains('=') && !trimmed.starts_with("local ") {
        Some(format!("{} ", trimmed))
    } else {
        None
    }
}

fn close_multiline<'a, I: Iterator<Item = &'a str>>(
    first: &str,
    lines: &mut std::iter::Peekable<I>,
) -> Option<String> {
    if !first.starts_with('"') {
        return None;
    }
    if quote_count(first).is_multiple_of(2) {
        return None;
    }
    let mut acc = first[1..].to_string();
    for l in lines.by_ref() {
        let clean = strip_comment(l).trim().to_string();
        acc.push('\n');
        acc.push_str(&clean);
        if quote_count(&clean) % 2 == 1 {
            break;
        }
    }
    let stripped = acc.trim_end_matches('"').to_string();
    Some(stripped.trim().to_string())
}

fn quote_count(s: &str) -> usize {
    s.matches('"').count()
}

fn parse_func_header(line: &str) -> Option<String> {
    let t = line.trim();
    let rest = t.strip_suffix('{')?.trim_end();
    let rest = rest.strip_suffix("()")?.trim_end();
    let name = rest.trim();
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    if name
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(true)
    {
        return None;
    }
    Some(name.to_string())
}

fn parse_assign(line: &str) -> Option<(String, String)> {
    let t = line.trim();
    let t = t.strip_prefix("export ").unwrap_or(t).trim_start();
    let t = t.strip_prefix("local ").unwrap_or(t).trim_start();
    let eq = t.find('=')?;
    let name = t[..eq].trim().to_string();
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    if name
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(true)
    {
        return None;
    }
    let raw = t[eq + 1..].trim();
    if raw == "(" {
        return Some((name, "(".to_string()));
    }
    Some((name, raw.to_string()))
}

pub fn unquote(s: &str) -> String {
    let s = s.trim();
    for (open, close) in [('"', '"'), ('\'', '\'')] {
        if s.starts_with(open) {
            if let Some(end) = matching_quote(s, open) {
                return s[1..end].to_string();
            }
            return s[1..].to_string();
        }
        let _ = close;
    }
    s.to_string()
}

fn matching_quote(s: &str, q: char) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = 1;
    while i < bytes.len() {
        if bytes[i] == b'\\' && q == '"' {
            i += 2;
            continue;
        }
        if bytes[i] as char == q {
            return Some(i);
        }
        i += 1;
    }
    None
}

pub fn strip_comment(line: &str) -> String {
    let bytes = line.as_bytes();
    let mut out = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        match c {
            '\'' if !in_double => {
                in_single = !in_single;
                out.push('\'');
            }
            '"' if !in_single => {
                in_double = !in_double;
                out.push('"');
            }
            '#' if !in_single && !in_double => {
                if i > 0 && !bytes[i - 1].is_ascii_whitespace() {
                    out.push(c);
                } else {
                    break;
                }
            }
            _ => out.push(c),
        }
        i += 1;
    }
    out
}

pub fn brace_delta(line: &str) -> i32 {
    let mut depth = 0;
    let mut in_single = false;
    let mut in_double = false;
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c == '\\' && in_double {
            i += 2;
            continue;
        }
        match c {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '{' if !in_single && !in_double => depth += 1,
            '}' if !in_single && !in_double => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    depth
}

fn split_array_items(s: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut token = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for c in s.chars() {
        if escaped {
            token.push(c);
            escaped = false;
            continue;
        }
        match c {
            '\\' if quote == Some('"') => {
                escaped = true;
                token.push(c);
            }
            '\'' if quote.is_none() => quote = Some('\''),
            '\'' if quote == Some('\'') => quote = None,
            '"' if quote.is_none() => quote = Some('"'),
            '"' if quote == Some('"') => quote = None,
            c if c.is_whitespace() && quote.is_none() => {
                if !token.is_empty() {
                    items.push(token.clone());
                    token.clear();
                }
            }
            _ => token.push(c),
        }
    }
    if !token.trim().is_empty() {
        items.push(token.trim().to_string());
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globals_and_arrays() {
        let doc = parse(
            "DESCRIPTION=\"Foo bar # not comment\"\n# real comment\nMY_PV=1.0\nSRC_URI=(\n  https://a/t.gz\n  https://b/t2.gz -> t2.tar.gz\n)\n",
        );
        assert_eq!(
            doc.vars[0],
            ("DESCRIPTION".into(), "Foo bar # not comment".into())
        );
        assert_eq!(doc.vars[1], ("MY_PV".into(), "1.0".into()));
        assert_eq!(doc.arrays[0].0, "SRC_URI");
        assert_eq!(doc.arrays[0].1.len(), 4);
        assert_eq!(doc.arrays[0].1[2], "->");
    }

    #[test]
    fn functions_with_quoted_braces() {
        let doc = parse(
            "src_install() {\n    emake DESTDIR=\"${D}\" install\n    dodoc README\n    sed -i 's/{/}/g' file\n    echo \"${D}\"\n}\n",
        );
        let body = doc.funcs.get("src_install").unwrap();
        assert_eq!(body.len(), 4);
    }

    #[test]
    fn comment_strip_keeps_hashes_in_strings() {
        assert_eq!(strip_comment("a=\"x#y\" # tail"), "a=\"x#y\" ");
        assert_eq!(strip_comment("a='x#y'"), "a='x#y'");
        assert_eq!(strip_comment("v=1#x"), "v=1#x");
    }

    #[test]
    fn multiline_quoted_value() {
        let doc = parse("POST_INSTALL_OBJECTS=\nSLOT=\"0\"\n");
        assert_eq!(doc.vars[1], ("SLOT".into(), "0".into()));
    }

    #[test]
    fn brace_delta_ignores_quotes() {
        assert_eq!(brace_delta("echo \"${x}\" {"), 1);
        assert_eq!(brace_delta("sed 's/{//'"), 0);
        assert_eq!(brace_delta("}"), -1);
    }
}

#[cfg(test)]
mod dbg_tests {
    use super::*;

    #[test]
    fn debug_multiline_rdepend() {
        let text = "SLOT=\"0\"\nRDEPEND=\"\n\tsys-libs/ncurses\n\tzlib>=1.2\n\"\n";
        let d = parse(text);
        eprintln!("vars: {:?}", d.vars);
        assert!(d.vars.iter().any(|(k, _)| k == "RDEPEND"));
    }
}
