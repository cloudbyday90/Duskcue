// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use clap::Parser;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "duskcue",
    version,
    about = "Self-hosted media streaming server"
)]
pub struct CliArgs {
    #[arg(long, env = "DUSKCUE_DATABASE_URL")]
    pub database_url: Option<String>,

    #[arg(long, env = "DUSKCUE_BIND_ADDRESS", default_value = "0.0.0.0")]
    pub bind_address: String,

    #[arg(long, env = "DUSKCUE_PORT", default_value_t = 48027)]
    pub port: u16,

    #[arg(long, env = "DUSKCUE_DATA_DIR")]
    pub data_dir: Option<PathBuf>,

    #[arg(long, env = "DUSKCUE_CACHE_DIR")]
    pub cache_dir: Option<PathBuf>,

    #[arg(long, env = "DUSKCUE_LOG_LEVEL", default_value = "info")]
    pub log_level: String,

    #[arg(long, env = "DUSKCUE_ENVIRONMENT", default_value = "production")]
    pub environment: String,

    #[arg(long, env = "DUSKCUE_ENCRYPTION_KEY")]
    pub encryption_key: Option<String>,

    #[arg(long, env = "DUSKCUE_GEOIP_LICENSE_KEY")]
    pub geoip_license_key: Option<String>,

    #[arg(long, env = "DUSKCUE_CONFIG")]
    pub config: Option<PathBuf>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct BootstrapConfig {
    pub database_url: Option<String>,
    pub bind_address: String,
    pub port: u16,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub log_level: String,
    pub environment: String,
    pub encryption_key: Option<String>,
    pub geoip_license_key: Option<String>,
}

fn default_data_dir() -> PathBuf {
    if cfg!(target_os = "windows") {
        std::env::var("PROGRAMDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("C:\\ProgramData\\Duskcue"))
    } else if cfg!(target_os = "macos") {
        dirs::home_dir()
            .map(|h| h.join("Library/Application Support/Duskcue"))
            .unwrap_or_else(|| PathBuf::from("/var/lib/duskcue"))
    } else {
        PathBuf::from("/var/lib/duskcue")
    }
}

pub fn build_bootstrap_config(cli: CliArgs) -> Result<BootstrapConfig, Box<dyn std::error::Error>> {
    let data_dir = cli.data_dir.unwrap_or_else(default_data_dir);
    let cache_dir = cli.cache_dir.unwrap_or_else(|| data_dir.join("cache"));

    let config_path = cli
        .config
        .or_else(|| std::env::var("DUSKCUE_CONFIG").ok().map(PathBuf::from))
        .unwrap_or_else(|| data_dir.join("config/config.toml"));

    let mut builder = config::Config::builder()
        .set_default("log_level", cli.log_level.as_str())?
        .set_default("environment", cli.environment.as_str())?
        .set_default("bind_address", cli.bind_address.as_str())?
        .set_default("port", i64::from(cli.port))?
        .set_default("data_dir", data_dir.to_str().unwrap_or(""))?
        .set_default("cache_dir", cache_dir.to_str().unwrap_or(""))?
        .add_source(config::File::from(config_path).required(false))
        .add_source(
            config::Environment::with_prefix("DUSKCUE")
                .prefix_separator("_")
                .separator("_"),
        );

    builder = builder.set_override_option("database_url", cli.database_url)?;
    builder = builder.set_override_option("encryption_key", cli.encryption_key)?;
    builder = builder.set_override_option("geoip_license_key", cli.geoip_license_key)?;
    builder = builder.set_override("bind_address", cli.bind_address)?;
    builder = builder.set_override("port", i64::from(cli.port))?;

    let settings = builder.build()?;

    let mut bootstrap: BootstrapConfig = settings.try_deserialize()?;

    if bootstrap.data_dir.as_os_str().is_empty() {
        bootstrap.data_dir = data_dir;
    }
    if bootstrap.cache_dir.as_os_str().is_empty() {
        bootstrap.cache_dir = cache_dir;
    }

    if bootstrap.bind_address.trim().is_empty() {
        return Err("bind_address cannot be empty".into());
    }
    if bootstrap.port == 0 {
        return Err("port must be between 1 and 65535".into());
    }

    let valid_environments = ["development", "staging", "production"];
    if !valid_environments.contains(&bootstrap.environment.as_str()) {
        return Err(format!(
            "Invalid environment '{}'. Must be one of: development, staging, production",
            bootstrap.environment
        )
        .into());
    }

    Ok(bootstrap)
}
