# tpt-quant-core — Project Roadmap

License: MIT OR Apache-2.0 · Copyright TPT Solutions

Status snapshot (2026-08-31): Phases 1–5 are functionally complete and verified locally
(`cargo test --all-features`, `cargo build --no-default-features`, `cargo clippy --all-targets
--all-features -- -D warnings`, `cargo fmt --all --check` all green). Phase 0 governance/CI
scaffolding is in place; the repo just hasn't been pushed to `origin` yet. Remaining open work
is genuinely time-gated or CI-only (branch protection, scheduled fuzz runs, Miri in CI, the
soak period, and the crates.io release itself).

## Phase 0 — Project Setup & Governance
- [x] `cargo init` as a single crate (not a workspace) with `#![no_std]` and `alloc` as the only feature
- [x] Add `LICENSE-MIT` and `LICENSE-APACHE` (copyright: TPT Solutions), set `license = "MIT OR Apache-2.0"`, `authors = ["TPT Solutions"]`, `repository`, `description`, `keywords`, `categories` in `Cargo.toml`
- [x] Add SPDX license headers where appropriate, README stub, CONTRIBUTING.md, `.gitignore`
- [x] Choose and document MSRV (minimum supported Rust version) — 1.85.0, edition 2024
- [x] Add `rustfmt.toml` and `clippy.toml`; enforce in CI
- [x] Set up GitHub Actions CI: build + test (default and `--features alloc`) + fmt check + clippy on stable/beta/MSRV
- [x] Add `cargo-deny` to CI to guard the zero-external-dependency goal — fail CI if any non-dev dependency is ever added
- [x] Scaffold `tests/roundtrip.rs` and `tests/pack_edges.rs` (wired into `cargo test`; both now fully implemented, see Phase 3/7)
- [x] Scaffold `fuzz/` directory with `cargo-fuzz`, matching tpt-zero-bytes'/tpt-local-ai's fuzz-target convention
- [x] Write the non-goals into README on day one: no GPU/WGSL/backend code, no model-format code (no `.gguf`/`.safetensors` awareness), no training-time/QAT support, no feature flags beyond `alloc`
- [ ] Create GitHub repo under `github.com/tpt-solutions/tpt-quant-core`, set default branch protections — `origin` remote is already configured; repo exists but local `master` is 1 commit ahead and hasn't been pushed, and branch protection rules haven't been confirmed on GitHub yet

## Phase 1 — `scheme.rs`: stable public vocabulary
- [x] Define `BitWidth` enum: `Int2`, `Int3`, `Int4`, `Int8`
- [x] Define `QuantScheme` struct: `bits: BitWidth`, `group_size: usize`, `symmetric: bool`
- [x] Define `QuantError` (shared error type for pack/unpack/quantize/dequantize)
- [x] Doc-comment every public item — this module is the frozen-at-1.0 vocabulary, get names/semantics right before building on it
- [x] Unit tests: enum exhaustiveness, `Debug`/`Clone`/`Copy`/`PartialEq` derives as appropriate

