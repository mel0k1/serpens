use anyhow::Result;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::build;
use crate::config::Paths;
use crate::db::{self, Db, Installed};
use crate::fetch;
use crate::recipe::Recipe;
use crate::resolver;
use crate::util;
use crate::world;

pub fn run(paths: &Paths, targets: &[String], force: bool) -> Result<()> {
    if targets.is_empty() {
        util::warn("укажи хотя бы один пакет");
        return Ok(());
    }
    let dbpath = paths.db.join("installed.json");
    let db = db::load(&dbpath)?;
    let plan = resolver::resolve(paths, &db, targets, force)?;
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
        println!(
            "  {} {} {}{}",
            action,
            util::bold(&r.package.name),
            util::dim(&r.package.version),
            if r.package.slot == "0" {
                String::new()
            } else {
                util::dim(&format!(":{}", r.package.slot))
            }
        );
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
        let result = build::run(r, paths, &paths.logs)?;
        if let Some(old) = db::lookup(&db, &r.package.name, Some(&r.package.slot)) {
            if old.version != r.package.version {
                crate::remove::uninstall_files(paths, &r.package.name, old)?;
            }
        }
        merge(r, paths, &result, &mut newdb)?;
        util::info(&format!(
            "{} установлено, {} файлов, {}",
            util::green(&pf),
            result.files.len(),
            util::human_size(result.size)
        ));
    }
    db::save(&dbpath, &newdb)?;
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
    run(paths, &w, false)
}

pub fn fetch_only(paths: &Paths, targets: &[String], force: bool) -> Result<()> {
    let dbpath = paths.db.join("installed.json");
    let db = db::load(&dbpath)?;
    let plan = resolver::resolve(paths, &db, targets, force)?;
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
        let st = std::process::Command::new("tar")
            .arg("-czf")
            .arg(&out)
            .arg("-C")
            .arg(&paths.root)
            .arg("-T")
            .arg(&list)
            .status()?;
        if st.success() {
            let size = std::fs::metadata(&out)?.len();
            util::info(&format!(
                "{} упакован в {} ({})",
                util::bold(&pf),
                out.display(),
                util::human_size(size)
            ));
        } else {
            anyhow::bail!("tar не справился с {}", pf);
        }
        let _ = std::fs::remove_file(&list);
    }
    Ok(())
}

fn timestamp() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", now.as_secs())
}

fn merge(recipe: &Recipe, paths: &Paths, result: &build::BuildResult, db: &mut Db) -> Result<()> {
    for d in &result.dirs {
        std::fs::create_dir_all(paths.root.join(d))?;
    }
    for f in &result.files {
        let src = result.dest.join(f);
        let dst = paths.root.join(f);
        if let Some(p) = dst.parent() {
            std::fs::create_dir_all(p)?;
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
    let inst = Installed {
        version: recipe.package.version.clone(),
        files: result.files.clone(),
        dirs: result.dirs.clone(),
        use_flags: recipe.active_flags(&paths.use_flags),
        built: timestamp(),
        size: result.size,
        slot: recipe.package.slot.clone(),
    };
    db.insert(db::key(&recipe.package.name, &recipe.package.slot), inst);
    Ok(())
}
