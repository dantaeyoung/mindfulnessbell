// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager,
};

// Global state for bell enabled status (will be properly managed later)
static BELL_ENABLED: AtomicBool = AtomicBool::new(true);

/// Load the appropriate tray icon based on enabled state
fn load_tray_icon(enabled: bool) -> Image<'static> {
    let icon_bytes: &[u8] = if enabled {
        include_bytes!("../icons/tray-bell.png")
    } else {
        include_bytes!("../icons/tray-bell-disabled.png")
    };
    Image::from_bytes(icon_bytes).expect("Failed to load tray icon")
}

/// Update the tray icon based on current enabled state
#[allow(dead_code)]
fn update_tray_icon(app: &AppHandle, enabled: bool) {
    BELL_ENABLED.store(enabled, Ordering::SeqCst);
    if let Some(tray) = app.tray_by_id("main-tray") {
        let icon = load_tray_icon(enabled);
        let _ = tray.set_icon(Some(icon));
    }
}

/// Open or focus the settings window
fn open_settings_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.show();
        let _ = window.set_focus();
    } else {
        // Create settings window if it doesn't exist
        let _settings_window = tauri::WebviewWindowBuilder::new(
            app,
            "settings",
            tauri::WebviewUrl::App("index.html".into()),
        )
        .title("Mindfulness Bell Settings")
        .inner_size(400.0, 500.0)
        .resizable(false)
        .build();
    }
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            // Create menu items
            let settings =
                MenuItem::with_id(app, "settings", "Settings...", true, None::<&str>)?;
            let separator1 = PredefinedMenuItem::separator(app)?;
            let test_bell =
                MenuItem::with_id(app, "test_bell", "Test Bell", true, None::<&str>)?;
            let separator2 = PredefinedMenuItem::separator(app)?;
            let quit =
                MenuItem::with_id(app, "quit", "Quit Mindfulness Bell", true, None::<&str>)?;

            // Build the tray menu
            let menu = Menu::with_items(
                app,
                &[&settings, &separator1, &test_bell, &separator2, &quit],
            )?;

            // Load the initial tray icon
            let icon = load_tray_icon(BELL_ENABLED.load(Ordering::SeqCst));

            // Create the tray icon with menu
            // Left-click shows the menu (consistent macOS behavior)
            let _tray = TrayIconBuilder::with_id("main-tray")
                .icon(icon)
                .icon_as_template(true)
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => {
                        app.exit(0);
                    }
                    "settings" => {
                        open_settings_window(app);
                    }
                    "test_bell" => {
                        // Placeholder - will be wired up in a later task
                        println!("Test bell triggered!");
                    }
                    _ => {}
                })
                .build(app)?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
