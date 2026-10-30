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
    /// Thème de l'interface : "plume" (par défaut) ou "discord".
    pub theme: String,
    /// Économie de mémoire du moteur WebView2 (appliquée au redémarrage).
    pub memory: MemoryMode,
    /// Libère la mémoire quand Plume n'est pas au premier plan.
    #[serde(rename = "trimInBackground")]
    pub trim_in_background: bool,
    /// Cherche une nouvelle version de Plume au démarrage.
    #[serde(rename = "autoUpdate")]
    pub auto_update: bool,
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
    /// standard ~700 Mo, reduced ~570-590 Mo, max ~560 Mo (mémoire privée).
    pub fn browser_args(self) -> String {
        // Les valeurs par défaut de Tauri, qu'un argument personnalisé remplace.
        // + l'audio sans geste préalable : la chaîne de gain du micro
        // (WebAudio) doit tourner dès que Discord ouvre le micro.
        // + la capture de sa propre page acceptée sans sélecteur (seulement les
        // demandes `preferCurrentTab`) : l'enregistreur d'appels filme Plume.
        let base = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --autoplay-policy=no-user-gesture-required --auto-accept-this-tab-capture";
        // V8 compact : petite jeune génération. Jamais `--lite-mode` ni
        // `--jitless` : ils coupent WebAssembly, dont le vocal de Discord a
        // besoin (chiffrement DAVE) ; sans lui, l'appel se reconnecte en boucle.
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
    /// Suppression de bruit RNNoise (WebAssembly), avant le reste de la chaîne.
    pub rnnoise: bool,
    pub eq: Eq,
}

/// Égaliseur du micro : passe-haut + 5 bandes, en dB.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Eq {
    pub enabled: bool,
    /// Fréquence du passe-haut en Hz ; 0 = coupé.
    pub highpass: u32,
    /// Grave 120 Hz, bas-médium 400 Hz, médium 1,5 kHz, présence 4 kHz, air 10 kHz.
    pub bands: [f32; 5],
}

impl Default for Eq {
    fn default() -> Self {
        Self { enabled: false, highpass: 80, bands: [0.0; 5] }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            client_mod: ClientMod::Nightcord,
            audio: Audio::default(),
            theme: "plume".into(),
            memory: MemoryMode::Reduced,
            trim_in_background: true,
            auto_update: true,
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
            rnnoise: false,
            eq: Eq::default(),
        }
    }
}

impl Config {
    fn path(app: &AppHandle) -> Option<std::path::PathBuf> {
        app.path().app_config_dir().ok().map(|d| d.join("config.json"))
    }

    /// Ramène les valeurs dans ce qu'Opus et WebRTC acceptent.
    pub fn sanitize(&mut self) {
        if self.theme != "discord" {
            self.theme = "plume".into();
        }
        let a = &mut self.audio;
        a.bitrate = a.bitrate.clamp(6_000, 510_000);
        a.gain = if a.gain.is_finite() { a.gain.clamp(0.0, 8.0) } else { 1.0 };
        a.eq.highpass = a.eq.highpass.min(400);
        for b in &mut a.eq.bands {
            *b = if b.is_finite() { b.clamp(-15.0, 15.0) } else { 0.0 };
        }
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
