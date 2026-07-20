# Fontick macOS 构建与测试指南

适用于 Apple Silicon（arm64）测试机，从 Git 仓库干净构建、测试和打包 Fontick。

本文只记录长期通用流程。具体分支、目标提交、专项测试文本和当次回传要求，应由每轮测试提示词单独指定。

## 1. 准备条件

- Apple Silicon Mac；当前流程不覆盖 Intel Mac。
- 已安装 Adobe Photoshop，并至少准备一个包含文字图层的 PSD。
- 可以访问 GitHub、npm、crates.io 和项目固定的 Tao 分叉。
- 测试机不需要登录 GitHub；公开仓库可以匿名拉取。

先确认当前终端没有运行在 Rosetta 下：

```bash
uname -m
arch
sysctl -in sysctl.proc_translated 2>/dev/null || true
```

预期：

```text
arm64
arm64
0
```

## 2. 安装基础环境

### 2.1 Xcode Command Line Tools

```bash
xcode-select --install
```

如果已经安装，确认路径和编译器：

```bash
xcode-select -p
clang --version
git --version
```

正常路径通常为：

```text
/Library/Developer/CommandLineTools
```

### 2.2 Homebrew 和 Node.js

优先使用 Apple Silicon Homebrew：

```bash
command -v brew
brew --version
```

正常路径通常为：

```text
/opt/homebrew/bin/brew
```

如果尚未安装 Homebrew：

```bash
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
echo 'eval "$(/opt/homebrew/bin/brew shellenv)"' >> ~/.zprofile
eval "$(/opt/homebrew/bin/brew shellenv)"
```

安装 Node.js：

```bash
brew install node
```

验证 Node.js 和 npm：

```bash
node -v
npm -v
file "$(command -v node)"
```

Node.js 必须包含 arm64 架构。不要在 Rosetta 终端中安装仅有 x86_64 的 Node.js。

### 2.3 Rust

使用 rustup 安装 Rust：

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
rustup default stable
rustup target add aarch64-apple-darwin
```

验证：

```bash
rustc -Vv
cargo -V
rustup show active-toolchain
```

`rustc -Vv` 中的 host 应为：

```text
aarch64-apple-darwin
```

## 3. 获取干净源码

### 3.1 全新匿名克隆

将变量设置为当轮测试指定的分支：

```bash
BRANCH="replace-with-test-branch"

cd ~/Downloads
git clone --branch "$BRANCH" --single-branch \
  https://github.com/bakedbody/Fontick.git \
  Fontick-macos-test

cd Fontick-macos-test
git rev-parse HEAD
git status --short --branch
```

将输出的完整 HEAD 与当轮指定提交逐字核对。工作区必须干净。

### 3.2 从现有仓库建立隔离 worktree

不要在带有旧补丁、测试字体或构建产物的工作区继续开发：

```bash
BRANCH="replace-with-test-branch"

cd /path/to/existing/Fontick
git fetch origin "$BRANCH"

git worktree add \
  ../Fontick-macos-test \
  "origin/$BRANCH"

cd ../Fontick-macos-test
git rev-parse HEAD
git status --short --branch
```

测试机只需匿名 fetch，不需要 `gh auth login`、PAT 或个人 SSH 密钥。

## 4. 安装项目依赖

```bash
cd app
npm ci
```

Rust 依赖会在第一次 `cargo` 或 Tauri 构建时自动下载。

Fontick 在 macOS 使用固定的 Tao 分叉，以创建不会抢走 Photoshop 焦点的 NSPanel：

```text
https://github.com/bakedbody/tao.git
ff0c2dd24e093e60a3cfd2e7f416219c41bfc508
```

确认依赖中启用了对应 feature：

```bash
cargo tree \
  --manifest-path src-tauri/Cargo.toml \
  -e features \
  | grep -E 'tao|fontick-nonactivating-panel'
