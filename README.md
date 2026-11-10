[English](README.md) | [简体中文](README_ZH.md)

<div align="center">

# 🔨GVM🔨

**A Gradle version manager with managed installs, fast switching, and clean cross-platform release binaries.**

[![Release](https://img.shields.io/github/v/release/crowforkotlin/gradle-version-manager?label=release)](https://github.com/crowforkotlin/gradle-version-manager/releases)
[![AUR](https://img.shields.io/aur/version/gvm-bin?label=AUR)](https://aur.archlinux.org/packages/gvm-bin)
[![License](https://img.shields.io/github/license/crowforkotlin/gradle-version-manager)](LICENSE)

</div>

## Install

### Quick install with curl

```bash
curl -fsSL https://raw.githubusercontent.com/crowforkotlin/gradle-version-manager/main/install.sh | bash
```

This runs directly from the pipe, does not save `install.sh` locally, and asks where to add the gvm `PATH`. The default follows the current shell: bash uses `~/.bashrc`, zsh uses `~/.zshrc`, and other shells default to no changes.

### Install from a release archive

```bash
curl -fsSL https://raw.githubusercontent.com/crowforkotlin/gradle-version-manager/main/install.sh | GVM_RELEASE_URL="<release-tarball-url>" bash
```

### Install on Arch Linux

```bash
paru -S gvm-bin
```

```bash
yay -S gvm-bin
```

### Build from source

```bash
cargo build --release
./install.sh --from ./target/release/gvm --activate
```

### Pin a specific release

```bash
curl -fsSL https://raw.githubusercontent.com/crowforkotlin/gradle-version-manager/main/install.sh | GVM_RELEASE_TAG=release-1.0.2 bash
```

Activation asks whether to update `~/.bashrc`, `~/.zshrc`, or no file. `install.sh --rc-file <path>` can select a file directly. The required `PATH` entry is:

```bash
export PATH="$HOME/.local/bin:$HOME/.gvm/bin:$PATH"
```

`install.sh` is a POSIX `sh` script and can run on Linux, macOS, and Git Bash.
Android builds are published as separate release assets for Termux or another Android shell environment.

## Quick start

Install a Gradle version:

```bash
gvm install 8.13
```

Install the latest stable release:

```bash
gvm install current
gvm install latest
gvm install latest-8
```

Switch and inspect:

```bash
gvm use 8.13
gvm current
gvm list
```

Add an existing Gradle home:

```bash
gvm add ~/.gradle/wrapper/dists/gradle-8.13-all/<hash>/gradle-8.13
gvm add --link /opt/gradle-8.13
```

## Command reference

| Command | Purpose |
| --- | --- |
| `gvm install <version>` | Download and install a managed Gradle version |
| `gvm list-remote [--major N] [--all]` | List downloadable versions from Gradle's version service |
| `gvm detect` | Find existing Gradle homes on the machine |
| `gvm add <path> [--link]` | Add an existing Gradle home by copying or symlinking it |
| `gvm list` | Show managed versions |
| `gvm use <version>` | Switch the active global version |
| `gvm current` | Print the selected version |
| `gvm remove <version> [--force]` | Remove a managed version |
| `gvm clean [--wrapper-cache\|--all]` | Clean `~/.gvm/tmp`, broken links, and optional Wrapper remnants |
| `gvm cache list [--home <dir>]` | Show removable Gradle caches with sizes and file counts |
| `gvm cache clean [--selector <area>]` | Delete selected Gradle caches; asks for confirmation first |
| `gvm stop [--kind gradle,kotlin] [--dry-run]` | Terminate running Gradle daemons and Kotlin compile daemons |
| `gvm projects [<path>...] [--depth N]` | List projects and the Gradle versions they use |
| `gvm update [--check]` | Replace the running binary with the latest release |

## Gradle cache management

`gvm cache list` is read-only. It measures the removable areas under the Gradle
user home (`~/.gradle` unless `GRADLE_USER_HOME` is set):

```bash
gvm cache list
```

```text
AREA                 SIZE    FILES  PATH
modules           4.9 GiB    18234  /home/you/.gradle/caches/modules-2
transforms        1.2 GiB     9042  /home/you/.gradle/caches/transforms-4
build-cache     912.4 MiB     1201  /home/you/.gradle/caches/build-cache-1
```

Deletion always prints the exact paths, file counts, and sizes, then asks
`Delete these caches and <size> of data? [y/N]`:

```bash
gvm cache clean --selector modules,build-cache
gvm cache clean --version 8.13
gvm cache clean --all --dry-run
```

`--selector` accepts `version`, `modules`, `jars`, `transforms`, `build-cache`,
`journal`, `kotlin-dsl`, `daemon-logs`, and `wrapper-dists`. A run that neither
selects an area nor passes `--version`/`--all` is rejected. When stdin is not a
terminal, deletion requires `--yes`; without it gvm exits with an error and
deletes nothing. `daemon-logs` removes only `*.log` files.

Deleted caches are not recoverable: Gradle re-downloads dependencies and rebuilds
transformed artifacts on the next build.

## Stopping Gradle processes

```bash
gvm stop --dry-run
gvm stop --kind gradle
```

`gvm stop` finds processes whose command line contains
`org.gradle.launcher.daemon.bootstrap.GradleDaemon` or
`org.jetbrains.kotlin.daemon.KotlinCompileDaemon`, sends `SIGTERM`, waits, then
sends `SIGKILL` to survivors. On Windows it uses `taskkill /T /F`.

## Project version report

```bash
gvm projects ~/code --depth 4
```

```text
8.13         8.13                    /home/you/code/service-a
-            9.3.0,9.5.0             /home/you/code/libs/build-logic
```

The first column is the `distributionUrl` version in
`gradle/wrapper/gradle-wrapper.properties`; the second column lists the
directories under the project's `.gradle/`, which record versions that have run
there. The scan skips `.git`, `build`, `target`, `node_modules`, and similar
directories.

## Self update

```bash
gvm update --check
gvm update
```

`gvm update` resolves the latest tag from
`https://github.com/crowforkotlin/gradle-version-manager/releases/latest`,
downloads `gvm-<version>-<target>.tar.gz` (or the `.exe` on Windows), verifies
its `.sha256`, and replaces the running binary after confirmation. `--check`
only reports the available version.

## Remote version aliases

| Alias | Meaning |
| --- | --- |
| `current` | The current stable Gradle release |
| `latest` | Same as `current` |
| `latest-8` | The latest stable release in the `8.x` line |

`gvm install lts` is intentionally unsupported because Gradle does not publish an official LTS line.

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

## How it works

- `gvm` keeps its own store under `~/.gvm`.
- `~/.gradle/wrapper/dists` is treated as a detection and add source, not as the managed source of truth.
- `gvm add` copies by default so cleaning Wrapper caches or SDKMAN does not break managed versions.
- `gvm remove` deletes copied installs and removes only the gvm-side symlink for `gvm add --link`.
- `gvm clean` removes transient files from `~/.gvm/tmp` and can also remove partial Wrapper cache files such as `.part` and `.lck`.

## Release notes

`CHANGELOG.md` is the source of release notes. `.github/workflows/release.yml`
renders the body for `release-<version>` with `scripts/release-notes.sh`, which
reads the `## <version>` section and falls back to `## Unreleased`. A release
fails when both sections are empty.

## Release assets

Current release archives are published for:

- `linux-x86_64`
- `linux-aarch64`
- `android-x86_64`
- `android-aarch64`
- `darwin-x86_64`
- `darwin-aarch64`
- `windows-x86_64`
- `windows-aarch64`

The AUR package is for Arch Linux and currently supports `x86_64` and `aarch64`. Android binaries are distributed through GitHub Releases instead of AUR.

Windows releases also include standalone executables:

- `gvm-<version>-windows-x86_64.exe`
- `gvm-<version>-windows-aarch64.exe`
