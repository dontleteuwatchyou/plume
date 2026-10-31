//! Compteur d'utilisateurs de Plume, servi par un Cloudflare Worker
//! (dossier `community/` du dépôt).
//!
//! Chaque Plume envoie un identifiant tiré au hasard (config.rs) au démarrage
//! puis toutes les 6 h : c'est ce qui est compté. Le profil Discord (identifiant,
//! pseudo, avatar) ne part que si la personne a coché « Apparaître dans la
//! liste », et il est effacé du serveur dès qu'elle décoche.
//!
//! Badge Plume (inject/badge.js) : avec le réglage `badge` (activé par défaut),
//! l'identifiant Discord du compte part aussi, et le serveur rend la liste des
//! comptes qui ont le badge. Décoché, l'identifiant est effacé du serveur.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::AppHandle;

/// Adresse du Worker une fois déployé (voir community/README.md).
/// Vide : la section Communauté indique qu'il n'est pas encore configuré.
pub const URL: &str = "";

#[derive(Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub username: String,
    pub global_name: Option<String>,
    pub avatar: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct Stats {
    /// Plume lancés au moins une fois ces 7 derniers jours.
    pub active: u64,
    /// Toutes les installations jamais vues.
    pub total: u64,
    /// Profils de ceux qui ont choisi d'apparaître (actifs ces 7 jours).
    pub users: Vec<Profile>,
    /// Identifiants Discord qui portent le badge Plume.
    #[serde(default)]
    pub badges: Vec<String>,
}

fn ping(body: &Value) -> Result<Stats, String> {
    if URL.is_empty() {
        return Err("Le serveur Communauté n'est pas encore configuré.".into());
    }
    let bytes = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .build()
        .new_agent()
        .post(format!("{}/ping", URL.trim_end_matches('/')))
        .header("Content-Type", "application/json")
        .header("User-Agent", "plume")
        .send(body.to_string())
        .map_err(|e| format!("serveur injoignable : {e}"))?
        .body_mut()
        .with_config()
        .limit(1024 * 1024)
        .read_to_vec()
        .map_err(|e| e.to_string())?;
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}

fn version(app: &AppHandle) -> String {
    app.package_info().version.to_string()
}

/// Appelé par la section Communauté et par le badge à chaque chargement de
/// Discord : envoie les choix de la personne (son profil si elle veut
/// apparaître, son identifiant Discord si elle garde le badge), rend les
/// chiffres, la liste et les badges.
#[tauri::command]
pub async fn community_sync(app: AppHandle, profile: Option<Profile>, discord: Option<String>) -> Result<Stats, String> {
    let c = crate::settings::current(&app).community;
    let profile = if c.listed { profile } else { None };
    let discord = if c.badge { discord } else { None };
    let body = json!({ "id": c.id, "version": version(&app), "listed": c.listed, "profile": profile, "badge": c.badge, "discord": discord });
    tauri::async_runtime::spawn_blocking(move || ping(&body)).await.map_err(|e| e.to_string())?
}

/// Présence en fond : une minute après le lancement, puis toutes les 6 h.
/// `listed: null` = « ne touche pas au profil déjà enregistré » ; `false` le retire.
pub fn spawn(app: AppHandle) {
    if URL.is_empty() {
        return;
    }
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(60));
        loop {
            let c = crate::settings::current(&app).community;
            let listed = if c.listed { Value::Null } else { Value::Bool(false) };
            let badge = if c.badge { Value::Null } else { Value::Bool(false) };
            let body = json!({ "id": c.id, "version": version(&app), "listed": listed, "badge": badge });
            if let Err(e) = ping(&body) {
                eprintln!("[plume] communauté : {e}");
            }
            std::thread::sleep(Duration::from_secs(6 * 3600));
        }
    });
}
