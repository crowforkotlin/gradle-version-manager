[English](README.md) | **简体中文**

<div align="center">

# 🔨GVM🔨

**面向 Linux 的 Gradle 版本管理器，提供受管安装、快速切换和干净的发布二进制。**

[![Release](https://img.shields.io/github/v/release/crowforkotlin/gradle-version-manager?label=release)](https://github.com/crowforkotlin/gradle-version-manager/releases)
[![AUR](https://img.shields.io/aur/version/gvm-bin?label=AUR)](https://aur.archlinux.org/packages/gvm-bin)
[![License](https://img.shields.io/github/license/crowforkotlin/gradle-version-manager)](LICENSE)

</div>

## 安装

| 方式 | 命令 |
| --- | --- |
| Release 压缩包 | `./install.sh --url <release-tarball-url> --activate` |
| Arch Linux | `paru -S gvm-bin` 或 `yay -S gvm-bin` |
| 源码构建 | `cargo build --release && ./install.sh --from ./target/release/gvm --activate` |

`install.sh --activate` 会把需要的 `PATH` 追加到一个 shell 启动文件中：

```bash
export PATH="$HOME/.local/bin:$HOME/.gvm/bin:$PATH"
```

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

## Release 产物

当前 release 提供：

- `linux-x86_64`
- `linux-aarch64`
