//! Réglages WebView2 que la page ne peut pas faire seule (Windows).

use std::path::PathBuf;

use tauri::{Manager, WebviewWindow};

#[cfg(windows)]
mod win {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use webview2_com::Microsoft::Web::WebView2::Win32::*;
    use webview2_com::{
        take_pwstr, BrowserExtensionEnableCompletedHandler, BrowserExtensionRemoveCompletedHandler,
        ProfileAddBrowserExtensionCompletedHandler, ProfileGetBrowserExtensionsCompletedHandler, WebResourceRequestedEventHandler,
        WindowCloseRequestedEventHandler,
    };
    use windows::core::{Interface, HSTRING, PWSTR};

    /// Télémétrie de Discord, comme Legcord : statistiques d'usage (`science`),
    /// mesures de performance (`metrics`) et rapports d'erreurs (Sentry).
    /// Le plugin NoTrack du mod en coupe une partie dans le code ; ici c'est
    /// le réseau, donc aussi sans mod. Seules ces adresses déclenchent le
    /// gestionnaire, et la réponse est un 204 vide : Discord croit l'envoi
    /// réussi et ne réessaie pas.
    const TRACKERS: [&str; 7] = [
        "*discord.com/api/v*/science*",
        "*discord.com/api/science*",
        "*discord.com/api/v*/metrics*",
        "*discord.com/error-reporting-proxy/*",
        "*.sentry.io/*",
        // Mesures de Cloudflare (RUM) et Google Analytics, comme discord-lite.
        "*discord.com/cdn-cgi/rum*",
        "*.google-analytics.com/*",
    ];

