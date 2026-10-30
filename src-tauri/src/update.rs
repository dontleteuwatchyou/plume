//! Mise à jour de Plume lui-même, depuis les releases GitHub (`latest.json`).
//! Les paquets sont signés : sans la clé privée du projet, rien ne s'installe.
//! On ne met jamais à jour en silence (un appel vocal pourrait couper) : on
//! prévient, et l'utilisateur installe depuis Paramètres Plume → Mises à jour.

use serde::Serialize;
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_updater::UpdaterExt;

#[derive(Serialize)]
pub struct UpdateInfo {
    current: String,
    available: bool,
    version: Option<String>,
    notes: Option<String>,
}

async fn check(app: &AppHandle) -> Result<Option<tauri_plugin_updater::Update>, String> {
    app.updater().map_err(|e| e.to_string())?.check().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_check(app: AppHandle) -> Result<UpdateInfo, String> {
    let update = check(&app).await?;
    Ok(UpdateInfo {
        current: app.package_info().version.to_string(),
        available: update.is_some(),
        version: update.as_ref().map(|u| u.version.clone()),
        notes: update.and_then(|u| u.body),
    })
}

/// Télécharge, vérifie la signature, lance l'installateur puis redémarre.
#[tauri::command]
pub async fn update_install(app: AppHandle) -> Result<(), String> {
    let update = check(&app).await?.ok_or("Plume est déjà à jour")?;
    update.download_and_install(|_, _| {}, || {}).await.map_err(|e| e.to_string())?;
    app.restart();
}

/// Vérification au démarrage, une fois Discord chargé.
pub fn check_on_startup(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio_sleep(30).await;
        let Ok(Some(update)) = check(&app).await else { return };
        let _ = app
            .notification()
            .builder()
            .title(format!("Plume {} est disponible", update.version))
            .body("Paramètres Plume → Mises à jour pour l'installer.")
            .show();
        if let Some(w) = app.get_webview_window(crate::MAIN) {
            let _ = w.eval(format!("window.__plumeUpdateAvailable && window.__plumeUpdateAvailable({:?})", update.version));
        }
    });
}

async fn tokio_sleep(secs: u64) {
    let _ = tauri::async_runtime::spawn_blocking(move || std::thread::sleep(std::time::Duration::from_secs(secs))).await;
}
