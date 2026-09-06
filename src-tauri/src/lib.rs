mod browser;
mod codex;
pub mod model;
mod persistence;
pub mod providers;
mod service;

use service::{Bootstrap, UsageService};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State,
};
use tauri_plugin_autostart::ManagerExt;

struct Page(Mutex<String>);

fn show(app: &AppHandle, page: &str) {
    if let Some(state) = app.try_state::<Page>() {
        if let Ok(mut selected) = state.0.lock() {
            *selected = page.into();
        }
    }
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        let _ = app.emit("show-page", page);
    }
}

#[tauri::command]
async fn bootstrap(service: State<'_, Arc<UsageService>>) -> Result<Bootstrap, String> {
    Ok(service.bootstrap().await)
}
#[tauri::command]
fn current_page(page: State<'_, Page>) -> String {
    page.0
        .lock()
        .map(|p| p.clone())
        .unwrap_or_else(|_| "usage".into())
}
#[tauri::command]
async fn refresh_usage(
    service: State<'_, Arc<UsageService>>,
    provider_id: Option<String>,
) -> Result<(), String> {
    service.refresh(provider_id.as_deref()).await;
    Ok(())
}
#[tauri::command]
async fn save_provider(
    service: State<'_, Arc<UsageService>>,
    provider_id: String,
    enabled: bool,
    fields: BTreeMap<String, String>,
) -> Result<(), String> {
    service.save_provider(&provider_id, enabled, fields).await
}
#[tauri::command]
async fn sign_in(
    service: State<'_, Arc<UsageService>>,
    provider_id: String,
) -> Result<String, String> {
    service.sign_in(&provider_id).await
}
#[tauri::command]
async fn forget_provider(
    service: State<'_, Arc<UsageService>>,
    provider_id: String,
) -> Result<(), String> {
    service.forget(&provider_id).await
}
#[tauri::command]
async fn save_preferences(
    service: State<'_, Arc<UsageService>>,
    refresh_minutes: u64,
) -> Result<(), String> {
    service.save_interval(refresh_minutes).await
}
#[tauri::command]
fn autostart_enabled(app: AppHandle) -> Result<bool, String> {
    app.autolaunch()
        .is_enabled()
        .map_err(|_| "Could not read startup settings.".into())
}
#[tauri::command]
fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    (if enabled {
        app.autolaunch().enable()
    } else {
        app.autolaunch().disable()
    })
    .map_err(|_| "Could not update Windows startup settings.".into())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            show(app, "usage")
        }))
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .app_name("AI Usage")
                .arg("--tray")
                .build(),
        )
        .manage(Page(Mutex::new("usage".into())))
        .invoke_handler(tauri::generate_handler![
            bootstrap,
            current_page,
            refresh_usage,
            save_provider,
            sign_in,
            forget_provider,
            save_preferences,
            autostart_enabled,
            set_autostart
        ])
        .setup(|app| {
            let root = app.path().app_local_data_dir()?;
            std::fs::create_dir_all(&root)?;
            let service = Arc::new(UsageService::new(app.handle().clone(), root));
            app.manage(service.clone());
            let usage = MenuItem::with_id(app, "usage", "Show usage", true, None::<&str>)?;
            let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
            let exit = MenuItem::with_id(app, "exit", "Exit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&usage, &settings, &exit])?;
            let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/32x32.png"))?;
            TrayIconBuilder::with_id("usage-tray")
                .icon(icon)
                .tooltip("AI Usage")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "usage" => show(app, "usage"),
                    "settings" => show(app, "settings"),
                    "exit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| match event {
                    TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    }
                    | TrayIconEvent::DoubleClick {
                        button: MouseButton::Left,
                        ..
                    } => show(tray.app_handle(), "usage"),
                    _ => {}
                })
                .build(app)?;
            if let Some(window) = app.get_webview_window("main") {
                let close = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = close.hide();
                    }
                });
            }
            if !std::env::args().any(|a| a == "--tray") {
                show(app.handle(), "usage");
            }
            tauri::async_runtime::spawn(async move {
                service.refresh(None).await;
                let mut elapsed = 0;
                loop {
                    tokio::time::sleep(Duration::from_secs(15)).await;
                    elapsed += 15;
                    if elapsed >= service.interval().await * 60 {
                        service.refresh(None).await;
                        elapsed = 0;
                    }
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("AI Usage could not start");
}