```

如果无法下载 Tao，先检查 GitHub 网络访问，不要临时改回 crates.io 版本或本地绝对路径。

## 5. 自动测试

```bash
npm test
cargo test --manifest-path src-tauri/Cargo.toml
```

记录通过数量和失败日志。任何测试失败都应先停止，不要继续生成正式包。

## 6. Debug 构建

```bash
npm run build -- --debug
```

Debug App 通常位于：

```text
app/src-tauri/target/debug/bundle/macos/Fontick.app
```

确认架构和版本：

```bash
APP="src-tauri/target/debug/bundle/macos/Fontick.app"

file "$APP/Contents/MacOS/fontick"
lipo -archs "$APP/Contents/MacOS/fontick"
plutil -p "$APP/Contents/Info.plist" \
  | grep -E 'CFBundleIdentifier|CFBundleShortVersionString|CFBundleVersion'
```

预期架构：

```text
arm64
```

启动指定 App，避免误用旧构建：

```bash
open "$APP"
```

## 7. macOS 权限

Fontick 需要两类权限：

1. **辅助功能**：向 Photoshop 发送按键，用于判断和退出文字编辑状态、读取文字选区。
2. **自动化 → Adobe Photoshop**：通过 Apple Events 调用 Photoshop 执行 JSX。

设置位置：

```text
系统设置 → 隐私与安全性 → 辅助功能
系统设置 → 隐私与安全性 → 自动化
```

操作步骤：

1. 先启动 Photoshop 并打开 PSD。
2. 启动本轮刚构建的 `Fontick.app`。
3. 将该 App 加入“辅助功能”并允许。
4. 第一次连接 Photoshop 时允许“Fontick 控制 Adobe Photoshop”。
5. 修改权限后完全退出并重新打开 Fontick。

Debug 或 ad-hoc 重新签名后，macOS 可能把新二进制视为不同应用。如果权限看似已勾选但仍失败，删除旧条目后重新加入当前 App。

仅在权限状态明显损坏时重置：

```bash
tccutil reset Accessibility com.baotwo.fontick
tccutil reset AppleEvents com.baotwo.fontick
```

重置后必须重新授权。

## 8. 通用人工回归

专项功能按当轮提示词测试；每次至少覆盖以下基础路径。

### 8.1 窗口与焦点

- 点击字体、滚动列表、拖动 Fontick 窗口时，Photoshop 文字编辑状态和选区不应丢失。
- 搜索框可以输入英文和中文，输入法候选框位置正常。
- 关闭后重新显示、切换窗口和连续点击不应崩溃。

### 8.2 普通字体

- 非编辑态：修改整个活动文字图层。
- 编辑态、只有输入光标：退出编辑并修改整个图层。
- 正向和反向文字选区：只修改选区。
- 从第一个字符开始的选区、重复文字、跨行选区：范围正确。
- 点文本、段落文本、竖排、多行、emoji、组合字符和中英文混排正常。

### 8.3 复合字体

- 按当前规则应用到整个活动文字图层。
- 汉字、假名、拉丁字母、数字、标点、符号和自定义正则分类正确。
- “跟随基础字体”必须回到基础字体，不能继承前一个分类字体。
- 原有字号、颜色、字距等非字体样式不应被破坏。
- 一次撤销应恢复本次复合字体修改。

### 8.4 状态与稳定性

- 剪贴板文字、图片和其他格式完整恢复。
- 无文档、非文字图层和缺失字体时提示正确。
- 连续点击至少 10 次，无卡死、长时间排队或崩溃。
- 记录典型操作耗时以及异常复现次数。

## 9. Release 构建

只有 Debug 和人工回归通过后再构建：

```bash
cd app
npm run build
```

Release App 通常位于：

```text
app/src-tauri/target/release/bundle/macos/Fontick.app
```

验证：

```bash
APP="src-tauri/target/release/bundle/macos/Fontick.app"

file "$APP/Contents/MacOS/fontick"
lipo -archs "$APP/Contents/MacOS/fontick"
plutil -p "$APP/Contents/Info.plist" \
  | grep -E 'CFBundleIdentifier|CFBundleShortVersionString|CFBundleVersion'
