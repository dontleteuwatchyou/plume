//! Push-to-talk global. Discord web n'entend les touches que si sa fenêtre a le
//! focus ; ici un fil surveille la combinaison PTT dans tout Windows et prévient
//! la page. On lit l'état des touches (GetAsyncKeyState) sans rien intercepter :
//! le jeu au premier plan reçoit toujours ses touches.
//!
//! Le fil ne lit le clavier que lorsque la page lui a confié une combinaison,
//! c'est-à-dire pendant un appel en mode push-to-talk (inject/ptt.js), et
//! seulement les touches de cette combinaison. Le reste du temps il dort : pas
//! de lecture du clavier en continu.

use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use tauri::{AppHandle, Manager};

/// Codes de touches virtuelles (VK) de la combinaison PTT. Vide = PTT inactif.
#[derive(Default)]
pub struct PttKeys(pub Arc<(Mutex<Vec<i32>>, Condvar)>);

impl PttKeys {
    pub fn set(&self, keys: Vec<i32>) {
        let (lock, wake) = &*self.0;
        *lock.lock().unwrap() = keys;
        wake.notify_all();
    }
}

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
        let (lock, wake) = &*keys;
        let mut active = false;
        loop {
            // Sans combinaison : on dort jusqu'à ce que la page en donne une.
            let combo = {
                let mut k = lock.lock().unwrap();
                while k.is_empty() {
                    if active {
                        break;
                    }
                    k = wake.wait(k).unwrap();
                }
                k.clone()
            };
            let now = !combo.is_empty() && combo.iter().all(|&vk| pressed(vk));
            if now != active {
                active = now;
                if let Some(w) = app.get_webview_window(crate::MAIN) {
                    let _ = w.eval(format!("window.__plumePTT && window.__plumePTT({active})"));
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    });
}
