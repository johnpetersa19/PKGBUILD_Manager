# po/meson.build
#
# .mo files are compiled from the .po files in this directory by i18n.gettext().
# The list of languages is read from the LINGUAS file.
#
# .pot generation (for translators) is a separate optional step:
#   meson compile -C build generate-pot
#
# i18n.gettext() handles:
#   - msgfmt    to compile .po -> .mo
#   - installation of .mo to $localedir/<lang>/LC_MESSAGES/

i18n = import('i18n')

pkg      = meson.project_name()   # 'pkgbuild_manager'
po_dir   = meson.current_source_dir()
root_dir = meson.project_source_root()

# ─── Compile and install .mo files ───────────────────────────────────────────
# Reads LINGUAS for the list of languages.
# Looks for <lang>.po in the current source directory (po/).
# Compiles each .po -> .mo with msgfmt.
# Installs to $localedir/<lang>/LC_MESSAGES/pkgbuild_manager.mo
i18n.gettext(pkg,
  args:        ['--keyword=gettext', '--keyword=_'],
  preset:      'glib',
  install:     true,
  install_dir: get_option('localedir'),
)

# ─── Optional: regenerate .pot from sources (not part of normal build) ───────
# Run with: meson compile -C build generate-pot
xgettext = find_program('xgettext', required: false)
msgcat   = find_program('msgcat',   required: false)

if xgettext.found() and msgcat.found()
  rust_sources = [
    root_dir / 'src/actions/aur_push.rs',
    root_dir / 'src/actions/build.rs',
    root_dir / 'src/actions/checksums.rs',
    root_dir / 'src/actions/clean.rs',
    root_dir / 'src/actions/install.rs',
    root_dir / 'src/actions/mod.rs',
    root_dir / 'src/actions/namcap.rs',
    root_dir / 'src/actions/shellcheck.rs',
    root_dir / 'src/actions/srcinfo.rs',
    root_dir / 'src/config.rs',
    root_dir / 'src/main.rs',
  ]

  python_sources = [
    root_dir / 'src/settings/app.py',
    root_dir / 'data/nautilus-extension/pkgbuild_manager.py',
    root_dir / 'data/nautilus-extension/pkgbuild_manager_caja.py',
    root_dir / 'data/nautilus-extension/pkgbuild_manager_nemo.py',
  ]

  rust_pot = custom_target(
    'extract-rust-pot',
    input:            rust_sources,
    output:           'rust.pot',
    build_by_default: false,
    command: [
      xgettext,
      '--from-code=UTF-8', '--language=C', '--keyword=gettext',
      '--add-comments=translators',
      '--package-name=' + pkg,
      '--package-version=' + meson.project_version(),
      '--output=@OUTPUT@', '@INPUT@',
    ],
  )

  python_pot = custom_target(
    'extract-python-pot',
    input:            python_sources,
    output:           'python.pot',
    build_by_default: false,
    command: [
      xgettext,
      '--from-code=UTF-8', '--language=Python', '--keyword=_',
      '--add-comments=translators',
      '--package-name=' + pkg,
      '--package-version=' + meson.project_version(),
      '--output=@OUTPUT@', '@INPUT@',
    ],
  )

  custom_target(
    'generate-pot',
    input:            [rust_pot, python_pot, po_dir / 'bash_notify.pot.in'],
    output:           'pkgbuild_manager.pot',
    build_by_default: false,
    install:          false,
    command: [
      msgcat, '--use-first', '--output=@OUTPUT@',
      rust_pot, python_pot, po_dir / 'bash_notify.pot.in',
    ],
  )
endif
