//! Fenêtre « Paramètres Plume » (dist/settings.html) et application des
//! réglages : à chaud pour l'audio, en rechargeant Discord pour le mod.

use std::sync::mpsc;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::Value;
use tauri::menu::CheckMenuItem;
use tauri::{AppHandle, Manager, Wry, WebviewUrl, WebviewWindowBuilder};

use crate::config::{Audio, Config};
use crate::mods::{self, ClientMod};

const SETTINGS: &str = "settings";

/// Cases à cocher du menu de la zone de notification, gardées en phase.
pub struct TrayItems {
    pub stereo: CheckMenuItem<Wry>,
    pub mods: Vec<(ClientMod, CheckMenuItem<Wry>)>,
}

/// Réponse attendue de la page Discord pour un `audio_probe` en cours.
#[derive(Default)]
pub struct ProbeSlot(Mutex<Option<mpsc::Sender<Value>>>);

pub fn current(app: &AppHandle) -> Config {
    app.state::<Mutex<Config>>().lock().unwrap().clone()
}

/// Enregistre et applique une config complète.
pub fn apply(app: &AppHandle, mut next: Config) {
    next.sanitize();
    let previous_mod = {
        let state = app.state::<Mutex<Config>>();
        let mut cfg = state.lock().unwrap();
        std::mem::replace(&mut *cfg, next.clone()).client_mod
    };
    next.save(app);
    if let Some(items) = app.try_state::<TrayItems>() {
        let _ = items.stereo.set_checked(next.audio.stereo);
        for (m, item) in &items.mods {
            let _ = item.set_checked(next.client_mod == *m);
        }
    }
    if next.client_mod != previous_mod {
        mods::refresh_in_background(app.clone(), next.client_mod);
        crate::restart_main(app);
    } else if let Some(w) = app.get_webview_window(crate::MAIN) {
        let audio = serde_json::to_string(&next.audio).unwrap();
        let _ = w.eval(format!("window.__plumeApply && window.__plumeApply({audio})"));
    }
}

pub fn open(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(SETTINGS) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    let built = WebviewWindowBuilder::new(app, SETTINGS, WebviewUrl::App("settings.html".into()))
        .title("Paramètres Plume")
        .inner_size(600.0, 780.0)
        .min_inner_size(520.0, 520.0)
        .background_color(tauri::window::Color(0x1a, 0x1a, 0x1e, 0xff))
        // Même environnement WebView2 que la fenêtre Discord, sinon refus.
        .browser_extensions_enabled(true)
        .build();
    if let Err(e) = built {
        eprintln!("[plume] fenêtre Paramètres : {e}");
    }
}

// Asynchrone exprès : créer une fenêtre depuis une commande synchrone bloque
// la boucle d'événements sous Windows.
#[tauri::command]
pub async fn open_settings(app: AppHandle) {
    open(&app);
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

/// Demande à la page Discord l'état de l'audio (`status`) ou un auto-test
/// (`test`), et attend sa réponse.
#[tauri::command]
pub async fn audio_probe(app: AppHandle, kind: String) -> Result<Value, String> {
    let kind = if kind == "test" { "test" } else { "status" };
    let w = app.get_webview_window(crate::MAIN).ok_or("Discord n'est pas ouvert")?;
    let (tx, rx) = mpsc::channel();
    *app.state::<ProbeSlot>().0.lock().unwrap() = Some(tx);
    w.eval(format!("window.__plumeProbe && window.__plumeProbe({kind:?})")).map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || rx.recv_timeout(Duration::from_secs(12)))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|_| "pas de réponse de Discord".into())
}

#[tauri::command]
pub fn audio_report(data: Value, slot: tauri::State<ProbeSlot>) {
    if let Some(tx) = slot.0.lock().unwrap().take() {
        let _ = tx.send(data);
    }
}
