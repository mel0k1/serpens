use anyhow::{bail, Result};
use std::collections::BTreeSet;
use std::io::Write;

use crate::config::Paths;
use crate::db::{self, Db, Installed};
use crate::recipe::find_recipe_slot;
use crate::resolver;
use crate::util;
use crate::world;

pub fn uninstall_files(paths: &Paths, _name: &str, entry: &Installed) -> Result<()> {
    for f in &entry.files {
        let p = paths.root.join(f);
        if p.exists() {
            std::fs::remove_file(&p)?;
        }
    }
    let mut dirs: Vec<&String> = entry.dirs.iter().collect();
    dirs.sort();
    dirs.reverse();
    for d in dirs {
        let p = paths.root.join(d);
        if p.is_dir() {
            let _ = std::fs::remove_dir(&p);
        }
    }
    Ok(())
}

fn collect_selection(db: &Db, targets: &[String]) -> Vec<(String, Installed)> {
    let mut selected: Vec<(String, Installed)> = Vec::new();
    for t in targets {
        let d = util::parse_dep(t);
        let hits: Vec<(String, Installed)> = db::matching(db, &d.name, d.slot.as_deref())
            .into_iter()
            .map(|(k, e)| (k.clone(), e.clone()))
            .collect();
        if hits.is_empty() {
            util::warn(&format!("{} не установлен", d.name));
        }
        for (k, e) in hits {
            if !selected.iter().any(|(sk, _)| sk == &k) {
                selected.push((k, e));
            }
        }
    }
    selected
}

fn required_by(paths: &Paths, db: &Db, targets: &[String]) -> Result<Vec<(String, Vec<String>)>> {
    let mut rdeps: BTreeSet<String> = BTreeSet::new();
    for t in targets {
        let d = util::parse_dep(t);
        for (k, _) in db::matching(db, &d.name, d.slot.as_deref()) {
            rdeps.insert(k.clone());
        }
    }
    if rdeps.is_empty() {
        return Ok(Vec::new());
    }
    let mut report: Vec<(String, Vec<String>)> = Vec::new();
    for k in db.keys() {
        if rdeps.contains(k) {
            continue;
        }
        let (name, slot) = db::split_key(k);
        let Ok((_, r)) = find_recipe_slot(&paths.repos, name, Some(slot)) else {
            continue;
        };
        let active = r.active_flags(&paths.use_flags, &paths.pkg_use);
        let mut needs: Vec<String> = Vec::new();
        for dep in r.runtime_deps_for(&active) {
            let d = util::parse_dep(&dep);
            let hit = match d.slot.as_deref() {
                Some(s) => rdeps.contains(&db::key(&d.name, s)),
                None => rdeps.iter().any(|x| db::split_key(x).0 == d.name),
            };
            if hit {
                needs.push(dep);
            }
        }
        if !needs.is_empty() {
            report.push((k.clone(), needs));
        }
    }
    Ok(report)
}

pub fn run(
    paths: &Paths,
    targets: &[String],
    assume_yes: bool,
    pretend: bool,
    force: bool,
) -> Result<()> {
    let dbpath = paths.db.join("installed.json");
    let mut db = db::load(&dbpath)?;
    let selected = collect_selection(&db, targets);
    if selected.is_empty() {
        return Ok(());
    }
    let needs = required_by(paths, &db, targets)?;
    for (k, e) in &selected {
        util::info(&format!(
            "к удалению {} {} {}",
            util::bold(k),
            util::dim(&e.version),
            util::dim(&format!("{} файлов", e.files.len()))
        ));
    }
    if !needs.is_empty() {
        for (k, deps) in &needs {
            util::warn(&format!("{} нужен для {}", util::bold(k), deps.join(" ")));
        }
        if !force && !pretend {
            bail!(
                "есть зависимые пакеты, отказ; подтверди --force или почисти зависимости через depclean"
            );
        }
    }
    if pretend {
        util::info("черновик, ничего не удалено");
        return Ok(());
    }
    if !assume_yes {
        print!(
            "удалить {}? [y/N] ",
            util::plural_ru(selected.len(), "пакет", "пакета", "пакетов")
        );
        std::io::stdout().flush()?;
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        let l = line.trim().to_lowercase();
        if l != "y" && l != "yes" {
            util::info("отменено");
            return Ok(());
        }
    }
    let mut removed_names = Vec::new();
    for (k, e) in &selected {
        uninstall_files(paths, k, e)?;
        db.remove(k);
        removed_names.push(k.clone());
        util::info(&format!("удалён {}-{}", k, e.version));
    }
    db::save(&dbpath, &db)?;
    let mut w = world::load(&paths.world);
    w.retain(|x| !removed_names.contains(x));
    world::save(&paths.world, &w)?;
    util::ldconfig(&paths.root);
    Ok(())
}

