use anyhow::{anyhow, bail, Result};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;

pub fn sha256_file(path: &Path) -> Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 65536];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{:02x}", b)).collect())
}

pub fn fetch(url: &str, dest: &Path, expected: Option<&str>) -> Result<u64> {
    if let Some(p) = url.strip_prefix("file://") {
        std::fs::copy(p, dest).map_err(|e| anyhow!("{}: {}", url, e))?;
    } else if url.starts_with('/') {
        std::fs::copy(url, dest).map_err(|e| anyhow!("{}: {}", url, e))?;
    } else {
        download(url, dest)?;
    }
    if let Some(sum) = expected {
        let real = sha256_file(dest)?;
        if real != sum.to_lowercase() {
            bail!(
                "контрольная сумма {} не сошлась: ожидалось {}, получено {}",
                dest.display(),
                &sum[..sum.len().min(16)],
                &real[..16]
            );
        }
    }
    Ok(std::fs::metadata(dest)?.len())
}

fn download(url: &str, dest: &Path) -> Result<()> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(30))
        .build();
    let resp = agent
        .get(url)
        .call()
        .map_err(|e| anyhow!("{}: {}", url, e))?;
    let total: Option<u64> = resp.header("Content-Length").and_then(|h| h.parse().ok());
    let mut reader = resp.into_reader();
    let mut out = std::fs::File::create(dest)?;
    let mut buf = vec![0u8; 65536];
    let mut got: u64 = 0;
    let mut next_report: u64 = 1 << 20;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        std::io::Write::write_all(&mut out, &buf[..n])?;
        got += n as u64;
        if got >= next_report {
            let msg = match total {
                Some(t) => format!(
                    "{}: {} из {}",
                    url,
                    crate::util::human_size(got),
                    crate::util::human_size(t)
                ),
                None => format!("{}: {}", url, crate::util::human_size(got)),
            };
            crate::util::info(&msg);
            next_report += 1 << 21;
        }
    }
    if let Some(t) = total {
        if got != t {
            bail!(
                "{}: загрузка оборвалась, ожидалось {}, получено {}",
                url,
                t,
                got
            );
        }
    }
    Ok(())
}
