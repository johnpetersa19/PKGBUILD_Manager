# PKGBUILD Manager

A Rust CLI tool and file-manager context-menu integration for Arch Linux package maintainers. It automates common tasks around writing, testing, updating, and publishing `PKGBUILD` files to the AUR (Arch User Repository).

Interactive actions (compilation, linting, etc.) open a terminal window. Background tasks (checksums, clean, etc.) run silently and report their result via `notify-send`.

---

## Features

- **Compilation & Installation** — Multiple `makepkg` wrappers: clean builds, forced builds, skip-check, skip-GPG, fetch-only, and custom flags.
- **Metadata Management** — Update checksums via `updpkgsums`, generate checksums to stdout, and regenerate `.SRCINFO` automatically.
- **Quality Assurance** — Lint with `namcap` (PKGBUILD + compiled packages) and `shellcheck` (bash-mode).
- **Git & AUR Integration** — Auto-generates `upgpkg: <name> <version>-<release>` commit messages from `.SRCINFO`, always stages `.SRCINFO` before committing, and supports annotated version tags.
- **Configurable Context Menu** — A GTK4 + Libadwaita settings panel lets you organise the 21 available actions into named groups, reorder them, rename labels, and toggle items on/off. Changes are saved to `~/.config/pkgbuild-manager/menu.json` and applied to Nautilus without any `sudo`.
- **Multi-file-manager Support** — Context-menu extensions for **Nautilus**, **Caja**, and **Nemo**. Dolphin/KDE is supported via an auto-generated `.desktop` file written to `~/.local/share/kio/servicemenus/` (no root required).
- **Full Internationalization (i18n)** — All menu labels, desktop notifications, and CLI strings are translated. Languages supported: **English** (default), **Português (pt_BR)**, **Español (es)**, **Deutsch (de)**, **Français (fr)**, **Italiano (it)**. Locale is detected automatically from `$LANGUAGE` / `$LC_MESSAGES` / `$LANG`.

---

## Settings Panel

> **How to open:** press <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>P</kbd> from any window, or run `pkgbuild-manager-settings` in a terminal.
> The app does **not** appear in the application grid by design — it is meant to be launched via the keyboard shortcut.

The settings panel (`pkgbuild-manager-settings`) is a GTK4 + Libadwaita application that lets you fully customise the context-menu layout:

- **Create / delete / rename groups** — organise actions into labelled submenus.
- **Add items from the full catalogue** — click *+ Add Item* inside any group to pick from all 21 available actions.
- **Reorder groups and items** with the ↑ / ↓ buttons.
- **Enable / disable individual items** with a toggle switch.
- **Save** writes `~/.config/pkgbuild-manager/menu.json`, restarts Nautilus in the background (reopening every window in the same folder), and regenerates the Dolphin `.desktop` — all without requesting a password.
- **Reset** restores the built-in default layout.

---

## Context-Menu Actions

All 21 actions are available to assign to any group in the settings panel. The numeric prefix controls ordering and is never shown to the user.

| Script | Default label (EN) | Underlying command |
|---|---|---|
| `00_Full Workflow` | Full Workflow | `makepkg -si` + srcinfo + push |
| `01_Build` | Build | `makepkg` |
| `02b_Build and Clean` | Build and Clean | `makepkg -c` |
| `08_Build Force` | Build Force | `makepkg -f` |
| `09_Build NoCheck` | Build NoCheck | `makepkg --nocheck` |
| `10_Build NoGPG` | Build NoGPG | `makepkg --skippgpcheck` |
| `11_Fetch Sources` | Fetch Sources | `makepkg -o` |
| `02_Install` | Install | `makepkg -si` |
| `12_Install Force` | Install Force | `makepkg -sif` |
| `13_Install RmDeps` | Install RmDeps | `makepkg -sir` |
| `14_Install NoCheck` | Install NoCheck | `makepkg -si --nocheck` |
| `15_Install NoGPG` | Install NoGPG | `makepkg -si --skippgpcheck` |
| `03_Update Checksums` | Update Checksums | `updpkgsums` |
| `04_Update .SRCINFO` | Update .SRCINFO | `makepkg --printsrcinfo > .SRCINFO` |
| `16_Gen Checksums` | Gen Checksums | `makepkg -g` |
| `05_Namcap` | Namcap | `namcap PKGBUILD *.pkg.tar.*` |
| `05b_ShellCheck` | ShellCheck | `shellcheck --shell=bash PKGBUILD` |
| `06_Push AUR` | Push AUR | `git add && git commit && git push` |
| `17_Push AUR Tag` | Push AUR Tag | `git tag -a <tag> && git push --tags` |
| `07_Clean srcdir` | Clean srcdir | `makepkg -c` |
| `07b_Clean Everything` | Clean Everything | `rm -rf src/ pkg/ *.pkg.tar.*` |

