# FrogScribe — Voice Dictation for Linux GNOME

```
     @..@
    (----)
   ( >__< )
   ^^ ~~ ^^
```

Voice dictation for Linux GNOME, built with **GTK4** and Rust. Press a hotkey, speak, and your words are transcribed and inserted at the cursor using on-device AI. Like a frog catching flies — quick, precise, and always listening when you need it.

**Key technologies:** GTK4, whisper.cpp (OpenBLAS + Vulkan), Phi-3 Mini 128K (llama.cpp), GNOME Shell Extension, PipeWire/PulseAudio, D-Bus, evdev

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
- **Local Summarization** — Generate structured meeting notes using on-device Phi-3 Mini 128K (no cloud)
- **Non-blocking Pipeline** — Start a new recording while previous transcription/summarization processes in background
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
sudo dnf install rust cargo clang-devel gtk4-devel cmake \
    openssl-devel pango-devel gdk-pixbuf2-devel cairo-devel glib2-devel atk-devel \
    gtk-layer-shell-devel openblas-devel vulkan-headers vulkan-loader-devel \
    spirv-headers-devel glslang glslc ffmpeg-free ydotool

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
model = "phi-3-mini"  # Phi-3 Mini 128K Instruct for structured meeting notes
```

## Model Storage

Whisper models are stored in `~/.local/share/frogscribe/models/`.
Summarization models are stored in `~/.local/share/frogscribe/summarization/`.

## Model Download Integrity

All model files are downloaded from Hugging Face over HTTPS and verified before use. Before each download, FrogScribe fetches the file's authoritative content digest and size from Hugging Face's TLS-protected metadata API, then checks the downloaded bytes against it:

- Git-LFS tracked files (all large model weights) are verified against the published **SHA256** (`lfs.oid`).
- Non-LFS files (e.g. `tokenizer.json`) are verified against the published **Git blob id** (`SHA1("blob " + len + "\0" + content)`).
- The exact published file size is also checked.

On any size or digest mismatch, the file is deleted and the operation aborts — mitigating tampering by a compromised CDN, cache, or on-path (MITM) attacker. See `src/model_integrity/`.

## Summarization Models

FrogScribe includes local summarization powered by Phi-3 Mini 128K Instruct via llama.cpp (with Vulkan GPU acceleration). All inference happens on-device — no data leaves your machine. The model is downloaded from HuggingFace on first use.

| Model | Size | Context | Speed | License |
|-------|------|---------|-------|---------|
| `phi-3-mini` (Phi-3 Mini 128K Instruct Q4_K_M) | ~2.3GB | 128K tokens | ~30 tok/s (GPU) | MIT |

Phi-3 Mini 128K is a 3.8B parameter instruction-following model with a 128K token context window, meaning it can process full-length meeting transcripts (1+ hours) without truncation. It generates structured meeting notes including:
- Meeting overview and context
- Key topics discussed with details
- Decisions made
- Action items with owners
- Deadlines mentioned

The model runs with Vulkan GPU acceleration when available (NVIDIA, Intel Arc, AMD), falling back to CPU. Processing happens in the background — you can start a new recording while the previous session's summary is being generated.

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
├── model_integrity/mod.rs     # Hugging Face download checksum verification (SHA256 / Git-blob)
├── models/mod.rs              # Model metadata, download management
├── notifications/mod.rs       # Desktop notifications via notify-send
├── onboarding/                # First-run setup wizard (GTK4)
├── practice/mod.rs            # Practice recording for onboarding
├── refinement/mod.rs          # Rule-based text cleanup
├── settings/mod.rs            # TOML config persistence
├── streaming/mod.rs           # Streaming transcription with sliding window
├── summarization/mod.rs       # Local Phi-3 summarization (llama.cpp, Vulkan)
├── tests.rs                   # Test suite (98 tests)
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
