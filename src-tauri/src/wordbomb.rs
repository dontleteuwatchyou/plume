//! Frappe d'un mot dans l'Activity WordBomb / BombParty.
//!
//! Le jeu tourne dans une iframe cross-origin : la page Discord ne peut ni la
//! lire ni lui envoyer d'événements. WebView2, lui, peut injecter des entrées
//! au niveau du navigateur (DevTools Protocol), ce que faisait le plugin
//! Electron de Nightcord avec SendInput. On clique donc dans l'iframe, on tape
//! le mot touche par touche, puis Entrée.

use std::time::Duration;

use tauri::WebviewWindow;

/// Tape `text` dans l'Activity, aux coordonnées CSS données par la page.
/// Ne rend la main qu'une fois la frappe terminée.
#[tauri::command]
pub async fn wordbomb_type(window: WebviewWindow, text: String, x: f64, y: f64) -> Result<(), String> {
    #[cfg(windows)]
    {
        let text: String = text.chars().filter(|c| !c.is_control()).take(64).collect();
        if text.is_empty() {
            return Ok(());
        }
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        window
            .with_webview(move |wv| unsafe {
                if let Ok(core) = wv.controller().CoreWebView2() {
                    win::type_word(&core, &text, x, y, tx);
                }
            })
            .map_err(|e| e.to_string())?;
        // La chaîne CDP continue après le retour de `with_webview` ; on attend
        // sa fin pour que la page puisse reprendre le focus après la frappe.
        let _ = tauri::async_runtime::spawn_blocking(move || {
            let _ = rx.recv_timeout(Duration::from_secs(10));
        })
        .await;
    }
    #[cfg(not(windows))]
    let _ = (window, text, x, y);
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
    /// chaque commande n'est lancée qu'une fois la précédente terminée.
    fn step(core: &ICoreWebView2, queue: &Queue, done: &Done) {
        let Some((method, params)) = queue.lock().unwrap().pop_front() else {
            if let Some(tx) = done.lock().unwrap().take() {
                let _ = tx.send(());
            }
            return;
        };
        let (next_core, next_queue, next_done) = (core.clone(), queue.clone(), done.clone());
        let handler = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |res, _| {
            match res {
                Ok(()) => step(&next_core, &next_queue, &next_done),
                Err(e) => eprintln!("[plume] wordbomb : {e}"),
            }
            Ok(())
        }));
        unsafe {
            let _ = core.CallDevToolsProtocolMethod(
                &HSTRING::from(method),
                &HSTRING::from(params),
                &handler,
            );
        }
    }

    /// Deux commandes CDP : un `Input.dispatchKeyEvent` ne porte qu'un
    /// événement, donc une par appui et une par relâchement.
    fn key(code: u32, name: &str, text: &str) -> [(&'static str, String); 2] {
        let ev_code = match name {
            "Enter" => "Enter".to_string(),
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

    pub fn type_word(core: &ICoreWebView2, text: &str, x: f64, y: f64, done: Sender<()>) {
        let queue: Queue = Arc::new(Mutex::new(VecDeque::new()));
        {
            let mut q = queue.lock().unwrap();
            let mouse = |kind: &str| {
                (
                    "Input.dispatchMouseEvent",
                    json!({
                        "type": kind,
                        "x": x,
                        "y": y,
                        "button": "left",
                        "buttons": if kind == "mouseReleased" { 0 } else { 1 },
                        "clickCount": 1,
                    })
                    .to_string(),
                )
            };
            // Le survol aide les champs qui n'affichent le curseur qu'au hover.
            q.push_back((
                "Input.dispatchMouseEvent",
                json!({ "type": "mouseMoved", "x": x, "y": y }).to_string(),
            ));
            q.push_back(mouse("mousePressed"));
            q.push_back(mouse("mouseReleased"));

            for ch in text.chars() {
                let (code, name) = match ch {
                    'a'..='z' | 'A'..='Z' => (ch.to_ascii_uppercase() as u32, ch.to_string()),
                    '0'..='9' => (ch as u32, ch.to_string()),
                    ' ' => (32, " ".into()),
                    _ => (0, ch.to_string()),
                };
                for event in key(code, &name, &ch.to_string()) {
                    q.push_back(event);
                }
            }
            for event in key(13, "Enter", "\r") {
                q.push_back(event);
            }
        }
        step(core, &queue, &Arc::new(Mutex::new(Some(done))));
    }
}