> The default menu layout groups these into five sections: **Actions**, **Metadata**, **Audit**, **Git / AUR**, and **Clean**.

---

## Dependencies

### Build Dependencies
- **Rust / Cargo** — compiles the backend dispatcher
- **Meson** & **Ninja** — build system
- **gettext** — compiles `.mo` translation files

### Runtime Dependencies
- **python-nautilus** — Nautilus Python extension host
- **libnotify** — `notify-send` for background-task notifications
- **pacman-contrib** — `updpkgsums` (checksums command)
- **namcap** — audit command *(optional)*
- **shellcheck** — lint command *(optional)*
- **python-gobject** — required by the settings panel and all three file-manager extensions

### Optional File Managers
- **nautilus** — GNOME Files
- **caja** — MATE Files
- **nemo** — Cinnamon Files
- **dolphin** — KDE (uses the auto-generated `~/.local/share/kio/servicemenus/pkgbuild-manager.desktop`)

---

## Installation

### Via Meson (recommended)

```bash
meson setup build
meson compile -C build
sudo meson install -C build
```

This installs:
- `pkgbuild_manager` binary → `/usr/bin/`
- Action scripts → `/usr/share/pkgbuild-manager/scripts/`
- `.po` translation files → `/usr/share/pkgbuild-manager/i18n/`
- Compiled `.mo` files → `/usr/share/locale/<lang>/LC_MESSAGES/`
- Nautilus extension → `/usr/share/nautilus-python/extensions/`
- Caja extension → `/usr/share/caja-python/extensions/`
- Nemo extension → `/usr/share/nemo-python/extensions/`
- `regen-dolphin-desktop` helper → `/usr/share/pkgbuild-manager/`
- Settings panel → installed as `pkgbuild-manager-settings`

### Via Cargo (manual)

```bash
cargo build --release
sudo cp target/release/pkgbuild_manager /usr/local/bin/
```

### First-Run Setup

After installation, open the settings panel to generate your initial `menu.json`:

- **Press <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>P</kbd>** — recommended, works from any window.
- Or run `pkgbuild-manager-settings` in a terminal.

> The settings app does **not** appear in the application grid by design (`NoDisplay=true`). Use the keyboard shortcut to open it.

Or use the CLI to set up Nautilus scripts directly:

```bash
pkgbuild_manager setup-nautilus
# For a manual build without installed locale files:
PKGBUILD_MANAGER_LOCALEDIR=build/po pkgbuild_manager setup-nautilus
```

---

## How the Settings Panel Applies Changes

1. **Saves** `~/.config/pkgbuild-manager/menu.json` — no root needed, this is a user file.
2. **Restarts Nautilus** — queries all open window locations via DBus, kills Nautilus with `nautilus -q`, waits ~1 s, then relaunches each window at its original path. The file manager extensions read `menu.json` fresh on every process start.
3. **Regenerates the Dolphin service menu** — writes `~/.local/share/kio/servicemenus/pkgbuild-manager.desktop` from the current `menu.json`. No `pkexec` or `sudo` prompt is shown because the target path is user-owned.

---

## CLI Usage

Run `pkgbuild_manager <command> [path] [flags...]`

