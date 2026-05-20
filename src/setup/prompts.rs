use std::env;

use anyhow::Result;
use serde::Deserialize;

use crate::config::DEFAULT_PROMPT;
use crate::ui;

pub fn provider() -> Result<Provider> {
    let presets = ["openrouter", "ollama", "custom endpoint"];
    let choice = ui::select("provider", &presets)?;

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
    let debounce_secs: u64 = ui::input_with_default("debounce (seconds)", "15")?.parse()?;
    let max_diff_lines: usize = ui::input_with_default("max diff lines", "500")?.parse()?;
    let prompt = ui::input_with_default("custom prompt", DEFAULT_PROMPT)?;

    Ok(Tuning {
        debounce_secs,
        max_diff_lines,
        prompt,
    })
}

fn open_router() -> Result<Provider> {
    let api_key = ui::password("OpenRouter API key (openrouter.ai/keys)")?;
    let model = ui::input_with_default("OpenRouter model id", OPENROUTER_MODEL_DEFAULT)?;

    Ok(Provider {
        inference_type: "openrouter".to_string(),
        endpoint: OPENROUTER_ENDPOINT.into(),
        model,
        api_key,
    })
}

fn ollama() -> Result<Provider> {
    let model_names = get_model_names();
    let model = if model_names.is_empty() {
        ui::input_with_default("ollama model", OLLAMA_DEFAULT_MODEL)?
    } else {
        let choice = ui::select("available ollama models", &model_names)?;
        model_names[choice].clone()
    };

    Ok(Provider {
        inference_type: "ollama".to_string(),
        model,
        endpoint: OLLAMA_ENDPOINT.to_string(),
        api_key: String::new(),
    })
}

fn custom() -> Result<Provider> {
    let endpoint = ui::input("endpoint")?;
    let model = ui::input("model")?;
    let api_key = ui::password("api key")?;

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

fn get_model_names() -> Vec<String> {
    match get_downloaded_models() {
        Ok(models) => models.models.into_iter().map(|m| m.name).collect(),
        Err(e) => {
            eprintln!("warning: failed to fetch models from ollama (is it running?): {e}");
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
