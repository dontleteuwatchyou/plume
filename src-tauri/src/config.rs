use serde::{Deserialize, Serialize};

use crate::mods::ClientMod;
use tauri::{AppHandle, Manager};

/// Réglages persistés dans `%APPDATA%\plume\config.json` et transmis à la page
/// sous `window.__PLUME__`.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Mod client chargé comme extension : nightcord, equicord ou none.
    #[serde(rename = "mod")]
    pub client_mod: ClientMod,
    pub stereo: bool,
    /// Débit Opus visé pour le micro, en bit/s (Opus plafonne à 510 000).
    pub bitrate: u32,
    /// Combinaison PTT forcée, en codes VK (ex. [5] = bouton souris 4).
    /// Vide : on reprend la touche réglée dans Discord.
    #[serde(rename = "pttKeys")]
    pub ptt_keys: Vec<i32>,
}

impl Default for Config {
    fn default() -> Self {
        Self { client_mod: ClientMod::Nightcord, stereo: true, bitrate: 510_000, ptt_keys: Vec::new() }
    }
}

impl Config {
    fn path(app: &AppHandle) -> Option<std::path::PathBuf> {
        app.path().app_config_dir().ok().map(|d| d.join("config.json"))
    }

    pub fn load(app: &AppHandle) -> Self {
        let mut cfg: Self = Self::path(app)
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        cfg.bitrate = cfg.bitrate.clamp(6_000, 510_000);
        cfg
    }

    pub fn save(&self, app: &AppHandle) {
        if let Some(p) = Self::path(app) {
            let _ = std::fs::create_dir_all(p.parent().unwrap());
            let _ = std::fs::write(p, serde_json::to_string_pretty(self).unwrap());
        }
    }
}
