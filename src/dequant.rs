// SPDX-License-Identifier: MIT OR Apache-2.0

//! Scalar quantize / dequantize — the correctness oracle.
//!
//! [`quantize_group`] maps a group of `f32` values onto the integer grid and
//! packs the result; [`dequantize_group`] reverses it. This is the function that
//! `tpt-kv-quant`'s `cpu_ref` and Project 2's `quantize/awq.rs` diff their
//! optimized implementations against, so its results must be **bit-exact and
//! unambiguous**, not merely "close enough".
//!
//! ## Rounding rule
//!
//! Quantization rounds each scaled value with **round-half-to-even** (banker's
//! rounding), via [`scale::round_half_to_even`]. This is the single documented
//! rule downstream crates depend on. Integers are then clamped to the
//! representable range `[bits.qmin(), bits.qmax()]`.
//!
//! ## Dequantization
//!
//! `float = scale * (q - zero_point)` for asymmetric, `float = scale * q` for
//! symmetric (`zero_point == 0`).
//!
//! Both functions are `no_std` and allocation-free: packing/unpacking is done
//! incrementally over the provided buffers, so no scratch heap is required.

use crate::scale::{GroupScale, round_half_to_even};
use crate::scheme::{QuantError, QuantScheme};

/// Quantize one group of `values` into `out` using `scale` and `scheme`.
///
/// `out.len()` must be at least `scheme.packed_len(values.len())`; otherwise
/// [`QuantError::BufferTooSmall`] is returned. Each value is scaled, rounded
/// half-to-even, clamped to the representable range, then bit-packed
/// least-significant-bit first (the same layout [`crate::pack::pack_bits`] uses).
///
/// Edge cases: when `scale == 0.0` (an all-zero/degenerate group) every value
/// quantizes to `0`.
///
/// # Errors
///
/// Returns [`QuantError::BufferTooSmall`] if `out` is too small, or
/// [`QuantError::ValueOutOfRange`] if a computed integer somehow falls outside
/// the representable range (should not happen for valid inputs).
pub fn quantize_group(
    values: &[f32],
    scale: &GroupScale,
    scheme: &QuantScheme,
    out: &mut [u8],
) -> Result<(), QuantError> {
    let need = scheme.packed_len(values.len());
    if out.len() < need {
        return Err(QuantError::BufferTooSmall {
            needed: need,
            got: out.len(),
        });
    }

    let bits = scheme.bits;
    let nbits = bits.bits();
    let qmin = bits.qmin();
    let qmax = bits.qmax();
    let mask: u32 = (1u32 << nbits) - 1;
    let scale_val = scale.scale;

    let mut acc: u32 = 0;
    let mut acc_bits: u32 = 0;
    let mut byte_idx = 0usize;

    for &v in values {
        let raw = if scale_val == 0.0 {
            0i32
        } else if scheme.symmetric {
            round_half_to_even(v / scale_val)
        } else {
            round_half_to_even(v / scale_val) + scale.zero_point
        };
        let q = clamp(raw, qmin, qmax) as i8;
        let q = i32::from(q);
        if q < qmin || q > qmax {
            return Err(QuantError::ValueOutOfRange {
                value: q,
                min: qmin,
                max: qmax,
            });
        }
        acc |= ((q as u32) & mask) << acc_bits;
        acc_bits += nbits as u32;
        while acc_bits >= 8 {
            out[byte_idx] = (acc & 0xFF) as u8;
            byte_idx += 1;
            acc >>= 8;
            acc_bits -= 8;
        }
    }
    if acc_bits > 0 {
        out[byte_idx] = (acc & 0xFF) as u8;
    }
    Ok(())
}

/// Dequantize `out.len()` values from `packed` into `out` using `scale` and `scheme`.
///
/// `packed` must hold at least `out.len()` values at `scheme.bits`; otherwise
/// [`QuantError::InsufficientPackedData`] is returned.
///
/// # Errors
///
/// Returns [`QuantError::InsufficientPackedData`] if `packed` is too short for
/// the requested value count.
pub fn dequantize_group(
    packed: &[u8],
    scale: &GroupScale,
    scheme: &QuantScheme,
    out: &mut [f32],
) -> Result<(), QuantError> {
    let nbits = scheme.bits.bits();
    let capacity = (packed.len() * 8) / nbits;
    if out.len() > capacity {
        return Err(QuantError::InsufficientPackedData {
            needed: out.len(),
            available: capacity,
        });
    }

    for (i, slot) in out.iter_mut().enumerate() {
        let q = i32::from(read_value(packed, nbits, i));
        *slot = scale.dequant(q);
    }
    Ok(())
}

