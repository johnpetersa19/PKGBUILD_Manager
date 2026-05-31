#!/usr/bin/env python3
# pkgbuild_manager.py — Nautilus Python extension
# Adds a "PKGBUILD" submenu directly in the right-click context menu.
# Labels are loaded from installed .mo files via gettext.
#
# Install to: /usr/share/nautilus-python/extensions/  (system-wide, via meson)
#
# Requires: nautilus-python (python-nautilus on Arch)

import os
import gettext
import subprocess
import gi

gi.require_version("Nautilus", "4.1")
from gi.repository import Nautilus, GObject

# ---------------------------------------------------------------------------
# Lazy gettext — initialised on first use so the GNOME session locale
# (LANG / LC_MESSAGES) is already set when we look up translations.
# ---------------------------------------------------------------------------

_DOMAIN = "pkgbuild_manager"
_LOCALEDIR = os.environ.get("PKGBUILD_MANAGER_LOCALEDIR", "/usr/share/locale")
_gettext_func = None

def _tr(msgid: str) -> str:
    """Translate msgid, initialising gettext on the first call."""
    global _gettext_func
    if _gettext_func is None:
        t = gettext.translation(_DOMAIN, localedir=_LOCALEDIR, fallback=True)
        _gettext_func = t.gettext
    return _gettext_func(msgid)


# ---------------------------------------------------------------------------
# Action list — (internal_script_name, gettext_msgid)
# Order here defines the menu order shown to the user.
# ---------------------------------------------------------------------------

_ACTIONS = [
    ("00_Full Workflow",     "00_Full Workflow"),
    ("01_Build",             "01_Build"),
    ("02b_Build and Clean",  "02b_Build and Clean"),
    ("02_Install",           "02_Install"),
    ("03_Update Checksums",  "03_Update Checksums"),
    ("04_Update .SRCINFO",   "04_Update .SRCINFO"),
    ("05b_ShellCheck",       "05b_ShellCheck"),
    ("05_Namcap",            "05_Namcap"),
    ("06_Push AUR",          "06_Push AUR"),
    ("07b_Clean Everything", "07b_Clean Everything"),
    ("07_Clean srcdir",      "07_Clean srcdir"),
]

# ---------------------------------------------------------------------------
# Resolve the scripts directory
# ---------------------------------------------------------------------------

def _scripts_dir() -> str:
    installed = "/usr/share/pkgbuild-manager/scripts"
    if os.path.isdir(installed):
        return installed
    here = os.path.dirname(os.path.abspath(__file__))
    return os.path.normpath(os.path.join(here, "..", "nautilus-scripts"))


# ---------------------------------------------------------------------------
# Nautilus extension class
# ---------------------------------------------------------------------------

class PkgbuildMenuProvider(GObject.GObject, Nautilus.MenuProvider):
    """Injects a PKGBUILD submenu into the Nautilus right-click context menu."""

    def _get_items(self, files):
        # Only show when exactly one local file called "PKGBUILD" is selected
        if len(files) != 1:
            return []
        f = files[0]

        # Guard against remote/trash URIs where get_path() returns None
        if not f.get_uri().startswith("file://"):
            return []
        if f.get_name() != "PKGBUILD":
            return []
        if f.get_file_type() != Nautilus.FileType.REGULAR:
            return []

        pkgbuild_path = f.get_location().get_path()
        if pkgbuild_path is None:
            return []

        scripts = _scripts_dir()

        top = Nautilus.MenuItem(
            name="PkgbuildManager::TopMenu",
            label="PKGBUILD",
            tip="PKGBUILD Manager actions",
        )
        submenu = Nautilus.Menu()
        top.set_submenu(submenu)

        for script_name, msgid in _ACTIONS:
            script_path = os.path.join(scripts, script_name)

            # Skip scripts that are not installed or not executable
            if not os.path.isfile(script_path) or not os.access(script_path, os.X_OK):
                continue

            label = _tr(msgid)

            item = Nautilus.MenuItem(
                name=f"PkgbuildManager::{script_name.replace(' ', '_')}",
                label=label,
                tip=f"Run {script_name}",
            )

            def make_callback(spath, pkgpath):
                def cb(_item):
                    subprocess.Popen(
                        ["bash", spath, pkgpath],
                        cwd=os.path.dirname(pkgpath),
                        close_fds=True,
                    )
                return cb

            item.connect("activate", make_callback(script_path, pkgbuild_path))
            submenu.append_item(item)

        return [top]

    def get_file_items(self, files):
        return self._get_items(files)

    def get_background_items(self, folder):
        return []
