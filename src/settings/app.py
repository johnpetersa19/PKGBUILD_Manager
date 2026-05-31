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
import gettext
from pathlib import Path

# i18n setup
gettext.bindtextdomain("pkgbuild_manager", "/usr/share/locale")
gettext.textdomain("pkgbuild_manager")
_ = gettext.gettext

CONFIG_DIR  = Path(os.environ.get("XDG_CONFIG_HOME", Path.home() / ".config")) / "pkgbuild-manager"
CONFIG_FILE = CONFIG_DIR / "menu.json"


def _default_menu():
    return [
        {
            "group": _("Actions"),
            "items": [
                {"id": "00_Full Workflow",    "label": _("00_Full Workflow"),   "enabled": True},
                {"id": "02_Install",          "label": _("02_Install"),         "enabled": True},
                {"id": "01_Build",            "label": _("01_Build"),           "enabled": True},
                {"id": "02b_Build and Clean", "label": _("02b_Build and Clean"), "enabled": True},
            ]
        },
        {
            "group": _("Metadata"),
            "items": [
                {"id": "03_Update Checksums", "label": _("03_Update Checksums"), "enabled": True},
                {"id": "04_Update .SRCINFO",  "label": _("04_Update .SRCINFO"),  "enabled": True},
            ]
        },
        {
            "group": _("Audit"),
            "items": [
                {"id": "05_Namcap",      "label": _("05_Namcap"),      "enabled": True},
                {"id": "05b_ShellCheck", "label": _("05b_ShellCheck"), "enabled": True},
            ]
        },
        {
            "group": _("Git / AUR"),
            "items": [
                {"id": "06_Push AUR", "label": _("06_Push AUR"), "enabled": True},
            ]
        },
        {
            "group": _("Clean"),
            "items": [
                {"id": "07_Clean srcdir",      "label": _("07_Clean srcdir"),      "enabled": True},
                {"id": "07b_Clean Everything", "label": _("07b_Clean Everything"), "enabled": True},
            ]
        },
    ]


def _all_actions():
    return [
        {"id": "00_Full Workflow",     "label": _("00_Full Workflow")},
        {"id": "01_Build",             "label": _("01_Build")},
        {"id": "02b_Build and Clean",  "label": _("02b_Build and Clean")},
        {"id": "02_Install",           "label": _("02_Install")},
        {"id": "03_Update Checksums",  "label": _("03_Update Checksums")},
        {"id": "04_Update .SRCINFO",   "label": _("04_Update .SRCINFO")},
        {"id": "05_Namcap",            "label": _("05_Namcap")},
        {"id": "05b_ShellCheck",       "label": _("05b_ShellCheck")},
        {"id": "06_Push AUR",          "label": _("06_Push AUR")},
        {"id": "07_Clean srcdir",      "label": _("07_Clean srcdir")},
        {"id": "07b_Clean Everything", "label": _("07b_Clean Everything")},
    ]


def load_config():
    if CONFIG_FILE.exists():
        try:
            data = json.loads(CONFIG_FILE.read_text())
            # Re-translate labels from known ids so saved configs also get
            # translated on next load (if the user's locale changed).
            id_to_label = {a["id"]: a["label"] for a in _all_actions()}
            for group in data:
                for item in group.get("items", []):
                    if item["id"] in id_to_label:
                        item["label"] = id_to_label[item["id"]]
            return data
        except Exception:
            pass
    return _default_menu()


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
        self.win.set_title(_("PKGBUILD Manager — Menu Settings"))
        self.win.set_default_size(700, 600)

        toolbar_view = Adw.ToolbarView()
        header = Adw.HeaderBar()

        reset_btn = Gtk.Button(label=_("Reset"))
        reset_btn.add_css_class("destructive-action")
        reset_btn.connect("clicked", self._on_reset)
        header.pack_start(reset_btn)

        save_btn = Gtk.Button(label=_("Save"))
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

        add_btn = Gtk.Button(label=_("+ Add Group"))
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

        add_item_btn = Gtk.Button(label=_("+ Add Item"))
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
        self.menu_data.append({"group": _("New Group"), "items": []})
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
        available = [a for a in _all_actions() if a["id"] not in used]

        if not available:
            self.win.add_toast(Adw.Toast(title=_("All actions are already in a group.")))
            return

        dialog = Adw.Dialog()
        dialog.set_title(_("Add Action"))
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
        self.win.add_toast(Adw.Toast(title=_("Saved! Restart the file manager to apply.")))

    def _on_reset(self, _btn):
        self.menu_data = _default_menu()
        self._render_groups()


def main():
    app = SettingsApp()
    return app.run(None)
