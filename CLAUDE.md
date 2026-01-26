# CLAUDE.md

This file provides guidance for Claude Code when working on this project.

## Project Overview

Mindfulness Bell is a macOS menu bar app built with Tauri v2. It rings a bell and dims the screen at configurable intervals to encourage mindfulness breaks.

## Architecture

### Backend (Rust)
- `src-tauri/src/main.rs` - Main application code
  - Settings management with `tauri-plugin-store` for persistence
  - System tray icon with menu
  - Scheduler for triggering bells (supports clock-aligned and fixed interval modes)
  - Overlay window creation for screen dimming
  - Audio playback using `rodio`

### Frontend (HTML/CSS/JS)
- `dist/index.html` - Settings window UI
- `dist/overlay.html` - Fullscreen dimming overlay with fade animation

### Key Patterns

**Settings Flow:**
1. Settings stored in `SettingsState` (Mutex-wrapped) for thread-safe access
2. Persisted to disk via `tauri-plugin-store`
3. Frontend reads/writes via `get_settings` and `save_settings` Tauri commands

**Bell Trigger Flow:**
1. Scheduler runs in background thread, checks settings each second
2. Emits `trigger-bell` event when it's time
3. `trigger_bell()` creates overlay windows on all monitors
4. First overlay triggers sound playback after configured delay via JS setTimeout
5. Overlays close themselves when animation completes

**Tray App Behavior:**
- App stays running when windows close (using `api.prevent_exit()` in `RunEvent::ExitRequested`)
- Explicit quit via menu sets exit code, allowing app to close

## Build Commands

```bash
cd src-tauri
cargo build          # Development build
cargo run            # Run in dev mode
cargo tauri build    # Release bundle
```

## Key Files

| File | Purpose |
|------|---------|
| `src-tauri/src/main.rs` | All Rust backend code |
| `src-tauri/tauri.conf.json` | Tauri configuration |
| `src-tauri/Cargo.toml` | Rust dependencies |
| `dist/index.html` | Settings UI |
| `dist/overlay.html` | Dimming overlay |
| `src-tauri/sounds/bell.mp3` | Default bell sound |
| `src-tauri/icons/` | Tray icons (enabled/disabled states) |

## Important Notes

- The app creates overlay windows dynamically with unique labels (`overlay-0`, `overlay-1`, etc.)
- Only the first overlay window triggers sound playback (to avoid duplicate sounds on multi-monitor setups)
- Sound delay is implemented in overlay.html JS, not Rust, to ensure timing is relative to visual dimming start
- Scheduler uses version counter to gracefully restart when settings change
