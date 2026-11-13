mod atoms;
mod doc;
mod translate;

use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Serialize)]
struct Recipe {
    package: Package,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<Source>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dependencies: Option<Deps>,
    #[serde(skip_serializing_if = "Option::is_none")]
    flags: Option<Flags>,
    #[serde(skip_serializing_if = "Option::is_none")]
    build: Option<Build>,
}

#[derive(Serialize)]
struct Package {
    name: String,
    version: String,
    description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    homepage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    license: Option<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    slot: String,
}

#[derive(Serialize)]
struct Source {
    url: String,
}

#[derive(Serialize)]
struct Deps {
    #[serde(skip_serializing_if = "Option::is_none")]
    build: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    runtime: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    optional: Option<BTreeMap<String, Vec<String>>>,
}

#[derive(Serialize)]
struct Flags {
    #[serde(skip_serializing_if = "Option::is_none")]
    default: Option<Vec<String>>,
}

#[derive(Serialize)]
struct Build {
    #[serde(skip_serializing_if = "Option::is_none")]
    prepare: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    configure: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    compile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    install: Option<String>,
}

struct Converted {
    toml: String,
    warnings: Vec<String>,
}

fn main() {
    let mut out_dir: Option<PathBuf> = None;
    let mut files: Vec<PathBuf> = Vec::new();
    let mut to_stdout = false;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "-o" | "--out" => match args.next() {
                Some(d) => out_dir = Some(PathBuf::from(d)),
                None => die("после -o нужен каталог"),
            },
            "--stdout" => to_stdout = true,
            "-h" | "--help" => {
                println!("usage: ebuild2flx [-o DIR] [--stdout] FILE.ebuild...");
                println!("  конвертирует ебилды Gentoo в рецепты .flx для Serpens");
                std::process::exit(0);
            }
            _ => files.push(PathBuf::from(a)),
        }
    }
    if files.is_empty() {
        die("укажи хотя бы один .ebuild");
    }
    let mut failed = false;
    for f in &files {
        match convert_file(f) {
            Ok(c) => {
                for w in &c.warnings {
                    eprintln!("предупреждение: {}: {}", f.display(), w);
                }
                if to_stdout {
                    print!("{}", c.toml);
                } else {
                    let dir = out_dir
                        .clone()
                        .unwrap_or_else(|| f.parent().map(|p| p.to_path_buf()).unwrap_or_default());
                    if let Err(e) = std::fs::create_dir_all(&dir) {
                        die(&format!("{}: {}", dir.display(), e));
                    }
                    let out = dir.join(format!("{}.flx", c.name));
                    if let Err(e) = std::fs::write(&out, &c.toml) {
                        die(&format!("{}: {}", out.display(), e));
                    }
                    println!("конвертирован {} -> {}", f.display(), out.display());
                }
            }
            Err(e) => {
                eprintln!("ошибка: {}: {}", f.display(), e);
                failed = true;
            }
        }
    }
    if failed {
        std::process::exit(1);
    }
}

struct Output {
    name: String,
    toml: String,
    warnings: Vec<String>,
}

impl Output {
    fn from_converted(name: String, c: Converted) -> Output {
        Output {
            name,
            toml: c.toml,
            warnings: c.warnings,
        }
    }
}

fn die(msg: &str) -> ! {
    eprintln!("ошибка: {}", msg);
    std::process::exit(1)
}

fn convert_file(path: &Path) -> Result<Output, String> {
    let fname = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let (pn, pv) = split_pkg_filename(&fname)
        .ok_or_else(|| format!("имя файла {} не похоже на ебилд", fname))?;
    let text = std::fs::read_to_string(path).map_err(|e| format!("не читаю: {}", e))?;
    let c = convert_text(&text, &pn, &pv)?;
    Ok(Output::from_converted(pn, c))
}

pub fn split_pkg_filename(fname: &str) -> Option<(String, String)> {
    let base = fname.strip_suffix(".ebuild")?;
    let pos = base.rfind('-')?;
    let v = &base[pos + 1..];
    if v.is_empty()
        || !v
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
    {
        return None;
    }
    Some((base[..pos].to_string(), v.to_string()))
}

