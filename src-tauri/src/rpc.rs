//! Rich Presence des jeux (« En partie classée, 3-2 »…). Les jeux la confient
//! à l'appli Discord par un tube nommé local, `\\.\pipe\discord-ipc-N` ; Discord
//! web n'en a pas. Plume ouvre ce tube, comprend les messages comme l'appli
//! (même protocole que arRPC) et passe l'activité à la page (inject/activity.js).
//!
//! Le canal reste local des deux côtés : tube nommé sous Windows, socket
//! Unix `$XDG_RUNTIME_DIR/discord-ipc-N` ailleurs. Seuls les programmes de
//! cette session y ont accès, comme pour l'appli Discord.

use std::io::{Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

const HANDSHAKE: u32 = 0;
const FRAME: u32 = 1;
const CLOSE: u32 = 2;
const PING: u32 = 3;
const PONG: u32 = 4;
const MAX_MESSAGE: usize = 64 * 1024;

fn read_message(r: &mut impl Read) -> Option<(u32, Value)> {
    let mut head = [0u8; 8];
    r.read_exact(&mut head).ok()?;
    let op = u32::from_le_bytes(head[..4].try_into().unwrap());
    let len = u32::from_le_bytes(head[4..].try_into().unwrap()) as usize;
    if len > MAX_MESSAGE {
        return None;
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body).ok()?;
    Some((op, serde_json::from_slice(&body).unwrap_or(Value::Null)))
}

fn write_message(w: &mut impl Write, op: u32, body: &Value) -> bool {
    let body = body.to_string();
    let mut out = Vec::with_capacity(8 + body.len());
    out.extend_from_slice(&op.to_le_bytes());
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(body.as_bytes());
    w.write_all(&out).is_ok()
}

/// Activité transmise à la page ; `null` = plus d'activité pour ce jeu.
fn to_page(app: &AppHandle, msg: Value) {
    if let Some(w) = app.get_webview_window(crate::MAIN) {
        let _ = w.eval(format!("window.__plumeRpc && window.__plumeRpc({msg})"));
    }
}

/// Une connexion de jeu, jusqu'à ce qu'il la ferme.
fn serve(app: &AppHandle, mut conn: impl Read + Write, socket_id: String) {
    let mut client_id = String::new();
    while let Some((op, msg)) = read_message(&mut conn) {
        match op {
            HANDSHAKE => {
                client_id = msg["client_id"].as_str().unwrap_or_default().to_string();
                // Discord répond « prêt » avec un utilisateur : les jeux l'attendent
                // avant d'envoyer leur activité, sans s'en servir.
                let ready = json!({
                    "cmd": "DISPATCH", "evt": "READY",
                    "data": {
                        "v": 1,
                        "config": { "cdn_host": "cdn.discordapp.com", "api_endpoint": "//discord.com/api", "environment": "production" },
                        "user": { "id": "1045800378228281345", "username": "plume", "discriminator": "0", "global_name": "Plume", "avatar": null, "bot": false, "flags": 0, "premium_type": 0 }
                    }
                });
                if !write_message(&mut conn, FRAME, &ready) {
                    break;
                }
            }
            FRAME => {
                let cmd = msg["cmd"].as_str().unwrap_or_default();
                let mut data = Value::Null;
                if cmd == "SET_ACTIVITY" && !client_id.is_empty() {
                    data = msg["args"]["activity"].clone();
                    to_page(app, json!({ "clientId": client_id, "socketId": socket_id, "pid": msg["args"]["pid"], "activity": data }));
                }
                let reply = json!({ "cmd": cmd, "data": data, "evt": null, "nonce": msg["nonce"] });
                if !write_message(&mut conn, FRAME, &reply) {
                    break;
                }
            }
            PING => {
                if !write_message(&mut conn, PONG, &msg) {
                    break;
                }
            }
            CLOSE => break,
            _ => {}
        }
    }
    if !client_id.is_empty() {
        to_page(app, json!({ "clientId": client_id, "socketId": socket_id, "activity": null }));
    }
}

#[cfg(windows)]
mod pipe {
    use windows::core::HSTRING;
    use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows::Win32::Storage::FileSystem::{FlushFileBuffers, ReadFile, WriteFile, FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX};
    use windows::Win32::System::Pipes::{ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT};

    pub struct Pipe(HANDLE);
    // Un tube n'est utilisé que par le fil qui sert sa connexion.
    unsafe impl Send for Pipe {}

    impl Pipe {
        pub fn create(name: &str, first: bool) -> Option<Pipe> {
            let mode = if first { PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE } else { PIPE_ACCESS_DUPLEX };
            let h = unsafe {
                CreateNamedPipeW(&HSTRING::from(name), mode, PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS, PIPE_UNLIMITED_INSTANCES, 65536, 65536, 0, None)
            };
            (h != INVALID_HANDLE_VALUE && !h.is_invalid()).then_some(Pipe(h))
        }

        /// Attend qu'un jeu se connecte.
        pub fn accept(&self) -> bool {
            // Déjà connecté entre la création et l'attente : ce n'est pas une erreur.
            match unsafe { ConnectNamedPipe(self.0, None) } {
                Ok(()) => true,
                Err(e) => e.code() == windows::Win32::Foundation::ERROR_PIPE_CONNECTED.to_hresult(),
            }
        }
    }

    impl std::io::Read for Pipe {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let mut n = 0u32;
            unsafe { ReadFile(self.0, Some(buf), Some(&mut n), None) }.map_err(std::io::Error::other)?;
            Ok(n as usize)
        }
    }

    impl std::io::Write for Pipe {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            let mut n = 0u32;
            unsafe { WriteFile(self.0, Some(buf), Some(&mut n), None) }.map_err(std::io::Error::other)?;
            Ok(n as usize)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            unsafe { FlushFileBuffers(self.0) }.map_err(std::io::Error::other)
        }
    }

    impl Drop for Pipe {
        fn drop(&mut self) {
            unsafe {
                let _ = DisconnectNamedPipe(self.0);
                let _ = CloseHandle(self.0);
            }
        }
    }
}

