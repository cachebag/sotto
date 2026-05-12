use anyhow::{Context, Result};

use std::fs;
use std::path::Path;

/// Write a generated commit message, diff hash, and optional index tree OID.
pub fn write(
    cache_dir: &Path,
    repo_id: &str,
    message: &str,
    diff_hash: &str,
    staged_tree: Option<&str>,
) -> Result<()> {
    let dir = cache_dir.join(repo_id);
    fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create cache dir: {}", dir.display()))?;

    fs::write(dir.join("message"), message).context("failed to write cached message")?;
    fs::write(dir.join("diff_hash"), diff_hash).context("failed to write cached diff hash")?;

    match staged_tree {
        Some(t) => {
            fs::write(dir.join("staged_tree"), t).context("failed to write cached staged tree")?
        }
        None => {
            let _ = fs::remove_file(dir.join("staged_tree"));
        }
    }

    Ok(())
}

/// Read a cached commit message for a repo.
/// Returns None if no cache exists (missing message or diff_hash).
pub fn read(cache_dir: &Path, repo_id: &str) -> Option<CacheEntry> {
    let dir = cache_dir.join(repo_id);

    let message = fs::read_to_string(dir.join("message")).ok()?;
    let diff_hash = fs::read_to_string(dir.join("diff_hash")).ok()?;
    let staged_tree = fs::read_to_string(dir.join("staged_tree"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    Some(CacheEntry {
        message: message.trim().to_string(),
        diff_hash: diff_hash.trim().to_string(),
        staged_tree,
    })
}

pub struct CacheEntry {
    pub message: String,
    pub diff_hash: String,
    pub staged_tree: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_with_staged_tree() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "repo1",
            "feat: thing",
            "abc",
            Some("tree_oid_1"),
        )
        .unwrap();

        let entry = read(dir.path(), "repo1").unwrap();
        assert_eq!(entry.message, "feat: thing");
        assert_eq!(entry.diff_hash, "abc");
        assert_eq!(entry.staged_tree.as_deref(), Some("tree_oid_1"));
    }

    #[test]
    fn roundtrip_without_staged_tree() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "repo2", "fix: bug", "def", None).unwrap();

        let entry = read(dir.path(), "repo2").unwrap();
        assert_eq!(entry.message, "fix: bug");
        assert_eq!(entry.diff_hash, "def");
        assert!(entry.staged_tree.is_none());
    }

    #[test]
    fn staged_tree_cleared_on_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "repo3", "m1", "h1", Some("tree_a")).unwrap();
        write(dir.path(), "repo3", "m2", "h2", None).unwrap();

        let entry = read(dir.path(), "repo3").unwrap();
        assert_eq!(entry.message, "m2");
        assert!(entry.staged_tree.is_none());
    }
}
