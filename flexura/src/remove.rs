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
    let mut removed = Vec::new();
    for t in targets {
        let name = util::parse_dep(t).0;
        match db.get(&name) {
            Some(e) => removed.push((name.clone(), e.clone())),
            None => util::warn(&format!("{} не установлен", name)),
        }
    }
    if removed.is_empty() {
        return Ok(());
    }
    for (name, e) in &removed {
        util::info(&format!("к удалению {} {}", util::bold(name), util::dim(&e.version)));
    }
    if !assume_yes {
        print!("удалить {}? [y/N] ", util::plural_ru(removed.len(), "пакет", "пакета", "пакетов"));
        std::io::stdout().flush()?;
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        let l = line.trim().to_lowercase();
        if l != "y" && l != "yes" {
            util::info("отменено");
            return Ok(());
        }
    }
    for (name, e) in &removed {
        uninstall_files(paths, name, e)?;
        db.remove(name);
        util::info(&format!("удалён {}-{}", name, e.version));
    }
    db::save(&dbpath, &db)?;
    let mut w = world::load(&paths.world);
    w.retain(|x| !removed.iter().any(|(n, _)| n == x));
    world::save(&paths.world, &w)?;
    Ok(())
}

pub fn depclean(paths: &Paths, assume_yes: bool) -> Result<()> {
    let dbpath = paths.db.join("installed.json");
    let db = db::load(&dbpath)?;
    let w = world::load(&paths.world);
    let keep = resolver::required_set(paths, &db, &w)?;
    let mut candidates = Vec::new();
    for (name, e) in &db {
        if !keep.contains(name) {
            candidates.push((name.clone(), e.clone()));
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
        print!("удалить {}? [y/N] ", util::plural_ru(candidates.len(), "пакет", "пакета", "пакетов"));
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
    for (name, e) in &candidates {
        uninstall_files(paths, name, e)?;
        newdb.remove(name);
        util::info(&format!("удалён {}-{}", name, e.version));
    }
    db::save(&dbpath, &newdb)?;
    Ok(())
}