#[cfg(windows)]
pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        // Les jeux essaient discord-ipc-0 à 9 : on prend le premier libre (si
        // l'appli Discord tourne aussi, elle garde le 0 et ses jeux).
        let Some((name, first)) = (0..10).find_map(|i| {
            let name = format!(r"\\.\pipe\discord-ipc-{i}");
            pipe::Pipe::create(&name, true).map(|p| (name, p))
        }) else {
            eprintln!("[plume] rich presence : aucun tube discord-ipc libre");
            return;
        };
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let mut waiting = Some(first);
        loop {
            let Some(conn) = waiting.take().or_else(|| pipe::Pipe::create(&name, false)) else {
                std::thread::sleep(std::time::Duration::from_secs(5));
                continue;
            };
            if !conn.accept() {
                continue;
            }
            let app = app.clone();
            let socket_id = format!("plume-rpc-{}", NEXT.fetch_add(1, Ordering::Relaxed));
            std::thread::spawn(move || serve(&app, conn, socket_id));
        }
    });
}

#[cfg(unix)]
pub fn spawn(app: AppHandle) {
    use std::os::unix::net::{UnixListener, UnixStream};

    std::thread::spawn(move || {
        // Là où l'appli Discord la pose, donc là où les jeux la cherchent.
        let dir = std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from).unwrap_or_else(std::env::temp_dir);
        // Les jeux essaient discord-ipc-0 à 9 : on prend la première libre (si
        // l'appli Discord tourne aussi, elle garde la 0 et ses jeux).
        let Some(listener) = (0..10).find_map(|i| {
            let path = dir.join(format!("discord-ipc-{i}"));
            // Une socket laissée par un processus mort occuperait la place pour
            // rien : on ne retire que celles où plus personne ne répond.
            if path.exists() && UnixStream::connect(&path).is_err() {
                let _ = std::fs::remove_file(&path);
            }
            UnixListener::bind(&path).ok()
        }) else {
            eprintln!("[plume] rich presence : aucune socket discord-ipc libre");
            return;
        };
        static NEXT: AtomicU64 = AtomicU64::new(0);
        for conn in listener.incoming().flatten() {
            let app = app.clone();
            let socket_id = format!("plume-rpc-{}", NEXT.fetch_add(1, Ordering::Relaxed));
            std::thread::spawn(move || serve(&app, conn, socket_id));
        }
    });
}

#[cfg(not(any(windows, unix)))]
pub fn spawn(_: AppHandle) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trames_aller_retour() {
        let mut buf = Vec::new();
        assert!(write_message(&mut buf, FRAME, &json!({ "cmd": "SET_ACTIVITY", "nonce": "1" })));
        let (op, msg) = read_message(&mut buf.as_slice()).unwrap();
        assert_eq!(op, FRAME);
        assert_eq!(msg["nonce"], "1");
        // Taille annoncée démesurée : refusée sans rien allouer.
        let mut big = Vec::new();
        big.extend_from_slice(&FRAME.to_le_bytes());
        big.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(read_message(&mut big.as_slice()).is_none());
    }
}