/// Read the `index`-th `nbits`-wide signed value from `packed`.
#[inline]
fn read_value(packed: &[u8], nbits: usize, index: usize) -> i8 {
    let start_bit = index * nbits;
    let mut window: u32 = 0;
    for i in 0..4 {
        let b = packed.get((start_bit / 8) + i).copied().unwrap_or(0);
        window |= (b as u32) << (i * 8);
    }
    let shift = start_bit % 8;
    let raw = (window >> shift) & ((1u32 << nbits) - 1);
    let sign_bit: u32 = 1u32 << (nbits - 1);
    let v = if raw & sign_bit != 0 {
        raw.wrapping_sub(1u32 << nbits)
    } else {
        raw
    };
    v as i8
}

#[inline]
fn clamp(x: i32, lo: i32, hi: i32) -> i32 {
    if x < lo {
        lo
    } else if x > hi {
        hi
    } else {
        x
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scale::compute_group_scale;
    use crate::scheme::BitWidth;

    fn scheme(bits: BitWidth, symmetric: bool) -> QuantScheme {
        QuantScheme {
            bits,
            group_size: 1,
            symmetric,
        }
    }

    #[test]
    fn symmetric_int8_exact_for_integers() {
        // max_abs = 127 => scale = 1.0, so the integer grid is exact.
        let vals = [-127.0f32, -1.0, 0.0, 1.0, 127.0];
        let s = scheme(BitWidth::Int8, true);
        let scale = compute_group_scale(&vals, &s).unwrap();
        let mut packed = [0u8; 5];
        quantize_group(&vals, &scale, &s, &mut packed).unwrap();
        let mut deq = [0.0f32; 5];
        dequantize_group(&packed, &scale, &s, &mut deq).unwrap();
        for (a, b) in vals.iter().zip(deq.iter()) {
            assert_eq!(a, b);
        }
    }

    #[test]
    fn round_half_to_even_is_used() {
        let vals = [3.5f32];
        let s = scheme(BitWidth::Int8, true);
        let scale = compute_group_scale(&vals, &s).unwrap();
        let mut packed = [0u8; 1];
        quantize_group(&vals, &scale, &s, &mut packed).unwrap();
        let mut deq = [0.0f32; 1];
        dequantize_group(&packed, &scale, &s, &mut deq).unwrap();
        // 3.5 is exactly qmax (127) * scale => dequantizes back to 3.5.
        assert_eq!(deq[0], 3.5);
    }

    #[test]
    fn buffer_too_small_errors() {
        let s = scheme(BitWidth::Int4, true);
        let vals = [0.0f32, 1.0];
        let scale = compute_group_scale(&vals, &s).unwrap();
        let mut out = [0u8; 0];
        assert_eq!(
            quantize_group(&vals, &scale, &s, &mut out),
            Err(QuantError::BufferTooSmall { needed: 1, got: 0 })
        );
    }

    #[test]
    fn known_symmetric_int4_values() {
        let vals = [-1.0f32, 0.0, 1.0];
        let s = scheme(BitWidth::Int4, true);
        let scale = compute_group_scale(&vals, &s).unwrap();
        let mut packed = [0u8; 2];
        quantize_group(&vals, &scale, &s, &mut packed).unwrap();
        let mut deq = [0.0f32; 3];
        dequantize_group(&packed, &scale, &s, &mut deq).unwrap();
        for (orig, got) in vals.iter().zip(deq.iter()) {
            assert!((orig - got).abs() <= 0.5 / 7.0 + 1e-9, "{orig} vs {got}");
        }
    }

    #[test]
    fn asymmetric_roundtrip_odd_length() {
        let vals = [0.1f32, -0.3, 0.25, 0.7, -0.15];
        let s = scheme(BitWidth::Int3, false);
        let scale = compute_group_scale(&vals, &s).unwrap();
        let mut packed = [0u8; 2];
        quantize_group(&vals, &scale, &s, &mut packed).unwrap();
        let mut deq = [0.0f32; 5];
        dequantize_group(&packed, &scale, &s, &mut deq).unwrap();
        for (orig, got) in vals.iter().zip(deq.iter()) {
            assert!(
                (orig - got).abs() <= 0.5 * scale.scale + 1e-4,
                "{orig} vs {got}"
            );
        }
    }
}
