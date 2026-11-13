use anyhow::{bail, Result};
use std::collections::{BTreeMap, BTreeSet};

use crate::config::Paths;
use crate::db::{self, Db};
use crate::recipe::{find_recipe_slot, Recipe, DEFAULT_SLOT};
use crate::util::{constraint_matches, parse_dep, Blocker, Constraint, Dep};

#[derive(Debug)]
pub struct Plan {
    pub order: Vec<Recipe>,
}

pub fn resolve(
    paths: &Paths,
    db: &Db,
    targets: &[String],
    force: bool,
    upgrade: bool,
) -> Result<Plan> {
    let mut state = State {
        planned: BTreeMap::new(),
        constraints: BTreeMap::new(),
        blockers: Vec::new(),
        order: Vec::new(),
        force,
    };
    for t in targets {
        let d = parse_dep(t);
        if d.is_blocker() {
            bail!("блокер {} нельзя ставить как цель установки", d);
        }
        if upgrade {
            let (_, best) =
                crate::recipe::find_recipe_slot(&paths.repos, &d.name, d.slot.as_deref())?;
            if !db::matching(db, &best.package.name, Some(&best.package.slot))
                .iter()
                .any(|(_, e)| e.version == best.package.version)
            {
                state
                    .constraints
                    .entry((d.name.clone(), d.slot.clone()))
                    .or_default()
                    .push(d.constraint.clone());
                state.plan_recipe(&best, paths, db)?;
            }
            continue;
        }
        state
            .constraints
            .entry((d.name.clone(), d.slot.clone()))
            .or_default()
            .push(d.constraint.clone());
        state.visit(&d, paths, db)?;
    }
    state.check_blockers(db)?;
    Ok(Plan { order: state.order })
}

pub fn required_set(paths: &Paths, db: &Db, roots: &[String]) -> Result<BTreeSet<String>> {
    let mut keep = BTreeSet::new();
    for r in roots {
        visit_runtime(r, paths, db, &mut keep)?;
    }
    Ok(keep)
}

fn visit_runtime(spec: &str, paths: &Paths, db: &Db, keep: &mut BTreeSet<String>) -> Result<()> {
    let d = parse_dep(spec);
    if d.is_blocker() {
        return Ok(());
    }
    for (k, entry) in db::matching(db, &d.name, d.slot.as_deref()) {
        if !constraint_matches(&entry.version, &d.constraint) {
            continue;
        }
        if !keep.insert(k.clone()) {
            continue;
        }
        let (_, slot) = db::split_key(k);
        let recipe = match find_recipe_slot(&paths.repos, &d.name, Some(slot)) {
            Ok((_, r)) => Some(r),
            Err(_) => find_recipe_slot(&paths.repos, &d.name, None)
                .ok()
                .map(|(_, r)| r),
        };
        if let Some(r) = recipe {
            let active = r.active_flags(&paths.use_flags);
            for dep in r.runtime_deps_for(&active) {
                visit_runtime(&dep, paths, db, keep)?;
            }
        }
    }
    Ok(())
}

struct State {
    planned: BTreeMap<(String, String), Recipe>,
    constraints: BTreeMap<(String, Option<String>), Vec<Constraint>>,
    blockers: Vec<Dep>,
    order: Vec<Recipe>,
    force: bool,
}

impl State {
    fn satisfied_installed(&self, dep: &Dep, db: &Db) -> bool {
        for (_, entry) in db::matching(db, &dep.name, dep.slot.as_deref()) {
            if constraint_matches(&entry.version, &dep.constraint)
                && self.satisfied_by_constraints(&dep.name, dep.slot.as_deref(), &entry.version)
            {
                return true;
            }
        }
        false
    }

    fn satisfied_planned(&self, dep: &Dep) -> bool {
        for ((name, slot), r) in &self.planned {
            if name != &dep.name {
                continue;
            }
            if let Some(s) = &dep.slot {
                if s != slot {
                    continue;
                }
            }
            if constraint_matches(&r.package.version, &dep.constraint)
                && self.satisfied_by_constraints(name, Some(slot), &r.package.version)
            {
                return true;
            }
        }
        false
    }

    fn satisfied_by_constraints(&self, name: &str, slot: Option<&str>, version: &str) -> bool {
        if let Some(cs) = self.constraints.get(&(name.to_string(), None)) {
            for c in cs {
                if !constraint_matches(version, c) {
                    return false;
                }
            }
        }
        if let Some(s) = slot {
            if let Some(cs) = self
                .constraints
                .get(&(name.to_string(), Some(s.to_string())))
            {
                for c in cs {
                    if !constraint_matches(version, c) {
                        return false;
                    }
                }
            }
        }
        true
    }

    fn visit(&mut self, dep: &Dep, paths: &Paths, db: &Db) -> Result<()> {
        if dep.is_blocker() {
            self.blockers.push(dep.clone());
            return Ok(());
        }
        if !self.force && (self.satisfied_installed(dep, db) || self.satisfied_planned(dep)) {
            return Ok(());
        }
        let (_, recipe) = find_recipe_slot(&paths.repos, &dep.name, dep.slot.as_deref())?;
        self.plan_recipe(&recipe, paths, db)
    }

