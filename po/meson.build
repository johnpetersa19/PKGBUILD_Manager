# po/meson.build
#
# Automatic .pot generation from all translatable sources:
#   - Rust files listed in POTFILES.in  (keyword: gettext)
#   - Python files listed in POTFILES.in (keyword: _)
#   - data/nautilus-extension/*.py       (keyword: _)
#   - bash notify_* keys injected as a static .pot fragment
#
# The final merged .pot is written to po/pkgbuild_manager.pot.
# Every existing .po is updated via msgmerge automatically.
# .mo files are compiled and installed by i18n.gettext().

i18n = import('i18n')
fs   = import('fs')

pkg      = meson.project_name()          # 'pkgbuild_manager'
po_dir   = meson.current_source_dir()
root_dir = meson.project_source_root()

# ─── 1. Collect Rust source files ────────────────────────────────────────────
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

# ─── 2. Collect Python source files ──────────────────────────────────────────
python_sources = [
  root_dir / 'src/settings/app.py',
  root_dir / 'data/nautilus-extension/pkgbuild_manager.py',
  root_dir / 'data/nautilus-extension/pkgbuild_manager_caja.py',
  root_dir / 'data/nautilus-extension/pkgbuild_manager_nemo.py',
]

# ─── 3. Extract .pot from Rust (xgettext --language=C --keyword=gettext) ─────
xgettext = find_program('xgettext', required: true)

rust_pot = custom_target(
  'extract-rust-pot',
  input:   rust_sources,
  output:  'rust.pot',
  command: [
    xgettext,
    '--from-code=UTF-8',
    '--language=C',
    '--keyword=gettext',
    '--add-comments=translators',
    '--package-name=' + pkg,
    '--package-version=' + meson.project_version(),
    '--output=@OUTPUT@',
    '@INPUT@',
  ],
  build_by_default: false,
)

# ─── 4. Extract .pot from Python ─────────────────────────────────────────────
python_pot = custom_target(
  'extract-python-pot',
  input:   python_sources,
  output:  'python.pot',
  command: [
    xgettext,
    '--from-code=UTF-8',
    '--language=Python',
    '--keyword=_',
    '--add-comments=translators',
    '--package-name=' + pkg,
    '--package-version=' + meson.project_version(),
    '--output=@OUTPUT@',
    '@INPUT@',
  ],
  build_by_default: false,
)

# ─── 5. Static bash notify_* fragment ────────────────────────────────────────
# The bash scripts use symbolic keys (notify_building, notify_build_ok, …)
# looked up at runtime by _i18n. xgettext cannot extract them automatically.
# This fragment is the authoritative list — add new notify_* keys here
# whenever you add a new bash script operation.
bash_pot_content = '''# PKGBUILD Manager — bash script notification keys
# These are symbolic keys resolved at runtime by data/nautilus-scripts/_i18n.
# ADD new entries here when adding new bash script operations.
msgid ""
msgstr ""
"Content-Type: text/plain; charset=UTF-8\\n"
"Content-Transfer-Encoding: 8bit\\n"

msgid "notify_building"
msgstr ""

msgid "notify_build_ok"
msgstr ""

msgid "notify_build_fail"
msgstr ""

msgid "notify_installing"
msgstr ""

msgid "notify_install_ok"
msgstr ""

msgid "notify_install_fail"
msgstr ""

msgid "notify_checksums"
msgstr ""

msgid "notify_checksums_ok"
msgstr ""

msgid "notify_checksums_fail"
msgstr ""

msgid "notify_srcinfo"
msgstr ""

msgid "notify_srcinfo_ok"
msgstr ""

msgid "notify_srcinfo_fail"
msgstr ""

msgid "notify_namcap"
msgstr ""

msgid "notify_namcap_warn"
msgstr ""

msgid "notify_namcap_ok"
msgstr ""

msgid "notify_namcap_fail"
msgstr ""

msgid "notify_shellcheck"
msgstr ""

msgid "notify_shellcheck_warn"
msgstr ""

msgid "notify_shellcheck_ok"
msgstr ""

msgid "notify_shellcheck_fail"
msgstr ""

msgid "notify_push_aur"
msgstr ""

msgid "notify_push_aur_ok"
msgstr ""

msgid "notify_push_aur_fail"
msgstr ""

msgid "notify_clean_src"
msgstr ""

msgid "notify_clean_src_ok"
msgstr ""

msgid "notify_clean_src_fail"
msgstr ""

msgid "notify_clean_all"
msgstr ""

msgid "notify_clean_all_ok"
msgstr ""

msgid "notify_clean_all_fail"
msgstr ""

msgid "notify_full_workflow"
msgstr ""

msgid "notify_full_workflow_ok"
msgstr ""

msgid "notify_full_workflow_fail"
msgstr ""
'''

bash_pot_file = configure_file(
  input:         'bash_notify.pot.in',
  output:        'bash_notify.pot',
  copy:          true,
)

# ─── 6. Merge all .pot fragments into the final pkgbuild_manager.pot ──────────
msgcat = find_program('msgcat', required: true)

final_pot = custom_target(
  'generate-pot',
  input:   [rust_pot, python_pot, po_dir / 'bash_notify.pot.in'],
  output:  'pkgbuild_manager.pot',
  command: [
    msgcat,
    '--use-first',
    '--output=@OUTPUT@',
    rust_pot,
    python_pot,
    po_dir / 'bash_notify.pot.in',
  ],
  build_by_default: false,
  install:          false,
)

# ─── 7. Update existing .po files (msgmerge) and compile .mo ─────────────────
# i18n.gettext() handles:
#   - msgmerge  for each language in LINGUAS
#   - msgfmt    to compile .mo files
#   - installation of .mo to $localedir/<lang>/LC_MESSAGES/
i18n.gettext(pkg,
  args:         ['--keyword=gettext', '--keyword=_'],
  preset:       'glib',
  install:      true,
  install_dir:  get_option('localedir'),
)
