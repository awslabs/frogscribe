// SPDX-License-Identifier: Apache-2.0
import GObject from 'gi://GObject';
import St from 'gi://St';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

const DBUS_NAME = 'com.frogscribe.Daemon';
const DBUS_PATH = '/com/frogscribe/Daemon';
const DBUS_IFACE = `
<node>
  <interface name="com.frogscribe.Daemon">
    <method name="ToggleRecording">
      <arg type="s" direction="out"/>
    </method>
    <method name="GetStatus">
      <arg type="s" direction="out"/>
    </method>
    <method name="StartLongForm">
      <arg type="s" direction="out"/>
    </method>
    <method name="StopLongForm">
      <arg type="s" direction="out"/>
    </method>
    <method name="SetAutoTranscriptionPaused">
      <arg type="b" direction="in" name="paused"/>
      <arg type="s" direction="out"/>
    </method>
    <method name="GetAutoTranscriptionEnabled">
      <arg type="s" direction="out"/>
    </method>
    <method name="Quit">
      <arg type="s" direction="out"/>
    </method>
    <signal name="StatusChanged">
      <arg type="s" name="status"/>
    </signal>
  </interface>
</node>`;

const FrogScribeProxy = Gio.DBusProxy.makeProxyWrapper(DBUS_IFACE);

