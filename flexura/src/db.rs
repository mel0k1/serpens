use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Installed {
    pub version: String,
    pub files: Vec<String>,
    pub dirs: Vec<String>,
    #[serde(default)]
    pub use_flags: Vec<String>,
    #[serde(default)]
    pub built: String,
    #[serde(default)]
    pub size: u64,
}

pub type Db = BTreeMap<String, Installed>;

pub fn load(path: &Path) -> Result<Db> {
    if !path.exists() {
        return Ok(Db::new());
    }
    let text = std::fs::read_to_string(path).with_context(|| format!("читаю {}", path.display()))?;
    let db: Db = serde_json::from_str(&text).with_context(|| format!("разбираю {}", path.display()))?;
    Ok(db)
}

pub fn save(path: &Path, db: &Db) -> Result<()> {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    let text = serde_json::to_string_pretty(db)?;
    std::fs::write(path, text)?;
    Ok(())
}
