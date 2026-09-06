fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "bootstrap",
            "current_page",
            "refresh_usage",
            "save_provider",
            "sign_in",
            "forget_provider",
            "save_preferences",
            "autostart_enabled",
            "set_autostart",
        ]),
    ))
    .expect("Could not build the app permission manifest");
}