const FrogScribeIndicator = GObject.registerClass(
class FrogScribeIndicator extends PanelMenu.Button {
    _init(extensionPath) {
        super._init(0.0, 'FrogScribe Voice Dictation');

        this._recording = false;
        this._spawnAttempted = false;
        this._userQuit = false;
        this._extensionPath = extensionPath;

        // Load custom icon from extension directory
        const iconFile = Gio.File.new_for_path(`${extensionPath}/frogscribe-symbolic.svg`);
        const gicon = new Gio.FileIcon({file: iconFile});

        // Panel icon in a box (required for proper size allocation)
        const box = new St.BoxLayout({style_class: 'panel-status-indicators-box'});
        this._icon = new St.Icon({
            gicon: gicon,
            style_class: 'system-status-icon',
        });
        box.add_child(this._icon);
        this.add_child(box);

        // Menu items
        this._statusItem = new PopupMenu.PopupMenuItem('FrogScribe — Connecting...', {reactive: false});
        this.menu.addMenuItem(this._statusItem);

        this.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());

        this._toggleItem = new PopupMenu.PopupMenuItem('Toggle Recording');
        this._toggleItem.connect('activate', () => this._toggleRecording());
        this.menu.addMenuItem(this._toggleItem);

        this._longFormItem = new PopupMenu.PopupMenuItem('Start Long-Form Dictation');
        this._longFormItem.connect('activate', () => this._startLongForm());
        this.menu.addMenuItem(this._longFormItem);

        this._pauseAutoItem = new PopupMenu.PopupMenuItem('Pause Auto-Transcription');
        this._autoTranscriptionPaused = false;
        this._pauseAutoItem.connect('activate', () => {
            this._autoTranscriptionPaused = !this._autoTranscriptionPaused;
            this._pauseAutoItem.label.text = this._autoTranscriptionPaused
                ? 'Resume Auto-Transcription'
                : 'Pause Auto-Transcription';
            if (this._proxy)
                this._proxy.SetAutoTranscriptionPausedRemote(this._autoTranscriptionPaused, ([result]) => {});
        });
        this.menu.addMenuItem(this._pauseAutoItem);

        // Gray out pause item when auto-transcription is not enabled
        this.menu.connect('open-state-changed', (menu, open) => {
            if (open && this._proxy) {
                this._proxy.GetAutoTranscriptionEnabledRemote(([result]) => {
                    this._pauseAutoItem.setSensitive(result === 'true');
                });
            }
        });

        this.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());

        const settingsItem = new PopupMenu.PopupMenuItem('Settings...');
        settingsItem.connect('activate', () => {
            GLib.spawn_command_line_async('frogscribe --settings');
        });
        this.menu.addMenuItem(settingsItem);

        const historyItem = new PopupMenu.PopupMenuItem('Transcription History');
        historyItem.connect('activate', () => {
            GLib.spawn_command_line_async('frogscribe --history');
        });
        this.menu.addMenuItem(historyItem);

        this.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());

        const quitItem = new PopupMenu.PopupMenuItem('Quit FrogScribe');
        quitItem.connect('activate', () => this._quit());
        this.menu.addMenuItem(quitItem);

        const restartItem = new PopupMenu.PopupMenuItem('Restart FrogScribe');
        restartItem.connect('activate', () => {
            this._userQuit = false;
            if (this._proxy)
                this._proxy.QuitRemote(([result]) => {});
        });
        this.menu.addMenuItem(restartItem);

        // Connect to D-Bus
        this._proxy = null;
        this._signalId = null;
        this._watchId = Gio.bus_watch_name(
            Gio.BusType.SESSION,
            DBUS_NAME,
            Gio.BusNameWatcherFlags.NONE,
            () => this._onDaemonAppeared(),
            () => this._onDaemonVanished(),
        );
    }

    _onDaemonAppeared() {
        this._spawnAttempted = false;
        try {
            this._proxy = new FrogScribeProxy(Gio.DBus.session, DBUS_NAME, DBUS_PATH);
            this._signalId = this._proxy.connectSignal('StatusChanged', (_proxy, _sender, [status]) => {
                this._updateStatus(status);
            });
            // Get initial status
            this._proxy.GetStatusRemote(([result]) => {
                this._updateStatus(result);
            });
        } catch (e) {
            log(`FrogScribe: Failed to connect to daemon: ${e.message}`);
        }
    }

    _onDaemonVanished() {
        this._proxy = null;
        this._signalId = null;
        this._updateStatus('offline');
        this._spawnDaemon();
    }

    _spawnDaemon() {
        if (this._spawnAttempted || this._userQuit)
            return;
        this._spawnAttempted = true;
        try {
            const [success] = GLib.spawn_command_line_async('frogscribe');
            if (success)
                log('FrogScribe: spawned daemon');
        } catch (e) {
            log(`FrogScribe: failed to spawn daemon: ${e.message}`);
        }
    }

    _updateStatus(status) {
        const isRecording = status.startsWith('recording');
        this._recording = isRecording;

        if (status === 'offline') {
            this._statusItem.label.text = 'FrogScribe — Not Running';
            this._icon.opacity = 128;
            this._icon.remove_style_class_name('frogscribe-recording');
            this._hideTopBar();
            this._hidePill();
        } else if (isRecording) {
            this._statusItem.label.text = 'FrogScribe — Recording';
            this._icon.opacity = 255;
            this._icon.add_style_class_name('frogscribe-recording');
            const parts = status.split(':');
            const color = parts[1] || 'teal';
            const topbarEnabled = parts[2] === '1';
            const pillEnabled = parts[3] === '1';
            if (topbarEnabled)
                this._showTopBar(color);
            else
                this._hideTopBar();
            if (pillEnabled)
                this._showPill(color);
            else
                this._hidePill();
        } else {
            this._statusItem.label.text = 'FrogScribe — Ready';
            this._icon.opacity = 255;
            this._icon.remove_style_class_name('frogscribe-recording');
            this._hideTopBar();
            this._hidePill();
        }
    }

    _showTopBar(color) {
        const colorMap = {
            teal: 'rgba(20,184,166,0.85)',
            blue: 'rgba(59,130,246,0.85)',
            purple: 'rgba(139,92,246,0.85)',
            pink: 'rgba(236,72,153,0.85)',
            orange: 'rgba(249,115,22,0.85)',
            green: 'rgba(34,197,94,0.85)',
            yellow: 'rgba(234,179,8,0.85)',
        };
        const isRainbow = color === 'rainbow';
        const bg = isRainbow ? null : (colorMap[color] || colorMap.teal);

        if (!this._topBar) {
            this._topBar = new St.Widget({
                reactive: false,
                x: 0,
                y: Main.panel.height,
                width: Main.layoutManager.primaryMonitor.width,
                height: 6,
            });
            Main.layoutManager.addTopChrome(this._topBar);
        }
        if (isRainbow) {
            this._topBar.set_style('background-color: transparent;');
            this._topBar.remove_all_children();
            const colors = ['rgba(255,0,0,0.85)', 'rgba(255,127,0,0.85)', 'rgba(255,255,0,0.85)',
                           'rgba(0,200,0,0.85)', 'rgba(0,100,255,0.85)', 'rgba(75,0,130,0.85)', 'rgba(148,0,211,0.85)'];
            const segWidth = Math.floor(this._topBar.width / colors.length);
            for (let i = 0; i < colors.length; i++) {
                const seg = new St.Widget({
                    style: `background-color: ${colors[i]};`,
                    x: i * segWidth,
                    y: 0,
                    width: i === colors.length - 1 ? this._topBar.width - i * segWidth : segWidth,
                    height: 6,
                });
                this._topBar.add_child(seg);
            }
        } else {
            this._topBar.remove_all_children();
            this._topBar.set_style(`background-color: ${bg};`);
        }
        this._topBar.show();
        this._topBar.ease({opacity: 255, duration: 300, mode: imports.gi.Clutter.AnimationMode.EASE_OUT_QUAD});
        this._startPulse();
    }

    _hideTopBar() {
        if (this._topBar) {
            this._stopPulse();
            this._topBar.ease({
                opacity: 0,
                duration: 200,
                mode: imports.gi.Clutter.AnimationMode.EASE_IN_QUAD,
                onComplete: () => this._topBar.hide(),
            });
        }
    }

    _showPill(color) {
        const colorMap = {
            teal: 'rgba(20,184,166,0.8)',
            blue: 'rgba(59,130,246,0.8)',
            purple: 'rgba(139,92,246,0.8)',
            pink: 'rgba(236,72,153,0.8)',
            orange: 'rgba(249,115,22,0.8)',
            green: 'rgba(34,197,94,0.8)',
            yellow: 'rgba(234,179,8,0.8)',
        };
        const isRainbow = color === 'rainbow';
        const bg = colorMap[color] || colorMap.teal;
        const pillWidth = 200;
        const pillHeight = 36;
        const monitor = Main.layoutManager.primaryMonitor;
        const x = Math.floor((monitor.width - pillWidth) / 2);
        const y = Main.panel.height + 12;

        if (!this._pill) {
            this._pill = new St.BoxLayout({
                reactive: false,
                x: x,
                y: y,
                width: pillWidth,
                height: pillHeight,
                style_class: 'frogscribe-pill',
            });
            const label = new St.Label({
                text: '🎙 Recording...',
                y_align: imports.gi.Clutter.ActorAlign.CENTER,
                x_align: imports.gi.Clutter.ActorAlign.CENTER,
                x_expand: true,
            });
            label.set_style('color: white; font-weight: bold; font-size: 13px;');
            this._pill.add_child(label);
            Main.layoutManager.addTopChrome(this._pill);
        }
        if (isRainbow) {
            this._pill.set_style('border-radius: 18px; background-color: rgba(255,0,0,0.8);');
            this._startRainbowPill();
        } else {
            this._stopRainbowPill();
            this._pill.set_style(`border-radius: 18px; background-color: ${bg};`);
        }
        this._pill.show();
        this._pill.ease({opacity: 255, duration: 300, mode: imports.gi.Clutter.AnimationMode.EASE_OUT_QUAD});
    }

    _hidePill() {
        if (this._pill) {
            this._stopRainbowPill();
            this._pill.ease({
                opacity: 0,
                duration: 200,
                mode: imports.gi.Clutter.AnimationMode.EASE_IN_QUAD,
                onComplete: () => this._pill.hide(),
            });
        }
    }

    _startRainbowPill() {
        this._stopRainbowPill();
        let hue = 0;
        this._rainbowLoop = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 50, () => {
            if (!this._pill || !this._pill.visible)
                return GLib.SOURCE_REMOVE;
            hue = (hue + 5) % 360;
            const h = hue / 60;
            const i = Math.floor(h);
            const f = h - i;
            const q = 1 - f;
            let r, g, b;
            switch (i % 6) {
                case 0: r=1; g=f; b=0; break;
                case 1: r=q; g=1; b=0; break;
                case 2: r=0; g=1; b=f; break;
                case 3: r=0; g=q; b=1; break;
                case 4: r=f; g=0; b=1; break;
                default: r=1; g=0; b=q;
            }
            const rgba = `rgba(${Math.round(r*255)},${Math.round(g*255)},${Math.round(b*255)},0.8)`;
            this._pill.set_style(`border-radius: 18px; background-color: ${rgba};`);
            return GLib.SOURCE_CONTINUE;
        });
    }

    _stopRainbowPill() {
        if (this._rainbowLoop) {
            GLib.source_remove(this._rainbowLoop);
            this._rainbowLoop = null;
        }
    }

    _showWindowPicker(invocation) {
        const Clutter = imports.gi.Clutter;
        const monitor = Main.layoutManager.primaryMonitor;
        const actors = global.get_window_actors().filter(a => {
            const w = a.get_meta_window();
            return w && w.get_window_type() === 0;
        });
        if (actors.length === 0) {
            invocation.return_value(new GLib.Variant('(s)', ['']));
            return;
        }

        // Create overlay background
        const overlay = new St.Widget({
            reactive: true,
            x: 0, y: 0,
            width: monitor.width,
            height: monitor.height,
            style: 'background-color: rgba(0,0,0,0.7);',
        });

        // Title
        const title = new St.Label({
            text: 'Select window to paste into:',
            style: 'color: white; font-size: 16px; font-weight: bold;',
            x: Math.round(monitor.width / 2 - 150),
            y: 30,
        });
        overlay.add_child(title);

        // Layout window clones in a grid
        const padding = 20;
        const cols = Math.ceil(Math.sqrt(actors.length));
        const rows = Math.ceil(actors.length / cols);
        const cellW = Math.floor((monitor.width - padding * (cols + 1)) / cols);
        const cellH = Math.floor((monitor.height - 100 - padding * (rows + 1)) / rows);

        actors.forEach((actor, i) => {
            const win = actor.get_meta_window();
            const col = i % cols;
            const row = Math.floor(i / cols);
            const x = padding + col * (cellW + padding);
            const y = 70 + padding + row * (cellH + padding);

            // Container for clone + label
            const container = new St.Widget({
                reactive: true,
                x: x, y: y,
                width: cellW, height: cellH,
                style: 'border: 2px solid rgba(255,255,255,0.3); border-radius: 8px;',
            });

            // Clone the window actor
            const rect = win.get_frame_rect();
            const scale = Math.min((cellW - 10) / rect.width, (cellH - 30) / rect.height);
            const clone = new Clutter.Clone({
                source: actor,
                x: 5,
                y: 5,
                width: Math.round(rect.width * scale),
                height: Math.round(rect.height * scale),
            });
            container.add_child(clone);

            // Window title label
            const label = new St.Label({
                text: win.get_title() || win.get_wm_class() || 'Window',
                style: 'color: white; font-size: 11px;',
                x: 5,
                y: cellH - 22,
            });
            label.set_width(cellW - 10);
            container.add_child(label);

            // Click handler
            container.connect('button-press-event', () => {
                Main.layoutManager.removeChrome(overlay);
                overlay.destroy();
                invocation.return_value(new GLib.Variant('(s)', [win.get_id().toString()]));
                return Clutter.EVENT_STOP;
            });

            // Hover highlight
            container.connect('enter-event', () => {
                container.set_style('border: 2px solid rgba(255,255,255,1); border-radius: 8px; background-color: rgba(255,255,255,0.1);');
            });
            container.connect('leave-event', () => {
                container.set_style('border: 2px solid rgba(255,255,255,0.3); border-radius: 8px;');
            });

            overlay.add_child(container);
        });

        // ESC to cancel
        overlay.connect('key-press-event', (actor, event) => {
            if (event.get_key_symbol() === Clutter.KEY_Escape) {
                Main.layoutManager.removeChrome(overlay);
                overlay.destroy();
                invocation.return_value(new GLib.Variant('(s)', ['']));
                return Clutter.EVENT_STOP;
            }
            return Clutter.EVENT_PROPAGATE;
        });

        Main.layoutManager.addTopChrome(overlay);
        overlay.grab_key_focus();
    }

    _startPulse() {
        this._stopPulse();
        this._pulseLoop = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 1200, () => {
            if (!this._topBar || !this._topBar.visible)
                return GLib.SOURCE_REMOVE;
            this._topBar.ease({opacity: 180, duration: 600, mode: imports.gi.Clutter.AnimationMode.EASE_IN_OUT_SINE,
                onComplete: () => {
                    this._topBar.ease({opacity: 255, duration: 600, mode: imports.gi.Clutter.AnimationMode.EASE_IN_OUT_SINE});
                }
            });
            return GLib.SOURCE_CONTINUE;
        });
    }

    _stopPulse() {
        if (this._pulseLoop) {
            GLib.source_remove(this._pulseLoop);
            this._pulseLoop = null;
        }
    }

    _toggleRecording() {
        if (this._proxy) {
            this._proxy.ToggleRecordingRemote(([result]) => {});
        }
    }

    _startLongForm() {
        if (this._proxy) {
            this._proxy.StartLongFormRemote(([result]) => {});
        }
    }

    _quit() {
        this._userQuit = true;
        if (this._proxy) {
            this._proxy.QuitRemote(([result]) => {});
        }
    }

    destroy() {
        this._stopPulse();
        this._stopRainbowPill();
        if (this._topBar) {
            Main.layoutManager.removeChrome(this._topBar);
            this._topBar.destroy();
            this._topBar = null;
        }
        if (this._pill) {
            Main.layoutManager.removeChrome(this._pill);
            this._pill.destroy();
            this._pill = null;
        }
        if (this._watchId) {
            Gio.bus_unwatch_name(this._watchId);
            this._watchId = null;
        }
        if (this._proxy && this._signalId) {
            this._proxy.disconnectSignal(this._signalId);
        }
        this._proxy = null;
        super.destroy();
    }
});

