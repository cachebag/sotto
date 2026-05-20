use std::net::IpAddr;
use std::sync::Once;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::SottoConfig;

pub fn generate(config: &SottoConfig, diff: &str) -> Result<String> {
    match config.inference_type.as_str() {
        "ollama" => generate_ollama(config, diff),
        _ => generate_openai_compatible(config, diff),
    }
}

fn generate_openai_compatible(config: &SottoConfig, diff: &str) -> Result<String> {
    warn_if_insecure_endpoint(&config.endpoint);

    let body = build_chat_request(config, diff);

    let response = ureq::post(&config.endpoint)
        .header("Authorization", &format!("Bearer {}", config.api_key))
        .send_json(&body)?;

    let body_str = response.into_body().read_to_string()?;

    parse_chat_response(&body_str)
}

fn generate_ollama(config: &SottoConfig, diff: &str) -> Result<String> {
    let prompt = format!("{}\n\nGenerate a commit message for this diff:\n\n{}", config.prompt, diff);
    let model = config.model.split(':').next().unwrap_or(&config.model).to_string();

    let body = OllamaRequest {
        model,
        prompt,
        stream: false,
    };

    let response: OllamaResponse = ureq::post(&config.endpoint)
        .send_json(&body)?
        .body_mut()
        .read_json()?;

    let message = response.response.trim().to_string();
    if message.is_empty() {
        anyhow::bail!("Ollama returned an empty commit message");
    }

    Ok(message)
}

fn build_chat_request(config: &SottoConfig, diff: &str) -> ChatRequest {
    let user = format!("Generate a commit message for this diff:\n\n{diff}");

    ChatRequest {
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
    }
}

fn parse_chat_response(body: &str) -> Result<String> {
    let chat: ChatResponse = serde_json::from_str(body).context("failed to parse API response")?;

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

fn warn_if_insecure_endpoint(endpoint: &str) {
    static WARNED: Once = Once::new();

    if std::env::var("SOTTO_ALLOW_HTTP").is_ok_and(|v| v == "1") {
        return;
    }

    let is_http = endpoint.starts_with("http://");
    if !is_http {
        return;
    }

    let host = extract_host(endpoint);
    if host.as_deref().map(is_local_host).unwrap_or(true) {
        return;
    }

    WARNED.call_once(|| {
        eprintln!(
            "warning: endpoint uses http:// with non-local host. \
             api key and diffs will be sent unencrypted. \
             set SOTTO_ALLOW_HTTP=1 to silence."
        );
    });
}

fn extract_host(url: &str) -> Option<String> {
    let without_scheme = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))?;
    let host_port = without_scheme.split('/').next()?;

    if let Some(bracketed) = host_port.strip_prefix('[') {
        let host = bracketed.split(']').next()?;
        return Some(host.to_string());
    }

    let host = host_port.split(':').next()?;
    Some(host.to_string())
}

fn is_local_host(host: &str) -> bool {
    if host == "localhost" {
        return true;
    }

    let Ok(ip) = host.parse::<IpAddr>() else {
        return false;
    };

    match ip {
        IpAddr::V4(v4) => v4.is_loopback() || v4.is_private(),
        IpAddr::V6(v6) => v6.is_loopback(),
    }
}

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

#[derive(Serialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
    stream: bool,
}

#[derive(Deserialize)]
struct OllamaResponse {
    response: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_host_from_urls() {
        assert_eq!(
            extract_host("http://localhost:8080/api"),
            Some("localhost".into())
        );
        assert_eq!(
            extract_host("https://api.openai.com/v1"),
            Some("api.openai.com".into())
        );
        assert_eq!(
            extract_host("http://192.168.1.1:11434/api"),
            Some("192.168.1.1".into())
        );
        assert_eq!(
            extract_host("http://127.0.0.1/path"),
            Some("127.0.0.1".into())
        );
        assert_eq!(extract_host("http://[::1]:8080/api"), Some("::1".into()));
        assert_eq!(
            extract_host("http://[fe80::1]:11434/api"),
            Some("fe80::1".into())
        );
        assert_eq!(extract_host("invalid-url"), None);
    }

    #[test]
    fn local_host_detection() {
        assert!(is_local_host("localhost"));
        assert!(is_local_host("127.0.0.1"));
        assert!(is_local_host("192.168.1.100"));
        assert!(is_local_host("10.0.0.1"));
        assert!(is_local_host("172.16.0.1"));
        assert!(is_local_host("::1"));

        assert!(!is_local_host("api.openai.com"));
        assert!(!is_local_host("8.8.8.8"));
        assert!(!is_local_host("example.com"));
    }
}
