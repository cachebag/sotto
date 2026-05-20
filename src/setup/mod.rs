mod prompts;

use anyhow::{Context, Result};
use std::fs;

#[cfg(unix)]
use std::io::Write;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use crate::config::{Paths, SottoConfig};
use crate::shell;

pub fn run(paths: &Paths) -> Result<()> {
    println!("welcome to sotto.\n");

    let provider = prompts::provider()?;
    let shell = prompts::detect_shell()
        .context("couldn't detect your shell - set $SHELL or pass --shell")?;
    let tuning = prompts::tuning()?;

    let config = build_config(provider, tuning);
    let toml_str = config.to_toml()?;

    paths.init_dirs()?;
    write_config(&paths.config, &toml_str)?;

    shell::inject(&shell, paths)?;

    println!("\nyou are good to go and commit, but first, restart your shell or run:");
    println!("  source ~/.{}rc", shell);
    Ok(())
}

fn build_config(provider: prompts::Provider, tuning: prompts::Tuning) -> SottoConfig {
    SottoConfig {
        endpoint: provider.endpoint,
        model: provider.model,
        api_key: provider.api_key,
        debounce_secs: tuning.debounce_secs,
        max_diff_lines: tuning.max_diff_lines,
        prompt: tuning.prompt,
        inference_type: provider.inference_type,
    }
}

fn write_config(path: &std::path::Path, config: &str) -> Result<()> {
    #[cfg(unix)]
    {
        use std::path::Path;

        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));

        let basename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("config.toml");

        let mut tmp = tempfile::Builder::new()
            .prefix(&format!("{basename}."))
            .suffix(".tmp")
            .permissions(fs::Permissions::from_mode(0o600))
            .tempfile_in(parent)
            .with_context(|| format!("failed to create temp file in {}", parent.display()))?;

        tmp.as_file_mut()
            .write_all(config.as_bytes())
            .with_context(|| format!("failed to write {}", tmp.path().display()))?;
        tmp.as_file_mut()
            .sync_all()
            .with_context(|| format!("failed to flush {}", tmp.path().display()))?;

        tmp.persist(path)
            .map_err(|e| anyhow::Error::from(e.error))
            .with_context(|| format!("failed to install {}", path.display()))?;

        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .with_context(|| format!("failed to set permissions on {}", path.display()))?;

        Ok(())
    }

    #[cfg(not(unix))]
    {
        fs::write(path, config).with_context(|| format!("failed to write {}", path.display()))
    }
}
