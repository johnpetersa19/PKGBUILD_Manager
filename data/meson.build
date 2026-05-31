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

# Nautilus extension (required)
install_data(
  'nautilus-extension/pkgbuild_manager.py',
  install_dir: get_option('datadir') / 'nautilus-python/extensions',
  install_mode: 'rwxr-xr-x',
)

# Nemo extension (optional — only installed if nemo-python is present)
nemo_python_dir = '/usr/share/nemo-python/extensions'
if run_command('test', '-d', nemo_python_dir, check: false).returncode() == 0
  install_data(
    'nautilus-extension/pkgbuild_manager_nemo.py',
    install_dir: nemo_python_dir,
    install_mode: 'rwxr-xr-x',
  )
endif

# Caja extension (optional — only installed if caja-python is present)
caja_python_dir = '/usr/share/caja-python/extensions'
if run_command('test', '-d', caja_python_dir, check: false).returncode() == 0
  install_data(
    'nautilus-extension/pkgbuild_manager_caja.py',
    install_dir: caja_python_dir,
    install_mode: 'rwxr-xr-x',
  )
endif

# Dolphin KIO Servicemenu (optional — only installed if KDE servicemenus dir exists)
dolphin_servicemenu_dir = get_option('datadir') / 'kio/servicemenus'
install_data(
  'dolphin-servicemenu/pkgbuild-manager.desktop',
  install_dir: dolphin_servicemenu_dir,
  install_mode: 'rw-r--r--',
)
