//! Push-to-talk global. Discord web n'entend les touches que si sa fenêtre a le
//! focus ; ici un fil surveille la combinaison PTT dans tout Windows et prévient
//! la page. On lit l'état des touches (GetAsyncKeyState) sans rien intercepter :
//! le jeu au premier plan reçoit toujours ses touches.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::{AppHandle, Manager};

/// Codes de touches virtuelles (VK) de la combinaison PTT. Vide = PTT inactif.
#[derive(Default)]
pub struct PttKeys(pub Arc<Mutex<Vec<i32>>>);

#[cfg(windows)]
fn pressed(vk: i32) -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    unsafe { (GetAsyncKeyState(vk) as u16 & 0x8000) != 0 }
}

#[cfg(not(windows))]
fn pressed(_: i32) -> bool {
    false
}

pub fn spawn(app: AppHandle) {
    let keys = app.state::<PttKeys>().0.clone();
    std::thread::spawn(move || {
        let mut active = false;
        loop {
            std::thread::sleep(Duration::from_millis(10));
            let combo = keys.lock().unwrap().clone();
            let now = !combo.is_empty() && combo.iter().all(|&vk| pressed(vk));
            if now != active {
                active = now;
                if let Some(w) = app.get_webview_window(crate::MAIN) {
                    let _ = w.eval(format!("window.__plumePTT && window.__plumePTT({active})"));
                }
            }
        }
    });
}