fn convert_text(text: &str, pn: &str, pv: &str) -> Result<Converted, String> {
    let mut warnings: Vec<String> = Vec::new();
    let d = doc::parse(text);
    let mut env: BTreeMap<String, String> = BTreeMap::new();
    env.insert("PN".into(), pn.to_string());
    env.insert("PV".into(), pv.to_string());
    env.insert("PR".into(), "r0".to_string());
    let p = format!("{}-{}", pn, pv);
    env.insert("P".into(), p.clone());
    env.insert("PF".into(), format!("{}-r0", p));
    for (k, v) in &d.vars {
        env.insert(k.clone(), expand(v, &env, 0));
    }
    for (k, items) in &d.arrays {
        let joined = items
            .iter()
            .map(|i| expand(i, &env, 0))
            .collect::<Vec<_>>()
            .join(" ");
        env.insert(k.clone(), joined);
    }

    let get = |name: &str| -> String { env.get(name).cloned().unwrap_or_default() };

    let description = get("DESCRIPTION");
    if description.is_empty() {
        warnings.push("DESCRIPTION пуст".to_string());
    }
    let homepage_raw = get("HOMEPAGE");
    let homepage = homepage_raw
        .split_whitespace()
        .next()
        .map(|s| s.to_string())
        .unwrap_or_default();
    let license = get("LICENSE");
    let slot = {
        let s = get("SLOT");
        if s.is_empty() || s.contains('*') {
            "0".to_string()
        } else {
            s
        }
    };

    let source = first_url(&get("SRC_URI"), &mut warnings).map(|url| Source { url });

    let mut build_deps: Vec<String> = Vec::new();
    let mut runtime_deps: Vec<String> = Vec::new();
    let mut optional: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for var in ["DEPEND", "BDEPEND"] {
        collect_deps(&get(var), &mut build_deps, &mut optional, &mut warnings);
    }
    for var in ["RDEPEND", "PDEPEND"] {
        collect_deps(&get(var), &mut runtime_deps, &mut optional, &mut warnings);
    }
    build_deps.sort();
    build_deps.dedup();
    runtime_deps.sort();
    runtime_deps.dedup();

    let mut flags: Vec<String> = Vec::new();
    let mut default_flags: Vec<String> = Vec::new();
    for f in get("IUSE").split_whitespace() {
        if f.starts_with('+') {
            let name = f.trim_start_matches('+').to_string();
            if !name.is_empty() && !default_flags.contains(&name) {
                default_flags.push(name.clone());
            }
        }
        let name = f
            .trim_start_matches('+')
            .trim_start_matches('-')
            .to_string();
        if !name.is_empty() && !flags.contains(&name) {
            flags.push(name);
        }
    }

    let mut prepare = None;
    let mut configure = None;
    let mut compile = None;
    let mut install = None;
    if let Some(body) = d.funcs.get("src_prepare") {
        prepare = translate::translate_phase(body, "prepare", &mut warnings);
    }
    for (func, field) in [
        ("src_configure", &mut configure),
        ("src_compile", &mut compile),
        ("src_install", &mut install),
    ] {
        match d.funcs.get(func) {
            Some(body) => {
                *field = translate::translate_phase(
                    body,
                    func.strip_prefix("src_").unwrap(),
                    &mut warnings,
                );
            }
            None => {
                let def = translate::default_for(func.strip_prefix("src_").unwrap());
                if !def.is_empty() {
                    *field = Some(def);
                }
            }
        }
    }
    if let Some(body) = d.funcs.get("src_unpack") {
        if !body.is_empty() {
            warnings.push("src_unpack не переносится, используется распаковка flexura".to_string());
        }
    }
    for func in d.funcs.keys() {
        if func.starts_with("pkg_") {
            warnings.push(format!("{} не переносится", func));
        }
    }
    if !get("PYTHON_COMPAT").is_empty() {
        warnings.push("питоний пакет, Recipes потребуют ручной доработки".to_string());
    }

    let deps = if build_deps.is_empty() && runtime_deps.is_empty() && optional.is_empty() {
        None
    } else {
        Some(Deps {
            build: some_nonempty(build_deps),
            runtime: some_nonempty(runtime_deps),
            optional: if optional.is_empty() {
                None
            } else {
                Some(optional)
            },
        })
    };
    let flags = if default_flags.is_empty() {
        None
    } else {
        Some(Flags {
            default: some_nonempty(default_flags),
        })
    };
    let build =
        if prepare.is_none() && configure.is_none() && compile.is_none() && install.is_none() {
            None
        } else {
            Some(Build {
                prepare,
                configure,
                compile,
                install,
            })
        };

    let recipe = Recipe {
        package: Package {
            name: pn.to_string(),
            version: pv.to_string(),
            description,
            homepage: some_str(homepage),
            license: some_str(license),
            slot,
        },
        source,
        dependencies: deps,
        flags,
        build,
    };
    let toml = toml::to_string_pretty(&recipe).map_err(|e| format!("toml: {}", e))?;
    Ok(Converted { toml, warnings })
}

fn some_nonempty(v: Vec<String>) -> Option<Vec<String>> {
    if v.is_empty() {
        None
    } else {
        Some(v)
    }
}

fn some_str(s: String) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

