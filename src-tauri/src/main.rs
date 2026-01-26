// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use chrono::{Local, Timelike};
use rodio::{Decoder, OutputStream, Sink};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufReader, Cursor};
use std::path::Path;
use std::sync::{
    atomic::{AtomicU64, AtomicUsize, Ordering},
    Mutex,
};
use std::thread;
use std::time::Duration;
use tauri::{
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Listener, Manager, PhysicalPosition, PhysicalSize,
};
use tauri_plugin_store::StoreExt;

// Default bell sound embedded in the binary
const DEFAULT_BELL_SOUND: &[u8] = include_bytes!("../sounds/bell.aiff");

// Counter for unique overlay window IDs
static OVERLAY_COUNTER: AtomicUsize = AtomicUsize::new(0);

// Scheduler version - incremented when settings change to restart scheduler
static SCHEDULER_VERSION: AtomicU64 = AtomicU64::new(0);

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
    store.set("timingMode", settings.timing_mode.clone());
    store.set("intervalMinutes", settings.interval_minutes);
    store.set("opacity", settings.opacity);
    store.set("durationSeconds", settings.duration_seconds);
    store.set("volume", settings.volume);
    store.set("customSoundPath", settings.custom_sound_path.clone());
    store.save().map_err(|e| e.to_string())?;

    // Restart the scheduler to pick up new settings
    start_scheduler(app.clone());

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

/// Calculate seconds until the next clock-aligned bell trigger
/// For clock-aligned mode, we trigger at :00, :15, :30, :45 (based on interval)
fn seconds_until_next_aligned_trigger(interval_minutes: i32) -> u64 {
    let now = Local::now();
    let current_minute = now.minute() as i32;
    let current_second = now.second() as i32;

    // Find the next trigger minute based on interval
    // With 15-min interval: triggers at 0, 15, 30, 45
    // With 30-min interval: triggers at 0, 30
    // With 60-min interval: triggers at 0 only
    let next_trigger_minute = if interval_minutes >= 60 {
        // Next hour
        60
    } else {
        // Find next aligned minute
        let mut next = ((current_minute / interval_minutes) + 1) * interval_minutes;
        if next > 59 {
            next = 60; // Will wrap to next hour
        }
        next
    };

    // Calculate seconds until next trigger
    let minutes_to_wait = next_trigger_minute - current_minute;
    let seconds_to_wait = (minutes_to_wait * 60) - current_second;

    // Ensure we always wait at least 1 second
    if seconds_to_wait <= 0 {
        // If we're exactly on the trigger minute:second, wait for the next interval
        (interval_minutes * 60) as u64
    } else {
        seconds_to_wait as u64
    }
}

/// Check if bell is enabled from settings state
fn is_bell_enabled(app: &AppHandle) -> bool {
    let state = app.state::<SettingsState>();
    let result = match state.0.lock() {
        Ok(settings) => settings.is_enabled,
        Err(_) => false,
    };
    result
}

/// Start the bell scheduler
/// The scheduler runs in a background thread and triggers the bell at configured intervals
fn start_scheduler(app: AppHandle) {
    // Increment version to invalidate any existing scheduler
    let version = SCHEDULER_VERSION.fetch_add(1, Ordering::SeqCst) + 1;

    thread::spawn(move || {
        loop {
            // Check if this scheduler is still valid
            if SCHEDULER_VERSION.load(Ordering::SeqCst) != version {
                // A new scheduler has been started, exit this one
                return;
            }

            // Check if bell is enabled (read from settings state)
            if !is_bell_enabled(&app) {
                // Bell is disabled, sleep and check again
                thread::sleep(Duration::from_secs(1));
                continue;
            }

            // Get current settings
            let (timing_mode, interval_minutes) = {
                let state = app.state::<SettingsState>();
                let settings = match state.0.lock() {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("Failed to lock settings: {}", e);
                        thread::sleep(Duration::from_secs(1));
                        continue;
                    }
                };
                (settings.timing_mode.clone(), settings.interval_minutes)
            };

            // Calculate sleep duration based on timing mode
            let sleep_seconds = if timing_mode == "clock-aligned" {
                seconds_until_next_aligned_trigger(interval_minutes)
            } else {
                // Fixed interval mode: just use the interval
                (interval_minutes * 60) as u64
            };

            // Sleep in small increments to allow for settings changes and version checks
            let sleep_end = std::time::Instant::now() + Duration::from_secs(sleep_seconds);

            while std::time::Instant::now() < sleep_end {
                // Check if scheduler version changed
                if SCHEDULER_VERSION.load(Ordering::SeqCst) != version {
                    return;
                }

                // Check if bell was disabled
                if !is_bell_enabled(&app) {
                    break;
                }

                // Sleep for 1 second at a time
                thread::sleep(Duration::from_secs(1));
            }

            // Double-check conditions before triggering
            if SCHEDULER_VERSION.load(Ordering::SeqCst) != version {
                return;
            }

            if !is_bell_enabled(&app) {
                continue;
            }

            // Trigger the bell by emitting an event
            let _ = app.emit("trigger-bell", ());
        }
    });
}

