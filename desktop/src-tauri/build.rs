fn main() {
    // The app's own commands, so a capability can allow exactly these (capabilities/app.json).
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(&["app_version", "update_check", "update_progress", "update_install", "pick_folder", "open_link", "app_ready", "snap_layouts", "notify", "tray_needs", "engine_retry", "open_log"])))
        .expect("tauri build");
}
