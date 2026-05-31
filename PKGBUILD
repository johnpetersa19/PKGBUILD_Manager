# Maintainer: John <john@example.com>
pkgname=pkgbuild-manager-git
_pkgname=PKGBUILD_Manager
pkgver=v0.1.0.r20.458d243
pkgrel=1
pkgdesc="A Rust-based headless CLI tool and Nautilus context menu integration for PKGBUILD management"
arch=('x86_64')
url="https://github.com/johnpetersa19/PKGBUILD_Manager"
license=('GPL-3.0-or-later')
depends=(
  'pacman-contrib'
  'libnotify'
  'nautilus'
  'python-nautilus'
)
makedepends=('git' 'meson' 'ninja' 'rust' 'cargo')
optdepends=(
  'namcap: for auditing package metadata and structure'
  'shellcheck: for linting PKGBUILD bash code'
)
provides=("pkgbuild-manager")
conflicts=("pkgbuild-manager")
install=pkgbuild-manager.install
source=("$_pkgname::git+file://$PWD")
sha256sums=('SKIP')

pkgver() {
  cd "$srcdir/$_pkgname"
  git describe --long --tags | sed 's/\([^-]*-\)g/r\1/;s/-/./g'
}

build() {
  arch-meson "$srcdir/$_pkgname" build
  meson compile -C build
}

package() {
  meson install -C build --destdir="$pkgdir"
}
