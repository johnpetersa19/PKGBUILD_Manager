subdir('icons')

nautilus_scripts = [
  'nautilus-scripts/_i18n',
  'nautilus-scripts/_run_in_terminal',
  'nautilus-scripts/00_Full Workflow',
  'nautilus-scripts/01_Build',
  'nautilus-scripts/02_Install',
  'nautilus-scripts/02b_Build and Clean',
  'nautilus-scripts/03_Update Checksums',
  'nautilus-scripts/04_Update .SRCINFO',
  'nautilus-scripts/05_Namcap',
  'nautilus-scripts/05b_ShellCheck',
  'nautilus-scripts/06_Push AUR',
  'nautilus-scripts/07_Clean srcdir',
  'nautilus-scripts/07b_Clean Everything',
]

install_data(
  nautilus_scripts,
  install_dir: get_option('datadir') / 'pkgbuild-manager/scripts',
  install_mode: 'rwxr-xr-x',
)

# Plain .po files for the bash _i18n helper.
# _i18n reads text .po files at runtime — it cannot use compiled .mo binaries.
# These are installed to /usr/share/pkgbuild-manager/i18n/<lang>.po.
po_files = [
  '../po/pt_BR.po',
  '../po/es.po',
  '../po/de.po',
  '../po/fr.po',
  '../po/it.po',
]

install_data(
  po_files,
  install_dir: get_option('datadir') / 'pkgbuild-manager/i18n'
)

# Nautilus Python extension — installed system-wide so Nautilus picks it up
# without needing setup-nautilus. Requires nautilus-python (python-nautilus).
install_data(
  'nautilus-extension/pkgbuild_manager.py',
  install_dir: get_option('datadir') / 'nautilus-python/extensions'
)
