mod health;

use crate::config::{Paths, SottoConfig};

pub fn run(paths: &Paths, config: SottoConfig) -> anyhow::Result<()> {
    let locations = health::parse_location_configs(paths);
    let config_lines = health::parse_configs(&config);
    health::generate_report(locations, config_lines);
    Ok(())
}