/// Create overlay windows on all monitors with the given opacity and duration
fn create_overlay_windows(app: &AppHandle, opacity: f64, duration: f64) {
    let monitors = match app.available_monitors() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Failed to get monitors: {}", e);
            return;
        }
    };

    for monitor in monitors {
        let counter = OVERLAY_COUNTER.fetch_add(1, Ordering::SeqCst);
        let window_label = format!("overlay-{}", counter);
        let position = monitor.position();
        let size = monitor.size();
        let url = format!("overlay.html?opacity={}&duration={}", opacity, duration);

        match tauri::WebviewWindowBuilder::new(
            app,
            &window_label,
            tauri::WebviewUrl::App(url.into()),
        )
        .title("")
        .inner_size(size.width as f64, size.height as f64)
        .position(position.x as f64, position.y as f64)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .visible_on_all_workspaces(true)
        .skip_taskbar(true)
        .resizable(false)
        .focused(false)
        .visible(true)
        .build()
        {
            Ok(window) => {
                let _ = window.set_ignore_cursor_events(true);
                let _ = window.set_position(PhysicalPosition::new(position.x, position.y));
                let _ = window.set_size(PhysicalSize::new(size.width, size.height));
                let _ = window.set_visible_on_all_workspaces(true);
            }
            Err(e) => {
                eprintln!("Failed to create overlay window: {}", e);
            }
        }
    }
}

/// Load sound data from the given path, or fall back to default bell sound
fn load_sound_data(custom_path: &str) -> Vec<u8> {
    if !custom_path.is_empty() && Path::new(custom_path).exists() {
        match File::open(custom_path) {
            Ok(file) => {
                let mut reader = BufReader::new(file);
                let mut data = Vec::new();
                if std::io::Read::read_to_end(&mut reader, &mut data).is_ok() {
                    return data;
                }
            }
            Err(e) => {
                eprintln!("Failed to open custom sound file: {}", e);
            }
        }
    }
    DEFAULT_BELL_SOUND.to_vec()
}

/// Trigger the bell: play sound and show overlay
fn trigger_bell(app: &AppHandle) {
    let state = app.state::<SettingsState>();
    let settings = match state.0.lock() {
        Ok(s) => s.clone(),
        Err(e) => {
            eprintln!("Failed to lock settings: {}", e);
            return;
        }
    };

    // Play the bell sound
    let sound_data = load_sound_data(&settings.custom_sound_path);
    if let Err(e) = play_sound_with_volume(sound_data, settings.volume as f32) {
        eprintln!("Failed to play bell sound: {}", e);
    }

    // Show the overlay on all monitors
    create_overlay_windows(app, settings.opacity, settings.duration_seconds);
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
    if let Some(tray) = app.tray_by_id("main-tray") {
        let icon = load_tray_icon(enabled);
        let _ = tray.set_icon(Some(icon));
    }
}

