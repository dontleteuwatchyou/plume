fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&["notify", "open_url", "ptt_keys", "win_minimize", "win_toggle_maximize", "win_close"]),
    ))
    .expect("échec de tauri-build");
}
