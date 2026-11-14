use std::cmp::Ordering as Cmp;
use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, Ordering};

static QUIET: AtomicBool = AtomicBool::new(false);

pub fn set_quiet(q: bool) {
    QUIET.store(q, Ordering::Relaxed);
}

fn color_enabled() -> bool {
    std::env::var("NO_COLOR").is_err() && std::io::stderr().is_terminal()
}

pub fn paint(s: &str, code: &str) -> String {
    if color_enabled() {
        format!("\x1b[{}m{}\x1b[0m", code, s)
    } else {
        s.to_string()
    }
}

pub fn red(s: &str) -> String {
    paint(s, "31")
}

pub fn green(s: &str) -> String {
    paint(s, "32")
}

pub fn yellow(s: &str) -> String {
    paint(s, "33")
}

pub fn magenta(s: &str) -> String {
    paint(s, "35")
}

pub fn bold(s: &str) -> String {
    paint(s, "1")
}

pub fn dim(s: &str) -> String {
    paint(s, "2")
}

pub fn info(msg: &str) {
    if !QUIET.load(Ordering::Relaxed) {
        eprintln!("{} {}", bold(&magenta("flexura")), msg);
    }
}

pub fn warn(msg: &str) {
    eprintln!("{} {}", yellow("предупреждение:"), msg);
}

pub fn error(msg: &str) {
    eprintln!("{} {}", red("ошибка:"), msg);
}

pub fn human_size(n: u64) -> String {
    let units = ["B", "K", "M", "G", "T"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < units.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{} {}", n, units[0])
    } else {
        format!("{:.1}{}", v, units[i])
    }
}

pub fn plural_ru(n: usize, one: &str, few: &str, many: &str) -> String {
    let m10 = n % 10;
    let m100 = n % 100;
    if m10 == 1 && m100 != 11 {
        format!("{} {}", n, one)
    } else if (2..=4).contains(&m10) && !(12..=14).contains(&m100) {
        format!("{} {}", n, few)
    } else {
        format!("{} {}", n, many)
    }
}

pub fn epoch_utc(ts: u64) -> String {
    let days = ts / 86400;
    let rem = ts % 86400;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", y, mo, d, h, m, s)
}

