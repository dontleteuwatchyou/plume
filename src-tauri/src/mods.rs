//! Mods clients (forks de Vencord), chargés comme de vraies extensions
//! WebView2 : exactement ce qu'ils font dans Chrome (styles, retrait de la CSP
//! de Discord pour leurs plugins réseau…), sans adaptation de notre part.
//!
//! On démarre toujours sur la version locale la plus récente ; la mise à jour
//! se télécharge en fond dans un nouveau dossier et sert au lancement suivant.
//! Sans version locale (premier lancement), on télécharge tout de suite.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

/// Nightcord Web+ (github.com/dontleteuwatchyou/nightcord-web-plus) : la version
/// web recompilée chaque jour avec les plugins que l'officielle retire et les
/// réglages des plugins qui ne sautent plus.
const PLUME_NIGHTCORD: &str = "https://github.com/dontleteuwatchyou/nightcord-web-plus/releases/latest/download/nightcord-web-plus.zip";
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

/// Télécharge l'extension dans un nouveau dossier `v<horodatage>`. Chaque
/// version a son propre dossier : on ne remplace jamais sur place un dossier
/// que WebView2 a enregistré (il le marque alors cassé et refuse de le
/// réinstaller).
fn download(m: ClientMod, root: &Path) -> Result<PathBuf, String> {
    let zip = match m {
        // Repli sur l'extension officielle si la nôtre n'est pas publiée.
        ClientMod::Nightcord => get_bytes(PLUME_NIGHTCORD).or_else(|e| {
            eprintln!("[plume] nightcord-web-plus : {e}, repli sur l'officielle");
            get_bytes(&extension_url(m)?)
        })?,
        _ => get_bytes(&extension_url(m)?)?,
    };
    let now = SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    // Dossier temporaire propre à ce téléchargement.
    let tmp = root.join(format!("tmp-{}", now.as_nanos()));
    zip::ZipArchive::new(std::io::Cursor::new(zip))
        .and_then(|mut a| a.extract(&tmp))
        .map_err(|e| e.to_string())?;
    // Garde-fou : ne jamais installer n'importe quoi à la place du mod.
    if !tmp.join("manifest.json").is_file() {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err("archive inattendue".into());
    }
    let dest = root.join(format!("v{}", now.as_secs()));
    std::fs::rename(&tmp, &dest).map_err(|e| e.to_string())?;
    Ok(dest)
}

/// Versions présentes, de la plus récente à la plus ancienne.
fn versions(root: &Path) -> Vec<(u64, PathBuf)> {
    let mut v: Vec<_> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().strip_prefix('v')?.parse::<u64>().ok()?;
            e.path().join("manifest.json").is_file().then(|| (n, e.path()))
        })
        .collect();
    v.sort_by(|a, b| b.0.cmp(&a.0));
    v
}

/// Dossier de l'extension à installer : la version la plus récente. Les
/// anciennes (et les dossiers d'avant ce schéma) sont supprimées au passage ;
/// si WebView2 les tient encore, on réessaiera au prochain lancement.
pub fn prepare(app: &AppHandle, m: ClientMod) -> Option<PathBuf> {
    if m == ClientMod::None {
        return None;
    }
    let root = root(app, m)?;
    let _ = std::fs::create_dir_all(&root);
    let current = match versions(&root).into_iter().next() {
        Some((_, path)) => path,
        None => match download(m, &root) {
            Ok(path) => path,
            Err(e) => {
                eprintln!("[plume] {} : {e}", m.id());
                return None;
            }
        },
    };
    for entry in std::fs::read_dir(&root).into_iter().flatten().flatten() {
        let path = entry.path();
        // Un `tmp-*` récent est un téléchargement en cours : on n'y touche pas.
        let busy = entry.file_name().to_string_lossy().starts_with("tmp-")
            && entry.metadata().and_then(|m| m.modified()).map(|t| t.elapsed().unwrap_or_default() < Duration::from_secs(3600)).unwrap_or(false);
        if path != current && path.is_dir() && !busy {
            let _ = std::fs::remove_dir_all(path);
        }
    }
    Some(current)
}

/// Identifiant que Chromium donne à une extension non empaquetée : SHA-256 du
/// chemin (UTF-16, tel quel), 16 premiers octets écrits avec les lettres a à p.
pub fn extension_id(path: &Path) -> String {
    use sha2::{Digest, Sha256};
    let bytes: Vec<u8> = path.as_os_str().to_string_lossy().encode_utf16().flat_map(u16::to_le_bytes).collect();
    Sha256::digest(&bytes)[..16]
        .iter()
        .flat_map(|b| [b >> 4, b & 0xf])
        .map(|n| (b'a' + n) as char)
        .collect()
}

pub fn refresh_in_background(app: AppHandle, m: ClientMod) {
    if m == ClientMod::None {
        return;
    }
    std::thread::spawn(move || {
        let Some(root) = root(&app, m) else { return };
        // Sans version locale, c'est `prepare` qui télécharge : pas en double.
        let Some((stamp, _)) = versions(&root).into_iter().next() else { return };
        let now = SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
        if now.saturating_sub(stamp) >= MAX_AGE.as_secs() {
            if let Err(e) = download(m, &root) {
                eprintln!("[plume] mise à jour de {} : {e}", m.id());
            }
        }
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn extension_id_comme_chromium() {
        // Identifiant relevé dans le profil WebView2 pour ce dossier.
        let path = std::path::Path::new(r"C:\Users\Privacy\AppData\Local\app.plume.desktop\mods\nightcord\ext");
        assert_eq!(super::extension_id(path), "kppndccigfofjnjgebokhkemgbbhjfkj");
    }
}
