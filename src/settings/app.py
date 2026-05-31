#!/usr/bin/env python3
# pkgbuild-manager-settings
# GTK4 + Libadwaita settings panel.
# Lets the user choose which actions appear in the file-manager context menu,
# rename them, reorder them and group them into named submenus.
# Config is saved to ~/.config/pkgbuild-manager/menu.json and is read at
# runtime by all file-manager extensions (Nautilus, Nemo, Caja, Dolphin).

import gi
gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
from gi.repository import Gtk, Adw, Gio

import json
import os
import copy
import subprocess
from pathlib import Path

CONFIG_DIR  = Path(os.environ.get("XDG_CONFIG_HOME", Path.home() / ".config")) / "pkgbuild-manager"
CONFIG_FILE = CONFIG_DIR / "menu.json"

DEFAULT_MENU = [
    {
        "group": "Actions",
        "items": [
            {"id": "00_Full Workflow",    "label": "Full Workflow",   "enabled": True},
            {"id": "02_Install",          "label": "Install",         "enabled": True},
            {"id": "01_Build",            "label": "Build",           "enabled": True},
            {"id": "02b_Build and Clean", "label": "Build and Clean", "enabled": True},
        ]
    },
    {
        "group": "Metadata",
        "items": [
            {"id": "03_Update Checksums", "label": "Update Checksums", "enabled": True},
            {"id": "04_Update .SRCINFO",  "label": "Update .SRCINFO",  "enabled": True},
        ]
    },
    {
        "group": "Audit",
        "items": [
            {"id": "05_Namcap",      "label": "Namcap",     "enabled": True},
            {"id": "05b_ShellCheck", "label": "ShellCheck", "enabled": True},
        ]
    },
    {
        "group": "Git / AUR",
        "items": [
            {"id": "06_Push AUR", "label": "Push AUR", "enabled": True},
        ]
    },
    {
        "group": "Clean",
        "items": [
            {"id": "07_Clean srcdir",      "label": "Clean srcdir",    "enabled": True},
            {"id": "07b_Clean Everything", "label": "Clean Everything", "enabled": True},
        ]
    },
]

ALL_ACTIONS = [
    {"id": "00_Full Workflow",     "label": "Full Workflow"},
    {"id": "01_Build",             "label": "Build"},
    {"id": "02b_Build and Clean",  "label": "Build and Clean"},
    {"id": "02_Install",           "label": "Install"},
    {"id": "03_Update Checksums",  "label": "Update Checksums"},
    {"id": "04_Update .SRCINFO",   "label": "Update .SRCINFO"},
    {"id": "05_Namcap",            "label": "Namcap"},
    {"id": "05b_ShellCheck",       "label": "ShellCheck"},
    {"id": "06_Push AUR",          "label": "Push AUR"},
    {"id": "07_Clean srcdir",      "label": "Clean srcdir"},
    {"id": "07b_Clean Everything", "label": "Clean Everything"},
]


def load_config():
    if CONFIG_FILE.exists():
        try:
            return json.loads(CONFIG_FILE.read_text())
        except Exception:
            pass
    return copy.deepcopy(DEFAULT_MENU)


def save_config(data):
    CONFIG_DIR.mkdir(parents=True, exist_ok=True)
    CONFIG_FILE.write_text(json.dumps(data, indent=2, ensure_ascii=False))
    _notify_file_managers()


def _notify_file_managers():
    regen = "/usr/share/pkgbuild-manager/regen-dolphin-desktop"
    if os.path.isfile(regen) and os.access(regen, os.X_OK):
        subprocess.Popen([regen], close_fds=True)


