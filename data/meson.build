subdir('icons')

nautilus_scripts = [
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
  install_dir: get_option('datadir') / 'pkgbuild-manager/scripts'
)
