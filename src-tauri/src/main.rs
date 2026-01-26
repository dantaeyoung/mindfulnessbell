// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use std::sync::{atomic::{AtomicBool, Ordering}, Mutex};
use tauri::{
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager,
};
use tauri_plugin_store::StoreExt;

// Global state for bell enabled status
static BELL_ENABLED: AtomicBool = AtomicBool::new(true);

// Settings store filename
const SETTINGS_FILE: &str = "settings.json";

/// Application settings structure
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub is_enabled: bool,
    pub timing_mode: String,
    pub interval_minutes: i32,
    pub opacity: f64,
    pub duration_seconds: f64,
    pub volume: f64,
    pub custom_sound_path: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            is_enabled: true,
            timing_mode: "clock-aligned".to_string(),
            interval_minutes: 15,
            opacity: 0.8,
            duration_seconds: 3.0,
            volume: 0.7,
            custom_sound_path: String::new(),
        }
    }
}

/// Global settings state wrapped in a Mutex for thread-safe access
struct SettingsState(Mutex<Settings>);

/// Get the current settings
#[tauri::command]
fn get_settings(state: tauri::State<'_, SettingsState>) -> Result<Settings, String> {
    let settings = state.0.lock().map_err(|e| e.to_string())?;
    Ok(settings.clone())
}

/// Save settings and persist to store
#[tauri::command]
fn save_settings(
    app: AppHandle,
    state: tauri::State<'_, SettingsState>,
    settings: Settings,
) -> Result<(), String> {
    // Update in-memory state
    {
        let mut current = state.0.lock().map_err(|e| e.to_string())?;
        *current = settings.clone();
    }

    // Update tray icon if enabled state changed
    let enabled = settings.is_enabled;
    update_tray_icon(&app, enabled);

    // Persist to store
    let store = app.store(SETTINGS_FILE).map_err(|e| e.to_string())?;
    store.set("isEnabled", settings.is_enabled);
    store.set("timingMode", settings.timing_mode);
    store.set("intervalMinutes", settings.interval_minutes);
    store.set("opacity", settings.opacity);
    store.set("durationSeconds", settings.duration_seconds);
    store.set("volume", settings.volume);
    store.set("customSoundPath", settings.custom_sound_path);
    store.save().map_err(|e| e.to_string())?;

    Ok(())
}

/// Load settings from store or return defaults
fn load_settings_from_store(app: &AppHandle) -> Settings {
    let store = match app.store(SETTINGS_FILE) {
        Ok(s) => s,
        Err(_) => return Settings::default(),
    };

    Settings {
        is_enabled: store.get("isEnabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
        timing_mode: store.get("timingMode")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "clock-aligned".to_string()),
        interval_minutes: store.get("intervalMinutes")
            .and_then(|v| v.as_i64())
            .map(|v| v as i32)
            .unwrap_or(15),
        opacity: store.get("opacity")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.8),
        duration_seconds: store.get("durationSeconds")
            .and_then(|v| v.as_f64())
            .unwrap_or(3.0),
        volume: store.get("volume")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.7),
        custom_sound_path: store.get("customSoundPath")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_default(),
    }
}

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
        .plugin(tauri_plugin_store::Builder::default().build())
        .manage(SettingsState(Mutex::new(Settings::default())))
        .invoke_handler(tauri::generate_handler![get_settings, save_settings])
        .setup(|app| {
            // Load settings from store on startup
            let loaded_settings = load_settings_from_store(app.handle());

            // Update the managed state with loaded settings
            let state = app.state::<SettingsState>();
            if let Ok(mut settings) = state.0.lock() {
                *settings = loaded_settings.clone();
            }

            // Update the global enabled flag
            BELL_ENABLED.store(loaded_settings.is_enabled, Ordering::SeqCst);

            // Create menu items
            let settings_menu =
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
                &[&settings_menu, &separator1, &test_bell, &separator2, &quit],
            )?;

            // Load the initial tray icon based on persisted enabled state
            let icon = load_tray_icon(loaded_settings.is_enabled);

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
