mod cache;
mod generator;
mod watcher;

use anyhow::Result;

use crate::{
    config::{Paths, SottoConfig},
    daemon::watcher::RepoWatcher,
};

pub fn start(config: &SottoConfig, paths: &Paths) -> Result<()> {
    let mut watcher = RepoWatcher::from_cwd()?;
    watcher.start(config, paths)
}
