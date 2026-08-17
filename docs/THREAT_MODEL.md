# FrogScribe Threat Model

**Document Version:** 1.1
**Date:** July 15, 2026 (updated August 17, 2026)
**Author:** Tom Callaway
**Status:** Initial Review

---

## 1. System Description

### What are we working on?

FrogScribe is a voice dictation application for Linux GNOME desktops. It captures audio from the microphone (and optionally desktop audio), transcribes it on-device using whisper.cpp, and inserts the resulting text into the user's chosen application window.

**Key properties:**
- Runs entirely on-device — no cloud transcription services
- Operates as a user-space daemon managed by a GNOME Shell extension
- Processes real-time audio from microphone and/or system audio monitor sources
- Inserts text into arbitrary application windows via ydotool (uinput)
- Exposes a D-Bus session bus interface for control
- Stores configuration, transcription history, and saved transcripts locally

### Components

| Component | Description | Language |
|-----------|-------------|----------|
| `frogscribe` daemon | Main process: audio capture, transcription, insertion, settings | Rust |
| GNOME Shell Extension | Panel indicator, pill/topbar overlays, window picker, D-Bus bridge | JavaScript (GJS) |
| ydotoold | System service for input injection (external dependency) | C |
| whisper.cpp | On-device speech-to-text engine (linked via whisper-rs) | C++ |

### Trust Boundaries

1. **User session ↔ System services**: ydotoold runs as root; FrogScribe communicates via Unix socket
2. **FrogScribe daemon ↔ GNOME Shell**: D-Bus session bus communication
3. **FrogScribe daemon ↔ other session processes**: D-Bus exposure on session bus
4. **Audio input ↔ transcription engine**: Untrusted audio content processed by whisper.cpp
5. **Transcription output ↔ target application**: Text inserted into arbitrary apps via uinput

---

## 2. Data Flow Diagram

```
┌─────────────────────────────────────────────────────────────────────┐
│                         User Session                                 │
│                                                                     │
│  ┌──────────┐     ┌─────────────────────┐     ┌──────────────────┐ │
│  │Microphone│────▶│                     │     │  GNOME Shell     │ │
│  └──────────┘     │   FrogScribe Daemon │◀───▶│  Extension       │ │
│                   │                     │D-Bus│  (panel icon,    │ │
│  ┌──────────┐     │  ┌───────────────┐  │     │   overlays,      │ │
│  │ Desktop  │────▶│  │ whisper.cpp   │  │     │   window picker) │ │
│  │ Audio    │     │  │ (transcribe)  │  │     └──────────────────┘ │
│  │ Monitor  │     │  └───────────────┘  │                          │
│  └──────────┘     │          │          │     ┌──────────────────┐ │
│                   │          ▼          │     │  Other D-Bus     │ │
│                   │  ┌───────────────┐  │     │  Clients         │ │
│                   │  │ Text Output   │  │◀ ─ ─│  (scripts, apps) │ │
│                   │  └───────┬───────┘  │     └──────────────────┘ │
│                   └──────────┼──────────┘                          │
│                              │                                      │
│                              ▼                                      │
│  ┌─────────────────────────────────────────────────────┐           │
│  │                    ydotoold                           │           │
│  │         (root process, uinput access)                │           │
│  └──────────────────────┬──────────────────────────────┘           │
│                          │                                          │
│                          ▼                                          │
│  ┌──────────────────────────────────────────────────────┐          │
│  │              Target Application Window                │          │
│  │         (receives typed/pasted text)                  │          │
│  └──────────────────────────────────────────────────────┘          │
│                                                                     │
│  ┌─────────────────────────────────────────────┐                   │
│  │              Local Storage                    │                   │
│  │  ~/.config/frogscribe/settings.toml          │                   │
│  │  ~/.local/share/frogscribe/history.json      │                   │
│  │  ~/.local/share/frogscribe/models/*.bin      │                   │
│  │  ~/.frogscribe/transcriptions/*.txt          │                   │
│  └─────────────────────────────────────────────┘                   │
└─────────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────────┐
│                      System Layer (root)                             │
│                                                                     │
│  /dev/uinput          - synthetic input device                      │
│  /dev/input/event*    - keyboard devices (read by evdev)            │
│  /tmp/.ydotool_socket - ydotoold communication socket               │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 3. Important Data Flows

### 3.1 Audio Capture → Transcription → Text Insertion

```
User speaks ──▶ parec (mic capture) ──▶ f32 PCM buffer (memory)
                                              │
                                              ▼
                                       whisper.cpp engine
                                              │
                                              ▼
                                       Raw transcription text
                                              │
                                              ▼
                                       Refinement (filler removal,
                                       capitalization, vocab)
                                              │
                                              ▼
                                       Sanitized text (control chars stripped)
                                              │
                                              ▼
                                    ┌─────────┴──────────┐
                                    │                    │
                              [auto_paste=true]    [auto_paste=false]
                                    │                    │
                                    ▼                    ▼
                           Window Picker OR        Copy to clipboard
                           direct ydotool type         only
                                    │
                                    ▼
                           ydotoold socket ──▶ /dev/uinput ──▶ Target app
