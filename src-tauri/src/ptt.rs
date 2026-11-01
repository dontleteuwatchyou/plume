//! Push-to-talk global. Discord web n'entend les touches que si sa fenêtre a le
//! focus ; ici un fil surveille la combinaison PTT dans tout le système et
//! prévient la page. On lit l'état des touches sans rien intercepter : le jeu au
//! premier plan reçoit toujours ses touches.
//!
//! Windows : GetAsyncKeyState. Linux : l'état des claviers et souris lu
//! directement dans le noyau (evdev), seul chemin qui marche pareil sous X11 et
//! sous tous les compositeurs Wayland ; il faut faire partie du groupe `input`.
//!
//! Le fil ne lit le clavier que lorsque la page lui a confié une combinaison,
//! c'est-à-dire pendant un appel en mode push-to-talk (inject/ptt.js), et
//! seulement les touches de cette combinaison. Le reste du temps il dort : pas
//! de lecture du clavier en continu.

use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use tauri::{AppHandle, Manager};

/// La combinaison PTT : chaque touche est une liste d'équivalents (Maj gauche
/// ou droite…), dans les codes du système. Vide = PTT inactif.
#[derive(Default)]
pub struct PttKeys(pub Arc<(Mutex<Vec<Vec<i32>>>, Condvar)>);

impl PttKeys {
    /// `keys` : codes de touches virtuelles Windows (VK), ceux que donne la page.
    pub fn set(&self, keys: Vec<i32>) {
        let (lock, wake) = &*self.0;
        *lock.lock().unwrap() = keys.into_iter().map(physical).collect();
        wake.notify_all();
    }
}

#[cfg(windows)]
fn physical(vk: i32) -> Vec<i32> {
    vec![vk]
}

#[cfg(windows)]
#[derive(Default)]
struct Keyboard;

#[cfg(windows)]
impl Keyboard {
    fn open() -> Self {
        Keyboard
    }
    fn poll(&mut self) {}
    fn pressed(&self, vk: i32) -> bool {
        use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
        unsafe { (GetAsyncKeyState(vk) as u16 & 0x8000) != 0 }
    }
}

/// Code VK → touches physiques Linux (codes evdev). Les lettres suivent la
/// disposition du clavier (le A d'un AZERTY n'est pas à la place du A d'un
/// QWERTY) : on les demande à GDK, qui connaît la disposition active sous X11
/// comme sous Wayland. Le reste du clavier ne bouge pas d'une disposition à
/// l'autre.
#[cfg(not(windows))]
fn physical(vk: i32) -> Vec<i32> {
    if (0x41..=0x5a).contains(&vk) {
        let keyval = gtk::gdk::keys::Key::from(u32::try_from(vk + 0x20).unwrap_or(0));
        let from_layout = gtk::gdk::Display::default()
            .and_then(|d| gtk::gdk::Keymap::for_display(&d))
            .map(|map| map.entries_for_keyval(*keyval).iter().filter(|k| k.group() == 0).map(|k| k.keycode() as i32 - 8).collect::<Vec<_>>())
            .unwrap_or_default();
        if !from_layout.is_empty() {
            return from_layout;
        }
        // Sans affichage, on retombe sur la place QWERTY.
        const LETTERS: [i32; 26] = [30, 48, 46, 32, 18, 33, 34, 35, 23, 36, 37, 38, 50, 49, 24, 25, 16, 19, 31, 20, 22, 47, 17, 45, 21, 44];
        return vec![LETTERS[(vk - 0x41) as usize]];
    }
    let codes: &[i32] = match vk {
        0x01 => &[0x110], // boutons de souris : gauche, droit, milieu, côté
        0x02 => &[0x111],
        0x04 => &[0x112],
        0x05 => &[0x113],
        0x06 => &[0x114],
        0x08 => &[14],
        0x09 => &[15],
        0x0d => &[28, 96],
        0x10 => &[42, 54],
        0x11 => &[29, 97],
        0x12 => &[56, 100],
        0x13 => &[119],
        0x14 => &[58],
        0x1b => &[1],
        0x20 => &[57],
        0x21 => &[104],
        0x22 => &[109],
        0x23 => &[107],
        0x24 => &[102],
        0x25 => &[105],
        0x26 => &[103],
        0x27 => &[106],
        0x28 => &[108],
        0x2c => &[99],
        0x2d => &[110],
        0x2e => &[111],
        0x30 => &[11],
        0x31..=0x39 => return vec![vk - 0x31 + 2],
        0x5b => &[125],
        0x5c => &[126],
        0x5d => &[127],
        0x60..=0x69 => return vec![[82, 79, 80, 81, 75, 76, 77, 71, 72, 73][(vk - 0x60) as usize]],
        0x6a => &[55],
        0x6b => &[78],
        0x6d => &[74],
        0x6e => &[83],
        0x6f => &[98],
        0x70..=0x79 => return vec![vk - 0x70 + 59],
        0x7a => &[87],
        0x7b => &[88],
        0x7c..=0x87 => return vec![vk - 0x7c + 183],
        0x90 => &[69],
        0x91 => &[70],
        0xa0 => &[42],
        0xa1 => &[54],
        0xa2 => &[29],
        0xa3 => &[97],
        0xa4 => &[56],
        0xa5 => &[100],
        0xba => &[39],
        0xbb => &[13],
        0xbc => &[51],
        0xbd => &[12],
        0xbe => &[52],
        0xbf => &[53],
        0xc0 => &[41],
        0xdb => &[26],
        0xdc => &[43],
        0xdd => &[27],
        0xde => &[40],
        0xe2 => &[86],
        _ => &[],
    };
    codes.to_vec()
}

