//! Lancement au démarrage de Windows : une entrée « Plume » dans la clé Run de
//! l'utilisateur, qui démarre Plume caché dans la zone de notification.
//! Réécrite à chaque lancement, pour suivre le chemin de plume.exe si Plume a
//! été réinstallé ailleurs.

#[cfg(windows)]
pub fn apply(enabled: bool) {
    use windows::core::{w, HSTRING};
    use windows::Win32::System::Registry::{RegCloseKey, RegCreateKeyW, RegDeleteValueW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, REG_SZ};

    unsafe {
        let mut run = HKEY::default();
        if RegCreateKeyW(HKEY_CURRENT_USER, w!(r"Software\Microsoft\Windows\CurrentVersion\Run"), &mut run).is_err() {
            return;
        }
        if enabled {
            if let Ok(exe) = std::env::current_exe() {
                let command = format!("\"{}\" --minimized", exe.display());
                let data: Vec<u16> = command.encode_utf16().chain([0]).collect();
                let bytes = std::slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * 2);
                let _ = RegSetValueExW(run, &HSTRING::from("Plume"), None, REG_SZ, Some(bytes));
            }
        } else {
            let _ = RegDeleteValueW(run, &HSTRING::from("Plume"));
        }
        let _ = RegCloseKey(run);
    }
}

#[cfg(not(windows))]
pub fn apply(_enabled: bool) {}
