//! Lancement à l'ouverture de session, Plume caché dans la zone de
//! notification. Windows : une entrée « Plume » dans la clé Run de
//! l'utilisateur. Linux : un `.desktop` dans `~/.config/autostart`, que lisent
//! GNOME, KDE, XFCE… et Hyprland par `dex` ou `exec-once` (README).
//! Réécrite à chaque lancement, pour suivre le chemin de l'exécutable si Plume
//! a été réinstallé ailleurs.

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
pub fn apply(enabled: bool) {
    let Some(dir) = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))
    else {
        return;
    };
    let file = dir.join("autostart").join("plume.desktop");
    if !enabled {
        let _ = std::fs::remove_file(file);
        return;
    }
    let Ok(exe) = std::env::current_exe() else { return };
    let entry = format!(
        "[Desktop Entry]\nType=Application\nName=Plume\nIcon=plume\nExec=\"{}\" --minimized\nX-GNOME-Autostart-enabled=true\n",
        exe.display()
    );
    let _ = std::fs::create_dir_all(file.parent().unwrap());
    let _ = std::fs::write(file, entry);
}
