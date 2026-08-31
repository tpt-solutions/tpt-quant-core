# Contributing to tpt-quant-core

Thanks for helping make the org's quantization math rock-solid.

## Principles

- **Zero external dependencies.** This crate must build with no runtime deps. The
  only allowed dependency is `alloc`, behind the `alloc` feature. If you think you
  need a crate, stop and discuss — the whole point is *not* needing maintenance.
- **`no_std` core.** Scale / pack / dequant must compile with
  `--no-default-features`. Only `calibration::RunningStats` may use `alloc`.
- **Frozen vocabulary.** `BitWidth`, `QuantScheme`, `QuantError`, `GroupScale`,
  and the four free functions are frozen at 1.0. Don't rename or change their
  semantics. New `BitWidth` variants are the *only* expected post-1.0 change, and
  must be additive enum additions.
- **The oracle is load-bearing.** `dequantize_group` is diffed against optimized
  implementations in two other repos. Its results must be bit-exact and
  unambiguous, not "close enough". `round_half_to_even` is the one rounding rule.

## Before opening a PR

```bash
cargo test --all-features
cargo test --no-default-features
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all --check
cargo miri test --all-features --lib   # only if you touched pack/dequant
```

## License

Contributions are dual-licensed under MIT OR Apache-2.0, matching the project.
By contributing you agree your contributions are licensed similarly.
