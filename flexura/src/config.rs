use anyhow::Result;
use serde::Deserialize;
use std::path::{Path, PathBuf};

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