fn expand(s: &str, env: &BTreeMap<String, String>, depth: u8) -> String {
    if depth > 8 {
        return s.to_string();
    }
    let mut out = String::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() {
            if bytes[i + 1] == b'{' {
                if let Some(end) = s[i + 2..].find('}') {
                    let name = &s[i + 2..i + 2 + end];
                    if let Some(v) = env.get(name) {
                        out.push_str(&expand(v, env, depth + 1));
                        i += 2 + end + 1;
                        continue;
                    }
                    out.push_str("${");
                    out.push_str(name);
                    out.push('}');
                    i += 2 + end + 1;
                    continue;
                }
            } else {
                let mut j = i + 1;
                while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                    j += 1;
                }
                if j > i + 1 {
                    let name = &s[i + 1..j];
                    if let Some(v) = env.get(name) {
                        out.push_str(&expand(v, env, depth + 1));
                        i = j;
                        continue;
                    }
                }
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn first_url(src_uri: &str, warnings: &mut Vec<String>) -> Option<String> {
    let tokens: Vec<&str> = src_uri.split_whitespace().collect();
    let mut i = 0;
    while i < tokens.len() {
        let t = tokens[i];
        if t == "->" {
            i += 2;
            continue;
        }
        if t.starts_with("http://") || t.starts_with("https://") || t.starts_with("ftp://") {
            return Some(t.trim_end_matches('/').to_string());
        }
        if let Some(rest) = t.strip_prefix("mirror://") {
            let mapped = map_mirror(rest, warnings);
            if let Some(m) = mapped {
                return Some(m);
            }
            i += 1;
            continue;
        }
        i += 1;
    }
    if !src_uri.trim().is_empty() {
        warnings.push("SRC_URI: подходящего URL не найдено".to_string());
    }
    None
}

fn map_mirror(rest: &str, warnings: &mut Vec<String>) -> Option<String> {
    let (host, tail) = {
        let i = rest.find('/')?;
        (&rest[..i], &rest[i + 1..])
    };
    match host {
        "sourceforge" => {
            let parts: Vec<&str> = tail.splitn(2, '/').collect();
            if parts.len() == 2 {
                return Some(format!(
                    "https://downloads.sourceforge.net/project/{}/{}",
                    parts[0], parts[1]
                ));
            }
            None
        }
        "github" => {
            warnings.push("mirror://github требует ручной правки URL".to_string());
            None
        }
        "pypi" => {
            warnings.push("mirror://pypi требует ручной правки URL".to_string());
            None
        }
        _ => {
            warnings.push(format!("зеркало mirror://{} не поддержано", host));
            None
        }
    }
}

fn collect_deps(
    text: &str,
    simple: &mut Vec<String>,
    optional: &mut BTreeMap<String, Vec<String>>,
    warnings: &mut Vec<String>,
) {
    let list = atoms::split_atoms(text);
    for a in &list {
        match a {
            atoms::Atom::Simple(s) => {
                if let Some(dep) = map_atom(s, warnings) {
                    simple.push(dep);
                }
            }
            atoms::Atom::Group {
                flag,
                negated,
                items,
            } => {
                if *negated {
                    warnings.push(format!(
                        "условие !{}? переносится как безусловные зависимости",
                        flag
                    ));
                    for dep in atoms::flatten_leaves(items) {
                        if let Some(dep) = map_atom(&dep, warnings) {
                            simple.push(dep);
                        }
                    }
                    continue;
                }
                let entry = optional.entry(flag.clone()).or_default();
                for dep in atoms::flatten_leaves(items) {
                    if let Some(dep) = map_atom(&dep, warnings) {
                        if !entry.contains(&dep) {
                            entry.push(dep);
                        }
                    }
                }
            }
            atoms::Atom::Any(items) => {
                warnings.push("|| ( ... ) берётся как первый вариант".to_string());
                if let Some(atoms::Atom::Simple(first)) = items.first() {
                    if let Some(dep) = map_atom(first, warnings) {
                        simple.push(dep);
                    }
                }
            }
        }
    }
}

fn map_atom(s: &str, warnings: &mut Vec<String>) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    if t.contains('[') {
        warnings.push(format!("атом с USE-условием пропущен: {}", t));
        return None;
    }
    if t.contains('*') {
        warnings.push(format!("атом с глобом пропущен: {}", t));
        return None;
    }
    let dep = t.to_string();
    let prefix_len = dep.chars().take_while(|c| *c == '!').count();
    let (prefix, rest) = dep.split_at(prefix_len);
    let mut rest = rest.to_string();
    if let Some(i) = rest.find('/') {
        rest = rest[i + 1..].to_string();
    }
    if rest.ends_with('=') {
        rest.truncate(rest.len() - 1);
    }
    if rest.ends_with(':') {
        rest.truncate(rest.len() - 1);
    }
    if rest.ends_with('=') {
        rest.truncate(rest.len() - 1);
    }
    let dep = format!("{}{}", prefix, rest);
    if dep.starts_with('-') {
        warnings.push(format!("атом с диапазоном пропущен: {}", t));
        return None;
    }
    Some(dep)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filename_split() {
        assert_eq!(
            split_pkg_filename("bash-5.2.21.ebuild"),
            Some(("bash".into(), "5.2.21".into()))
        );
        assert_eq!(
            split_pkg_filename("openssl-3.3.1.ebuild"),
            Some(("openssl".into(), "3.3.1".into()))
        );
        assert_eq!(split_pkg_filename("README"), None);
        assert_eq!(split_pkg_filename("foo-.ebuild"), None);
    }

    #[test]
    fn var_expansion() {
        let mut env = BTreeMap::new();
        env.insert("PN".to_string(), "htop".to_string());
        env.insert("PV".to_string(), "3.3.0".to_string());
        let s = expand("${PN}-${PV}", &env, 0);
        assert_eq!(s, "htop-3.3.0");
        let s = expand("$PN/$PV.tar", &env, 0);
        assert_eq!(s, "htop/3.3.0.tar");
        let s = expand("${UNKNOWN}-x", &env, 0);
        assert_eq!(s, "${UNKNOWN}-x");
    }

    #[test]
    fn src_uri_pick() {
        let mut w = Vec::new();
        assert_eq!(
            first_url("https://example.com/a.tar.gz", &mut w),
            Some("https://example.com/a.tar.gz".into())
        );
        assert_eq!(
            first_url(
                "mirror1 https://example.com/a.tar.gz -> renamed.tar.gz",
                &mut w
            ),
            Some("https://example.com/a.tar.gz".into())
        );
        assert_eq!(first_url("no-url-here mirror://gentoo/x", &mut w), None);
        assert!(!w.is_empty());
    }

    #[test]
    fn atom_mapping() {
        let mut w = Vec::new();
        assert_eq!(map_atom("dev-libs/openssl", &mut w), Some("openssl".into()));
        assert_eq!(
            map_atom("dev-lang/python:3.12", &mut w),
            Some("python:3.12".into())
        );
        assert_eq!(map_atom("!dev-libs/foo", &mut w), Some("!foo".into()));
        assert_eq!(
            map_atom("sys-libs/binutils-libs:=", &mut w),
            Some("binutils-libs".into())
        );
        assert_eq!(
            map_atom("dev-libs/libnl:3=", &mut w),
            Some("libnl:3".into())
        );
        assert_eq!(map_atom("foo[bar]", &mut w), None);
        assert!(!w.is_empty());
    }

    #[test]
    fn atom_collect_optional() {
        let mut w = Vec::new();
        let mut simple = Vec::new();
        let mut optional = BTreeMap::new();
        collect_deps(
            "sys-libs/ncurses bfd? ( dev-libs/binutils-libs )",
            &mut simple,
            &mut optional,
            &mut w,
        );
        assert!(simple.contains(&"ncurses".to_string()));
        assert!(optional.contains_key("bfd"));
    }

    #[test]
    fn full_conversion() {
        let text = r#"
EAPI=8
DESCRIPTION="Test package"
HOMEPAGE="https://example.com/"
SRC_URI="https://example.com/${P}.tar.gz"
LICENSE="MIT"
SLOT="0"
IUSE="+ssl static"
RDEPEND="dev-libs/openssl zlib>=1.2"
DEPEND="${RDEPEND} ssl? ( dev-libs/openssl )"

src_configure() {
    econf $(use_enable ssl)
}

src_compile() {
    emake
}

src_install() {
    emake DESTDIR="${D}" install
    dodoc README
}
"#;
        let c = convert_text(text, "testpkg", "1.2.3").unwrap();
        assert!(c.toml.contains("name = \"testpkg\""));
        assert!(c.toml.contains("version = \"1.2.3\""));
        assert!(c
            .toml
            .contains("url = \"https://example.com/testpkg-1.2.3.tar.gz\""));
        assert!(c
            .toml
            .contains("./configure --prefix=/usr $(use_enable ssl)"));
        assert!(c.toml.contains("make -j${NPROC}"));
        assert!(c.toml.contains("[dependencies.optional]"));
        assert!(c.toml.contains("[flags]"));
        let parsed: Result<toml::Value, _> = toml::from_str(&c.toml);
        assert!(parsed.is_ok());
    }

    #[test]
    fn default_phases_when_absent() {
        let text = "DESCRIPTION=\"x\"\nSLOT=\"1.0\"\n";
        let c = convert_text(text, "minimal", "0.1").unwrap();
        assert!(c.toml.contains("slot = \"1.0\""));
        assert!(c.toml.contains("./configure --prefix=/usr"));
        assert!(c.toml.contains("make DESTDIR=\"${D}\" install"));
    }
}
