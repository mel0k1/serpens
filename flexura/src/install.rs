use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::build;
use crate::config::Paths;
use crate::db::{self, Db, Installed};
use crate::fetch;
use crate::resolver;
use crate::util;
use crate::world;

pub const META_NAME: &str = ".flexura-meta.toml";

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct TarMeta {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub slot: String,
    #[serde(default)]
    pub use_flags: Vec<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub license: String,
    #[serde(default)]
    pub homepage: String,
}

pub fn run(
    paths: &Paths,
    targets: &[String],
    force: bool,
    upgrade: bool,
    pretend: bool,
    usepkg: bool,
    oneshot: bool,
) -> Result<()> {
    if targets.is_empty() {
        util::warn("укажи хотя бы один пакет");
        return Ok(());
    }
    let dbpath = paths.db.join("installed.json");
    let db = db::load(&dbpath)?;
    let plan = resolver::resolve(paths, &db, targets, force, upgrade)?;
    if plan.order.is_empty() {
        util::info("делать нечего, всё уже установлено");
        return Ok(());
    }
    paths.ensure_dirs()?;
    util::info(&format!(
        "план сборки, {}:",
        util::plural_ru(plan.order.len(), "пакет", "пакета", "пакетов")
    ));
    for r in &plan.order {
        let action = match db::lookup(&db, &r.package.name, Some(&r.package.slot)) {
            Some(e) if e.version == r.package.version && !force => util::dim("пересборка"),
            Some(e) => util::yellow(&format!("обновление с {}", e.version)),
            None => util::green("новый"),
        };
        let via =
            if usepkg && quickpkg_tarball(paths, &r.package.name, &r.package.version).is_some() {
                util::dim(" из тарбола")
            } else {
                String::new()
            };
        println!(
            "  {} {} {}{}{}",
            action,
            util::bold(&r.package.name),
            util::dim(&r.package.version),
            if r.package.slot == "0" {
                String::new()
            } else {
                util::dim(&format!(":{}", r.package.slot))
            },
            via
        );
    }
    if pretend {
        util::info("черновик, ничего не установлено");
        return Ok(());
    }
    let mut newdb = db.clone();
    for r in &plan.order {
        let pf = format!("{}-{}", r.package.name, r.package.version);
        let rebuild = match db::lookup(&db, &r.package.name, Some(&r.package.slot)) {
            Some(e) => e.version == r.package.version,
            None => false,
        };
        let verb = if rebuild {
            "пересборка"
        } else {
            "сборка"
        };
        util::info(&format!("{} {}", verb, util::bold(&pf)));
        let (result, tar_meta) = if usepkg {
            match use_tarball(paths, &r.package.name, &r.package.version)? {
                Some((res, meta)) => {
                    util::info(&format!("{} из готового тарбола", util::dim(&pf)));
                    (res, meta)
                }
                None => (build::run(r, paths, &paths.logs)?, None),
            }
        } else {
            (build::run(r, paths, &paths.logs)?, None)
        };
        if let Some(old) = db::lookup(&db, &r.package.name, Some(&r.package.slot)) {
            if old.version != r.package.version {
                crate::remove::uninstall_files(paths, &r.package.name, old)?;
            }
        }
        let slot = tar_meta
            .as_ref()
            .map(|m| m.slot.clone())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| r.package.slot.clone());
        let active = match &tar_meta {
            Some(m) if !m.use_flags.is_empty() => m.use_flags.clone(),
            _ => r.active_flags(&paths.use_flags, &paths.pkg_use),
        };
        merge(
            paths,
            &r.package.name,
            &r.package.version,
            &slot,
            &active,
            &result,
            &mut newdb,
        )?;
        util::info(&format!(
            "{} установлено, {} файлов, {}",
            util::green(&pf),
            result.files.len(),
            util::human_size(result.size)
        ));
        for m in &r.messages.post {
            println!("  {} {}", util::magenta("*"), m);
        }
    }
    db::save(&dbpath, &newdb)?;
    if !oneshot {
        let mut w = world::load(&paths.world);
        for t in targets {
            let d = util::parse_dep(t);
            let entry = match &d.slot {
                Some(s) => db::key(&d.name, s),
                None => d.name.clone(),
            };
            if !w.contains(&entry) {
                w.push(entry);
            }
        }
        world::save(&paths.world, &w)?;
    }
    util::ldconfig(&paths.root);
    Ok(())
}

