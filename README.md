[English](README.md) | [简体中文](README_ZH.md)

# gvm

Rust-based Gradle version manager with a stable managed launcher.

## Design

`gvm` keeps its own store under `~/.gvm` instead of treating `~/.gradle/wrapper/dists` as the source of truth.

- `~/.gvm/versions/<version>` stores extracted Gradle homes
- `~/.gvm/current` is the active version symlink
- `~/.gvm/bin/gradle` is the stable launcher you put on `PATH`

That separation is intentional: Wrapper caches can contain duplicate hashes, partial downloads, and entries the user may delete independently.

## Scope

This version implements the first complete CLI surface:

- `gvm install <version>`
- `gvm list-remote`
- `gvm detect`
- `gvm add <path>`
- `gvm list`
- `gvm remove <version>`
- `gvm clean`
- `gvm use <version>`
- `gvm current`

It manages a global active version. Detection scans `GRADLE_HOME`, `PATH`, `~/.gradle/wrapper/dists`, SDKMAN, and common system prefixes. Project-local `.gradle-version` and Wrapper-aware auto-switching are still future work.

## Installation

### Release binary

If a release binary is available for your platform, install it with `install.sh`.

Install from a release tarball or raw binary URL:

```bash
./install.sh --url <release-tarball-or-binary-url> --activate
```

You can also install from a local binary file:

```bash
./install.sh --from ./target/release/gvm --activate
```

### Shell configuration

To make both `gvm` and the managed Gradle launcher available in new terminal sessions, your `PATH` should include:

- the directory containing the `gvm` executable
- `~/.gvm/bin`

`install.sh --activate` appends the required `PATH` line to one shell startup file, such as `~/.zshrc`, `~/.bashrc`, or `~/.profile`:

```bash
export PATH="$HOME/.local/bin:$HOME/.gvm/bin:$PATH"
```

If you prefer to edit the shell startup file manually, add the same line yourself.

### Build from source

Build from source if you are developing locally or if no release binary is available for your platform.

```bash
cargo build --release
./install.sh --from ./target/release/gvm --activate
```

You can override the managed home for testing with `GVM_HOME=/some/path`.

### Arch Linux

If an AUR package is available, install it with:

```bash
paru -S gvm-bin
# or
yay -S gvm-bin
```

## Usage

Install a managed version from the official Gradle distribution service:

```bash
gvm install 8.13
```

Install the latest stable release without looking it up manually:

```bash
gvm install current
gvm install latest
gvm install latest-8
```

List downloadable versions:

```bash
gvm list-remote
gvm list-remote --major 8
gvm list-remote --all
```

`gvm install lts` is intentionally unsupported because Gradle does not publish an official LTS line.

Detect existing Gradle homes:

```bash
gvm detect
```

Add an existing Gradle home by copying it into `~/.gvm/versions`:

```bash
gvm add ~/.gradle/wrapper/dists/gradle-8.13-all/<hash>/gradle-8.13
```

Add by symlinking instead of copying:

```bash
gvm add --link /opt/gradle-8.13
```

Switch the global active version:

```bash
gvm use 8.13
```

Show the active version:

```bash
gvm current
```

List managed versions:

```bash
gvm list
```

Remove a managed version:

```bash
gvm remove 8.13
gvm remove 8.13 --force
```

Clean temporary files and optionally Wrapper cache remnants:

```bash
gvm clean
gvm clean --wrapper-cache
gvm clean --all
```

Example output:

```text
  8.9
* 8.13
  9.5.0
```

## Managed layout

```text
~/.gvm/
  versions/
    8.13/
  current -> /home/you/.gvm/versions/8.13
  bin/
    gradle -> ../current/bin/gradle
  tmp/
```

## Notes

- `install` downloads `gradle-<version>-bin.zip` and verifies the official `.sha256` checksum before extracting.
- `install` understands exact versions plus `current`, `latest`, and `latest-<major>`.
- `list-remote` reads the official Gradle version metadata service instead of making you search manually.
- `detect` treats `~/.gradle/wrapper/dists` as a discovery source only, not as the managed store.
- `add` defaults to copying so clearing Wrapper caches or SDKMAN installs does not break managed versions; use `--link` only when you want to keep the external installation as the source of truth.
- `remove` deletes managed directories for installed or copied versions, and only removes the managed symlink for `gvm add --link`.
- `tmp` is only a staging area for downloads, extraction, and copy-based adds. It is normally empty after successful commands because temporary directories are cleaned automatically.
- `clean` removes `~/.gvm/tmp` contents and broken managed symlinks; `--wrapper-cache` and `--all` also remove partial Wrapper cache files such as `.part` and `.lck`.
- The launcher path remains stable, so your shell only needs one `PATH` entry.
- `~/.gradle/wrapper/dists` should be treated as a future detection/add source, not as the managed store itself.