export default class FrogScribeExtension extends Extension {
    enable() {
        this._indicator = new FrogScribeIndicator(this.path);
        Main.panel.addToStatusArea(this.uuid, this._indicator);
        // Register window list D-Bus service
        this._dbusId = Gio.DBus.session.register_object(
            '/org/frogscribe/Windows',
            Gio.DBusNodeInfo.new_for_xml(`
                <node>
                    <interface name="org.frogscribe.Windows">
                        <method name="List">
                            <arg type="s" direction="out"/>
                        </method>
                        <method name="Activate">
                            <arg type="s" direction="in" name="window_id"/>
                            <arg type="s" direction="out"/>
                        </method>
                        <method name="GetThumbnails">
                            <arg type="s" direction="out"/>
                        </method>
                    </interface>
                </node>
            `).interfaces[0],
            (connection, sender, path, iface, method, params, invocation) => {
                if (method === 'List') {
                    const windows = [];
                    for (const actor of global.get_window_actors()) {
                        const win = actor.get_meta_window();
                        if (!win || win.get_window_type() !== 0) continue;
                        windows.push({
                            id: win.get_id().toString(),
                            title: win.get_title() || '',
                            wm_class: win.get_wm_class() || '',
                        });
                    }
                    invocation.return_value(new GLib.Variant('(s)', [JSON.stringify(windows)]));
                } else if (method === 'Activate') {
                    const winId = params.get_child_value(0).get_string()[0];
                    for (const actor of global.get_window_actors()) {
                        const win = actor.get_meta_window();
                        if (win && win.get_id().toString() === winId) {
                            Main.activateWindow(win);
                            break;
                        }
                    }
                    invocation.return_value(new GLib.Variant('(s)', ['ok']));
                } else if (method === 'GetThumbnails') {
                    // Show a compositor-native window picker using Clutter.Clone
                    // and return the selected window ID
                    this._indicator._showWindowPicker(invocation);
                }
            },
            null, null
        );
        Gio.DBus.session.own_name('org.frogscribe.Windows', Gio.BusNameOwnerFlags.NONE, null, null);
    }

    disable() {
        if (this._dbusId) {
            Gio.DBus.session.unregister_object(this._dbusId);
            this._dbusId = null;
        }
        this._indicator?.destroy();
        this._indicator = null;
    }
}
