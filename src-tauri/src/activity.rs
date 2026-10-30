//! Détection des jeux (« Joue à … »), comme l'appli Discord : Discord web ne
//! voit pas les programmes lancés. Toutes les 15 s on compare les processus à
//! la liste publique des applications détectables de Discord, puis la page
//! (inject/activity.js) annonce le jeu à Discord.
//!
//! Seuls les noms des exécutables sont lus, et rien ne sort de la machine à
//! part ce que Discord publie déjà lui-même : le jeu en cours.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

const DETECTABLE: &str = "https://discord.com/api/v9/applications/detectable";
const MAX_AGE: Duration = Duration::from_secs(24 * 3600);

#[derive(Deserialize)]
struct Executable {
    name: String,
    os: String,
    #[serde(default)]
    is_launcher: bool,
}

#[derive(Deserialize)]
struct Application {
    id: String,
    name: String,
    #[serde(default)]
    executables: Option<Vec<Executable>>,
}

#[derive(Clone, Serialize)]
struct Game {
    id: String,
    name: String,
    /// Début de la partie, en ms (pour le chronomètre de Discord).
    start: u64,
}

/// Exécutable Windows → jeu. Une entrée `dossier/jeu.exe` se compare à la fin
/// du chemin complet ; un simple `jeu.exe` au nom du fichier.
#[derive(Default)]
struct Catalog {
    by_name: HashMap<String, (String, String)>,
    by_suffix: Vec<(String, (String, String))>,
}

impl Catalog {
    fn build(apps: Vec<Application>) -> Self {
        let mut c = Catalog::default();
        for app in apps {
            for exe in app.executables.into_iter().flatten() {
                if exe.os != "win32" || exe.is_launcher {
                    continue;
                }
                let name = exe.name.to_lowercase().replace('\\', "/");
                let name = name.trim_start_matches('>').to_string();
                let game = (app.id.clone(), app.name.clone());
                if name.contains('/') {
                    c.by_suffix.push((format!("/{name}"), game));
                } else {
                    c.by_name.entry(name).or_insert(game);
                }
            }
        }
        c
    }

    fn find(&self, path: &str) -> Option<&(String, String)> {
        let path = path.to_lowercase().replace('\\', "/");
        if let Some((_, g)) = self.by_suffix.iter().find(|(s, _)| path.ends_with(s.as_str())) {
            return Some(g);
        }
        self.by_name.get(path.rsplit('/').next()?)
    }
}

fn cache_file(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_cache_dir().ok().map(|d| d.join("detectable.json"))
}

/// Liste en cache (rafraîchie une fois par jour), sinon téléchargée.
fn load(app: &AppHandle) -> Option<Catalog> {
    let file = cache_file(app)?;
    let fresh = std::fs::metadata(&file)
        .and_then(|m| m.modified())
        .map(|t| t.elapsed().unwrap_or_default() < MAX_AGE)
        .unwrap_or(false);
    if !fresh {
        match download() {
            Ok(bytes) => {
                let _ = std::fs::create_dir_all(file.parent()?);
                let _ = std::fs::write(&file, &bytes);
            }
            Err(e) => eprintln!("[plume] jeux détectables : {e}"),
        }
    }
    let apps: Vec<Application> = serde_json::from_slice(&std::fs::read(&file).ok()?).ok()?;
    Some(Catalog::build(apps))
}

fn download() -> Result<Vec<u8>, String> {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(120)))
        .build()
        .new_agent()
        .get(DETECTABLE)
        .header("User-Agent", "plume")
        .call()
        .map_err(|e| e.to_string())?
        .body_mut()
        .with_config()
        .limit(64 * 1024 * 1024)
        .read_to_vec()
        .map_err(|e| e.to_string())
}

/// Chemins des exécutables en cours (nom seul si le chemin est protégé).
#[cfg(windows)]
fn processes() -> Vec<(u32, String)> {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};
    use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};

    let mut out = Vec::new();
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return out };
        let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut ok = Process32FirstW(snap, &mut entry).is_ok();
        while ok {
            let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
            let mut path = String::from_utf16_lossy(&entry.szExeFile[..len]);
            if let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, entry.th32ProcessID) {
                let mut buf = [0u16; 1024];
                let mut size = buf.len() as u32;
                if QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut size).is_ok() {
                    path = String::from_utf16_lossy(&buf[..size as usize]);
                }
                let _ = CloseHandle(h);
            }
            out.push((entry.th32ProcessID, path));
            ok = Process32NextW(snap, &mut entry).is_ok();
        }
        let _ = CloseHandle(snap);
    }
    out
}

#[cfg(not(windows))]
fn processes() -> Vec<(u32, String)> {
    Vec::new()
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        let Some(catalog) = load(&app) else { return };
        // Jeu → début de partie, gardé tant que le jeu tourne.
        let mut running: HashMap<String, Game> = HashMap::new();
        loop {
            let mut seen: HashMap<String, Game> = HashMap::new();
            for (_, path) in processes() {
                if let Some((id, name)) = catalog.find(&path) {
                    let start = running.get(id).map(|g| g.start).unwrap_or_else(now_ms);
                    seen.insert(id.clone(), Game { id: id.clone(), name: name.clone(), start });
                }
            }
            running = seen;
            let mut games: Vec<&Game> = running.values().collect();
            games.sort_by_key(|g| g.start);
            let json = serde_json::to_string(&games).unwrap_or_else(|_| "[]".into());
            // Envoyé à chaque tour : la page a pu se recharger entre-temps.
            if let Some(w) = app.get_webview_window(crate::MAIN) {
                let _ = w.eval(format!("window.__plumeGames && window.__plumeGames({json})"));
            }
            std::thread::sleep(Duration::from_secs(15));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconnait_les_jeux() {
        let apps: Vec<Application> = serde_json::from_str(r#"[
            {"id":"1","name":"Rainbow Six Siege","executables":[{"is_launcher":false,"name":"rainbowsix.exe","os":"win32"},{"is_launcher":false,"name":"tom clancy's rainbow six siege/rainbowsix_vulkan.exe","os":"win32"}]},
            {"id":"2","name":"Lanceur","executables":[{"is_launcher":true,"name":"launcher.exe","os":"win32"}]},
            {"id":"3","name":"Sans exécutable"}
        ]"#).unwrap();
        let c = Catalog::build(apps);
        assert_eq!(c.find(r"C:\Games\R6\RainbowSix.exe").unwrap().1, "Rainbow Six Siege");
        assert_eq!(c.find(r"D:\Tom Clancy's Rainbow Six Siege\RainbowSix_Vulkan.exe").unwrap().0, "1");
        assert!(c.find(r"C:\x\rainbowsix_vulkan.exe").is_none());
        assert!(c.find(r"C:\x\launcher.exe").is_none());
    }
}
