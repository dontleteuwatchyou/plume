//! « Enregistrer l'image » du clic droit, que Discord ne propose que dans son
//! appli (inject/desktop.js ajoute l'entrée). Téléchargé ici plutôt que dans
//! la page : pas de souci de CORS, et la page ne peut viser que les serveurs
//! de fichiers de Discord.

use std::time::Duration;

use tauri::{AppHandle, Manager, Url};

fn allowed(url: &Url) -> bool {
    let host = url.host_str().unwrap_or("");
    url.scheme() == "https" && ["discordapp.com", "discordapp.net", "discord.com"].iter().any(|d| host == *d || host.ends_with(&format!(".{d}")))
}

#[tauri::command]
pub async fn save_url(app: AppHandle, url: String) -> Result<(), String> {
    let url: Url = url.parse().map_err(|_| "adresse invalide")?;
    if !allowed(&url) {
        return Err("seuls les fichiers de Discord".into());
    }
    let name: String = url
        .path_segments()
        .and_then(|mut s| s.next_back())
        .unwrap_or("image")
        .chars()
        .filter(|c| !r#"<>:"/\|?*"#.contains(*c) && !c.is_control())
        .collect();
    let name = if name.is_empty() { "image".to_string() } else { name };
    let data = tauri::async_runtime::spawn_blocking({
        let url = url.to_string();
        move || {
            ureq::Agent::config_builder()
                .timeout_global(Some(Duration::from_secs(120)))
                .build()
                .new_agent()
                .get(&url)
                .call()
                .map_err(|e| e.to_string())?
                .body_mut()
                .with_config()
                .limit(512 * 1024 * 1024)
                .read_to_vec()
                .map_err(|e| e.to_string())
        }
    })
    .await
    .map_err(|e| e.to_string())??;
    let dir = app.path().download_dir().map_err(|e| e.to_string())?;
    let dest = crate::recorder::unique(&dir, &name);
    std::fs::write(&dest, data).map_err(|e| e.to_string())?;
    let file = dest.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if let Some(w) = app.get_webview_window(crate::MAIN) {
        let text = serde_json::to_string(&format!("{file} enregistré dans Téléchargements")).unwrap();
        let _ = w.eval(format!("window.__plumeToast && window.__plumeToast({text}, true)"));
    }
    crate::notify(app, "Image enregistrée".into(), format!("{file} est dans Téléchargements"));
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn serveurs_de_discord_seulement() {
        let ok = |u: &str| super::allowed(&u.parse().unwrap());
        assert!(ok("https://cdn.discordapp.com/attachments/1/2/a.png"));
        assert!(ok("https://media.discordapp.net/attachments/1/2/a.png?width=10"));
        assert!(!ok("https://evil.com/a.png"));
        assert!(!ok("https://discordapp.com.evil.com/a.png"));
        assert!(!ok("http://cdn.discordapp.com/a.png"));
    }
}
