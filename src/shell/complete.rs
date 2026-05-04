use git2::{DiffFormat, DiffOptions, Repository};
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
    let entry = cache::read(&paths.cache_dir, &repo_id)?;

    // validate that cached message matches the diff we're about to commit
    let config = SottoConfig::load_silently(paths)?;

    let staged_diff = get_staged_diff(&repo, config.max_diff_lines).ok()?;
    let staged_hash = hash_string(&staged_diff);

    // if there are staged changes, message MUST match staged diff
    // (user is committing staged changes, not workdir changes)
    if !staged_diff.is_empty() {
        if staged_hash != entry.diff_hash {
            return None; // message is for wrong changes
        }
    } else {
        // no staged changes yet - check workdir diff (pre-staging scenario)
        let workdir_diff = get_workdir_diff(&repo, config.max_diff_lines).ok()?;
        let workdir_hash = hash_string(&workdir_diff);
        if workdir_hash != entry.diff_hash {
            return None;
        }
    }

    Some(entry.message)
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
