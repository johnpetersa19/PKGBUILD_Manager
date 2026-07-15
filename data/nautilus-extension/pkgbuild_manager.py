#!/usr/bin/env python3
# pkgbuild_manager.py — Nautilus Python extension
# Reads menu layout from ~/.config/pkgbuild-manager/menu.json

import os
import json
import gettext
import subprocess
import threading
import gi
import re
from urllib.parse import urlparse
from pathlib import Path

gi.require_version("Nautilus", "4.1")
gi.require_version("Gdk", "4.0")
gi.require_version("Gtk", "4.0")
from gi.repository import Nautilus, GObject, Gdk, Gtk, GLib

_user_locale = os.path.expanduser("~/.local/share/locale")
gettext.bindtextdomain("pkgbuild_manager", _user_locale if os.path.isdir(_user_locale) else "/usr/share/locale")
gettext.textdomain("pkgbuild_manager")
_ = gettext.gettext

CONFIG_FILE = Path(os.environ.get("XDG_CONFIG_HOME", Path.home() / ".config")) / "pkgbuild-manager" / "menu.json"

# Mantém paridade com all_actions() em src/settings_gui/config.rs
DEFAULT_ACTIONS = [
    ("00_Full Workflow",     "Full Workflow"),
    ("01_Build",             "Build"),
    ("02b_Build and Clean",  "Build and Clean"),
    ("08_Build Force",       "Force Build"),
    ("09_Build NoCheck",     "Build without Checks"),
    ("10_Build NoGPG",       "Build without GPG"),
    ("11_Fetch Sources",     "Fetch Sources"),
    ("02_Install",           "Install"),
    ("12_Install Force",     "Force Install"),
    ("13_Install RmDeps",    "Install and Remove Build Dependencies"),
    ("14_Install NoCheck",   "Install without Checks"),
    ("15_Install NoGPG",     "Install without GPG"),
    ("03_Update Checksums",  "Update Checksums"),
    ("04_Update .SRCINFO",   "Update .SRCINFO"),
    ("16_Gen Checksums",     "Generate Checksums"),
    ("05_Namcap",            "Namcap"),
    ("05b_ShellCheck",       "ShellCheck"),
    ("06_Push AUR",          "Push to AUR"),
    ("17_Push AUR Tag",      "Push AUR Tag"),
    ("07_Clean srcdir",      "Clean srcdir"),
    ("07b_Clean Everything", "Clean Everything"),
]
DEFAULT_LABELS = dict(DEFAULT_ACTIONS)

ROOT_GROUP = "PKGBUILD"


def _notify(message, error=False):
    args = ["notify-send", "-a", "PKGBUILD Manager"]
    if error:
        args.extend(["-u", "critical"])
    subprocess.Popen(args + [message], close_fds=True)


def _show_error_window(message):
    dialog = Gtk.AlertDialog()
    dialog.set_message(_("Repository download error"))
    dialog.set_detail(str(message))
    dialog.set_buttons([_("Close")])
    dialog.show(None)


def _repository_from_url(text):
    """Return (clone_url, optional_branch) for a copied repository URL."""
    value = (text or "").strip().splitlines()[0] if (text or "").strip() else ""
    if re.match(r"^[^\s@]+@[^\s:]+:.+", value):
        return value, None

    parsed = urlparse(value)
    if parsed.scheme not in ("http", "https", "ssh", "git") or not parsed.netloc:
        return None

    parts = [part for part in parsed.path.split("/") if part]
    host = parsed.netloc.lower()
    branch = None

    if host in ("github.com", "www.github.com") and len(parts) >= 2:
        owner, repository = parts[0], parts[1].removesuffix(".git")
        if len(parts) >= 4 and parts[2] == "tree":
            branch = "/".join(parts[3:])
        return f"{parsed.scheme}://{parsed.netloc}/{owner}/{repository}.git", branch

    # GitLab uses /owner/repository/-/tree/branch.
    if "-/tree" in parsed.path:
        prefix, branch = parsed.path.split("/-/tree/", 1)
        return f"{parsed.scheme}://{parsed.netloc}{prefix}.git", branch.strip("/")

    # Codeberg/Gitea uses /owner/repository/src/branch/branch-name.
    if "/src/branch/" in parsed.path:
        prefix, branch = parsed.path.split("/src/branch/", 1)
        return f"{parsed.scheme}://{parsed.netloc}{prefix}.git", branch.strip("/")

    if len(parts) >= 2:
        # Unknown Git servers are tested as copied. Some do not accept a .git
        # suffix, so avoid rewriting URLs when their web layout is unknown.
        return value.rstrip("/"), None
    return None


def _repository_name(clone_url):
    path = clone_url.rstrip("/")
    # Handles both normal URLs and SCP-style SSH addresses (git@host:repo).
    name = re.split(r"[/:]", path)[-1].removesuffix(".git").strip()
    return name or None


