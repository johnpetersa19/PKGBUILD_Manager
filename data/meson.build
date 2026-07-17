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
  'nautilus-scripts/08_Build Force',
  'nautilus-scripts/09_Build NoCheck',
  'nautilus-scripts/10_Build NoGPG',
  'nautilus-scripts/11_Fetch Sources',
  'nautilus-scripts/12_Install Force',
  'nautilus-scripts/13_Install RmDeps',
  'nautilus-scripts/14_Install NoCheck',
  'nautilus-scripts/15_Install NoGPG',
  'nautilus-scripts/16_Gen Checksums',
  'nautilus-scripts/17_Push AUR Tag',
]

install_data(
  nautilus_scripts,
  install_dir: get_option('datadir') / 'pkgbuild-manager/scripts',
  install_mode: 'rwxr-xr-x',
)

po_files = [
  '../po/pt_BR.po',
  '../po/es.po',
  '../po/en_US.po',
  '../po/de.po',
  '../po/fr.po',
  '../po/it.po',
  '../po/ru.po',
  '../po/zh_CN.po',
  '../po/ja.po',
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

# Nemo extension (optional)
nemo_python_dir = '/usr/share/nemo-python/extensions'
if run_command('test', '-d', nemo_python_dir, check: false).returncode() == 0
  install_data(
    'nautilus-extension/pkgbuild_manager_nemo.py',
    install_dir: nemo_python_dir,
    install_mode: 'rwxr-xr-x',
  )
endif

# Caja extension (optional)
caja_python_dir = '/usr/share/caja-python/extensions'
if run_command('test', '-d', caja_python_dir, check: false).returncode() == 0
  install_data(
    'nautilus-extension/pkgbuild_manager_caja.py',
    install_dir: caja_python_dir,
    install_mode: 'rwxr-xr-x',
  )
endif

# Dolphin KIO Servicemenu (optional)
install_data(
  'dolphin-servicemenu/pkgbuild-manager.desktop',
  install_dir: get_option('datadir') / 'kio/servicemenus',
  install_mode: 'rw-r--r--',
)

# Dolphin desktop regen helper
install_data(
  'regen-dolphin-desktop',
  install_dir: get_option('datadir') / 'pkgbuild-manager',
  install_mode: 'rwxr-xr-x',
)

# .desktop application entry
install_data(
  'applications/pkgbuild-manager-settings.desktop',
  install_dir: get_option('datadir') / 'applications',
)

# Flatpak launcher. Its filename must match the application ID so Flatpak can
# export it to the host desktop.
install_data(
  'applications/io.github.PkgbuildManage.desktop',
  install_dir: get_option('datadir') / 'applications',
)

# Post-install scripts: registra/remove atalho Ctrl+Alt+P no DE do usuário
install_data(
  'post-install/register-shortcut',
  'post-install/unregister-shortcut',
  install_dir: get_option('datadir') / 'pkgbuild-manager',
  install_mode: 'rwxr-xr-x',
)
