//! Lecteur réduit (« Pop out ») : la fenêtre détachée que Discord ouvre pour
//! un stream, un appel ou un message.
//!
//! Discord l'ouvre par `window.open("/popout")` puis **y dessine depuis la
//! fenêtre principale** (portail React) : la page fille a besoin de la même
//! session, de la même origine et d'un `window.opener` utilisable. Refuser la
//! fenêtre et envoyer l'URL au navigateur (ce que Plume faisait) donnait donc
//! un `discord.com/popout` vide dans Edge ou Firefox.
//!
//! La seule façon d'avoir une vraie fenêtre fille est de répondre `Create` au
//! gestionnaire de nouvelles fenêtres avec une fenêtre bâtie sur
//! l'environnement WebView2 de celle qui l'a demandée : c'est ce que fait
//! `window_features`, et WebView2 se charge ensuite d'y charger l'URL.
//!
//! Comme c'est la fenêtre principale qui dessine dedans, tout ce qui la
//! dégrade la viderait : `any_open` sert de garde-fou à la veille
//! (`sleep.rs`) et à la mise en sommeil de l'affichage (`main.rs`).

use std::sync::atomic::{AtomicU32, Ordering};

use tauri::webview::NewWindowFeatures;
use tauri::window::Color;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

/// Début des étiquettes des lecteurs réduits : sert aussi à les reconnaître.
const PREFIX: &str = "popout-";

/// Fenêtre fille pour `url`, rattachée au moteur de la fenêtre qui l'a
/// demandée. À rendre telle quelle dans `NewWindowResponse::Create`.
pub fn build(app: &AppHandle, features: NewWindowFeatures) -> tauri::Result<WebviewWindow> {
    static NEXT: AtomicU32 = AtomicU32::new(1);
    let label = format!("{PREFIX}{}", NEXT.fetch_add(1, Ordering::Relaxed));
    let nav_app = app.clone();
    let popup_app = app.clone();
    WebviewWindowBuilder::new(app, label, WebviewUrl::External("about:blank".parse().unwrap()))
        .title("Plume — lecteur réduit")
        .background_color(Color(0x0e, 0x0c, 0x16, 0xff))
        // Taille et position demandées par Discord si elle en donne une.
        .inner_size(860.0, 520.0)
        .min_inner_size(320.0, 180.0)
        // Le but d'un lecteur réduit : rester visible par-dessus le jeu.
        .always_on_top(true)
        // Cadre Windows, contrairement à la fenêtre principale : la page du
        // lecteur ne dessine aucune barre qu'on puisse attraper (WebView2
        // ignore `-webkit-app-region: drag`, propre à Electron), sans cadre
        // elle ne serait ni déplaçable ni fermable.
        .on_navigation(move |url| {
            if crate::is_discord(url) {
                return true;
            }
            crate::open_external(&nav_app, url.as_str());
            false
        })
        // Un lecteur réduit n'en ouvre pas d'autre : ses liens partent dans le
        // navigateur, comme ceux de la fenêtre principale.
        .on_new_window(move |url, _| {
            crate::open_external(&popup_app, url.as_str());
            tauri::webview::NewWindowResponse::Deny
        })
        // Applique l'environnement WebView2 de la fenêtre appelante (sans quoi
        // WebView2 refuse la fenêtre fille), sa taille et sa position.
        .window_features(features)
        .build()
        .inspect(|w| {
            // Discord referme le lecteur par `window.close()` : sans ça il
            // resterait une fenêtre Plume vide, toujours au-dessus des autres.
            crate::native::close_window_on_page_close(w);
        })
}

/// Un lecteur réduit est-il ouvert ? La fenêtre principale doit alors rester
/// chargée et rendue : c'est elle qui dessine dedans.
pub fn any_open(app: &AppHandle) -> bool {
    app.webview_windows().keys().any(|l| l.starts_with(PREFIX))
}

/// Avant de recharger Discord : la page qui dessinait dedans disparaît, les
/// lecteurs réduits resteraient vides.
pub fn close_all(app: &AppHandle) {
    for (label, w) in app.webview_windows() {
        if label.starts_with(PREFIX) {
            let _ = w.close();
        }
    }
}
