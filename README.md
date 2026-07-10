# FrogScribe — Voice Dictation for Linux GNOME

```
     @..@
    (----)
   ( >__< )
   ^^ ~~ ^^
```

Voice dictation for Linux GNOME. Press a hotkey, speak, and your words are transcribed and inserted at the cursor using on-device AI. Like a frog catching flies — quick, precise, and always listening when you need it.

## Features

- **Global Hotkey** — Press `Ctrl+Shift+Space` (configurable) to toggle recording from anywhere
- **Hold-to-Talk** — Hold a key to record, release to transcribe (push-to-talk)
- **On-Device Transcription** — Uses whisper.cpp for fast, private speech-to-text (no internet required)
- **Direct Text Insertion** — Transcribed text is typed or pasted into the active app
- **Desktop Audio Capture** — Mix speaker/headphone output with mic for full meeting transcription
- **Window Picker** — Choose which window receives the transcription with live window previews
- **Auto-Submit** — Optionally press Enter after insertion for hands-free submission
- **Multiple Models** — Tiny through Large-v3, download and switch freely
- **Language Selection** — 99+ languages supported by Whisper
- **Translate to English** — Translate non-English speech into English text
- **Audio Device Selection** — Pick which microphone to use for recording
- **Office Mode** — Boosts soft-spoken audio for improved accuracy in quiet environments
- **Streaming Transcription** — Live preview of partial text as you speak
- **Long-Form Dictation** — Continuous recording mode for extended dictation
- **Text Refinement** — Filler word removal, capitalization fixes, custom vocabulary
- **Visual Feedback** — Recording indicator with Pill and/or Top Bar style, 7 accent colors
- **GNOME Shell Extension** — Panel indicator with status, controls, and top bar recording overlay
- **Automatic Transcription** — Detects when another app uses the microphone and starts recording
- **Auto-Save** — Optionally save all transcriptions as timestamped files with context headers
- **Transcription History** — Browse past transcriptions with text, timestamp, and duration
- **D-Bus Service** — Control FrogScribe from scripts via `com.frogscribe.Daemon`
- **CLI Mode** — Transcribe audio files from the command line
- **Configurable Hotkey** — Any modifier+key combo (Alt+Space, Ctrl+Shift+R, Super+Space, etc.)

## Requirements

- Linux with PulseAudio or PipeWire
- GNOME Shell 45+ (for the panel extension)
- `ydotool` for text insertion (system service with socket permissions)
- `ffmpeg` for CLI audio file transcription
- `notify-send` for desktop notifications
- `gtk-layer-shell` for Wayland overlay support
- User in `input` group for global hotkeys: `sudo usermod -aG input $USER`

## Build

```bash
# Install system dependencies (Debian/Ubuntu)
sudo apt install build-essential libclang-dev libgtk-3-dev \
    libappindicator3-dev libgtk-layer-shell-dev libopenblas-dev \
    libvulkan-dev ffmpeg ydotool

# NOTE: Ubuntu 24.04 ships Rust 1.75 which is too old. Install Rust 1.85+ via rustup:
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Install system dependencies (Fedora)
sudo dnf install rust cargo clang-devel gtk3-devel libappindicator-gtk3-devel \
    openssl-devel pango-devel gdk-pixbuf2-devel cairo-devel glib2-devel atk-devel \
    gtk-layer-shell-devel openblas-devel vulkan-headers vulkan-loader-devel \
    ffmpeg-free ydotool

# Build
export BLAS_INCLUDE_DIRS=/usr/include/openblas
cargo build --release

# Install
sudo cp target/release/frogscribe /usr/local/bin/
```

## Usage

```bash
# Run the daemon (with system tray)
frogscribe

# Transcribe a file
frogscribe --transcribe recording.wav

# Use a specific model and language
frogscribe --transcribe audio.mp3 --model small --language ja

# Translate to English
frogscribe --transcribe audio.mp3 --translate
```

## D-Bus Control

Control FrogScribe from scripts or other applications:

```bash
# Toggle recording
dbus-send --session --dest=com.frogscribe.Daemon \
    /com/frogscribe/Daemon com.frogscribe.Daemon.ToggleRecording

# Start/stop recording
dbus-send --session --dest=com.frogscribe.Daemon \
    /com/frogscribe/Daemon com.frogscribe.Daemon.StartRecording
dbus-send --session --dest=com.frogscribe.Daemon \
    /com/frogscribe/Daemon com.frogscribe.Daemon.StopRecording

# Get status
dbus-send --session --dest=com.frogscribe.Daemon \
    /com/frogscribe/Daemon com.frogscribe.Daemon.GetStatus

# Quit
dbus-send --session --dest=com.frogscribe.Daemon \
    /com/frogscribe/Daemon com.frogscribe.Daemon.Quit
```

