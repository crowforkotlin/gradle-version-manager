[English](README.md) | **简体中文**

# gvm

Rust 编写的 Gradle 版本管理器，提供稳定的 managed launcher。

## 设计

`gvm` 使用自己的目录 `~/.gvm`，而不是直接把 `~/.gradle/wrapper/dists` 当作主仓库。

- `~/.gvm/versions/<version>` 存放受 gvm 管理的 Gradle Home
- `~/.gvm/current` 是当前激活版本的软链接
- `~/.gvm/bin/gradle` 是放到 `PATH` 里的稳定入口

这样设计的原因很直接：Wrapper 缓存里可能有重复 hash、未完成下载和会被用户单独清理的目录。

## 安装

### 发行二进制

如果你的平台已经提供了 release 二进制，可以直接用 `install.sh` 安装。

通过 release tarball 或二进制 URL 安装：

```bash
./install.sh --url <release-tarball-or-binary-url> --activate
```

如果你已经有本地二进制文件，也可以直接安装：

```bash
./install.sh --from ./target/release/gvm --activate
```

### Shell 启动文件配置

为了让 `gvm` 和受管的 `gradle` 命令在新的终端会话里都可用，`PATH` 里应该包含：

- `gvm` 可执行文件所在目录
- `~/.gvm/bin`

`install.sh --activate` 会把下面这条 PATH 配置追加到一个 shell 启动文件中，例如 `~/.zshrc`、`~/.bashrc` 或 `~/.profile`：

```bash
export PATH="$HOME/.local/bin:$HOME/.gvm/bin:$PATH"
```

如果你不想自动修改启动文件，也可以手动添加同样的配置。

### 从源码构建

如果你正在本地开发，或者当前平台还没有可用的 release 二进制，可以从源码构建：

```bash
cargo build --release
./install.sh --from ./target/release/gvm --activate
```

### Arch Linux

如果 AUR 包已经可用，可以这样安装：

```bash
paru -S gvm-bin
# 或
yay -S gvm-bin
```

## 命令

- `gvm install <version>`
- `gvm list-remote`
- `gvm detect`
- `gvm add <path>`
- `gvm list`
- `gvm remove <version>`
- `gvm clean`
- `gvm use <version>`
- `gvm current`

## 用法

安装官方版本：

```bash
gvm install 8.13
gvm install current
gvm install latest
gvm install latest-8
```

列出可下载版本：

```bash
gvm list-remote
gvm list-remote --major 8
gvm list-remote --all
```

`gvm install lts` 不支持，因为 Gradle 官方没有单独的 LTS 线。

检测已有 Gradle：

```bash
gvm detect
```

把已有 Gradle 加入 gvm：

```bash
gvm add ~/.gradle/wrapper/dists/gradle-8.13-all/<hash>/gradle-8.13
gvm add --link /opt/gradle-8.13
```

切换当前版本：

```bash
gvm use 8.13
gvm current
gvm list
```

移除受管版本：

```bash
gvm remove 8.13
gvm remove 8.13 --force
```

清理临时文件和残留：

```bash
gvm clean
gvm clean --wrapper-cache
gvm clean --all
```

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

## 说明

- `install` 会下载 `gradle-<version>-bin.zip`，校验官方 `.sha256` 后再解压。
- `list-remote` 直接读取 Gradle 官方版本元数据，不需要你自己查。
- `add` 默认是 **copy**，这样即使 Wrapper 缓存或 SDKMAN 目录被清掉，gvm 里的版本也不会失效。
- `gvm add --link` 才是只链接外部目录。
- `remove` 对 install/copy 版本会删除 managed 目录；对 `add --link` 版本只删 gvm 自己的软链接。
- `tmp` 是下载、解压、复制时的临时工作区。成功后通常会自动清空，所以空目录是正常的。
- `clean` 会清理 `~/.gvm/tmp`、坏链接，以及可选的 Wrapper `.part` / `.lck` 残留。
- `~/.gradle/wrapper/dists` 更适合作为 detection/add 来源，而不是 gvm 的主仓库。
