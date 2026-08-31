# Changelog

All notable changes to this crate are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this crate
adheres to **additive-only versioning** once 1.0 is reached.

## Versioning policy

- **Pre-1.0 (current):** `0.x` — the API may still change; treat as unstable.
- **Post-1.0:** the only expected changes are **new `BitWidth` variants**
  (e.g. `Int1`, `Int6`). Adding a variant is an additive, non-breaking change to
  an `#[non_exhaustive]` enum, so it is a **minor** bump. No breaking changes to
  existing public items will ever be made.
- The frozen public vocabulary (`BitWidth`, `QuantScheme`, `QuantError`,
  `GroupScale`, `compute_group_scale`, `pack_bits`/`unpack_bits`,
  `quantize_group`/`dequantize_group`) is guaranteed stable for the entire 1.x
  line.

## [Unreleased]

### Added
- `BitWidth` enum: `Int2`, `Int3`, `Int4`, `Int8`.
- `QuantScheme` struct (`bits`, `group_size`, `symmetric`).
- `QuantError` shared error type (all fallible functions return this).
- `GroupScale` (`scale`, `zero_point`).
- `compute_group_scale` — symmetric & asymmetric group scaling, with degenerate
  (all-zero / single-element / `min == max`) handling.
- `pack_bits` / `unpack_bits` — bit-packing for all four widths, `no_std`,
  allocation-free, never panicking.
- `quantize_group` / `dequantize_group` — the correctness oracle, using
  round-half-to-even and incremental (no-scratch) packing.
- `RunningStats` *(alloc)* — running min/max/count + fixed-bucket histogram
  percentile estimation.
- Property tests (proptest): pack↔unpack round-trip, quantize↔dequantize error
  bound, scale determinism/non-negativity, across every
  `(bits, group_size, symmetric)` combination.
- Exhaustive edge-case tests (`tests/pack_edges.rs`).
- Fuzz target `fuzz/fuzz_pack_unpack.rs` (cargo-fuzz, sibling-repo convention).
- `no_std` core confirmed building with `--no-default-features` (no `alloc`).
- `cargo-deny` + empty-normal-dependency-tree CI guard to enforce the
  zero-external-dependency goal.
