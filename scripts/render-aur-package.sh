#!/usr/bin/env sh
set -eu

usage() {
  cat <<'EOF'
Render PKGBUILD and .SRCINFO for the gvm AUR package.

Usage:
  ./scripts/render-aur-package.sh <output-dir> <pkgname> <pkgver> <repo> <target> <sha256>
EOF
}

if [ "$#" -ne 6 ]; then
  usage >&2
  exit 1
fi

output_dir=$1
pkgname=$2
pkgver=$3
repo=$4
target=$5
sha256=$6

archive_name="gvm-${pkgver}-${target}.tar.gz"
archive_dir="gvm-${pkgver}-${target}"
package_url="https://github.com/${repo}"
release_url="${package_url}/releases/download/release-${pkgver}/${archive_name}"

mkdir -p "$output_dir"

cat >"${output_dir}/PKGBUILD" <<EOF
pkgname=${pkgname}
pkgver=${pkgver}
pkgrel=1
pkgdesc='Gradle version manager'
arch=('x86_64')
url='${package_url}'
license=('MIT')
provides=('gvm')
conflicts=('gvm')
source=("${archive_name}::${release_url}")
sha256sums=('${sha256}')

package() {
  install -Dm755 "\${srcdir}/${archive_dir}/gvm" "\${pkgdir}/usr/bin/gvm"
  install -Dm755 "\${srcdir}/${archive_dir}/install.sh" "\${pkgdir}/usr/share/doc/\${pkgname}/install.sh"
  install -Dm644 "\${srcdir}/${archive_dir}/README.md" "\${pkgdir}/usr/share/doc/\${pkgname}/README.md"
  install -Dm644 "\${srcdir}/${archive_dir}/README_ZH.md" "\${pkgdir}/usr/share/doc/\${pkgname}/README_ZH.md"
}
EOF

cat >"${output_dir}/.SRCINFO" <<EOF
pkgbase = ${pkgname}
	pkgdesc = Gradle version manager
	pkgver = ${pkgver}
	pkgrel = 1
	url = ${package_url}
	arch = x86_64
	license = MIT
	conflicts = gvm
	provides = gvm
	source = ${archive_name}::${release_url}
	sha256sums = ${sha256}

pkgname = ${pkgname}
EOF
