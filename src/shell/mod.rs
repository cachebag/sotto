pub mod complete;

use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

use crate::config::Paths;

pub fn inject(shell: &str, paths: &Paths) -> Result<()> {
    match shell {
        "zsh" => inject_zsh(paths),
        "fish" => inject_fish(paths),
        _ => {
            println!("  shell '{}' not supported yet", shell);
            Ok(())
        }
    }
}

fn inject_zsh(paths: &Paths) -> Result<()> {
    let script_path = script_dir(paths)?.join("sotto.zsh");
    fs::write(&script_path, ZSH_WIDGET).context("failed to write zsh widget")?;

    let rc = dirs::home_dir()
        .context("could not find home dir")?
        .join(".zshrc");

    let source_line = format!("source \"{}\"", script_path.display());
    append_if_missing(&rc, &source_line)?;

    Ok(())
}

fn inject_fish(paths: &Paths) -> Result<()> {
    let script_path = script_dir(paths)?.join("sotto.fish");
    fs::write(&script_path, FISH_WIDGET).context("failed to write fish widget")?;

    let conf_dir = dirs::config_dir()
        .context("could not find config dir")?
        .join("fish")
        .join("conf.d");

    fs::create_dir_all(&conf_dir)?;
    fs::copy(&script_path, conf_dir.join("sotto.fish")).context("failed to install fish widget")?;

    Ok(())
}

fn script_dir(paths: &Paths) -> Result<PathBuf> {
    let dir = paths
        .cache_dir
        .parent()
        .context("cache dir has no parent")?
        .join("shell");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn append_if_missing(rc_file: &PathBuf, line: &str) -> Result<()> {
    let contents = fs::read_to_string(rc_file).unwrap_or_default();

    if contents.contains(line) {
        return Ok(()); // already injected
    }

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(rc_file)?;

    use std::io::Write;
    writeln!(file, "\n# sotto - git commit message completion")?;
    writeln!(file, "{line}")?;

    Ok(())
}

const ZSH_WIDGET: &str = include_str!("../shell/sotto.zsh");
const FISH_WIDGET: &str = include_str!("../shell/sotto.fish");
