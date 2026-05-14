#!/usr/bin/env sh
set -eu

usage() {
  cat <<'EOF'
Package a built gvm binary into a release archive.

Usage:
  ./scripts/build-release-archive.sh <version> <target> <binary> <output-dir>
EOF
}

if [ "$#" -ne 4 ]; then
  usage >&2
  exit 1
fi

version=$1
target=$2
binary=$3
output_dir=$4

if [ ! -f "$binary" ]; then
  echo "error: binary not found: $binary" >&2
  exit 1
fi

package_root="gvm-${version}-${target}"
binary_name=$(basename "$binary")
staging_dir=$(mktemp -d)
cleanup() {
  rm -rf "$staging_dir"
}
trap cleanup EXIT INT TERM

mkdir -p "$staging_dir/$package_root" "$output_dir"
install -m 755 "$binary" "$staging_dir/$package_root/$binary_name"
install -m 755 ./install.sh "$staging_dir/$package_root/install.sh"
install -m 644 ./README.md "$staging_dir/$package_root/README.md"
install -m 644 ./README_ZH.md "$staging_dir/$package_root/README_ZH.md"

archive_path="$output_dir/${package_root}.tar.gz"
tar -C "$staging_dir" -czf "$archive_path" "$package_root"
if command -v sha256sum >/dev/null 2>&1; then
  sha256sum "$archive_path" > "${archive_path}.sha256"
else
  shasum -a 256 "$archive_path" > "${archive_path}.sha256"
fi