pub fn depclean(paths: &Paths, assume_yes: bool, pretend: bool) -> Result<()> {
    let dbpath = paths.db.join("installed.json");
    let db = db::load(&dbpath)?;
    let w = world::load(&paths.world);
    let keep = resolver::required_set(paths, &db, &w)?;
    let mut candidates = Vec::new();
    for (k, e) in &db {
        if !keep.contains(k) {
            candidates.push((k.clone(), e.clone()));
        }
    }
    if candidates.is_empty() {
        util::info("осиротевших пакетов нет");
        return Ok(());
    }
    for (name, e) in &candidates {
        println!("  {} {}", util::bold(name), util::dim(&e.version));
    }
    if pretend {
        util::info("черновик, ничего не удалено");
        return Ok(());
    }
    if !assume_yes {
        print!(
            "удалить {}? [y/N] ",
            util::plural_ru(candidates.len(), "пакет", "пакета", "пакетов")
        );
        std::io::stdout().flush()?;
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        let l = line.trim().to_lowercase();
        if l != "y" && l != "yes" {
            util::info("отменено");
            return Ok(());
        }
    }
    let mut newdb = db.clone();
    for (k, e) in &candidates {
        uninstall_files(paths, k, e)?;
        newdb.remove(k);
        util::info(&format!("удалён {}-{}", k, e.version));
    }
    db::save(&dbpath, &newdb)?;
    util::ldconfig(&paths.root);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Paths;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn write(path: &std::path::Path, text: &str) {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p).unwrap();
        }
        std::fs::write(path, text).unwrap();
    }

    fn recipe_toml(name: &str, version: &str, slot: &str, deps: &str) -> String {
        format!(
            "[package]\nname = \"{}\"\nversion = \"{}\"\nslot = \"{}\"\ndescription = \"x\"\n\n[dependencies]\n{}\n",
            name, version, slot, deps
        )
    }

    fn setup(tag: &str) -> (Paths, Db, PathBuf) {
        let base =
            std::env::temp_dir().join(format!("flexura-remove-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let repo = base.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let paths = Paths {
            root: base.clone(),
            db: base.join("var/db/flexura"),
            world: base.join("var/db/flexura/world"),
            distdir: base.join("var/cache/dist"),
            tmpdir: base.join("var/tmp"),
            pkgdir: base.join("var/cache/pkg"),
            logs: base.join("var/log"),
            repos_raw: vec![repo.display().to_string()],
            repos: vec![repo.clone()],
            jobs: 1,
            use_flags: Vec::new(),
            pkg_use: BTreeMap::new(),
            masks: Vec::new(),
            profile: None,
        };
        (paths, Db::new(), base)
    }

    fn install(db: &mut Db, name: &str, slot: &str, version: &str) {
        db.insert(
            db::key(name, slot),
            Installed {
                version: version.to_string(),
                slot: slot.to_string(),
                ..Default::default()
            },
        );
    }

    #[test]
    fn required_by_flags_dependents() {
        let (paths, mut db, base) = setup("rdep");
        write(
            &base.join("repo/libs/zlib.flx"),
            &recipe_toml("zlib", "1.3.1", "0", ""),
        );
        write(
            &base.join("repo/tools/app.flx"),
            &recipe_toml("app", "1.0", "0", "runtime = [\"zlib>=1.0\"]"),
        );
        install(&mut db, "zlib", "0", "1.3.1");
        install(&mut db, "app", "0", "1.0");
        install(&mut db, "lonely", "0", "1.0");
        let report = required_by(&paths, &db, &["zlib".into()]).unwrap();
        assert_eq!(report.len(), 1);
        assert_eq!(report[0].0, "app:0");
        let none = required_by(&paths, &db, &["lonely".into()]).unwrap();
        assert!(none.is_empty());
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn required_by_slot_specific() {
        let (paths, mut db, base) = setup("rdepslot");
        write(
            &base.join("repo/libs/openssl-3.flx"),
            &recipe_toml("openssl", "3.3.1", "0", ""),
        );
        write(
            &base.join("repo/libs/openssl-11.flx"),
            &recipe_toml("openssl", "1.1.1w", "1.1", ""),
        );
        write(
            &base.join("repo/tools/legacy.flx"),
            &recipe_toml("legacy", "1.0", "0", "runtime = [\"openssl:1.1\"]"),
        );
        write(
            &base.join("repo/tools/modern.flx"),
            &recipe_toml("modern", "1.0", "0", "runtime = [\"openssl:0\"]"),
        );
        install(&mut db, "openssl", "0", "3.3.1");
        install(&mut db, "openssl", "1.1", "1.1.1w");
        install(&mut db, "legacy", "0", "1.0");
        install(&mut db, "modern", "0", "1.0");
        let report = required_by(&paths, &db, &["openssl:1.1".into()]).unwrap();
        assert_eq!(report.len(), 1);
        assert_eq!(report[0].0, "legacy:0");
        std::fs::remove_dir_all(&base).unwrap();
    }
}
