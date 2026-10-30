//! Ce que l'enregistreur d'appels de Nightcord (AutoCallRecorder) demande à la
//! partie native d'Electron : choisir un dossier et écrire le fichier. Sa page
//! passe par `inject/recorder.js`.

use std::path::{Path, PathBuf};

use tauri::ipc::{InvokeBody, Request};
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;

#[tauri::command]
pub async fn recorder_pick_folder(app: AppHandle) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || app.dialog().file().set_title("Dossier des enregistrements").blocking_pick_folder())
        .await
        .ok()
        .flatten()
        .and_then(|p| p.into_path().ok())
        .map(|p| p.to_string_lossy().into_owned())
}

/// Corps = le fichier ; en-têtes `filename`, `folder` (vide = Téléchargements)
/// et `ask` (« Enregistrer sous »). Vrai si le fichier est écrit.
#[tauri::command]
pub async fn recorder_save(app: AppHandle, request: Request<'_>) -> Result<bool, String> {
    let InvokeBody::Raw(data) = request.body() else { return Err("corps attendu en binaire".into()) };
    let header = |name: &str| {
        request
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(|v| percent_decode(v))
            .unwrap_or_default()
    };
    // Le nom vient de la page : jamais de chemin dedans.
    let filename: String = header("filename").chars().filter(|c| !r#"<>:"/\|?*"#.contains(*c) && !c.is_control()).collect();
    let filename = if filename.trim().is_empty() { "enregistrement.webm".to_string() } else { filename };

    let dest = if header("ask") == "1" {
        let dialog = app.dialog().file().set_title("Enregistrer l'appel").set_file_name(&filename);
        let picked = tauri::async_runtime::spawn_blocking(move || dialog.blocking_save_file()).await.ok().flatten();
        match picked.and_then(|p| p.into_path().ok()) {
            Some(p) => p,
            None => return Ok(false),
        }
    } else {
        let folder = header("folder");
        let folder = if folder.trim().is_empty() {
            app.path().download_dir().map_err(|e| e.to_string())?
        } else {
            PathBuf::from(folder.trim())
        };
        std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        unique(&folder, &filename)
    };
    std::fs::write(&dest, data).map_err(|e| e.to_string())?;
    Ok(true)
}

/// `nom.ext`, sinon `nom (2).ext`, `nom (3).ext`… comme le plugin d'origine.
pub(crate) fn unique(folder: &Path, filename: &str) -> PathBuf {
    let path = folder.join(filename);
    let (stem, ext) = match filename.rsplit_once('.') {
        Some((s, e)) => (s, format!(".{e}")),
        None => (filename, String::new()),
    };
    let mut candidate = path;
    let mut n = 1;
    while candidate.exists() {
        n += 1;
        candidate = folder.join(format!("{stem} ({n}){ext}"));
    }
    candidate
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

// ---- Enregistrement au fil de l'eau ------------------------------------------
// Comme sous Electron : les morceaux (toutes les 5 s) vont sur le disque au lieu
// de s'accumuler en mémoire — une heure d'appel filmé, c'est plus d'1 Go.

static STREAM: std::sync::Mutex<Option<(PathBuf, std::fs::File)>> = std::sync::Mutex::new(None);

fn clean_name(name: &str) -> String {
    let name: String = name.chars().filter(|c| !r#"<>:"/\|?*"#.contains(*c) && !c.is_control()).collect();
    if name.trim().is_empty() { "enregistrement.webm".into() } else { name }
}

#[tauri::command]
pub fn recorder_stream_init(app: AppHandle, filename: String) -> Result<bool, String> {
    let dir = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("enregistrements");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(clean_name(&filename));
    let file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
    *STREAM.lock().unwrap() = Some((path, file));
    Ok(true)
}

#[tauri::command]
pub fn recorder_stream_append(request: Request<'_>) -> Result<bool, String> {
    use std::io::Write;
    let InvokeBody::Raw(data) = request.body() else { return Err("corps attendu en binaire".into()) };
    match STREAM.lock().unwrap().as_mut() {
        Some((_, file)) => file.write_all(data).map(|_| true).map_err(|e| e.to_string()),
        None => Ok(false),
    }
}

/// Ferme le fichier, y inscrit la durée (WebM) et le range dans le dossier choisi.
#[tauri::command]
pub async fn recorder_stream_finalize(app: AppHandle, folder: String, filename: String, duration_ms: f64) -> Result<bool, String> {
    let Some((path, file)) = STREAM.lock().unwrap().take() else { return Ok(false) };
    drop(file);
    if duration_ms > 0.0 && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("webm") || e.eq_ignore_ascii_case("mkv")) {
        if let Err(e) = webm_duration(&path, duration_ms) {
            eprintln!("[plume] durée de l'enregistrement : {e}");
        }
    }
    let folder = if folder.trim().is_empty() { app.path().download_dir().map_err(|e| e.to_string())? } else { PathBuf::from(folder.trim()) };
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let dest = unique(&folder, &clean_name(&filename));
    if std::fs::rename(&path, &dest).is_err() {
        std::fs::copy(&path, &dest).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(&path);
    }
    Ok(true)
}

/// Taille d'un élément EBML : (valeur, nombre d'octets).
fn read_vint(b: &[u8]) -> Option<(u64, usize)> {
    let first = *b.first()?;
    let len = first.leading_zeros() as usize + 1;
    if len > 8 || b.len() < len {
        return None;
    }
    let mut v = (first as u64) & (0xff >> len);
    for &x in &b[1..len] {
        v = (v << 8) | x as u64;
    }
    Some((v, len))
}

/// MediaRecorder n'écrit pas la durée : sans elle, pas de barre de lecture.
/// On ajoute l'élément Duration (ms, flottant) à la fin de « Info », dont la
/// taille est connue, en réécrivant seulement l'en-tête du fichier.
fn webm_duration(path: &Path, ms: f64) -> Result<(), String> {
    use std::io::{Read, Seek, SeekFrom, Write};
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut head = vec![0u8; 64 * 1024];
    let n = file.read(&mut head).map_err(|e| e.to_string())?;
    head.truncate(n);
    const INFO: [u8; 4] = [0x15, 0x49, 0xA9, 0x66];
    let at = head.windows(4).position(|w| w == INFO).ok_or("en-tête Info introuvable")?;
    let (size, len) = read_vint(&head[at + 4..]).ok_or("taille Info illisible")?;
    let body = at + 4 + len;
    let end = body + size as usize;
    if end > head.len() || head[body..end].windows(2).any(|w| w == [0x44, 0x89]) {
        return Ok(()); // déjà une durée, ou en-tête inattendu : on ne touche à rien
    }
    let mut duration = vec![0x44, 0x89, 0x88];
    duration.extend_from_slice(&ms.to_be_bytes());
    let new_size = size + duration.len() as u64;
    // Même nombre d'octets pour la taille, pour ne rien décaler d'autre.
    if new_size >= (1u64 << (7 * len)) - 1 {
        return Err("Info trop grande".into());
    }
    let mut size_bytes = vec![0u8; len];
    for (i, b) in size_bytes.iter_mut().enumerate() {
        *b = (new_size >> (8 * (len - 1 - i))) as u8;
    }
    size_bytes[0] |= 0x80 >> (len - 1);

    let tmp = path.with_extension("tmp");
    let mut out = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
    out.write_all(&head[..at + 4]).map_err(|e| e.to_string())?;
    out.write_all(&size_bytes).map_err(|e| e.to_string())?;
    out.write_all(&head[body..end]).map_err(|e| e.to_string())?;
    out.write_all(&duration).map_err(|e| e.to_string())?;
    file.seek(SeekFrom::Start(end as u64)).map_err(|e| e.to_string())?;
    std::io::copy(&mut file, &mut out).map_err(|e| e.to_string())?;
    drop((file, out));
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn duree_ajoutee_a_info() {
        // EBML minimal : en-tête, Segment (taille inconnue), Info { TimecodeScale }, Cluster.
        let mut f: Vec<u8> = vec![0x1A, 0x45, 0xDF, 0xA3, 0x80];
        f.extend([0x18, 0x53, 0x80, 0x67, 0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        f.extend([0x15, 0x49, 0xA9, 0x66, 0x87, 0x2A, 0xD7, 0xB1, 0x83, 0x0F, 0x42, 0x40]);
        f.extend([0x1F, 0x43, 0xB6, 0x75, 0x81, 0x00]);
        let path = std::env::temp_dir().join("plume-test-duree.webm");
        std::fs::write(&path, &f).unwrap();
        super::webm_duration(&path, 1234.0).unwrap();
        let out = std::fs::read(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        let at = out.windows(4).position(|w| w == [0x15, 0x49, 0xA9, 0x66]).unwrap();
        assert_eq!(out[at + 4], 0x80 | 18, "taille d'Info mise à jour");
        let d = out.windows(3).position(|w| w == [0x44, 0x89, 0x88]).unwrap();
        assert_eq!(f64::from_be_bytes(out[d + 3..d + 11].try_into().unwrap()), 1234.0);
        assert!(out.ends_with(&[0x1F, 0x43, 0xB6, 0x75, 0x81, 0x00]), "suite intacte");
    }
}