If no `path` is specified, defaults to the current working directory.

You can also use `--` to disambiguate pre-path flags from post-path flags:

- `pkgbuild_manager build -c -f` → acts on `.` with flags `-c -f`
- `pkgbuild_manager build /path/to/pkg -- -c -f` → acts on `/path/to/pkg` with flags `-c -f`

### Available Subcommands

| Category | Subcommand | Description | Equivalent Command |
|---|---|---|---|
| **Build** | `build` | Compile the package | `makepkg` |
| | `build-clean` | Compile and clean after | `makepkg -c` |
| | `build-force` | Force recompilation | `makepkg -f` |
| | `build-nocheck` | Skip `check()` functions | `makepkg --nocheck` |
| | `build-nogpg` | Skip PGP checks | `makepkg --skippgpcheck` |
| | `build-custom` | Custom flags | `makepkg [flags]` |
| | `fetch-sources` | Download sources only | `makepkg -o` |
| **Install** | `install` | Build + install | `makepkg -si` |
| | `install-clean` | Build + install + clean | `makepkg -sic` |
| | `install-force` | Force build + install | `makepkg -sif` |
| | `install-rmdeps` | Install + remove build deps | `makepkg -sir` |
| | `install-nocheck` | Install, skip tests | `makepkg -si --nocheck` |
| | `install-nogpg` | Install, skip GPG | `makepkg -si --skippgpcheck` |
| | `install-custom` | Custom flags | `makepkg -si [flags]` |
| **Metadata** | `checksums` | Update checksums in PKGBUILD | `updpkgsums` |
| | `genchecksums` | Print checksums to stdout | `makepkg -g` |
| | `srcinfo` | Regenerate `.SRCINFO` | `makepkg --printsrcinfo > .SRCINFO` |
| **Audit** | `namcap` | Audit PKGBUILD + packages | `namcap PKGBUILD *.pkg.tar.*` |
| | `shellcheck` | Lint PKGBUILD | `shellcheck --shell=bash PKGBUILD` |
| **Clean** | `clean` | Clean source directory | `makepkg -c` |
| | `clean-all` | Remove all build outputs | removes `src/`, `pkg/`, any `*.pkg.tar.*`, bare git caches and `_build*` dirs |
| **AUR / Git** | `aur-push [msg]` | Stage, commit, push (auto message if omitted) | `git add && git commit && git push` |
| | `aur-push-tag <t>` | Stage, commit, tag, push | `git tag -a <t> && git push --tags` |
| **Other** | `setup-nautilus` | Clean up old scripts and verify Nautilus extension | `nautilus -q` + extension checks |
| | `--version` | Print program version | — |

### Notes on `aur-push`

- Commit messages are auto-generated from `.SRCINFO` in the form `upgpkg: <pkgname> <pkgver>-<pkgrel>`.
- For split packages, the first `pkgname` entry found in `.SRCINFO` is used in the commit message. This is usually the main package; if you maintain multiple split packages, consider overriding the message explicitly via `aur-push "your message"`.

---

## How i18n Works

The project uses a two-layer translation system:

1. **Rust CLI & settings panel** — standard GNU `gettext` compiled `.mo` files, built by Meson and stored in `/usr/share/locale/<lang>/LC_MESSAGES/pkgbuild_manager.mo`.
2. **Bash action scripts & notifications** — a lightweight `_i18n` helper reads plain `.po` text files at runtime from `/usr/share/pkgbuild-manager/i18n/<lang>.po`. No `gettext` shell tools needed at runtime.

Locale resolution order: `$LANGUAGE` → `$LC_ALL` → `$LC_MESSAGES` → `$LANG`. Falls back to English if no translation is found.

### Adding a New Language

1. Copy `po/pkgbuild_manager.pot` to `po/<lang>.po`.
2. Translate all `msgstr` entries.
3. Add the language code to `po/LINGUAS`.
4. Add `'../po/<lang>.po'` to `po_files` in `data/meson.build`.
5. Run `meson install` — no other changes needed.
