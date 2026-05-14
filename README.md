[English](README.md) | [简体中文](README_ZH.md)

<div align="center">

# 🔨GVM🔨

**A Gradle version manager for Linux with managed installs, fast switching, and clean release binaries.**

[![Release](https://img.shields.io/github/v/release/crowforkotlin/gradle-version-manager?label=release)](https://github.com/crowforkotlin/gradle-version-manager/releases)
[![AUR](https://img.shields.io/aur/version/gvm-bin?label=AUR)](https://aur.archlinux.org/packages/gvm-bin)
[![License](https://img.shields.io/github/license/crowforkotlin/gradle-version-manager)](LICENSE)

</div>

## Why 🔨GVM🔨

| Feature | Details |
| --- | --- |
| Official downloads | Installs Gradle from the official distribution service with SHA-256 verification |
| Smooth switching | Uses a stable `~/.gvm/bin/gradle` launcher and a managed `current` symlink |
| Smart reuse | Detects existing Gradle homes from Wrapper caches, `PATH`, SDKMAN, and common system paths |
| Safer installs | `gvm install` shows a progress bar, keeps partial downloads, and resumes after interruption |

## Install

| Method | Command |
| --- | --- |
| Release archive | `./install.sh --url <release-tarball-url> --activate` |
| Arch Linux | `paru -S gvm-bin` or `yay -S gvm-bin` |
| From source | `cargo build --release && ./install.sh --from ./target/release/gvm --activate` |

`install.sh --activate` adds the required `PATH` entry to one shell startup file:

```bash
export PATH="$HOME/.local/bin:$HOME/.gvm/bin:$PATH"
```

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

## Release assets

Current release archives are published for:

- `linux-x86_64`
- `linux-aarch64`

