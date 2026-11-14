use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

use crate::recipe::DEFAULT_SLOT;

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
    #[serde(default)]
    pub slot: String,
    #[serde(default)]
    pub hashes: BTreeMap<String, String>,
}

pub type Db = BTreeMap<String, Installed>;

pub fn key(name: &str, slot: &str) -> String {
    format!("{}:{}", name, slot)
}

pub fn split_key(k: &str) -> (&str, &str) {
    match k.rfind(':') {
        Some(i) => (&k[..i], &k[i + 1..]),
        None => (k, DEFAULT_SLOT),
    }
}

pub fn name_of(k: &str) -> String {
    split_key(k).0.to_string()
}

pub fn lookup<'a>(db: &'a Db, name: &str, slot: Option<&str>) -> Option<&'a Installed> {
    match slot {
        Some(s) => db.get(&key(name, s)),
        None => db
            .get(&key(name, DEFAULT_SLOT))
            .or_else(|| db.iter().find(|(k, _)| name_of(k) == name).map(|(_, v)| v)),
    }
}

pub fn matching<'a>(
    db: &'a Db,
    name: &str,
    slot: Option<&str>,
) -> Vec<(&'a String, &'a Installed)> {
    db.iter()
        .filter(|(k, _)| name_of(k) == name)
        .filter(|(k, _)| slot.map(|s| split_key(k).1 == s).unwrap_or(true))
        .collect()
}

pub fn load(path: &Path) -> Result<Db> {
    if !path.exists() {
        return Ok(Db::new());
    }
    let text =
        std::fs::read_to_string(path).with_context(|| format!("читаю {}", path.display()))?;
    let raw: Db =
        serde_json::from_str(&text).with_context(|| format!("разбираю {}", path.display()))?;
    let mut db = Db::new();
    for (k, mut v) in raw {
        if v.slot.is_empty() {
            v.slot = split_key(&k).1.to_string();
        }
        if v.slot.is_empty() {
            v.slot = DEFAULT_SLOT.to_string();
        }
        db.insert(key(&name_of(&k), &v.slot), v);
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_roundtrip() {
        assert_eq!(split_key("bash:0"), ("bash", "0"));
        assert_eq!(split_key("openssl:1.1"), ("openssl", "1.1"));
        assert_eq!(split_key("bash"), ("bash", "0"));
        assert_eq!(name_of("ncurses:6"), "ncurses");
        assert_eq!(key("zlib", "0"), "zlib:0");
    }

    #[test]
    fn lookup_prefers_default_slot() {
        let mut db = Db::new();
        db.insert(
            key("openssl", "0"),
            Installed {
                version: "3.3.1".into(),
                slot: "0".into(),
                ..Default::default()
            },
        );
        db.insert(
            key("openssl", "1.1"),
            Installed {
                version: "1.1.1w".into(),
                slot: "1.1".into(),
                ..Default::default()
            },
        );
        assert_eq!(lookup(&db, "openssl", None).unwrap().version, "3.3.1");
        assert_eq!(
            lookup(&db, "openssl", Some("1.1")).unwrap().version,
            "1.1.1w"
        );
        assert!(lookup(&db, "openssl", Some("2")).is_none());
        assert_eq!(matching(&db, "openssl", None).len(), 2);
        assert_eq!(matching(&db, "openssl", Some("1.1")).len(), 1);
    }

    #[test]
    fn load_migrates_plain_keys() {
        let dir = std::env::temp_dir().join(format!("flexura-db-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("installed.json");
        std::fs::write(
            &path,
            r#"{"bash": {"version": "5.2.21", "files": [], "dirs": []}}"#,
        )
        .unwrap();
        let db = load(&path).unwrap();
        assert!(db.contains_key("bash:0"));
        assert_eq!(db["bash:0"].slot, "0");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn hashes_survive_roundtrip() {
        let mut hashes = BTreeMap::new();
        hashes.insert("usr/lib/libz.so.1".to_string(), "deadbeef".to_string());
        let entry = Installed {
            version: "1.3.1".into(),
            files: vec!["usr/lib/libz.so.1".into()],
            dirs: vec!["usr/lib".into()],
            hashes,
            ..Default::default()
        };
        let text = serde_json::to_string(&entry).unwrap();
        let back: Installed = serde_json::from_str(&text).unwrap();
        assert_eq!(back.hashes["usr/lib/libz.so.1"], "deadbeef");
        let legacy: Installed =
            serde_json::from_str(r#"{"version": "1.0", "files": [], "dirs": [], "slot": "0"}"#)
                .unwrap();
        assert!(legacy.hashes.is_empty());
    }
}
