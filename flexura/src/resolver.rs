use anyhow::{bail, Result};
use std::collections::{BTreeMap, BTreeSet};

use crate::config::Paths;
use crate::db::Db;
use crate::recipe::{find_recipe, Recipe};
use crate::util::{constraint_matches, parse_dep, Constraint};

pub struct Plan {
    pub order: Vec<Recipe>,
}

pub fn resolve(paths: &Paths, db: &Db, targets: &[String], force: bool) -> Result<Plan> {
    let mut state = State {
        planned: BTreeMap::new(),
        constraints: BTreeMap::new(),
        order: Vec::new(),
        force,
    };
    for t in targets {
        let (name, c) = parse_dep(t);
        state
            .constraints
            .entry(name.clone())
            .or_default()
            .push(c);
        if !state.planned.contains_key(&name) {
            state.visit(&name, paths, db)?;
        }
    }
    Ok(Plan {
        order: state.order,
    })
}

pub fn required_set(paths: &Paths, db: &Db, roots: &[String]) -> Result<BTreeSet<String>> {
    let mut keep = BTreeSet::new();
    for r in roots {
        visit_runtime(r, paths, db, &mut keep)?;
    }
    Ok(keep)
}

fn visit_runtime(
    spec: &str,
    paths: &Paths,
    db: &Db,
    keep: &mut BTreeSet<String>,
) -> Result<()> {
    let (name, c) = parse_dep(spec);
    if !keep.insert(name.clone()) {
        return Ok(());
    }
    let Some(entry) = db.get(&name) else {
        return Ok(());
    };
    if !constraint_matches(&entry.version, &c) {
        return Ok(());
    }
    if let Ok((_, recipe)) = find_recipe(&paths.repos, &name) {
        let active = recipe.active_flags(&paths.use_flags);
        for d in recipe.runtime_deps_for(&active) {
            visit_runtime(&d, paths, db, keep)?;
        }
    }
    Ok(())
}

struct State {
    planned: BTreeMap<String, Recipe>,
    constraints: BTreeMap<String, Vec<Constraint>>,
    order: Vec<Recipe>,
    force: bool,
}

impl State {
    fn visit(&mut self, name: &str, paths: &Paths, db: &Db) -> Result<()> {
        if let Some(r) = self.planned.get(name) {
            self.check(name, &r.package.version)?;
            return Ok(());
        }
        if !self.force {
            if let Some(entry) = db.get(name) {
                if let Some(cs) = self.constraints.get(name) {
                    if cs.iter().all(|c| constraint_matches(&entry.version, c)) {
                        return Ok(());
                    }
                }
            }
        }
        let (_, recipe) = find_recipe(&paths.repos, name)?;
        let version = recipe.package.version.clone();
        self.check(name, &version)?;
        self.planned
            .insert(name.to_string(), recipe.clone());
        let active = recipe.active_flags(&paths.use_flags);
        for d in recipe.deps_for(&active) {
            let (dn, dc) = parse_dep(&d);
            self.constraints.entry(dn.clone()).or_default().push(dc);
            self.visit(&dn, paths, db)?;
        }
        self.order.push(recipe);
        Ok(())
    }

    fn check(&self, name: &str, version: &str) -> Result<()> {
        if let Some(cs) = self.constraints.get(name) {
            for c in cs {
                if !constraint_matches(version, c) {
                    bail!(
                        "{}-{} не удовлетворяет ограничению {}",
                        name,
                        version,
                        crate::util::constraint_str(c)
                    );
                }
            }
        }
        Ok(())
    }
}
