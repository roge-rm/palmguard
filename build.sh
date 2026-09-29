#!/bin/sh
# Build the arm64 binary and package the Magisk module zip.
#   On a PC:     cargo install cargo-ndk && rustup target add aarch64-linux-android
#                (needs the Android NDK; ANDROID_NDK_HOME set)
#   In Termux:   pkg install rust zip   (builds natively, no NDK needed)
set -e
cd "$(dirname "$0")"

if [ -n "$TERMUX_VERSION" ]; then
  cargo build --release
  BIN=target/release/palmguard
else
  cargo ndk -t arm64-v8a --platform 29 build --release
  BIN=target/aarch64-linux-android/release/palmguard
fi

mkdir -p module/bin
cp "$BIN" module/bin/palmguard
rm -f palmguard-magisk.zip
(cd module && zip -r9 ../palmguard-magisk.zip . -x '.*')
echo "Built palmguard-magisk.zip"
