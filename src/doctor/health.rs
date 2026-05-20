use std::path::Path;

use crate::config::{Paths, SottoConfig};
use crate::ui;

pub fn parse_location_configs(paths: &Paths) -> Vec<String> {
    let items = [
        ("cache", paths.cache_dir.display().to_string()),
        ("socket", paths.socket.display().to_string()),
        ("config", paths.config.display().to_string()),
    ];

    items
        .into_iter()
        .map(|(label, path)| {
            let (icon, detail) = if !Path::new(&path).exists() {
                ("x", "path not found".to_string())
            } else {
                ("✓", truncate(&path, 99))
            };
            ui::status_line(icon, label, &detail)
        })
        .collect()
}

pub fn parse_configs(config: &SottoConfig) -> Vec<String> {
    let items = [
        ("api_key", mask_secret(&config.api_key)),
        ("endpoint", config.endpoint.clone()),
        ("model", config.model.clone()),
        ("debounce", config.debounce_secs.to_string()),
        ("diff", config.max_diff_lines.to_string()),
        ("prompt", truncate(&config.prompt, 60)),
        ("inference_type", config.inference_type.clone()),
    ];

    items
        .into_iter()
        .map(|(label, value)| {
            let (icon, detail) = if value.is_empty() {
                ("x", "config field is empty".to_string())
            } else {
                ("✓", value)
            };
            ui::status_line(icon, label, &detail)
        })
        .collect()
}

pub fn generate_report(locations: Vec<String>, config: Vec<String>) {
    ui::header("SOTTO DOCTOR");
    print!("{}", ui::report_section("Location Config:", &locations));
    print!("{}", ui::report_section("Sotto Config:", &config));
}

fn truncate(s: &str, max_width: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_width {
        return s.to_string();
    }
    if max_width <= 3 {
        return s.chars().take(max_width).collect();
    }
    format!("{}...", s.chars().take(max_width - 3).collect::<String>())
}

fn mask_secret(s: &str) -> String {
    let char_count = s.chars().count();
    if char_count == 0 {
        return String::new();
    }
    if char_count <= 8 {
        return "*".repeat(char_count);
    }
    let prefix: String = s.chars().take(4).collect();
    let suffix: String = s.chars().skip(char_count - 4).collect();
    format!("{prefix}...{suffix}")
}
