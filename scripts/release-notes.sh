#!/usr/bin/env sh
# Render GitHub release notes for one gvm version from CHANGELOG.md.
set -eu

usage() {
  cat <<'EOF'
Render a gvm release body from CHANGELOG.md.

Usage:
  ./scripts/release-notes.sh <version> [--no-downloads]

The notes come from the first non-empty section out of:
  1. `## <version>` (the leading `v` or `release-` prefix is optional)
  2. `## Unreleased`

The command fails when neither section exists or the chosen section is empty.
EOF
}

if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
  usage >&2
  exit 1
fi

version=$1
show_downloads=1
if [ "${2:-}" = "--no-downloads" ]; then
  show_downloads=0
elif [ -n "${2:-}" ]; then
  usage >&2
  exit 1
fi

repo_root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
changelog="${repo_root}/CHANGELOG.md"

if [ ! -f "${changelog}" ]; then
  echo "error: CHANGELOG.md not found at ${changelog}" >&2
  exit 1
fi

# Normalize release-1.2.3 and v1.2.3 to 1.2.3.
version=$(printf '%s\n' "${version}" | sed -e 's/^release-//' -e 's/^v//')
if [ -z "${version}" ]; then
  echo "error: version is empty after normalization" >&2
  exit 1
fi

section() {
  awk -v heading="## $1" '
    $0 == heading { in_section = 1; next }
    /^## / && in_section { exit }
    in_section && !content && /^[[:space:]]*$/ { next }
    in_section { content = 1; print }
  ' "${changelog}"
}

notes=$(section "${version}")
if [ -z "$(printf '%s' "${notes}" | tr -d '[:space:]')" ]; then
  notes=$(section "Unreleased")
fi
if [ -z "$(printf '%s' "${notes}" | tr -d '[:space:]')" ]; then
  echo "error: CHANGELOG.md has no non-empty '## ${version}' or '## Unreleased' section" >&2
  exit 1
fi

printf '%s\n' "${notes}"

if [ "${show_downloads}" -eq 1 ]; then
  cat <<EOF

## Downloads

- \`linux-x86_64\`
- \`linux-aarch64\`
- \`android-x86_64\`
- \`android-aarch64\`
- \`darwin-x86_64\`
- \`darwin-aarch64\`
- \`windows-x86_64\`
- \`windows-aarch64\`
- \`windows-x86_64.exe\`
- \`windows-aarch64.exe\`

## Installation

\`\`\`bash
curl -fsSL https://raw.githubusercontent.com/crowforkotlin/gradle-version-manager/main/install.sh | bash
\`\`\`

The installer asks whether to update \`~/.bashrc\`, \`~/.zshrc\`, or no shell
startup file. Release archives unpack into \`gvm-<version>-<target>/\` and contain
\`gvm\`, \`install.sh\`, and both README files; install with
\`./install.sh --from ./gvm\` from inside the unpacked directory.
EOF
fi
