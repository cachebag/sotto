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
    let model_names = get_model_names();
    let model: String = match model_names.is_empty() {
        true => Input::new()
            .with_prompt("ollama model: ")
            .default(OLLAMA_DEFAULT_MODEL.into())
            .interact()?,
        false => {
            let choice = Select::new()
                .with_prompt("available ollama models")
                .items(&model_names)
                .default(0)
                .interact()?;
            model_names[choice].to_string()
        }
    };

    Ok(Provider {
        inference_type: "ollama".to_string(),
        model,
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

fn get_downloaded_models() -> Result<OllamaList> {
    let models: OllamaList = ureq::get("http://localhost:11434/api/tags")
        .call()?
        .body_mut()
        .read_json()?;

    Ok(models)
}

//let model_names = models.models.into_iter().map(|m| m.name).collect();

fn get_model_names() -> Vec<String> {
    let model_names = get_downloaded_models();
    match model_names {
        Ok(models) => models.models.into_iter().map(|m| m.name).collect(),
        Err(e) => {
            eprintln!(
                "⚠️ Warning: Failed to fetch models from Ollama (is it running?). Error: {e}"
            );

            Vec::new()
        }
    }
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
const OLLAMA_DEFAULT_MODEL: &str = "llama3.2:latest";
