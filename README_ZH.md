[English](README.md) | **简体中文**

<div align="center">

# 🔨GVM🔨

**面向多平台的 Gradle 版本管理器，提供受管安装、快速切换和干净的发布二进制。**

[![Release](https://img.shields.io/github/v/release/crowforkotlin/gradle-version-manager?label=release)](https://github.com/crowforkotlin/gradle-version-manager/releases)
[![AUR](https://img.shields.io/aur/version/gvm-bin?label=AUR)](https://aur.archlinux.org/packages/gvm-bin)
[![License](https://img.shields.io/github/license/crowforkotlin/gradle-version-manager)](LICENSE)

</div>

## 安装

### 通过 curl 直接安装

```bash
curl -fsSL https://raw.githubusercontent.com/crowforkotlin/gradle-version-manager/main/install.sh | bash
```

该命令直接从管道执行，不会在本地保存 `install.sh`，并会询问把 gvm 的 `PATH` 追加到哪里。默认选项会根据当前 shell 判断：bash 使用 `~/.bashrc`，zsh 使用 `~/.zshrc`，其他 shell 默认不修改文件。

### 通过 release 安装

```bash
curl -fsSL https://raw.githubusercontent.com/crowforkotlin/gradle-version-manager/main/install.sh | GVM_RELEASE_URL="<release-tarball-url>" bash
```

### 在 Arch Linux 上安装

```bash
paru -S gvm-bin
```

```bash
yay -S gvm-bin
```

### 从源码构建安装

```bash
cargo build --release
./install.sh --from ./target/release/gvm --activate
```

### 固定某个 release

```bash
curl -fsSL https://raw.githubusercontent.com/crowforkotlin/gradle-version-manager/main/install.sh | GVM_RELEASE_TAG=release-1.0.2 bash
```

激活时会询问修改 `~/.bashrc`、`~/.zshrc`，还是不修改文件。也可以通过 `install.sh --rc-file <path>` 直接指定文件。需要追加的 `PATH` 内容是：

```bash
export PATH="$HOME/.local/bin:$HOME/.gvm/bin:$PATH"
```

`install.sh` 本身是 POSIX `sh` 脚本，可以在 Linux、macOS 和 Git Bash 中运行。
Android 版本会作为独立的 release 产物发布，可用于 Termux 或其他 Android shell 环境。

## 快速开始

安装一个 Gradle 版本：

```bash
gvm install 8.13
```

安装最新稳定版：

```bash
gvm install current
gvm install latest
gvm install latest-8
```

切换和查看当前版本：

```bash
gvm use 8.13
gvm current
gvm list
```

把已有 Gradle 加入 gvm：

```bash
gvm add ~/.gradle/wrapper/dists/gradle-8.13-all/<hash>/gradle-8.13
gvm add --link /opt/gradle-8.13
```

## 命令总览

| 命令 | 作用 |
| --- | --- |
| `gvm install <version>` | 下载并安装一个受管 Gradle 版本 |
| `gvm list-remote [--major N] [--all]` | 查看 Gradle 官方可下载版本 |
| `gvm detect` | 扫描系统里已有的 Gradle Home |
| `gvm add <path> [--link]` | 通过复制或软链接把已有 Gradle 加入管理 |
| `gvm list` | 列出受管版本 |
| `gvm use <version>` | 切换当前全局版本 |
| `gvm current` | 输出当前选中的版本 |
| `gvm remove <version> [--force]` | 移除一个受管版本 |
| `gvm clean [--wrapper-cache\|--all]` | 清理 `~/.gvm/tmp`、坏链接和可选的 Wrapper 残留 |
| `gvm cache list [--home <dir>]` | 列出可清理的 Gradle 缓存及其大小、文件数 |
| `gvm cache clean [--selector <area>]` | 删除选定的 Gradle 缓存，删除前要求确认 |
| `gvm stop [--kind gradle,kotlin] [--dry-run]` | 结束正在运行的 Gradle Daemon 和 Kotlin 编译 Daemon |
| `gvm projects [<path>...] [--depth N]` | 列出项目及其使用的 Gradle 版本 |
| `gvm update [--check]` | 用最新 release 替换当前运行的二进制 |

## Gradle 缓存管理

`gvm cache list` 只读，不会删除任何文件。它统计 Gradle 用户目录（默认
`~/.gradle`，设置了 `GRADLE_USER_HOME` 时使用该值）下可清理的区域：

```bash
gvm cache list
```

```text
AREA                 SIZE    FILES  PATH
modules           4.9 GiB    18234  /home/you/.gradle/caches/modules-2
transforms        1.2 GiB     9042  /home/you/.gradle/caches/transforms-4
build-cache     912.4 MiB     1201  /home/you/.gradle/caches/build-cache-1
```

删除前会先列出将要删除的完整路径、文件数和大小，然后询问
`Delete these caches and <size> of data? [y/N]`：

```bash
gvm cache clean --selector modules,build-cache
gvm cache clean --version 8.13
gvm cache clean --all --dry-run
```

`--selector` 接受 `version`、`modules`、`jars`、`transforms`、`build-cache`、
`journal`、`kotlin-dsl`、`daemon-logs`、`wrapper-dists`。既没有 `--selector`，
也没有 `--version`/`--all` 时命令直接报错，不删除任何内容。stdin 不是终端时
必须显式传 `--yes`，否则命令报错退出且不删除文件。`daemon-logs` 只删除
`*.log` 文件。

被删除的缓存无法恢复：下一次构建时 Gradle 会重新下载依赖并重建
transforms 产物。

## 结束 Gradle 进程

```bash
gvm stop --dry-run
gvm stop --kind gradle
```

`gvm stop` 查找命令行中包含
`org.gradle.launcher.daemon.bootstrap.GradleDaemon` 或
`org.jetbrains.kotlin.daemon.KotlinCompileDaemon` 的进程，先发送 `SIGTERM`，
等待后再对仍然存活的进程发送 `SIGKILL`。Windows 上使用 `taskkill /T /F`。

## 项目版本报告

```bash
gvm projects ~/code --depth 4
```

```text
8.13         8.13                    /home/you/code/service-a
-            9.3.0,9.5.0             /home/you/code/libs/build-logic
```

第一列是 `gradle/wrapper/gradle-wrapper.properties` 中 `distributionUrl` 的版本；
第二列是项目 `.gradle/` 下记录运行过的版本目录。扫描会跳过 `.git`、`build`、
`target`、`node_modules` 等目录。

## 自更新

```bash
gvm update --check
gvm update
```

`gvm update` 通过
`https://github.com/crowforkotlin/gradle-version-manager/releases/latest`
的跳转结果确定最新版本号，下载 `gvm-<version>-<target>.tar.gz`（Windows 上是
`.exe`），校验同名 `.sha256`，确认后替换当前运行的二进制。`--check` 只报告
可用版本，不做替换。

## 远程版本别名

| 别名 | 含义 |
| --- | --- |
| `current` | 当前稳定版 Gradle |
| `latest` | 与 `current` 相同 |
| `latest-8` | `8.x` 主线中的最新稳定版 |

`gvm install lts` 故意不支持，因为 Gradle 官方没有单独的 LTS 线。

## 目录结构

```text
~/.gvm/
  versions/
    8.13/
  current -> /home/you/.gvm/versions/8.13
  bin/
    gradle -> ../current/bin/gradle
  tmp/
```

## 工作方式

- `gvm` 使用自己的 `~/.gvm` 存储目录。
- `~/.gradle/wrapper/dists` 只作为 detection/add 来源，不作为主仓库。
- `gvm add` 默认复制，这样清理 Wrapper 缓存或 SDKMAN 后不会影响受管版本。
- `gvm remove` 会删除复制安装的版本；对 `gvm add --link` 只删除 gvm 自己的软链接。
- `gvm clean` 会清理 `~/.gvm/tmp`，也可以额外清理 Wrapper 里的 `.part`、`.lck` 等残留。

## Release 说明

Release 说明以 `CHANGELOG.md` 为准。`.github/workflows/release.yml` 通过
`scripts/release-notes.sh` 读取 `## <version>` 段落生成 `release-<version>`
的说明内容，该段落为空时回退到 `## Unreleased`；两者都为空时发布失败。

## Release 产物

当前 release 提供：

- `linux-x86_64`
- `linux-aarch64`
- `android-x86_64`
- `android-aarch64`
- `darwin-x86_64`
- `darwin-aarch64`
- `windows-x86_64`
- `windows-aarch64`

AUR 包面向 Arch Linux，目前支持 `x86_64` 和 `aarch64`。Android 二进制通过 GitHub Releases 发布，不放入 AUR 包。

Windows 还会额外提供独立可执行文件：

- `gvm-<version>-windows-x86_64.exe`
- `gvm-<version>-windows-aarch64.exe`
