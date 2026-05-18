use crate::config::{Paths, SottoConfig};

mod health;

pub fn run(paths: &Paths, config: SottoConfig) -> anyhow::Result<()> {
    let parsed_locations_config = health::parse_location_configs(paths)?;
    let parsed_sotto_config = health::parse_configs(config)?;
    health::generate_report(parsed_locations_config, parsed_sotto_config)?;
    Ok(())
}
