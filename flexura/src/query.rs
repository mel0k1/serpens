use anyhow::Result;
use std::collections::BTreeSet;

use crate::config::Paths;
use crate::db;
use crate::recipe::{all_recipes, find_recipe};
use crate::util;

pub fn search(paths: &Paths, query: &str) -> Result<()> {
    let installed = db::load(&paths.db.join("installed.json"))?;
    let q = query.to_lowercase();
    let mut n = 0;
    for r in all_recipes(&paths.repos) {
        if r.package.name.to_lowercase().contains(&q)
            || r.package.description.to_lowercase().contains(&q)
        {
            let mark = if installed.contains_key(&r.package.name) {
                util::green("*")
            } else {
                util::dim(" ")
            };
            println!(
                "{} {} {}",
                mark,
                util::bold(&format!("{}-{}", r.package.name, r.package.version)),
                r.package.description
            );
            n += 1;
        }
    }
    util::info(&format!("найдено {}", util::plural_ru(n, "совпадение", "совпадения", "совпадений")));
    Ok(())
}

pub fn list(paths: &Paths, installed_only: bool) -> Result<()> {
    if installed_only {
        let installed = db::load(&paths.db.join("installed.json"))?;
        if installed.is_empty() {
            util::info("ничего не установлено");
            return Ok(());
        }
        for (name, e) in &installed {
            println!(
                "  {} {} {} {}",
                util::bold(name),
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
        println!("  {} {}", util::bold(&format!("{}-{}", r.package.name, r.package.version)), r.package.description);
    }
    Ok(())
}

pub fn info_system(paths: &Paths) -> Result<()> {
    let installed = db::load(&paths.db.join("installed.json"))?;
    println!("{} {}", util::bold("корень:"), paths.root.display());
    println!(
        "{} {}",
        util::bold("профиль:"),
        paths.profile.clone().unwrap_or_else(|| "не задан".to_string())
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
    let (name, c) = util::parse_dep(target);
    let (_, r) = find_recipe(&paths.repos, &name)?;
    let installed = db::load(&paths.db.join("installed.json"))?;
    println!(
        "{} {}",
        util::bold(&r.package.name),
        util::magenta(&r.package.version)
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
    let cstr = util::constraint_str(&c);
    if !cstr.is_empty() {
        println!("  ограничение: {}", cstr);
    }
    match installed.get(&name) {
        Some(e) => println!(
            "  установлен: {} {}",
            util::green(&e.version),
            util::dim(&format!("{} файлов, {}", e.files.len(), util::human_size(e.size)))
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
    let (name, c) = util::parse_dep(spec);
    if !seen.insert(format!("{}{}", name, util::constraint_str(&c))) && depth > 0 {
        return Ok(());
    }
    let Ok((_, r)) = find_recipe(&paths.repos, &name) else {
        println!("{}{} (рецепта нет)", prefix, util::red(&name));
        return Ok(());
    };
    let cstr = util::constraint_str(&c);
    println!(
        "{}{}-{}{}",
        prefix,
        util::bold(&r.package.name),
        r.package.version,
        if cstr.is_empty() {
            String::new()
        } else {
            format!(" {}", util::magenta(&cstr))
        }
    );
    let active = r.active_flags(&paths.use_flags);
    for d in r.deps_for(&active) {
        let mut np = prefix.to_string();
        np.push_str("  ");
        walk_deps(paths, &d, depth + 1, seen, &np)?;
    }
    Ok(())
}

pub fn file(paths: &Paths, path: &str) -> Result<()> {
    let installed = db::load(&paths.db.join("installed.json"))?;
    let norm = path.trim_start_matches('/').trim_end_matches('/');
    let mut n = 0;
    for (name, e) in &installed {
        let hit = e.files.iter().any(|f| f == norm);
        if hit {
            println!("{} {}", util::bold(name), util::dim(&e.version));
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
    for name in &w {
        match installed.get(name) {
            Some(e) => println!("  {} {}", util::bold(name), util::dim(&e.version)),
            None => println!("  {} {}", util::bold(name), util::yellow("не установлен")),
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

