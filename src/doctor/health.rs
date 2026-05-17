use std::{fmt::Write, path::Path};

use serde_json::json;

use crate::config::{Paths, SottoConfig};

pub fn parse_location_configs(paths: &Paths) -> anyhow::Result<String> {
    let config_map = json!({
        "cached dir" : paths.cache_dir,
        "socket" : paths.socket,
        "config" : paths.config,
    });
    let mut report = String::new();

    if let Some(map) = config_map.as_object() {
        for (field_name, value) in map {
            if let Some(path_str) = value.as_str() {
                if !Path::new(path_str).exists() {
                    let _ = writeln!(
                        &mut report,
                        "[X] {:<19} {:<20}",
                        field_name, "path not found"
                    );
                } else {
                    let _ = writeln!(
                        &mut report,
                        "[✔] {:<18} {}",
                        field_name,
                        value.to_string().replace('\\', " ").replace('"', " ")
                    );
                }
            }
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
        "diff lines": config.max_diff_lines,
        "prompt": config.prompt
    });
    if let Some(map) = config_map.as_object() {
        for (field_name, value) in map {
            if let Some(s) = value.as_str() {
                if s.is_empty() {
                    let _ = writeln!(
                        &mut report,
                        "[X] {:<19} {:<20}",
                        field_name, "config field is empty"
                    );
                } else {
                    let _ = writeln!(
                        &mut report,
                        "[✔] {:<18} {}",
                        field_name,
                        truncate(value.to_string(), 99)
                            .replace('\\', " ")
                            .replace('"', " ")
                    );
                }
            }
        }
    }

    Ok(report)
}

pub fn generate_report(locations: String, config: String) -> anyhow::Result<()> {
    let mut report = String::new();

    writeln!(&mut report, "{}", "\nSOTTO DOCTOR")?;

    writeln!(&mut report, "{}", "\nLocation Config:\n")?;
    writeln!(&mut report, "{}", locations)?;
    writeln!(&mut report, "{}", "Sotto Config:\n")?;
    writeln!(&mut report, "{}", config)?;
    println!("{}", &report);

    Ok(())
}

fn truncate(s: String, max_width: usize) -> String {
    s.chars().take(max_width).collect()
}