pub fn upgrade(paths: &Paths) -> Result<()> {
    let w = world::load(&paths.world);
    if w.is_empty() {
        util::warn("world пуст, обновлять нечего");
        return Ok(());
    }
    util::info(&format!(
        "обновляю мир, {}",
        util::plural_ru(w.len(), "пакет", "пакета", "пакетов")
    ));
    run(paths, &w, false, true, false, false, false)
}

pub fn fetch_only(paths: &Paths, targets: &[String], force: bool) -> Result<()> {
    let dbpath = paths.db.join("installed.json");
    let db = db::load(&dbpath)?;
    let plan = resolver::resolve(paths, &db, targets, force, false)?;
    paths.ensure_dirs()?;
    for r in &plan.order {
        match &r.source {
            Some(src) => {
                let distfile = paths.distdir.join(build::file_name(&src.url));
                if distfile.exists() {
                    util::info(&format!("в кэше {}", distfile.display()));
                } else {
                    util::info(&format!("загрузка {}", src.url));
                    fetch::fetch(&src.url, &distfile, src.sha256.as_deref())?;
                }
            }
            None => util::info(&format!("{} без исходников", r.package.name)),
        }
    }
    Ok(())
}

pub fn quickpkg(paths: &Paths, names: &[String]) -> Result<()> {
    let dbpath = paths.db.join("installed.json");
    let db = db::load(&dbpath)?;
    paths.ensure_dirs()?;
    for n in names {
        let d = util::parse_dep(n);
        let Some(e) = db::lookup(&db, &d.name, d.slot.as_deref()) else {
            util::warn(&format!("{} не установлен", d.name));
            continue;
        };
        let pf = format!("{}-{}", d.name, e.version);
        let out = paths.pkgdir.join(format!("{}.ftar.tar.gz", pf));
        let list = paths.tmpdir.join(format!("{}.pkglist", pf));
        std::fs::write(&list, format!("{}\n", e.files.join("\n")))?;
        let plain = paths.tmpdir.join(format!("{}.ftar", pf));
        let _ = std::fs::remove_file(&plain);
        let st = std::process::Command::new("tar")
            .arg("-cf")
            .arg(&plain)
            .arg("-C")
            .arg(&paths.root)
            .arg("-T")
            .arg(&list)
            .status()?;
        if !st.success() {
            anyhow::bail!("tar не справился с {}", pf);
        }
        let meta = TarMeta {
            version: e.version.clone(),
            slot: e.slot.clone(),
            use_flags: e.use_flags.clone(),
            description: String::new(),
            license: String::new(),
            homepage: String::new(),
        };
        let meta_path = paths.tmpdir.join(META_NAME);
        std::fs::write(&meta_path, toml::to_string(&meta)?)?;
        let st = std::process::Command::new("tar")
            .arg("-rf")
            .arg(&plain)
            .arg("-C")
            .arg(&paths.tmpdir)
            .arg(META_NAME)
            .status()?;
        let _ = std::fs::remove_file(&meta_path);
        if !st.success() {
            anyhow::bail!("метаданные не легли в тарбол {}", pf);
        }
        let _ = std::fs::remove_file(&out);
        let st = std::process::Command::new("gzip")
            .arg("-f")
            .arg(&plain)
            .status()?;
        if !st.success() {
            anyhow::bail!("gzip не справился с {}", pf);
        }
        std::fs::rename(format!("{}.gz", plain.display()), &out)?;
        let size = std::fs::metadata(&out)?.len();
        util::info(&format!(
            "{} упакован в {} ({})",
            util::bold(&pf),
            out.display(),
            util::human_size(size)
        ));
        let _ = std::fs::remove_file(&list);
    }
    Ok(())
}

fn quickpkg_tarball(paths: &Paths, name: &str, version: &str) -> Option<std::path::PathBuf> {
    let pf = format!("{}-{}", name, version);
    let p = paths.pkgdir.join(format!("{}.ftar.tar.gz", pf));
    p.is_file().then_some(p)
}

