Name:           frogscribe
Version:        0.2.1
Release:        1%{?dist}
Summary:        Voice dictation for Linux GNOME — press a hotkey, speak, text appears at cursor

License:        Apache-2.0
URL:            https://github.com/awslabs/frogscribe
Source0:        frogscribe-%{version}.tar.gz

BuildRequires:  rust >= 1.70
BuildRequires:  cargo
BuildRequires:  clang-devel
BuildRequires:  gtk4-devel
BuildRequires:  openssl-devel
BuildRequires:  pango-devel
BuildRequires:  gdk-pixbuf2-devel
BuildRequires:  cairo-devel
BuildRequires:  glib2-devel
BuildRequires:  atk-devel
BuildRequires:  gtk-layer-shell-devel
BuildRequires:  openblas-devel
BuildRequires:  vulkan-headers
BuildRequires:  vulkan-loader-devel

Requires:       pulseaudio-utils
Requires:       ydotool
Requires:       ffmpeg-free
Requires:       libnotify
Requires:       gtk-layer-shell

Requires:       gnome-shell

Recommends:     python3-pyannote-audio

%description
FrogScribe is a voice dictation application for Linux GNOME desktops. Press a
global hotkey, speak, and your words are transcribed using on-device AI
(whisper.cpp) and inserted at the cursor position. Features include
hold-to-talk, streaming transcription, long-form dictation, text
refinement, speaker diarization, and a system tray indicator.

%prep
%autosetup -n %{name}-%{version}

%build
export BLAS_INCLUDE_DIRS=/usr/include/openblas
cargo build --release

%install
install -Dm755 target/release/%{name} %{buildroot}%{_bindir}/%{name}
install -Dm644 resources/frogscribe.desktop %{buildroot}%{_datadir}/applications/%{name}.desktop
install -Dm644 resources/frogscribe-48.png %{buildroot}%{_datadir}/icons/hicolor/48x48/apps/frogscribe.png
install -Dm644 resources/frogscribe-128.png %{buildroot}%{_datadir}/icons/hicolor/128x128/apps/frogscribe.png
install -Dm644 gnome-extension/metadata.json %{buildroot}%{_datadir}/gnome-shell/extensions/frogscribe@frogscribe.app/metadata.json
install -Dm644 gnome-extension/extension.js %{buildroot}%{_datadir}/gnome-shell/extensions/frogscribe@frogscribe.app/extension.js
install -Dm644 gnome-extension/stylesheet.css %{buildroot}%{_datadir}/gnome-shell/extensions/frogscribe@frogscribe.app/stylesheet.css
install -Dm644 resources/frogscribe-symbolic.svg %{buildroot}%{_datadir}/gnome-shell/extensions/frogscribe@frogscribe.app/frogscribe-symbolic.svg
install -Dm644 resources/ydotool-socket-perms.conf %{buildroot}%{_unitdir}/ydotool.service.d/socket-perms.conf

%files
%license README.md
%{_bindir}/%{name}
%{_datadir}/applications/%{name}.desktop
%{_datadir}/icons/hicolor/48x48/apps/frogscribe.png
%{_datadir}/icons/hicolor/128x128/apps/frogscribe.png
%{_datadir}/gnome-shell/extensions/frogscribe@frogscribe.app/
%{_unitdir}/ydotool.service.d/socket-perms.conf

%changelog
* Mon Jul 27 2026 Tom Callaway <spotaws@amazon.com> - 0.2.1-1
- Local summarization with BART models (distilbart-cnn-12-6, bart-large-cnn)
- CLI: --summarize, --output, --summary-output flags
- Recording indicator color syncs with accent color setting
- Set FrogScribe window icon for all GTK4 windows
- Added ONNX Runtime and tokenizers dependencies
- 92 tests

* Tue Jul 15 2026 Tom Callaway <spotaws@amazon.com> - 0.2.0-1
- Updated panel icon
- README cleanup and GTK4 highlight
- Removed auto_start dead code

* Fri Jul 10 2026 Tom Callaway <spotaws@amazon.com> - 0.1.9-1
- Migrated from GTK3 to GTK4 (resolves RUSTSEC-2024-0394 glib vulnerability)
- Updated glib from 0.18 to 0.20
- Removed unmaintained gtk3 dependency
- Removed libappindicator dependency

* Fri Jul 10 2026 Tom Callaway <spotaws@amazon.com> - 0.1.8-1
- Renamed project to FrogScribe
- Desktop audio capture (mix monitor source with mic for meeting transcription)
- Removed analytics module entirely
- Security: D-Bus caller authentication
- Security: sanitize control characters in transcribed text
- Performance: streaming skip-when-idle, reduced animation redraw rate
- Code cleanup: removed dead modules, fixed warnings
- Removed AppIndicator tray (extension-only)

* Tue Jun 30 2026 Tom Callaway <spotaws@amazon.com> - 0.1.7-1
- Window picker rendered in compositor with Clutter.Clone (works on GNOME 46+50)
- Colored text in color selector using Pango markup
- Renamed Streaming setting to "Streaming (Live Preview)"
- Fixed ydotool text insertion (use ydotool type)
- Removed old AppIndicator tray (extension only)

* Thu Jun 18 2026 Tom Callaway <spotaws@amazon.com> - 0.1.6-1
- Window picker with live window thumbnails
- Fix ydotool text insertion (use ydotool type instead of key scancodes)
- Remove duplicate AppIndicator tray (extension-only indicator)
- Mic status display in onboarding with unmute button
- Debian packaging: udev rules for /dev/uinput, ydotoold service
- Live preview tabbed singleton window

* Tue Jun 17 2026 Tom Callaway <spotaws@amazon.com> - 0.1.5-1
- Window picker for paste target selection
- Auto-transcription with mic activity detection and VAD
- Auto-save transcriptions with context headers
- Live preview: tabbed singleton window, sliding window fix for repetition
- Pill indicator moved to GNOME Shell extension (always on top)
- Model download UI in settings
- Debian packaging support
- Streaming live preview improvements (scroll, save button)

* Tue Jun 02 2026 Tom Callaway <spotaws@amazon.com> - 0.1.4-1
- Appearance settings: independent Pill and TopBar toggles
- TopBar indicator drawn by GNOME Shell extension (works on Wayland)
- Settings reload on each recording start
- Default hotkey changed to Ctrl+Shift+Space (avoid GNOME conflict)
- Fix settings deserialization for legacy configs (serde defaults)
- Add tooltips to all settings options
- Wire recording indicator overlay to appearance settings

* Thu May 28 2026 Tom Callaway <spotaws@amazon.com> - 0.1.3-1
- Use FrogScribe app icon in panel, tray, and onboarding
- Fix ydotool socket permissions via systemd drop-in
- Add Long-Form Dictation and Settings to extension menu
- Fix Quit not working (extension no longer respawns daemon)

* Thu May 28 2026 Tom Callaway <spotaws@amazon.com> - 0.1.2-1
- Fix ydotool socket detection (auto-detect user/system service socket)
- Startup check warns user if ydotoold is misconfigured
- Package GNOME Shell extension for panel indicator

* Wed May 27 2026 Tom Callaway <spotaws@amazon.com> - 0.1.1-1
- Port all UI to GTK3 (fix gdk_display_manager_get crash)
- Remove GTK4 dependency

* Wed May 27 2026 Tom Callaway <spotaws@amazon.com> - 0.1.0-1
- Initial package
