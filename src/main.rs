mod build;
mod config;
mod setup;
pub mod shell;

use anyhow::Result;
use config::Paths;

fn main() -> Result<()> {
    let paths = Paths::resolve()?;
    setup::run(&paths)?;
    Ok(())
}