    pub fn block_trackers(core: &ICoreWebView2, env: ICoreWebView2Environment) {
        let handler = WebResourceRequestedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()) };
            unsafe {
                let response = env.CreateWebResourceResponse(None, 204, &HSTRING::from("No Content"), &HSTRING::new())?;
                args.SetResponse(&response)?;
            }
            Ok(())
        }));
        let mut token = 0i64;
        unsafe {
            for uri in TRACKERS {
                let _ = core.AddWebResourceRequestedFilter(&HSTRING::from(uri), COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL);
            }
            let _ = core.add_WebResourceRequested(&handler, &mut token);
        }
    }

    /// `window.close()` depuis la page : wry ne détruit que le conteneur du
    /// webview, la fenêtre Tauri resterait ouverte et vide.
    pub fn on_page_close(core: &ICoreWebView2, f: Box<dyn Fn() + Send>) {
        let handler = WindowCloseRequestedEventHandler::create(Box::new(move |_, _| {
            f();
            Ok(())
        }));
        let mut token = 0i64;
        unsafe {
            let _ = core.add_WindowCloseRequested(&handler, &mut token);
        }
    }

    pub fn memory_level(core: &ICoreWebView2, low: bool) {
        if let Ok(core) = core.cast::<ICoreWebView2_19>() {
            let level = if low {
                COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW
            } else {
                COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL
            };
            unsafe {
                let _ = core.SetMemoryUsageTargetLevel(level);
            }
        }
    }

    type Done = Arc<Mutex<Option<Box<dyn FnOnce() + Send>>>>;

    fn finish(done: &Done) {
        if let Some(f) = done.lock().unwrap().take() {
            f();
        }
    }

    fn add(profile: &ICoreWebView2Profile7, folder: Option<&PathBuf>, done: &Done) {
        let Some(folder) = folder else { return finish(done) };
        let d = done.clone();
        let handler = ProfileAddBrowserExtensionCompletedHandler::create(Box::new(move |res, _| {
            if let Err(e) = res {
                eprintln!("[plume] installation du mod : {e}");
            }
            finish(&d);
            Ok(())
        }));
        unsafe {
            if profile.AddBrowserExtension(&HSTRING::from(folder.as_os_str()), &handler).is_err() {
                finish(done);
            }
        }
    }

    /// Met le profil en phase avec le mod voulu : garde l'extension de
    /// `folder` si elle est déjà installée (réactivée au besoin), retire toutes
    /// les autres, et n'installe `folder` que s'il manque. Puis appelle `then`.
    /// Une même extension n'est donc jamais retirée puis réinstallée d'un coup,
    /// ce que WebView2 refuse parfois (0x80004005).
    pub fn install_extension(core: &ICoreWebView2, folder: Option<PathBuf>, then: Box<dyn FnOnce() + Send>) {
        let done: Done = Arc::new(Mutex::new(Some(then)));
        let profile = core
            .cast::<ICoreWebView2_13>()
            .and_then(|c| unsafe { c.Profile() })
            .and_then(|p| p.cast::<ICoreWebView2Profile7>());
        let Ok(profile) = profile else { return finish(&done) };
        let wanted = folder.as_deref().map(crate::mods::extension_id);

        let (p, d) = (profile.clone(), done.clone());
        let handler = ProfileGetBrowserExtensionsCompletedHandler::create(Box::new(move |_, list| {
            let mut keep = None;
            let mut stale = Vec::new();
            if let Some(list) = list {
                let mut count = 0u32;
                unsafe {
                    let _ = list.Count(&mut count);
                    for i in 0..count {
                        let Ok(ext) = list.GetValueAtIndex(i) else { continue };
                        let mut id = PWSTR::null();
                        let id = ext.Id(&mut id).map(|_| take_pwstr(id)).unwrap_or_default();
                        if Some(&id) == wanted.as_ref() { keep = Some(ext) } else { stale.push(ext) }
                    }
                }
            }
            // Ce qui reste à faire une fois les anciennes extensions retirées.
            let (p2, d2, f2) = (p.clone(), d.clone(), folder.clone());
            let next = move || match keep {
                Some(ext) => unsafe {
                    let mut on = windows::core::BOOL::default();
                    let _ = ext.IsEnabled(&mut on);
                    if on.as_bool() {
                        return finish(&d2);
                    }
                    let d3 = d2.clone();
                    let enabled = BrowserExtensionEnableCompletedHandler::create(Box::new(move |_| {
                        finish(&d3);
                        Ok(())
                    }));
                    if ext.Enable(true, &enabled).is_err() {
                        finish(&d2);
                    }
                },
                None => add(&p2, f2.as_ref(), &d2),
            };
            if stale.is_empty() {
                next();
                return Ok(());
            }
            let pending = Arc::new(AtomicUsize::new(stale.len()));
            let next = Arc::new(Mutex::new(Some(next)));
            for ext in stale {
                let (pending, next) = (pending.clone(), next.clone());
                let removed = BrowserExtensionRemoveCompletedHandler::create(Box::new(move |_| {
                    if pending.fetch_sub(1, Ordering::SeqCst) == 1 {
                        if let Some(f) = next.lock().unwrap().take() {
                            f();
                        }
                    }
                    Ok(())
                }));
                unsafe {
                    let _ = ext.Remove(&removed);
                }
            }
            Ok(())
        }));
        unsafe {
            if profile.GetBrowserExtensions(&handler).is_err() {
                finish(&done);
            }
        }
    }
}

/// WebKitGTK ne charge pas d'extension : le mod y est posé à la main, sous la
/// forme de scripts utilisateur exécutés avant le code de Discord — exactement
/// ce que l'extension fait dans Chrome.
#[cfg(not(windows))]
mod unix {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::path::PathBuf;

    use webkit2gtk::{UserContentInjectedFrames, UserContentManagerExt, UserScript, UserScriptInjectionTime, WebView, WebViewExt};

    thread_local! {
        /// Les scripts que Plume a posés sur chaque fenêtre, pour les retirer au
        /// changement de mod : `remove_all_scripts` emporterait aussi ceux de
        /// Tauri (l'IPC et tout `inject/`). Les objets GTK ne quittent pas le
        /// fil de l'interface, d'où le `thread_local`.
        static ADDED: RefCell<HashMap<String, Vec<UserScript>>> = RefCell::new(HashMap::new());
    }

