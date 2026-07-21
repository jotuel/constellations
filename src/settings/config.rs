use cosmic_config::{CosmicConfigEntry, cosmic_config_derive::CosmicConfigEntry};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, CosmicConfigEntry)]
#[version = 1]
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
    pub fn load() -> Self {
        if let Ok(config_handler) = cosmic_config::Config::new("fi.joonastuomi.Constellations", 1) {
            match Self::get_entry(&config_handler) {
                Ok(config) => config,
                Err((errors, fallback)) => {
                    for err in errors {
                        tracing::warn!("Failed to load config from COSMIC Config: {:?}", err);
                    }
                    fallback
                }
            }
        } else {
            tracing::warn!("Failed to create COSMIC Config handler, using default config");
            Self::default()
        }
    }

    pub fn save(&self) -> Result<(), String> {
        if let Ok(config_handler) = cosmic_config::Config::new("fi.joonastuomi.Constellations", 1) {
            self.write_entry(&config_handler)
                .map_err(|e| format!("Failed to save config to COSMIC Config: {:?}", e))
        } else {
            Err("Failed to create COSMIC Config handler".to_string())
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
    #[serial_test::serial]
    fn test_config_save_load() {
        let tmp_dir = tempdir().unwrap();
        unsafe {
            std::env::set_var("HOME", tmp_dir.path());
            std::env::set_var("XDG_CONFIG_HOME", tmp_dir.path());
            std::env::set_var("APPDATA", tmp_dir.path());
        }

        let config = Config {
            show_sync_indicator: true,
            ..Default::default()
        };

        config.save().expect("Failed to save config");

        let loaded = Config::load();
        assert_eq!(config, loaded);
    }

    #[test]
    #[serial_test::serial]
    fn test_config_load_nonexistent() {
        let tmp_dir = tempdir().unwrap();
        unsafe {
            std::env::set_var("HOME", tmp_dir.path());
            std::env::set_var("XDG_CONFIG_HOME", tmp_dir.path());
            std::env::set_var("APPDATA", tmp_dir.path());
        }

        let loaded = Config::load();
        assert_eq!(loaded, Config::default());
    }
}