    fn plan_recipe(&mut self, recipe: &Recipe, paths: &Paths, db: &Db) -> Result<()> {
        self.check(
            &recipe.package.name,
            Some(&recipe.package.slot),
            &recipe.package.version,
        )?;
        let pkey = (recipe.package.name.clone(), recipe.package.slot.clone());
        if self.planned.contains_key(&pkey) {
            return Ok(());
        }
        self.planned.insert(pkey.clone(), recipe.clone());
        let active = recipe.active_flags(&paths.use_flags);
        for d in recipe.deps_for(&active) {
            let dd = parse_dep(&d);
            if !dd.is_blocker() {
                self.constraints
                    .entry((dd.name.clone(), dd.slot.clone()))
                    .or_default()
                    .push(dd.constraint.clone());
            }
            self.visit(&dd, paths, db)?;
        }
        let r = self.planned.get(&pkey).cloned().unwrap();
        self.order.push(r);
        Ok(())
    }

    fn check(&self, name: &str, slot: Option<&str>, version: &str) -> Result<()> {
        let mut probe: Vec<Option<String>> = vec![None];
        if let Some(s) = slot {
            probe.push(Some(s.to_string()));
        }
        for key_slot in probe {
            if let Some(cs) = self.constraints.get(&(name.to_string(), key_slot.clone())) {
                for c in cs {
                    if !constraint_matches(version, c) {
                        bail!(
                            "{}-{} (слот {}) не удовлетворяет ограничению {}",
                            name,
                            version,
                            key_slot.as_deref().unwrap_or(DEFAULT_SLOT),
                            crate::util::constraint_str(c)
                        );
                    }
                }
            }
        }
        Ok(())
    }

