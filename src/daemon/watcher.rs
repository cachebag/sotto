use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use git2::{DiffFormat, DiffOptions, Repository};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher as NotifyWatcher};
use sha2::{Digest, Sha256};

use crate::config::{Paths, SottoConfig};
use crate::daemon::cache;
use crate::daemon::generator;

fn install_shutdown_hook() -> Result<Arc<AtomicBool>> {
    let flag = Arc::new(AtomicBool::new(false));

    #[cfg(unix)]
    {
        use signal_hook::consts::signal::{SIGINT, SIGTERM};
        use signal_hook::flag;

        flag::register(SIGTERM, Arc::clone(&flag)).context("register SIGTERM handler")?;
        flag::register(SIGINT, Arc::clone(&flag)).context("register SIGINT handler")?;
    }

    #[cfg(windows)]
    {
        let f = Arc::clone(&flag);
        ctrlc::set_handler(move || {
            f.store(true, Ordering::Relaxed);
        })
        .context("set Ctrl+C handler")?;
    }

    Ok(flag)
}

impl RepoWatcher {
    /// Open a repo at the current working directory.
    // FIXME: Support bare repositories
    pub fn from_cwd() -> Result<Self> {
        let repo = Repository::discover(".").context("not inside a git repository")?;

        let workdir = repo
            .workdir()
            .context("bare repositories are not supported")?
            .to_path_buf();

        Ok(Self {
            repo,
            workdir,
            last_diff_hash: None,
            debounce_secs: 15,
        })
    }
    /// Watch a working tree and generate commit messages
    pub fn start(&mut self, config: &SottoConfig, paths: &Paths) -> Result<()> {
        let shutdown = install_shutdown_hook()?;

        let (tx, rx) = mpsc::channel::<Event>();

        let mut watcher = RecommendedWatcher::new(
            move |res: notify::Result<Event>| {
                if let Ok(event) = res {
                    let _ = tx.send(event);
                }
            },
            notify::Config::default(),
        )?;

        watcher.watch(&self.workdir, RecursiveMode::Recursive)?;

        self.debounce_secs = config.debounce_secs;
        let debounce = Duration::from_secs(self.debounce_secs);
        let mut last_event: Option<Instant> = None;

        loop {
            match rx.recv_timeout(Duration::from_millis(500)) {
                Ok(event) => {
                    if self.should_ignore(&event) {
                        continue;
                    }
                    last_event = Some(Instant::now());
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if shutdown.load(Ordering::Relaxed) {
                        break;
                    }
                    // check if debounce window passed
                    if let Some(ts) = last_event
                        && ts.elapsed() >= debounce
                    {
                        last_event = None;
                        if let Err(e) = self.on_debounce(config, paths) {
                            eprintln!("sotto: {e}");
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        Ok(())
    }

    fn should_ignore(&self, event: &Event) -> bool {
        event.paths.iter().all(|path| {
            // allow .git/index changes (staging events)
            if path == &self.workdir.join(".git").join("index") {
                return false;
            }

            // ignore other changes inside .git/
            if path.starts_with(self.workdir.join(".git")) {
                return true;
            }

            // ignore .gitignore
            if let Ok(relative) = path.strip_prefix(&self.workdir)
                && self.repo.status_should_ignore(relative).unwrap_or(false)
            {
                return true;
            }

            false
        })
    }

    fn on_debounce(&mut self, config: &SottoConfig, paths: &Paths) -> Result<()> {
        // check staged diff first (higher priority - user might be about to commit)
        let staged_diff = self.get_staged_diff(config)?;
        let workdir_diff = self.get_workdir_diff(config)?;

        // prefer staged diff if it exists, otherwise use workdir diff
        let diff = if !staged_diff.is_empty() {
            staged_diff
        } else if !workdir_diff.is_empty() {
            workdir_diff
        } else {
            return Ok(()); // nothing to generate for
        };

        let hash = hash_string(&diff);

        // nothing meaningfully changed
        if self.last_diff_hash.as_ref() == Some(&hash) {
            return Ok(());
        }

        let repo_id = self.repo_cache_id()?;
        let message = generator::generate(config, &diff)?;

        cache::write(&paths.cache_dir, &repo_id, &message, &hash)?;
        self.last_diff_hash = Some(hash);

        Ok(())
    }

    fn get_workdir_diff(&self, config: &SottoConfig) -> Result<String> {
        let mut opts = DiffOptions::new();
        opts.include_untracked(false);

        let diff = self
            .repo
            .diff_index_to_workdir(None, Some(&mut opts))
            .context("failed to compute workdir diff")?;

        diff_to_string(&diff, config.max_diff_lines)
    }

    /// Staged diff
    /// Called by the completer path when the user is at `git commit`.
    pub fn get_staged_diff(&self, config: &SottoConfig) -> Result<String> {
        let head_tree = self.repo.head().and_then(|h| h.peel_to_tree()).ok();

        let mut opts = DiffOptions::new();

        let diff = self
            .repo
            .diff_tree_to_index(head_tree.as_ref(), None, Some(&mut opts))
            .context("failed to compute staged diff")?;

        diff_to_string(&diff, config.max_diff_lines)
    }

    fn repo_cache_id(&self) -> Result<String> {
        Ok(hash_string(&self.workdir.to_string_lossy()))
    }
}

// FIXME: This is a copy of the function in shell/complete.rs
// Should move this to a shared module.
fn diff_to_string(diff: &git2::Diff, max_lines: usize) -> Result<String> {
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

pub struct RepoWatcher {
    repo: Repository,
    workdir: PathBuf,
    last_diff_hash: Option<String>,
    debounce_secs: u64,
}
