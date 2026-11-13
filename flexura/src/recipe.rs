use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::util;

pub const DEFAULT_SLOT: &str = "0";

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Recipe {
    pub package: Package,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    #[serde(default)]
    pub dependencies: Deps,
    #[serde(default)]
    pub flags: RecipeFlags,
    #[serde(default)]
    pub build: Build,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub description: String,
    #[serde(default)]
    pub homepage: String,
    #[serde(default)]
    pub license: String,
    #[serde(default = "default_slot")]
    pub slot: String,
}

fn default_slot() -> String {
    DEFAULT_SLOT.to_string()
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Source {
    pub url: String,
    #[serde(default)]
    pub sha256: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct Deps {
    #[serde(default)]
    pub build: Vec<String>,
    #[serde(default)]
    pub runtime: Vec<String>,
    #[serde(default)]
    pub optional: BTreeMap<String, Vec<String>>,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct RecipeFlags {
    #[serde(default)]
    pub default: Vec<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct Build {
    #[serde(default)]
    pub prepare: Vec<String>,
    #[serde(default)]
    pub configure: Vec<String>,
    #[serde(default)]
    pub compile: Vec<String>,
    #[serde(default)]
    pub install: Vec<String>,
}

impl Recipe {
    pub fn load(path: &Path) -> Result<Recipe> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("читаю {}", path.display()))?;
        let r: Recipe =
            toml::from_str(&text).with_context(|| format!("разбираю {}", path.display()))?;
        Ok(r)
    }

    pub fn active_flags(&self, user_flags: &[String]) -> Vec<String> {
        let mut v = self.flags.default.clone();
        for f in user_flags {
            if !v.contains(f) {
                v.push(f.clone());
            }
        }
        v
    }

    pub fn deps_for(&self, active: &[String]) -> Vec<String> {
        let mut v = Vec::new();
        for d in self
            .dependencies
            .build
            .iter()
            .chain(self.dependencies.runtime.iter())
        {
            if !v.contains(d) {
                v.push(d.clone());
            }
        }
        for (flag, deps) in &self.dependencies.optional {
            if active.contains(flag) {
                for d in deps {
                    if !v.contains(d) {
                        v.push(d.clone());
                    }
                }
            }
        }
        v
    }

    pub fn runtime_deps_for(&self, active: &[String]) -> Vec<String> {
        let mut v = Vec::new();
        for d in &self.dependencies.runtime {
            if !v.contains(d) {
                v.push(d.clone());
            }
        }
        for (flag, deps) in &self.dependencies.optional {
            if active.contains(flag) {
                for d in deps {
                    if !v.contains(d) {
                        v.push(d.clone());
                    }
                }
            }
        }
        v
    }
}

pub fn find_recipe_slot(
    repos: &[PathBuf],
    name: &str,
    slot: Option<&str>,
) -> Result<(PathBuf, Recipe)> {
    for repo in repos {
        let mut found: Vec<Recipe> = Vec::new();
        walk_flx(repo, &mut |p| {
            if let Ok(r) = Recipe::load(p) {
                if r.package.name == name {
                    found.push(r);
                }
            }
        });
        if !found.is_empty() {
            let picked = pick(&found, slot)?;
            return Ok((repo.clone(), picked));
        }
    }
    bail!("рецепт {} не найден в репозиториях", name)
}

fn pick(candidates: &[Recipe], slot: Option<&str>) -> Result<Recipe> {
    let mut pool: Vec<&Recipe> = candidates.iter().collect();
    if let Some(s) = slot {
        pool.retain(|r| r.package.slot == s);
        if pool.is_empty() {
            bail!(
                "пакет {} есть, но без слота {}",
                candidates[0].package.name,
                s
            );
        }
    }
    pool.sort_by(|a, b| util::version_cmp(&b.package.version, &a.package.version));
    Ok(pool[0].clone())
}

pub fn all_recipes(repos: &[PathBuf]) -> Vec<Recipe> {
    let mut v = Vec::new();
    for repo in repos {
        walk_flx(repo, &mut |p| {
            if let Ok(r) = Recipe::load(p) {
                v.push(r);
            }
        });
    }
    v.sort_by(|a, b| a.package.name.cmp(&b.package.name));
    v
}

fn walk_flx(dir: &Path, f: &mut impl FnMut(&Path)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for p in paths {
        if p.is_dir() {
            walk_flx(&p, f);
        } else if p.extension().map(|x| x == "flx").unwrap_or(false) {
            f(&p);
        }
    }
}
