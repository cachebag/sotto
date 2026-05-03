use anyhow::{Context, Result};

use std::fs;
use std::path::Path;

/// Write a generated commit message and its diff hash to the cache.
pub fn write(cache_dir: &Path, repo_id: &str, message: &str, diff_hash: &str) -> Result<()> {
    let dir = cache_dir.join(repo_id);
    fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create cache dir: {}", dir.display()))?;

    fs::write(dir.join("message"), message).context("failed to write cached message")?;

    fs::write(dir.join("diff_hash"), diff_hash).context("failed to write cached diff has")?;

    Ok(())
}

/// Read a cached commit message for a repo.
/// Returns None if no cache exists.
pub fn read(cache_dir: &Path, repo_id: &str) -> Option<CacheEntry> {
    let dir = cache_dir.join(repo_id);

    let message = fs::read_to_string(dir.join("message")).ok()?;
    let diff_hash = fs::read_to_string(dir.join("diff_hash")).ok()?;

    Some(CacheEntry {
        message: message.trim().to_string(),
        diff_hash: diff_hash.trim().to_string(),
    })
}

pub struct CacheEntry {
    pub message: String,
    pub diff_hash: String,
}
