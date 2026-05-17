//! Persistência mínima de `config.toml` (seção clipboard).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfigFile {
    #[serde(default)]
    pub clipboard: ClipboardConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardConfig {
    #[serde(default = "default_true")]
    pub auto_sync: bool,
}

impl Default for ClipboardConfig {
    fn default() -> Self {
        Self { auto_sync: true }
    }
}

pub struct TomlConfigStore {
    config_dir: PathBuf,
}

impl TomlConfigStore {
    pub fn new(config_dir: PathBuf) -> Self {
        Self { config_dir }
    }

    fn config_path(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    pub fn load_or_create(&self) -> anyhow::Result<AppConfigFile> {
        let path = self.config_path();
        if !path.exists() {
            let cfg = AppConfigFile::default();
            self.save(&cfg)?;
            return Ok(cfg);
        }
        let raw = std::fs::read_to_string(&path)?;
        Ok(toml::from_str(&raw).unwrap_or_default())
    }

    pub fn save(&self, config: &AppConfigFile) -> anyhow::Result<()> {
        std::fs::create_dir_all(&self.config_dir)?;
        let raw = toml::to_string_pretty(config)?;
        std::fs::write(self.config_path(), raw)?;
        Ok(())
    }

    pub fn save_clipboard_auto_sync(&self, enabled: bool) -> anyhow::Result<()> {
        let mut cfg = self.load_or_create()?;
        cfg.clipboard.auto_sync = enabled;
        self.save(&cfg)
    }
}
