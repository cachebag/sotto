mod prompts;

use anyhow::{Context, Result};

use crate::config::Paths;
use crate::shell;

pub fn run(paths: &Paths) -> Result<()> {
    println!("welcome to sotto.\n");

    let provider = prompts::provider()?;
    let shell = prompts::detect_shell()
        .context("couldn't detect your shell - set $SHELL or pass --shell")?;
    let tuning = prompts::tuning()?;

    let config = build_config(provider, tuning);
    paths.init_dirs()?;
    std::fs::write(&paths.config, config)?;

    shell::inject(&shell, paths)?;

    println!("\nyou are good to go and commit, but first, restart your shell or run:");
    println!("  source ~/.{}rc", shell);
    Ok(())
}

fn build_config(provider: prompts::Provider, tuning: prompts::Tuning) -> String {
    format!(
        r#"endpoint = "{}"
model = "{}"
api_key = "{}"
debounce_secs = {}
max_diff_lines = {}
"#,
        provider.endpoint,
        provider.model,
        provider.api_key,
        tuning.debounce_secs,
        tuning.max_diff_lines,
    )
}