/// Play a sound file with the given volume (0.0 to 1.0)
fn play_sound_with_volume(sound_data: Vec<u8>, volume: f32) -> Result<(), String> {
    thread::spawn(move || {
        let (_stream, stream_handle) = OutputStream::try_default()
            .map_err(|e| format!("Failed to get audio output stream: {}", e))?;

        let sink = Sink::try_new(&stream_handle)
            .map_err(|e| format!("Failed to create audio sink: {}", e))?;

        let cursor = Cursor::new(sound_data);
        let source = Decoder::new(cursor)
            .map_err(|e| format!("Failed to decode audio: {}", e))?;

        sink.set_volume(volume);
        sink.append(source);
        sink.sleep_until_end();

        Ok::<(), String>(())
    });

    Ok(())
}

/// Play the bell sound using current settings
#[tauri::command]
fn play_bell(state: tauri::State<'_, SettingsState>) -> Result<(), String> {
    let settings = state.0.lock().map_err(|e| e.to_string())?;
    let volume = settings.volume as f32;
    let custom_path = settings.custom_sound_path.clone();
    drop(settings); // Release lock before potentially long I/O operation

    let sound_data = load_sound_data(&custom_path);
    play_sound_with_volume(sound_data, volume)
}

/// Show fullscreen overlay on all monitors
#[tauri::command]
fn show_overlay(app: AppHandle, state: tauri::State<'_, SettingsState>) -> Result<(), String> {
    let settings = state.0.lock().map_err(|e| e.to_string())?;
    let opacity = settings.opacity;
    let duration = settings.duration_seconds;
    drop(settings);

    create_overlay_windows(&app, opacity, duration);
    Ok(())
}

/// Close an overlay window (called from JS when animation completes)
#[tauri::command]
fn close_overlay_window(window: tauri::WebviewWindow) -> Result<(), String> {
    window.close().map_err(|e| e.to_string())
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
        .inner_size(400.0, 580.0)
        .resizable(false)
        .build();
    }
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .manage(SettingsState(Mutex::new(Settings::default())))
        .invoke_handler(tauri::generate_handler![get_settings, save_settings, play_bell, show_overlay, close_overlay_window])
        .setup(|app| {
            // Load settings from store on startup
            let loaded_settings = load_settings_from_store(app.handle());

            // Update the managed state with loaded settings
            let state = app.state::<SettingsState>();
            if let Ok(mut settings) = state.0.lock() {
                *settings = loaded_settings.clone();
            }

            // Create menu items
            let settings_menu =
                MenuItem::with_id(app, "settings", "Settings...", true, None::<&str>)?;
            let separator1 = PredefinedMenuItem::separator(app)?;
            let test_bell =
                MenuItem::with_id(app, "test_bell", "Test Bell", true, None::<&str>)?;
            let test_5sec =
                MenuItem::with_id(app, "test_5sec", "Test in 5 seconds", true, None::<&str>)?;
            let separator2 = PredefinedMenuItem::separator(app)?;
            let quit =
                MenuItem::with_id(app, "quit", "Quit Mindfulness Bell", true, None::<&str>)?;

            // Build the tray menu
            let menu = Menu::with_items(
                app,
                &[&settings_menu, &separator1, &test_bell, &test_5sec, &separator2, &quit],
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
                        trigger_bell(app);
                    }
                    "test_5sec" => {
                        // Trigger bell in 5 seconds using event system (like scheduler)
                        let app_handle = app.clone();
                        std::thread::spawn(move || {
                            std::thread::sleep(std::time::Duration::from_secs(5));
                            let _ = app_handle.emit("trigger-bell", ());
                        });
                    }
                    _ => {}
                })
                .build(app)?;

            // Set up event listener for trigger-bell events from the scheduler
            let app_handle = app.handle().clone();
            app.listen("trigger-bell", move |_| {
                trigger_bell(&app_handle);
            });

            // Always start the scheduler - it handles enabled state internally
            start_scheduler(app.handle().clone());

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app_handle, event| {
            // Prevent app from exiting when all windows are closed (we're a tray app)
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                api.prevent_exit();
            }
        });
}