fn use_tarball(
    paths: &Paths,
    name: &str,
    version: &str,
) -> Result<Option<(build::BuildResult, Option<TarMeta>)>> {
    let Some(tarball) = quickpkg_tarball(paths, name, version) else {
        return Ok(None);
    };
    let pf = format!("{}-{}", name, version);
    let dest = paths.tmpdir.join(format!("{}.bindest", pf));
    if dest.exists() {
        std::fs::remove_dir_all(&dest)?;
    }
    std::fs::create_dir_all(&dest)?;
    let out = std::process::Command::new("tar")
        .arg("-xzf")
        .arg(&tarball)
        .arg("-C")
        .arg(&dest)
        .output()
        .context("запускаю tar")?;
    if !out.status.success() {
        bail!(
            "тарбол {} не распаковывается: {}",
            tarball.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let meta_path = dest.join(META_NAME);
    let meta = std::fs::read_to_string(&meta_path)
        .ok()
        .and_then(|text| toml::from_str::<TarMeta>(&text).ok());
    let _ = std::fs::remove_file(&meta_path);
    Ok(Some((build::from_tree(&dest)?, meta)))
}

pub fn verify(paths: &Paths) -> Result<()> {
    let dbpath = paths.db.join("installed.json");
    let db = db::load(&dbpath)?;
    if db.is_empty() {
        util::info("ничего не установлено, проверять нечего");
        return Ok(());
    }
    let mut broken = 0usize;
    let mut checked = 0usize;
    for (k, e) in &db {
        let mut changed: Vec<&String> = Vec::new();
        let mut missing: Vec<&String> = Vec::new();
        for f in &e.files {
            let Some(want) = e.hashes.get(f) else {
                continue;
            };
            let p = paths.root.join(f);
            let m = std::fs::symlink_metadata(&p);
            match m {
                Err(_) => missing.push(f),
                Ok(m) if m.is_symlink() => {}
                Ok(_) => {
                    checked += 1;
                    let got = fetch::sha256_file(&p)?;
                    if got != want.to_lowercase() {
                        changed.push(f);
                    }
                }
            }
        }
        if changed.is_empty() && missing.is_empty() {
            util::info(&format!("{} цел", k));
        } else {
            broken += changed.len() + missing.len();
            util::warn(&format!(
                "{}: {} {}",
                k,
                util::plural_ru(
                    changed.len(),
                    "файл изменён",
                    "файла изменено",
                    "файлов изменено"
                ),
                if missing.is_empty() {
                    String::new()
                } else {
                    format!(
                        ", {} пропало",
                        util::plural_ru(missing.len(), "файл", "файла", "файлов")
                    )
                }
            ));
            for f in changed.iter().chain(missing.iter()).take(10) {
                println!("  {}", f);
            }
        }
    }
    util::info(&format!(
        "проверено {}, {}",
        util::plural_ru(checked, "файл", "файла", "файлов"),
        util::plural_ru(broken, "повреждение", "повреждения", "повреждений")
    ));
    if broken > 0 {
        bail!("целостность нарушена, {} повреждений", broken);
    }
    Ok(())
}

pub fn history(paths: &Paths, limit: usize) -> Result<()> {
    let dbpath = paths.db.join("installed.json");
    let db = db::load(&dbpath)?;
    if db.is_empty() {
        util::info("ничего не установлено");
        return Ok(());
    }
    let mut rows: Vec<(&String, &Installed)> = db.iter().collect();
    rows.sort_by(|a, b| b.1.built.cmp(&a.1.built));
    let shown = rows.len().min(limit);
    for (k, e) in rows.into_iter().take(shown) {
        let when = e
            .built
            .parse::<u64>()
            .map(util::epoch_utc)
            .unwrap_or_else(|_| e.built.clone());
        println!(
            "  {} {} {} {}",
            util::dim(&when),
            util::bold(k),
            e.version,
            util::dim(&format!(
                "{} файлов, {}",
                e.files.len(),
                util::human_size(e.size)
            ))
        );
    }
    Ok(())
}

fn collision_map(
    db: &Db,
    name: &str,
    slot: &str,
    files: &[String],
) -> BTreeMap<String, Vec<String>> {
    let mut hits: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let files_set: std::collections::BTreeSet<&String> = files.iter().collect();
    for (k, e) in db {
        if k == &db::key(name, slot) {
            continue;
        }
        for f in &e.files {
            if files_set.contains(f) {
                hits.entry(k.clone()).or_default().push(f.clone());
            }
        }
    }
    hits
}

fn collision_warnings(db: &Db, name: &str, slot: &str, files: &[String]) {
    for (k, mut list) in collision_map(db, name, slot, files) {
        list.sort();
        let tail: Vec<String> = list.iter().take(5).cloned().collect();
        let more = if list.len() > 5 {
            format!(" и ещё {}", list.len() - 5)
        } else {
            String::new()
        };
        util::warn(&format!(
            "{} перекрывает файлы {}: {}{}",
            db::key(name, slot),
            k,
            tail.join(", "),
            more
        ));
    }
}

fn file_hashes(paths: &Paths, result: &build::BuildResult) -> Result<BTreeMap<String, String>> {
    let mut m = BTreeMap::new();
    for f in &result.files {
        let p = paths.root.join(f);
        let Ok(md) = std::fs::symlink_metadata(&p) else {
            continue;
        };
        if md.is_symlink() {
            continue;
        }
        let sum = fetch::sha256_file(&p)?;
        m.insert(f.clone(), sum);
    }
    Ok(m)
}

fn timestamp() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", now.as_secs())
}

fn files_equal(a: &Path, b: &Path) -> bool {
    let (Ok(mut fa), Ok(mut fb)) = (std::fs::File::open(a), std::fs::File::open(b)) else {
        return false;
    };
    let mut ba = [0u8; 8192];
    let mut bb = [0u8; 8192];
    loop {
        match (fa.read(&mut ba), fb.read(&mut bb)) {
            (Ok(0), Ok(0)) => return true,
            (Ok(n), Ok(m)) if n == m && ba[..n] == bb[..m] => {}
            _ => return false,
        }
    }
}

fn merge(
    paths: &Paths,
    name: &str,
    version: &str,
    slot: &str,
    use_flags: &[String],
    result: &build::BuildResult,
    db: &mut Db,
) -> Result<()> {
    collision_warnings(db, name, slot, &result.files);
    for d in &result.dirs {
        std::fs::create_dir_all(paths.root.join(d))?;
    }
    let mut deferred: Vec<String> = Vec::new();
    for f in &result.files {
        let src = result.dest.join(f);
        let dst = paths.root.join(f);
        if let Some(p) = dst.parent() {
            std::fs::create_dir_all(p)?;
        }
        let protected = f.starts_with("etc/")
            && std::fs::symlink_metadata(&dst)
                .map(|m| m.is_file())
                .unwrap_or(false);
        if protected && files_equal(&src, &dst) {
            continue;
        }
        if protected {
            let newp = paths.root.join(format!("{}.flexura-new", f));
            std::fs::copy(&src, &newp)?;
            deferred.push(f.clone());
            continue;
        }
        if dst.is_symlink() || dst.exists() {
            std::fs::remove_file(&dst)?;
        }
        if src.is_symlink() {
            let target = std::fs::read_link(&src)?;
            std::os::unix::fs::symlink(target, &dst)?;
        } else {
            std::fs::copy(&src, &dst)?;
        }
    }
    if !deferred.is_empty() {
        for f in deferred.iter().take(5) {
            util::warn(&format!(
                "{} отличается от установленного, новый вариант в {}.flexura-new",
                f, f
            ));
        }
        if deferred.len() > 5 {
            util::warn(&format!("и ещё {} защищённых файлов", deferred.len() - 5));
        }
    }
    let hashes = file_hashes(paths, result)?;
    let inst = Installed {
        version: version.to_string(),
        files: result.files.clone(),
        dirs: result.dirs.clone(),
        use_flags: use_flags.to_vec(),
        built: timestamp(),
        size: result.size,
        slot: slot.to_string(),
        hashes,
    };
    db.insert(db::key(name, slot), inst);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Installed;

    fn entry(files: &[&str]) -> Installed {
        Installed {
            version: "1.0".into(),
            files: files.iter().map(|s| s.to_string()).collect(),
            dirs: Vec::new(),
            ..Default::default()
        }
    }

    #[test]
    fn collisions_skip_same_key() {
        let mut db = Db::new();
        db.insert(db::key("app", "0"), entry(&["usr/bin/app", "etc/app.conf"]));
        db.insert(
            db::key("lib", "0"),
            entry(&["usr/lib/liblib.so", "etc/app.conf"]),
        );
        let hits = collision_map(
            &db,
            "app",
            "0",
            &["usr/bin/app2".to_string(), "etc/other.conf".to_string()],
        );
        assert!(hits.is_empty());
        let hits = collision_map(
            &db,
            "app",
            "0",
            &[
                "usr/bin/app".to_string(),
                "etc/app.conf".to_string(),
                "usr/lib/liblib.so".to_string(),
            ],
        );
        assert_eq!(hits.len(), 1);
        assert!(hits.contains_key("lib:0"));
        assert!(hits["lib:0"].contains(&"usr/lib/liblib.so".to_string()));
    }

    #[test]
    fn hashes_skip_symlinks() {
        let base = std::env::temp_dir().join(format!("flexura-hash-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("root/usr/lib")).unwrap();
        let paths = Paths {
            root: base.join("root"),
            db: base.join("db"),
            world: base.join("db/world"),
            distdir: base.join("dist"),
            tmpdir: base.join("tmp"),
            pkgdir: base.join("pkg"),
            logs: base.join("log"),
            repos_raw: Vec::new(),
            repos: Vec::new(),
            jobs: 1,
            use_flags: Vec::new(),
            pkg_use: BTreeMap::new(),
            masks: Vec::new(),
            profile: None,
        };
        std::fs::write(paths.root.join("usr/lib/liba.so"), b"data").unwrap();
        std::os::unix::fs::symlink("liba.so", paths.root.join("usr/lib/liba.so.1")).unwrap();
        let result = build::BuildResult {
            dest: paths.root.clone(),
            files: vec![
                "usr/lib/liba.so".to_string(),
                "usr/lib/liba.so.1".to_string(),
            ],
            dirs: vec!["usr/lib".to_string()],
            size: 4,
        };
        let m = file_hashes(&paths, &result).unwrap();
        assert_eq!(m.len(), 1);
        assert!(m.contains_key("usr/lib/liba.so"));
        std::fs::remove_dir_all(&base).unwrap();
    }

    fn paths_in(base: &std::path::Path) -> Paths {
        Paths {
            root: base.join("root"),
            db: base.join("db"),
            world: base.join("db/world"),
            distdir: base.join("dist"),
            tmpdir: base.join("tmp"),
            pkgdir: base.join("pkg"),
            logs: base.join("log"),
            repos_raw: Vec::new(),
            repos: Vec::new(),
            jobs: 1,
            use_flags: Vec::new(),
            pkg_use: BTreeMap::new(),
            masks: Vec::new(),
            profile: None,
        }
    }

    #[test]
    fn tarball_keeps_meta() {
        let base = std::env::temp_dir().join(format!("flexura-meta-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let paths = paths_in(&base);
        std::fs::create_dir_all(paths.tmpdir.join("dest/usr/bin")).unwrap();
        std::fs::write(paths.tmpdir.join("dest/usr/bin/tool"), b"#!/bin/sh\n").unwrap();
        let db = Db::new();
        let mut newdb = db.clone();
        let result = build::from_tree(&paths.tmpdir.join("dest")).unwrap();
        merge(
            &paths,
            "tool",
            "1.0",
            "0",
            &["ssl".to_string()],
            &result,
            &mut newdb,
        )
        .unwrap();
        db::save(&paths.db.join("installed.json"), &newdb).unwrap();
        quickpkg(&paths, &["tool".to_string()]).unwrap();
        let (res, meta) = use_tarball(&paths, "tool", "1.0")
            .unwrap()
            .expect("тарбол должен быть");
        assert_eq!(res.files, vec!["usr/bin/tool".to_string()]);
        let meta = meta.expect("метаданные должны быть");
        assert_eq!(meta.version, "1.0");
        assert_eq!(meta.slot, "0");
        assert_eq!(meta.use_flags, vec!["ssl".to_string()]);
        assert!(!paths
            .tmpdir
            .join("tool-1.0.bindest")
            .join(META_NAME)
            .exists());
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn merge_shields_etc() {
        let base = std::env::temp_dir().join(format!("flexura-etc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let paths = paths_in(&base);
        std::fs::create_dir_all(paths.root.join("etc")).unwrap();
        std::fs::create_dir_all(paths.tmpdir.join("dest/etc")).unwrap();
        std::fs::write(paths.root.join("etc/app.conf"), b"user edit\n").unwrap();
        std::fs::write(paths.tmpdir.join("dest/etc/app.conf"), b"new default\n").unwrap();
        std::fs::write(paths.tmpdir.join("dest/etc/same.conf"), b"same\n").unwrap();
        std::fs::create_dir_all(paths.root.join("etc2")).unwrap();
        std::fs::write(paths.root.join("etc2/same.conf"), b"same\n").unwrap();
        let mut db = Db::new();
        let result = build::BuildResult {
            dest: paths.tmpdir.join("dest"),
            files: vec!["etc/app.conf".to_string(), "etc/same.conf".to_string()],
            dirs: vec!["etc".to_string()],
            size: 24,
        };
        merge(&paths, "app", "1.0", "0", &[], &result, &mut db).unwrap();
        assert_eq!(
            std::fs::read(paths.root.join("etc/app.conf")).unwrap(),
            b"user edit\n"
        );
        assert!(paths.root.join("etc/app.conf.flexura-new").exists());
        assert_eq!(
            std::fs::read(paths.root.join("etc/same.conf")).unwrap(),
            b"same\n"
        );
        assert!(!paths.root.join("etc/same.conf.flexura-new").exists());
        let entry = &db["app:0"];
        assert!(entry.hashes.contains_key("etc/app.conf"));
        std::fs::remove_dir_all(&base).unwrap();
    }
}
