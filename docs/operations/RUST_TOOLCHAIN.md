# Local Rust toolchain maintenance

## October 24, 2026 update

The user requested a Rust SDK update during the [IPTV contract increment](../design/IPTV.md). The installed Windows default is rustup-managed `stable-x86_64-pc-windows-msvc`; there is no repository toolchain override. The requested operation updates that installed SDK and its existing components. It does not select beta/nightly, update application dependencies, change the Rust edition/resolver, or claim that existing Linux compiler/container artifacts were rebuilt.

Before the update, `rustc` and Cargo were 1.96.0 and rustup was 1.29.0. `rustup check` identified stable Rust 1.99.1 and rustup 1.29.2 as available. The official [Rust 1.99.1 announcement](https://blog.rust-lang.org/2026/10/15/Rust-1.99.1/), [rustup 1.29.2 announcement](https://blog.rust-lang.org/2026/10/20/Rustup-1.29.2/) and [rustup update guidance](https://rust-lang.github.io/rustup/basics.html?highlight=update) were discovered through MCP search and opened before updating.

Rust 1.99.1 is the current stable point release in the retrieved official release index. Its fixes include FFI boolean return behavior and target/linker regressions. Rustup 1.29.2 updates certificate-validation dependencies. Prefer the current stable SDK for this explicitly requested maintenance; the cost is invalidating compiler caches and potentially surfacing new diagnostics. A separately pinned project compiler would improve repeatability across developer machines, but would change the project's toolchain policy and is not introduced by this local update.

## Procedure and preserved state

Wait for the current owned Cargo build to finish before changing its compiler. Preserve the running editor, unrelated workloads, PATH, installed target set and existing components. Update rustup first, then the stable toolchain:

```powershell
rustup self update
rustup update stable --no-self-update
rustup show active-toolchain
rustc --version
cargo --version
rustup component list --installed
rustup target list --installed
rustup check
```

The original installed set is Cargo, Clippy, Rust documentation, rust-src, rust-std, rustc and rustfmt, with the `x86_64-pc-windows-msvc` target. No additional target is needed for the configuration-only server slice. Existing immutable compiler/test artifacts retain their original source/compiler boundary; do not label them as qualified under the replacement SDK without rebuilding.

Run local verification through [guarded testing](../ci/TESTING_MEMORY.md). Record updated versions, component/target comparison, formatting, relevant tests/builds and strict Clippy results. An update check reporting an available version is not proof of installation, and an old-compiler test run is not evidence for the updated compiler.

## Outcome

Installation succeeded: `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, `cargo 1.99.1 (5f94df478 2026-08-27)` and `rustup 1.29.2 (d8c92c676 2026-10-21)`. The default stable MSVC target and all seven original components are retained. `rustup check` reports both SDK and manager up to date, and `cargo fmt --all --check` passes under the updated toolchain. The full server library suite passes 969 cases with seven existing environment-dependent ignores, and the real PostgreSQL/production-router Live TV contract passes with no ignore. [IPTV verification](../design/IPTV.md#verification-and-outcome) records exact commands and memory evidence.

Strict Clippy exits 101 on unchanged code: Cargo reports 28 library diagnostics and 32 library-test diagnostics (28 duplicates). A subsequent explicitly non-strict `-W warnings` pass completes all targets and identifies two more unchanged navigation-test casts. Neither pass reports a diagnostic in the new/changed files. The diagnostic inventory deduplicates to 27 path/line/lint tuples: nine `too_many_arguments`, six `chunks_exact_to_as_chunks`, three `double_must_use`, two `byte_char_slices`, two `unnecessary_cast`, and one each of `needless_late_init`, `if_same_then_else`, `collapsible_if`, `items_after_test_module` and `needless_borrow`. These remain unresolved findings, not a new whitelist. The historical thirteen-diagnostic record from a different compiler/scope is not reused as the result for 1.99.1. No source allowance or lint-policy change was introduced.

The replacement compiler reports existing `proc-macro-error2 2.0.1` code that will be rejected by a future compiler (private `proc_macro` re-export, E0365). `cargo report future-incompatibilities --id 1` supplies the detailed report and upstream repository. Current application builds/tests pass; dependency maintenance is follow-up work rather than an unverified vendor patch in this increment.

Rustup reported access-denied cleanup warnings for two temporary download files. A subsequent explicit cleanup was rejected by automatic tool approval with `blocked by policy`; the files were left in the SDK temporary directory. Installed-version, component and update checks succeeded independently. The repository's compiler-image/release artifacts were not rebuilt, and no release is requested.
