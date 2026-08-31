// SPDX-License-Identifier: MIT OR Apache-2.0

//! # tpt-quant-core
//!
//! Minimal, stable, zero-dependency quantization math.
//!
//! This crate provides the shared, frozen-at-1.0 vocabulary and primitives that
//! every quantization consumer in the org needs and none of them should
//! reimplement:
//!
//! - [`scheme`] — the stable public vocabulary ([`BitWidth`], [`QuantScheme`],
//!   [`QuantError`]).
//! - [`scale`] — group-wise scale / zero-point computation (symmetric &
//!   asymmetric).
//! - [`pack`] — `int8`/`int4`/`int3`/`int2` bit-packing and unpacking.
//! - [`dequant`] — scalar quantize / dequantize (the correctness oracle other
//!   crates diff against).
//! - [`calibration`] — running min/max + histogram-based percentile stats
//!   (requires the `alloc` feature).
//!
//! ## Non-goals
//!
//! This crate deliberately does **not** contain:
//!
//! - GPU / WGSL / backend-specific code (that is `tpt-kv-quant` and Project 2's
//!   `quantize/awq.rs`).
//! - Model-format code — it does not know what a `.gguf` or `.safetensors` file
//!   is. It takes `&[f32]` and returns bytes.
//! - Training-time / QAT support. Inference-time quantization only.
//! - Any feature flags beyond `alloc`.
//!
//! ## `no_std`
//!
//! The crate is `#![no_std]`. The only optional dependency is `alloc`, gated
//! behind the `alloc` feature (used by [`calibration`]). Core scale / pack /
//! dequant computation needs neither `std` nor `alloc`.

#![no_std]
#![deny(missing_docs)]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]

#[cfg(feature = "alloc")]
extern crate alloc;

// Tests are compiled with the std-based harness; expose it to unit tests that
// use `Vec`/`vec!` while the library itself stays `no_std`.
#[cfg(test)]
extern crate std;

pub mod dequant;
pub mod pack;
pub mod scale;
pub mod scheme;

// Frozen-at-1.0 top-level API (mirrors the documented public vocabulary so
// downstream crates can call `tpt_quant_core::quantize_group`, etc. directly).
pub use dequant::{dequantize_group, quantize_group};
pub use pack::{pack_bits, unpack_bits};
pub use scale::{GroupScale, compute_group_scale};

#[cfg(feature = "alloc")]
pub mod calibration;
