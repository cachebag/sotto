// Configuration for sotto state

use anyhow::{Context, Result};
use serde::Deserialize;
use std::{env, fs, path::PathBuf};

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

#[derive(Debug, Clone)]
pub struct Paths {
    pub cache_dir: PathBuf,
    pub socket: PathBuf,
    pub log: PathBuf,
    pub config: PathBuf,
}

#[derive(Debug, Deserialize)]
pub struct SottoConfig {
    pub api_key: String,
    pub endpoint: String,

    #[serde(default = "defaults::model")]
    pub model: String,

    #[serde(default = "defaults::debounce_secs")]
    pub debounce_secs: u64,

    #[serde(default = "defaults::max_diff_lines")]
    pub max_diff_lines: usize,
    // #[serde(default = "defaults::line_delta_threshold")]
    // pub line_delta_threshold: usize,
    #[serde(default = "defaults::prompt")]
    pub prompt: String,
}

impl SottoConfig {
    /// Only call this from `setup` or `doctor`
    /// Daemon and completer should use `load_silent` instead.
    pub fn load(paths: &Paths) -> Result<Self> {
        let contents = fs::read_to_string(&paths.config).context("could not read config.toml")?;

        toml::from_str(&contents).context("config.toml is malformed")
    }

    pub fn load_silently(paths: &Paths) -> Option<Self> {
        let contents = fs::read_to_string(&paths.config).ok()?;
        toml::from_str(&contents).ok()
    }
}

pub const DEFAULT_PROMPT: &str = "You are a concise git commit message generator. \
    Given a diff, write a single-line commit message. \
    Use conventional commit format (feat:, fix:, refactor:, etc). \
    Return nothing but the commit message.";

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
    // I don't know if this is needed right now
    // But if token waste becomes an issue then it's worth it
    // pub fn line_delta_threshold() -> usize {
    //    10
    // }
    pub fn prompt() -> String {
        super::DEFAULT_PROMPT.to_string()
    }
}
