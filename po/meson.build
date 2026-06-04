# po/meson.build
#
# Compiles .po files into .mo binaries and installs them.
# Language list is read from the LINGUAS file in this directory.
#
# To regenerate the .pot template from sources, run:
#   meson compile -C build generate-pot

i18n = import('i18n')

pkg      = meson.project_name()   # 'pkgbuild_manager'
po_dir   = meson.current_source_dir()
root_dir = meson.project_source_root()

# ─── Compile and install .mo files ─────────────────────────────────────────
# Reads LINGUAS for the list of languages.
# For each language, compiles po/<lang>.po -> .mo with msgfmt.
# Installs to $localedir/<lang>/LC_MESSAGES/pkgbuild_manager.mo
i18n.gettext(pkg,
  args:        ['--keyword=gettext', '--keyword=_'],
  install:     true,
  install_dir: get_option('localedir'),
)

# ─── Optional: regenerate .pot from sources (not part of normal build) ───────
# Run with: meson compile -C build generate-pot
xgettext = find_program('xgettext', required: false)
msgcat   = find_program('msgcat',   required: false)

if xgettext.found() and msgcat.found()
  rust_sources = [
    # CLI core
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
    # GUI — aur_push_gui
    root_dir / 'src/aur_push_gui/aur_dialog.rs',
    root_dir / 'src/aur_push_gui/release_dialog.rs',
    # GUI — settings_gui
    root_dir / 'src/settings_gui/app.rs',
  ]

  python_sources = [
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
