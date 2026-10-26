#!/bin/sh
# Duskcue — Self-hosted media streaming server
# Copyright (C) 2026-2026 Duskcue Contributors
# Licensed under AGPL-3.0. See LICENSE for details.

set -eu
set -o pipefail
root="$(pwd -P)"
cache="$root/target"
receipt="$cache/.duskcue-prepared-v1"
environment_receipt="$cache/.duskcue-environment-v1"
mkdir -p "$cache"
inputs="$(mktemp /tmp/duskcue-compiler-inputs-XXXXXX)"
trap 'rm -f "$inputs"' EXIT
{
        printf '%s\n' 'duskcue-recovery-environment-v1' "${RUST_IMAGE:?}" "${TARGETPLATFORM:?}" release
        env | LC_ALL=C sort | sed -n '/^\(CARGO_\|RUST\|CC=\|CXX=\|AR=\|CFLAGS=\|CXXFLAGS=\|LDFLAGS=\|PKG_CONFIG\|PROTOC\)/p'
        rustc -vV
        cargo --version
        cc --version
        protoc --version
        sha256sum /lib/apk/db/installed
} > "$inputs"
environment_identity="$(sha256sum "$inputs" | cut -d ' ' -f 1)"
{
        printf '%s\n' "$environment_identity" "${NATIVE_RECOVERY_MODE:?}" "${VOD_OUTPUT_MODE:?}" \
            "${FORMAT_CORPUS_MODE:?}" "${SOURCE_CAPABILITIES_MODE:?}"
        sha256sum /usr/local/libexec/duskcue-compile-cache.sh /usr/local/libexec/duskcue-cargo-artifact.sh \
            /usr/local/libexec/duskcue-recovery.Dockerfile
        find Cargo.toml Cargo.lock vendor crates server clients/desktop/src-tauri native/ffmpeg-bootstrap \
            -type f -print0 | LC_ALL=C sort -z | xargs -0 sha256sum
} > "$inputs"
identity="$(sha256sum "$inputs" | cut -d ' ' -f 1)"
matching() { [ -f "$1" ] && [ ! -L "$1" ] && [ "$(wc -c < "$1")" -eq 65 ] && [ "$(cat "$1")" = "$2" ]; }
if ! matching "$environment_receipt" "$environment_identity" || ! matching "$receipt" "$identity"; then
    rm -f "$receipt" "$receipt.tmp"
    if matching "$environment_receipt" "$environment_identity"; then
        cargo clean --release --locked --target-dir "$cache" \
            -p duskcue -p duskcue-types -p duskcue-db -p duskcue-desktop -p plist
    else
        rm -f "$environment_receipt" "$environment_receipt.tmp"
        cargo clean --release --locked --target-dir "$cache"
        printf '%s\n' "$environment_identity" > "$environment_receipt.tmp"
        mv -f "$environment_receipt.tmp" "$environment_receipt"
    fi
    printf '%s\n' "$identity" > "$receipt.tmp"
    mv -f "$receipt.tmp" "$receipt"
    printf '%s\n' 'DUSKCUE_COMPILER_CACHE=prepared'
else
    printf '%s\n' 'DUSKCUE_COMPILER_CACHE=matching'
fi
