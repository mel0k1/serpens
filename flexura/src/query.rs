use anyhow::Result;
use std::collections::BTreeSet;

use crate::config::Paths;
use crate::db;
use crate::recipe::{all_recipes, find_recipe_slot};
use crate::util;

pub fn search(paths: &Paths, query: &str) -> Result<()> {
    let installed = db::load(&paths.db.join("installed.json"))?;
    let q = query.to_lowercase();
    let mut n = 0;
    for r in all_recipes(&paths.repos) {
        if r.package.name.to_lowercase().contains(&q)
            || r.package.description.to_lowercase().contains(&q)
        {
            let mark = if db::lookup(&installed, &r.package.name, Some(&r.package.slot)).is_some() {
                util::green("*")
            } else {
                util::dim(" ")
            };
            let slot = if r.package.slot == "0" {
                String::new()
            } else {
                format!(":{}", r.package.slot)
            };
            println!(
                "{} {}{} {}",
                mark,
                util::bold(&format!("{}-{}", r.package.name, r.package.version)),
                util::dim(&slot),
                r.package.description
            );
            n += 1;
        }
    }
    util::info(&format!(
        "найдено {}",
        util::plural_ru(n, "совпадение", "совпадения", "совпадений")
    ));
    Ok(())
}

pub fn list(paths: &Paths, installed_only: bool) -> Result<()> {
    if installed_only {
        let installed = db::load(&paths.db.join("installed.json"))?;
        if installed.is_empty() {
            util::info("ничего не установлено");
            return Ok(());
        }
        for (k, e) in &installed {
            let (name, slot) = db::split_key(k);
            let slot_str = if slot == "0" {
                String::new()
            } else {
                format!(":{}", slot)
            };
            println!(
                "  {}{} {} {} {}",
                util::bold(name),
                util::dim(&slot_str),
                util::dim(&e.version),
                util::human_size(e.size),
                util::dim(&format!("{} файлов", e.files.len()))
            );
        }
        return Ok(());
    }
    let recipes = all_recipes(&paths.repos);
    if recipes.is_empty() {
        util::warn("дерево рецептов пусто, запусти flexura sync или поправь [main] repos");
        return Ok(());
    }
    for r in &recipes {
        println!(
            "  {} {}",
            util::bold(&format!("{}-{}", r.package.name, r.package.version)),
            r.package.description
        );
    }
    Ok(())
}

pub fn info_system(paths: &Paths) -> Result<()> {
    let installed = db::load(&paths.db.join("installed.json"))?;
    println!("{} {}", util::bold("корень:"), paths.root.display());
    println!(
        "{} {}",
        util::bold("профиль:"),
        paths
            .profile
            .clone()
            .unwrap_or_else(|| "не задан".to_string())
    );
    println!(
        "{} {}",
        util::bold("репозитории:"),
        if paths.repos_raw.is_empty() {
            "нет".to_string()
        } else {
            paths.repos_raw.join(", ")
        }
    );
    println!(
        "{} {}",
        util::bold("флаги USE:"),
        if paths.use_flags.is_empty() {
            "нет".to_string()
        } else {
            paths.use_flags.join(" ")
        }
    );
    println!("{} {}", util::bold("задач сборки:"), paths.jobs);
    println!("{} {}", util::bold("установлено пакетов:"), installed.len());
    println!(
        "{} {}",
        util::bold("кэш исходников:"),
        util::human_size(dir_size(&paths.distdir))
    );
    Ok(())
}

