use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use git2::{DiffFormat, DiffOptions, Oid, Repository};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher as NotifyWatcher};
use sha2::{Digest, Sha256};

use crate::config::{Paths, SottoConfig};
use crate::daemon::cache;
use crate::daemon::generator;

#[cfg(unix)]
use crate::ipc::protocol::RepoPhase;
#[cfg(unix)]
use crate::ipc::server::EventBus;

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
            last_staged_tree: None,
            debounce_secs: 15,
        })
    }
    /// Watch a working tree and generate commit messages
    pub fn start(&mut self, config: &SottoConfig, paths: &Paths) -> Result<()> {
        let shutdown = install_shutdown_hook()?;

        #[cfg(unix)]
        let mut event_bus: Option<EventBus> = {
            let repo_id = self.repo_cache_id()?;
            match EventBus::bind_with_log(
                &paths.socket,
                Arc::clone(&shutdown),
                repo_id,
                Some(paths.log.clone()),
            ) {
                Ok(bus) => {
                    eprintln!("sotto: ipc listening on {}", paths.socket.display());
                    Some(bus)
                }
                Err(e) => {
                    eprintln!("sotto: ipc bind failed: {e}");
                    None
                }
            }
        };

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
            if shutdown.load(Ordering::Relaxed) {
                break;
            }
            match rx.recv_timeout(Duration::from_millis(500)) {
                Ok(event) => {
                    if self.should_ignore(&event) {
                        continue;
                    }
                    let is_new_burst = last_event.is_none();
                    last_event = Some(Instant::now());

                    // One debouncing notify per idle→active burst, not per filesystem event.
                    #[cfg(unix)]
                    if is_new_burst && let Some(bus) = &mut event_bus {
                        bus.broadcast(RepoPhase::Debouncing, None, None);
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if let Some(ts) = last_event
                        && ts.elapsed() >= debounce
                    {
                        last_event = None;
                        #[cfg(unix)]
                        self.run_generation_cycle_unix(config, paths, &mut event_bus);
                        #[cfg(not(unix))]
                        self.run_generation_cycle(config, paths);
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        // EventBus::drop unlinks the socket file
        Ok(())
    }

    /// Check for a meaningful diff, generate a commit message, and broadcast
    /// IPC phase transitions at each step.
    #[cfg(unix)]
    fn run_generation_cycle_unix(
        &mut self,
        config: &SottoConfig,
        paths: &Paths,
        event_bus: &mut Option<EventBus>,
    ) {
        match self.check_diff(config) {
            Ok(Some(result)) => {
                if let Some(bus) = event_bus.as_mut() {
                    bus.broadcast(RepoPhase::Generating, None, None);
                }

                match self.generate_and_cache(config, paths, result) {
                    Ok(()) => {
                        if let Some(bus) = event_bus.as_mut() {
                            bus.broadcast(RepoPhase::Ready, self.last_diff_hash.clone(), None);
                        }
                    }
                    Err(e) => {
                        eprintln!("sotto: {e}");
                        if let Some(bus) = event_bus.as_mut() {
                            bus.broadcast(RepoPhase::Error, None, Some(e.to_string()));
                        }
                    }
                }
            }
            Ok(None) => {}
            Err(e) => {
                eprintln!("sotto: {e}");
                if let Some(bus) = event_bus.as_mut() {
                    bus.broadcast(RepoPhase::Error, None, Some(e.to_string()));
                }
            }
        }
    }

    #[cfg(not(unix))]
    fn run_generation_cycle(&mut self, config: &SottoConfig, paths: &Paths) {
        match self.check_diff(config) {
            Ok(Some(result)) => {
                if let Err(e) = self.generate_and_cache(config, paths, result) {
                    eprintln!("sotto: {e}");
                }
            }
            Ok(None) => {}
            Err(e) => eprintln!("sotto: {e}"),
        }
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

    /// Returns `Some(DiffResult)` when a new diff needs generation,
    /// `None` when the diff is empty or nothing meaningful changed.
    fn check_diff(&self, config: &SottoConfig) -> Result<Option<DiffResult>> {
        let staged_diff = self.get_staged_diff(config)?;
        let workdir_diff = self.get_workdir_diff(config)?;

        let (diff, staged_tree) = if !staged_diff.is_empty() {
            let tree_oid = self.index_tree_oid();
            (staged_diff, tree_oid)
        } else if !workdir_diff.is_empty() {
            (workdir_diff, None)
        } else {
            return Ok(None);
        };

        // When staging content we already generated for, the tree OID will
        // match even though the raw patch bytes differ — skip the regen.
        if let Some(ref tree) = staged_tree
            && self.last_staged_tree.as_ref() == Some(tree)
        {
            return Ok(None);
        }

        let hash = hash_string(&diff);

        if self.last_diff_hash.as_ref() == Some(&hash) {
            return Ok(None);
        }

        Ok(Some(DiffResult {
            diff,
            hash,
            staged_tree,
        }))
    }

    /// The tree OID that `git commit` would record given the current index.
    /// Returns `None` if the index can't be read or written as a tree.
    // FIXME: Duplicated in `shell/complete.rs`; consolidate. Confirm this matches `git write-tree` /
    // real commits for unusual index states (sparse checkout, conflict entries, etc.).
    fn index_tree_oid(&self) -> Option<String> {
        let mut index = self.repo.index().ok()?;
        let oid: Oid = index.write_tree().ok()?;
        Some(oid.to_string())
    }

    fn generate_and_cache(
        &mut self,
        config: &SottoConfig,
        paths: &Paths,
        result: DiffResult,
    ) -> Result<()> {
        let repo_id = self.repo_cache_id()?;
        let message = generator::generate(config, &result.diff)?;
        cache::write(
            &paths.cache_dir,
            &repo_id,
            &message,
            &result.hash,
            result.staged_tree.as_deref(),
        )?;
        self.last_diff_hash = Some(result.hash);
        self.last_staged_tree = result.staged_tree;
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

struct DiffResult {
    diff: String,
    hash: String,
    staged_tree: Option<String>,
}

pub struct RepoWatcher {
    repo: Repository,
    workdir: PathBuf,
    last_diff_hash: Option<String>,
    last_staged_tree: Option<String>,
    debounce_secs: u64,
}
