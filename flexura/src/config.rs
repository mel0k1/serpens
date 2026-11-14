use anyhow::Result;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::util::strip_category;

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct Config {
    pub main: Main,
    pub flags: Flags,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct Main {
    pub jobs: Option<usize>,
    pub tmpdir: Option<String>,
    pub distdir: Option<String>,
    pub pkgdir: Option<String>,
    pub repos: Vec<String>,
    pub profile: Option<String>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct Flags {
    #[serde(rename = "use")]
    pub use_flags: Vec<String>,
}

impl Config {
    pub fn load(root: &Path) -> Config {
        let path = root.join("etc/flexura/flexura.conf");
        if path.exists() {
            if let Ok(text) = std::fs::read_to_string(&path) {
                if let Ok(cfg) = toml::from_str::<Config>(&text) {
                    return cfg;
                }
            }
        }
        Config::default()
    }
}

pub struct Paths {
    pub root: PathBuf,
    pub db: PathBuf,
    pub world: PathBuf,
    pub distdir: PathBuf,
    pub tmpdir: PathBuf,
    pub pkgdir: PathBuf,
    pub logs: PathBuf,
    pub repos_raw: Vec<String>,
    pub repos: Vec<PathBuf>,
    pub jobs: usize,
    pub use_flags: Vec<String>,
    pub pkg_use: BTreeMap<String, Vec<String>>,
    pub masks: Vec<String>,
    pub profile: Option<String>,
}

impl Paths {
    pub fn new(root: &Path, cfg: &Config, jobs_override: Option<usize>) -> Paths {
        let db = root.join("var/db/flexura");
        let tmpdir = root.join(cfg.main.tmpdir.as_deref().unwrap_or("var/tmp/flexura"));
        let distdir = root.join(
            cfg.main
                .distdir
                .as_deref()
                .unwrap_or("var/cache/flexura/distfiles"),
        );
        let pkgdir = root.join(
            cfg.main
                .pkgdir
                .as_deref()
                .unwrap_or("var/cache/flexura/packages"),
        );
        let logs = root.join("var/log/flexura");
        let world = db.join("world");
        let jobs = jobs_override.or(cfg.main.jobs).unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1)
        });
        let mut repos = Vec::new();
        for raw in &cfg.main.repos {
            let p = PathBuf::from(raw);
            if p.is_absolute() {
                repos.push(p);
            } else {
                repos.push(root.join(raw));
            }
        }
        let mut use_flags = cfg.flags.use_flags.clone();
        if let (Some(first), Some(profile)) = (repos.first(), cfg.main.profile.as_deref()) {
            let md = first.join("profiles").join(profile).join("make.defaults");
            if md.exists() {
                if let Ok(text) = std::fs::read_to_string(&md) {
                    if let Ok(pc) = toml::from_str::<Config>(&text) {
                        for f in pc.flags.use_flags {
                            if !use_flags.contains(&f) {
                                use_flags.push(f);
                            }
                        }
                    }
                }
            }
        }
        let pkg_use = load_pkg_use(&root.join("etc/flexura/package.use"));
        let masks = load_lines(&root.join("etc/flexura/package.mask"));
        Paths {
            root: root.to_path_buf(),
            db,
            world,
            distdir,
            tmpdir,
            pkgdir,
            logs,
            repos_raw: cfg.main.repos.clone(),
            repos,
            jobs,
            use_flags,
            pkg_use,
            masks,
            profile: cfg.main.profile.clone(),
        }
    }

    pub fn ensure_dirs(&self) -> Result<()> {
        for d in [
            &self.db,
            &self.distdir,
            &self.tmpdir,
            &self.pkgdir,
            &self.logs,
        ] {
            std::fs::create_dir_all(d)?;
        }
        Ok(())
    }

    pub fn url_repo_dir(&self, url: &str) -> PathBuf {
        let name = url
            .rsplit('/')
            .next()
            .unwrap_or("repo")
            .trim_end_matches(".git");
        self.root.join("var/db/repos").join(name)
    }
}

fn meaningful_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.to_string())
        .collect()
}

pub fn load_lines(path: &Path) -> Vec<String> {
    match std::fs::read_to_string(path) {
        Ok(text) => meaningful_lines(&text),
        Err(_) => Vec::new(),
    }
}

pub fn load_pkg_use(path: &Path) -> BTreeMap<String, Vec<String>> {
    let mut m: BTreeMap<String, Vec<String>> = BTreeMap::new();
    if let Ok(text) = std::fs::read_to_string(path) {
        for line in meaningful_lines(&text) {
            let mut parts = line.split_whitespace();
            let Some(target) = parts.next() else {
                continue;
            };
            let flags: Vec<String> = parts.map(|s| s.to_string()).collect();
            if flags.is_empty() {
                continue;
            }
            m.entry(strip_category(target)).or_default().extend(flags);
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkg_use_parser() {
        let dir = std::env::temp_dir().join(format!("flexura-cfg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("package.use");
        std::fs::write(
            &p,
            "# comment\nopenssl -man +zlib\ndev-libs/other:6 flag1\n\n",
        )
        .unwrap();
        let m = load_pkg_use(&p);
        assert_eq!(m["openssl"], vec!["-man".to_string(), "+zlib".to_string()]);
        assert_eq!(m["other:6"], vec!["flag1".to_string()]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn mask_parser() {
        let dir = std::env::temp_dir().join(format!("flexura-mask-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("package.mask");
        std::fs::write(&p, ">=mail-client/bad-2.0\n# no\nplain").unwrap();
        let m = load_lines(&p);
        assert_eq!(
            m,
            vec![">=mail-client/bad-2.0".to_string(), "plain".to_string()]
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