pub fn ldconfig(root: &std::path::Path) {
    let has = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join("ldconfig").exists()))
        .unwrap_or(false);
    if !has {
        return;
    }
    let out = std::process::Command::new("ldconfig")
        .arg("-r")
        .arg(root)
        .output();
    if let Ok(o) = out {
        if !o.status.success() {
            let msg = String::from_utf8_lossy(&o.stderr);
            let msg = msg.trim();
            if !msg.is_empty() {
                warn(&format!("ldconfig: {}", msg));
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    Any,
    Eq,
    Ge,
    Le,
    Gt,
    Lt,
    Tilde,
}

#[derive(Debug, Clone)]
pub struct Constraint {
    pub op: Op,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blocker {
    None,
    Soft,
    Hard,
}

#[derive(Debug, Clone)]
pub struct Dep {
    pub name: String,
    pub constraint: Constraint,
    pub slot: Option<String>,
    pub blocker: Blocker,
}

impl Dep {
    pub fn is_blocker(&self) -> bool {
        self.blocker != Blocker::None
    }
}

impl std::fmt::Display for Dep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.blocker == Blocker::Hard {
            write!(f, "!!")?;
        } else if self.blocker == Blocker::Soft {
            write!(f, "!")?;
        }
        write!(f, "{}{}", self.name, constraint_str(&self.constraint))?;
        if let Some(s) = &self.slot {
            write!(f, ":{}", s)?;
        }
        Ok(())
    }
}

pub fn parse_dep(spec: &str) -> Dep {
    let s = spec.trim();
    let (blocker, s) = if let Some(r) = s.strip_prefix("!!") {
        (Blocker::Hard, r)
    } else if let Some(r) = s.strip_prefix('!') {
        (Blocker::Soft, r)
    } else {
        (Blocker::None, s)
    };
    let (body, slot) = split_slot(s);
    let (name, constraint) = parse_body(body);
    Dep {
        name: strip_category(&name),
        constraint,
        slot,
        blocker,
    }
}

pub fn strip_category(name: &str) -> String {
    match name.rfind('/') {
        Some(i) => name[i + 1..].to_string(),
        None => name.to_string(),
    }
}

fn split_slot(s: &str) -> (&str, Option<String>) {
    if let Some(pos) = s.rfind(':') {
        let tail = &s[pos + 1..];
        let ok = !tail.is_empty()
            && tail
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_')
            && tail
                .chars()
                .next()
                .map(|c| c.is_ascii_alphanumeric())
                .unwrap_or(false);
        if ok {
            return (&s[..pos], Some(tail.to_string()));
        }
    }
    (s, None)
}

fn parse_body(spec: &str) -> (String, Constraint) {
    let spec = spec.trim();
    let (op, rest) = if let Some(r) = spec.strip_prefix(">=") {
        (Op::Ge, r)
    } else if let Some(r) = spec.strip_prefix("<=") {
        (Op::Le, r)
    } else if let Some(r) = spec.strip_prefix("==") {
        (Op::Eq, r)
    } else if let Some(r) = spec.strip_prefix('=') {
        (Op::Eq, r)
    } else if let Some(r) = spec.strip_prefix('>') {
        (Op::Gt, r)
    } else if let Some(r) = spec.strip_prefix('<') {
        (Op::Lt, r)
    } else if let Some(r) = spec.strip_prefix('~') {
        (Op::Tilde, r)
    } else {
        (Op::Any, spec)
    };
    if op == Op::Any {
        if let Some((pos, sop)) = suffix_op(rest) {
            let name = rest[..pos].trim_end();
            let version = rest[pos + sop_len(&sop)..].trim_start();
            return (
                name.to_string(),
                Constraint {
                    op: sop,
                    version: if version.is_empty() {
                        None
                    } else {
                        Some(version.to_string())
                    },
                },
            );
        }
    }
    let (name, version) = split_name_version(rest);
    (
        name.to_string(),
        Constraint {
            op,
            version: version.map(|s| s.to_string()),
        },
    )
}

fn sop_len(op: &Op) -> usize {
    match op {
        Op::Any => 0,
        Op::Eq | Op::Tilde => 1,
        _ => 2,
    }
}

fn suffix_op(rest: &str) -> Option<(usize, Op)> {
    let tokens = [
        (">=", Op::Ge),
        ("<=", Op::Le),
        ("==", Op::Eq),
        (">", Op::Gt),
        ("<", Op::Lt),
        ("=", Op::Eq),
        ("~", Op::Tilde),
    ];
    let mut best: Option<(usize, Op)> = None;
    for (tok, op) in tokens {
        if let Some(pos) = rest.find(tok) {
            let replace = match &best {
                None => true,
                Some((bp, _)) => pos < *bp,
            };
            if replace {
                best = Some((pos, op));
            }
        }
    }
    best
}

pub fn split_name_version(s: &str) -> (&str, Option<&str>) {
    if let Some(pos) = s.rfind('-') {
        let (n, v) = (&s[..pos], &s[pos + 1..]);
        if !v.is_empty() && v.chars().next().unwrap().is_ascii_digit() {
            return (n, Some(v));
        }
    }
    (s, None)
}

fn version_key(v: &str) -> Vec<String> {
    v.split(['.', '_', '+']).map(|p| p.to_string()).collect()
}

pub fn version_cmp(a: &str, b: &str) -> Cmp {
    let ka = version_key(a);
    let kb = version_key(b);
    let n = ka.len().max(kb.len());
    for i in 0..n {
        let pa = ka.get(i).map(|s| s.as_str()).unwrap_or("0");
        let pb = kb.get(i).map(|s| s.as_str()).unwrap_or("0");
        let ord = match (pa.parse::<u64>(), pb.parse::<u64>()) {
            (Ok(x), Ok(y)) => x.cmp(&y),
            _ => pa.cmp(pb),
        };
        if ord != Cmp::Equal {
            return ord;
        }
    }
    Cmp::Equal
}

pub fn constraint_matches(version: &str, c: &Constraint) -> bool {
    let Some(target) = &c.version else {
        return true;
    };
    match c.op {
        Op::Any => true,
        Op::Eq => version_cmp(version, target) == Cmp::Equal,
        Op::Ge => version_cmp(version, target) != Cmp::Less,
        Op::Le => version_cmp(version, target) != Cmp::Greater,
        Op::Gt => version_cmp(version, target) == Cmp::Greater,
        Op::Lt => version_cmp(version, target) == Cmp::Less,
        Op::Tilde => {
            let base: Vec<&str> = target.split('.').take(2).collect();
            let vbase: Vec<String> = version_key(version).into_iter().take(2).collect();
            vbase.iter().map(|s| s.as_str()).eq(base)
        }
    }
}

pub fn constraint_str(c: &Constraint) -> String {
    match &c.version {
        None => String::new(),
        Some(v) => match c.op {
            Op::Any => String::new(),
            Op::Eq => format!("={}", v),
            Op::Ge => format!(">={}", v),
            Op::Le => format!("<={}", v),
            Op::Gt => format!(">{}", v),
            Op::Lt => format!("<{}", v),
            Op::Tilde => format!("~{}", v),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_ordering() {
        assert_eq!(version_cmp("1.2.3", "1.2.10"), Cmp::Less);
        assert_eq!(version_cmp("2.0", "1.99.9"), Cmp::Greater);
        assert_eq!(version_cmp("3.3.0", "3.3"), Cmp::Equal);
    }

    #[test]
    fn epoch_formatting() {
        assert_eq!(epoch_utc(0), "1970-01-01 00:00:00");
        assert_eq!(epoch_utc(951782400), "2000-02-29 00:00:00");
        assert_eq!(epoch_utc(1760486400), "2025-10-15 00:00:00");
        assert_eq!(epoch_utc(86399), "1970-01-01 23:59:59");
    }

    #[test]
    fn dep_parsing() {
        let d = parse_dep(">=ncurses-6.0");
        assert_eq!(d.name, "ncurses");
        assert!(constraint_matches("6.5", &d.constraint));
        assert!(!constraint_matches("5.9", &d.constraint));
        assert_eq!(d.slot, None);
        assert_eq!(d.blocker, Blocker::None);
        let d4 = parse_dep("ncurses>=6.0");
        assert_eq!(d4.name, "ncurses");
        assert!(constraint_matches("6.5", &d4.constraint));
        let d2 = parse_dep("lm-sensors");
        assert_eq!(d2.name, "lm-sensors");
        assert_eq!(d2.constraint.op, Op::Any);
        let d3 = parse_dep("hello=1.0");
        assert_eq!(d3.name, "hello");
        assert!(constraint_matches("1.0", &d3.constraint));
        assert!(!constraint_matches("1.1", &d3.constraint));
    }

    #[test]
    fn slot_parsing() {
        let d = parse_dep("openssl:1.1");
        assert_eq!(d.name, "openssl");
        assert_eq!(d.slot.as_deref(), Some("1.1"));
        let d = parse_dep(">=openssl-1.1.1:1.1");
        assert_eq!(d.name, "openssl");
        assert_eq!(d.slot.as_deref(), Some("1.1"));
        assert!(constraint_matches("1.1.1w", &d.constraint));
        let d = parse_dep("openssl>=3.0:3.0");
        assert_eq!(d.name, "openssl");
        assert_eq!(d.slot.as_deref(), Some("3.0"));
        assert!(constraint_matches("3.3.1", &d.constraint));
        let d = parse_dep("ncurses:6");
        assert_eq!(d.slot.as_deref(), Some("6"));
        assert_eq!(d.constraint.op, Op::Any);
        let d = parse_dep("dev-lang/python:3.12");
        assert_eq!(d.name, "python");
        assert_eq!(d.slot.as_deref(), Some("3.12"));
        let d = parse_dep("dev-libs/openssl>=3.0");
        assert_eq!(d.name, "openssl");
        assert!(constraint_matches("3.3.1", &d.constraint));
        let d = parse_dep("lm-sensors");
        assert_eq!(d.slot, None);
        assert_eq!(parse_dep("bash-5.2").slot, None);
    }

    #[test]
    fn blocker_parsing() {
        let d = parse_dep("!openssl");
        assert_eq!(d.blocker, Blocker::Soft);
        assert_eq!(d.name, "openssl");
        assert_eq!(d.to_string(), "!openssl");
        let d = parse_dep("!!openssl>=3.0");
        assert_eq!(d.blocker, Blocker::Hard);
        assert!(constraint_matches("3.3.1", &d.constraint));
        assert_eq!(d.to_string(), "!!openssl>=3.0");
        let d = parse_dep("!openssl:1.1");
        assert_eq!(d.blocker, Blocker::Soft);
        assert_eq!(d.slot.as_deref(), Some("1.1"));
        assert_eq!(d.to_string(), "!openssl:1.1");
        let d = parse_dep("!hello-1.0");
        assert_eq!(d.blocker, Blocker::Soft);
        assert_eq!(d.name, "hello");
        assert_eq!(d.constraint.op, Op::Any);
    }

    #[test]
    fn constraint_text() {
        let d = parse_dep("~bash-5.2");
        assert_eq!(constraint_str(&d.constraint), "~5.2");
        assert!(constraint_matches("5.2.37", &d.constraint));
        assert!(!constraint_matches("6.0.0", &d.constraint));
    }
}
