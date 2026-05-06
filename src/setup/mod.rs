mod prompts;

use anyhow::{Context, Result};
use std::fs::{self};

#[cfg(unix)]
use std::io::Write;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use crate::config::Paths;
use crate::shell;

pub fn run(paths: &Paths) -> Result<()> {
    println!("welcome to sotto.\n");

    let provider = prompts::provider()?;
    let shell = prompts::detect_shell()
        .context("couldn't detect your shell - set $SHELL or pass --shell")?;
    let tuning = prompts::tuning()?;

    let config = build_config(&provider, tuning);

    paths.init_dirs()?;
    write_config(&paths.config, &config)?;

    shell::inject(&shell, paths)?;

    println!("\nyou are good to go and commit, but first, restart your shell or run:");
    println!("  source ~/.{}rc", shell);
    Ok(())
}

fn build_config(provider: &prompts::Provider, tuning: prompts::Tuning) -> String {
    let escaped_prompt = tuning.prompt.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        r#"endpoint = "{}"
model = "{}"
api_key = "{}"
debounce_secs = {}
max_diff_lines = {}
prompt = "{}"
"#,
        provider.endpoint,
        provider.model,
        provider.api_key,
        tuning.debounce_secs,
        tuning.max_diff_lines,
        escaped_prompt,
    )
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
