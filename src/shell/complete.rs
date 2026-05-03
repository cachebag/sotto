use git2::Repository;
use sha2::{Digest, Sha256};

use crate::config::Paths;
use crate::daemon::cache;


/// Called by the shell widget. Prints the cached message to stdout.
/// Exists silently if anything fails. We don't event interrupt git.
pub fn run(paths: &Paths) {
    let Some(message) = try_read(paths) else {
        return;
    };

    print!("{message}");
}

fn try_read(paths: &Paths) -> Option<String> {
    let repo = Repository::discover(".").ok()?;
    let workdir = repo.workdir()?.to_string_lossy().to_string();

    let mut hasher = Sha256::new();
    hasher.update(workdir.as_bytes());
    let repo_id = hasher.finalize().iter().map(|b| format!("{:02x}", b)).collect::<String>();

    let entry = cache::read(&paths.cache_dir, &repo_id)?;
    Some(entry.message)
}
