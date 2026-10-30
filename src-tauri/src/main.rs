#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod mods;
mod native;
mod ptt;
mod settings;
mod update;

use std::sync::Mutex;

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::webview::NewWindowResponse;
use tauri::window::Color;
use tauri::{AppHandle, Manager, Url, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_window_state::StateFlags;

use config::Config;
use mods::ClientMod;

pub(crate) const DISCORD_URL: &str = "https://discord.com/app";
pub const MAIN: &str = "main";

/// Hôtes que la fenêtre a le droit d'afficher. Tout le reste part dans le navigateur.
/// Pages de Plume lui-même (l'écran de démarrage `splash.html`).
pub(crate) const SPLASH_URL: &str = "http://tauri.localhost/splash.html";

fn is_local(url: &Url) -> bool {
    url.host_str() == Some("tauri.localhost") || url.scheme() == "tauri"
}

/// Texte d'étape sur l'écran de démarrage (sans effet une fois sur Discord).
pub(crate) fn splash_status(w: &WebviewWindow, text: &str) {
    let text = serde_json::to_string(text).unwrap();
    let _ = w.eval(format!("window.plumeSplash && window.plumeSplash.status({text})"));
}

fn is_discord(url: &Url) -> bool {
    match url.scheme() {
        "about" | "blob" | "data" => return true,
        "https" => {}
        _ => return false,
    }
    let host = url.host_str().unwrap_or("");
    ["discord.com", "discordapp.com", "discord.gift", "discord.new"]
        .iter()
        .any(|d| host == *d || host.ends_with(&format!(".{d}")))
}

fn open_external(app: &AppHandle, url: &str) {
    if url.starts_with("http://") || url.starts_with("https://") {
        let _ = app.opener().open_url(url, None::<&str>);
    }
}

#[tauri::command]
fn notify(app: AppHandle, title: String, body: String) {
    let _ = app.notification().builder().title(title).body(body).show();
}

#[tauri::command]
fn open_url(app: AppHandle, url: String) {
    open_external(&app, &url);
}

#[tauri::command]
fn win_minimize(window: WebviewWindow) {
    let _ = window.minimize();
}

#[tauri::command]
fn win_toggle_maximize(window: WebviewWindow) {
    if window.is_maximized().unwrap_or(false) {
        let _ = window.unmaximize();
    } else {
        let _ = window.maximize();
    }
}

#[tauri::command]
fn win_close(window: WebviewWindow) {
    hide_to_tray(&window);
}

/// Fermer = réduire dans la zone de notification, comme Discord.
fn hide_to_tray(window: &WebviewWindow) {
    let _ = window.hide();
    native::set_memory_low(window, true);
}

#[tauri::command]
fn ptt_keys(keys: Vec<i32>, state: tauri::State<ptt::PttKeys>) {
    state.set(keys);
}

pub(crate) fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MAIN) {
        native::set_memory_low(&w, false);
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Scripts injectés avant le code de Discord, à chaque chargement de page.
fn init_scripts(cfg: &Config) -> Vec<String> {
    vec![
        format!("window.__PLUME__ = {};", serde_json::to_string(cfg).unwrap()),
        include_str!("../../inject/shim.js").to_string(),
        include_str!("../../inject/stereo.js").to_string(),
        include_str!("../../inject/ptt.js").to_string(),
        format!("window.__PLUME_THEME_CSS__ = {};", serde_json::to_string(include_str!("../../inject/theme.css")).unwrap()),
        include_str!("../../inject/theme.js").to_string(),
        include_str!("../../inject/brand.js").to_string(),
        include_str!("../../inject/titlebar.js").to_string(),
        include_str!("../../inject/panel.js").to_string(),
    ]
}

fn build_main(app: &AppHandle, cfg: &Config) -> tauri::Result<WebviewWindow> {
    let nav_app = app.clone();
    let popup_app = app.clone();
    // Page vide d'abord : Discord ne se charge qu'une fois le mod installé.
    let mut builder = WebviewWindowBuilder::new(app, MAIN, WebviewUrl::App("splash.html".into()))
        .title("Plume")
        .background_color(Color(0x0e, 0x0c, 0x16, 0xff))
        .browser_extensions_enabled(true)
        .additional_browser_args(&cfg.memory.browser_args())
        .inner_size(1280.0, 800.0)
        .min_inner_size(940.0, 500.0)
        .visible(false)
        // Pas de cadre Windows : barre de titre intégrée (inject/titlebar.js).
        .decorations(false)
        .disable_drag_drop_handler()
        .on_navigation(move |url| {
            if is_discord(url) || is_local(url) {
                return true;
            }
            open_external(&nav_app, url.as_str());
            false
        })
        .on_new_window(move |url, _| {
            open_external(&popup_app, url.as_str());
            NewWindowResponse::Deny
        });
    for s in init_scripts(cfg) {
        builder = builder.initialization_script(s);
    }
    let w = builder.build()?;
    native::setup_webview(&w);
    load_discord(app, &w, cfg.client_mod);
    Ok(w)
}

/// Prépare le mod (téléchargement au besoin, hors du fil de l'interface),
/// l'installe dans WebView2, puis charge Discord.
fn load_discord(app: &AppHandle, w: &WebviewWindow, m: ClientMod) {
    let (app, w) = (app.clone(), w.clone());
    std::thread::spawn(move || {
        // Laisse l'écran de démarrage se charger avant d'y écrire.
        std::thread::sleep(std::time::Duration::from_millis(300));
        if m != ClientMod::None {
            splash_status(&w, &format!("Chargement de {}", m.label()));
        }
        let folder = mods::prepare(&app, m);
        let _ = app.run_on_main_thread(move || {
            let target = w.clone();
            native::install_extension(&w, folder, move || {
                splash_status(&target, "Connexion à Discord");
                let _ = target.navigate(DISCORD_URL.parse().unwrap());
            });
        });
    });
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let cfg = settings::current(app);
    let show = MenuItem::with_id(app, "show", "Ouvrir Plume", true, None::<&str>)?;
    let prefs = MenuItem::with_id(app, "settings", "Paramètres Plume…", true, None::<&str>)?;
    let stereo = CheckMenuItem::with_id(app, "stereo", "Stéréo micro", true, cfg.audio.stereo, None::<&str>)?;
    let mod_items = ClientMod::ALL
        .iter()
        .map(|m| CheckMenuItem::with_id(app, format!("mod:{}", m.id()), m.label(), true, cfg.client_mod == *m, None::<&str>))
        .collect::<tauri::Result<Vec<_>>>()?;
    let mod_refs: Vec<&dyn tauri::menu::IsMenuItem<_>> = mod_items.iter().map(|i| i as _).collect();
    let mod_menu = Submenu::with_items(app, "Mod", true, &mod_refs)?;
    let reload = MenuItem::with_id(app, "reload", "Recharger", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quitter", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&show, &prefs, &sep, &stereo, &mod_menu, &reload, &sep2, &quit])?;
    app.manage(settings::TrayItems {
        stereo,
        mods: ClientMod::ALL.into_iter().zip(mod_items).collect(),
    });

    TrayIconBuilder::with_id("tray")
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip("Plume")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(tray.app_handle());
            }
        })
        .on_menu_event(|app, event| {
            let id = event.id.as_ref();
            let mut cfg = settings::current(app);
            match id {
                "show" => return show_main(app),
                "settings" => return settings::open(app),
                "reload" => return restart_main(app),
                "quit" => return app.exit(0),
                "stereo" => cfg.audio.stereo = !cfg.audio.stereo,
                _ => match ClientMod::ALL.into_iter().find(|m| id == format!("mod:{}", m.id())) {
                    Some(m) => cfg.client_mod = m,
                    None => return,
                },
            }
            settings::apply(app, cfg);
        })
        .build(app)?;
    Ok(())
}

