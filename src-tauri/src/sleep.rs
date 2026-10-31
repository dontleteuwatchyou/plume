//! Veille : quand Plume n'est plus au premier plan depuis quelques minutes (ou
//! seulement quand il est dans la zone de notification, au choix) et que
//! Discord n'a rien en cours (appel, stream, vidéo : inject/sleep.js), on
//! décharge complètement la page Discord pour l'écran Plume. Il ne reste que
//! le moteur, ~50-80 Mo au lieu de ~500. Au retour sur la fenêtre, Discord se
//! recharge là où on l'avait laissé. Contrepartie : pas de notifications
//! pendant la veille.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use tauri::{AppHandle, Manager, WebviewWindow};

#[derive(Default)]
pub struct Sleep {
    /// Appel, stream ou média en cours, d'après la page.
    busy: AtomicBool,
    /// Page Discord déchargée.
    asleep: AtomicBool,
    /// Où revenir au réveil.
    url: Mutex<Option<tauri::Url>>,
    /// Chaque perte de focus ou réveil invalide les minuteries d'avant.
    generation: AtomicU64,
}

#[tauri::command]
pub fn sleep_busy(state: tauri::State<Sleep>, busy: bool) {
    state.busy.store(busy, Ordering::SeqCst);
}

/// Plume vient de passer en arrière-plan (perte du focus ou zone de
/// notification). Lance le compte à rebours si le réglage le permet.
pub fn schedule(app: &AppHandle) {
    let state = app.state::<Sleep>();
    let gen = state.generation.fetch_add(1, Ordering::SeqCst) + 1;
    let cfg = crate::settings::current(app).sleep;
    // Lu sur la fenêtre plutôt que passé par l'appelant : la perte de focus
    // arrive aussi quand on la cache, et ne doit pas annuler ce compte à rebours.
    let hidden = app.get_webview_window(crate::MAIN).and_then(|w| w.is_visible().ok()) == Some(false);
    if !cfg.enabled || (cfg.tray_only && !hidden) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(u64::from(cfg.minutes) * 60));
        let state = app.state::<Sleep>();
        // Occupé : on réessaie toutes les 30 s tant que rien n'a changé.
        loop {
            if state.generation.load(Ordering::SeqCst) != gen {
                return;
            }
            if !state.busy.load(Ordering::SeqCst) {
                break;
            }
            std::thread::sleep(Duration::from_secs(30));
        }
        let _ = app.clone().run_on_main_thread(move || {
            let state = app.state::<Sleep>();
            if state.generation.load(Ordering::SeqCst) == gen {
                fall_asleep(&app);
            }
        });
    });
}

fn fall_asleep(app: &AppHandle) {
    let Some(w) = app.get_webview_window(crate::MAIN) else { return };
    let state = app.state::<Sleep>();
    let Ok(url) = w.url() else { return };
    if !crate::is_discord(&url) || state.asleep.swap(true, Ordering::SeqCst) {
        return;
    }
    *state.url.lock().unwrap() = Some(url);
    state.busy.store(false, Ordering::SeqCst);
    if let Ok(splash) = crate::SPLASH_URL.parse() {
        let _ = w.navigate(splash);
    }
    show_status(&w);
    crate::native::set_memory_low(&w, true);
}

/// Retour sur Plume : on recharge Discord là où on l'avait laissé.
pub fn wake(app: &AppHandle) {
    let state = app.state::<Sleep>();
    state.generation.fetch_add(1, Ordering::SeqCst);
    if !state.asleep.swap(false, Ordering::SeqCst) {
        return;
    }
    let Some(w) = app.get_webview_window(crate::MAIN) else { return };
    let url = state.url.lock().unwrap().take().or_else(|| crate::DISCORD_URL.parse().ok());
    if let Some(url) = url {
        let _ = w.navigate(url);
    }
}

/// L'écran Plume met un instant à se charger avant qu'on puisse y écrire.
fn show_status(w: &WebviewWindow) {
    let w = w.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(400));
        crate::splash_status(&w, "En veille — Discord se recharge quand tu reviens");
    });
}
