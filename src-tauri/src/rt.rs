//! Le moteur web de Plume selon la plateforme.
//!
//! Windows : WebView2, par wry (le runtime par défaut de Tauri).
//! Linux : Chromium (CEF), par tauri-runtime-cef. WebKitGTK n'allait pas :
//! Discord y prend Safari pour un vieux navigateur, son WebRTC passe par
//! GStreamer, et son tas JavaScript pèse le double de celui de V8 (mesuré :
//! ~1,2 Go contre ~600 Mo sous Windows). Dorion a fait le même choix.
//!
//! Tauri ne donne ses types par défaut que pour wry : tout le code passe par
//! les alias d'ici.

#[cfg(target_os = "linux")]
pub type Runtime = tauri_runtime_cef::CefRuntime<tauri::EventLoopMessage>;
#[cfg(not(target_os = "linux"))]
pub type Runtime = tauri::Wry;

pub type AppHandle = tauri::AppHandle<Runtime>;
pub type WebviewWindow = tauri::WebviewWindow<Runtime>;
pub type WebviewWindowBuilder<'a> = tauri::WebviewWindowBuilder<'a, Runtime, AppHandle>;

/// Démarrage de Chromium (Linux). Tout ici doit passer avant Tauri : CEF
/// relance ce même exécutable pour ses sous-processus (rendu, GPU, réseau),
/// qui doivent voir la même configuration.
#[cfg(target_os = "linux")]
pub mod cef {
    use std::sync::OnceLock;

    use tauri_runtime_cef::{CefConfig, DenyReason, PermissionKind};

    use crate::config::MemoryMode;

    /// Identifiant de Plume (tauri.conf.json) : le profil Chromium va dans
    /// `~/.cache/app.plume.desktop/cef`.
    const IDENTIFIER: &str = "app.plume.desktop";
    /// Schéma qui sert le mod à la page (modserver.rs).
    pub const MOD_SCHEME: &str = "plumemod";

    static APP: OnceLock<super::AppHandle> = OnceLock::new();

    /// Rend `true` si ce processus est un sous-processus de Chromium : il a
    /// alors fini son travail et `main` doit s'arrêter là.
    pub fn boot() -> bool {
        // Sans le binaire `chrome-sandbox` installé setuid root, le bac à sable
        // de Chromium ne démarre pas : comme Dorion, on relance sans. Les pages
        // restent isolées par processus (site par processus).
        if std::env::var_os("PLUME_CEF_REEXEC").is_none() && !std::env::args().any(|a| a.starts_with("--type=")) {
            let argv: Vec<_> = std::env::args_os().collect();
            if !argv.iter().any(|a| a == "--no-sandbox") {
                std::env::set_var("PLUME_CEF_REEXEC", "1");
                let exe = std::env::current_exe().unwrap_or_else(|_| argv[0].clone().into());
                let err = std::os::unix::process::CommandExt::exec(std::process::Command::new(exe).args(&argv[1..]).arg("--no-sandbox"));
                eprintln!("[plume] relance de Chromium sans bac à sable impossible : {err}");
                std::process::exit(1);
            }
        }
        tauri_runtime_cef::configure(CefConfig {
            identifier: IDENTIFIER.into(),
            command_line_args: switches(memory_mode()),
            custom_schemes: vec!["tauri".into(), "ipc".into(), "asset".into(), MOD_SCHEME.into()],
            ..Default::default()
        });
        if std::env::args().any(|a| a.starts_with("--type=")) {
            tauri_runtime_cef::run_cef_helper_process();
            return true;
        }
        policies();
        false
    }

    /// À appeler dans `setup` : les règles des popups ont besoin de l'appli.
    pub fn set_app(app: &super::AppHandle) {
        let _ = APP.set(app.clone());
    }

