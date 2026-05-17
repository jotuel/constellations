use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    pub show_sync_indicator: bool,
    pub send_typing_notifications: bool,
    pub render_markdown: bool,
    pub compact_mode: bool,
    pub hide_threaded_messages: bool,
    pub media_previews_display_policy: bool,
    pub invite_avatars_display_policy: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            show_sync_indicator: false,
            send_typing_notifications: false,
            render_markdown: false,
            compact_mode: false,
            hide_threaded_messages: true,
            media_previews_display_policy: true,
            invite_avatars_display_policy: true,
        }
    }
}

impl Config {
    fn config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("fi.joonastuomi.Constellations").join("config.json"))
    }

    pub fn load() -> Self {
        Self::load_from(Self::config_path())
    }

    pub fn load_from(path: Option<PathBuf>) -> Self {
        if let Some(path) = path
            && path.exists()
        {
            if let Ok(file) = std::fs::File::open(path) {
                if let Ok(config) = serde_json::from_reader(file) {
                    return config;
                } else {
                    tracing::warn!("Failed to deserialize config, using defaults");
                }
            } else {
                tracing::warn!("Failed to open config file, using defaults");
            }
        }
        Self::default()
    }

    pub fn save(&self) -> Result<(), String> {
        self.save_to(Self::config_path())
    }

    pub fn save_to(&self, path: Option<PathBuf>) -> Result<(), String> {
        if let Some(path) = path {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
            serde_json::to_writer_pretty(file, self).map_err(|e| e.to_string())?;
            Ok(())
        } else {
            Err("Failed to get config directory".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_config_serialization() {
        let config = Config {
            show_sync_indicator: true,
            send_typing_notifications: true,
            render_markdown: true,
            compact_mode: true,
            hide_threaded_messages: true,
            media_previews_display_policy: false,
            invite_avatars_display_policy: false,
        };

        let serialized = serde_json::to_string(&config).unwrap();
        let deserialized: Config = serde_json::from_str(&serialized).unwrap();

        assert_eq!(config, deserialized);
    }

    #[test]
    fn test_config_save_load() {
        let tmp_dir = tempdir().unwrap();
        let config_path = tmp_dir.path().join("config.json");

        let config = Config {
            show_sync_indicator: true,
            ..Default::default()
        };

        config
            .save_to(Some(config_path.clone()))
            .expect("Failed to save config");

        let loaded = Config::load_from(Some(config_path));
        assert_eq!(config, loaded);
    }

    #[test]
    fn test_config_load_nonexistent() {
        let tmp_dir = tempdir().unwrap();
        let config_path = tmp_dir.path().join("nonexistent.json");

        let loaded = Config::load_from(Some(config_path));
        assert_eq!(loaded, Config::default());
    }
}
