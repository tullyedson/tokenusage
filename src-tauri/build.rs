fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "bootstrap",
            "current_page",
            "refresh_usage",
            "save_provider",
            "add_account",
            "save_routing",
            "model_library",
            "save_model_pools",
            "discover_models",
            "sign_in",
            "forget_provider",
            "save_preferences",
            "autostart_enabled",
            "set_autostart",
        ]),
    ))
    .expect("Could not build the app permission manifest");
}
