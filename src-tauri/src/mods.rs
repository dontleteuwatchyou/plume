//! Mods clients (forks de Vencord), chargés comme de vraies extensions
//! WebView2 : exactement ce qu'ils font dans Chrome (styles, retrait de la CSP
//! de Discord pour leurs plugins réseau…), sans adaptation de notre part.
//!
//! On démarre toujours sur la copie locale ; la mise à jour se télécharge en
//! fond dans `ext.new` et prend la place de `ext` au lancement suivant. Sans
//! copie locale (premier lancement), on télécharge tout de suite.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

const NIGHTCORD_RELEASE: &str = "https://source.nightcord.st/api/v1/repos/nightcord/nightcord/releases/latest";
const EQUICORD_EXTENSION: &str = "https://github.com/Equicord/Equicord/releases/download/latest/extension-chrome.zip";
const MAX_AGE: Duration = Duration::from_secs(6 * 3600);

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClientMod {
    Nightcord,
    Equicord,
    None,
}

impl ClientMod {
    pub const ALL: [ClientMod; 3] = [ClientMod::Nightcord, ClientMod::Equicord, ClientMod::None];

    pub fn id(self) -> &'static str {
        match self {
            ClientMod::Nightcord => "nightcord",
            ClientMod::Equicord => "equicord",
            ClientMod::None => "none",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ClientMod::Nightcord => "Nightcord",
            ClientMod::Equicord => "Equicord",
            ClientMod::None => "Aucun",
        }
    }
}

fn root(app: &AppHandle, m: ClientMod) -> Option<PathBuf> {
    app.path().app_cache_dir().ok().map(|d| d.join("mods").join(m.id()))
}

fn get_bytes(url: &str) -> Result<Vec<u8>, String> {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(120)))
        .build()
        .new_agent()
        .get(url)
        .header("User-Agent", "plume")
        .call()
        .map_err(|e| e.to_string())?
        .body_mut()
        .with_config()
        .limit(64 * 1024 * 1024)
        .read_to_vec()
        .map_err(|e| e.to_string())
}

fn extension_url(m: ClientMod) -> Result<String, String> {
    match m {
        ClientMod::Equicord => Ok(EQUICORD_EXTENSION.into()),
        ClientMod::Nightcord => {
            #[derive(Deserialize)]
            struct Asset {
                name: String,
                browser_download_url: String,
            }
            #[derive(Deserialize)]
            struct Release {
                assets: Vec<Asset>,
            }
            let release: Release =
                serde_json::from_slice(&get_bytes(NIGHTCORD_RELEASE)?).map_err(|e| e.to_string())?;
            release
                .assets
                .into_iter()
                .find(|a| a.name == "extension-chrome.zip")
                .map(|a| a.browser_download_url)
                .ok_or_else(|| "extension-chrome.zip absente de la release".into())
        }
        ClientMod::None => Err("aucun mod".into()),
    }
}

/// Télécharge l'extension et la décompresse dans `dest` (remplacé s'il existe).
fn download(m: ClientMod, dest: &Path) -> Result<(), String> {
    let zip = get_bytes(&extension_url(m)?)?;
    let tmp = dest.with_extension("tmp");
    let _ = std::fs::remove_dir_all(&tmp);
    zip::ZipArchive::new(std::io::Cursor::new(zip))
        .and_then(|mut a| a.extract(&tmp))
        .map_err(|e| e.to_string())?;
    // Garde-fou : ne jamais remplacer une extension valide par n'importe quoi.
    if !tmp.join("manifest.json").is_file() {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err("archive inattendue".into());
    }
    // Date de téléchargement : les fichiers gardent celles de l'archive.
    std::fs::write(tmp.join(".fetched"), "").map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(dest);
    std::fs::rename(&tmp, dest).map_err(|e| e.to_string())
}

/// Dossier de l'extension à installer, prêt sur le disque.
pub fn prepare(app: &AppHandle, m: ClientMod) -> Option<PathBuf> {
    if m == ClientMod::None {
        return None;
    }
    let root = root(app, m)?;
    let (ext, new) = (root.join("ext"), root.join("ext.new"));
    if new.join("manifest.json").is_file() {
        let _ = std::fs::remove_dir_all(&ext);
        let _ = std::fs::rename(&new, &ext);
    }
    if !ext.join("manifest.json").is_file() {
        if let Err(e) = download(m, &ext) {
            eprintln!("[plume] {} : {e}", m.id());
            return None;
        }
    }
    Some(ext)
}

pub fn refresh_in_background(app: AppHandle, m: ClientMod) {
    if m == ClientMod::None {
        return;
    }
    std::thread::spawn(move || {
        let Some(root) = root(&app, m) else { return };
        let fresh = std::fs::metadata(root.join("ext").join(".fetched"))
            .and_then(|meta| meta.modified())
            .map(|t| SystemTime::now().duration_since(t).unwrap_or_default() < MAX_AGE)
            .unwrap_or(false);
        if !fresh {
            if let Err(e) = download(m, &root.join("ext.new")) {
                eprintln!("[plume] mise à jour de {} : {e}", m.id());
            }
        }
    });
}