codesign --verify --deep --strict --verbose=2 "$APP"
```

如果内部测试 App 尚未签名，可使用 ad-hoc 签名：

```bash
codesign --force --deep --sign - "$APP"
codesign --verify --deep --strict --verbose=2 "$APP"
```

ad-hoc 签名仅适合内部测试，不等于 Developer ID 签名和 Apple 公证。对外发布如需避免 Gatekeeper 警告，应另行配置 Developer ID 和 notarization。

## 10. 压缩和校验

从 `Info.plist` 读取版本，避免手工写错文件名：

```bash
APP="src-tauri/target/release/bundle/macos/Fontick.app"
VERSION=$(/usr/libexec/PlistBuddy \
  -c 'Print :CFBundleShortVersionString' \
  "$APP/Contents/Info.plist")
OUT="Fontick-${VERSION}-macos-arm64.zip"

ditto -c -k --sequesterRsrc --keepParent \
  "$APP" \
  "$OUT"

unzip -t "$OUT"
shasum -a 256 "$OUT" > "${OUT}.sha256"
cat "${OUT}.sha256"
```

回传前再次确认：

```bash
git rev-parse HEAD
git status --short --branch
file "$APP/Contents/MacOS/fontick"
lipo -archs "$APP/Contents/MacOS/fontick"
codesign --verify --deep --strict --verbose=2 "$APP"
```

没有修改源码时，通常只需回传：

```text
Fontick-<version>-macos-arm64.zip
Fontick-<version>-macos-arm64.zip.sha256
测试结论
完整 HEAD
git status
```

如果测试过程中修改了源码，应停止发布并按当轮要求回传 Git Bundle、Patch 和 SHA-256；不要在测试机登录 GitHub 或直接推送。

## 11. 常见问题

### Node.js 或 Rust 架构错误

症状：构建产物出现 x86_64，或链接到错误架构依赖。

```bash
arch
file "$(command -v node)"
rustc -Vv
```

确保终端、Node.js 和 Rust host 均为 arm64。

### 无法控制 Photoshop

错误包含 `-1743` 时，检查：

```text
系统设置 → 隐私与安全性 → 自动化 → Fontick → Adobe Photoshop
```

### 无法读取或退出文字编辑

检查“辅助功能”是否授权给本轮实际运行的 `Fontick.app`，而不是旧路径中的同名 App。

### 点击 Fontick 后 Photoshop 文字选区丢失

确认当前构建使用固定 Tao revision，并启用了：

```text
fontick-nonactivating-panel
```

不要将 Tao 临时替换为 crates.io 原版，也不要用 `/tmp/...` 本地绝对路径覆盖依赖。

### 启动的不是当前构建

```bash
pkill -x fontick 2>/dev/null || true
open "/absolute/path/to/current/Fontick.app"
ps -axo pid=,comm= | grep -i Fontick
```

## 12. 清理测试环境

先把 ZIP、SHA-256、测试结果和需要保留的补丁移出 worktree。测试字体也应移出仓库，不要提交或打进发布包。

确认工作区：

```bash
git status --short --branch
```

如果使用 worktree，在原仓库执行：

```bash
cd /path/to/existing/Fontick
git worktree list
git worktree remove ../Fontick-macos-test
git worktree prune
```

如果 worktree 中存在未跟踪文件，先逐项确认并移走；不要直接使用 `--force` 删除未知文件。

按需清理构建缓存：

```bash
cd /path/to/Fontick-macos-test/app
cargo clean --manifest-path src-tauri/Cargo.toml
rm -rf node_modules
```

只有确认路径正确且不再需要源码时，才删除测试工作区。权限也不再需要时，可从系统设置中删除 Fontick，或执行：

```bash
tccutil reset Accessibility com.baotwo.fontick
tccutil reset AppleEvents com.baotwo.fontick
```
