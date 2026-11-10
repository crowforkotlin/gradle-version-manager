# Changelog

All notable changes to gvm are recorded here. Release bodies are generated from
this file, so a release needs a non-empty `## <version>` or `## Unreleased`
section before it can be published.

## 1.0.3

- Add `gvm cache list` and `gvm cache clean` to inspect and delete Gradle caches
  under the Gradle user home. Deletion is per area, per Gradle version, or
  `--all`; every run prints the exact paths, file counts, and sizes first, and
  asks for confirmation unless `--yes` is passed. `--dry-run` reports without
  deleting.
- Add `gvm stop` to terminate running Gradle daemons and Kotlin compile daemons,
  optionally restricted with `--kind gradle,kotlin`; `--dry-run` lists matches.
- Add `gvm update` to resolve the latest GitHub release, verify its SHA-256, and
  replace the running binary. `--check` reports the available version only.
- Add `gvm projects <path>...` to list projects below the given directories with
  the version from `gradle/wrapper/gradle-wrapper.properties` and the versions
  recorded under each project's `.gradle/<version>` directory.
- Release notes now come from this changelog instead of the commit log.

## 1.0.2

- Publish Android `x86_64` and `aarch64` binaries and harden Gradle detection.

## 1.0.1

- Add Android release binaries.
- Harden Gradle home detection from `PATH`, Wrapper caches, SDKMAN, and common
  prefixes.

## 1.0.0

- Manage Gradle installs under `~/.gvm` with a stable `~/.gvm/bin/gradle`
  launcher, version switching through `~/.gvm/current`, and `install`,
  `list-remote`, `detect`, `add`, `list`, `use`, `current`, `remove`, and `clean`
  commands.
- Publish release archives for Linux, macOS, Windows, and Android, a Windows
  standalone executable, and an AUR package.
