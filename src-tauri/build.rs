fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&["notify", "open_url", "ptt_keys", "win_minimize", "win_toggle_maximize", "win_close", "get_config", "set_config", "audio_config", "rnnoise_worklet", "rnnoise_wasm", "update_check", "update_pending", "update_install", "wordbomb_type", "recorder_pick_folder", "recorder_save", "recorder_stream_init", "recorder_stream_append", "recorder_stream_finalize", "set_badge", "save_url", "capture_sources", "capture_start", "capture_stop", "set_ram_position", "plugins_cleared", "ram_usage", "community_sync", "sleep_busy", "plugins_backup_save", "plugins_backup_restore"]),
    ))
    .expect("échec de tauri-build");
}
