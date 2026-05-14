#!/usr/bin/env sh
set -eu

usage() {
  cat <<'EOF'
Install gvm from a local binary or a release asset.

Usage:
  ./install.sh --from ./target/release/gvm [--activate]
  ./install.sh --url <release-tarball-or-binary-url> [--activate]

Options:
  --from <path>         Install from an existing local gvm binary.
  --url <url>           Download and install from a release tarball or raw binary URL.
  --install-dir <dir>   Directory for the gvm executable. Default: $HOME/.local/bin
  --activate            Append PATH setup to a shell startup file.
  --rc-file <path>      Override which shell startup file is modified by --activate.
  --help                Show this help.

Environment:
  GVM_INSTALL_DIR       Same as --install-dir.
  GVM_MANAGED_BIN_DIR   Managed Gradle launcher directory. Default: $HOME/.gvm/bin
EOF
}

detect_rc_file() {
  if [ -n "${GVM_RC_FILE:-}" ]; then
    printf '%s\n' "$GVM_RC_FILE"
    return 0
  fi

  shell_name=$(basename "${SHELL:-sh}")
  case "$shell_name" in
    zsh) printf '%s\n' "$HOME/.zshrc" ;;
    bash) printf '%s\n' "$HOME/.bashrc" ;;
    *) printf '%s\n' "$HOME/.profile" ;;
  esac
}

download_to() {
  url=$1
  destination=$2
  curl --fail --location --silent --show-error "$url" -o "$destination"
}

extract_tarball_binary() {
  archive=$1
  temp_dir=$2

  tar -xzf "$archive" -C "$temp_dir"
  candidate=$(find "$temp_dir" -type f -name gvm | head -n 1 || true)
  if [ -z "$candidate" ]; then
    echo "error: could not find gvm binary inside archive" >&2
    exit 1
  fi
  printf '%s\n' "$candidate"
}

install_binary() {
  source_path=$1
  install_dir=$2

  mkdir -p "$install_dir"
  install -m 755 "$source_path" "$install_dir/gvm"
}

activate_shell() {
  install_dir=$1
  managed_bin_dir=$2
  rc_file=$3

  marker_begin="# >>> gvm initialize >>>"
  marker_end="# <<< gvm initialize <<<"
  export_line="export PATH=\"$install_dir:$managed_bin_dir:\$PATH\""

  mkdir -p "$(dirname "$rc_file")"
  touch "$rc_file"

  if grep -Fq "$marker_begin" "$rc_file"; then
    echo "shell startup file already contains a gvm PATH block: $rc_file"
    return 0
  fi

  {
    printf '\n%s\n' "$marker_begin"
    printf '%s\n' "$export_line"
    printf '%s\n' "$marker_end"
  } >>"$rc_file"

  echo "updated shell startup file: $rc_file"
}

SOURCE_PATH=
SOURCE_URL=
INSTALL_DIR=${GVM_INSTALL_DIR:-"$HOME/.local/bin"}
MANAGED_BIN_DIR=${GVM_MANAGED_BIN_DIR:-"$HOME/.gvm/bin"}
ACTIVATE=0
RC_FILE=

while [ $# -gt 0 ]; do
  case "$1" in
    --from)
      SOURCE_PATH=$2
      shift 2
      ;;
    --url)
      SOURCE_URL=$2
      shift 2
      ;;
    --install-dir)
      INSTALL_DIR=$2
      shift 2
      ;;
    --activate)
      ACTIVATE=1
      shift 1
      ;;
    --rc-file)
      RC_FILE=$2
      shift 2
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown option $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if [ -n "$SOURCE_PATH" ] && [ -n "$SOURCE_URL" ]; then
  echo "error: use either --from or --url, not both" >&2
  exit 1
fi

if [ -z "$SOURCE_PATH" ] && [ -z "$SOURCE_URL" ]; then
  if [ -x "./target/release/gvm" ]; then
    SOURCE_PATH=./target/release/gvm
  else
    echo "error: provide --from <path> or --url <release-url>" >&2
    exit 1
  fi
fi

temp_dir=$(mktemp -d)
cleanup() {
  rm -rf "$temp_dir"
}
trap cleanup EXIT INT TERM

if [ -n "$SOURCE_URL" ]; then
  downloaded="$temp_dir/asset"
  download_to "$SOURCE_URL" "$downloaded"
  case "$SOURCE_URL" in
    *.tar.gz|*.tgz)
      SOURCE_PATH=$(extract_tarball_binary "$downloaded" "$temp_dir")
      ;;
    *)
      chmod +x "$downloaded"
      SOURCE_PATH=$downloaded
      ;;
  esac
fi

if [ ! -f "$SOURCE_PATH" ]; then
  echo "error: source binary does not exist: $SOURCE_PATH" >&2
  exit 1
fi

install_binary "$SOURCE_PATH" "$INSTALL_DIR"
echo "installed gvm to $INSTALL_DIR/gvm"

if [ "$ACTIVATE" -eq 1 ]; then
  if [ -z "$RC_FILE" ]; then
    RC_FILE=$(detect_rc_file)
  fi
  activate_shell "$INSTALL_DIR" "$MANAGED_BIN_DIR" "$RC_FILE"
  echo "restart the shell or run: source $RC_FILE"
else
  echo "add this line to a shell startup file such as ~/.zshrc, ~/.bashrc, or ~/.profile:"
  echo "  export PATH=\"$INSTALL_DIR:$MANAGED_BIN_DIR:\$PATH\""
fi