class SettingsApp(Adw.Application):
    def __init__(self):
        super().__init__(
            application_id="io.github.johnpetersa19.PkgbuildManagerSettings",
            flags=Gio.ApplicationFlags.FLAGS_NONE,
        )
        self.connect("activate", self._on_activate)

    def _on_activate(self, *_):
        self.menu_data = load_config()
        self._build_window()
        self.win.present()

    def _build_window(self):
        self.win = Adw.ApplicationWindow(application=self)
        self.win.set_title("PKGBUILD Manager — Menu Settings")
        self.win.set_default_size(700, 600)

        toolbar_view = Adw.ToolbarView()
        header = Adw.HeaderBar()

        reset_btn = Gtk.Button(label="Reset")
        reset_btn.add_css_class("destructive-action")
        reset_btn.connect("clicked", self._on_reset)
        header.pack_start(reset_btn)

        save_btn = Gtk.Button(label="Save")
        save_btn.add_css_class("suggested-action")
        save_btn.connect("clicked", self._on_save)
        header.pack_end(save_btn)

        toolbar_view.add_top_bar(header)

        scroll = Gtk.ScrolledWindow(vexpand=True)
        self.main_box = Gtk.Box(
            orientation=Gtk.Orientation.VERTICAL,
            spacing=12,
            margin_top=12, margin_bottom=12,
            margin_start=16, margin_end=16,
        )
        scroll.set_child(self.main_box)
        toolbar_view.set_content(scroll)
        self.win.set_content(toolbar_view)

        self._render_groups()

    def _render_groups(self):
        while True:
            child = self.main_box.get_first_child()
            if child is None:
                break
            self.main_box.remove(child)

        for g_idx, group in enumerate(self.menu_data):
            self.main_box.append(self._build_group_widget(g_idx, group))

        add_btn = Gtk.Button(label="+ Add Group")
        add_btn.add_css_class("pill")
        add_btn.set_halign(Gtk.Align.CENTER)
        add_btn.connect("clicked", self._on_add_group)
        self.main_box.append(add_btn)

    def _build_group_widget(self, g_idx, group):
        frame = Gtk.Frame()
        frame.add_css_class("card")

        vbox = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=0)
        frame.set_child(vbox)

        header_row = Gtk.Box(
            orientation=Gtk.Orientation.HORIZONTAL, spacing=8,
            margin_top=8, margin_bottom=4,
            margin_start=12, margin_end=8,
        )

        up_btn = Gtk.Button(icon_name="go-up-symbolic")
        up_btn.add_css_class("flat")
        up_btn.set_sensitive(g_idx > 0)
        up_btn.connect("clicked", self._on_group_move, g_idx, -1)

        down_btn = Gtk.Button(icon_name="go-down-symbolic")
        down_btn.add_css_class("flat")
        down_btn.set_sensitive(g_idx < len(self.menu_data) - 1)
        down_btn.connect("clicked", self._on_group_move, g_idx, 1)

        name_entry = Gtk.Entry()
        name_entry.set_text(group["group"])
        name_entry.set_hexpand(True)
        name_entry.connect("changed", self._on_group_rename, g_idx)

        del_btn = Gtk.Button(icon_name="user-trash-symbolic")
        del_btn.add_css_class("flat")
        del_btn.add_css_class("error")
        del_btn.connect("clicked", self._on_group_delete, g_idx)

        header_row.append(up_btn)
        header_row.append(down_btn)
        header_row.append(name_entry)
        header_row.append(del_btn)
        vbox.append(header_row)
        vbox.append(Gtk.Separator())

        items_box = Gtk.Box(
            orientation=Gtk.Orientation.VERTICAL, spacing=0,
            margin_top=4, margin_bottom=8,
            margin_start=8, margin_end=8,
        )
        vbox.append(items_box)

        for i_idx, item in enumerate(group["items"]):
            items_box.append(self._build_item_row(g_idx, i_idx, item, len(group["items"])))

        add_item_btn = Gtk.Button(label="+ Add Item")
        add_item_btn.add_css_class("flat")
        add_item_btn.set_halign(Gtk.Align.START)
        add_item_btn.set_margin_start(4)
        add_item_btn.connect("clicked", self._on_add_item_dialog, g_idx)
        items_box.append(add_item_btn)

        return frame

    def _build_item_row(self, g_idx, i_idx, item, total):
        row = Gtk.Box(
            orientation=Gtk.Orientation.HORIZONTAL, spacing=8,
            margin_top=2, margin_bottom=2,
            margin_start=4, margin_end=4,
        )

        toggle = Gtk.Switch(valign=Gtk.Align.CENTER)
        toggle.set_active(item.get("enabled", True))
        toggle.connect("state-set", self._on_item_toggle, g_idx, i_idx)

        label_entry = Gtk.Entry()
        label_entry.set_text(item["label"])
        label_entry.set_hexpand(True)
        label_entry.connect("changed", self._on_item_rename, g_idx, i_idx)

        up_btn = Gtk.Button(icon_name="go-up-symbolic")
        up_btn.add_css_class("flat")
        up_btn.set_sensitive(i_idx > 0)
        up_btn.connect("clicked", self._on_item_move, g_idx, i_idx, -1)

        down_btn = Gtk.Button(icon_name="go-down-symbolic")
        down_btn.add_css_class("flat")
        down_btn.set_sensitive(i_idx < total - 1)
        down_btn.connect("clicked", self._on_item_move, g_idx, i_idx, 1)

        del_btn = Gtk.Button(icon_name="list-remove-symbolic")
        del_btn.add_css_class("flat")
        del_btn.connect("clicked", self._on_item_remove, g_idx, i_idx)

        row.append(toggle)
        row.append(label_entry)
        row.append(up_btn)
        row.append(down_btn)
        row.append(del_btn)
        return row

    # --- Group callbacks ---

    def _on_group_rename(self, entry, g_idx):
        self.menu_data[g_idx]["group"] = entry.get_text()

    def _on_group_move(self, _btn, g_idx, direction):
        d = self.menu_data
        new_idx = g_idx + direction
        if 0 <= new_idx < len(d):
            d[g_idx], d[new_idx] = d[new_idx], d[g_idx]
            self._render_groups()

    def _on_group_delete(self, _btn, g_idx):
        self.menu_data.pop(g_idx)
        self._render_groups()

    def _on_add_group(self, _btn):
        self.menu_data.append({"group": "New Group", "items": []})
        self._render_groups()

    # --- Item callbacks ---

    def _on_item_toggle(self, _switch, state, g_idx, i_idx):
        self.menu_data[g_idx]["items"][i_idx]["enabled"] = state

    def _on_item_rename(self, entry, g_idx, i_idx):
        self.menu_data[g_idx]["items"][i_idx]["label"] = entry.get_text()

    def _on_item_move(self, _btn, g_idx, i_idx, direction):
        items = self.menu_data[g_idx]["items"]
        new_idx = i_idx + direction
        if 0 <= new_idx < len(items):
            items[i_idx], items[new_idx] = items[new_idx], items[i_idx]
            self._render_groups()

    def _on_item_remove(self, _btn, g_idx, i_idx):
        self.menu_data[g_idx]["items"].pop(i_idx)
        self._render_groups()

    def _on_add_item_dialog(self, _btn, g_idx):
        used = {item["id"] for g in self.menu_data for item in g["items"]}
        available = [a for a in ALL_ACTIONS if a["id"] not in used]

        if not available:
            self.win.add_toast(Adw.Toast(title="All actions are already in a group."))
            return

        dialog = Adw.Dialog()
        dialog.set_title("Add Action")
        dialog.set_content_width(360)

        toolbar = Adw.ToolbarView()
        toolbar.add_top_bar(Adw.HeaderBar())

        list_box = Gtk.ListBox()
        list_box.set_selection_mode(Gtk.SelectionMode.SINGLE)
        list_box.add_css_class("boxed-list")
        list_box.set_margin_top(12)
        list_box.set_margin_bottom(12)
        list_box.set_margin_start(12)
        list_box.set_margin_end(12)

        for action in available:
            lbl = Gtk.Label(label=action["label"], xalign=0)
            lbl.set_margin_top(8)
            lbl.set_margin_bottom(8)
            lbl.set_margin_start(8)
            row = Gtk.ListBoxRow()
            row.set_child(lbl)
            row._action = action
            list_box.append(row)

        def on_row_activated(lb, row):
            self.menu_data[g_idx]["items"].append({
                "id": row._action["id"],
                "label": row._action["label"],
                "enabled": True,
            })
            dialog.close()
            self._render_groups()

        list_box.connect("row-activated", on_row_activated)
        toolbar.set_content(list_box)
        dialog.set_child(toolbar)
        dialog.present(self.win)

    # --- Save / Reset ---

    def _on_save(self, _btn):
        save_config(self.menu_data)
        self.win.add_toast(Adw.Toast(title="Saved! Restart the file manager to apply."))

    def _on_reset(self, _btn):
        self.menu_data = copy.deepcopy(DEFAULT_MENU)
        self._render_groups()


def main():
    app = SettingsApp()
    return app.run(None)
