# tpt-quant-core

Minimal, stable, **zero-dependency** quantization math for inference-time use:
group-wise scale computation, `int8`/`int4`/`int3`/`int2` pack–unpack, scalar
dequantization, and running calibration statistics.

This is the shared, frozen-at-1.0 vocabulary that every quantization consumer in
the org needs and none of them should reimplement. `tpt-kv-quant`'s `cpu_ref`
and Project 2's `quantize/awq.rs` diff their optimized implementations against
[`quantize_group`]/[`dequantize_group`] here — so those two functions are the
correctness oracle for two other repos, not just this one.

- **License:** MIT OR Apache-2.0
- **MSRV:** 1.85.0 (edition 2024)
- **`no_std`:** yes. The only optional dependency is `alloc`, behind the `alloc`
  feature (used by calibration stats). Core scale / pack / dequant need neither
  `std` nor `alloc`.

## Non-goals

Written into this README on day one, and audited before every release:

- **No GPU / WGSL / backend code.** That is `tpt-kv-quant` and Project 2's
  `quantize/awq.rs`. This crate is portable scalar math only.
- **No model-format code.** It does not know what a `.gguf` or `.safetensors`
  file is. It takes `&[f32]` and returns bytes. Full stop.
- **No training-time / QAT support.** Inference-time quantization only.
- **No feature flags beyond `alloc`.** Every speculative feature is a future
  maintenance liability on a crate whose whole point is not needing maintenance.

## Quick start

```rust
use tpt_quant_core::{compute_group_scale, quantize_group, dequantize_group, QuantScheme, BitWidth};

const SCHEME: QuantScheme = QuantScheme { bits: BitWidth::Int4, group_size: 4, symmetric: true };
let values = [0.15f32, -0.30, 0.05, 0.22];

let scale = compute_group_scale(&values, &SCHEME).unwrap();
let mut packed = [0u8; SCHEME.packed_len(4)];
quantize_group(&values, &scale, &SCHEME, &mut packed).unwrap();

let mut dequant = [0.0f32; 4];
dequantize_group(&packed, &scale, &SCHEME, &mut dequant).unwrap();
// `dequant` approximates `values` within 0.5 * scale.
```

## API overview

All functions are re-exported at the crate root (frozen vocabulary):

| Item | Purpose |
| --- | --- |
| `BitWidth` | `Int2` / `Int3` / `Int4` / `Int8` enum (additive only) |
| `QuantScheme` | `bits`, `group_size`, `symmetric` |
| `QuantError` | shared fallible return (never panics) |
| `GroupScale` | `scale` + `zero_point` |
| `compute_group_scale` | group → `GroupScale` (symmetric & asymmetric) |
| `pack_bits` / `unpack_bits` | raw bit-packing primitives |
| `quantize_group` / `dequantize_group` | the correctness oracle |
| `RunningStats` *(alloc)* | `min`/`max`/`count` + histogram percentiles |

### Rounding rule (load-bearing)

Quantization rounds each scaled value with **round-half-to-even** (banker's
rounding): ties (exact `.5`) round to the nearest *even* integer. This is the
single unambiguous rule downstream crates depend on for bit-exact diffs. It is
implemented in `scale::round_half_to_even` with raw IEEE-754 bit math because
`f32::floor`/`round` are `std`-only and this crate is `no_std` with no `libm`.

Dequantization is `scale * (q - zero_point)` (symmetric: `zero_point == 0`).

## How downstream crates consume this

1. Pick a `QuantScheme` (shared convention across the org).
2. For each group of `group_size` floats, call `compute_group_scale`.
3. Transport `(scheme, scale, packed_bytes)` to the consumer.
4. The consumer calls `dequantize_group` to recover floats — or, for optimized
   paths, implements its own dequant and **diffs against `dequantize_group`** to
   prove bit-equivalence on a test corpus.

## Development

```bash
cargo test --all-features      # unit tests + property tests
cargo test --no-default-features   # confirms no_std core builds without alloc
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all --check
cargo miri test --all-features --lib   # UB check for bit manipulation
```

Fuzzing (matches sibling-repo convention):

```bash
cargo +nightly fuzz run fuzz_pack_unpack
```

## Status

Pre-1.0. The core logic is complete and tested; see the roadmap (`todo.md`) for
the path to a stable 1.0. After 1.0, the only expected changes are new
`BitWidth` variants — additive, non-breaking enum additions.
