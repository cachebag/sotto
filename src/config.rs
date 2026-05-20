// Configuration for sotto state

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{env, fs, path::PathBuf};

impl Paths {
    pub fn resolve() -> Result<Self> {
        let data_dir = xdg_data_home()
            .context("could not resolve data directory")?
            .join("sotto");

        let config_dir = xdg_config_home()
            .context("could not resolve config directory")?
            .join("sotto");

        Ok(Self {
            cache_dir: data_dir.join("cache"),
            socket: data_dir.join("sotto.sock"),
            log: data_dir.join("sotto.log"),
            config: config_dir.join("config.toml"),
        })
    }

    /// Ensure all directories exist
    pub fn init_dirs(&self) -> Result<()> {
        let dirs = [
            self.cache_dir.as_path(),
            self.socket.parent().context("socket path has no parent")?,
            self.log.parent().context("log path has no parent")?,
            self.config.parent().context("config path has no parent")?,
        ];

        for dir in dirs {
            fs::create_dir_all(dir)
                .with_context(|| format!("failed to create dir: {}", dir.display()))?;
        }

        Ok(())
    }
}

impl SottoConfig {
    /// Only call this from `setup` or `doctor`
    /// Daemon and completer should use `load_silent` instead.
    pub fn load(paths: &Paths) -> Result<Self> {
        let contents = fs::read_to_string(&paths.config).context("could not read config.toml")?;
        let partial: SottoConfigPartial =
            toml::from_str(&contents).context("config.toml is malformed")?;
        Ok(partial.into())
    }

    pub fn load_silently(paths: &Paths) -> Option<Self> {
        let contents = fs::read_to_string(&paths.config).ok()?;
        let partial: SottoConfigPartial = toml::from_str(&contents).ok()?;
        Some(partial.into())
    }

    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(self).context("failed to serialize config")
    }
}

impl From<SottoConfigPartial> for SottoConfig {
    fn from(p: SottoConfigPartial) -> Self {
        Self {
            endpoint: p.endpoint,
            model: p.model,
            api_key: p.api_key,
            debounce_secs: p.debounce_secs,
            max_diff_lines: p.max_diff_lines,
            prompt: p.prompt,
            inference_type: p.inference_type,
        }
    }
}

fn xdg_config_home() -> Option<PathBuf> {
    env::var("XDG_CONFIG_HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".config")))
}

fn xdg_data_home() -> Option<PathBuf> {
    env::var("XDG_DATA_HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".local").join("share")))
}

#[derive(Debug, Clone)]
pub struct Paths {
    pub cache_dir: PathBuf,
    pub socket: PathBuf,
    pub log: PathBuf,
    pub config: PathBuf,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SottoConfig {
    pub endpoint: String,
    pub model: String,
    pub api_key: String,
    pub debounce_secs: u64,
    pub max_diff_lines: usize,
    pub prompt: String,
    pub inference_type: String,
}

#[derive(Debug, Deserialize)]
pub struct SottoConfigPartial {
    pub api_key: String,

    #[serde(default = "defaults::endpoint")]
    pub endpoint: String,

    #[serde(default = "defaults::model")]
    pub model: String,

    #[serde(default = "defaults::debounce_secs")]
    pub debounce_secs: u64,

    #[serde(default = "defaults::max_diff_lines")]
    pub max_diff_lines: usize,

    #[serde(default = "defaults::prompt")]
    pub prompt: String,

    #[serde(default = "defaults::inference_type")]
    pub inference_type: String,
}

pub const DEFAULT_PROMPT: &str = "You are a concise git commit message generator. \
    Given a diff, write a single-line commit message. \
    Use conventional commit format (feat:, fix:, refactor:, etc). \
    Return nothing but the commit message.";

pub const DEFAULT_INFERENCE_TYPE: &str = "openrouter";

mod defaults {
    pub fn endpoint() -> String {
        "https://openrouter.ai/api/v1/chat/completions".to_string()
    }
    pub fn model() -> String {
        "nothing".to_string()
    }
    pub fn debounce_secs() -> u64 {
        15
    }
    pub fn max_diff_lines() -> usize {
        500
    }
    pub fn prompt() -> String {
        super::DEFAULT_PROMPT.to_string()
    }
    pub fn inference_type() -> String {
        super::DEFAULT_INFERENCE_TYPE.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_roundtrip_with_special_chars() {
        let config = SottoConfig {
            endpoint: "https://example.com/v1".to_string(),
            model: "gpt-4".to_string(),
            api_key: r#"sk-key"with"quotes"#.to_string(),
            debounce_secs: 10,
            max_diff_lines: 200,
            prompt: "Line one.\nLine two with \"quotes\" and \\backslash.".to_string(),
            inference_type: "openrouter".to_string(),
        };

        let toml_str = config.to_toml().expect("serialization failed");
        let parsed: SottoConfigPartial = toml::from_str(&toml_str).expect("deserialization failed");
        let roundtripped: SottoConfig = parsed.into();

        assert_eq!(config.endpoint, roundtripped.endpoint);
        assert_eq!(config.model, roundtripped.model);
        assert_eq!(config.api_key, roundtripped.api_key);
        assert_eq!(config.debounce_secs, roundtripped.debounce_secs);
        assert_eq!(config.max_diff_lines, roundtripped.max_diff_lines);
        assert_eq!(config.prompt, roundtripped.prompt);
        assert_eq!(config.inference_type, roundtripped.inference_type);
    }

    #[test]
    fn config_roundtrip_multiline_prompt() {
        let config = SottoConfig {
            endpoint: "http://localhost:11434/api".to_string(),
            model: "llama3".to_string(),
            api_key: "".to_string(),
            debounce_secs: 5,
            max_diff_lines: 100,
            prompt: "First line\nSecond line\nThird \"quoted\" line".to_string(),
            inference_type: "ollama".to_string(),
        };

        let toml_str = config.to_toml().expect("serialization failed");
        let parsed: SottoConfigPartial = toml::from_str(&toml_str).expect("deserialization failed");
        let roundtripped: SottoConfig = parsed.into();

        assert_eq!(config.prompt, roundtripped.prompt);
    }
}