## Phase 2 — `scale.rs`: group-wise scale/zero-point computation
- [x] Define `GroupScale` struct: `scale: f32`, `zero_point: i32` (unused when symmetric, kept so callers don't branch)
- [x] Implement `compute_group_scale(values: &[f32], scheme: &QuantScheme) -> GroupScale` for the symmetric case
- [x] Implement the asymmetric case (min/max based zero-point)
- [x] Handle degenerate inputs: all-zero group, single-element group, group with `min == max`
- [x] Property tests (proptest): scale computation is deterministic and scale is always non-negative for random `f32` slices across all `QuantScheme` combinations

## Phase 3 — `pack.rs`: bit-packing primitives
- [x] Implement `pack_bits(values: &[i8], bits: BitWidth, out: &mut [u8]) -> Result<(), QuantError>` for Int8, Int4, Int3, Int2
- [x] Implement `unpack_bits(packed: &[u8], bits: BitWidth, out: &mut [i8]) -> Result<(), QuantError>` for all four widths
- [x] Validate output buffer sizing; return `QuantError` (not panic) on mismatched lengths
- [x] Exhaustive edge cases in `tests/pack_edges.rs`: odd-length arrays, `group_size` not dividing evenly, minimum/maximum representable values per bit-width, empty input
- [x] Property tests (proptest) in `tests/roundtrip.rs`: `pack_bits` → `unpack_bits` round-trips exactly for random `i8` values within each bit-width's representable range

## Phase 4 — `dequant.rs`: the correctness oracle
- [x] Implement `quantize_group(values: &[f32], scale: &GroupScale, scheme: &QuantScheme, out: &mut [u8]) -> Result<(), QuantError>` (builds on `pack_bits`)
- [x] Implement `dequantize_group(packed: &[u8], scale: &GroupScale, scheme: &QuantScheme, out: &mut [f32]) -> Result<(), QuantError>` (builds on `unpack_bits`)
- [x] Property tests: quantize → dequantize error stays within the theoretical bound for the given bit-width, for random inputs across every `(bits, group_size, symmetric)` combination
- [x] Extra scrutiny pass: this function is the oracle that `tpt-kv-quant`'s `cpu_ref` and Project 2's `quantize/awq.rs` will diff their optimized implementations against — review for bit-exact correctness, not just "close enough"
- [x] Document the exact rounding rule used (e.g. round-half-to-even) since downstream diffs depend on it being unambiguous

## Phase 5 — `calibration.rs`: running statistics (`alloc`-gated)
- [x] Define `RunningStats` struct behind `#[cfg(feature = "alloc")]`: min, max, count, fixed-bucket histogram
- [x] Implement `RunningStats::update(&mut self, values: &[f32])`
- [x] Implement `RunningStats::percentile(&self, p: f32) -> f32` via the histogram
- [x] Decide and document histogram bucket strategy (fixed range vs. adaptive) and its accuracy tradeoff
- [x] Unit tests: known distributions (uniform, all-equal, single outlier) produce expected percentile estimates
- [x] Confirm the rest of the crate (`scheme`/`scale`/`pack`/`dequant`) builds and tests pass with `--no-default-features` (no `alloc`)

## Phase 6 — Fuzzing
- [x] `fuzz/fuzz_pack_unpack.rs`: arbitrary bytes → `pack_bits`/`unpack_bits` must never panic, out-of-bounds, or UB, for every `BitWidth`
- [ ] Run an initial fuzzing session locally to build a seed corpus and shake out any immediate crashes
- [x] Wire fuzzing into CI as a scheduled (not per-PR) job, matching sibling-repo convention

## Phase 7 — Validation & Hardening (required before 1.0)
- [x] Consolidate and finalize the full proptest matrix across all `(bits, group_size, symmetric)` combinations for round-trip error bounds
- [ ] Run `cargo test` and fuzz targets clean for a sustained period ("green and stable for a few weeks with no found bugs" per spec) before cutting 1.0
- [x] Audit public API surface against the frozen API in the spec — confirm no accidental extra `pub` items leaked out (crate root re-exports match the documented vocabulary; `#[deny(missing_docs)]` is on)
- [x] Confirm every non-goal in the README still holds (no GPU code, no model-format code, no QAT, no extra feature flags crept in)
- [ ] Miri run over `pack.rs`/`dequant.rs` to catch any undefined behavior in the bit manipulation code — wired into CI (`miri` job) but not yet actually run/confirmed green

## Phase 8 — Release Prep (v1.0)
- [ ] Finalize `Cargo.toml` metadata (license, repository, description, keywords, categories) for crates.io
- [ ] Write top-level README: purpose, API overview, non-goals, and how downstream crates (`tpt-kv-quant`, Project 2) are expected to consume this crate
- [ ] Add CHANGELOG.md and document the additive-only versioning policy (only new `BitWidth` variants expected post-1.0)
- [ ] `cargo publish --dry-run` and manual review of the packaged crate contents
- [ ] Manually `cargo publish` v1.0.0 to crates.io
- [ ] Tag and publish the v1.0.0 GitHub release with notes

## Post-1.0 (tracked, not scheduled)
- [ ] New `BitWidth` variants (e.g. `Int1`, `Int6`) as additive, non-breaking enum additions only
