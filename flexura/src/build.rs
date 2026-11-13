use anyhow::{bail, Context, Result};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::Paths;
use crate::fetch;
use crate::recipe::Recipe;
use crate::util;

pub struct BuildResult {
    pub dest: PathBuf,
    pub files: Vec<String>,
    pub dirs: Vec<String>,
    pub size: u64,
}

pub const SHIM: &str = r#"use() {
    case " $USE " in
        *" $1 "*) return 0 ;;
        *) return 1 ;;
    esac
}
has_flag() {
    use "$@"
}
use_enable() {
    local flag="$1" opt="${2:-$1}" val="${3:-}" off="${4:-}"
    if use "$flag"; then
        printf -- "--enable-%s" "$opt"
        [ -n "$val" ] && printf "=%s" "$val"
    else
        printf -- "--disable-%s" "$opt"
        [ -n "$off" ] && printf "=%s" "$off"
    fi
}
use_with() {
    local flag="$1" opt="${2:-$1}" val="${3:-}" off="${4:-}"
    if use "$flag"; then
        printf -- "--with-%s" "$opt"
        [ -n "$val" ] && printf "=%s" "$val"
    else
        printf -- "--without-%s" "$opt"
        [ -n "$off" ] && printf "=%s" "$off"
    fi
}
"#;

pub fn run(recipe: &Recipe, paths: &Paths, log_dir: &Path) -> Result<BuildResult> {
    let name = &recipe.package.name;
    let version = &recipe.package.version;
    let pf = format!("{}-{}", name, version);
    let build_dir = paths.tmpdir.join(&pf);
    let dest_dir = paths.tmpdir.join(format!("{}.dest", pf));
    if build_dir.exists() {
        std::fs::remove_dir_all(&build_dir)?;
    }
    if dest_dir.exists() {
        std::fs::remove_dir_all(&dest_dir)?;
    }
    std::fs::create_dir_all(&build_dir)?;
    std::fs::create_dir_all(&dest_dir)?;
    std::fs::create_dir_all(log_dir)?;

    let log_path = log_dir.join(format!("{}.log", pf));
    let mut log = std::fs::File::create(&log_path)
        .with_context(|| format!("открываю лог {}", log_path.display()))?;

    let work_dir = match &recipe.source {
        Some(src) => {
            let distfile = paths.distdir.join(file_name(&src.url));
            if !distfile.exists() || std::fs::metadata(&distfile)?.len() == 0 {
                util::info(&format!("загрузка {}", src.url));
                fetch::fetch(&src.url, &distfile, src.sha256.as_deref())?;
            } else {
                verify_cached(&distfile, src.sha256.as_deref())?;
            }
            unpack(&distfile, &build_dir)?;
            flatten(&build_dir)
        }
        None => build_dir.clone(),
    };

    let envs = vec![
        ("PN", name.clone()),
        ("PV", version.clone()),
        ("PF", pf.clone()),
        ("P", pf.clone()),
        ("S", work_dir.display().to_string()),
        ("WORKDIR", work_dir.display().to_string()),
        ("T", build_dir.join(".tmp").display().to_string()),
        ("D", dest_dir.display().to_string()),
        ("EROOT", paths.root.display().to_string()),
        ("DISTDIR", paths.distdir.display().to_string()),
        ("NPROC", paths.jobs.to_string()),
        ("USE", recipe.active_flags(&paths.use_flags).join(" ")),
    ];
    std::fs::create_dir_all(build_dir.join(".tmp"))?;
    let shim_path = build_dir.join(".flexura-shim.sh");
    std::fs::write(&shim_path, SHIM)?;
    let shim = shell_quote(&shim_path.display().to_string());

    let phases = [
        ("prepare", &recipe.build.prepare),
        ("configure", &recipe.build.configure),
        ("compile", &recipe.build.compile),
        ("install", &recipe.build.install),
    ];
    for (phase, steps) in phases {
        for step in steps {
            run_step(&pf, phase, step, &work_dir, &envs, &shim, &mut log)?;
        }
    }

    let mut files = Vec::new();
    let mut dirs = Vec::new();
    scan(&dest_dir, &dest_dir, &mut files, &mut dirs)?;
    if files.is_empty() {
        bail!(
            "{}: фаза install ничего не положила в {}",
            pf,
            dest_dir.display()
        );
    }
    files.sort();
    dirs.sort();
    let mut size = 0u64;
    for f in &files {
        let p = dest_dir.join(f);
        let m = std::fs::symlink_metadata(&p)?;
        size += if m.is_symlink() {
            m.len().min(4096)
        } else {
            m.len()
        };
    }
    Ok(BuildResult {
        dest: dest_dir,
        files,
        dirs,
        size,
    })
}

pub fn file_name(url: &str) -> String {
    let base = url.rsplit('/').next().unwrap_or(url);
    base.split('?').next().unwrap_or(base).to_string()
}

