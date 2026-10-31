use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::mods::ClientMod;

/// Réglages persistés dans `%APPDATA%\app.plume.desktop\config.json` et transmis
/// à la page sous `window.__PLUME__`.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Mod client chargé comme extension : plumecord, nightcord, equicord ou none.
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
    /// Lancer Plume (caché) à l'ouverture de la session Windows (autostart.rs).
    pub autostart: bool,
    /// Combinaison PTT forcée, en codes VK (ex. [5] = bouton souris 4).
    /// Vide : on reprend la touche réglée dans Discord.
    #[serde(rename = "pttKeys")]
    pub ptt_keys: Vec<i32>,
    /// Compteur de RAM affiché par-dessus Discord (inject/ram.js).
    #[serde(rename = "ramWidget")]
    pub ram_widget: RamWidget,
    /// Faux à la toute première installation : les plugins du mod que celui-ci
    /// active d'office sont alors tous coupés une fois (inject/plugins.js).
    /// Les configs d'avant ce réglage valent vrai : on ne touche pas aux
    /// plugins de quelqu'un qui utilise déjà Plume.
    #[serde(rename = "pluginsCleared")]
    pub plugins_cleared: bool,
    /// Passage de Nightcord à Plumecord fait (une seule fois, à la mise à jour
    /// vers la 0.3.12). Faux dans les configs d'avant : on bascule alors ceux
    /// qui étaient sur Nightcord ; Equicord et « aucun » restent un choix.
    #[serde(rename = "plumecordMigrated", default)]
    pub plumecord_migrated: bool,
    /// La liste Communauté est passée « activée par défaut » en 0.3.13. Faux
    /// dans les configs d'avant : leur case décochée était l'ancien défaut,
    /// pas un choix, on l'active donc une fois.
    #[serde(rename = "listedByDefault", default)]
    pub listed_by_default: bool,
    pub community: Community,
    /// Veille : Discord déchargé quand Plume ne sert pas (sleep.rs).
    pub sleep: SleepCfg,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SleepCfg {
    pub enabled: bool,
    /// Minutes hors du premier plan avant la veille.
    pub minutes: u32,
    /// Seulement dans la zone de notification, pas juste derrière un jeu.
    pub tray_only: bool,
}

impl Default for SleepCfg {
    fn default() -> Self {
        Self { enabled: true, minutes: 5, tray_only: false }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RamWidget {
    pub enabled: bool,
    /// Privée + totale + nombre de processus, au lieu du seul chiffre.
    pub detailed: bool,
    pub spot: RamSpot,
    /// Position de `RamSpot::Custom`, en fraction de la place libre
    /// (0 = gauche/haut, 1 = droite/bas) : le compteur reste au même endroit
    /// relatif quand la fenêtre change de taille.
    pub x: f64,
    pub y: f64,
}

/// Emplacement du compteur. Il ne bouge pas tout seul : seul le mode
/// « Déplacer » du panneau le rend déplaçable, et il passe alors en `Custom`.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RamSpot {
    /// Dans la barre du haut, à gauche des boutons de la fenêtre.
    Bar,
    TopRight,
    BottomRight,
    Custom,
}

impl Default for RamWidget {
    fn default() -> Self {
        Self { enabled: false, detailed: false, spot: RamSpot::Bar, x: 1.0, y: 0.08 }
    }
}

/// Compteur d'utilisateurs (community.rs). `id` est tiré au hasard et ne dit
/// rien de la personne ; le profil Discord n'est envoyé que si `listed`.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Community {
    pub id: String,
    /// Apparaître dans la liste des utilisateurs (pseudo + avatar Discord).
    /// Activé par défaut, décochable dans Paramètres Plume → Communauté.
    pub listed: bool,
    /// Badge Plume sur le profil, visible des autres utilisateurs de Plume :
    /// envoie l'identifiant Discord (rien d'autre) au serveur.
    pub badge: bool,
}

