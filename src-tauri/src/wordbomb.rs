//! Frappe dans l'Activity WordBomb / BombParty.
//!
//! Le jeu tourne dans une iframe cross-origin : la page Discord ne peut ni la
//! lire ni lui envoyer d'événements. WebView2, lui, peut injecter des entrées
//! au niveau du navigateur (DevTools Protocol), ce que faisait le plugin
//! Electron de Nightcord avec SendInput. WebKitGTK n'a pas ce protocole : on y
//! passe par GTK, avec de vrais événements de souris et de clavier envoyés à la
//! vue, comme s'ils venaient du matériel. La page envoie une commande par touche
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
    let text: String = text
        .chars()
        .filter(|c| matches!(c, '\n' | '\u{8}') || !c.is_control())
        .take(32)
        .collect();
    if text.is_empty() && !click {
        return Ok(());
    }
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    #[cfg(windows)]
    window
        .with_webview(move |wv| unsafe {
            if let Ok(core) = wv.controller().CoreWebView2() {
                win::send(&core, &text, x, y, click, tx);
            }
        })
        .map_err(|e| e.to_string())?;
    #[cfg(not(windows))]
    window
        .with_webview(move |wv| unix::send(&wv.inner(), text, x, y, click, tx))
        .map_err(|e| e.to_string())?;
    // La frappe continue après le retour de `with_webview` ; on attend sa fin
    // pour que la page sache quand reprendre le focus.
    let _ = tauri::async_runtime::spawn_blocking(move || {
        let _ = rx.recv_timeout(Duration::from_secs(10));
    })
    .await;
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

#[cfg(not(windows))]
mod unix {
    use std::sync::mpsc::Sender;
    use std::time::Duration;

    use gtk::gdk::ffi as gdk;
    use gtk::glib::translate::{ToGlibPtr, IntoGlib};
    use gtk::prelude::*;
    use webkit2gtk::{WebView, WebViewExt};

    /// Envoie un événement fabriqué à GTK, qui le traite comme s'il venait du
    /// matériel : WebKit le passe à l'iframe sous le curseur ou qui a le focus,
    /// même d'une autre origine.
    unsafe fn dispatch(kind: gdk::GdkEventType, window: &gtk::gdk::Window, device: Option<&gtk::gdk::Device>, fill: impl FnOnce(*mut gdk::GdkEvent)) {
        unsafe {
            let ev = gdk::gdk_event_new(kind);
            // L'événement garde une référence à sa fenêtre, rendue par gdk_event_free.
            (*ev).any.window = window.to_glib_full();
            fill(ev);
            if let Some(d) = device {
                gdk::gdk_event_set_device(ev, d.to_glib_none().0);
            }
            gtk::ffi::gtk_main_do_event(ev);
            gdk::gdk_event_free(ev);
        }
    }

    fn seat(view: &WebView) -> Option<gtk::gdk::Seat> {
        WidgetExt::display(view).default_seat()
    }

    fn click(view: &WebView, x: f64, y: f64) {
        let Some(window) = view.window() else { return };
        // La page compte en pixels CSS, depuis le coin de la vue.
        let zoom = view.zoom_level();
        let (mut x, mut y) = (x * zoom, y * zoom);
        if !view.has_window() {
            let a = view.allocation();
            x += f64::from(a.x());
            y += f64::from(a.y());
        }
        let pointer = seat(view).and_then(|s| s.pointer());
        for kind in [gdk::GDK_BUTTON_PRESS, gdk::GDK_BUTTON_RELEASE] {
            unsafe {
                dispatch(kind, &window, pointer.as_ref(), |ev| {
                    let b = &mut (*ev).button;
                    b.x = x;
                    b.y = y;
                    b.button = 1;
                    b.state = if kind == gdk::GDK_BUTTON_RELEASE { gdk::GDK_BUTTON1_MASK } else { 0 };
                });
            }
        }
    }

    /// Entrée : une vraie touche, c'est elle que le jeu attend pour valider.
    fn enter(view: &WebView) {
        let Some(window) = view.toplevel().and_then(|t| t.window()) else { return };
        let keyval = gtk::gdk::keys::constants::Return.into_glib();
        let hardware = gtk::gdk::Keymap::for_display(&WidgetExt::display(view))
            .and_then(|m| m.entries_for_keyval(keyval).first().map(|k| k.keycode()))
            .unwrap_or(36);
        let keyboard = seat(view).and_then(|s| s.keyboard());
        for kind in [gdk::GDK_KEY_PRESS, gdk::GDK_KEY_RELEASE] {
            unsafe {
                dispatch(kind, &window, keyboard.as_ref(), |ev| {
                    let k = &mut (*ev).key;
                    k.keyval = keyval;
                    k.hardware_keycode = hardware as u16;
                });
            }
        }
    }

    /// Le texte, lui, s'insère par la commande d'édition du moteur, dans le
    /// champ qui a le focus : elle produit les mêmes événements `input` qu'une
    /// frappe, sans dépendre de la méthode de saisie du bureau (IBus, fcitx).
    fn type_text(view: &WebView, text: &str) {
        let mut run = String::new();
        let flush = |run: &mut String| {
            if !run.is_empty() {
                view.execute_editing_command_with_argument("InsertText", run);
                run.clear();
            }
        };
        for ch in text.chars() {
            match ch {
                '\n' => {
                    flush(&mut run);
                    enter(view);
                }
                '\u{8}' => {
                    flush(&mut run);
                    view.execute_editing_command("DeleteBackward");
                }
                c => run.push(c),
            }
        }
        flush(&mut run);
    }

    pub fn send(view: &WebView, text: String, x: f64, y: f64, click_first: bool, done: Sender<()>) {
        view.grab_focus();
        if !click_first {
            type_text(view, &text);
            let _ = done.send(());
            return;
        }
        click(view, x, y);
        // Le clic donne le focus au champ du jeu ; on le laisse arriver avant
        // d'écrire.
        let view = view.clone();
        gtk::glib::timeout_add_local_once(Duration::from_millis(60), move || {
            type_text(&view, &text);
            let _ = done.send(());
        });
    }
}