def _clone_repository(repository, destination):
    clone_url, branch = repository
    command = ["git", "clone"]
    if branch:
        command.extend(["--branch", branch, "--single-branch"])
    command.append(clone_url)
    result = subprocess.run(command, cwd=destination, text=True, capture_output=True)
    if result.returncode == 0:
        name = _repository_name(clone_url) or _("Git repository")
        GLib.idle_add(_notify, _("✓ Repository downloaded successfully: ") + name, False)
    else:
        detail = (result.stderr or result.stdout).strip().splitlines()
        message = detail[-1] if detail else _("Unknown Git error")
        GLib.idle_add(_show_error_window, message)


def _scripts_dir():
    user_flatpak = os.path.expanduser("~/.local/share/pkgbuild-manager/scripts")
    if os.path.isdir(user_flatpak):
        return user_flatpak
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
                        sid = item["id"]
                        label = item["label"]
                        if label == sid:
                            label = _(DEFAULT_LABELS.get(sid, label))
                        result.append((sid, label, group["group"]))
            return result, len(data)
        except Exception:
            pass
    return [(sid, _(default_label), ROOT_GROUP) for sid, default_label in DEFAULT_ACTIONS], 1


class PkgbuildMenuProvider(GObject.GObject, Nautilus.MenuProvider):

    def __init__(self):
        super().__init__()
        display = Gdk.Display.get_default()
        self._clipboard = display.get_clipboard() if display else None
        if self._clipboard is not None:
            self._clipboard.connect("changed", self._on_clipboard_changed)

    def _on_clipboard_changed(self, _clipboard):
        """Tell Nautilus to rebuild the current folder's context menu."""
        self.emit_items_updated_signal()

    def _read_clipboard_now(self):
        """Read clipboard text during menu creation, bounded to 100 ms."""
        if self._clipboard is None:
            return None
        loop = GLib.MainLoop()
        value = {"text": None, "finished": False}

        def clipboard_ready(source, result, _data=None):
            try:
                value["text"] = source.read_text_finish(result)
            except GLib.Error:
                pass
            value["finished"] = True
            loop.quit()

        def timed_out():
            loop.quit()
            return False

        self._clipboard.read_text_async(None, clipboard_ready, None)
        GLib.timeout_add(100, timed_out)
        loop.run()
        return value["text"] if value["finished"] else None

    def _build_menu(self, pkgbuild_path):
        scripts = _scripts_dir()
        items, num_groups = _load_menu()

        groups = {}
        group_order = []
        for sid, label, group in items:
            if group not in groups:
                groups[group] = []
                group_order.append(group)
            groups[group].append((sid, label))

        top = Nautilus.MenuItem(
            name="PkgbuildManager::TopMenu",
            label="PKGBUILD",
            tip="PKGBUILD Manager actions",
        )
        top_submenu = Nautilus.Menu()
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
                subprocess.Popen(["bash", spath, pkgpath],
                                 cwd=os.path.dirname(pkgpath), close_fds=True)
            return cb

        def append_items_flat(target_menu, group_name):
            for sid, label in groups.get(group_name, []):
                sp = os.path.join(scripts, sid)
                it = Nautilus.MenuItem(
                    name=f"PkgbuildManager::{sid.replace(' ', '_')}",
                    label=label,
                    tip=f"Run {sid}"
                )
                it.connect("activate", make_cb(sp, pkgbuild_path))
                target_menu.append_item(it)

        # Grupos extras são todos exceto o grupo raiz "PKGBUILD"
        extra_groups = [g for g in group_order if g != ROOT_GROUP]

        if not extra_groups:
            # Caso simples: só o grupo raiz — itens diretos no top_submenu
            append_items_flat(top_submenu, group_order[0] if group_order else ROOT_GROUP)
        else:
            # Itens do grupo raiz vão direto no top_submenu (sem criar submenu extra)
            if ROOT_GROUP in groups:
                append_items_flat(top_submenu, ROOT_GROUP)
                # Separador visual entre itens raiz e subgrupos extras
                sep = Nautilus.MenuItem(
                    name="PkgbuildManager::Sep",
                    label="",
                    tip=""
                )
                top_submenu.append_item(sep)
            # Grupos extras viram submenus aninhados
            for gname in extra_groups:
                git = Nautilus.MenuItem(
                    name=f"PkgbuildManager::G_{gname.replace(' ', '_')}",
                    label=gname,
                    tip=gname
                )
                gsub = Nautilus.Menu()
                git.set_submenu(gsub)
                append_items_flat(gsub, gname)
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
        if not folder.get_uri().startswith("file://"):
            return []
        destination = folder.get_location().get_path()
        if not destination:
            return []

        repository = _repository_from_url(self._read_clipboard_now())
        if repository is None:
            return []

        item = Nautilus.MenuItem(
            name="PkgbuildManager::CloneClipboardRepository",
            label=_("Download ") + (_repository_name(repository[0]) or _("Git repository")),
            tip=_("Clone the repository URL from the clipboard into this folder"),
        )
        item.connect(
            "activate",
            lambda _item: threading.Thread(
                target=_clone_repository,
                args=(repository, destination),
                daemon=True,
            ).start(),
        )
        return [item]
