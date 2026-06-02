#!/usr/bin/env python3
# pkgbuild-manager-aur-push
# GTK4 + Libadwaita dialog: commit message entry + per-step progress.

import gi
gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
from gi.repository import Gtk, Adw, Gio, GLib

import sys
import os
import re
import gettext
from pathlib import Path

gettext.bindtextdomain("pkgbuild_manager", "/usr/share/locale")
gettext.textdomain("pkgbuild_manager")
_ = gettext.gettext

# ── Step definitions ──────────────────────────────────────────────────────────
#   (key, label)
STEPS = [
    ("regen-srcinfo",    _("Regenerate .SRCINFO")),
    ("git-status",       _("Check git status")),
    ("git-add",          _("Stage PKGBUILD + .SRCINFO")),
    ("git-commit",       _("Create commit")),
    ("git-push",         _("Push to AUR")),
]

STATUS_PENDING  = "pending"
STATUS_RUNNING  = "running"
STATUS_OK       = "ok"
STATUS_ERROR    = "error"

_STEP_KEY_RE = re.compile(r"^\[STEP\] ([\w-]+) (start|ok|error)(?:: (.*))?$")


class StepRow(Gtk.Box):
    """One row in the step list: icon + label + status chip."""

    _ICON = {
        STATUS_PENDING: ("emblem-default-symbolic",  "dim-label"),
        STATUS_RUNNING: ("emblem-synchronizing-symbolic", "accent"),
        STATUS_OK:      ("emblem-ok-symbolic",        "success"),
        STATUS_ERROR:   ("dialog-error-symbolic",     "error"),
    }

    def __init__(self, label: str):
        super().__init__(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
        self.set_margin_top(6)
        self.set_margin_bottom(6)
        self.set_margin_start(12)
        self.set_margin_end(12)

        self._icon = Gtk.Image()
        self._icon.set_pixel_size(18)
        self.append(self._icon)

        self._label = Gtk.Label(label=label, xalign=0, hexpand=True)
        self.append(self._label)

        self._chip = Gtk.Label(label="", xalign=1)
        self._chip.add_css_class("caption")
        self.append(self._chip)

        self.set_status(STATUS_PENDING)

    def set_status(self, status: str, detail: str = ""):
        icon_name, css_class = self._ICON.get(status, self._ICON[STATUS_PENDING])
        self._icon.set_from_icon_name(icon_name)
        # Reset chip style
        for cls in ("dim-label", "accent", "success", "error"):
            self._chip.remove_css_class(cls)
        self._chip.add_css_class(css_class)
        chip_text = {
            STATUS_PENDING: _("waiting"),
            STATUS_RUNNING: _("running…"),
            STATUS_OK:      _("done"),
            STATUS_ERROR:   _("failed"),
        }.get(status, "")
        if detail and status == STATUS_ERROR:
            # Show first 60 chars of the error
            chip_text = detail[:60]
        self._chip.set_label(chip_text)


class AurPushWindow(Adw.ApplicationWindow):
    def __init__(self, app, pkgbuild_path: str):
        super().__init__(application=app)
        self._path = pkgbuild_path
        self._proc = None
        self._cancelled = False
        self._step_rows: dict[str, StepRow] = {}

        self.set_title(_("Push to AUR"))
        self.set_default_size(560, 600)
        self.set_resizable(True)

        self._build_ui()

    # ── UI construction ───────────────────────────────────────────────────────

    def _build_ui(self):
        toolbar_view = Adw.ToolbarView()
        header = Adw.HeaderBar()
        toolbar_view.add_top_bar(header)

        outer = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=0)
        scroll = Gtk.ScrolledWindow(vexpand=True)
        scroll.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)

        content = Gtk.Box(
            orientation=Gtk.Orientation.VERTICAL,
            spacing=16,
            margin_top=16,
            margin_bottom=16,
            margin_start=16,
            margin_end=16,
        )
        scroll.set_child(content)
        outer.append(scroll)

        # ── Commit message / notes ────────────────────────────────────────────
        msg_group = Adw.PreferencesGroup(
            title=_("Commit message"),
            description=_(
                "Leave empty to use the automatic \u201cupgpkg: pkgname ver-rel\u201d message."
            ),
        )
        self._msg_entry = Adw.EntryRow(title=_("Message (optional)"))
        msg_group.add(self._msg_entry)
        content.append(msg_group)

        # ── Notes ─────────────────────────────────────────────────────────────
        notes_group = Adw.PreferencesGroup(
            title=_("Notes / important info"),
            description=_("Optional: describe what changed in this release."),
        )
        notes_frame = Gtk.Frame()
        notes_scroll = Gtk.ScrolledWindow()
        notes_scroll.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)
        notes_scroll.set_min_content_height(80)
        notes_scroll.set_max_content_height(160)
        self._notes_view = Gtk.TextView()
        self._notes_view.set_wrap_mode(Gtk.WrapMode.WORD_CHAR)
        self._notes_view.set_margin_top(8)
        self._notes_view.set_margin_bottom(8)
        self._notes_view.set_margin_start(8)
        self._notes_view.set_margin_end(8)
        notes_scroll.set_child(self._notes_view)
        notes_frame.set_child(notes_scroll)
        notes_group.add(notes_frame)
        content.append(notes_group)

        # ── Step list ─────────────────────────────────────────────────────────
        steps_group = Adw.PreferencesGroup(title=_("Progress"))
        steps_list = Gtk.ListBox()
        steps_list.add_css_class("boxed-list")
        steps_list.set_selection_mode(Gtk.SelectionMode.NONE)
        for key, label in STEPS:
            row = StepRow(label)
            self._step_rows[key] = row
            steps_list.append(row)
        steps_group.add(steps_list)
        content.append(steps_group)

        # ── Log view ──────────────────────────────────────────────────────────
        log_group = Adw.PreferencesGroup(title=_("Log"))
        log_frame = Gtk.Frame()
        log_scroll = Gtk.ScrolledWindow()
        log_scroll.set_policy(Gtk.PolicyType.AUTOMATIC, Gtk.PolicyType.AUTOMATIC)
        log_scroll.set_min_content_height(120)
        log_scroll.set_max_content_height(240)
        self._log_buf = Gtk.TextBuffer()
        log_view = Gtk.TextView(buffer=self._log_buf)
        log_view.set_editable(False)
        log_view.set_monospace(True)
        log_view.set_wrap_mode(Gtk.WrapMode.WORD_CHAR)
        log_view.set_margin_top(8)
        log_view.set_margin_bottom(8)
        log_view.set_margin_start(8)
        log_view.set_margin_end(8)
        log_scroll.set_child(log_view)
        log_frame.set_child(log_scroll)
        log_group.add(log_frame)
        self._log_scroll = log_scroll
        content.append(log_group)

        # ── Action buttons ────────────────────────────────────────────────────
        btn_box = Gtk.Box(
            orientation=Gtk.Orientation.HORIZONTAL,
            spacing=8,
            halign=Gtk.Align.END,
            margin_top=4,
            margin_start=16,
            margin_end=16,
            margin_bottom=16,
        )
        self._cancel_btn = Gtk.Button(label=_("Cancel"))
        self._cancel_btn.connect("clicked", self._on_cancel)
        btn_box.append(self._cancel_btn)

        self._send_btn = Gtk.Button(label=_("Send to AUR"))
        self._send_btn.add_css_class("suggested-action")
        self._send_btn.add_css_class("pill")
        self._send_btn.connect("clicked", self._on_send)
        btn_box.append(self._send_btn)
        outer.append(btn_box)

        toolbar_view.set_content(outer)
        self.set_content(toolbar_view)

    # ── Button handlers ───────────────────────────────────────────────────────

    def _on_cancel(self, *_):
        if self._proc is not None:
            self._cancelled = True
            try:
                self._proc.send_signal(15)  # SIGTERM
            except Exception:
                pass
        self.close()

    def _on_send(self, *_):
        self._send_btn.set_sensitive(False)
        self._msg_entry.set_sensitive(False)
        self._notes_view.set_sensitive(False)
        self._reset_steps()
        self._log_buf.set_text("")
        self._launch_push()

    # ── Step helpers ──────────────────────────────────────────────────────────

    def _reset_steps(self):
        for row in self._step_rows.values():
            row.set_status(STATUS_PENDING)

    def _update_step(self, key: str, status: str, detail: str = ""):
        row = self._step_rows.get(key)
        if row:
            row.set_status(status, detail)

    # ── Process launch & stdout reading ──────────────────────────────────────

    def _launch_push(self):
        msg = self._msg_entry.get_text().strip()
        cmd = ["pkgbuild_manager", "aur-push"]
        if msg:
            cmd += [msg]
        cmd.append(self._path)

        flags = (
            Gio.SubprocessFlags.STDOUT_PIPE
            | Gio.SubprocessFlags.STDERR_MERGE
        )
        try:
            self._proc = Gio.Subprocess.new(cmd, flags)
        except GLib.Error as exc:
            self._append_log(f"[error] Failed to start process: {exc}\n")
            self._send_btn.set_sensitive(True)
            return

        stdout_stream = self._proc.get_stdout_pipe()
        data_stream = Gio.DataInputStream.new(stdout_stream)
        self._read_line(data_stream)

    def _read_line(self, stream: Gio.DataInputStream):
        stream.read_line_async(
            GLib.PRIORITY_DEFAULT,
            None,
            self._on_line_ready,
            stream,
        )

    def _on_line_ready(self, src, result, stream):
        try:
            line_bytes, _ = stream.read_line_finish(result)
        except GLib.Error:
            self._on_process_done()
            return

        if line_bytes is None:
            # EOF
            self._on_process_done()
            return

        line = line_bytes.decode("utf-8", errors="replace")
        self._append_log(line + "\n")
        self._parse_step_line(line)
        self._read_line(stream)

    def _parse_step_line(self, line: str):
        m = _STEP_KEY_RE.match(line.strip())
        if not m:
            return
        key, state, detail = m.group(1), m.group(2), m.group(3) or ""
        if state == "start":
            self._update_step(key, STATUS_RUNNING)
        elif state == "ok":
            self._update_step(key, STATUS_OK)
        elif state == "error":
            self._update_step(key, STATUS_ERROR, detail)

    def _on_process_done(self):
        if self._cancelled:
            return
        exit_code = 0
        if self._proc:
            self._proc.wait(None)
            exit_code = self._proc.get_exit_status()
        self._proc = None

        if exit_code == 0:
            toast = Adw.Toast(title=_("AUR push completed successfully!"))
            toast.set_timeout(4)
            self.add_toast(toast)
            self._send_btn.set_label(_("Send again"))
        else:
            toast = Adw.Toast(title=_("AUR push failed — see log below."))
            toast.set_timeout(6)
            self.add_toast(toast)
            self._send_btn.set_label(_("Retry"))

        self._send_btn.set_sensitive(True)
        self._msg_entry.set_sensitive(True)
        self._notes_view.set_sensitive(True)

    # ── Log helpers ───────────────────────────────────────────────────────────

    def _append_log(self, text: str):
        end_iter = self._log_buf.get_end_iter()
        self._log_buf.insert(end_iter, text)
        # Auto-scroll to bottom
        GLib.idle_add(self._scroll_log_to_bottom)

    def _scroll_log_to_bottom(self):
        adj = self._log_scroll.get_vadjustment()
        adj.set_value(adj.get_upper())
        return False


class AurPushApp(Adw.Application):
    def __init__(self, pkgbuild_path: str):
        super().__init__(
            application_id="io.github.johnpetersa19.PkgbuildManager.AurPush",
            flags=Gio.ApplicationFlags.FLAGS_NONE,
        )
        self._path = pkgbuild_path
        self._win = None
        self.connect("activate", self._on_activate)

    def _on_activate(self, *_):
        if self._win is not None:
            self._win.present()
            return
        self._win = AurPushWindow(self, self._path)
        self._win.connect("destroy", lambda *_: setattr(self, "_win", None))
        self._win.present()


def main(path: str | None = None):
    if path is None:
        path = sys.argv[1] if len(sys.argv) > 1 else os.getcwd()
    app = AurPushApp(path)
    return app.run(None)


if __name__ == "__main__":
    sys.exit(main())
