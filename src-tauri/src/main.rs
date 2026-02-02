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
const DEFAULT_BELL_SOUND: &[u8] = include_bytes!("../sounds/bell.mp3");

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
    pub clock_times: Vec<i32>,  // Minutes of the hour for clock-aligned mode (0-59)
    pub opacity: f64,
    pub duration_seconds: f64,
    pub bell_start_delay: f64,
    pub volume: f64,
    pub custom_sound_path: String,
    pub pause_media_on_bell: bool,
    pub resume_media_after_bell: bool,
    pub sound_enabled: bool,
    pub visual_enabled: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            is_enabled: true,
            timing_mode: "clock-aligned".to_string(),
            interval_minutes: 15,
            clock_times: vec![0, 15, 30, 45],  // Default: every quarter hour
            opacity: 0.8,
            duration_seconds: 3.0,
            bell_start_delay: 0.5,
            volume: 0.7,
            custom_sound_path: String::new(),
            pause_media_on_bell: false,
            resume_media_after_bell: false,
            sound_enabled: true,
            visual_enabled: true,
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
    store.set("clockTimes", settings.clock_times.clone());
    store.set("opacity", settings.opacity);
    store.set("durationSeconds", settings.duration_seconds);
    store.set("bellStartDelay", settings.bell_start_delay);
    store.set("volume", settings.volume);
    store.set("customSoundPath", settings.custom_sound_path.clone());
    store.set("pauseMediaOnBell", settings.pause_media_on_bell);
    store.set("resumeMediaAfterBell", settings.resume_media_after_bell);
    store.set("soundEnabled", settings.sound_enabled);
    store.set("visualEnabled", settings.visual_enabled);
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
        clock_times: store.get("clockTimes")
            .and_then(|v| {
                v.as_array().map(|arr| {
                    arr.iter().filter_map(|item| item.as_i64().map(|n| n as i32)).collect()
                })
            })
            .unwrap_or_else(|| vec![0, 15, 30, 45]),
        opacity: store.get("opacity")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.8),
        duration_seconds: store.get("durationSeconds")
            .and_then(|v| v.as_f64())
            .unwrap_or(3.0),
        bell_start_delay: store.get("bellStartDelay")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.5),
        volume: store.get("volume")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.7),
        custom_sound_path: store.get("customSoundPath")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_default(),
        pause_media_on_bell: store.get("pauseMediaOnBell")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        resume_media_after_bell: store.get("resumeMediaAfterBell")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        sound_enabled: store.get("soundEnabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
        visual_enabled: store.get("visualEnabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
    }
}

