import os
import shutil
import subprocess
from gi.repository import Nautilus, GObject
from typing import List

# Centralized translations dictionary for dynamic localization (i18n)
TRANSLATIONS = {
    "pt_BR": {
        "00_Full Workflow": "Fluxo Completo",
        "01_Build": "Compilar",
        "02b_Build and Clean": "Compilar e Limpar",
        "02_Install": "Instalar",
        "03_Update Checksums": "Atualizar Checksums",
        "04_Update .SRCINFO": "Atualizar .SRCINFO",
        "05b_ShellCheck": "Verificar com ShellCheck",
        "05_Namcap": "Analisar com Namcap",
        "06_Push AUR": "Enviar para AUR",
        "07b_Clean Everything": "Limpar Tudo",
        "07_Clean srcdir": "Limpar srcdir",
    },
    "es": {
        "00_Full Workflow": "Flujo Completo",
        "01_Build": "Compilar",
        "02b_Build and Clean": "Compilar y Limpiar",
        "02_Install": "Instalar",
        "03_Update Checksums": "Actualizar Checksums",
        "04_Update .SRCINFO": "Actualizar .SRCINFO",
        "05b_ShellCheck": "Verificar con ShellCheck",
        "05_Namcap": "Analizar con Namcap",
        "06_Push AUR": "Publicar en AUR",
        "07b_Clean Everything": "Limpiar Todo",
        "07_Clean srcdir": "Limpiar srcdir",
    },
    "en": {
        "00_Full Workflow": "Full Workflow",
        "01_Build": "Build",
        "02b_Build and Clean": "Build and Clean",
        "02_Install": "Install",
        "03_Update Checksums": "Update Checksums",
        "04_Update .SRCINFO": "Update .SRCINFO",
        "05b_ShellCheck": "ShellCheck",
        "05_Namcap": "Namcap",
        "06_Push AUR": "Push AUR",
        "07b_Clean Everything": "Clean Everything",
        "07_Clean srcdir": "Clean srcdir",
    }
}

# Ordered scripts list to guarantee precise menu sequencing
ACTIONS = [
    ("00_Full Workflow", "00_Full Workflow"),
    ("01_Build", "01_Build"),
    ("02b_Build and Clean", "02b_Build and Clean"),
    ("02_Install", "02_Install"),
    ("03_Update Checksums", "03_Update Checksums"),
    ("04_Update .SRCINFO", "04_Update .SRCINFO"),
    ("05b_ShellCheck", "05b_ShellCheck"),
    ("05_Namcap", "05_Namcap"),
    ("06_Push AUR", "06_Push AUR"),
    ("07b_Clean Everything", "07b_Clean Everything"),
    ("07_Clean srcdir", "07_Clean srcdir"),
]

def get_locale() -> str:
    """Detect the environment language using standard variables."""
    for var in ["LC_ALL", "LC_MESSAGES", "LANG"]:
        val = os.environ.get(var)
        if val:
            lang = val.split('.')[0]
            if lang in TRANSLATIONS:
                return lang
            lang_short = lang.split('_')[0]
            if lang_short in TRANSLATIONS:
                return lang_short
    return "en"

class PkgbuildManagerExtension(GObject.GObject, Nautilus.MenuProvider):
    def __init__(self):
        super().__init__()

    def _run_action(self, menu: Nautilus.MenuItem, script_name: str, target_dir: str) -> None:
        """Execute the target bash script from the centralized scripts directory."""
        script_path = os.path.join("/usr/share/pkgbuild-manager/scripts", script_name)
        # Fallback to local development path if system path does not exist
        if not os.path.exists(script_path):
            script_path = os.path.join(os.path.expanduser("~"), "Projects/PKGBUILD_Manager/data/nautilus-scripts", script_name)

        if os.path.exists(script_path):
            subprocess.Popen(["bash", script_path, target_dir])
        else:
            subprocess.run([
                "notify-send", "-u", "critical", "PKGBUILD Manager",
                f"Script not found: {script_name}"
            ])

    def _create_menu(self, target_path: str) -> List[Nautilus.MenuItem]:
        """Generate the localized PKGBUILD context menu."""
        if os.path.isfile(target_path):
            target_dir = os.path.dirname(target_path)
        else:
            target_dir = target_path

        locale_code = get_locale()
        
        main_item = Nautilus.MenuItem(
            name="PkgbuildManager::PKGBUILD",
            label="PKGBUILD",
            tip="PKGBUILD Manager"
        )
        
        submenu = Nautilus.Menu()
        main_item.set_submenu(submenu)
        
        for i, (script_name, gettext_key) in enumerate(ACTIONS):
            label = TRANSLATIONS.get(locale_code, TRANSLATIONS["en"]).get(gettext_key, script_name)
            item = Nautilus.MenuItem(
                name=f"PkgbuildManager::Action_{i}",
                label=label,
                tip=label
            )
            item.connect("activate", self._run_action, script_name, target_dir)
            submenu.append_item(item)
            
        return [main_item]

    def get_file_items(self, files: List[Nautilus.FileInfo]) -> List[Nautilus.MenuItem]:
        """Add PKGBUILD menu when a PKGBUILD file or directory containing it is selected."""
        if len(files) != 1:
            return []
        
        file = files[0]
        # Skip remote virtual files (only support local filesystem)
        if file.get_uri_scheme() != "file":
            return []
            
        path = file.get_location().get_path()
        
        if not file.is_directory():
            if file.get_name() == "PKGBUILD":
                return self._create_menu(path)
        else:
            if os.path.exists(os.path.join(path, "PKGBUILD")):
                return self._create_menu(path)
                
        return []

    def get_background_items(self, folder: Nautilus.FileInfo) -> List[Nautilus.MenuItem]:
        """Add PKGBUILD menu when right-clicking on a folder background containing a PKGBUILD."""
        if folder.get_uri_scheme() != "file":
            return []
            
        path = folder.get_location().get_path()
        if os.path.exists(os.path.join(path, "PKGBUILD")):
            return self._create_menu(path)
            
        return []
