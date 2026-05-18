use std::{fmt::Write, path::Path};

use serde_json::json;

use crate::config::{Paths, SottoConfig};

pub fn parse_location_configs(paths: &Paths) -> anyhow::Result<String> {
    let config_map = json!({
        "cache" : paths.cache_dir,
        "socket" : paths.socket,
        "config" : paths.config,
    });
    let mut report = String::new();

    if let Some(map) = config_map.as_object() {
        for (field_name, value) in map {
            let (icon, detail) = match value.as_str() {
                Some(s) if !Path::new(s).exists() => ("x", "path not found".to_string()),
                _ => (
                    "✓",
                    truncate(value.to_string(), 99).replace(['\\', '"'], ""),
                ),
            };
            let _ = writeln!(&mut report, "[{icon}] {field_name:<13} {detail}");
        }
    }

    Ok(report)
}

pub fn parse_configs(config: SottoConfig) -> anyhow::Result<String> {
    let mut report = String::new();
    let config_map = json!({
        "api_key" : config.api_key,
        "endpoint" : config.endpoint,
        "model" : config.model,
        "debounce": config.debounce_secs,
        "diff": config.max_diff_lines,
        "prompt": config.prompt
    });

    if let Some(map) = config_map.as_object() {
        for (field_name, value) in map {
            let (icon, detail) = match value.as_str() {
                Some(s) if s.is_empty() => ("x", "config field is empty".to_string()),
                _ => (
                    "✓",
                    truncate(value.to_string(), 99).replace(['\\', '"'], ""),
                ),
            };
            let _ = writeln!(&mut report, "[{icon}] {field_name:<13} {detail}");
        }
    }
    Ok(report)
}

pub fn generate_report(locations: String, config: String) -> anyhow::Result<()> {
    let mut report = String::new();

    writeln!(&mut report, "\nSOTTO DOCTOR")?;
    writeln!(&mut report, "\nLocation Config:\n")?;
    writeln!(&mut report, "{}", locations)?;
    writeln!(&mut report, "Sotto Config:\n")?;
    writeln!(&mut report, "{}", config)?;
    println!("{}", &report);

    Ok(())
}

fn truncate(s: String, max_width: usize) -> String {
    s.chars().take(max_width).collect()
}
