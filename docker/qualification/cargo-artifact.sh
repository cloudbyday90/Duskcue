#!/bin/sh
# Duskcue — Self-hosted media streaming server
# Copyright (C) 2026-2026 Duskcue Contributors
# Licensed under AGPL-3.0. See LICENSE for details.

set -eu
manifest="$1"
target="$2"
mode="$3"
output="$4"
case "$target" in playback_stop_contract|playback_retention) kind=test ;; duskcue) kind=lib ;; *) exit 1 ;; esac
case "$mode" in none|paced) ;; *) exit 1 ;; esac
executable="$(jq -ers --arg target "$target" --arg kind "$kind" \
    '[.[] | select(.reason == "compiler-artifact" and .target.name == $target and ((.target.kind // []) | index($kind)) and .profile.test == true) | .executable | select(. != null)] | unique | select(length == 1) | .[0]' "$manifest")"
case "$executable" in "$PWD/target/release/deps/$target"-*) test -f "$executable" && test -x "$executable" && test ! -L "$executable" ;; *) exit 1 ;; esac
suffix="${executable#"$PWD/target/release/deps/$target-"}"
case "$suffix" in ''|*[!0-9a-f]*) exit 1 ;; esac
jq -es --arg target "$target" --arg kind "$kind" --arg mode "$mode" --arg executable "$executable" \
    '[.[] | select(.reason == "compiler-artifact" and .target.name == $target and ((.target.kind // []) | index($kind)) and .profile.test == true and .executable == $executable)] | length > 0 and all(.[]; (((.features // []) | index("playback-qualification") != null) == ($mode == "paced")))' "$manifest" >/dev/null
cp "$executable" "$output"
(cd "$(dirname "$output")" && sha256sum "$(basename "$output")" > "$(basename "$output").sha256")
