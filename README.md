# Mindfulness Bell

A lightweight macOS menu bar app that rings a bell and dims the screen at configurable intervals to encourage mindfulness breaks.

## Features

- **Menu bar app** - Runs quietly in your menu bar with no dock icon
- **Configurable intervals** - 15 minutes, 30 minutes, or 1 hour
- **Two timing modes**:
  - **Clock-aligned** - Triggers at :00, :15, :30, :45 (based on interval)
  - **Fixed interval** - Triggers every N minutes from when enabled
- **Screen dimming** - Full-screen overlay with smooth fade in/out animation
- **Customizable appearance**:
  - Screen opacity (50-100%)
  - Duration (1-30 seconds)
  - Bell start delay (0-3 seconds) - lets the dim start before the bell rings
- **Custom sounds** - Use the built-in bell or choose your own audio file
- **Volume control** - Adjust bell volume independently

## Building

Requires [Rust](https://rustup.rs/) and the Tauri CLI.

```bash
# Install Tauri CLI
cargo install tauri-cli

# Build for development
cd src-tauri
cargo build

# Run in development mode
cargo run

# Build release bundle
cargo tauri build
```

## Usage

1. Launch the app - it appears as a bell icon in your menu bar
2. Click the icon to access the menu
3. Select "Settings..." to configure intervals, appearance, and sound
4. Use "Test Bell" in settings to preview your configuration
5. The bell icon shows enabled/disabled state

## Tech Stack

- [Tauri v2](https://tauri.app/) - Rust + Web frontend
- [rodio](https://github.com/RustAudio/rodio) - Audio playback
- Vanilla HTML/CSS/JS for the settings UI

## License

MIT
