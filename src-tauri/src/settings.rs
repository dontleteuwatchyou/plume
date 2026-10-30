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
    if next.memory != previous.memory {
        app.restart();
    }
    if next.client_mod != previous.client_mod {
        mods::refresh_in_background(app.clone(), next.client_mod);
        crate::restart_main(app);
    } else if let Some(w) = app.get_webview_window(crate::MAIN) {
        let audio = serde_json::to_string(&next.audio).unwrap();
        let _ = w.eval(format!("window.__plumeApply && window.__plumeApply({audio})"));
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
pub fn set_config(app: AppHandle, config: Config) {
    apply(&app, config);
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
