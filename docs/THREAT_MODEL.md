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
│  $XDG_RUNTIME_DIR/.ydotool_socket - ydotoold socket (user-private, 0600) │
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
                                          GetConnectionUnixUser(caller)
                                          must equal the daemon's UID
                                          (same-user only; read-only
                                          methods are unrestricted)
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
- Sensitive D-Bus methods authorize the caller by **UID**: the daemon queries the caller's kernel-reported UID (`GetConnectionUnixUser`) and only accepts callers running as the same user as the daemon
- Read-only methods (`GetStatus`, `GetAutoTranscriptionEnabled`) remain open
- Process-name (`/proc/PID/comm`) matching was removed: a name is not an identity — it is spoofable, and legitimate control uses the generic `gdbus`/`dbus-send` tools, so a name allowlist admitted essentially any caller by construction

**Residual risk:** The daemon is on the session bus, which is shared by all processes of the current user. The UID check is non-spoofable and blocks other users, but it cannot distinguish a *malicious process running as the same user* — such a process is inside the trust boundary by definition on a session bus (the same limitation applies to other GNOME session services). Fully isolating same-user peers would require moving control off the shared session bus onto a private, filesystem-permission-restricted socket; even then, a same-UID attacker could open that socket, so it raises the bar rather than closing the gap. Tracked as future work.

### T3: ydotool Socket Exposure / Input Injection
**STRIDE:** Tampering, Information Disclosure, Elevation of Privilege
**Description:** ydotoold exposes a control socket that injects synthetic input (keystrokes/mouse) via `/dev/uinput`. If that socket is world-accessible, any local process can `connect()` to it and inject arbitrary input into the session, or squat the path to intercept insertion. No race or timing is required — a plain `connect()` suffices.
**Likelihood:** High (any local process can connect to a world-writable socket)
**Impact:** High (arbitrary keystroke/input injection; interception of inserted text)
**Mitigation implemented:**
- ydotoold runs as a **per-user** systemd service with its socket in `$XDG_RUNTIME_DIR` (a 0700 user-owned directory), created with `--socket-perm=0600` — only the owning user can open it
- The previous world-writable configuration was removed entirely: the `/tmp/.ydotool_socket` path, the `--socket-perm=0666` flag, and the systemd drop-in that chmod'd the socket to 0666 are all gone. Package upgrades disable/remove any pre-existing *system* ydotoold service and delete the stale `/tmp` socket
- The daemon no longer falls back to a world-accessible socket path: it only uses `$YDOTOOL_SOCKET` or the per-user `$XDG_RUNTIME_DIR` socket, so it cannot be tricked into using a squatted world-writable socket

**Residual risk:** A process running as the same user can still connect to the per-user socket — inherent to a user-owned IPC endpoint, and the same trust-boundary limitation as T2. This is the intended boundary: input injection is scoped to the user's own processes rather than any local user. (Note: the earlier "0666 via ExecStartPost" line was itself the vulnerability, not a mitigation.)

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
- Storage is opt-in: transcription history (`history_enabled`) and auto-save (`auto_save_transcriptions`) are both **off by default**, so nothing is persisted unless the user turns it on
- Data stored in user-private directories (`~/.config/`, `~/.local/share/`)

**Residual risk:** No encryption at rest once the user enables storage. Consider adding optional encryption for sensitive transcription storage.

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
**Likelihood:** Medium (supply-chain: a compromised/poisoned CDN, a caching proxy, or an on-path attacker can substitute a model — "requires MITM on HTTPS" understated this) — reduced to Low in practice by the checksum verification below
**Impact:** High (a substituted model yields attacker-influenced transcripts/summaries that may then be auto-inserted — see T1 — and untrusted weights are parsed by native whisper.cpp/llama.cpp code)
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
- D-Bus caller authorization limits callers to the same user (UID check)

**Residual risk:** No rate limiting. A same-user caller could still flood.

### T11: Silent Auto-Transcription Activation
**STRIDE:** Information Disclosure, Spoofing
**Description:** When auto-transcription is enabled, FrogScribe watches PulseAudio/PipeWire for another application opening a microphone source (`pactl subscribe`) and automatically starts recording — with no explicit user gesture (see the §3.3 data flow). A local application could open a mic source specifically to induce FrogScribe to start recording (surveillance), and the activation decision relies on source-output metadata (app name/PID) that a local process can influence or spoof.
**Likelihood:** Low (auto-transcription is off by default; requires the user to opt in)
**Impact:** High (unintended or attacker-induced recording of microphone audio)
**Mitigation implemented:**
- Auto-transcription is **off by default** (opt-in)
- FrogScribe filters out its own process's source-outputs so it does not self-trigger
- Recording always raises a visible indicator (pill and/or top-bar overlay), so activation is not silent to an attentive user
- VAD, when enabled (`vad_enabled`, on by default but optional), auto-stops after a configurable silence window (default 30s), bounding how long audio is captured; with VAD off, recording continues until the user stops it
- The user can pause auto-transcription via the `SetAutoTranscriptionPaused` D-Bus method (now UID-authorized — see T2)
- Captured audio is held in memory only; it is not written to disk unless history/auto-save is enabled (both off by default — see T6)

**Residual risk:** While enabled, any local process that opens a mic source can trigger activation; the user sees the indicator but a short window of audio may be captured before they react. Activation keys off app metadata that a local process can spoof, so allow/deny heuristics based on the triggering app are not a strong control.

---

## 6. What Are We Going to Do About It?

### Mitigations Already Implemented

| Threat | Mitigation | Status |
|--------|-----------|--------|
| T1 | Control character sanitization | ✅ Implemented |
| T1 | Window picker (user confirms target) | ✅ Implemented (default on) |
| T2 | D-Bus caller authorization by UID (same-user only) | ✅ Implemented |
| T3 | Per-user ydotoold socket in `$XDG_RUNTIME_DIR` (0600); removed world-writable `/tmp` socket | ✅ Implemented |
| T5 | Default to TypeEveryCharacter mode | ✅ Implemented |
| T6 | Opt-in storage: history and auto-save both off by default | ✅ Implemented |
| T11 | Auto-transcription off by default; self-filter; visible recording indicator | ✅ Implemented |
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
3. Is same-UID D-Bus authorization sufficient, or should sensitive control move to a private permission-restricted socket to reduce the same-user attack surface?

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
