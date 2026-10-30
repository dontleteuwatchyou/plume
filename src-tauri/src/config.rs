use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::mods::ClientMod;

/// Réglages persistés dans `%APPDATA%\app.plume.desktop\config.json` et transmis
/// à la page sous `window.__PLUME__`.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Mod client chargé comme extension : nightcord, equicord ou none.
    #[serde(rename = "mod")]
    pub client_mod: ClientMod,
    pub audio: Audio,
    /// Économie de mémoire du moteur WebView2 (appliquée au redémarrage).
    pub memory: MemoryMode,
    /// Libère la mémoire quand Plume n'est pas au premier plan.
    #[serde(rename = "trimInBackground")]
    pub trim_in_background: bool,
    /// Combinaison PTT forcée, en codes VK (ex. [5] = bouton souris 4).
    /// Vide : on reprend la touche réglée dans Discord.
    #[serde(rename = "pttKeys")]
    pub ptt_keys: Vec<i32>,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemoryMode {
    Standard,
    Reduced,
    Max,
}

impl MemoryMode {
    /// Arguments du navigateur WebView2. Mesuré sur un compte connecté :
    /// standard ~700 Mo, reduced ~615 Mo, max ~560 Mo (mémoire privée).
    pub fn browser_args(self) -> String {
        // Les valeurs par défaut de Tauri, qu'un argument personnalisé remplace.
        // + l'audio sans geste préalable : la chaîne de gain du micro
        // (WebAudio) doit tourner dès que Discord ouvre le micro.
        let base = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --autoplay-policy=no-user-gesture-required";
        let v8 = r#"--js-flags="--optimize-for-size --max-semi-space-size=2""#;
        match self {
            MemoryMode::Standard => base.into(),
            // Le processus graphique tourne dans le processus principal.
            MemoryMode::Reduced => format!("{base} --in-process-gpu {v8}"),
            // Rendu logiciel : le moins de mémoire, plus de processeur.
            MemoryMode::Max => format!("{base} --disable-gpu {v8}"),
        }
    }
}

/// Micro envoyé par WebRTC (Opus). Lu par inject/stereo.js.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Audio {
    pub stereo: bool,
    /// Gain appliqué au micro avant l'envoi : 1.0 = 100 %, jusqu'à 8.0.
    pub gain: f32,
    /// Limiteur après le gain : évite la saturation quand on pousse.
    pub limiter: bool,
    /// Débit Opus visé, en bit/s (Opus plafonne à 510 000).
    pub bitrate: u32,
    /// Durée d'une trame Opus en ms : 10, 20, 40 ou 60.
    pub frame_ms: u32,
    /// Débit constant plutôt que variable.
    pub cbr: bool,
    /// Correction d'erreurs intégrée (résiste aux pertes, coûte du débit).
    pub fec: bool,
    /// Transmission discontinue : n'envoie presque rien pendant les silences.
    pub dtx: bool,
    pub echo_cancellation: bool,
    pub noise_suppression: bool,
    pub auto_gain_control: bool,
    /// Priorité réseau haute pour l'audio.
    pub high_priority: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            client_mod: ClientMod::Nightcord,
            audio: Audio::default(),
            memory: MemoryMode::Reduced,
            trim_in_background: true,
            ptt_keys: Vec::new(),
        }
    }
}

impl Default for Audio {
    fn default() -> Self {
        Self {
            stereo: true,
            gain: 1.0,
            limiter: true,
            bitrate: 510_000,
            frame_ms: 20,
            cbr: false,
            fec: true,
            dtx: false,
            echo_cancellation: false,
            noise_suppression: false,
            auto_gain_control: false,
            high_priority: true,
        }
    }
}

impl Config {
    fn path(app: &AppHandle) -> Option<std::path::PathBuf> {
        app.path().app_config_dir().ok().map(|d| d.join("config.json"))
    }

    /// Ramène les valeurs dans ce qu'Opus et WebRTC acceptent.
    pub fn sanitize(&mut self) {
        let a = &mut self.audio;
        a.bitrate = a.bitrate.clamp(6_000, 510_000);
        a.gain = if a.gain.is_finite() { a.gain.clamp(0.0, 8.0) } else { 1.0 };
        if ![10, 20, 40, 60].contains(&a.frame_ms) {
            a.frame_ms = 20;
        }
    }

    pub fn load(app: &AppHandle) -> Self {
        let mut cfg: Self = Self::path(app)
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        cfg.sanitize();
        cfg
    }

    pub fn save(&self, app: &AppHandle) {
        if let Some(p) = Self::path(app) {
            let _ = std::fs::create_dir_all(p.parent().unwrap());
            let _ = std::fs::write(p, serde_json::to_string_pretty(self).unwrap());
        }
    }
}
