//! Frappe dans l'Activity WordBomb / BombParty.
//!
//! Le jeu tourne dans une iframe cross-origin : la page Discord ne peut ni la
//! lire ni lui envoyer d'événements. WebView2, lui, peut injecter des entrées
//! au niveau du navigateur (DevTools Protocol), ce que faisait le plugin
//! Electron de Nightcord avec SendInput. La page envoie une commande par touche
//! pour garder la main sur le rythme (vitesse réglable, fautes) ; ici on ne
//! fait que cliquer puis taper ce qu'on nous donne.

use std::time::Duration;

use tauri::WebviewWindow;

/// `click` place le curseur dans l'Activity, `text` est tapé ensuite.
/// `\n` = Entrée, `\u{8}` = retour arrière. Ne rend la main qu'une fois fait.
#[tauri::command]
pub async fn wordbomb_type(
    window: WebviewWindow,
    text: String,
    x: f64,
    y: f64,
    click: bool,
) -> Result<(), String> {
    #[cfg(windows)]
    {
        let text: String = text
            .chars()
            .filter(|c| matches!(c, '\n' | '\u{8}') || !c.is_control())
            .take(32)
            .collect();
        if text.is_empty() && !click {
            return Ok(());
        }
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        window
            .with_webview(move |wv| unsafe {
                if let Ok(core) = wv.controller().CoreWebView2() {
                    win::send(&core, &text, x, y, click, tx);
                }
            })
            .map_err(|e| e.to_string())?;
        // La chaîne CDP continue après le retour de `with_webview` ; on attend
        // sa fin pour que la page sache quand reprendre le focus.
        let _ = tauri::async_runtime::spawn_blocking(move || {
            let _ = rx.recv_timeout(Duration::from_secs(10));
        })
        .await;
    }
    #[cfg(not(windows))]
    let _ = (window, text, x, y, click);
    Ok(())
}

#[cfg(windows)]
mod win {
    use std::collections::VecDeque;
    use std::sync::mpsc::Sender;
    use std::sync::{Arc, Mutex};

    use serde_json::json;
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2;
    use webview2_com::CallDevToolsProtocolMethodCompletedHandler;
    use windows::core::HSTRING;

    type Queue = Arc<Mutex<VecDeque<(&'static str, String)>>>;
    type Done = Arc<Mutex<Option<Sender<()>>>>;

    /// Envoie la commande suivante, puis la suivante, etc. L'ordre compte :
    /// chaque commande n'est lancée qu'une fois la précédente terminée. Une
    /// erreur n'arrête pas la chaîne, sinon la page attendrait dix secondes.
    fn step(core: &ICoreWebView2, queue: &Queue, done: &Done) {
        let Some((method, params)) = queue.lock().unwrap().pop_front() else {
            if let Some(tx) = done.lock().unwrap().take() {
                let _ = tx.send(());
            }
            return;
        };
        let (next_core, next_queue, next_done) = (core.clone(), queue.clone(), done.clone());
        let handler = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |res, _| {
            if let Err(e) = res {
                eprintln!("[plume] wordbomb : {e}");
            }
            step(&next_core, &next_queue, &next_done);
            Ok(())
        }));
        let sent = unsafe {
            core.CallDevToolsProtocolMethod(
                &HSTRING::from(method),
                &HSTRING::from(params),
                &handler,
            )
        };
        if let Err(e) = sent {
            eprintln!("[plume] wordbomb : {method} : {e}");
            step(core, queue, done);
        }
    }

    /// Deux commandes CDP : un `Input.dispatchKeyEvent` ne porte qu'un
    /// événement, donc une par appui et une par relâchement.
    fn key(code: u32, name: &str, text: &str) -> [(&'static str, String); 2] {
        let ev_code = match name {
            "Enter" => "Enter".to_string(),
            "Backspace" => "Backspace".to_string(),
            " " => "Space".to_string(),
            n if n.len() == 1 && n.chars().all(|c| c.is_ascii_alphabetic()) => {
                format!("Key{}", n.to_ascii_uppercase())
            }
            n if n.len() == 1 && n.chars().all(|c| c.is_ascii_digit()) => format!("Digit{n}"),
            n => n.to_string(),
        };
        let event = |kind: &str, text: Option<&str>| {
            let mut v = json!({
                "type": kind,
                "key": name,
                "code": ev_code,
                "windowsVirtualKeyCode": code,
                "nativeVirtualKeyCode": code,
            });
            if let Some(t) = text {
                v["text"] = json!(t);
            }
            v.to_string()
        };
        [
            ("Input.dispatchKeyEvent", event("keyDown", Some(text))),
            ("Input.dispatchKeyEvent", event("keyUp", None)),
        ]
    }

    pub fn send(core: &ICoreWebView2, text: &str, x: f64, y: f64, click: bool, done: Sender<()>) {
        let queue: Queue = Arc::new(Mutex::new(VecDeque::new()));
        {
            let mut q = queue.lock().unwrap();
            if click {
                let mouse = |kind: &str, buttons: i32| {
                    (
                        "Input.dispatchMouseEvent",
                        json!({
                            "type": kind,
                            "x": x,
                            "y": y,
                            "button": "left",
                            "buttons": buttons,
                            "clickCount": 1,
                        })
                        .to_string(),
                    )
                };
                q.push_back((
                    "Input.dispatchMouseEvent",
                    json!({ "type": "mouseMoved", "x": x, "y": y }).to_string(),
                ));
                q.push_back(mouse("mousePressed", 1));
                q.push_back(mouse("mouseReleased", 0));
            }

            for ch in text.chars() {
                let (code, name, typed) = match ch {
                    '\n' => (13, "Enter".to_string(), "\r".to_string()),
                    '\u{8}' => (8, "Backspace".to_string(), String::new()),
                    c => {
                        let vk = if c.is_ascii_alphabetic() {
                            c.to_ascii_uppercase() as u32
                        } else if c.is_ascii_digit() {
                            c as u32
                        } else if c == ' ' {
                            32
                        } else {
                            0
                        };
                        (vk, c.to_string(), c.to_string())
                    }
                };
                for event in key(code, &name, &typed) {
                    q.push_back(event);
                }
            }
        }
        step(core, &queue, &Arc::new(Mutex::new(Some(done))));
    }
}
