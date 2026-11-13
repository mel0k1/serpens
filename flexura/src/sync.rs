use anyhow::{bail, Result};
use std::path::Path;
use std::process::Command;

use crate::config::Paths;
use crate::util;

pub fn run(paths: &Paths) -> Result<()> {
    paths.ensure_dirs()?;
    if paths.repos_raw.is_empty() {
        util::warn("список репозиториев пуст, пропиши [main] repos в etc/flexura/flexura.conf");
        return Ok(());
    }
    for raw in &paths.repos_raw {
        if raw.starts_with("http://") || raw.starts_with("https://") || raw.starts_with("git@") {
            let dir = paths.url_repo_dir(raw);
            if dir.join(".git").exists() {
                git(&["pull", "--ff-only"], &dir)?;
                util::info(&format!(
                    "обновлён {}",
                    util::bold(&dir.display().to_string())
                ));
            } else {
                if let Some(parent) = dir.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let name = dir
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "repo".to_string());
                git(&["clone", raw, &name], dir.parent().unwrap())?;
                util::info(&format!("склонирован {}", raw));
            }
        } else {
            let p = Path::new(raw);
            if p.exists() {
                util::info(&format!("локальный {}", p.display()));
            } else {
                util::warn(&format!("локальный {} не существует", p.display()));
            }
        }
    }
    Ok(())
}

fn git(args: &[&str], cwd: &Path) -> Result<()> {
    let out = Command::new("git").args(args).current_dir(cwd).output()?;
    if !out.status.success() {
        bail!(
            "git {} не удался: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}