```

### 3.2 D-Bus Control Flow

```
GNOME Extension ──▶ D-Bus session bus ──▶ com.frogscribe.Daemon
                                                │
                                          [Caller auth check]
                                          Verify /proc/PID/comm
                                          matches allowed list:
                                          gnome-shell, frogscribe,
                                          gdbus, dbus-send
                                                │
                                                ▼
                                          Execute command
                                          (StartRecording, Quit, etc.)
```

### 3.3 Automatic Transcription Trigger

```
External app (Zoom, Teams) ──▶ PipeWire/PulseAudio ──▶ opens mic source
                                                              │
                                                              ▼
                                                    pactl subscribe event
                                                    (new source-output)
                                                              │
                                                              ▼
                                                    FrogScribe checks:
                                                    - Is it our own process? (filter)
                                                    - Is auto-transcription paused?
                                                    - Which app triggered it?
                                                              │
                                                              ▼
                                                    StartAutoTranscription
                                                    (begins recording + VAD monitoring)
```

### 3.4 Window Picker (Compositor-level)

```
Transcription complete ──▶ Daemon calls GetThumbnails (gdbus)
                                     │
                                     ▼
                           Extension renders Clutter.Clone overlay
                           (live window previews in compositor layer)
                                     │
                                     ▼
                           User clicks window ──▶ window ID returned
                                     │
                                     ▼
                           Extension activates window (focus)
                                     │
                                     ▼
                           Daemon inserts text via ydotool
