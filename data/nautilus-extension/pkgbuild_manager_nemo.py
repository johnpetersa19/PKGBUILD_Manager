#!/usr/bin/env python3
# pkgbuild_manager_nemo.py — Nemo Python extension
# Reads menu layout from ~/.config/pkgbuild-manager/menu.json

import os
import json
import subprocess
from pathlib import Path
from gi.repository import Nemo, GObject

CONFIG_FILE = Path(os.environ.get("XDG_CONFIG_HOME", Path.home() / ".config")) / "pkgbuild-manager" / "menu.json"

DEFAULT_ACTIONS = [
    ("00_Full Workflow",     "Full Workflow"),
    ("01_Build",             "Build"),
    ("02b_Build and Clean",  "Build and Clean"),
    ("02_Install",           "Install"),
    ("03_Update Checksums",  "Update Checksums"),
    ("04_Update .SRCINFO",   "Update .SRCINFO"),
    ("05_Namcap",            "Namcap"),
    ("05b_ShellCheck",       "ShellCheck"),
    ("06_Push AUR",          "Push AUR"),
    ("07_Clean srcdir",      "Clean srcdir"),
    ("07b_Clean Everything", "Clean Everything"),
]


def _scripts_dir():
    installed = "/usr/share/pkgbuild-manager/scripts"
    if os.path.isdir(installed):
        return installed
    here = os.path.dirname(os.path.abspath(__file__))
    return os.path.normpath(os.path.join(here, "..", "nautilus-scripts"))


def _load_menu():
    if CONFIG_FILE.exists():
        try:
            data = json.loads(CONFIG_FILE.read_text())
            result = []
            for group in data:
                for item in group.get("items", []):
                    if item.get("enabled", True):
                        result.append((item["id"], item["label"], group["group"]))
            return result, len(data)
        except Exception:
            pass
    return [(sid, lbl, "PKGBUILD") for sid, lbl in DEFAULT_ACTIONS], 1


class PkgbuildMenuProvider(GObject.GObject, Nemo.MenuProvider):

    def _build_menu(self, pkgbuild_path):
        scripts = _scripts_dir()
        items, _ = _load_menu()

        groups = {}
        group_order = []
        for sid, label, group in items:
            if group not in groups:
                groups[group] = []
                group_order.append(group)
            groups[group].append((sid, label))

        top = Nemo.MenuItem(name="PkgbuildManager::TopMenu",
                            label="PKGBUILD", tip="PKGBUILD Manager actions")
        top_submenu = Nemo.Menu()
        top.set_submenu(top_submenu)

        def make_cb(spath, pkgpath):
            def cb(_item):
                subprocess.Popen(["bash", spath, pkgpath],
                                 cwd=os.path.dirname(pkgpath), close_fds=True)
            return cb

        if len(group_order) <= 1:
            for sid, label in groups.get(group_order[0] if group_order else "", []):
                sp = os.path.join(scripts, sid)
                if not os.path.isfile(sp) or not os.access(sp, os.X_OK):
                    continue
                it = Nemo.MenuItem(name=f"PkgbuildManager::{sid.replace(' ','_')}",
                                   label=label, tip=f"Run {sid}")
                it.connect("activate", make_cb(sp, pkgbuild_path))
                top_submenu.append_item(it)
        else:
            for gname in group_order:
                git = Nemo.MenuItem(name=f"PkgbuildManager::G_{gname.replace(' ','_')}",
                                    label=gname, tip=gname)
                gsub = Nemo.Menu()
                git.set_submenu(gsub)
                for sid, label in groups[gname]:
                    sp = os.path.join(scripts, sid)
                    if not os.path.isfile(sp) or not os.access(sp, os.X_OK):
                        continue
                    it = Nemo.MenuItem(name=f"PkgbuildManager::{sid.replace(' ','_')}",
                                       label=label, tip=f"Run {sid}")
                    it.connect("activate", make_cb(sp, pkgbuild_path))
                    gsub.append_item(it)
                top_submenu.append_item(git)

        return [top]

    def _check_file(self, files):
        if len(files) != 1:
            return None
        f = files[0]
        if not f.get_uri().startswith("file://"):
            return None
        if f.get_name() != "PKGBUILD" or f.is_directory():
            return None
        return f.get_location().get_path()

    def get_file_items(self, files):
        path = self._check_file(files)
        return self._build_menu(path) if path else []

    def get_background_items(self, folder):
        return []
