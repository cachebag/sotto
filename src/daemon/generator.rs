use std::net::IpAddr;
use std::sync::Once;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::SottoConfig;

pub fn generate(config: &SottoConfig, diff: &str) -> Result<String> {
    warn_if_insecure_endpoint(&config.endpoint);

    let body = build_request(config, diff);

    let response = ureq::post(&config.endpoint)
        .header("Authorization", &format!("Bearer {}", config.api_key))
        .send_json(&body)?;

    let body_str = response.into_body().read_to_string()?;

    parse_response(&body_str)
}

fn build_request(config: &SottoConfig, diff: &str) -> ChatRequest {
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

fn parse_response(body: &str) -> Result<String> {
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

    if std::env::var("SOTTO_ALLOW_HTTP").is_ok() {
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