## Configuration

Settings are stored in `~/.config/frogscribe/settings.toml`:

```toml
[hotkey]
toggle_key = "Ctrl+Shift+Space"
activation_method = "Toggle"  # or "HoldToTalk"

[audio]
office_mode = false
sample_rate = 16000

[transcription]
model = "base"
language = "en"
translate_to_english = false
streaming = false

[refinement]
enabled = true
remove_fillers = true
fix_capitalization = true
custom_vocabulary = ["Rust", "GNOME", "PipeWire", "Wayland"]

[appearance]
pill_enabled = true
topbar_enabled = false
accent_color = "teal"     # teal, blue, purple, pink, orange, green, yellow

[general]
auto_submit = false
history_enabled = true
auto_save_transcriptions = false
context_header = true

[auto_transcription]
enabled = false
vad_enabled = true
silence_seconds = 30
```

## Model Storage

Models are stored in `~/.local/share/frogscribe/models/`.

## Transcription History

History is stored in `~/.local/share/frogscribe/history.json`.

## Architecture

```
FrogScribe/src/
├── main.rs                  # Entry point, event loop, CLI dispatch
├── audio/
│   ├── mod.rs               # PulseAudio/PipeWire recording (16kHz mono f32)
│   └── devices.rs           # Audio device enumeration via pactl
├── auto_transcription/mod.rs # Mic activity detection + VAD auto-stop
├── autostart/mod.rs         # XDG autostart .desktop management
├── cli/mod.rs               # CLI audio file transcription via ffmpeg
├── dbus/mod.rs              # D-Bus service (com.frogscribe.Daemon)
├── history/mod.rs           # Transcription history (JSON storage)
├── history_window/mod.rs    # GTK3 history viewer
├── hotkey/mod.rs            # Configurable global hotkeys via evdev
├── indicator/mod.rs         # GTK3 recording indicator overlay (Pill mode)
├── insertion/mod.rs         # Text insertion via ydotool + clipboard
├── languages/mod.rs         # 99+ language registry
├── longform/mod.rs          # Long-form continuous dictation sessions
├── models/mod.rs            # Model metadata, download, health check
├── notifications/mod.rs     # Desktop notifications via notify-send
├── onboarding/              # First-run setup wizard
├── refinement/mod.rs        # Rule-based text cleanup
├── settings/mod.rs          # TOML config persistence
├── streaming/mod.rs         # Streaming transcription with partial results
├── tests.rs                 # Test suite (94 tests)
├── transcript_window/mod.rs # Long-form transcript window
├── transcription/mod.rs     # whisper.cpp integration via whisper-rs
├── tray/mod.rs              # Ayatana AppIndicator system tray
└── ui/mod.rs                # GTK3 settings window

gnome-extension/
├── extension.js             # GNOME Shell panel indicator + top bar overlay
├── metadata.json            # Extension metadata
├── stylesheet.css           # Extension styles
└── frogscribe-symbolic.svg        # Panel icon
```

## Feature Parity with macOS

| macOS Feature | Linux Implementation |
|---------------|---------------------|
| Global Hotkey (⌥Space) | evdev monitoring (`Ctrl+Shift+Space`, configurable) |
| Fn Hold-to-Talk | Hold-to-talk via evdev (any key) |
| WhisperKit (CoreML) | whisper.cpp (OpenBLAS + Vulkan) |
| Direct Text Insertion | ydotool + clipboard |
| Auto-Submit | ydotool Enter key simulation |
| Multiple Models | ggml-tiny through ggml-large-v3 |
| 99+ Languages | Full Whisper language registry |
| Translate to English | whisper.cpp translate mode |
| Audio Device Selection | pactl source enumeration |
| Office Mode | Peak normalization |
| Streaming Transcription | Periodic re-transcription with live preview window |
| Long-Form Dictation | Continuous recording + transcript window |
| Text Refinement (Local) | Regex-based filler/caps/vocab |
| Recording Indicator | GTK3 pill overlay + GNOME Shell top bar |
| Transcription History | JSON storage + GTK3 viewer |
| Menu Bar | GNOME Shell extension panel indicator |
| Launch at Login | Managed by GNOME Shell extension |
| Settings UI | GTK3 notebook (5 tabs) |
| CLI Transcription | --transcribe flag + ffmpeg |
| Model Health Check | Size validation + auto-repair |
| Auto-Transcription | Mic activity detection + energy VAD |
| Auto-Save | Timestamped files with context headers |
| Auto-Update | — (use system package manager) |

## License

MIT
