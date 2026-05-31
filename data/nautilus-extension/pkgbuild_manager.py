#!/usr/bin/env python3
# pkgbuild_manager.py — Nautilus Python extension
# Adds a "PKGBUILD" submenu directly in the right-click context menu
# (no intermediate "Scripts" level) when the selected file is named PKGBUILD.
#
# Install to: ~/.local/share/nautilus/extensions/4/  (Nautilus 43+)
#          or ~/.local/share/nautilus/python-extensions/  (Nautilus < 43)
# System-wide: /usr/share/nautilus-python/extensions/
#
# Requires: nautilus-python (python-nautilus on Arch)

import os
import subprocess
import locale
import gi

gi.require_version("Nautilus", "4.0")
from gi.repository import Nautilus, GObject

# ---------------------------------------------------------------------------
# Localisation — detect user locale and pick translated labels
# ---------------------------------------------------------------------------

def _detect_lang() -> str:
    """Return the base language code from the environment (e.g. 'pt', 'es')."""
    for var in ("LANGUAGE", "LC_MESSAGES", "LC_ALL", "LANG"):
        val = os.environ.get(var, "")
        if val:
            # 'pt_BR.UTF-8' → 'pt'
            return val.split("_")[0].split(".")[0].lower()
    return "en"

# Each entry: (internal_script_name, label_per_lang)
# Order here defines the menu order.
_ACTIONS = [
    ("00_Full Workflow", {
        "pt": "Fluxo Completo",
        "es": "Flujo Completo",
        "de": "Vollständiger Ablauf",
        "fr": "Processus complet",
        "it": "Flusso completo",
        "en": "Full Workflow",
    }),
    ("01_Build", {
        "pt": "Compilar",
        "es": "Compilar",
        "de": "Paket bauen",
        "fr": "Compiler",
        "it": "Compilare",
        "en": "Build",
    }),
    ("02b_Build and Clean", {
        "pt": "Compilar e Limpar",
        "es": "Compilar y Limpiar",
        "de": "Bauen und bereinigen",
        "fr": "Compiler et nettoyer",
        "it": "Compilare e pulire",
        "en": "Build and Clean",
    }),
    ("02_Install", {
        "pt": "Instalar",
        "es": "Instalar",
        "de": "Installieren",
        "fr": "Installer",
        "it": "Installare",
        "en": "Install",
    }),
    ("03_Update Checksums", {
        "pt": "Atualizar Checksums",
        "es": "Actualizar Checksums",
        "de": "Prüfsummen aktualisieren",
        "fr": "Mettre à jour les sommes de contrôle",
        "it": "Aggiorna checksum",
        "en": "Update Checksums",
    }),
    ("04_Update .SRCINFO", {
        "pt": "Atualizar .SRCINFO",
        "es": "Actualizar .SRCINFO",
        "de": ".SRCINFO aktualisieren",
        "fr": "Mettre à jour .SRCINFO",
        "it": "Aggiorna .SRCINFO",
        "en": "Update .SRCINFO",
    }),
    ("05b_ShellCheck", {
        "pt": "Verificar com ShellCheck",
        "es": "Verificar con ShellCheck",
        "de": "Mit ShellCheck prüfen",
        "fr": "Vérifier avec ShellCheck",
        "it": "Verifica con ShellCheck",
        "en": "ShellCheck",
    }),
    ("05_Namcap", {
        "pt": "Analisar com Namcap",
        "es": "Analizar con Namcap",
        "de": "Mit Namcap analysieren",
        "fr": "Analyser avec Namcap",
        "it": "Analizza con Namcap",
        "en": "Namcap",
    }),
    ("06_Push AUR", {
        "pt": "Enviar para AUR",
        "es": "Publicar en AUR",
        "de": "An AUR übertragen",
        "fr": "Envoyer vers l'AUR",
        "it": "Pubblica su AUR",
        "en": "Push AUR",
    }),
    ("07b_Clean Everything", {
        "pt": "Limpar Tudo",
        "es": "Limpiar Todo",
        "de": "Alles bereinigen",
        "fr": "Tout nettoyer",
        "it": "Pulisci tutto",
        "en": "Clean Everything",
    }),
    ("07_Clean srcdir", {
        "pt": "Limpar srcdir",
        "es": "Limpiar srcdir",
        "de": "srcdir bereinigen",
        "fr": "Nettoyer srcdir",
        "it": "Pulisci srcdir",
        "en": "Clean srcdir",
    }),
]

# ---------------------------------------------------------------------------
# Resolve the scripts directory (installed or dev fallback)
# ---------------------------------------------------------------------------

def _scripts_dir() -> str:
    installed = "/usr/share/pkgbuild-manager/scripts"
    if os.path.isdir(installed):
        return installed
    # Development fallback: look relative to this file
    here = os.path.dirname(os.path.abspath(__file__))
    dev = os.path.normpath(os.path.join(here, "..", "nautilus-scripts"))
    return dev


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
        lang = _detect_lang()

        # Top-level menu item "PKGBUILD" that opens a submenu
        top = Nautilus.MenuItem(
            name="PkgbuildManager::TopMenu",
            label="PKGBUILD",
            tip="PKGBUILD Manager actions",
        )
        submenu = Nautilus.Menu()
        top.set_submenu(submenu)

        for script_name, labels in _ACTIONS:
            script_path = os.path.join(scripts, script_name)
            if not os.path.exists(script_path):
                continue

            label = labels.get(lang) or labels.get("en", script_name)

            item = Nautilus.MenuItem(
                name=f"PkgbuildManager::{script_name.replace(' ', '_')}",
                label=label,
                tip=f"Run {script_name}",
            )

            # Capture loop variables
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
    candidates = [
        "kgx",          # GNOME Console (default on modern GNOME)
        "gnome-terminal",
        "konsole",
        "xfce4-terminal",
        "xterm",
        "alacritty",
        "foot",
        "kitty",
    ]
    import shutil
    for t in candidates:
        path = shutil.which(t)
        if path:
            return path
    return None
