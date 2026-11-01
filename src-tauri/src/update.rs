//! Mise à jour de Plume lui-même, depuis les releases GitHub (`latest.json`).
//! Les paquets sont signés : sans la clé privée du projet, rien ne s'installe.
//! On ne met jamais à jour en silence (un appel vocal pourrait couper) : on
//! prévient, et l'utilisateur installe depuis Paramètres Plume → Mises à jour.

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_updater::UpdaterExt;

/// Version disponible connue, gardée côté Plume : la page la redemande à
/// chaque chargement (pastilles, bulle), même après un rechargement de Discord.
#[derive(Default)]
pub struct Pending(pub Mutex<Option<String>>);

#[tauri::command]
pub fn update_pending(pending: tauri::State<Pending>) -> Option<String> {
    pending.0.lock().unwrap().clone()
}

fn remember(app: &AppHandle, version: Option<String>) {
    *app.state::<Pending>().0.lock().unwrap() = version.clone();
    if let (Some(v), Some(w)) = (version, app.get_webview_window(crate::MAIN)) {
        let _ = w.eval(format!("window.__plumeUpdateAvailable && window.__plumeUpdateAvailable({v:?})"));
    }
}

#[derive(Serialize)]
pub struct UpdateInfo {
    current: String,
    available: bool,
    version: Option<String>,
    notes: Option<String>,
    /// Pourquoi la vérification a échoué, en clair ; `None` si elle a abouti.
    error: Option<String>,
}

/// Sous Linux, Plume s'installe par le gestionnaire de paquets (PKGBUILD) :
/// c'est lui qui le met à jour, avec le moteur web qui va avec.
#[cfg(not(windows))]
const PACKAGED: &str = "Plume est installé par ton gestionnaire de paquets : il se met à jour avec lui (paru -Syu, yay -Syu…).";

async fn check(app: &AppHandle) -> Result<Option<tauri_plugin_updater::Update>, String> {
    #[cfg(not(windows))]
    return Err(PACKAGED.into());
    #[allow(unreachable_code)]
    app.updater().map_err(|e| e.to_string())?.check().await.map_err(|e| e.to_string())
}

/// Toujours une réponse (avec la version installée), même si GitHub ne répond
/// pas : la page Mises à jour affiche alors la raison au lieu d'une erreur brute.
#[tauri::command]
pub async fn update_check(app: AppHandle) -> UpdateInfo {
    let current = app.package_info().version.to_string();
    let result = check(&app).await;
    if let Ok(update) = &result {
        remember(&app, update.as_ref().map(|u| u.version.clone()));
    }
    match result {
        Ok(update) => UpdateInfo {
            current,
            available: update.is_some(),
            version: update.as_ref().map(|u| u.version.clone()),
            notes: update.and_then(|u| u.body),
            error: None,
        },
        Err(e) => UpdateInfo {
            current,
            available: false,
            version: None,
            notes: None,
            error: Some(if e.starts_with("Plume est installé") {
                e
            } else if e.contains("valid release JSON") {
                "Aucune version avec mise à jour automatique n'est encore publiée (latest.json absent de la dernière release).".into()
            } else {
                format!("GitHub injoignable : {e}")
            }),
        },
    }
}

/// Télécharge, vérifie la signature, lance l'installateur puis redémarre.
/// Pendant ce temps, la fenêtre affiche l'écran Plume avec la progression.
#[tauri::command]
pub async fn update_install(app: AppHandle) -> Result<(), String> {
    let update = check(&app).await?.ok_or("Plume est déjà à jour")?;
    let w = app.get_webview_window(crate::MAIN);
    if let Some(w) = &w {
        if let Ok(url) = format!("{}?update={}", crate::SPLASH_URL, update.version).parse() {
            let _ = w.navigate(url);
        }
    }
    let (mut done, mut shown) = (0u64, 0u64);
    let progress = w.clone();
    let finished = w.clone();
    let result = update
        .download_and_install(
            move |chunk, total| {
                done += chunk as u64;
                let Some(w) = &progress else { return };
                // Une mise à jour d'écran par pour-cent, pas une par paquet réseau.
                let pct = total.map(|t| done * 100 / t.max(1)).unwrap_or(0);
                if pct != shown || total.is_none() {
                    shown = pct;
                    let script = match total {
                        Some(_) => format!("window.plumeSplash && (plumeSplash.status('Téléchargement de la mise à jour — {pct} %', false), plumeSplash.progress({}))", pct as f64 / 100.0),
                        None => "window.plumeSplash && plumeSplash.progress(null)".into(),
                    };
                    let _ = w.eval(script);
                }
            },
            move || {
                if let Some(w) = &finished {
                    crate::splash_status(w, "Installation");
                    let _ = w.eval("window.plumeSplash && plumeSplash.progress(null)");
                }
            },
        )
        .await;
    if let Err(e) = result {
        // Échec : on rend Discord à l'utilisateur.
        if let Some(w) = &w {
            let _ = w.navigate(crate::DISCORD_URL.parse().unwrap());
        }
        return Err(e.to_string());
    }
    app.restart();
}

/// Vérification au démarrage, une fois Discord chargé.
/// Vérification en fond, sans rien demander : 15 s après le lancement, puis
/// toutes les 6 h. Une version trouvée allume la pastille et affiche la carte
/// « Mise à jour disponible » dans Plume (inject/titlebar.js) ; fenêtre
/// cachée, une notification Windows en plus.
pub fn check_on_startup(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio_sleep(15).await;
        let mut announced: Option<String> = None;
        loop {
            if let Ok(Some(update)) = check(&app).await {
                let hidden = app.get_webview_window(crate::MAIN).map(|w| !w.is_visible().unwrap_or(false)).unwrap_or(true);
                if hidden && announced.as_deref() != Some(update.version.as_str()) {
                    let _ = app
                        .notification()
                        .builder()
                        .title(format!("Plume {} est disponible", update.version))
                        .body("Ouvre Plume et clique sur la carte « Mise à jour disponible ».")
                        .show();
                }
                announced = Some(update.version.clone());
                remember(&app, Some(update.version));
            }
            tokio_sleep(6 * 3600).await;
        }
    });
}

async fn tokio_sleep(secs: u64) {
    let _ = tauri::async_runtime::spawn_blocking(move || std::thread::sleep(std::time::Duration::from_secs(secs))).await;
}
