//! Liens discord:// (boutons « Ouvrir dans l'appli », invitations…) ouverts
//! dans Plume, comme le fait l'appli Discord. Plume s'enregistre pour ce
//! protocole sans droits admin : dans le registre de l'utilisateur (HKCU) sous
//! Windows, comme gestionnaire `x-scheme-handler/discord` ailleurs.

use tauri::{AppHandle, Manager};

/// `discord://-/channels/1/2` → `/channels/1/2`.
fn path_of(link: &str) -> Option<String> {
    let rest = link.strip_prefix("discord://").or_else(|| link.strip_prefix("discord:"))?;
    let rest = rest.trim_start_matches('-').trim_start_matches('/');
    // Seulement un chemin Discord : pas d'autre site, pas de script.
    if rest.contains("://") || rest.contains(':') {
        return None;
    }
    Some(format!("/{rest}"))
}

/// Lien reçu au lancement : Discord s'ouvre directement dessus (une seule
/// fois : « Recharger » revient à l'accueil).
pub fn take_start_url() -> Option<String> {
    use std::sync::atomic::{AtomicBool, Ordering};
    static TAKEN: AtomicBool = AtomicBool::new(false);
    if TAKEN.swap(true, Ordering::Relaxed) {
        return None;
    }
    let args: Vec<String> = std::env::args().collect();
    args.iter().find_map(|a| path_of(a)).map(|p| format!("https://discord.com{p}"))
}

/// Ouvre dans la fenêtre principale le lien discord:// passé en argument.
pub fn handle(app: &AppHandle, args: &[String]) {
    let Some(path) = args.iter().find_map(|a| path_of(a)) else { return };
    if let Some(w) = app.get_webview_window(crate::MAIN) {
        let path = serde_json::to_string(&path).unwrap();
        let _ = w.eval(format!("window.__plumeOpen ? window.__plumeOpen({path}) : location.assign({path})"));
        crate::show_main(app);
    }
}

#[cfg(windows)]
pub fn register() {
    use windows::core::{w, HSTRING, PCWSTR};
    use windows::Win32::System::Registry::{RegCloseKey, RegCreateKeyW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, REG_SZ};

    let Ok(exe) = std::env::current_exe() else { return };
    let command = format!("\"{}\" \"%1\"", exe.display());
    let set = |key: HKEY, name: PCWSTR, value: &str| unsafe {
        let data: Vec<u16> = value.encode_utf16().chain([0]).collect();
        let bytes = std::slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * 2);
        let _ = RegSetValueExW(key, name, None, REG_SZ, Some(bytes));
    };
    unsafe {
        let mut root = HKEY::default();
        if RegCreateKeyW(HKEY_CURRENT_USER, w!(r"Software\Classes\discord"), &mut root).is_err() {
            return;
        }
        set(root, PCWSTR::null(), "URL:Discord Protocol");
        set(root, w!("URL Protocol"), "");
        let mut cmd = HKEY::default();
        if RegCreateKeyW(root, &HSTRING::from(r"shell\open\command"), &mut cmd).is_ok() {
            set(cmd, PCWSTR::null(), &command);
            let _ = RegCloseKey(cmd);
        }
        let _ = RegCloseKey(root);
    }
}

/// Le paquet installe `plume.desktop`, qui déclare le protocole ; reste à en
/// faire le gestionnaire par défaut de l'utilisateur, comme sous Windows.
#[cfg(not(windows))]
pub fn register() {
    std::thread::spawn(|| {
        let _ = std::process::Command::new("xdg-mime")
            .args(["default", "plume.desktop", "x-scheme-handler/discord"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn chemins() {
        assert_eq!(super::path_of("discord://-/channels/1/2").as_deref(), Some("/channels/1/2"));
        assert_eq!(super::path_of("discord:///invite/abc").as_deref(), Some("/invite/abc"));
        assert_eq!(super::path_of("discord://javascript:alert(1)"), None);
        assert_eq!(super::path_of("--minimized"), None);
    }
}
