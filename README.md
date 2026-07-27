# FrogScribe — Voice Dictation for Linux GNOME

```
     @..@
    (----)
   ( >__< )
   ^^ ~~ ^^
```

Voice dictation for Linux GNOME, built with **GTK4** and Rust. Press a hotkey, speak, and your words are transcribed and inserted at the cursor using on-device AI. Like a frog catching flies — quick, precise, and always listening when you need it.

**Key technologies:** GTK4, whisper.cpp (OpenBLAS + Vulkan), GNOME Shell Extension, PipeWire/PulseAudio, D-Bus, evdev

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
- **Local Summarization** — Generate summaries of transcriptions using on-device BART models (no cloud)
- **Configurable Hotkey** — Any modifier+key combo (Alt+Space, Ctrl+Shift+R, Super+Space, etc.)

## Requirements

- Rust 1.85+ (for building from source)
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
sudo apt install build-essential libclang-dev libgtk-4-dev \
    libgtk-layer-shell-dev libopenblas-dev libvulkan-dev \
    glslc cmake ffmpeg ydotool

# NOTE: Ubuntu 24.04 ships Rust 1.75 which is too old. Install Rust 1.85+ via rustup:
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Install system dependencies (Fedora)
sudo dnf install rust cargo clang-devel gtk4-devel \
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
# Run the daemon (managed by GNOME Shell extension)
frogscribe

# Transcribe a file
frogscribe --transcribe recording.wav

# Transcribe and save output to a file
frogscribe --transcribe recording.wav --output transcript.txt

# Use a specific model and language
frogscribe --transcribe audio.mp3 --model small --language ja

# Translate to English
frogscribe --transcribe audio.mp3 --translate

# Summarize an existing transcript
frogscribe --summarize transcript.txt

# Summarize and write to a specific output file
frogscribe --summarize transcript.txt --summary-output summary.txt
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
capture_desktop_audio = false  # mix speaker output with mic for meetings

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
auto_paste = true          # insert text into target window
use_window_picker = true   # show window picker before pasting
insertion_method = "TypeEveryCharacter"  # or "PasteFullTranscript" or "Off"
history_enabled = true
auto_save_transcriptions = false
context_header = true

[auto_transcription]
enabled = false
vad_enabled = true
silence_seconds = 30

[summarization]
enabled = false
model = "distilbart-cnn-12-6"  # or "bart-large-cnn" for best quality
```

## Model Storage

Whisper models are stored in `~/.local/share/frogscribe/models/`.
Summarization models are stored in `~/.local/share/frogscribe/summarization/`.

## Summarization Models

FrogScribe includes local summarization powered by BART-family models running via ONNX Runtime. All inference happens on-device — no data leaves your machine. Models are downloaded from HuggingFace on first use.

| Model | Size | Speed | Quality | License |
|-------|------|-------|---------|---------|
| `distilbart-cnn-12-6` | ~800MB | ~2-5s | Good | Apache-2.0 |
| `bart-large-cnn` | ~1.6GB | ~5-15s | Best | MIT |

**distilbart-cnn-12-6** (default) — A distilled version of BART fine-tuned on CNN/DailyMail for summarization. Uses 6 decoder layers instead of 12, making it roughly 2x faster with only minor quality loss. Best for real-time use where speed matters.

**bart-large-cnn** — The full BART-large model fine-tuned for summarization. Produces more coherent, nuanced summaries especially for longer or complex text. Choose this if summary quality is more important than speed.

Both models are purpose-built for summarization (not general-purpose LLMs), making them small, fast, and focused. They work best with English text.

## Transcription History

History is stored in `~/.local/share/frogscribe/history.json`.

## Architecture

```
frogscribe/src/
├── main.rs                    # Entry point, event loop, CLI dispatch
├── audio/
│   ├── mod.rs                 # PulseAudio/PipeWire recording + desktop audio capture
│   └── devices.rs             # Audio device enumeration via pactl
├── auto_transcription/mod.rs  # Mic activity detection + VAD auto-stop
├── cli/mod.rs                 # CLI audio file transcription via ffmpeg
├── dbus/mod.rs                # D-Bus service (com.frogscribe.Daemon) with auth
├── diarization/mod.rs         # Speaker diarization (pyannote.audio)
├── ear_protection/mod.rs      # Bluetooth audio profile switch protection
├── escape_cancel/mod.rs       # Escape key monitoring to cancel recording
├── history/mod.rs             # Transcription history (JSON storage)
├── history_window/mod.rs      # GTK4 history viewer
├── hotkey/mod.rs              # Configurable global hotkeys via evdev
├── indicator/mod.rs           # GTK4 recording indicator overlay
├── insertion/mod.rs           # Text insertion via ydotool (type/paste modes)
├── known_terms/mod.rs         # Known term corrections (VS Code, macOS, etc.)
├── languages/mod.rs           # 99+ language registry
├── live_preview/mod.rs        # Tabbed live streaming preview window
├── longform/mod.rs            # Long-form continuous dictation sessions
├── model_doctor/mod.rs        # Model integrity checking and repair
├── models/mod.rs              # Model metadata, download management
├── notifications/mod.rs       # Desktop notifications via notify-send
├── onboarding/                # First-run setup wizard (GTK4)
├── practice/mod.rs            # Practice recording for onboarding
├── refinement/mod.rs          # Rule-based text cleanup
├── settings/mod.rs            # TOML config persistence
├── streaming/mod.rs           # Streaming transcription with sliding window
├── summarization/mod.rs       # Local BART-based summarization (ONNX Runtime)
├── tests.rs                   # Test suite (92 tests)
├── transcript_window/mod.rs   # Long-form transcript window
├── transcription/mod.rs       # whisper.cpp integration via whisper-rs
├── ui/mod.rs                  # GTK4 settings window
├── vocabulary/mod.rs          # Custom vocabulary management
└── window_picker/mod.rs       # Window target selection via D-Bus

gnome-extension/
├── extension.js               # Panel indicator, pill/topbar overlays, window picker
├── metadata.json              # Extension metadata (frogscribe@frogscribe.app)
├── stylesheet.css             # Extension styles
└── frogscribe-symbolic.svg    # Panel icon
```

## License

Apache-2.0
