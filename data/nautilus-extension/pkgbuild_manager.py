#!/usr/bin/env python3
# pkgbuild_manager.py — Nautilus Python extension
# Adds a "PKGBUILD" submenu directly in the right-click context menu.
# Labels are loaded from installed .mo files via gettext — add a new .po
# file and recompile to support a new language, no changes needed here.
#
# Install to: /usr/share/nautilus-python/extensions/  (system-wide, via meson)
#          or ~/.local/share/nautilus/extensions/4/    (Nautilus 43+, per-user)
#
# Requires: nautilus-python (python-nautilus on Arch)

import os
import gettext
import subprocess
import shutil
import gi

gi.require_version("Nautilus", "4.0")
from gi.repository import Nautilus, GObject

# ---------------------------------------------------------------------------
# Gettext setup — reads compiled .mo from the standard locale directory.
# PKGBUILD_MANAGER_LOCALEDIR env var allows overriding for development.
# ---------------------------------------------------------------------------

_DOMAIN = "pkgbuild_manager"
_LOCALEDIR = os.environ.get(
    "PKGBUILD_MANAGER_LOCALEDIR",
    "/usr/share/locale",
)

# Load the translation for the current locale (falls back to msgid if missing)
_t = gettext.translation(_DOMAIN, localedir=_LOCALEDIR, fallback=True)
_ = _t.gettext

# ---------------------------------------------------------------------------
# Action list — (internal_script_name, gettext_msgid)
# The msgid must match exactly what is in the .po/.mo files.
# To add a new language: create po/<lang>.po with the msgids below translated,
# run `meson compile` — no changes needed in this file.
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
# Resolve the scripts directory (installed or dev fallback)
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
        # Only show when exactly one file called "PKGBUILD" is selected
        if len(files) != 1:
            return []
        f = files[0]
        if f.get_name() != "PKGBUILD":
            return []
        if f.get_file_type() != Nautilus.FileType.REGULAR:
            return []

        pkgbuild_path = f.get_location().get_path()
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
            if not os.path.exists(script_path):
                continue

            # gettext returns the translated label; falls back to msgid if
            # no .mo is installed or the msgid has no translation yet
            label = _(msgid)

            item = Nautilus.MenuItem(
                name=f"PkgbuildManager::{script_name.replace(' ', '_')}",
                label=label,
                tip=f"Run {script_name}",
            )

            def make_callback(spath, pkgpath):
                def cb(_item):
                    terminal = _find_terminal()
                    run_helper = os.path.join(os.path.dirname(spath), "_run_in_terminal")
                    env = os.environ.copy()
                    env["NAUTILUS_SCRIPT_SELECTED_FILE_PATHS"] = pkgpath + "\n"
                    if terminal and os.path.exists(run_helper):
                        subprocess.Popen(
                            [terminal, "--", run_helper, spath, pkgpath],
                            env=env,
                        )
                    else:
                        subprocess.Popen(
                            ["bash", spath],
                            env=env,
                            cwd=os.path.dirname(pkgpath),
                        )
                return cb

            item.connect("activate", make_callback(script_path, pkgbuild_path))
            submenu.append_item(item)

        return [top]

    def get_file_items(self, files):
        return self._get_items(files)

    def get_background_items(self, folder):
        return []


def _find_terminal() -> str | None:
    """Return the path to an available terminal emulator, or None."""
    for t in ("kgx", "gnome-terminal", "konsole", "xfce4-terminal", "xterm", "alacritty", "foot", "kitty"):
        path = shutil.which(t)
        if path:
            return path
    return None
