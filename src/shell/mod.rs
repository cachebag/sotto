mod complete;

use crate::config::Paths;
use anyhow::Result;
use std::path::Path;

pub fn inject(shell: &str, _paths: &Paths) -> Result<()> {
    unimplemented!("inject for shell: {}", shell)
}