pub fn info_pkg(paths: &Paths, target: &str) -> Result<()> {
    let d = util::parse_dep(target);
    let (_, r) = find_recipe_slot(&paths.repos, &d.name, d.slot.as_deref())?;
    let installed = db::load(&paths.db.join("installed.json"))?;
    let slot_str = if r.package.slot == "0" {
        String::new()
    } else {
        format!(":{}", r.package.slot)
    };
    println!(
        "{} {}{}",
        util::bold(&r.package.name),
        util::magenta(&r.package.version),
        util::dim(&slot_str)
    );
    println!("  {}", r.package.description);
    if !r.package.homepage.is_empty() {
        println!("  {}", util::dim(&r.package.homepage));
    }
    if !r.package.license.is_empty() {
        println!("  лицензия: {}", r.package.license);
    }
    let active = r.active_flags(&paths.use_flags);
    if !active.is_empty() {
        println!("  USE: {}", active.join(" "));
    }
    let deps = r.deps_for(&active);
    if !deps.is_empty() {
        println!("  зависимости: {}", deps.join(" "));
    }
    if let Some(src) = &r.source {
        println!("  исходники: {}", src.url);
    }
    let cstr = util::constraint_str(&d.constraint);
    if !cstr.is_empty() {
        println!("  ограничение: {}", cstr);
    }
    match db::lookup(&installed, &r.package.name, d.slot.as_deref()) {
        Some(e) => println!(
            "  установлен: {} {}",
            util::green(&e.version),
            util::dim(&format!(
                "{} файлов, {}",
                e.files.len(),
                util::human_size(e.size)
            ))
        ),
        None => println!("  установлен: {}", util::yellow("нет")),
    }
    Ok(())
}

pub fn deps(paths: &Paths, target: &str) -> Result<()> {
    let mut seen = BTreeSet::new();
    walk_deps(paths, target, 0, &mut seen, "  ")?;
    Ok(())
}

fn walk_deps(
    paths: &Paths,
    spec: &str,
    depth: usize,
    seen: &mut BTreeSet<String>,
    prefix: &str,
) -> Result<()> {
    if depth > 12 {
        return Ok(());
    }
    let d = util::parse_dep(spec);
    let seen_id = format!("{}", d);
    if !seen.insert(seen_id) && depth > 0 {
        return Ok(());
    }
    let Ok((_, r)) = find_recipe_slot(&paths.repos, &d.name, d.slot.as_deref()) else {
        println!("{}{} (рецепта нет)", prefix, util::red(&d.name));
        return Ok(());
    };
    let cstr = util::constraint_str(&d.constraint);
    let slot_str = if r.package.slot == "0" {
        String::new()
    } else {
        format!(":{}", r.package.slot)
    };
    println!(
        "{}{}-{}{}{}",
        prefix,
        util::bold(&r.package.name),
        r.package.version,
        util::dim(&slot_str),
        if cstr.is_empty() {
            String::new()
        } else {
            format!(" {}", util::magenta(&cstr))
        }
    );
    let active = r.active_flags(&paths.use_flags);
    for dep in r.deps_for(&active) {
        let mut np = prefix.to_string();
        np.push_str("  ");
        walk_deps(paths, &dep, depth + 1, seen, &np)?;
    }
    Ok(())
}

pub fn file(paths: &Paths, path: &str) -> Result<()> {
    let installed = db::load(&paths.db.join("installed.json"))?;
    let norm = path.trim_start_matches('/').trim_end_matches('/');
    let mut n = 0;
    for (k, e) in &installed {
        let hit = e.files.iter().any(|f| f == norm);
        if hit {
            println!("{} {}", util::bold(k), util::dim(&e.version));
            n += 1;
        }
    }
    if n == 0 {
        util::warn("файл не принадлежит ни одному установленному пакету");
    }
    Ok(())
}

pub fn world(paths: &Paths) -> Result<()> {
    let installed = db::load(&paths.db.join("installed.json"))?;
    let w = crate::world::load(&paths.world);
    if w.is_empty() {
        util::warn("world пуст, пакеты попадают в него через install");
        return Ok(());
    }
    for entry in &w {
        let d = util::parse_dep(entry);
        match db::lookup(&installed, &d.name, d.slot.as_deref()) {
            Some(e) => println!(
                "  {} {} {}",
                util::bold(entry),
                util::dim(&e.version),
                util::dim(&format!("слот {}", e.slot))
            ),
            None => println!("  {} {}", util::bold(entry), util::yellow("не установлен")),
        }
    }
    Ok(())
}

fn dir_size(dir: &std::path::Path) -> u64 {
    let mut total = 0;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                total += dir_size(&p);
            } else if let Ok(m) = e.metadata() {
                total += m.len();
            }
        }
    }
    total
}