/// Les périphériques d'entrée lisibles, et l'état de toutes leurs touches à la
/// dernière lecture. On ne lit jamais leurs événements : seulement l'état
/// courant, que le noyau tient à jour (EVIOCGKEY).
#[cfg(not(windows))]
struct Keyboard {
    devices: Vec<std::fs::File>,
    state: [u8; 96],
}

#[cfg(not(windows))]
impl Keyboard {
    fn open() -> Self {
        use std::os::unix::fs::OpenOptionsExt;
        let devices: Vec<_> = std::fs::read_dir("/dev/input")
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("event"))
            .filter_map(|e| std::fs::OpenOptions::new().read(true).custom_flags(libc::O_NONBLOCK).open(e.path()).ok())
            .collect();
        if devices.is_empty() {
            eprintln!("[plume] push-to-talk global : aucun clavier lisible, ajoute-toi au groupe input (sudo usermod -aG input $USER)");
        }
        Keyboard { devices, state: [0; 96] }
    }

    fn poll(&mut self) {
        use std::os::fd::AsRawFd;
        // EVIOCGKEY(96) : _IOC(_IOC_READ, 'E', 0x18, 96).
        const EVIOCGKEY: libc::c_ulong = (2 << 30) | (96 << 16) | ((b'E' as libc::c_ulong) << 8) | 0x18;
        self.state = [0; 96];
        for dev in &self.devices {
            let mut bits = [0u8; 96];
            if unsafe { libc::ioctl(dev.as_raw_fd(), EVIOCGKEY as _, bits.as_mut_ptr()) } >= 0 {
                for (s, b) in self.state.iter_mut().zip(bits) {
                    *s |= b;
                }
            }
        }
    }

    fn pressed(&self, code: i32) -> bool {
        let code = code as usize;
        code < 96 * 8 && self.state[code / 8] & (1 << (code % 8)) != 0
    }
}

pub fn spawn(app: AppHandle) {
    let keys = app.state::<PttKeys>().0.clone();
    std::thread::spawn(move || {
        let (lock, wake) = &*keys;
        let mut active = false;
        let mut last: Vec<Vec<i32>> = Vec::new();
        let mut keyboard = None;
        loop {
            // Sans combinaison : on dort jusqu'à ce que la page en donne une.
            let combo = {
                let mut k = lock.lock().unwrap();
                while k.is_empty() {
                    if active {
                        break;
                    }
                    keyboard = None;
                    k = wake.wait(k).unwrap();
                }
                k.clone()
            };
            // Nouvelle combinaison : on rouvre les périphériques, pour voir un
            // clavier ou une souris branchés depuis.
            if combo != last || keyboard.is_none() {
                keyboard = Some(Keyboard::open());
                last = combo.clone();
            }
            let kb = keyboard.as_mut().unwrap();
            kb.poll();
            let now = !combo.is_empty() && combo.iter().all(|alts| alts.iter().any(|&k| kb.pressed(k)));
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