/// Calculate seconds until the next clock-aligned bell trigger
/// For clock-aligned mode, we trigger at the specified minutes of each hour
fn seconds_until_next_aligned_trigger(clock_times: &[i32]) -> u64 {
    if clock_times.is_empty() {
        // No times configured, wait an hour
        return 3600;
    }

    let now = Local::now();
    let current_minute = now.minute() as i32;
    let current_second = now.second() as i32;

    // Sort clock times and find the next one
    let mut sorted_times: Vec<i32> = clock_times.to_vec();
    sorted_times.sort();

    // Find the next trigger minute
    let next_trigger_minute = sorted_times
        .iter()
        .find(|&&m| m > current_minute || (m == current_minute && current_second == 0))
        .copied();

    let (minutes_to_wait, wrap_to_next_hour) = match next_trigger_minute {
        Some(next_min) if next_min > current_minute => (next_min - current_minute, false),
        Some(next_min) if next_min == current_minute && current_second == 0 => (0, false),
        _ => {
            // Wrap to next hour - use the first time in sorted list
            let first_time = sorted_times[0];
            (60 - current_minute + first_time, true)
        }
    };

    // Calculate seconds until next trigger
    let seconds_to_wait = if wrap_to_next_hour || minutes_to_wait > 0 {
        (minutes_to_wait * 60) - current_second
    } else {
        0
    };

    // Ensure we always wait at least 1 second
    if seconds_to_wait <= 0 {
        1
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
            let (timing_mode, interval_minutes, clock_times) = {
                let state = app.state::<SettingsState>();
                let settings = match state.0.lock() {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("Failed to lock settings: {}", e);
                        thread::sleep(Duration::from_secs(1));
                        continue;
                    }
                };
                (settings.timing_mode.clone(), settings.interval_minutes, settings.clock_times.clone())
            };

            // Calculate sleep duration based on timing mode
            let sleep_seconds = if timing_mode == "clock-aligned" {
                seconds_until_next_aligned_trigger(&clock_times)
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

/// Create overlay windows on all monitors with the given opacity, duration, and sound settings
fn create_overlay_windows(app: &AppHandle, opacity: f64, duration: f64, bell_delay: f64, sound_enabled: bool) {
    let monitors = match app.available_monitors() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Failed to get monitors: {}", e);
            return;
        }
    };

    let mut is_first = true;
    for monitor in monitors {
        let counter = OVERLAY_COUNTER.fetch_add(1, Ordering::SeqCst);
        let window_label = format!("overlay-{}", counter);
        let position = monitor.position();
        let size = monitor.size();
        // Only the first overlay window triggers the sound (if sound is enabled)
        let play_sound = if is_first && sound_enabled { "true" } else { "false" };
        is_first = false;
        let url = format!("overlay.html?opacity={}&duration={}&bellDelay={}&playSound={}", opacity, duration, bell_delay, play_sound);

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

/// Check if media is currently playing using MediaRemote framework
fn is_media_playing() -> bool {
    let swift_code = r#"
import Foundation

let bundle = CFBundleCreate(kCFAllocatorDefault, NSURL(fileURLWithPath: "/System/Library/PrivateFrameworks/MediaRemote.framework"))

guard let MRMediaRemoteGetNowPlayingInfoPointer = CFBundleGetFunctionPointerForName(bundle, "MRMediaRemoteGetNowPlayingInfo" as CFString) else {
    print("false")
    exit(0)
}

typealias MRMediaRemoteGetNowPlayingInfoFunction = @convention(c) (DispatchQueue, @escaping ([String: Any]) -> Void) -> Void
let MRMediaRemoteGetNowPlayingInfo = unsafeBitCast(MRMediaRemoteGetNowPlayingInfoPointer, to: MRMediaRemoteGetNowPlayingInfoFunction.self)

var didPrint = false

MRMediaRemoteGetNowPlayingInfo(DispatchQueue.main) { info in
    if let rate = info["kMRMediaRemoteNowPlayingInfoPlaybackRate"] as? Double {
        print(rate > 0 ? "true" : "false")
    } else {
        print("false")
    }
    didPrint = true
    CFRunLoopStop(CFRunLoopGetMain())
}

// Run the loop to allow callback to execute
RunLoop.main.run(until: Date(timeIntervalSinceNow: 1.0))
if !didPrint { print("false") }
"#;

    let output = std::process::Command::new("swift")
        .arg("-e")
        .arg(swift_code)
        .output();

    match output {
        Ok(out) => {
            let result = String::from_utf8_lossy(&out.stdout).trim().to_string();
            result == "true"
        }
        Err(_) => false,
    }
}

/// Send media play/pause key on macOS using system media key event
fn send_media_key() {
    // Use Swift to send the actual media play/pause key event
    // NX_KEYTYPE_PLAY = 16
    let swift_code = r#"
import Cocoa

func sendMediaKey(_ key: Int32) {
    func doKey(_ down: Bool) {
        let flags = NSEvent.ModifierFlags(rawValue: (down ? 0xa00 : 0xb00))
        let data1 = Int((key << 16) | (down ? 0xa00 : 0xb00))
        let event = NSEvent.otherEvent(
            with: .systemDefined,
            location: .zero,
            modifierFlags: flags,
            timestamp: 0,
            windowNumber: 0,
            context: nil,
            subtype: 8,
            data1: data1,
            data2: -1
        )
        event?.cgEvent?.post(tap: .cghidEventTap)
    }
    doKey(true)
    doKey(false)
}

sendMediaKey(16)  // 16 = Play/Pause
"#;

    let _ = std::process::Command::new("swift")
        .arg("-e")
        .arg(swift_code)
        .output();
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

/// Trigger the bell: show overlay which will play sound after configured delay
fn trigger_bell(app: &AppHandle) {
    let state = app.state::<SettingsState>();
    let settings = match state.0.lock() {
        Ok(s) => s.clone(),
        Err(e) => {
            eprintln!("Failed to lock settings: {}", e);
            return;
        }
    };

    // Pause media if enabled and media is currently playing
    let did_pause = if settings.pause_media_on_bell && is_media_playing() {
        send_media_key();
        true
    } else {
        false
    };

    // Schedule media resume if we actually paused something
    if did_pause && settings.resume_media_after_bell {
        let duration = settings.duration_seconds;
        thread::spawn(move || {
            // Wait for the bell duration to complete
            thread::sleep(Duration::from_secs_f64(duration + 0.5)); // Add small buffer
            send_media_key(); // Toggle play/pause again to resume
        });
    }

    // Create overlay windows if visual is enabled
    // Pass sound_enabled flag to control whether sound plays
    if settings.visual_enabled {
        create_overlay_windows(
            app,
            settings.opacity,
            settings.duration_seconds,
            settings.bell_start_delay,
            settings.sound_enabled,
        );
    } else if settings.sound_enabled {
        // No visual overlay, but sound is enabled - play sound directly
        let volume = settings.volume as f32;
        let custom_path = settings.custom_sound_path.clone();
        let delay = settings.bell_start_delay;
        thread::spawn(move || {
            thread::sleep(Duration::from_secs_f64(delay));
            let sound_data = load_sound_data(&custom_path);
            let _ = play_sound_with_volume(sound_data, volume);
        });
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
    let bell_delay = settings.bell_start_delay;
    let sound_enabled = settings.sound_enabled;
    drop(settings);

    create_overlay_windows(&app, opacity, duration, bell_delay, sound_enabled);
    Ok(())
}

/// Trigger the bell with all settings (including delay) - used by Test Bell button
#[tauri::command]
fn trigger_test_bell(app: AppHandle) -> Result<(), String> {
    trigger_bell(&app);
    Ok(())
}

/// Close an overlay window (called from JS when animation completes)
#[tauri::command]
fn close_overlay_window(window: tauri::WebviewWindow) -> Result<(), String> {
    window.close().map_err(|e| e.to_string())
}

/// Open or focus the settings window
fn open_settings_window(app: &AppHandle) {
    let window = if let Some(window) = app.get_webview_window("settings") {
        // Window exists, just show it
        let _ = window.unminimize();
        let _ = window.show();
        window
    } else {
        // Create settings window if it doesn't exist
        match tauri::WebviewWindowBuilder::new(
            app,
            "settings",
            tauri::WebviewUrl::App("index.html".into()),
        )
        .title("Mindfulness Bell Settings")
        .inner_size(400.0, 620.0)
        .min_inner_size(350.0, 500.0)
        .resizable(true)
        .focused(true)
        .build()
        {
            Ok(w) => w,
            Err(e) => {
                eprintln!("Failed to create settings window: {}", e);
                return;
            }
        }
    };

    // Force window to front on macOS using always_on_top trick
    let _ = window.set_always_on_top(true);
    let _ = window.set_always_on_top(false);
    let _ = window.set_focus();
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .manage(SettingsState(Mutex::new(Settings::default())))
        .invoke_handler(tauri::generate_handler![get_settings, save_settings, play_bell, show_overlay, close_overlay_window, trigger_test_bell])
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
            let separator = PredefinedMenuItem::separator(app)?;
            let quit =
                MenuItem::with_id(app, "quit", "Quit Mindfulness Bell", true, None::<&str>)?;

            // Build the tray menu
            let menu = Menu::with_items(
                app,
                &[&settings_menu, &separator, &quit],
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
            match event {
                tauri::RunEvent::ExitRequested { api, code, .. } => {
                    // Only prevent exit if no explicit code was given (i.e., window close)
                    // Allow exit if user explicitly quits (code = Some(0))
                    if code.is_none() {
                        api.prevent_exit();
                    }
                }
                _ => {}
            }
        });
}
