use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::SottoConfig;

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    max_tokens: u32,
}

#[derive(Serialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ResponseMessage,
}

#[derive(Deserialize)]
struct ResponseMessage {
    content: String,
}

// FIXME:
// Some of this is quite crude. Obviously, we should eventually allow the user to
// configure/customize how they like their commits.
pub fn generate(config: &SottoConfig, diff: &str) -> Result<String> {
    let user = format!("Generate a commit message for this diff:\n\n{diff}");

    let body = ChatRequest {
        model: config.model.clone(),
        messages: vec![
            Message {
                role: "system".into(),
                content: config.prompt.clone(),
            },
            Message {
                role: "user".into(),
                content: user,
            },
        ],
        max_tokens: 100,
    };

    let response = ureq::post(&config.endpoint)
        .header("Authorization", &format!("Bearer {}", config.api_key))
        .send_json(&body)?;

    let body_str = response.into_body().read_to_string()?;

    let chat: ChatResponse =
        serde_json::from_str(&body_str).context("failed to parse API response")?;
    let message = chat
        .choices
        .first()
        .map(|c| c.message.content.trim().to_string())
        .unwrap_or_default();

    if message.is_empty() {
        anyhow::bail!("API returned an empty commit message");
    }

    Ok(message)
}
