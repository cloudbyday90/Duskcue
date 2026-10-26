#!/bin/sh
# Duskcue — Self-hosted media streaming server
# Copyright (C) 2026-2026 Duskcue Contributors
# Licensed under AGPL-3.0. See LICENSE for details.

set -eu
set -o pipefail
fixture="$(mktemp -d /tmp/duskcue-build-cache-XXXXXX)"
case "$fixture" in /tmp/duskcue-build-cache-*) ;; *) exit 1 ;; esac
trap 'rm -rf "$fixture"' EXIT
cd "$fixture"
guard=/usr/local/libexec/duskcue-compile-cache.sh
artifact=/usr/local/libexec/duskcue-cargo-artifact.sh
mkdir -p server/src crates/types/src crates/db/src vendor/plist-1.9.0/src clients/desktop/src-tauri/src native/ffmpeg-bootstrap evidence
printf '%s\n' '[workspace]' 'members = ["server", "crates/types", "crates/db", "vendor/plist-1.9.0", "clients/desktop/src-tauri"]' 'resolver = "3"' > Cargo.toml
for package in duskcue duskcue-types duskcue-db duskcue-desktop plist; do
    case "$package" in
        duskcue) directory=server ;; duskcue-types) directory=crates/types ;; duskcue-db) directory=crates/db ;;
        duskcue-desktop) directory=clients/desktop/src-tauri ;; plist) directory=vendor/plist-1.9.0 ;;
    esac
    printf '[package]\nname = "%s"\nversion = "1.0.0"\nedition = "2024"\n' "$package" > "$directory/Cargo.toml"
    printf '\n[features]\nplayback-qualification = []\n' >> "$directory/Cargo.toml"
    printf '%s\n' 'pub fn present() -> bool { true }' > "$directory/src/lib.rs"
done
printf '%s\n' 'pub fn value() -> &'"'"'static str { "one" }' '#[test] fn actual_value() { assert_eq!(value(), std::env::var("DUSKCUE_CACHE_EXPECT").unwrap()); }' > server/src/lib.rs
cp server/src/lib.rs original.rs
cargo generate-lockfile --offline
tests=0
pass() { tests=$((tests + 1)); printf 'CACHE_TEST %s\n' "$1"; }
prepared() { sh "$guard" | grep -qx 'DUSKCUE_COMPILER_CACHE=prepared'; }
verify() {
    cargo test --release --locked --offline -p duskcue -j 2 --lib --no-run --message-format=json > evidence/artifacts.jsonl
    sh "$artifact" evidence/artifacts.jsonl duskcue none "$fixture/evidence/tests"
    (cd evidence && sha256sum -c tests.sha256)
    DUSKCUE_CACHE_EXPECT="$1" evidence/tests --test-threads=1
}
prepared
verify one
pass absent_receipt_and_verified_binary
printf '%s\n' retained > target/release/dependency-sentinel
sh "$guard" | grep -qx 'DUSKCUE_COMPILER_CACHE=matching'
test -f target/release/dependency-sentinel
pass same_input_resume
sed 's/"one"/"two"/' original.rs > server/src/lib.rs
touch -r Cargo.lock server/src/lib.rs
prepared
verify two
pass changed_backdated_source
cp original.rs server/src/lib.rs
touch -r Cargo.lock server/src/lib.rs
prepared
verify one
pass reverted_backdated_source
printf '%s\n' corrupt > target/.duskcue-prepared-v1
prepared
test -f target/release/dependency-sentinel
pass corrupt_receipt_preserves_unrelated_work
real_cargo="$(command -v cargo)"
mkdir bin
printf '#!/bin/sh\nif [ "$1" = clean ]; then exit 73; fi\nexec "%s" "$@"\n' "$real_cargo" > bin/cargo
chmod 0555 bin/cargo
printf '%s\n' corrupt > target/.duskcue-prepared-v1
if PATH="$fixture/bin:$PATH" sh "$guard"; then exit 1; fi
test ! -e target/.duskcue-prepared-v1
test ! -e target/.duskcue-prepared-v1.tmp
prepared
pass failed_preparation_has_no_receipt
rm target/.duskcue-prepared-v1
printf '%s\n' interrupted > target/.duskcue-prepared-v1.tmp
prepared
test ! -e target/.duskcue-prepared-v1.tmp
pass interrupted_preparation
RUST_IMAGE=changed prepared
test ! -e target/release/dependency-sentinel
prepared
pass toolchain_identity_change
mkdir -p target/release
printf '%s\n' retained > target/release/dependency-sentinel
printf '%s\n' corrupt > target/.duskcue-environment-v1
prepared
test ! -e target/release/dependency-sentinel
pass unknown_environment_discards_dependencies
VOD_OUTPUT_MODE=changed prepared
prepared
pass mode_identity_change
verify one
jq -c 'if .reason == "compiler-artifact" then .features = ["playback-qualification"] else . end' evidence/artifacts.jsonl > evidence/bad.jsonl
if sh "$artifact" evidence/bad.jsonl duskcue none evidence/refused; then exit 1; fi
pass feature_mismatch
jq -c 'if .reason == "compiler-artifact" then .executable = "/tmp/foreign" else . end' evidence/artifacts.jsonl > evidence/bad.jsonl
if sh "$artifact" evidence/bad.jsonl duskcue none evidence/refused; then exit 1; fi
pass foreign_path
cat evidence/artifacts.jsonl > evidence/bad.jsonl
jq -c 'select(.reason == "compiler-artifact") | .executable += "a"' evidence/artifacts.jsonl >> evidence/bad.jsonl
if sh "$artifact" evidence/bad.jsonl duskcue none evidence/refused; then exit 1; fi
pass ambiguous_artifact
jq -c 'if .reason == "compiler-artifact" then .profile.test = false else . end' evidence/artifacts.jsonl > evidence/bad.jsonl
if sh "$artifact" evidence/bad.jsonl duskcue none evidence/refused; then exit 1; fi
pass wrong_profile
mv Cargo.lock saved.lock
if sh "$guard"; then exit 1; fi
mv saved.lock Cargo.lock
pass incomplete_inputs_refused
real_sha256="$(command -v sha256sum)"
printf '#!/bin/sh\nfor argument do\nif [ "$argument" = server/src/lib.rs ]; then exit 71; fi\ndone\nexec "%s" "$@"\n' "$real_sha256" > bin/sha256sum
chmod 0555 bin/sha256sum
if PATH="$fixture/bin:$PATH" sh "$guard"; then exit 1; fi
pass file_hash_failure_refused
cargo test --release --locked --offline -p duskcue -j 2 --features playback-qualification --lib --no-run --message-format=json > evidence/paced.jsonl
sh "$artifact" evidence/paced.jsonl duskcue paced "$fixture/evidence/paced-tests"
if sh "$artifact" evidence/paced.jsonl duskcue none evidence/refused; then exit 1; fi
(cd evidence && sha256sum -c paced-tests.sha256)
pass actual_feature_parity
printf 'DUSKCUE_BUILD_DURABILITY={"passed":true,"tests":%s,"actual_cargo":true,"changed_and_reverted_backdated_source":true}\n' "$tests"
