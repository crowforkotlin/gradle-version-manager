#!/usr/bin/env sh
set -eu

usage() {
  cat <<'EOF'
Install gvm from a local binary or a release asset.

Usage:
  ./install.sh --from ./target/release/gvm [--activate]
  ./install.sh --url <release-tarball-or-binary-url> [--activate]
  curl -fsSL https://raw.githubusercontent.com/crowforkotlin/gradle-version-manager/main/install.sh | sh -s -- [--activate]

Options:
  --from <path>         Install from an existing local gvm binary.
  --url <url>           Download and install from a release tarball or raw binary URL.
  --repo <owner/repo>   GitHub repository used for automatic release lookup.
  --tag <tag>           Release tag to install. Default: latest
  --install-dir <dir>   Directory for the gvm executable. Default: $HOME/.local/bin
  --activate            Append PATH setup to a shell startup file.
  --rc-file <path>      Override which shell startup file is modified by --activate.
  --help                Show this help.

Environment:
  GVM_INSTALL_DIR       Same as --install-dir.
  GVM_MANAGED_BIN_DIR   Managed Gradle launcher directory. Default: $HOME/.gvm/bin
  GVM_RELEASE_REPO      Same as --repo. Default: crowforkotlin/gradle-version-manager
  GVM_RELEASE_TAG       Same as --tag. Default: latest
EOF
}

launcher_name() {
  case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*) printf '%s\n' "gvm.exe" ;;
    *) printf '%s\n' "gvm" ;;
  esac
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

resolve_platform() {
  os=$(uname -s)
  arch=$(uname -m)

  case "$os" in
    Linux) platform_os=linux ;;
    Darwin) platform_os=darwin ;;
    MINGW*|MSYS*|CYGWIN*) platform_os=windows ;;
    *)
      echo "error: unsupported operating system: $os" >&2
      exit 1
      ;;
  esac

  case "$arch" in
    x86_64|amd64) platform_arch=x86_64 ;;
    arm64|aarch64) platform_arch=aarch64 ;;
    *)
      echo "error: unsupported architecture: $arch" >&2
      exit 1
      ;;
  esac

  printf '%s-%s\n' "$platform_os" "$platform_arch"
}

resolve_release_tag() {
  repo=$1
  requested_tag=$2
  temp_dir=$3

  if [ "$requested_tag" != "latest" ]; then
    printf '%s\n' "$requested_tag"
    return 0
  fi

  metadata="$temp_dir/release.json"
  download_to "https://api.github.com/repos/${repo}/releases/latest" "$metadata"
  tag=$(sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$metadata" | head -n 1)
  if [ -z "$tag" ]; then
    echo "error: could not resolve the latest release tag from GitHub API" >&2
    exit 1
  fi
  printf '%s\n' "$tag"
}

resolve_release_url() {
  repo=$1
  requested_tag=$2
  temp_dir=$3

  tag=$(resolve_release_tag "$repo" "$requested_tag" "$temp_dir")
  version=${tag#release-}
  platform=$(resolve_platform)

  printf 'https://github.com/%s/releases/download/%s/gvm-%s-%s.tar.gz\n' \
    "$repo" "$tag" "$version" "$platform"
}

extract_tarball_binary() {
  archive=$1
  temp_dir=$2

  tar -xzf "$archive" -C "$temp_dir"
  candidate=$(find "$temp_dir" -type f \( -name gvm -o -name gvm.exe \) | head -n 1 || true)
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
  install -m 755 "$source_path" "$install_dir/$(launcher_name)"
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
RELEASE_REPO=${GVM_RELEASE_REPO:-"crowforkotlin/gradle-version-manager"}
RELEASE_TAG=${GVM_RELEASE_TAG:-"latest"}
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
    --repo)
      RELEASE_REPO=$2
      shift 2
      ;;
    --tag)
      RELEASE_TAG=$2
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

temp_dir=$(mktemp -d)
cleanup() {
  rm -rf "$temp_dir"
}
trap cleanup EXIT INT TERM

if [ -z "$SOURCE_PATH" ] && [ -z "$SOURCE_URL" ]; then
  local_binary="./target/release/$(launcher_name)"
  if [ -x "$local_binary" ]; then
    SOURCE_PATH=$local_binary
  else
    SOURCE_URL=$(resolve_release_url "$RELEASE_REPO" "$RELEASE_TAG" "$temp_dir")
  fi
fi

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
