use git2::{DiffFormat, DiffOptions, Oid, Repository};
use sha2::{Digest, Sha256};

use crate::config::{Paths, SottoConfig};
use crate::daemon::cache;

/// Called by the shell widget. Prints the cached message to stdout.
/// Exits silently if anything fails. We don't want to interrupt git.
pub fn run(paths: &Paths) {
    let Some(message) = try_read(paths) else {
        return;
    };

    print!("{message}");
}

fn try_read(paths: &Paths) -> Option<String> {
    let repo = Repository::discover(".").ok()?;
    let workdir = repo.workdir()?.to_string_lossy().to_string();
    let repo_id = hash_string(&workdir);

    if let Some(msg) = try_socket_fast_path(paths, &repo_id) {
        return Some(msg);
    }

    try_disk_validated(paths, &repo, &repo_id)
}

/// Ask the daemon over the socket if the cache is fresh. If the daemon says
/// `Ready` and the cache entry exists, trust it — skip local diff computation.
#[cfg(unix)]
fn try_socket_fast_path(paths: &Paths, repo_id: &str) -> Option<String> {
    use crate::ipc::client::query_state;
    use crate::ipc::protocol::RepoPhase;

    let state = query_state(&paths.socket, repo_id)?;

    if state.phase != RepoPhase::Ready {
        return None;
    }

    let entry = cache::read(&paths.cache_dir, repo_id)?;

    if state.diff_hash.as_ref() != Some(&entry.diff_hash) {
        return None;
    }

    Some(entry.message)
}

#[cfg(not(unix))]
fn try_socket_fast_path(_paths: &Paths, _repo_id: &str) -> Option<String> {
    None
}

/// Original path: read cache, recompute diffs locally, validate staleness.
///
/// For staged content, prefer comparing the index tree OID rather than raw
/// patch bytes — staging the same content the daemon already saw should reuse
/// the cached message even though the diff text formatting may differ.
fn try_disk_validated(paths: &Paths, repo: &Repository, repo_id: &str) -> Option<String> {
    let entry = cache::read(&paths.cache_dir, repo_id)?;
    let config = SottoConfig::load_silently(paths)?;

    let staged_diff = get_staged_diff(repo, config.max_diff_lines).ok()?;

    if !staged_diff.is_empty() {
        if let Some(ref cached_tree) = entry.staged_tree
            && let Some(current_tree) = index_tree_oid(repo)
            && *cached_tree == current_tree
        {
            return Some(entry.message);
        }

        let staged_hash = hash_string(&staged_diff);
        if staged_hash != entry.diff_hash {
            return None;
        }
    } else {
        let workdir_diff = get_workdir_diff(repo, config.max_diff_lines).ok()?;
        let workdir_hash = hash_string(&workdir_diff);
        if workdir_hash != entry.diff_hash {
            return None;
        }
    }

    Some(entry.message)
}

// FIXME: Duplicated in `daemon/watcher.rs`; consolidate. Confirm this matches `git write-tree` /
// real commits for unusual index states (sparse checkout, conflict entries, etc.).
fn index_tree_oid(repo: &Repository) -> Option<String> {
    let mut index = repo.index().ok()?;
    let oid: Oid = index.write_tree().ok()?;
    Some(oid.to_string())
}

fn get_workdir_diff(repo: &Repository, max_lines: usize) -> Result<String, git2::Error> {
    let mut opts = DiffOptions::new();
    opts.include_untracked(false);

    let diff = repo.diff_index_to_workdir(None, Some(&mut opts))?;
    diff_to_string(&diff, max_lines)
}

fn get_staged_diff(repo: &Repository, max_lines: usize) -> Result<String, git2::Error> {
    let head_tree = repo.head().and_then(|h| h.peel_to_tree()).ok();

    let mut opts = DiffOptions::new();
    let diff = repo.diff_tree_to_index(head_tree.as_ref(), None, Some(&mut opts))?;
    diff_to_string(&diff, max_lines)
}

fn diff_to_string(diff: &git2::Diff, max_lines: usize) -> Result<String, git2::Error> {
    let mut output = String::new();
    let mut line_count: usize = 0;

    diff.print(DiffFormat::Patch, |_delta, _hunk, line| {
        if line_count >= max_lines {
            return false;
        }

        let prefix = match line.origin() {
            '+' | '-' | ' ' => String::from(line.origin()),
            _ => String::new(),
        };

        if let Ok(content) = std::str::from_utf8(line.content()) {
            output.push_str(&prefix);
            output.push_str(content);
            line_count += 1;
        }

        true
    })?;

    if line_count >= max_lines {
        output.push_str("\n... diff truncated ...\n");
    }

    Ok(output)
}

fn hash_string(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect()
}
