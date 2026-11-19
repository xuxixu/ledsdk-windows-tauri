#!/usr/bin/env bash
# 在 WSL Debian 上做 Linux 原生构建验证：装 Rust + 复制到本地盘 + 编译 + 自检。
# 用法：wsl.exe -u root -e bash /mnt/d/Project/LedSdk_windows_Tauri/build-linux-verify.sh
set -euo pipefail

SRC=/mnt/d/Project/LedSdk_windows_Tauri
WORK=/root/ledsdk-build
export RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/usr/local/cargo
export PATH="$CARGO_HOME/bin:$PATH"

echo "== 1/4 系统依赖 =="
export DEBIAN_FRONTEND=noninteractive
apt-get install -y --no-install-recommends build-essential curl pkg-config \
  libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev >/dev/null 2>&1

echo "== 2/4 安装 Rust（若未安装） =="
if ! command -v cargo >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --no-modify-path --profile minimal --default-toolchain stable
fi
rustc --version; cargo --version

echo "== 3/4 复制到本地 ext4 并编译 =="
# /mnt/d 是 Windows 挂载盘，IO 很慢；本地盘编译从 ~10min 降到 ~1.5min
mkdir -p "$WORK"
for d in src ui icons config sdk capabilities; do cp -r "$SRC/$d" "$WORK/" 2>/dev/null || true; done
cp "$SRC/Cargo.toml" "$SRC/build.rs" "$SRC/tauri.conf.json" "$WORK/"
cd "$WORK"
cargo build

echo "== 4/4 无窗口自检 =="
./target/debug/ledsdk-tauri --selfcheck

echo "== 完成 =="
ls -l target/debug/ledsdk-tauri target/debug/libLedSDK.so.1 target/debug/LedSDKTextRenderer
