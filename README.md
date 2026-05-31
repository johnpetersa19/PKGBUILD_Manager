# PKGBUILD Manager

A headless Rust CLI tool and Nautilus context menu integration for Arch Linux package maintainers. It automates common tasks around writing, testing, updating, and publishing `PKGBUILD` files to the AUR (Arch User Repository).

The project has **no GUI**. Interactive actions (like package compilation or linting) open a terminal window, while silent background tasks (like updating checksums or cleaning directories) run silently and report their result via system notifications (`notify-send`).

---

## Features

- **Compilation & Installation**: Multiple wrappers around `makepkg` (clean builds, forced builds, skipping tests, skipping GPG checks, or custom flags).
- **Metadata Management**: Quick commands to update checksums via `updpkgsums` and automatically regenerate `.SRCINFO`.
- **Quality Assurance**: Automated linting/auditing using `namcap` (on the PKGBUILD and compiled `.pkg.tar.*` packages) and `shellcheck` (optimized for PKGBUILD bash code).
- **Git & AUR Integration**: Auto-generates standard commit messages like `upgpkg: <name> <version>-<release>`, regenerates `.SRCINFO` automatically before staging, and supports pushing version tags.
- **Nautilus Context Menu Integration**: A single `PKGBUILD` submenu appears when right-clicking any PKGBUILD file, with all actions translated to the user's system language automatically.
- **Full Internationalization (i18n)**: All menu labels, desktop notifications, and CLI strings are translated. Languages currently supported: **English** (default), **Português (pt_BR)**, **Español (es)**, **Deutsch (de)**, **Français (fr)**, **Italiano (it)**. The locale is detected automatically from the environment (`LANG`, `LC_MESSAGES`, etc.) — no manual configuration required.

---

## Nautilus Menu Structure

When right-clicking a `PKGBUILD` file in Nautilus, a single **PKGBUILD** submenu is shown. The labels below are displayed in the user's language automatically:

| Internal script name | 🇧🇷 Português | 🇺🇸 English | 🇪🇸 Español |
|---|---|---|---|
| `00_Full Workflow` | Fluxo Completo | Full Workflow | Flujo Completo |
| `01_Build` | Compilar | Build | Compilar |
| `02b_Build and Clean` | Compilar e Limpar | Build and Clean | Compilar y Limpiar |
| `02_Install` | Instalar | Install | Instalar |
| `03_Update Checksums` | Atualizar Checksums | Update Checksums | Actualizar Checksums |
| `04_Update .SRCINFO` | Atualizar .SRCINFO | Update .SRCINFO | Actualizar .SRCINFO |
| `05b_ShellCheck` | Verificar com ShellCheck | ShellCheck | Verificar con ShellCheck |
| `05_Namcap` | Analisar com Namcap | Namcap | Analizar con Namcap |
| `06_Push AUR` | Enviar para AUR | Push AUR | Publicar en AUR |
| `07b_Clean Everything` | Limpar Tudo | Clean Everything | Limpiar Todo |
| `07_Clean srcdir` | Limpar srcdir | Clean srcdir | Limpiar srcdir |

> The numeric prefixes (`00_`, `01_`, …) control ordering in the menu but are never shown to the user.

---

## Dependencies

### Build Dependencies
- **Rust / Cargo** (for compiling the backend dispatcher)
- **Meson** & **Ninja** (build system)
- **gettext** (for translations/internationalization support)

### Runtime Dependencies
- **pacman-contrib** (required for `updpkgsums` / `checksums` command)
- **namcap** (required for `namcap` audit command)
- **shellcheck** (required for `shellcheck` lint command)
- **libnotify** (provides `notify-send` for desktop notifications on background tasks)
- **nautilus** (to use the context menu scripts)

---

## Installation

### 1. Build and Install the CLI
You can compile the project using the Meson build system:

```bash
# Configure the build directory
meson setup build

# Compile the project
meson compile -C build

# Install the binary and assets
sudo meson install -C build
```

This installs:
- The `pkgbuild_manager` binary to `/usr/bin/`
- Nautilus action scripts to `/usr/share/pkgbuild-manager/scripts/`
- Plain `.po` translation files to `/usr/share/pkgbuild-manager/i18n/` (read at runtime by the bash `_i18n` helper)
- The Nautilus Python extension to `/usr/share/nautilus-python/extensions/`

