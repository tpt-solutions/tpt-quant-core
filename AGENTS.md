# AGENTS.md — tpt-quant-core

## What this crate is (and is not)
- Rust `no_std`, **zero-runtime-dependency** library. Quantization math only: group scale, `int2/3/4/8` pack/unpack, dequant, calibration stats.
- Hard non-goals (do not add): GPU/WGSL/backend code, model-format code (`.gguf`/`.safetensors`), training/QAT, feature flags beyond `alloc`.

## Load-bearing constraints
- **Never add a runtime dependency.** CI enforces this via `cargo tree -e normal --no-default-features` being empty and `cargo-deny`. The only allowed dependency is `alloc` behind the `alloc` feature. If you think you need a crate, stop.
- **Frozen-at-1.0 public API.** Do not rename or change semantics of `BitWidth`, `QuantScheme`, `QuantError`, `GroupScale`, `compute_group_scale`, `quantize_group`, `dequantize_group`, `pack_bits`, `unpack_bits`. Post-1.0 changes are limited to *additive* `BitWidth` enum variants only.
- **`dequantize_group` is the correctness oracle** that two other repos (`tpt-kv-quant`, Project 2's `quantize/awq.rs`) diff against for bit-exact equivalence. Its output must be exact, never "close enough".
- **Rounding rule is `round_half_to_even` (banker's rounding),** implemented in `scale.rs` with raw IEEE-754 bit math because `f32::round`/`floor` are `std`-only and this crate is `no_std` with no `libm`. Do not replace it with `std`/`libm` rounding.

## Dev commands
```bash
cargo test --all-features          # unit + property tests (proptest)
cargo test --no-default-features   # confirms no_std core builds without alloc
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all --check            # rustfmt: edition 2024, max_width 100
cargo miri test --all-features --lib   # only needed if you touched pack/dequant
```
- CI (`ci.yml`) runs the matrix `stable / beta / 1.85.0` (MSRV) × `--all-features / --no-default-features`, with `RUSTFLAGS=-D warnings`, `cargo-deny`, and a weekly scheduled fuzz job.
- Fuzzing: `cargo +nightly fuzz run fuzz_pack_unpack` from the `fuzz/` dir (target `fuzz_pack_unpack`).

## Test gotchas
- Property tests use **proptest** (`dev-dependency`). If a property test fails and writes a regression, **commit it** (`tests/roundtrip.proptest-regressions`) rather than deleting — it pins a discovered failing case.
- The library is `#![no_std]`, but tests pull in `std` via `#[cfg(test)] extern crate std`; don't assume `std` is available in `src/`.

## Tooling config
- `clippy.toml`: `avoid-breaking-exported-api = true` (don't break the public API), `msrv = "1.85.0"`.
- `Cargo.toml` sets `lints.clippy.all = warn` with `priority = -1`; CI still escalates to `-D warnings`.
- `deny.toml`: licensing (MIT/Apache-2.0 + permissive cousins), crates.io-only sources.