    /// Le mod ne s'applique qu'aux pages de Discord, comme le `matches` du
    /// manifeste. Cadre du haut seulement : l'extension vise aussi les
    /// sous-cadres, mais y reparser huit mégaoctets pour chaque captcha ou
    /// activité intégrée ne sert à rien.
    const MATCHES: [&str; 2] = ["*://discord.com/*", "*://*.discord.com/*"];

    fn add(manager: &webkit2gtk::UserContentManager, label: &str, source: &str) {
        let script = UserScript::new(
            source,
            UserContentInjectedFrames::TopFrame,
            UserScriptInjectionTime::Start,
            &MATCHES,
            &[],
        );
        manager.add_script(&script);
        ADDED.with(|a| a.borrow_mut().entry(label.to_string()).or_default().push(script));
    }

    pub fn install_mod(view: &WebView, label: &str, folder: Option<PathBuf>) {
        let Some(manager) = view.user_content_manager() else { return };
        ADDED.with(|a| {
            if let Some(old) = a.borrow_mut().remove(label) {
                for script in old {
                    manager.remove_script(&script);
                }
            }
        });
        let Some(folder) = folder else { return };
        let Some(bundle) = crate::mods::bundle(&folder) else {
            eprintln!("[plume] mod : bundle introuvable dans {}", folder.display());
            return;
        };
        let source = match std::fs::read_to_string(folder.join(&bundle.script)) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[plume] mod : {e}");
                return;
            }
        };
        // Le mod réclame sa feuille de style et l'éditeur QuickCSS par leur
        // adresse ; le serveur local les lui donne (modserver.rs).
        let base = crate::modserver::serve(folder).unwrap_or_default();
        let json = |v: &str| serde_json::to_string(v).unwrap_or_else(|_| "\"\"".into());
        // Ce que `content.js` fait dans l'extension.
        let shim = format!(
            "(() => {{ const meta = {{ EXTENSION_VERSION: {}, EXTENSION_BASE_URL: {}, RENDERER_CSS_URL: {} }};
             addEventListener('DOMContentLoaded', () => window.postMessage({{ type: 'vencord:meta', meta }}, '*'), {{ once: true }}); }})();",
            json(&bundle.version),
            json(&base),
            json(&format!("{base}{}", bundle.css)),
        );
        add(&manager, label, &shim);
        add(&manager, label, &source);
    }
}

