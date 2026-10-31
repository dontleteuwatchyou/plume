//! Sauvegarde des réglages du mod (plugins activés et leurs options), pour
//! qu'une mise à jour ou un bug qui les désactive tous ne fasse rien perdre.
//!
//! inject/backup.js lit `EquicordSettings` (localStorage de discord.com, commun
//! à Plumecord, Nightcord et Equicord) au tout début de chaque chargement, avant
//! le mod, et l'envoie ici. On garde les 10 dernières versions différentes dans
//! `plugin-backups/`. Si d'un coup beaucoup moins de plugins sont actifs que
//! dans la meilleure sauvegarde, la page propose de restaurer : elle réécrit
//! la sauvegarde dans localStorage et recharge Discord.

use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, Manager};

const KEEP: usize = 10;

#[derive(Serialize)]
pub struct BackupInfo {
    /// Plugins actifs maintenant.
    current: usize,
    /// La sauvegarde avec le plus de plugins actifs, parmi les 10 gardées.
    best: usize,
    #[serde(rename = "bestDate")]
    best_date: u64,
}

fn dir(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_local_data_dir().ok().map(|d| d.join("plugin-backups"))
}

/// Nombre de plugins activés dans un `EquicordSettings`.
fn enabled(data: &str) -> usize {
    serde_json::from_str::<serde_json::Value>(data)
        .ok()
        .and_then(|v| v.get("plugins").and_then(|p| p.as_object()).map(|p| p.values().filter(|s| s.get("enabled").and_then(|e| e.as_bool()) == Some(true)).count()))
        .unwrap_or(0)
}

/// Sauvegardes, de la plus récente à la plus ancienne : (horodatage, chemin).
fn list(dir: &PathBuf) -> Vec<(u64, PathBuf)> {
    let mut v: Vec<_> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| Some((e.file_name().to_string_lossy().strip_suffix(".json")?.parse::<u64>().ok()?, e.path())))
        .collect();
    v.sort_by(|a, b| b.0.cmp(&a.0));
    v
}

fn best(dir: &PathBuf) -> Option<(usize, u64, PathBuf)> {
    list(dir)
        .into_iter()
        .filter_map(|(t, p)| Some((enabled(&std::fs::read_to_string(&p).ok()?), t, p)))
        .max_by_key(|(n, t, _)| (*n, *t))
}

#[tauri::command]
pub fn plugins_backup_save(app: AppHandle, data: String) -> Option<BackupInfo> {
    let current = enabled(&data);
    let dir = dir(&app)?;
    let _ = std::fs::create_dir_all(&dir);
    let all = list(&dir);
    // Rien de neuf, ou réglages vides (mod pas encore lancé) : pas de copie.
    let same = all.first().and_then(|(_, p)| std::fs::read_to_string(p).ok()).as_deref() == Some(data.as_str());
    if !same && current > 0 {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
        let _ = std::fs::write(dir.join(format!("{now}.json")), &data);
        // Les plus anciennes partent, sauf la meilleure, qu'on garde toujours.
        let keep = best(&dir).map(|b| b.2);
        for (_, p) in list(&dir).into_iter().skip(KEEP) {
            if Some(&p) != keep.as_ref() {
                let _ = std::fs::remove_file(p);
            }
        }
    }
    let (best, best_date, _) = best(&dir)?;
    Some(BackupInfo { current, best, best_date })
}

/// La meilleure sauvegarde (le plus de plugins actifs), pour la restaurer.
#[tauri::command]
pub fn plugins_backup_restore(app: AppHandle) -> Option<String> {
    dir(&app).and_then(|d| best(&d)).and_then(|(_, _, p)| std::fs::read_to_string(p).ok())
}

#[cfg(test)]
mod tests {
    #[test]
    fn compte_les_plugins_actifs() {
        let s = r#"{"plugins":{"A":{"enabled":true},"B":{"enabled":false},"C":{"enabled":true,"x":1},"D":{}}}"#;
        assert_eq!(super::enabled(s), 2);
        assert_eq!(super::enabled("pas du json"), 0);
    }
}
