use anyhow::Result;
use dialoguer::{Input, Password, Select};
use std::env;

pub fn provider() -> Result<Provider> {
    let presets = vec!["openrouter", "custom endpoint"];
    let choice = Select::new()
        .with_prompt("provider")
        .items(&presets)
        .default(0)
        .interact()?;

    match choice {
        0 => open_router(),
        _ => custom(),
    }
}

fn open_router() -> Result<Provider> {
    let api_key = Password::new()
        .with_prompt("OpenRouter API key (openrouter.ai/keys)")
        .interact()?;

    Ok(Provider {
        endpoint: "https://openrouter.ai/api/v1/chat/completions".into(),
        model: "openai/gpt-oss-120b:free".into(),
        api_key,
    })
}

fn custom() -> Result<Provider> {
    let endpoint: String = Input::new().with_prompt("endpoint").interact()?;

    let model: String = Input::new().with_prompt("model").interact()?;

    let api_key = Password::new().with_prompt("api key").interact()?;

    Ok(Provider {
        endpoint,
        model,
        api_key,
    })
}

pub fn detect_shell() -> Option<String> {
    env::var("SHELL")
        .ok()
        .and_then(|s| s.rsplit('/').next().map(String::from))
}

pub fn tuning() -> Result<Tuning> {
    let debounce_secs: u64 = Input::new()
        .with_prompt("debounce (seconds)")
        .default(15)
        .interact()?;

    let max_diff_lines: usize = Input::new()
        .with_prompt("max diff lines")
        .default(500)
        .interact()?;

    Ok(Tuning {
        debounce_secs,
        max_diff_lines,
    })
}

pub struct Provider {
    pub endpoint: String,
    pub model: String,
    pub api_key: String,
}

pub struct Tuning {
    pub debounce_secs: u64,
    pub max_diff_lines: usize,
}