/// Attend (5 s au plus) que le moteur WebView2 d'un Plume précédent ait fini de
/// se fermer : il garde `EBWebView\lockfile` ouvert en exclusif tant qu'il
/// tourne. Sinon le nouveau Plume se raccroche à ce moteur mourant, et Windows
/// range ses processus à part (« WebView2 Manager ») au lieu de sous Plume.
pub fn wait_previous_engine(app: &tauri::AppHandle) {
    let Ok(dir) = app.path().app_local_data_dir() else { return };
    let lock = dir.join("EBWebView").join("lockfile");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while lock.exists() && std::fs::OpenOptions::new().write(true).open(&lock).is_err() {
        if std::time::Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

pub fn setup_webview(w: &WebviewWindow) {
    #[cfg(not(windows))]
    {
        use webkit2gtk::{SecurityManagerExt, SettingsExt, WebContextExt, WebViewExt};
        let _ = w.with_webview(|wv| {
            let view = wv.inner();
            // Les fichiers du mod sont servis en clair sur la boucle locale
            // (modserver.rs), et WebKit refuse tout contenu en clair dans une
            // page https. Chrome et Firefox font une exception pour
            // 127.0.0.1, considéré comme sûr par nature ; WebKitGTK ne la fait
            // pas, on la lui déclare. L'ouverture reste étroite : la politique
            // de sécurité de Discord, elle, n'autorise qu'une seule adresse en
            // clair, `http://127.0.0.1:*`, et rien d'autre.
            if let Some(manager) = view.web_context().and_then(|c| c.security_manager()) {
                manager.register_uri_scheme_as_secure("http");
            }
            let Some(settings) = view.settings() else { return };
            // WebKitGTK livre le moteur vocal éteint : sans ça, la page n'a ni
            // getUserMedia ni RTCPeerConnection, donc ni appel ni stéréo.
            settings.set_enable_media_stream(true);
            settings.set_enable_webrtc(true);
            settings.set_enable_mediasource(true);
            // Pas de moteur de débogage distant sous WebKitGTK : en build de
            // développement, la console de la page part sur la sortie standard.
            #[cfg(debug_assertions)]
            settings.set_enable_write_console_messages_to_stdout(true);
        });
    }
    #[cfg(windows)]
    let _ = w.with_webview(|wv| unsafe {
        if let Ok(core) = wv.controller().CoreWebView2() {
            win::block_trackers(&core, wv.environment());
        }
    });
}

/// Installe le mod (extension) puis appelle `then` ; sans WebView2, appelle
/// `then` directement.
pub fn install_extension(w: &WebviewWindow, folder: Option<PathBuf>, then: impl FnOnce() + Send + 'static) {
    #[cfg(windows)]
    {
        let then: Box<dyn FnOnce() + Send> = Box::new(then);
        let _ = w.with_webview(move |wv| unsafe {
            match wv.controller().CoreWebView2() {
                Ok(core) => win::install_extension(&core, folder, then),
                Err(_) => then(),
            }
        });
    }
    #[cfg(not(windows))]
    {
        let label = w.label().to_string();
        let app = w.app_handle().clone();
        let then: Box<dyn FnOnce() + Send> = Box::new(then);
        let _ = w.with_webview(move |wv| {
            unix::install_mod(&wv.inner(), &label, folder);
            // `then` navigue et écrit dans la page. Le faire ici, encore à
            // l'intérieur du message `with_webview`, bloque le fil principal :
            // l'évaluation de wry attend un verrou que la boucle GTK tient
            // encore. On remet donc le rappel en file depuis un autre fil, pour
            // qu'il soit traité au tour suivant de la boucle.
            std::thread::spawn(move || {
                let _ = app.run_on_main_thread(then);
            });
        });
    }
}

/// Fenêtre cachée dans la zone de notification : plus aucun affichage à
/// calculer (animations, GIF, vidéos), et les minuteries de la page ralentissent
/// comme dans un onglet en arrière-plan. Le son et le vocal continuent.
pub fn set_rendering(w: &WebviewWindow, visible: bool) {
    #[cfg(windows)]
    let _ = w.with_webview(move |wv| unsafe {
        let _ = wv.controller().SetIsVisible(visible);
    });
    #[cfg(not(windows))]
    let _ = (w, visible);
}

/// Ferme la fenêtre quand la page appelle `window.close()`. wry ne détruit
/// alors que le conteneur du webview : il resterait une fenêtre Plume vide, et
/// c'est ainsi que Discord referme un lecteur réduit (popout.rs).
pub fn close_window_on_page_close(w: &WebviewWindow) {
    #[cfg(windows)]
    {
        let target = w.clone();
        let _ = w.with_webview(move |wv| unsafe {
            if let Ok(core) = wv.controller().CoreWebView2() {
                win::on_page_close(
                    &core,
                    Box::new(move || {
                        // Fermer depuis le gestionnaire WebView2 serait
                        // réentrant : on repasse par la boucle d'événements.
                        let w = target.clone();
                        std::thread::spawn(move || {
                            let app = w.app_handle().clone();
                            let _ = app.run_on_main_thread(move || {
                                let _ = w.close();
                            });
                        });
                    }),
                );
            }
        });
    }
    #[cfg(not(windows))]
    let _ = w;
}

/// Fenêtre cachée dans la zone de notification : WebView2 libère ce qu'il peut.
pub fn set_memory_low(w: &WebviewWindow, low: bool) {
    #[cfg(windows)]
    let _ = w.with_webview(move |wv| unsafe {
        if let Ok(core) = wv.controller().CoreWebView2() {
            win::memory_level(&core, low);
        }
    });
}
