use anyhow::Result;
use std::io::Write;

use crate::config::Paths;
use crate::db::{self, Installed};
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

pub fn run(paths: &Paths, targets: &[String], assume_yes: bool) -> Result<()> {
    let dbpath = paths.db.join("installed.json");
    let mut db = db::load(&dbpath)?;
    let mut selected: Vec<(String, Installed)> = Vec::new();
    for t in targets {
        let d = util::parse_dep(t);
        let hits: Vec<(String, Installed)> = db::matching(&db, &d.name, d.slot.as_deref())
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
    if selected.is_empty() {
        return Ok(());
    }
    for (k, e) in &selected {
        util::info(&format!(
            "к удалению {} {} {}",
            util::bold(k),
            util::dim(&e.version),
            util::dim(&format!("{} файлов", e.files.len()))
        ));
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
    Ok(())
}

pub fn depclean(paths: &Paths, assume_yes: bool) -> Result<()> {
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
    Ok(())
}
