use anyhow::Result;
use std::path::Path;

pub fn load(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .map(|t| {
            t.lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

pub fn save(path: &Path, world: &[String]) -> Result<()> {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    let text = if world.is_empty() {
        String::new()
    } else {
        format!("{}\n", world.join("\n"))
    };
    std::fs::write(path, text)?;
    Ok(())
}
