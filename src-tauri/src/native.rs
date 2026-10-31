//! Réglages WebView2 que la page ne peut pas faire seule (Windows).

use std::path::PathBuf;

use tauri::WebviewWindow;

#[cfg(windows)]
mod win {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use webview2_com::Microsoft::Web::WebView2::Win32::*;
    use webview2_com::{
        take_pwstr, BrowserExtensionEnableCompletedHandler, BrowserExtensionRemoveCompletedHandler, PermissionRequestedEventHandler,
        ProfileAddBrowserExtensionCompletedHandler, ProfileGetBrowserExtensionsCompletedHandler, WebResourceRequestedEventHandler,
    };
    use windows::core::{Interface, HSTRING, PWSTR};

    /// Discord demande micro, caméra, notifications, presse-papiers : on accorde
    /// sans fenêtre de confirmation, mais seulement à Discord.
    pub fn allow_permissions(core: &ICoreWebView2) {
        let handler = PermissionRequestedEventHandler::create(Box::new(|_, args| {
            let Some(args) = args else { return Ok(()) };
            unsafe {
                let mut uri = PWSTR::null();
                args.Uri(&mut uri)?;
                let uri = take_pwstr(uri);
                let mut kind = COREWEBVIEW2_PERMISSION_KIND::default();
                args.PermissionKind(&mut kind)?;
                let allowed = [
                    COREWEBVIEW2_PERMISSION_KIND_MICROPHONE,
                    COREWEBVIEW2_PERMISSION_KIND_CAMERA,
                    COREWEBVIEW2_PERMISSION_KIND_NOTIFICATIONS,
                    COREWEBVIEW2_PERMISSION_KIND_CLIPBOARD_READ,
                    COREWEBVIEW2_PERMISSION_KIND_AUTOPLAY,
                ];
                let discord = tauri::Url::parse(&uri).map(|u| crate::is_discord(&u)).unwrap_or(false);
                if discord && allowed.contains(&kind) {
                    args.SetState(COREWEBVIEW2_PERMISSION_STATE_ALLOW)?;
                }
            }
            Ok(())
        }));
        let mut token = 0i64;
        unsafe {
            let _ = core.add_PermissionRequested(&handler, &mut token);
        }
    }

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

/// Attend (5 s au plus) que le moteur WebView2 d'un Plume précédent ait fini de
/// se fermer : il garde `EBWebView\lockfile` ouvert en exclusif tant qu'il
/// tourne. Sinon le nouveau Plume se raccroche à ce moteur mourant, et Windows
/// range ses processus à part (« WebView2 Manager ») au lieu de sous Plume.
pub fn wait_previous_engine(app: &tauri::AppHandle) {
    use tauri::Manager;
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
    #[cfg(windows)]
    let _ = w.with_webview(|wv| unsafe {
        if let Ok(core) = wv.controller().CoreWebView2() {
            win::allow_permissions(&core);
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
        let _ = (w, folder);
        then();
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

/// Fenêtre cachée dans la zone de notification : WebView2 libère ce qu'il peut.
pub fn set_memory_low(w: &WebviewWindow, low: bool) {
    #[cfg(windows)]
    let _ = w.with_webview(move |wv| unsafe {
        if let Ok(core) = wv.controller().CoreWebView2() {
            win::memory_level(&core, low);
        }
    });
}
