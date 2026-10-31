//! Application des réglages : à chaud pour l'audio, en rechargeant Discord pour
//! le mod, en redémarrant Plume pour l'économie de mémoire (les arguments du
//! moteur WebView2 sont fixés à son lancement).
//! Le panneau lui-même vit dans la page Discord (inject/panel.js).

use std::sync::Mutex;

use tauri::menu::CheckMenuItem;
use tauri::{AppHandle, Manager, Wry};

use crate::config::{Audio, Config};
use crate::mods::{self, ClientMod};

/// Cases à cocher du menu de la zone de notification, gardées en phase.
pub struct TrayItems {
    pub stereo: CheckMenuItem<Wry>,
    pub mods: Vec<(ClientMod, CheckMenuItem<Wry>)>,
}

pub fn current(app: &AppHandle) -> Config {
    app.state::<Mutex<Config>>().lock().unwrap().clone()
}

/// Enregistre et applique une config complète.
pub fn apply(app: &AppHandle, mut next: Config) {
    next.sanitize();
    let previous = {
        let state = app.state::<Mutex<Config>>();
        let mut cfg = state.lock().unwrap();
        std::mem::replace(&mut *cfg, next.clone())
    };
    next.save(app);
    if let Some(items) = app.try_state::<TrayItems>() {
        let _ = items.stereo.set_checked(next.audio.stereo);
        for (m, item) in &items.mods {
            let _ = item.set_checked(next.client_mod == *m);
        }
    }
    if next.autostart != previous.autostart {
        crate::autostart::apply(next.autostart);
    }
    if next.memory != previous.memory {
        app.restart();
    }
    if next.client_mod != previous.client_mod {
        mods::refresh_in_background(app.clone(), next.client_mod);
        crate::reload_discord(app);
    } else if let Some(w) = app.get_webview_window(crate::MAIN) {
        let audio = serde_json::to_string(&next.audio).unwrap();
        let theme = serde_json::to_string(&next.theme).unwrap();
        let _ = w.eval(format!(
            "window.__plumeApply && window.__plumeApply({audio}); window.__plumeApplyTheme && window.__plumeApplyTheme({theme})"
        ));
    }
}

/// Affiche Discord et ouvre le panneau Paramètres Plume dedans.
pub fn open(app: &AppHandle) {
    crate::show_main(app);
    if let Some(w) = app.get_webview_window(crate::MAIN) {
        let _ = w.eval("window.__plumeOpenPanel && window.__plumeOpenPanel()");
    }
}

#[tauri::command]
pub fn get_config(app: AppHandle) -> Config {
    current(&app)
}

#[tauri::command]
pub fn set_config(app: AppHandle, mut config: Config) {
    // Ce que la page ne gère pas depuis le panneau : la position du compteur
    // (déplacé pendant que le panneau avait une copie plus ancienne),
    // l'identifiant Communauté et le nettoyage des plugins déjà fait.
    let cur = current(&app);
    config.ram_widget.x = cur.ram_widget.x;
    config.ram_widget.y = cur.ram_widget.y;
    config.community.id = cur.community.id;
    config.plugins_cleared |= cur.plugins_cleared;
    config.listed_by_default |= cur.listed_by_default;
    apply(&app, config);
}

/// Nouvelle position du compteur de RAM, après un glisser.
#[tauri::command]
pub fn set_ram_position(app: AppHandle, x: f64, y: f64) {
    // Rien à appliquer : on garde et on enregistre, sans repasser l'audio.
    let state = app.state::<Mutex<Config>>();
    let mut cfg = state.lock().unwrap();
    cfg.ram_widget.spot = crate::config::RamSpot::Custom;
    cfg.ram_widget.x = x;
    cfg.ram_widget.y = y;
    cfg.sanitize();
    cfg.save(&app);
}

/// Les plugins activés d'office ont été coupés (première installation).
#[tauri::command]
pub fn plugins_cleared(app: AppHandle) {
    let mut cfg = current(&app);
    cfg.plugins_cleared = true;
    apply(&app, cfg);
}

/// Réglages audio à jour, relus par la page à chaque chargement (les scripts
/// d'initialisation, eux, sont figés à la création de la fenêtre).
#[tauri::command]
pub fn audio_config(app: AppHandle) -> Audio {
    current(&app).audio
}

/// Processeur AudioWorklet de RNNoise, livré à la page seulement quand la
/// suppression de bruit est activée.
#[tauri::command]
pub fn rnnoise_worklet() -> &'static str {
    include_str!("../../vendor/rnnoise/worklet.js")
}

/// Binaire WebAssembly de RNNoise (SIMD), envoyé en octets bruts.
#[tauri::command]
pub fn rnnoise_wasm() -> tauri::ipc::Response {
    tauri::ipc::Response::new(include_bytes!("../../vendor/rnnoise/rnnoise_simd.wasm").to_vec())
}
