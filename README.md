# PKGBUILD Manager

A headless Rust CLI tool and Nautilus context menu integration for Arch Linux package maintainers. It automates common tasks around writing, testing, updating, and publishing `PKGBUILD` files to the AUR (Arch User Repository).

The project has **no GUI**. Interactive actions (like package compilation or linting) open a terminal window, while silent background tasks (like updating checksums or cleaning directories) run silently and report their result via system notifications (`notify-send`).

---

## Features

- **Compilation & Installation**: Multiple wrappers around `makepkg` (clean builds, forced builds, skipping tests, skipping GPG checks, or custom flags).
- **Metadata Management**: Quick commands to update checksums via `updpkgsums` and automatically regenerate `.SRCINFO`.
- **Quality Assurance**: Automated linting/auditing using `namcap` (on the PKGBUILD and compiled `.pkg.tar.*` packages) and `shellcheck` (optimized for PKGBUILD bash code).
- **Git & AUR Integration**: Auto-generates standard commit messages like `upgpkg: <name> <version>-<release>`, regenerates `.SRCINFO` automatically before staging, and supports pushing version tags.
- **Nautilus Context Menu Integration**: A suite of scripts to execute any of the features directly from the Nautilus file manager.

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

This will create a `PKGBUILD` submenu inside Nautilus and automatically translate all commands to your system's language using `gettext`.

If you compiled manually (without `makepkg`) and want to install them from the source folder, you can also run:

```bash
PKGBUILD_MANAGER_LOCALEDIR=build/po pkgbuild_manager setup-nautilus
```

After installing or updating the scripts, restart Nautilus to apply changes:
```bash
nautilus -q
```

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