/// Plume quitte le premier plan (un jeu, par exemple) : après 20 s, WebView2
/// libère ce qu'il peut. Revenir dessus rétablit le niveau normal.
fn background_trim(app: &AppHandle, focused: bool) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static GENERATION: AtomicU64 = AtomicU64::new(0);
    let gen = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let Some(w) = app.get_webview_window(MAIN) else { return };
    if focused {
        native::set_memory_low(&w, false);
        return;
    }
    if !settings::current(app).trim_in_background {
        return;
    }
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(20));
        if GENERATION.load(Ordering::SeqCst) == gen {
            native::set_memory_low(&w, true);
        }
    });
}

pub(crate) fn restart_main(app: &AppHandle) {
    let app = app.clone();
    // Hors du gestionnaire d'événement, sinon la destruction bloque la boucle.
    std::thread::spawn(move || {
        let mut visible = true;
        if let Some(w) = app.get_webview_window(MAIN) {
            visible = w.is_visible().unwrap_or(true);
            let _ = w.destroy();
        }
        // La destruction est asynchrone : attendre que le libellé soit libre.
        for _ in 0..100 {
            if app.get_webview_window(MAIN).is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let cfg = settings::current(&app);
        let a = app.clone();
        let _ = app.run_on_main_thread(move || {
            if let Ok(w) = build_main(&a, &cfg) {
                if visible {
                    let _ = w.show();
                } else {
                    native::set_memory_low(&w, true);
                }
            }
        });
    });
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| show_main(app)))
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(StateFlags::all() & !StateFlags::VISIBLE)
                .build(),
        )
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            notify,
            open_url,
            ptt_keys,
            win_minimize,
            win_toggle_maximize,
            win_close,
            settings::get_config,
            settings::set_config,
            settings::audio_config,
            settings::rnnoise_worklet,
            settings::rnnoise_wasm,
            update::update_check,
            update::update_install
        ])
        .manage(ptt::PttKeys::default())
        .setup(|app| {
            let handle = app.handle().clone();
            let cfg = Config::load(&handle);
            app.manage(Mutex::new(cfg.clone()));
            native::wait_previous_engine(&handle);
            let main = build_main(&handle, &cfg)?;
            // --minimized : démarrage directement dans la zone de notification.
            if std::env::args().any(|a| a == "--minimized") {
                native::set_memory_low(&main, true);
            } else {
                main.show()?;
            }
            build_tray(&handle)?;
            ptt::spawn(handle.clone());
            mods::refresh_in_background(handle.clone(), cfg.client_mod);
            if cfg.auto_update {
                update::check_on_startup(handle);
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::Focused(focused) = event {
                if window.label() == MAIN {
                    background_trim(window.app_handle(), *focused);
                }
            }
            // Alt+F4 et la fermeture depuis la barre des tâches passent aussi ici.
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == MAIN {
                    api.prevent_close();
                    if let Some(w) = window.app_handle().get_webview_window(MAIN) {
                        hide_to_tray(&w);
                    }
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("échec du démarrage de Plume");
}