Alternatively, build with Cargo directly and copy the binary to your path:

```bash
cargo build --release
sudo cp target/release/pkgbuild_manager /usr/local/bin/
```

### 2. Install Nautilus Scripts

If you installed the project via **`makepkg` (the Arch package)**, you can automatically enable and translate the Nautilus scripts for your user by running:

```bash
pkgbuild_manager setup-nautilus
```

This creates the `PKGBUILD` submenu inside Nautilus with all actions translated to your system language.

If you compiled manually (without `makepkg`) and want to install from the source folder, run:

```bash
PKGBUILD_MANAGER_LOCALEDIR=build/po pkgbuild_manager setup-nautilus
```

After installing or updating the scripts, restart Nautilus to apply changes:
```bash
nautilus -q
```

---

## How i18n Works

The project uses a two-layer translation system:

1. **Rust CLI & menu labels** — use `gettext` compiled `.mo` files (standard GNU gettext). These are compiled by Meson during `meson install` and stored in `/usr/share/locale/<lang>/LC_MESSAGES/pkgbuild_manager.mo`.

2. **Bash scripts & desktop notifications** — use a lightweight bash helper (`_i18n`) that reads plain `.po` text files at runtime from `/usr/share/pkgbuild-manager/i18n/<lang>.po`. This avoids any dependency on `gettext` shell tools at runtime.

The locale is resolved in priority order: `$LANGUAGE` → `$LC_ALL` → `$LC_MESSAGES` → `$LANG`. If no translation is found for the current locale, English strings are used as fallback.

### Adding a New Language

1. Copy an existing file, e.g. `po/pt_BR.po`, to `po/<lang>.po`.
2. Translate all `msgstr` entries.
3. Add the language code to `po/LINGUAS`.
4. Add `'../po/<lang>.po'` to the `po_files` list in `data/meson.build`.
5. Run `meson install` — no other changes are needed.

---

## CLI Usage

Run `pkgbuild_manager <command> [path] [flags...]`

If no `path` is specified, it defaults to the current working directory. If the second argument starts with `-`, it is treated as a flag for the current directory.

### Available Subcommands

| Category | Subcommand | Description | Equivalent Underlying Command |
|---|---|---|---|
| **Build** | `build` | Compile the package | `makepkg` |
| | `build-clean` | Compile and clean build directory after | `makepkg -c` |
| | `build-force` | Force recompilation | `makepkg -f` |
| | `build-nocheck` | Skip check() functions | `makepkg --nocheck` |
| | `build-nogpg` | Skip PGP signature checks | `makepkg --skippgpcheck` |
| | `build-custom` | Compile passing custom flags | `makepkg [custom flags...]` |
| | `fetch-sources` | Download and extract source files | `makepkg -o` |
| **Install** | `install` | Build, install and resolve dependencies | `makepkg -si` |
| | `install-clean` | Build, install, and clean build directory | `makepkg -sic` |
| | `install-force` | Force build and installation | `makepkg -sif` |
| | `install-rmdeps` | Install and remove build-only dependencies | `makepkg -sir` |
| | `install-nocheck`| Install, skipping package tests | `makepkg -si --nocheck` |
| | `install-nogpg` | Install, skipping signature validation | `makepkg -si --skippgpcheck`|
| | `install-custom` | Install, passing custom flags | `makepkg -si [custom flags...]` |
| **Metadata** | `checksums` | Update checksums inside PKGBUILD | `updpkgsums` |
| | `genchecksums` | Generate checksums and print to stdout | `makepkg -g` |
| | `srcinfo` | Generate/update `.SRCINFO` | `makepkg --printsrcinfo > .SRCINFO` |
| **Audit** | `namcap` | Audit PKGBUILD and compiled packages | `namcap PKGBUILD *.pkg.tar.*` |
| | `shellcheck` | Lint PKGBUILD shell script code | `shellcheck --shell=bash PKGBUILD` |
| **Clean** | `clean` | Clean source directory | `makepkg -c` |
| | `clean-all` | Remove all build outputs, source and package dirs | `rm -rf src/ pkg/ *.pkg.tar.*` |
| **AUR / Git**| `aur-push [msg]` | Stage, commit, and push to AUR | `git add && git commit && git push` |
| | `aur-push-tag <t>`| Stage, commit, add a version tag, and push | `git tag -a <t> && git push --tags` |
