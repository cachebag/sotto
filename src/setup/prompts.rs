use std::env;

use anyhow::Result;
use dialoguer::{Input, Password, Select};
use serde::Deserialize;

use crate::config::DEFAULT_PROMPT;

pub fn provider() -> Result<Provider> {
    let presets = vec!["openrouter", "ollama", "custom endpoint"];
    let choice = Select::new()
        .with_prompt("provider")
        .items(&presets)
        .default(0)
        .interact()?;

    match choice {
        0 => open_router(),
        1 => ollama(),
        _ => custom(),
    }
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

    let prompt: String = Input::new()
        .with_prompt("custom prompt")
        .default(DEFAULT_PROMPT.to_string())
        .interact()?;

    Ok(Tuning {
        debounce_secs,
        max_diff_lines,
        prompt,
    })
}

fn open_router() -> Result<Provider> {
    let api_key = Password::new()
        .with_prompt("OpenRouter API key (openrouter.ai/keys)")
        .interact()?;

    let model: String = Input::new()
        .with_prompt("OpenRouter model id")
        .default(OPENROUTER_MODEL_DEFAULT.into())
        .interact_text()?;

    Ok(Provider {
        inference_type: "openrouter".to_string(),
        endpoint: OPENROUTER_ENDPOINT.into(),
        model,
        api_key,
    })
}

fn ollama() -> Result<Provider> {
    let model_names = get_downloaded_models()?;

    if model_names.is_empty() {
        anyhow::bail!("no ollama models found - run `ollama pull <model>` first");
    }

    let choice = Select::new()
        .with_prompt("available ollama models")
        .items(&model_names)
        .default(0)
        .interact()?;

    Ok(Provider {
        inference_type: "ollama".to_string(),
        model: model_names[choice].clone(),
        endpoint: OLLAMA_ENDPOINT.to_string(),
        api_key: String::new(),
    })
}

fn custom() -> Result<Provider> {
    let endpoint: String = Input::new().with_prompt("endpoint").interact()?;
    let model: String = Input::new().with_prompt("model").interact()?;
    let api_key = Password::new().with_prompt("api key").interact()?;

    Ok(Provider {
        inference_type: "custom".to_string(),
        endpoint,
        model,
        api_key,
    })
}

fn get_downloaded_models() -> Result<Vec<String>> {
    let models: OllamaList = ureq::get("http://localhost:11434/api/tags")
        .call()?
        .body_mut()
        .read_json()?;

    let model_names = models.models.into_iter().map(|m| m.name).collect();
    Ok(model_names)
}

pub struct Provider {
    pub endpoint: String,
    pub model: String,
    pub api_key: String,
    pub inference_type: String,
}

pub struct Tuning {
    pub debounce_secs: u64,
    pub max_diff_lines: usize,
    pub prompt: String,
}

#[derive(Deserialize)]
struct OllamaList {
    models: Vec<OllamaModel>,
}

#[derive(Deserialize)]
struct OllamaModel {
    name: String,
}

const OPENROUTER_ENDPOINT: &str = "https://openrouter.ai/api/v1/chat/completions";
const OPENROUTER_MODEL_DEFAULT: &str = "openai/gpt-oss-120b:free";
const OLLAMA_ENDPOINT: &str = "http://localhost:11434/api/generate";
