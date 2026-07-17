#!/usr/bin/env python3
# pkgbuild_manager_caja.py — Caja Python extension
# Reads menu layout from ~/.config/pkgbuild-manager/menu.json

import os
import json
import gettext
import subprocess
from pathlib import Path
from gi.repository import Caja, GObject

_user_locale = os.path.expanduser("~/.local/share/locale")
gettext.bindtextdomain("pkgbuild_manager", _user_locale if os.path.isdir(_user_locale) else "/usr/share/locale")
gettext.textdomain("pkgbuild_manager")
_ = gettext.gettext

CONFIG_FILE = Path(os.environ.get("XDG_CONFIG_HOME", Path.home() / ".config")) / "pkgbuild-manager" / "menu.json"

DEFAULT_ACTIONS = [
    ("00_Full Workflow",     "00_Full Workflow"),
    ("01_Build",             "01_Build"),
    ("02b_Build and Clean",  "02b_Build and Clean"),
    ("02_Install",           "02_Install"),
    ("03_Update Checksums",  "03_Update Checksums"),
    ("04_Update .SRCINFO",   "04_Update .SRCINFO"),
    ("05_Namcap",            "05_Namcap"),
    ("05b_ShellCheck",       "05b_ShellCheck"),
    ("06_Push AUR",          "06_Push AUR"),
    ("17_Push AUR Tag",      "17_Push AUR Tag"),
    ("07_Clean srcdir",      "07_Clean srcdir"),
    ("07b_Clean Everything", "07b_Clean Everything"),
]

TAG_ACTION = "17_Push AUR Tag"
ARCHIVE_SUFFIXES = (
    ".zip", ".tar", ".tar.gz", ".tgz", ".tar.bz2", ".tbz2",
    ".tar.xz", ".txz", ".tar.zst", ".tzst", ".7z", ".rar",
)


def _project_root(path):
    current = Path(path).resolve().parent
    for directory in (current, *current.parents):
        if (directory / ".git").exists():
            return directory
    return None


def _is_aur_repository(root):
    try:
        result = subprocess.run(
            ["git", "-C", str(root), "remote", "get-url", "origin"],
            capture_output=True, text=True, timeout=2, check=False,
        )
        return "aur.archlinux.org" in result.stdout.lower()
    except (OSError, subprocess.TimeoutExpired):
        return False


def _is_archive(path):
    return str(path).lower().endswith(ARCHIVE_SUFFIXES)


def _scripts_dir():
    here = os.path.dirname(os.path.abspath(__file__))
    if here.startswith("/usr/share/"):
        installed = "/usr/share/pkgbuild-manager/scripts"
        if os.path.isdir(installed):
            return installed
    if here.startswith("/usr/local/share/"):
        installed = "/usr/local/share/pkgbuild-manager/scripts"
        if os.path.isdir(installed):
            return installed
    user_flatpak = os.path.expanduser("~/.local/share/pkgbuild-manager/scripts")
    if os.path.isdir(user_flatpak):
        return user_flatpak
    installed = "/usr/share/pkgbuild-manager/scripts"
    if os.path.isdir(installed):
        return installed
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
    return [(sid, _(sid), "PKGBUILD") for sid, _ in DEFAULT_ACTIONS], 1


class PkgbuildMenuProvider(GObject.GObject, Caja.MenuProvider):

    def _build_menu(self, pkgbuild_path, tag_only=False):
        scripts = _scripts_dir()
        items, num_groups = _load_menu()
        root = Path(pkgbuild_path) if Path(pkgbuild_path).is_dir() else Path(pkgbuild_path).parent
        if _is_aur_repository(root):
            items = [item for item in items if item[0] != TAG_ACTION]
        elif tag_only:
            items = [item for item in items if item[0] == TAG_ACTION]
        if not items:
            return []

        groups = {}
        group_order = []
        for sid, label, group in items:
            if group not in groups:
                groups[group] = []
                group_order.append(group)
            groups[group].append((sid, label))

        top = Caja.MenuItem(
            name="PkgbuildManager::TopMenu",
            label="PKGBUILD",
            tip="PKGBUILD Manager actions",
        )
        top_submenu = Caja.Menu()
        top.set_submenu(top_submenu)

        def make_cb(spath, pkgpath):
            def cb(_item):
                if not os.path.isfile(spath) or not os.access(spath, os.X_OK):
                    subprocess.Popen([
                        "notify-send", "-a", "PKGBUILD Manager",
                        "Script not found",
                        f"Missing: {spath}\nReinstall pkgbuild-manager."
                    ], close_fds=True)
                    return
                workdir = pkgpath if os.path.isdir(pkgpath) else os.path.dirname(pkgpath)
                subprocess.Popen(["bash", spath, pkgpath],
                                 cwd=workdir, close_fds=True)
            return cb

        if len(group_order) <= 1:
            for sid, label in groups.get(group_order[0] if group_order else "", []):
                sp = os.path.join(scripts, sid)
                it = Caja.MenuItem(name=f"PkgbuildManager::{sid.replace(' ','_')}",
                                   label=label, tip=f"Run {sid}")
                it.connect("activate", make_cb(sp, pkgbuild_path))
                top_submenu.append_item(it)
        else:
            for gname in group_order:
                git = Caja.MenuItem(
                    name=f"PkgbuildManager::G_{gname.replace(' ','_')}",
                    label=gname, tip=gname)
                gsub = Caja.Menu()
                git.set_submenu(gsub)
                for sid, label in groups[gname]:
                    sp = os.path.join(scripts, sid)
                    it = Caja.MenuItem(name=f"PkgbuildManager::{sid.replace(' ','_')}",
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
        if f.is_directory():
            return None
        path = f.get_location().get_path()
        if f.get_name() == "PKGBUILD":
            return path, False
        if _is_archive(path):
            root = _project_root(path)
            if root is not None and not _is_aur_repository(root):
                return str(root), True
        return None

    def get_file_items(self, files):
        context = self._check_file(files)
        return self._build_menu(*context) if context else []

    def get_background_items(self, folder):
        return []
