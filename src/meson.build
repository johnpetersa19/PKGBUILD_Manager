pkgdatadir = get_option('prefix') / get_option('datadir') / meson.project_name()
gnome = import('gnome')

conf = configuration_data()
conf.set_quoted('VERSION', meson.project_version())
conf.set_quoted('GETTEXT_PACKAGE', 'pkgbuild_manager')
conf.set_quoted('LOCALEDIR', get_option('prefix') / get_option('localedir'))
conf.set_quoted('PKGDATADIR', pkgdatadir)

configure_file(
    input: 'config.rs.in',
    output: 'config.rs',
    configuration: conf
)

# Copy the config.rs output to the source directory.
run_command(
  'cp',
  meson.project_build_root() / 'src' / 'config.rs',
  meson.project_source_root() / 'src' / 'config.rs',
  check: true
)

cargo_bin  = find_program('cargo')
cargo_opt  = [ '--manifest-path', meson.project_source_root() / 'Cargo.toml' ]
cargo_opt += [ '--target-dir', meson.project_build_root()  / 'src' ]
cargo_env  = [ 'CARGO_HOME=' + meson.project_build_root()  / 'cargo-home' ]

if get_option('buildtype') == 'release'
  cargo_opt += [ '--release' ]
  rust_target = 'release'
else
  rust_target = 'debug'
endif

cargo_build = custom_target(
  'cargo-build',
  build_by_default: true,
  build_always_stale: true,
  output: meson.project_name(),
  console: true,
  install: true,
  install_dir: get_option('bindir'),
  command: [
    'env', cargo_env,
    cargo_bin, 'build',
    cargo_opt,
    '&&', 'cp', 'src' / rust_target / meson.project_name(), '@OUTPUT@',
  ]
)

custom_target(
  'cargo-build-aur-push',
  build_by_default: true,
  build_always_stale: true,
  output: 'pkgbuild-manager-aur-push',
  console: true,
  install: true,
  install_dir: get_option('bindir'),
  depends: cargo_build,
  command: [
    'cp',
    meson.project_build_root() / 'src' / rust_target / 'pkgbuild-manager-aur-push',
    '@OUTPUT@',
  ]
)

custom_target(
  'cargo-build-release-gui',
  build_by_default: true,
  build_always_stale: true,
  output: 'pkgbuild-manager-release-gui',
  console: true,
  install: true,
  install_dir: get_option('bindir'),
  depends: cargo_build,
  command: [
    'cp',
    meson.project_build_root() / 'src' / rust_target / 'pkgbuild-manager-release-gui',
    '@OUTPUT@',
  ]
)

custom_target(
  'cargo-build-settings',
  build_by_default: true,
  build_always_stale: true,
  output: 'pkgbuild-manager-settings',
  console: true,
  install: true,
  install_dir: get_option('bindir'),
  depends: cargo_build,
  command: [
    'cp',
    meson.project_build_root() / 'src' / rust_target / 'pkgbuild-manager-settings',
    '@OUTPUT@',
  ]
)
