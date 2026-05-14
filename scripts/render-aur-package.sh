#!/usr/bin/env sh
set -eu

usage() {
  cat <<'EOF'
Render PKGBUILD and .SRCINFO for the gvm AUR package.

Usage:
  ./scripts/render-aur-package.sh <output-dir> <pkgname> <pkgver> <pkgrel> <repo> <arch> <sha256> [<arch> <sha256>...]
EOF
}

if [ "$#" -lt 7 ] || [ $((($# - 5) % 2)) -ne 0 ]; then
  usage >&2
  exit 1
fi

output_dir=$1
pkgname=$2
pkgver=$3
pkgrel=$4
repo=$5
shift 5

package_url="https://github.com/${repo}"

mkdir -p "$output_dir"

arch_values=
pkgbuild_sources=
srcinfo_arches=
srcinfo_sources=

while [ "$#" -gt 0 ]; do
  arch=$1
  sha256=$2
  shift 2

  archive_name="gvm-${pkgver}-linux-${arch}.tar.gz"
  release_url="${package_url}/releases/download/release-${pkgver}/${archive_name}"

  arch_values="${arch_values} '${arch}'"
  pkgbuild_sources="${pkgbuild_sources}
source_${arch}=(\"${archive_name}::${release_url}\")
sha256sums_${arch}=('${sha256}')"
  srcinfo_arches="${srcinfo_arches}
	arch = ${arch}"
  srcinfo_sources="${srcinfo_sources}
	source_${arch} = ${archive_name}::${release_url}
	sha256sums_${arch} = ${sha256}"
done

cat >"${output_dir}/PKGBUILD" <<EOF
pkgname=${pkgname}
pkgver=${pkgver}
pkgrel=${pkgrel}
pkgdesc='Gradle version manager'
arch=(${arch_values# })
url='${package_url}'
license=('MIT')
provides=('gvm')
conflicts=('gvm')
${pkgbuild_sources#?}

package() {
  archive_dir="gvm-${pkgver}-linux-\${CARCH}"
  install -Dm755 "\${srcdir}/\${archive_dir}/gvm" "\${pkgdir}/usr/bin/gvm"
  install -Dm755 "\${srcdir}/\${archive_dir}/install.sh" "\${pkgdir}/usr/share/doc/\${pkgname}/install.sh"
  install -Dm644 "\${srcdir}/\${archive_dir}/README.md" "\${pkgdir}/usr/share/doc/\${pkgname}/README.md"
  install -Dm644 "\${srcdir}/\${archive_dir}/README_ZH.md" "\${pkgdir}/usr/share/doc/\${pkgname}/README_ZH.md"
}
EOF

cat >"${output_dir}/.SRCINFO" <<EOF
pkgbase = ${pkgname}
	pkgdesc = Gradle version manager
	pkgver = ${pkgver}
	pkgrel = ${pkgrel}
	url = ${package_url}
${srcinfo_arches#?}
	license = MIT
	conflicts = gvm
	provides = gvm
${srcinfo_sources#?}

pkgname = ${pkgname}
EOF
