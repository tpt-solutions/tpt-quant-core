// SPDX-License-Identifier: MIT OR Apache-2.0

//! Property tests: pack→unpack round-trips exactly, quantize→dequantize stays
//! within the theoretical error bound, and scale computation is deterministic and
//! non-negative — for random inputs across every `(bits, group_size, symmetric)`
//! combination.

use proptest::collection::vec;
use proptest::prelude::*;
use tpt_quant_core::pack::{pack_bits, unpack_bits};
use tpt_quant_core::scale::compute_group_scale;
use tpt_quant_core::scheme::{BitWidth, QuantError, QuantScheme};
use tpt_quant_core::{dequantize_group, quantize_group};

fn all_bits() -> impl Strategy<Value = BitWidth> {
    prop_oneof![
        Just(BitWidth::Int2),
        Just(BitWidth::Int3),
        Just(BitWidth::Int4),
        Just(BitWidth::Int8),
    ]
}

fn scheme_strategy() -> impl Strategy<Value = QuantScheme> {
    (all_bits(), 1usize..=64, prop::bool::ANY).prop_map(|(bits, group_size, symmetric)| {
        QuantScheme {
            bits,
            group_size,
            symmetric,
        }
    })
}

/// Finite (non-NaN, non-inf) `f32` values, bounded so that `max - min` cannot
/// overflow to `inf` in `compute_group_scale` (which would make the bound-check
/// `clamp(lo, hi)` misbehave). Magnitudes stay far below `f32::MAX`.
fn finite_f32() -> impl Strategy<Value = f32> {
    any::<i32>().prop_map(|i| (i as f32) / 1000.0)
}

/// Clamp an arbitrary `i8` into the representable range for `bits`.
fn clamp_to_range(v: i8, bits: BitWidth) -> i8 {
    v.clamp(bits.min_repr(), bits.max_repr())
}

fn quant_err_to_test(e: QuantError) -> TestCaseError {
    TestCaseError::reject(format!("{e:?}"))
}

proptest! {
    /// pack → unpack recovers the exact input for any in-range values.
    #[test]
    fn pack_unpack_roundtrip(values in vec(any::<i8>(), 0..300), bits in all_bits()) {
        let values: Vec<i8> = values.iter().map(|&v| clamp_to_range(v, bits)).collect();
        let need = (values.len() * bits.bits()).div_ceil(8);
        let mut packed = vec![0u8; need];
        pack_bits(&values, bits, &mut packed).map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
        let mut out = vec![0i8; values.len()];
        unpack_bits(&packed, bits, &mut out).map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
        prop_assert_eq!(out, values);
    }

    /// Scale computation is deterministic and never negative for random inputs.
    #[test]
    fn scale_is_deterministic_and_nonnegative(
        values in vec(finite_f32(), 1..200),
        scheme in scheme_strategy(),
    ) {
        let a = compute_group_scale(&values, &scheme).map_err(quant_err_to_test)?;
        let b = compute_group_scale(&values, &scheme).map_err(quant_err_to_test)?;
        prop_assert_eq!(a, b, "scale computation must be deterministic");
        prop_assert!(a.scale >= 0.0, "scale must be non-negative");
    }

    /// quantize → dequantize error stays within the theoretical bound:
    /// `|dequant - orig| <= clip_error + 0.5 * scale` (per value), for every
    /// `(bits, group_size, symmetric)` combination.
    #[test]
    fn quantize_dequantize_within_bound(
        values in vec(finite_f32(), 1..200),
        scheme in scheme_strategy(),
    ) {
        let scale = compute_group_scale(&values, &scheme).map_err(quant_err_to_test)?;
        let mut packed = vec![0u8; scheme.packed_len(values.len())];
        quantize_group(&values, &scale, &scheme, &mut packed).map_err(quant_err_to_test)?;
        let mut deq = vec![0.0f32; values.len()];
        dequantize_group(&packed, &scale, &scheme, &mut deq).map_err(quant_err_to_test)?;

        let qmin = scheme.bits.qmin() as f32;
        let qmax = scheme.bits.qmax() as f32;
        let lo = scale.scale * (qmin - scale.zero_point as f32);
        let hi = scale.scale * (qmax - scale.zero_point as f32);

        for (&orig, &got) in values.iter().zip(deq.iter()) {
            let clamped = orig.clamp(lo, hi);
            let clip_err = (orig - clamped).abs();
            // Theoretical bound is `clip_err + 0.5 * scale`; add a small
            // proportional fuzz for f32 rounding at exact half-steps.
            let bound = clip_err + 0.5 * scale.scale + scale.scale.abs() * 1e-3 + 1e-3;
            prop_assert!(
                (orig - got).abs() <= bound,
                "orig {orig} dequant {got} exceeds bound {bound} (scale {})",
                scale.scale
            );
        }
    }
}
