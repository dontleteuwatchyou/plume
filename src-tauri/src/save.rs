//! Téléchargements de fichiers Discord faits par Plume, comme l'appli :
//!  - « Enregistrer l'image » du clic droit (inject/desktop.js, `save_url`) ;
//!  - les fichiers joints (zip, exe, pdf…) : Discord web les ouvre dans une
//!    nouvelle fenêtre, que Plume enverrait au navigateur ; main.rs les passe
//!    ici à la place (`is_attachment`).
//! Toujours vers Téléchargements, avec une confirmation dans Discord. Seuls les
//! serveurs de fichiers de Discord sont acceptés.

use std::time::Duration;

use tauri::{AppHandle, Manager, Url};

fn allowed(url: &Url) -> bool {
    let host = url.host_str().unwrap_or("");
    url.scheme() == "https" && ["discordapp.com", "discordapp.net", "discord.com"].iter().any(|d| host == *d || host.ends_with(&format!(".{d}")))
}

/// Pièce jointe qui n'est ni une image ni une vidéo ni un son : un fichier à
/// télécharger (les médias s'ouvrent toujours dans le navigateur).
pub fn is_attachment(url: &Url) -> bool {
    const MEDIA: [&str; 16] = ["png", "jpg", "jpeg", "gif", "webp", "avif", "bmp", "svg", "mp4", "webm", "mov", "mkv", "mp3", "ogg", "wav", "flac"];
    let ext = url.path().rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    allowed(url) && url.path().starts_with("/attachments/") && !MEDIA.contains(&ext.as_str())
}

fn toast(app: &AppHandle, text: &str, ok: bool) {
    if let Some(w) = app.get_webview_window(crate::MAIN) {
        let text = serde_json::to_string(text).unwrap();
        let _ = w.eval(format!("window.__plumeToast && window.__plumeToast({text}, {ok})"));
    }
}

/// Télécharge vers Téléchargements (écrit au fil de l'eau, sans tout garder en
/// mémoire) et prévient. Renvoie le nom du fichier écrit.
fn save_blocking(app: &AppHandle, url: &Url) -> Result<String, String> {
    if !allowed(url) {
        return Err("seuls les fichiers de Discord".into());
    }
    let name: String = url
        .path_segments()
        .and_then(|mut s| s.next_back())
        .unwrap_or("fichier")
        .chars()
        .filter(|c| !r#"<>:"/\|?*"#.contains(*c) && !c.is_control())
        .collect();
    let name = if name.is_empty() { "fichier".to_string() } else { name };
    let mut response = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30 * 60)))
        .build()
        .new_agent()
        .get(url.as_str())
        .call()
        .map_err(|e| e.to_string())?;
    let dir = app.path().download_dir().map_err(|e| e.to_string())?;
    let dest = crate::recorder::unique(&dir, &name);
    let result = std::fs::File::create(&dest)
        .map_err(|e| e.to_string())
        .and_then(|mut file| std::io::copy(&mut response.body_mut().as_reader(), &mut file).map_err(|e| e.to_string()));
    if let Err(e) = result {
        let _ = std::fs::remove_file(&dest);
        return Err(e);
    }
    let file = dest.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    toast(app, &format!("{file} enregistré dans Téléchargements"), true);
    crate::notify(app.clone(), "Téléchargement terminé".into(), format!("{file} est dans Téléchargements"));
    Ok(file)
}

#[tauri::command]
pub async fn save_url(app: AppHandle, url: String) -> Result<(), String> {
    let url: Url = url.parse().map_err(|_| "adresse invalide")?;
    tauri::async_runtime::spawn_blocking(move || save_blocking(&app, &url).map(|_| ())).await.map_err(|e| e.to_string())?
}

/// Pour les fichiers joints ouverts depuis Discord (fil à part : la fenêtre ne
/// doit pas attendre la fin du téléchargement).
pub fn download_in_background(app: &AppHandle, url: Url) {
    let app = app.clone();
    toast(&app, "Téléchargement en cours…", true);
    std::thread::spawn(move || {
        if let Err(e) = save_blocking(&app, &url) {
            toast(&app, &format!("Échec du téléchargement : {e}"), false);
        }
    });
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

    #[test]
    fn fichiers_joints_a_telecharger() {
        let att = |u: &str| super::is_attachment(&u.parse().unwrap());
        assert!(att("https://cdn.discordapp.com/attachments/1/2/jeu.zip?ex=1"));
        assert!(att("https://cdn.discordapp.com/attachments/1/2/setup.exe"));
        assert!(!att("https://cdn.discordapp.com/attachments/1/2/photo.PNG"));
        assert!(!att("https://cdn.discordapp.com/avatars/1/a.webp"));
        assert!(!att("https://evil.com/attachments/1/2/x.zip"));
    }
}