    fn check_blockers(&self, db: &Db) -> Result<()> {
        for b in &self.blockers {
            for ((name, slot), r) in &self.planned {
                if name != &b.name {
                    continue;
                }
                if let Some(s) = &b.slot {
                    if s != slot {
                        continue;
                    }
                }
                if constraint_matches(&r.package.version, &b.constraint) {
                    bail!(
                        "блокер {} конфликтует с {}-{} (слот {}) в этой же транзакции",
                        b,
                        name,
                        r.package.version,
                        slot
                    );
                }
            }
            if b.blocker == Blocker::Hard {
                for (_, entry) in db::matching(db, &b.name, b.slot.as_deref()) {
                    if constraint_matches(&entry.version, &b.constraint) {
                        bail!(
                            "блокер {} конфликтует с установленным {}-{} (слот {}), удалите его через remove",
                            b,
                            b.name,
                            entry.version,
                            entry.slot
                        );
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Paths;
    use crate::db::Installed;
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
            std::env::temp_dir().join(format!("flexura-resolver-{}-{}", tag, std::process::id()));
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
    fn plan_respects_slots() {
        let (paths, db, base) = setup("slots");
        write(
            &base.join("repo/libs/openssl-3.flx"),
            &recipe_toml("openssl", "3.3.1", "3.0", "runtime = [\"zlib>=1.2\"]"),
        );
        write(
            &base.join("repo/libs/openssl-11.flx"),
            &recipe_toml("openssl", "1.1.1w", "1.1", "runtime = [\"zlib>=1.2\"]"),
        );
        write(
            &base.join("repo/libs/zlib.flx"),
            &recipe_toml("zlib", "1.3.1", "0", ""),
        );
        write(
            &base.join("repo/tools/app.flx"),
            &recipe_toml("app", "1.0", "0", "build = [\"openssl:1.1\"]"),
        );
        let plan = resolve(&paths, &db, &["app".into()], false, false).unwrap();
        let names: Vec<&str> = plan.order.iter().map(|r| r.package.name.as_str()).collect();
        assert_eq!(names, vec!["zlib", "openssl", "app"]);
        assert_eq!(plan.order[1].package.slot, "1.1");
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn missing_slot_fails() {
        let (paths, db, base) = setup("noslot");
        write(
            &base.join("repo/libs/openssl.flx"),
            &recipe_toml("openssl", "3.3.1", "3.0", ""),
        );
        let err = resolve(&paths, &db, &["openssl:1.1".into()], false, false).unwrap_err();
        assert!(err.to_string().contains("1.1"));
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn installed_slot_satisfies_dep() {
        let (paths, mut db, base) = setup("sat");
        write(
            &base.join("repo/libs/zlib.flx"),
            &recipe_toml("zlib", "1.3.1", "0", ""),
        );
        write(
            &base.join("repo/tools/app.flx"),
            &recipe_toml("app", "1.0", "0", "build = [\"zlib>=1.0:0\"]"),
        );
        install(&mut db, "zlib", "0", "1.3.1");
        let plan = resolve(&paths, &db, &["app".into()], false, false).unwrap();
        assert_eq!(plan.order.len(), 1);
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn soft_blocker_conflicts_in_transaction() {
        let (paths, db, base) = setup("soft");
        write(
            &base.join("repo/old.flx"),
            &recipe_toml("oldmail", "1.0", "0", ""),
        );
        write(
            &base.join("repo/new.flx"),
            &recipe_toml("newmail", "2.0", "0", "build = [\"!oldmail\"]"),
        );
        let err = resolve(
            &paths,
            &db,
            &["newmail".into(), "oldmail".into()],
            false,
            false,
        )
        .unwrap_err();
        assert!(err.to_string().contains("блокер"));
        let plan = resolve(&paths, &db, &["newmail".into()], false, false).unwrap();
        assert_eq!(plan.order.len(), 1);
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn hard_blocker_conflicts_with_installed() {
        let (paths, mut db, base) = setup("hard");
        write(
            &base.join("repo/new.flx"),
            &recipe_toml("newmail", "2.0", "0", "build = [\"!!oldmail>=1\"]"),
        );
        install(&mut db, "oldmail", "0", "1.0");
        let err = resolve(&paths, &db, &["newmail".into()], false, false).unwrap_err();
        assert!(err.to_string().contains("установленным"));
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn soft_blocker_ignores_installed() {
        let (paths, mut db, base) = setup("softinst");
        write(
            &base.join("repo/new.flx"),
            &recipe_toml("newmail", "2.0", "0", "build = [\"!oldmail\"]"),
        );
        install(&mut db, "oldmail", "0", "1.0");
        let plan = resolve(&paths, &db, &["newmail".into()], false, false).unwrap();
        assert_eq!(plan.order.len(), 1);
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn version_conflict_bails() {
        let (paths, db, base) = setup("conflict");
        write(
            &base.join("repo/libs/ncurses.flx"),
            &recipe_toml("ncurses", "6.5", "6", ""),
        );
        write(
            &base.join("repo/tools/a.flx"),
            &recipe_toml("a", "1.0", "0", "build = [\"ncurses>=6.0\"]"),
        );
        write(
            &base.join("repo/tools/b.flx"),
            &recipe_toml("b", "1.0", "0", "build = [\"ncurses<6.0\"]"),
        );
        let err = resolve(&paths, &db, &["a".into(), "b".into()], false, false).unwrap_err();
        assert!(err.to_string().contains("ограничению"));
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn depclean_keeps_runtime_chain() {
        let (paths, mut db, base) = setup("keep");
        write(
            &base.join("repo/libs/zlib.flx"),
            &recipe_toml("zlib", "1.3.1", "0", ""),
        );
        write(
            &base.join("repo/tools/app.flx"),
            &recipe_toml("app", "1.0", "0", "runtime = [\"zlib>=1.0\"]"),
        );
        install(&mut db, "app", "0", "1.0");
        install(&mut db, "zlib", "0", "1.3.1");
        install(&mut db, "extra", "0", "9.9");
        write(
            &base.join("repo/extra.flx"),
            &recipe_toml("extra", "9.9", "0", ""),
        );
        let keep = required_set(&paths, &db, &["app".into()]).unwrap();
        assert!(keep.contains("app:0"));
        assert!(keep.contains("zlib:0"));
        assert!(!keep.contains("extra:0"));
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn upgrade_picks_best_candidate() {
        let (paths, mut db, base) = setup("upgrade");
        write(
            &base.join("repo/app-10.flx"),
            &recipe_toml("app", "1.0", "0", "runtime = [\"zlib>=1.0\"]"),
        );
        write(
            &base.join("repo/app-11.flx"),
            &recipe_toml("app", "1.1", "0", "runtime = [\"zlib>=1.2\"]"),
        );
        write(
            &base.join("repo/libs/zlib.flx"),
            &recipe_toml("zlib", "1.3.1", "0", ""),
        );
        install(&mut db, "app", "0", "1.0");
        let same = resolve(&paths, &db, &["app".into()], false, false).unwrap();
        assert!(same.order.is_empty());
        let mut udb = db.clone();
        install(&mut udb, "zlib", "0", "1.2.0");
        let plan = resolve(&paths, &udb, &["app".into()], false, true).unwrap();
        assert_eq!(plan.order.len(), 1);
        assert_eq!(plan.order[0].package.version, "1.1");
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn upgrade_skips_current_and_pulls_missing_deps() {
        let (paths, mut db, base) = setup("upcur");
        write(
            &base.join("repo/app-11.flx"),
            &recipe_toml("app", "1.1", "0", "runtime = [\"zlib>=1.2\"]"),
        );
        write(
            &base.join("repo/libs/zlib.flx"),
            &recipe_toml("zlib", "1.3.1", "0", ""),
        );
        install(&mut db, "app", "0", "1.1");
        let cur = resolve(&paths, &db, &["app".into()], false, true).unwrap();
        assert!(cur.order.is_empty());
        let fresh = Db::new();
        let plan = resolve(&paths, &fresh, &["app".into()], false, true).unwrap();
        assert_eq!(plan.order.len(), 2);
        assert_eq!(plan.order[0].package.name, "zlib");
        assert_eq!(plan.order[1].package.name, "app");
        std::fs::remove_dir_all(&base).unwrap();
    }
}
