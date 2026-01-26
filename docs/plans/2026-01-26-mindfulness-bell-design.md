# Mindfulness Bell - Design Document

A lightweight macOS menu bar app that plays a bell sound and dims the screen at regular intervals as a mindfulness reminder.

## Core Behavior

The app runs as a menu bar icon (bell glyph). At configured intervals, it:

1. Plays a bell sound
2. Creates a fullscreen overlay window on all displays
3. Fades the overlay from transparent → user-configured opacity
4. Holds briefly
5. Fades back to transparent
6. Removes the overlay

The overlay sits above all content but doesn't capture input, so typing continues uninterrupted.

## Settings

All settings live in the menu bar dropdown:

- **Timing mode**:
  - Clock-aligned (:00, :15, :30, :45) with interval selection (15 min / 30 min / 1 hour)
  - Fixed interval (15 min / 30 min / 1 hour from last bell)
- **Opacity**: Slider - 50% to 100% (black)
- **Duration**: Slider - 1 to 10 seconds (total fade in + hold + fade out)
- **Volume**: Slider - 0% to 100%
- **Sound**: Default bell / Choose custom file...
- **Enabled**: Toggle to pause/resume
- **Quit**: Exit the app

## Architecture

### Project Structure

```
MindfulnessBell/
├── MindfulnessBellApp.swift      # App entry, menu bar setup
├── MenuBarView.swift             # Settings dropdown UI
├── OverlayWindow.swift           # Fullscreen dim overlay
├── BellScheduler.swift           # Timer logic for both modes
├── AudioPlayer.swift             # Bell sound playback
├── Settings.swift                # @AppStorage persistence
└── Resources/
    └── bell.aiff                 # Default bell sound
```

### Technical Details

- **Menu bar**: SwiftUI `MenuBarExtra` (macOS 13+)
- **Overlay**: `NSPanel` with `.fullScreenAuxiliary` level, one per display
- **Persistence**: `@AppStorage` (UserDefaults) for all settings
- **Audio**: `AVAudioPlayer` for sound playback
- **Multi-monitor**: Detects all screens via `NSScreen.screens`, creates overlay on each

### Requirements

- **macOS version**: 13+ (Ventura) for `MenuBarExtra`
- **Permissions**: None required

## UI Details

- **Menu bar icon**: SF Symbol `bell.fill`, shows `bell.slash.fill` when paused
- **Launch at login**: Not included (user can add manually via System Settings)

## Edge Cases

- Fullscreen apps (video, game): overlay still appears on top
- Multiple monitors: overlay appears on all simultaneously
- Sleep/screensaver: skip that bell, resume on next interval
- Settings change while running: next bell recalculates based on new setting

## Out of Scope

- Manual bell trigger from menu bar click
- Statistics/history
- Different sounds for different times
- Do Not Disturb integration
