# Mindfulness Bell Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a macOS menu bar app that plays a bell and dims the screen at regular intervals.

**Architecture:** SwiftUI app using MenuBarExtra for the menu bar interface, NSPanel for fullscreen overlays, AVAudioPlayer for audio. Settings persisted via @AppStorage (UserDefaults).

**Tech Stack:** Swift, SwiftUI, AppKit (NSPanel, NSScreen), AVFoundation

---

## Task 1: Create Xcode Project

**Files:**
- Create: Xcode project `MindfulnessBell`

**Step 1: Create the project**

Run in terminal:
```bash
cd /Users/provolot/github/mindfulnessbell
mkdir -p MindfulnessBell
cd MindfulnessBell
```

Create project via Xcode CLI or manually:
- Open Xcode → File → New → Project
- Choose: macOS → App
- Product Name: `MindfulnessBell`
- Team: None (or personal)
- Organization Identifier: `com.mindfulnessbell`
- Interface: SwiftUI
- Language: Swift
- Uncheck: Include Tests (we'll add manually if needed)
- Save to: `/Users/provolot/github/mindfulnessbell/MindfulnessBell`

**Step 2: Configure as menu bar only app**

Edit `MindfulnessBell/Info.plist` (or via Xcode target settings):
- Add key: `LSUIElement` = `YES` (Application is agent)

This hides the app from Dock.

**Step 3: Set deployment target**

In Xcode project settings:
- Set macOS Deployment Target to `13.0`

**Step 4: Verify project builds**

Run: `Cmd+B` in Xcode
Expected: Build Succeeded

**Step 5: Commit**

```bash
cd /Users/provolot/github/mindfulnessbell
git add .
git commit -m "feat: create Xcode project for MindfulnessBell"
```

---

## Task 2: Basic Menu Bar App Shell

**Files:**
- Modify: `MindfulnessBell/MindfulnessBellApp.swift`
- Delete: `MindfulnessBell/ContentView.swift` (not needed)

**Step 1: Replace MindfulnessBellApp.swift with MenuBarExtra**

```swift
import SwiftUI

@main
struct MindfulnessBellApp: App {
    var body: some Scene {
        MenuBarExtra {
            VStack(alignment: .leading, spacing: 8) {
                Text("Mindfulness Bell")
                    .font(.headline)
                Divider()
                Button("Quit") {
                    NSApplication.shared.terminate(nil)
                }
                .keyboardShortcut("q")
            }
            .padding()
        } label: {
            Image(systemName: "bell.fill")
        }
        .menuBarExtraStyle(.window)
    }
}
```

**Step 2: Delete ContentView.swift**

Delete the file `ContentView.swift` from the project (it's unused).

**Step 3: Build and run**

Run: `Cmd+R` in Xcode
Expected: App launches with bell icon in menu bar. Clicking shows "Mindfulness Bell" and Quit button.

**Step 4: Commit**

```bash
git add .
git commit -m "feat: add basic MenuBarExtra shell with quit button"
```

---

## Task 3: Settings Model with Persistence

**Files:**
- Create: `MindfulnessBell/Settings.swift`

**Step 1: Create Settings.swift**

```swift
import SwiftUI

enum TimingMode: String, CaseIterable {
    case clockAligned = "Clock-aligned"
    case fixedInterval = "Fixed interval"
}

enum IntervalMinutes: Int, CaseIterable {
    case fifteen = 15
    case thirty = 30
    case sixty = 60

    var label: String {
        switch self {
        case .fifteen: return "15 minutes"
        case .thirty: return "30 minutes"
        case .sixty: return "1 hour"
        }
    }
}

class AppSettings: ObservableObject {
    @AppStorage("isEnabled") var isEnabled: Bool = true
    @AppStorage("timingMode") var timingModeRaw: String = TimingMode.clockAligned.rawValue
    @AppStorage("intervalMinutes") var intervalMinutesRaw: Int = IntervalMinutes.fifteen.rawValue
    @AppStorage("opacity") var opacity: Double = 0.8
    @AppStorage("durationSeconds") var durationSeconds: Double = 3.0
    @AppStorage("volume") var volume: Double = 0.7
    @AppStorage("customSoundPath") var customSoundPath: String = ""

    var timingMode: TimingMode {
        get { TimingMode(rawValue: timingModeRaw) ?? .clockAligned }
        set { timingModeRaw = newValue.rawValue }
    }

    var intervalMinutes: IntervalMinutes {
        get { IntervalMinutes(rawValue: intervalMinutesRaw) ?? .fifteen }
        set { intervalMinutesRaw = newValue.rawValue }
    }

    var useCustomSound: Bool {
        !customSoundPath.isEmpty
    }
}
```

**Step 2: Build to verify no errors**

Run: `Cmd+B`
Expected: Build Succeeded

**Step 3: Commit**

```bash
git add .
git commit -m "feat: add AppSettings model with @AppStorage persistence"
```

---

## Task 4: Menu Bar Settings UI

**Files:**
- Create: `MindfulnessBell/MenuBarView.swift`
- Modify: `MindfulnessBell/MindfulnessBellApp.swift`

**Step 1: Create MenuBarView.swift**

```swift
import SwiftUI
import UniformTypeIdentifiers

struct MenuBarView: View {
    @ObservedObject var settings: AppSettings

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            // Header with enable toggle
            HStack {
                Text("Mindfulness Bell")
                    .font(.headline)
                Spacer()
                Toggle("", isOn: $settings.isEnabled)
                    .toggleStyle(.switch)
                    .labelsHidden()
            }

            Divider()

            // Timing Mode
            VStack(alignment: .leading, spacing: 4) {
                Text("Timing").font(.subheadline).foregroundColor(.secondary)
                Picker("Mode", selection: $settings.timingMode) {
                    ForEach(TimingMode.allCases, id: \.self) { mode in
                        Text(mode.rawValue).tag(mode)
                    }
                }
                .pickerStyle(.segmented)
                .labelsHidden()

                Picker("Interval", selection: $settings.intervalMinutes) {
                    ForEach(IntervalMinutes.allCases, id: \.self) { interval in
                        Text(interval.label).tag(interval)
                    }
                }
                .labelsHidden()
            }

            Divider()

            // Opacity
            VStack(alignment: .leading, spacing: 4) {
                Text("Screen Opacity: \(Int(settings.opacity * 100))%")
                    .font(.subheadline).foregroundColor(.secondary)
                Slider(value: $settings.opacity, in: 0.5...1.0)
            }

            // Duration
            VStack(alignment: .leading, spacing: 4) {
                Text("Duration: \(String(format: "%.1f", settings.durationSeconds))s")
                    .font(.subheadline).foregroundColor(.secondary)
                Slider(value: $settings.durationSeconds, in: 1...10)
            }

            // Volume
            VStack(alignment: .leading, spacing: 4) {
                Text("Volume: \(Int(settings.volume * 100))%")
                    .font(.subheadline).foregroundColor(.secondary)
                Slider(value: $settings.volume, in: 0...1)
            }

            Divider()

            // Sound selection
            VStack(alignment: .leading, spacing: 4) {
                Text("Sound").font(.subheadline).foregroundColor(.secondary)
                HStack {
                    Text(settings.useCustomSound ? URL(fileURLWithPath: settings.customSoundPath).lastPathComponent : "Default Bell")
                        .lineLimit(1)
                        .truncationMode(.middle)
                    Spacer()
                    Button("Choose...") {
                        selectCustomSound()
                    }
                    if settings.useCustomSound {
                        Button("Reset") {
                            settings.customSoundPath = ""
                        }
                    }
                }
            }

            Divider()

            Button("Quit Mindfulness Bell") {
                NSApplication.shared.terminate(nil)
            }
            .keyboardShortcut("q")
        }
        .padding()
        .frame(width: 280)
    }

    private func selectCustomSound() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.audio]
        panel.allowsMultipleSelection = false
        panel.canChooseDirectories = false

        if panel.runModal() == .OK, let url = panel.url {
            settings.customSoundPath = url.path
        }
    }
}
```

**Step 2: Update MindfulnessBellApp.swift to use MenuBarView**

```swift
import SwiftUI

@main
struct MindfulnessBellApp: App {
    @StateObject private var settings = AppSettings()

    var body: some Scene {
        MenuBarExtra {
            MenuBarView(settings: settings)
        } label: {
            Image(systemName: settings.isEnabled ? "bell.fill" : "bell.slash.fill")
        }
        .menuBarExtraStyle(.window)
    }
}
```

**Step 3: Build and run**

Run: `Cmd+R`
Expected: Menu bar shows full settings UI with all controls working. Settings persist across app restarts.

**Step 4: Commit**

```bash
git add .
git commit -m "feat: add complete settings UI in menu bar dropdown"
```

---

## Task 5: Audio Player

**Files:**
- Create: `MindfulnessBell/BellPlayer.swift`
- Add: `MindfulnessBell/Resources/bell.aiff` (default sound)

**Step 1: Find/create a bell sound**

Option A - Use a system sound as placeholder:
```bash
cp /System/Library/Sounds/Glass.aiff MindfulnessBell/Resources/bell.aiff
```

Option B - Download a free mindfulness bell sound (e.g., from freesound.org)

Create the Resources folder if needed, then add the sound to the Xcode project.

**Step 2: Create BellPlayer.swift**

```swift
import AVFoundation

class BellPlayer: ObservableObject {
    private var audioPlayer: AVAudioPlayer?

    func play(volume: Double, customSoundPath: String) {
        let url: URL

        if !customSoundPath.isEmpty {
            url = URL(fileURLWithPath: customSoundPath)
        } else if let bundleURL = Bundle.main.url(forResource: "bell", withExtension: "aiff") {
            url = bundleURL
        } else {
            print("No bell sound found")
            return
        }

        do {
            audioPlayer = try AVAudioPlayer(contentsOf: url)
            audioPlayer?.volume = Float(volume)
            audioPlayer?.play()
        } catch {
            print("Failed to play bell: \(error)")
        }
    }
}
```

**Step 3: Add sound file to Xcode project**

In Xcode:
- Right-click on MindfulnessBell folder → Add Files to "MindfulnessBell"
- Select the bell.aiff file
- Ensure "Copy items if needed" is checked
- Ensure target membership includes MindfulnessBell

**Step 4: Build and verify**

Run: `Cmd+B`
Expected: Build Succeeded

**Step 5: Commit**

```bash
git add .
git commit -m "feat: add BellPlayer for audio playback with custom sound support"
```

---

## Task 6: Overlay Window Manager

**Files:**
- Create: `MindfulnessBell/OverlayManager.swift`

**Step 1: Create OverlayManager.swift**

```swift
import AppKit
import SwiftUI

class OverlayManager: ObservableObject {
    private var overlayWindows: [NSWindow] = []

    func showOverlay(opacity: Double, duration: Double) {
        // Create overlay on each screen
        for screen in NSScreen.screens {
            let window = createOverlayWindow(for: screen)
            overlayWindows.append(window)
            window.orderFrontRegardless()
        }

        // Animate fade in, hold, fade out
        let fadeTime = duration / 3.0
        let holdTime = duration / 3.0

        // Fade in
        for window in overlayWindows {
            window.alphaValue = 0
            NSAnimationContext.runAnimationGroup { context in
                context.duration = fadeTime
                window.animator().alphaValue = CGFloat(opacity)
            }
        }

        // Schedule fade out after hold
        DispatchQueue.main.asyncAfter(deadline: .now() + fadeTime + holdTime) { [weak self] in
            self?.fadeOutAndRemove(fadeTime: fadeTime)
        }
    }

    private func fadeOutAndRemove(fadeTime: Double) {
        NSAnimationContext.runAnimationGroup { context in
            context.duration = fadeTime
            for window in overlayWindows {
                window.animator().alphaValue = 0
            }
        } completionHandler: { [weak self] in
            self?.removeAllOverlays()
        }
    }

    private func removeAllOverlays() {
        for window in overlayWindows {
            window.orderOut(nil)
        }
        overlayWindows.removeAll()
    }

    private func createOverlayWindow(for screen: NSScreen) -> NSWindow {
        let window = NSPanel(
            contentRect: screen.frame,
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )

        window.level = .screenSaver
        window.backgroundColor = .black
        window.isOpaque = false
        window.hasShadow = false
        window.ignoresMouseEvents = true
        window.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]

        return window
    }
}
```

**Step 2: Build to verify**

Run: `Cmd+B`
Expected: Build Succeeded

**Step 3: Commit**

```bash
git add .
git commit -m "feat: add OverlayManager for fullscreen dimming on all displays"
```

---

## Task 7: Bell Scheduler

**Files:**
- Create: `MindfulnessBell/BellScheduler.swift`

**Step 1: Create BellScheduler.swift**

```swift
import Foundation
import Combine

class BellScheduler: ObservableObject {
    private var timer: Timer?
    private var settings: AppSettings
    private var onBell: () -> Void

    init(settings: AppSettings, onBell: @escaping () -> Void) {
        self.settings = settings
        self.onBell = onBell
    }

    func start() {
        stop()
        scheduleNextBell()
    }

    func stop() {
        timer?.invalidate()
        timer = nil
    }

    func reschedule() {
        if settings.isEnabled {
            start()
        } else {
            stop()
        }
    }

    private func scheduleNextBell() {
        let interval = calculateNextInterval()

        timer = Timer.scheduledTimer(withTimeInterval: interval, repeats: false) { [weak self] _ in
            self?.ringBell()
        }
    }

    private func ringBell() {
        onBell()
        scheduleNextBell()
    }

    private func calculateNextInterval() -> TimeInterval {
        let intervalSeconds = Double(settings.intervalMinutes.rawValue * 60)

        switch settings.timingMode {
        case .clockAligned:
            return secondsUntilNextClockAlignedTime(intervalMinutes: settings.intervalMinutes.rawValue)
        case .fixedInterval:
            return intervalSeconds
        }
    }

    private func secondsUntilNextClockAlignedTime(intervalMinutes: Int) -> TimeInterval {
        let now = Date()
        let calendar = Calendar.current
        let minute = calendar.component(.minute, from: now)
        let second = calendar.component(.second, from: now)

        // Find next aligned minute
        let currentMinuteInCycle = minute % intervalMinutes
        let minutesUntilNext = intervalMinutes - currentMinuteInCycle

        // Calculate seconds until that time
        let secondsUntilNextMinute = Double((minutesUntilNext * 60) - second)

        // If we're exactly on an aligned time, wait for the next one
        if secondsUntilNextMinute <= 0 {
            return Double(intervalMinutes * 60)
        }

        return secondsUntilNextMinute
    }
}
```

**Step 2: Build to verify**

Run: `Cmd+B`
Expected: Build Succeeded

**Step 3: Commit**

```bash
git add .
git commit -m "feat: add BellScheduler with clock-aligned and fixed interval modes"
```

---

## Task 8: Wire Everything Together

**Files:**
- Modify: `MindfulnessBell/MindfulnessBellApp.swift`

**Step 1: Update MindfulnessBellApp.swift to connect all components**

```swift
import SwiftUI

@main
struct MindfulnessBellApp: App {
    @StateObject private var settings = AppSettings()
    @StateObject private var bellPlayer = BellPlayer()
    @StateObject private var overlayManager = OverlayManager()

    @State private var scheduler: BellScheduler?

    var body: some Scene {
        MenuBarExtra {
            MenuBarView(settings: settings)
                .onAppear {
                    setupScheduler()
                }
                .onChange(of: settings.isEnabled) { _ in
                    scheduler?.reschedule()
                }
                .onChange(of: settings.timingModeRaw) { _ in
                    scheduler?.reschedule()
                }
                .onChange(of: settings.intervalMinutesRaw) { _ in
                    scheduler?.reschedule()
                }
        } label: {
            Image(systemName: settings.isEnabled ? "bell.fill" : "bell.slash.fill")
        }
        .menuBarExtraStyle(.window)
    }

    private func setupScheduler() {
        guard scheduler == nil else { return }

        scheduler = BellScheduler(settings: settings) { [self] in
            ringBell()
        }

        if settings.isEnabled {
            scheduler?.start()
        }
    }

    private func ringBell() {
        bellPlayer.play(volume: settings.volume, customSoundPath: settings.customSoundPath)
        overlayManager.showOverlay(opacity: settings.opacity, duration: settings.durationSeconds)
    }
}
```

**Step 2: Build and run full test**

Run: `Cmd+R`
Expected:
- App appears in menu bar with bell icon
- Settings UI works
- For testing: temporarily change interval to a short time or add a "Test Bell" button
- Bell rings and screen dims at intervals

**Step 3: Commit**

```bash
git add .
git commit -m "feat: wire up scheduler, player, and overlay for complete functionality"
```

---

## Task 9: Add Test Bell Button (Optional but Recommended)

**Files:**
- Modify: `MindfulnessBell/MenuBarView.swift`

**Step 1: Add test button and callback**

Add to MenuBarView struct:
```swift
var onTestBell: () -> Void = {}
```

Add before Quit button in the view body:
```swift
Button("Test Bell") {
    onTestBell()
}

Divider()
```

**Step 2: Update MindfulnessBellApp.swift**

Pass the callback:
```swift
MenuBarView(settings: settings, onTestBell: ringBell)
```

Update MenuBarView init to accept the callback.

**Step 3: Build and test**

Run: `Cmd+R`
Expected: Clicking "Test Bell" plays sound and dims screen immediately.

**Step 4: Commit**

```bash
git add .
git commit -m "feat: add Test Bell button for easy testing"
```

---

## Task 10: Final Polish

**Files:**
- Various minor adjustments

**Step 1: Verify all edge cases**

- [ ] Test with multiple monitors
- [ ] Test clock-aligned mode waits for correct time
- [ ] Test fixed interval mode
- [ ] Test custom sound file selection
- [ ] Test volume slider
- [ ] Test opacity slider
- [ ] Test duration slider
- [ ] Test enable/disable toggle
- [ ] Test settings persist across restart
- [ ] Test Quit button works

**Step 2: Clean up any debug code**

Remove any temporary testing code or print statements.

**Step 3: Build release version**

In Xcode: Product → Archive (for distribution) or Product → Build for → Running

**Step 4: Final commit**

```bash
git add .
git commit -m "feat: complete mindfulness bell app v1.0"
```

---

## Summary

After completing all tasks, you'll have:

1. A menu bar app with bell icon (changes when paused)
2. Settings dropdown with all controls
3. Clock-aligned and fixed interval timing modes
4. Configurable opacity, duration, and volume
5. Default bell sound with custom sound option
6. Fullscreen dimming overlay on all monitors
7. Persistent settings via UserDefaults

The app is ~200-300 lines of Swift code total, lightweight, and fully functional.