impl Default for Community {
    fn default() -> Self {
        Self { id: String::new(), listed: true, badge: true }
    }
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
        // + des services de Chromium inutiles ici, comme discord-lite :
        // traduction, conseils d'optimisation, Cast, cache « page précédente »,
        // processus de rendu « de réserve » préchargé, et les requêtes de fond
        // du moteur (mesures de fiabilité, pings des liens).
        let base = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,Translate,OptimizationHints,MediaRouter,BackForwardCache,SpareRendererForSitePerProcess --disable-background-networking --disable-domain-reliability --no-pings --autoplay-policy=no-user-gesture-required --auto-accept-this-tab-capture";
        // V8 compact : petite jeune génération. Jamais `--lite-mode` ni
        // `--jitless` : ils coupent WebAssembly, dont le vocal de Discord a
        // besoin (chiffrement DAVE) ; sans lui, l'appel se reconnecte en boucle.
        let v8 = r#"--js-flags="--optimize-for-size --max-semi-space-size=2""#;
        match self {
            MemoryMode::Standard => base.into(),
            // Pas de `--in-process-gpu` : la carte graphique dans le processus
            // principal faisait saccader la page (mesuré sur un serveur de
            // 500 000 membres : 18 à 56 blocages de plus de 50 ms en 15 s, contre
            // 0 à 2 sans), pour un gain de mémoire négligeable.
            MemoryMode::Reduced => format!("{base} {v8}"),
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
            client_mod: ClientMod::Plumecord,
            audio: Audio::default(),
            theme: "plume".into(),
            memory: MemoryMode::Reduced,
            trim_in_background: true,
            auto_update: true,
            autostart: false,
            ptt_keys: Vec::new(),
            ram_widget: RamWidget::default(),
            plugins_cleared: true,
            plumecord_migrated: true,
            listed_by_default: true,
            community: Community::default(),
            sleep: SleepCfg::default(),
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
        self.sleep.minutes = self.sleep.minutes.clamp(1, 120);
        let w = &mut self.ram_widget;
        w.x = if w.x.is_finite() { w.x.clamp(0.0, 1.0) } else { 1.0 };
        w.y = if w.y.is_finite() { w.y.clamp(0.0, 1.0) } else { 0.08 };
    }

    pub fn load(app: &AppHandle) -> Self {
        let text = Self::path(app).and_then(|p| std::fs::read_to_string(p).ok());
        // Vraie première installation seulement : ni config, ni profil WebView2
        // (où le mod garde ses réglages). Appelé avant la création de la
        // fenêtre, donc le profil n'existe pas encore chez un nouveau venu.
        // Sans ce second test, une config illisible coupait tous les plugins
        // de quelqu'un qui utilisait déjà Plume (vu en 0.3.12).
        let profile = app.path().app_local_data_dir().map(|d| d.join("EBWebView").exists()).unwrap_or(true);
        let fresh = text.is_none() && !profile;
        let mut cfg: Self = text.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();        cfg.sanitize();
        let mut dirty = false;
        if fresh {
            // Première installation : Plume démarre sans aucun plugin actif.
            cfg.plugins_cleared = false;
            dirty = true;
        }
        if !cfg.plumecord_migrated {
            if cfg.client_mod == ClientMod::Nightcord {
                cfg.client_mod = ClientMod::Plumecord;
            }
            cfg.plumecord_migrated = true;
            dirty = true;
        }
        if !cfg.listed_by_default {
            cfg.community.listed = true;
            cfg.listed_by_default = true;
            dirty = true;
        }
        if cfg.community.id.len() != 32 {
            cfg.community.id = random_id();
            dirty = true;
        }
        // Enregistré tout de suite : sinon un nouvel identifiant à chaque
        // lancement, et la même personne comptée plusieurs fois.
        if dirty {
            cfg.save(app);
        }
        cfg
    }

    pub fn save(&self, app: &AppHandle) {
        if let Some(p) = Self::path(app) {
            let _ = std::fs::create_dir_all(p.parent().unwrap());
            let _ = std::fs::write(p, serde_json::to_string_pretty(self).unwrap());
        }
    }
}

/// 32 caractères hexadécimaux, sans lien avec la machine ni le compte.
fn random_id() -> String {
    use sha2::{Digest, Sha256};
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
    let local = 0u8;
    let seed = format!("{now}-{}-{:p}-{:?}", std::process::id(), &local, std::thread::current().id());
    Sha256::digest(seed.as_bytes())[..16].iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ancienne_config_basculee_vers_plumecord() {
        // Config d'avant la 0.3.12 : pas de champ plumecordMigrated.
        let old: Config = serde_json::from_str(r#"{"mod":"nightcord"}"#).unwrap();
        assert!(!old.plumecord_migrated);
        assert!(old.client_mod == ClientMod::Nightcord);
        // Nouvelle installation : Plumecord, déjà migrée.
        let fresh = Config::default();
        assert!(fresh.plumecord_migrated && fresh.client_mod == ClientMod::Plumecord);
        // Liste Communauté : activée par défaut, et les configs d'avant la 0.3.13
        // (sans listedByDefault) seront basculées une fois au chargement.
        assert!(fresh.community.listed && fresh.listed_by_default);
        assert!(!old.listed_by_default);
    }
}
