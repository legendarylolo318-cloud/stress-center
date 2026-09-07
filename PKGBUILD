# Maintainer: Stress Center Contributors
# Fork of Mission Center (https://gitlab.com/mission-center-devs/mission-center)
# that adds a built-in stress-ng powered Stress page.
pkgname=stress-center
pkgver=1.2.0
pkgrel=1
pkgdesc="System monitor with built-in stress-ng driven stress testing (a Mission Center fork)"
arch=('x86_64' 'aarch64')
url="https://github.com/legendarylolo318-cloud/stress-center"
license=('GPL-3.0-or-later')
depends=(
    'gtk4'
    'libadwaita'
    'systemd-libs'
    'stress-ng'
)
makedepends=(
    'rust'
    'cargo'
    'meson'
    'ninja'
    'blueprint-compiler'
    'git'
    'desktop-file-utils'
    'appstream'
)
source=("git+${url}.git#tag=v${pkgver}")
sha256sums=('SKIP')

prepare() {
    cd "$pkgname"
    git submodule update --init --recursive
}

build() {
    arch-meson "$pkgname" build \
        -Dbuildtype=release
    meson compile -C build
}

package() {
    meson install -C build --destdir="$pkgdir"
}
