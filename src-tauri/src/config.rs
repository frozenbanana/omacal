use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub locale: LocaleConfig,
    pub sync_interval_secs: u64,
    pub accounts: Vec<AccountConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocaleConfig {
    pub timezone: String,
    pub time_24h: bool,
    pub week_starts_on: u8, // 0=Sun, 1=Mon
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountConfig {
    pub id: String,
    pub display_name: String,
    pub caldav_url: String,
    pub username: String,
    /// Email addresses used for PARTSTAT / invite matching
    #[serde(default)]
    pub addresses: Vec<String>,
    pub enabled: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            locale: LocaleConfig {
                timezone: "Europe/Stockholm".into(),
                time_24h: true,
                week_starts_on: 1,
            },
            sync_interval_secs: 300,
            accounts: vec![],
        }
    }
}

pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("omarcal")
}

pub fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("omarcal")
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

pub fn db_path() -> PathBuf {
    data_dir().join("omarcal.db")
}

pub fn ensure_dirs() -> anyhow::Result<()> {
    fs::create_dir_all(config_dir())?;
    fs::create_dir_all(data_dir())?;
    Ok(())
}

pub fn load_config() -> anyhow::Result<AppConfig> {
    ensure_dirs()?;
    let path = config_path();
    if !path.exists() {
        let cfg = AppConfig::default();
        save_config(&cfg)?;
        return Ok(cfg);
    }
    let text = fs::read_to_string(&path)?;
    let cfg: AppConfig = toml::from_str(&text)?;
    Ok(cfg)
}

pub fn save_config(cfg: &AppConfig) -> anyhow::Result<()> {
    ensure_dirs()?;
    let text = toml::to_string_pretty(cfg)?;
    fs::write(config_path(), text)?;
    Ok(())
}
