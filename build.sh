#!/bin/sh
# Build the arm64 binary and package the Magisk module zip.
#   On a PC:     cargo install cargo-ndk && rustup target add aarch64-linux-android
#                (needs the Android SDK + NDK; ANDROID_NDK_HOME set, and
#                app/local.properties pointing at the SDK)
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

# The control app, installed by Magisk as a system app. Skipped in Termux
# (no Android SDK); set NO_APP=1 to skip it elsewhere.
if [ -z "$TERMUX_VERSION" ] && [ -z "$NO_APP" ]; then
  (cd app && ./gradlew -q assembleRelease)
  mkdir -p module/system/app/PalmGuard
  cp app/app/build/outputs/apk/release/app-release.apk module/system/app/PalmGuard/PalmGuard.apk
fi
rm -f palmguard-magisk.zip
(cd module && zip -r9 ../palmguard-magisk.zip . -x '.*')
echo "Built palmguard-magisk.zip"