fn verify_cached(distfile: &Path, expected: Option<&str>) -> Result<()> {
    if let Some(sum) = expected {
        let real = fetch::sha256_file(distfile)?;
        if real != sum.to_lowercase() {
            std::fs::remove_file(distfile)?;
            bail!(
                "{}: кэш не сходится по sha256, файл удалён и будет скачан заново",
                distfile.display()
            );
        }
    }
    Ok(())
}

fn unpack(distfile: &Path, build_dir: &Path) -> Result<()> {
    let lower = file_name(&distfile.display().to_string()).to_lowercase();
    let q = |p: &Path| shell_quote(&p.display().to_string());
    let is_tar = lower.ends_with(".tar")
        || lower.ends_with(".tar.gz")
        || lower.ends_with(".tgz")
        || lower.ends_with(".tar.xz")
        || lower.ends_with(".txz")
        || lower.ends_with(".tar.bz2")
        || lower.ends_with(".tbz2")
        || lower.ends_with(".tar.zst");
    if is_tar {
        run_raw(&format!("tar -xf {} -C {}", q(distfile), q(build_dir)))?;
    } else if lower.ends_with(".zip") {
        run_raw(&format!("unzip -q {} -d {}", q(distfile), q(build_dir)))?;
    } else {
        let name = file_name(&distfile.display().to_string());
        std::fs::copy(distfile, build_dir.join(name))?;
    }
    Ok(())
}

fn flatten(build_dir: &Path) -> PathBuf {
    let Ok(entries) = std::fs::read_dir(build_dir) else {
        return build_dir.to_path_buf();
    };
    let paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    let dirs: Vec<&PathBuf> = paths.iter().filter(|p| p.is_dir()).collect();
    let files: Vec<&PathBuf> = paths.iter().filter(|p| p.is_file()).collect();
    if dirs.len() == 1 && files.is_empty() {
        return dirs[0].clone();
    }
    build_dir.to_path_buf()
}

fn run_step(
    pf: &str,
    phase: &str,
    cmd: &str,
    cwd: &Path,
    envs: &[(&str, String)],
    shim: &str,
    log: &mut std::fs::File,
) -> Result<()> {
    let script = format!("source {};\n{}", shim, cmd);
    let mut c = Command::new("bash");
    c.arg("-e").arg("-c").arg(&script).current_dir(cwd);
    for (k, v) in envs {
        c.env(k, v);
    }
    let output = c
        .output()
        .with_context(|| format!("{}: фаза {}", pf, phase))?;
    log.write_all(format!("[{}] {}\n", phase, cmd).as_bytes())?;
    log.write_all(&output.stdout)?;
    log.write_all(&output.stderr)?;
    if !output.status.success() {
        let tail = String::from_utf8_lossy(&output.stderr);
        let lines: Vec<&str> = tail.lines().collect();
        let start = lines.len().saturating_sub(8);
        for l in &lines[start..] {
            util::error(&format!("{}| {}", pf, l));
        }
        bail!(
            "{}: фаза {} завершилась с кодом {}",
            pf,
            phase,
            output.status.code().unwrap_or(-1)
        );
    }
    Ok(())
}

fn run_raw(cmd: &str) -> Result<()> {
    let st = Command::new("bash").arg("-e").arg("-c").arg(cmd).status()?;
    if !st.success() {
        bail!("команда не удалась: {}", cmd);
    }
    Ok(())
}

pub fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn scan(base: &Path, dir: &Path, files: &mut Vec<String>, dirs: &mut Vec<String>) -> Result<()> {
    for e in std::fs::read_dir(dir)? {
        let p = e?.path();
        let rel = p.strip_prefix(base)?.to_string_lossy().to_string();
        if p.is_dir() {
            dirs.push(rel.clone());
            scan(base, &p, files, dirs)?;
        } else {
            files.push(rel);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shim_is_valid_bash() {
        let st = Command::new("bash")
            .arg("-n")
            .arg("-c")
            .arg(format!(
                "source /dev/stdin <<'FLEXURA_SHIM'\n{}FLEXURA_SHIM",
                SHIM
            ))
            .status()
            .unwrap();
        assert!(st.success());
    }

    #[test]
    fn shim_helpers_work() {
        let dir = std::env::temp_dir().join(format!("flexura-shim-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let shim = dir.join("shim.sh");
        std::fs::write(&shim, SHIM).unwrap();
        let out = Command::new("bash")
            .arg("-c")
            .arg(format!(
                "source '{}'; echo $(use_enable ssl); echo $(use_with zlib); echo $(use_enable debug) | cat",
                shim.display()
            ))
            .env("USE", "ssl")
            .output()
            .unwrap();
        let text = String::from_utf8(out.stdout).unwrap();
        assert!(text.contains("--enable-ssl"));
        assert!(text.contains("--without-zlib"));
        assert!(text.contains("--disable-debug"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