    /// Le mode mémoire, lu directement dans config.json : Tauri n'est pas
    /// encore démarré, et les sous-processus doivent avoir les mêmes réglages.
    fn memory_mode() -> MemoryMode {
        let path = std::env::var_os("XDG_CONFIG_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))
            .map(|d| d.join(IDENTIFIER).join("config.json"));
        path.and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .and_then(|v| serde_json::from_value(v.get("memory")?.clone()).ok())
            .unwrap_or(MemoryMode::Reduced)
    }

    /// Les mêmes réglages que WebView2 sous Windows (config.rs), en
    /// commutateurs Chromium.
    fn switches(mode: MemoryMode) -> Vec<(String, Option<String>)> {
        let mut s: Vec<(String, Option<String>)> = vec![
            // Services de Chromium inutiles ici (comme sous Windows).
            ("disable-features".into(), Some("Translate,OptimizationHints,MediaRouter,BackForwardCache,SpareRendererForSitePerProcess".into())),
            // Partage d'écran par le portail du bureau (PipeWire) : sous X11,
            // Chromium ne verrait que les fenêtres XWayland.
            ("enable-features".into(), Some("WebRTCPipeWireCapturer".into())),
            // Réseau et audio dans le processus principal plutôt qu'à part :
            // ~35 Mo de moins (mesuré), un seul compte connecté par processus.
            ("enable-features".into(), Some("NetworkServiceInProcess2".into())),
            ("disable-features".into(), Some("AudioServiceOutOfProcess".into())),
            ("disable-background-networking".into(), None),
            ("disable-domain-reliability".into(), None),
            ("no-pings".into(), None),
            // La chaîne du micro (WebAudio) doit tourner sans geste préalable.
            ("autoplay-policy".into(), Some("no-user-gesture-required".into())),
        ];
        // Tests seulement : jamais de port de débogage sans cette variable
        // (il donnerait la session Discord à tout programme local).
        if let Ok(port) = std::env::var("PLUME_DEBUG_PORT") {
            s.push(("remote-debugging-port".into(), Some(port)));
        }
        // Jamais `--lite-mode` ni `--jitless` : ils coupent WebAssembly, dont
        // le chiffrement vocal de Discord (DAVE) a besoin.
        let v8 = ("js-flags".to_string(), Some("--optimize-for-size --max-semi-space-size=2".to_string()));
        match mode {
            MemoryMode::Standard => {}
            MemoryMode::Reduced => s.push(v8),
            MemoryMode::Max => {
                s.push(v8);
                s.push(("disable-gpu".into(), None));
                s.push(("disable-gpu-compositing".into(), None));
            }
        }
        s
    }

    fn is_discord_origin(host: &str) -> bool {
        host == "discord.com" || host.ends_with(".discord.com")
    }

    fn policies() {
        // Micro, caméra, écran, notifications : accordés à Discord, sans
        // fenêtre de confirmation (le portail du bureau demande lui-même quoi
        // partager). Tout le reste, et toute autre origine : refusé.
        tauri_runtime_cef::set_permission_policy(|request, responder| {
            let Some(origin) = request.origin.as_ref() else {
                return responder.deny(DenyReason::InvalidOrigin);
            };
            if origin.scheme != "https" || !is_discord_origin(&origin.host) {
                return responder.deny(DenyReason::PolicyDenied);
            }
            let ok = request.kinds.iter().all(|k| {
                matches!(
                    k,
                    PermissionKind::Microphone
                        | PermissionKind::Camera
                        | PermissionKind::ScreenCapture
                        | PermissionKind::Notifications
                        | PermissionKind::CapturedSurfaceControl
                        | PermissionKind::ClipboardRead
                )
            });
            if ok {
                responder.allow();
            } else {
                responder.deny(DenyReason::PolicyDenied);
            }
        });
        // `on_new_window` n'est pas appelé sous CEF : la même règle ici.
        // Fichier joint → téléchargé ; lien externe → navigateur ; fenêtre de
        // Discord (lecteur réduit, appel détaché) → vraie fenêtre Chromium.
        tauri_runtime_cef::set_popup_policy(|req| {
            let Ok(url) = req.url.parse::<tauri::Url>() else { return false };
            let Some(app) = APP.get() else { return false };
            if crate::save::is_attachment(&url) {
                crate::save::download_in_background(app, url);
                return false;
            }
            if crate::is_discord(&url) {
                return true;
            }
            crate::open_external(app, url.as_str());
            false
        });
        // La CSP de Discord bloquerait le mod (ses styles et le schéma qui le
        // sert) : l'extension la retire sous Windows, ici c'est le moteur.
        tauri_runtime_cef::set_bypass_csp_policy(|_| true);
    }
}