```

---

## 4. Data Types Processed and Stored

| Data | Type | Storage | Sensitivity |
|------|------|---------|-------------|
| Audio samples (mic) | f32 PCM | Memory only (not persisted) | High — captures voice, ambient sound |
| Audio samples (desktop) | f32 PCM | Memory only | High — captures call participants |
| Transcription text | String | Memory, optionally saved to disk | High — contains spoken content |
| Transcription history | JSON | `~/.local/share/frogscribe/history.json` | High — all past transcriptions |
| Auto-saved transcripts | Text files | `~/.frogscribe/transcriptions/` | High — meeting transcripts |
| Whisper model files | Binary (GGML) | `~/.local/share/frogscribe/models/` | Low — public model weights |
| Configuration | TOML | `~/.config/frogscribe/settings.toml` | Low — user preferences |
| Window titles | Strings | Transient (D-Bus response) | Medium — may contain document names |
| Keyboard events | evdev events | Memory only | High — all keystrokes visible |

**Data classification:** FrogScribe processes **Customer Content** (voice recordings, transcribed text) and **Customer Metadata** (window titles, app names, timestamps).

---

## 5. What Can Go Wrong? (Threats)

### T1: Adversarial Audio Injection
**STRIDE:** Spoofing, Tampering
**Description:** An attacker plays crafted audio (e.g., via a website or nearby speaker) that whisper transcribes as malicious commands. If auto-paste is enabled and a terminal is focused, this could result in command execution.
**Likelihood:** Medium
**Impact:** High (arbitrary command execution)
**Mitigation implemented:**
- Control character sanitization strips non-printable/control characters before insertion, with the single exception of newline (`\n`), which is preserved to support multi-line and long-form dictation (tab and all other control characters are removed)
- Window picker (default enabled) requires user confirmation of target
- Auto-transcription is off by default

**Residual risk:** Newline is intentionally preserved for multi-line dictation, and a newline acts as Enter when inserted. So if the user disables the window picker and has a terminal focused, transcribed text could both execute (via the trailing/embedded newline) and contain shell-meaningful characters (`;`, `|`, `$`). The window picker (default on) requires the user to confirm the target window, which is the primary mitigation for this path.

### T2: Unauthenticated D-Bus Access
**STRIDE:** Elevation of Privilege
**Description:** A malicious process in the user session calls D-Bus methods to start recording (surveillance), inject text, or quit the daemon.
**Likelihood:** Medium
**Impact:** High (unauthorized recording, text injection)
**Mitigation implemented:**
- D-Bus caller authentication via `/proc/PID/comm` verification
- Only `gnome-shell`, `frogscribe`, `gdbus`, and `dbus-send` are allowed
- Read-only methods (`GetStatus`, `GetAutoTranscriptionEnabled`) remain open

**Residual risk:** An attacker could rename their binary to `gdbus` to bypass the check. Process name verification is not cryptographically strong.

### T3: ydotool Socket Hijacking
**STRIDE:** Tampering, Information Disclosure
**Description:** `/tmp/.ydotool_socket` is in a world-writable directory. A local attacker could race to create a malicious socket before ydotoold, intercepting all text insertion.
**Likelihood:** Low (requires local access + timing)
**Impact:** High (keystroke interception, input injection)
**Mitigation implemented:**
- Socket permissions set to 0666 via systemd ExecStartPost
- Prefer `XDG_RUNTIME_DIR` socket path when available

**Residual risk:** The `/tmp` socket path is inherently race-prone. Ideal fix would be moving socket to `/run/ydotool/` with restricted ownership.

### T4: Input Group Keylogging
**STRIDE:** Information Disclosure
**Description:** FrogScribe requires the `input` group to read evdev devices for hotkey detection. This grants read access to ALL input devices, enabling a compromised FrogScribe process to keylog.
**Likelihood:** Low (requires FrogScribe compromise first)
**Impact:** High (full keystroke capture)
**Mitigation implemented:**
- FrogScribe only reads from devices matching keyboard characteristics
- No keystrokes are logged or stored — only modifier+trigger combinations are evaluated

**Residual risk:** The `input` group permission is broader than needed. A dedicated approach (e.g., a small privileged helper for hotkey detection) would reduce scope.

### T5: Clipboard Exposure During Paste
**STRIDE:** Information Disclosure
**Description:** When using "PasteFullTranscript" insertion mode, transcribed text is placed on the clipboard for ~250ms. Other processes can read it.
**Likelihood:** Medium
**Impact:** Medium (sensitive transcription exposed to clipboard managers)
**Mitigation implemented:**
- Default insertion mode is "TypeEveryCharacter" (no clipboard use)
- Clipboard is restored after paste completes

**Residual risk:** Clipboard managers running in the session will capture the text. Users choosing "PasteFullTranscript" accept this tradeoff.

### T6: Local File Access to Transcription Data
**STRIDE:** Information Disclosure
**Description:** Transcription history and auto-saved files are stored with default user file permissions. Any process running as the user can read them.
**Likelihood:** High (any user-space process can access)
**Impact:** Medium (meeting transcripts, voice-to-text history)
**Mitigation implemented:**
- Files are only stored when user enables history/auto-save features
- Data stored in user-private directories (`~/.config/`, `~/.local/share/`)

**Residual risk:** No encryption at rest. Consider adding optional encryption for sensitive transcription storage.

### T7: Settings File Tampering
**STRIDE:** Tampering
**Description:** A malicious process modifies `settings.toml` to enable auto-transcription, auto-paste, and disable window picker — creating a surveillance + injection pipeline.
**Likelihood:** Low (requires write access to user config dir)
**Impact:** High (silent recording + auto-paste without user confirmation)
**Mitigation implemented:**
- Settings are re-read on each recording start (no persistent in-memory cache that could drift)

**Residual risk:** No integrity protection on the settings file. If an attacker has write access to the user's home directory, they have broader access anyway.

### T8: Window Title Information Disclosure
**STRIDE:** Information Disclosure
**Description:** The `org.frogscribe.Windows` D-Bus service exposes window titles to callers. Window titles may contain sensitive information (document names, URLs, email subjects).
**Likelihood:** Medium
**Impact:** Low (information disclosure to session-local processes)
**Mitigation implemented:**
- The D-Bus service is session-scoped (not system bus)
- Only accessible to processes in the same user session

**Residual risk:** Any process in the user session can enumerate window titles. This is consistent with GNOME's own `org.gnome.Shell.Introspect` behavior.

### T9: Model Download Integrity
**STRIDE:** Tampering
**Description:** Model files (whisper GGML weights and summarization models) are downloaded from Hugging Face over the internet. A MITM attacker, a compromised/poisoned CDN, or a malicious caching proxy could substitute a tampered or corrupted model.
**Likelihood:** Low (requires MITM on HTTPS connection)
**Impact:** Medium (corrupted transcription, potential model-level exploits)
**Mitigation implemented:**
- Downloads use HTTPS (reqwest with TLS)
- Every downloaded file is verified against the authoritative content digest that Hugging Face publishes via its TLS-protected metadata API (`paths-info`), fetched before the download begins:
  - Git-LFS tracked files (all large model weights) are verified against the published SHA256 (`lfs.oid`)
  - Non-LFS files (e.g. `tokenizer.json`) are verified against the published Git blob id (`SHA1("blob " + len + "\0" + content)`)
- The exact file size published by the API is also checked (exact-equality)
- On any size or digest mismatch the file is deleted and the operation aborts with an error flagging possible tampering (`src/model_integrity/`)

**Residual risk:** Verification trusts the Hugging Face metadata API (TLS to `huggingface.co`). It defends against a tampered file CDN, cache, or on-path attacker, but not against Hugging Face itself serving malicious content, nor against a simultaneous compromise of both the API and the file CDN using a valid `huggingface.co` certificate. Pinning downloads to a specific commit revision would further narrow the window for upstream content changes.

### T10: Denial of Service via D-Bus Flooding
**STRIDE:** Denial of Service
**Description:** A malicious process rapidly calls D-Bus methods (ToggleRecording, StartLongForm) to degrade performance or make the system unusable.
**Likelihood:** Low
**Impact:** Low (user annoyance, CPU usage)
**Mitigation implemented:**
- D-Bus caller authentication limits callers to known processes

**Residual risk:** No rate limiting. An allowed caller (or one masquerading as `gdbus`) could still flood.

---

## 6. What Are We Going to Do About It?

### Mitigations Already Implemented

| Threat | Mitigation | Status |
|--------|-----------|--------|
| T1 | Control character sanitization | ✅ Implemented |
| T1 | Window picker (user confirms target) | ✅ Implemented (default on) |
| T2 | D-Bus caller authentication via /proc/PID/comm | ✅ Implemented |
| T3 | Socket permissions via systemd drop-in | ✅ Implemented |
| T5 | Default to TypeEveryCharacter mode | ✅ Implemented |
| T6 | Opt-in storage (history/auto-save off by default) | ✅ Implemented |
| T9 | SHA256 / Git-blob digest + size verification of model downloads (Hugging Face API) | ✅ Implemented |

### Accepted Risks

| Threat | Rationale |
|--------|-----------|
| T4 | `input` group is required for evdev hotkey detection; no alternative on Linux without a privileged helper |
| T7 | If attacker has write access to `~/.config/`, they have equivalent access to `~/.ssh/`, `~/.gnupg/`, etc. |
| T8 | Consistent with GNOME Shell's own behavior; session-local only |
| T10 | Low impact; auth check prevents most abuse |

### Future Improvements (Recommended)

| Priority | Improvement |
|----------|-------------|
| Medium | Move ydotool socket to `XDG_RUNTIME_DIR` by default |
| Low | Pin model downloads to a specific Hugging Face commit revision |
| Low | Add optional encryption-at-rest for transcription history |
| Low | Implement D-Bus rate limiting |
| Low | Consider a minimal privileged helper for hotkey detection (avoid broad `input` group) |

---

## 7. Did We Do a Good Enough Job?

### Security Review History

| Date | Reviewer | Scope | Findings |
|------|----------|-------|----------|
| 2026-07-06 | Automated security scan | Full codebase | 15 findings (2 Critical, 4 High, 5 Medium, 4 Low) |
| 2026-07-06 | Manual remediation | Critical + High findings | Control char sanitization, D-Bus auth implemented |
| 2026-07-10 | Dependency audit | Cargo dependencies | GTK4 migration resolved RUSTSEC-2024-0394 |
| 2026-08-17 | Manual remediation | Model download supply chain | SHA256 / Git-blob digest + size verification for all model downloads (T9) |

### Open Questions

1. Should we warn users when auto-paste targets a terminal emulator?
2. Should we add a "sensitive mode" that disables all storage/history for confidential meetings?
3. Is the `/proc/PID/comm` D-Bus auth check sufficient, or do we need cryptographic peer verification?

### Review Cadence

This threat model should be reviewed when:
- New D-Bus methods are added
- Audio processing pipeline changes
- New storage locations are introduced
- External dependencies are updated
- The application is deployed to new environments

---

*Document generated: July 15, 2026*
*Next review due: October 15, 2026 (quarterly)*
