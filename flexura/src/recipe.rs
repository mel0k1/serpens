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
    #[serde(default)]
    pub messages: Messages,
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

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct Messages {
    #[serde(default)]
    pub post: Vec<String>,
}

impl Recipe {
    pub fn load(path: &Path) -> Result<Recipe> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("читаю {}", path.display()))?;
        let r: Recipe =
            toml::from_str(&text).with_context(|| format!("разбираю {}", path.display()))?;
        Ok(r)
    }

    pub fn active_flags(
        &self,
        user_flags: &[String],
        pkg_use: &BTreeMap<String, Vec<String>>,
    ) -> Vec<String> {
        let mut v = self.flags.default.clone();
        for f in user_flags {
            if !v.contains(f) {
                v.push(f.clone());
            }
        }
        let mut extra: Vec<String> = Vec::new();
        for (key, flags) in pkg_use {
            let (kname, kslot) = match key.rfind(':') {
                Some(i) => (&key[..i], Some(&key[i + 1..])),
                None => (key.as_str(), None),
            };
            if kname != self.package.name {
                continue;
            }
            if let Some(s) = kslot {
                if s != self.package.slot {
                    continue;
                }
            }
            extra.extend(flags.iter().cloned());
        }
        for f in extra {
            if let Some(neg) = f.strip_prefix('-') {
                v.retain(|x| x != neg);
            } else {
                let pos = f.strip_prefix('+').unwrap_or(&f);
                if !v.contains(&pos.to_string()) {
                    v.push(pos.to_string());
                }
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
    find_recipe_slot_masked(repos, name, slot, &[])
}

pub fn find_recipe_slot_masked(
    repos: &[PathBuf],
    name: &str,
    slot: Option<&str>,
    masks: &[String],
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
            let picked = pick(&found, slot, masks)?;
            return Ok((repo.clone(), picked));
        }
    }
    bail!("рецепт {} не найден в репозиториях", name)
}

pub fn is_masked(masks: &[String], name: &str, slot: &str, version: &str) -> bool {
    masks.iter().any(|m| {
        let d = util::parse_dep(m);
        d.name == name
            && d.slot.as_deref().map(|s| s == slot).unwrap_or(true)
            && util::constraint_matches(version, &d.constraint)
    })
}

fn pick(candidates: &[Recipe], slot: Option<&str>, masks: &[String]) -> Result<Recipe> {
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
    if !masks.is_empty() {
        let open: Vec<&Recipe> = pool
            .iter()
            .copied()
            .filter(|r| !is_masked(masks, &r.package.name, &r.package.slot, &r.package.version))
            .collect();
        if open.is_empty() {
            bail!(
                "все кандидаты {} замаскированы в package.mask",
                candidates[0].package.name
            );
        }
        pool = open;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn recipe(name: &str, slot: &str, defaults: &[&str]) -> Recipe {
        toml::from_str(&format!(
            "[package]\nname = \"{}\"\nversion = \"1.0\"\nslot = \"{}\"\ndescription = \"x\"\n\n[flags]\ndefault = {:?}\n",
            name, slot, defaults
        ))
        .unwrap()
    }

    #[test]
    fn pkg_use_overrides() {
        let r = recipe("openssl", "0", &["ssl", "man"]);
        let mut pu = BTreeMap::new();
        pu.insert(
            "openssl".to_string(),
            vec!["-man".to_string(), "+zlib".to_string()],
        );
        pu.insert("openssl:9".to_string(), vec!["other".to_string()]);
        let v = r.active_flags(&[], &pu);
        assert!(v.contains(&"ssl".to_string()));
        assert!(!v.contains(&"man".to_string()));
        assert!(v.contains(&"zlib".to_string()));
        assert!(!v.contains(&"other".to_string()));
        let r9 = recipe("openssl", "9", &[]);
        let v9 = r9.active_flags(&[], &pu);
        assert!(v9.contains(&"other".to_string()));
    }

    #[test]
    fn mask_matching() {
        let masks = vec![
            ">=openssl-3.0".to_string(),
            "bash:1.1".to_string(),
            "mail-client/spam-2.0".to_string(),
        ];
        assert!(is_masked(&masks, "openssl", "0", "3.3.1"));
        assert!(!is_masked(&masks, "openssl", "0", "1.1.1w"));
        assert!(is_masked(&masks, "bash", "1.1", "5.2.21"));
        assert!(!is_masked(&masks, "bash", "0", "5.2.21"));
        assert!(is_masked(&masks, "spam", "0", "2.0"));
    }
}
