// SPDX-License-Identifier: GPL-3.0-only

//! Persistent settings (stored with `cosmic-config`).

use cosmic::cosmic_config::{self, CosmicConfigEntry, cosmic_config_derive::CosmicConfigEntry};
use serde::{Deserialize, Serialize};

use crate::lyrics::Provider;
use crate::queue::Repeat;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppTheme {
    #[default]
    System,
    Dark,
    Light,
}

impl AppTheme {
    pub const ALL: [Self; 3] = [Self::System, Self::Dark, Self::Light];

    pub fn theme(self) -> cosmic::Theme {
        match self {
            Self::System => cosmic::theme::system_preference(),
            Self::Dark => cosmic::Theme::dark(),
            Self::Light => cosmic::Theme::light(),
        }
    }

    #[must_use]
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|t| *t == self).unwrap_or(0)
    }
}

#[derive(Debug, Clone, CosmicConfigEntry, PartialEq)]
#[version = 1]
pub struct Config {
    /// Folders indexed in the library (scanned recursively).
    pub library_folders: Vec<String>,
    /// Re-index automatically when files change.
    pub watch_folders: bool,
    pub app_theme: AppTheme,

    // Playback
    pub volume: f32,
    pub muted: bool,
    pub shuffle: bool,
    pub repeat: Repeat,
    pub replay_gain: bool,
    pub gapless: bool,

    // Session restore
    pub resume_paths: Vec<String>,
    pub resume_index: usize,
    pub resume_position_ms: u64,

    // Lyrics plugins, in priority order
    pub lyrics_order: Vec<Provider>,
    pub lyrics_disabled: Vec<Provider>,

    // Last.fm
    pub lastfm_api_key: String,
    pub lastfm_secret: String,
    pub lastfm_session: String,
    pub lastfm_user: String,
    pub lastfm_scrobble: bool,
    pub lastfm_now_playing: bool,
    pub lastfm_love: bool,

    // Discord
    pub discord_enabled: bool,
    pub discord_client_id: String,
    /// Look covers up online (Last.fm / iTunes) so Discord can display them.
    pub discord_online_covers: bool,
}

/// Official Discord application ("Cosmic Advanced Player"): its name is what
/// Discord shows in "Listening to …". Another one can be set in the settings.
pub const DEFAULT_DISCORD_CLIENT_ID: &str = "1571841011779633162";

impl Default for Config {
    fn default() -> Self {
        let music = dirs::audio_dir().filter(|d| d.is_dir());
        Self {
            library_folders: music
                .map(|d| d.to_string_lossy().into_owned())
                .into_iter()
                .collect(),
            watch_folders: true,
            app_theme: AppTheme::System,
            volume: 0.8,
            muted: false,
            shuffle: false,
            repeat: Repeat::Off,
            replay_gain: true,
            gapless: true,
            resume_paths: Vec::new(),
            resume_index: 0,
            resume_position_ms: 0,
            lyrics_order: Provider::ALL.to_vec(),
            lyrics_disabled: Vec::new(),
            lastfm_api_key: String::new(),
            lastfm_secret: String::new(),
            lastfm_session: String::new(),
            lastfm_user: String::new(),
            lastfm_scrobble: true,
            lastfm_now_playing: true,
            lastfm_love: true,
            discord_enabled: true,
            discord_client_id: String::new(),
            discord_online_covers: true,
        }
    }
}

impl Config {
    /// Loads the configuration, falling back to defaults on errors.
    #[must_use]
    pub fn load(app_id: &str) -> (Option<cosmic_config::Config>, Self) {
        match cosmic_config::Config::new(app_id, Self::VERSION) {
            Ok(handler) => {
                let mut config = match Self::get_entry(&handler) {
                    Ok(config) => config,
                    Err((_errors, config)) => config,
                };
                config.normalize();
                (Some(handler), config)
            }
            Err(err) => {
                eprintln!("failed to open config: {err}");
                (None, Self::default())
            }
        }
    }

    /// Makes sure every lyrics provider appears exactly once in the order list.
    pub fn normalize(&mut self) {
        let mut seen = Vec::new();
        self.lyrics_order.retain(|p| {
            let fresh = !seen.contains(p);
            seen.push(*p);
            fresh
        });
        for p in Provider::ALL {
            if !self.lyrics_order.contains(&p) {
                self.lyrics_order.push(p);
            }
        }
        self.volume = self.volume.clamp(0.0, 1.0);
    }

    /// Enabled lyrics providers, in priority order.
    #[must_use]
    pub fn lyrics_providers(&self) -> Vec<Provider> {
        self.lyrics_order
            .iter()
            .copied()
            .filter(|p| !self.lyrics_disabled.contains(p))
            .collect()
    }

    /// Discord application to use: the custom one, or the official one when empty.
    #[must_use]
    pub fn discord_app_id(&self) -> &str {
        let id = self.discord_client_id.trim();
        if id.is_empty() {
            DEFAULT_DISCORD_CLIENT_ID
        } else {
            id
        }
    }

    #[must_use]
    pub fn lastfm_connected(&self) -> bool {
        !self.lastfm_session.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_lyrics_order() {
        let mut c = Config {
            lyrics_order: vec![Provider::Lrclib, Provider::Lrclib],
            lyrics_disabled: vec![Provider::Embedded],
            volume: 3.0,
            ..Config::default()
        };
        c.normalize();
        assert_eq!(c.lyrics_order.len(), 4);
        assert_eq!(c.lyrics_order[0], Provider::Lrclib);
        assert!(!c.lyrics_providers().contains(&Provider::Embedded));
        assert!((c.volume - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn empty_discord_id_uses_the_official_app() {
        let mut c = Config::default();
        assert_eq!(c.discord_app_id(), DEFAULT_DISCORD_CLIENT_ID);
        c.discord_client_id = " 42 ".into();
        assert_eq!(c.discord_app_id(), "42");
    }
}
