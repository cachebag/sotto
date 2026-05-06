mod prompts;

use anyhow::{Context, Result};
use std::fs::{self, OpenOptions};
use std::io::Write;

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

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
        let mut temp_path = path.to_path_buf();
        temp_path.set_extension(format!("toml.tmp.{}", std::process::id()));

        let write_result = (|| -> Result<()> {
            let mut file = OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .mode(0o600)
                .open(&temp_path)
                .with_context(|| format!("failed to create {}", temp_path.display()))?;

            file.write_all(config.as_bytes())
                .with_context(|| format!("failed to write {}", temp_path.display()))?;
            file.sync_all()
                .with_context(|| format!("failed to flush {}", temp_path.display()))?;

            fs::rename(&temp_path, path)
                .with_context(|| format!("failed to install {}", path.display()))?;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))
                .with_context(|| format!("failed to set permissions on {}", path.display()))?;

            Ok(())
        })();

        if write_result.is_err() {
            let _ = fs::remove_file(&temp_path);
        }

        write_result
    }

    #[cfg(not(unix))]
    {
        fs::write(path, config).with_context(|| format!("failed to write {}", path.display()))
    }
}
