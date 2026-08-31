// SPDX-License-Identifier: MIT OR Apache-2.0

//! Group-wise scale / zero-point computation.
//!
//! [`compute_group_scale`] turns a slice of `f32` values (one quantization group)
//! into a [`GroupScale`] describing how to map floats to the integer grid of a
//! given [`QuantScheme`]. It handles both the symmetric and asymmetric cases and
//! a range of degenerate inputs (all-zero groups, single-element groups,
//! `min == max` groups).

use crate::scheme::{QuantError, QuantScheme};

/// The scale and (asymmetric-only) zero-point for one quantization group.
///
/// `scale` and `zero_point` are kept together in one struct even though
/// `zero_point` is unused for symmetric schemes. Keeping it in the struct means
/// callers de/quantize through one uniform API and never have to branch on
/// `symmetric` themselves.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroupScale {
    /// Multiplier mapping the integer grid back to floats: `float ≈ scale * (q - zero_point)`.
    ///
    /// Always non-negative for valid inputs. May be `0.0` for degenerate
    /// (all-equal) groups — see [`compute_group_scale`].
    pub scale: f32,
    /// Offset applied during dequantization.
    ///
    /// `0` for symmetric schemes. For asymmetric schemes it is the rounded
    /// integer that the group minimum maps to.
    pub zero_point: i32,
}

impl GroupScale {
    /// The dequantized value of integer `q` under this scale and its scheme.
    #[inline]
    pub fn dequant(self, q: i32) -> f32 {
        self.scale * (q - self.zero_point) as f32
    }
}

/// Compute the [`GroupScale`] for one group of `values` under `scheme`.
///
/// # Symmetric
///
/// `scale = max(|min|, |max|) / qmax`, `zero_point = 0`. The representable
/// float range is `[-qmax*scale, qmax*scale]`, centered on zero.
///
/// # Asymmetric
///
/// `scale = (max - min) / (qmax - qmin)`, `zero_point = qmin - round(min / scale)`.
/// The representable float range is `[scale*(qmin - zero_point), scale*(qmax - zero_point)]`,
/// which brackets `[min, max]`.
///
/// # Degenerate inputs
///
/// When the group has no spread (`min == max`, including the all-zero group), a
/// `scale` of `0.0` is returned (symmetric) or `1.0` (asymmetric) so that a
/// quantize→dequantize round-trip still reproduces the single value as closely
/// as rounding allows, without dividing by zero.
///
/// # Errors
///
/// Returns [`QuantError::EmptyInput`] if `values` is empty and
/// [`QuantError::ZeroGroupSize`] if `scheme.group_size == 0`.
pub fn compute_group_scale(values: &[f32], scheme: &QuantScheme) -> Result<GroupScale, QuantError> {
    if scheme.group_size == 0 {
        return Err(QuantError::ZeroGroupSize);
    }
    if values.is_empty() {
        return Err(QuantError::EmptyInput);
    }

    let bits = scheme.bits;
    let qmin = bits.qmin();
    let qmax = bits.qmax();

    let mut min = values[0];
    let mut max = values[0];
    for &v in &values[1..] {
        if v < min {
            min = v;
        }
        if v > max {
            max = v;
        }
    }

    if scheme.symmetric {
        let max_abs = max.abs().max(min.abs());
        if max_abs == 0.0 {
            // All-zero group: nothing to represent. scale 0 so dequant(0) == 0.
            return Ok(GroupScale {
                scale: 0.0,
                zero_point: 0,
            });
        }
        let scale = max_abs / qmax as f32;
        Ok(GroupScale {
            scale,
            zero_point: 0,
        })
    } else {
        let span = max - min;
        if span == 0.0 {
            // Degenerate group: pick scale 1 so (value - zero_point) reproduces
            // the constant value under rounding, avoiding division by zero.
            let zero_point = qmin - crate::scale::round_half_to_even(min);
            return Ok(GroupScale {
                scale: 1.0,
                zero_point,
            });
        }
        let scale = span / (qmax - qmin) as f32;
        let zero_point = qmin - crate::scale::round_half_to_even(min / scale);
        Ok(GroupScale { scale, zero_point })
    }
}

/// Floor of `x` implemented via raw IEEE-754 bit manipulation.
///
/// `f32::floor` lives in `std` (it needs `libm` in `no_std`), but this crate has
/// zero external dependencies, so we implement it directly. Correct for all
/// finite, `NaN`, and infinite inputs.
#[inline]
pub(crate) fn floor_f32(x: f32) -> f32 {
    let bits = x.to_bits();
    let sign = bits >> 31;
    let exp = (bits >> 23) & 0xFF;
    if exp == 0xFF {
        return x; // NaN or infinity
    }
    let exp_val = (exp as i32) - 127;
    if exp_val >= 23 {
        return x; // |x| >= 2^23 is already integral
    }
    if exp_val < 0 {
        // |x| < 1: floor is 0 for non-negative, -1 for negative (except 0 itself).
        if x == 0.0 {
            return 0.0;
        }
        return if sign == 1 { -1.0 } else { 0.0 };
    }
    let frac_bits = 23 - exp_val as u32;
    let frac_mask = (1u32 << frac_bits) - 1;
    let mant = bits & 0x7F_FFFF;
    if mant & frac_mask == 0 {
        return x; // already integral
    }
    // Truncate toward zero by clearing the fractional mantissa bits, then step
    // down by one for negatives.
    let truncated = f32::from_bits(bits & !frac_mask);
    if sign == 1 {
        truncated - 1.0
    } else {
        truncated
    }
}

/// Round `x` to the nearest integer using round-half-to-even (banker's rounding).
///
/// This is the single, unambiguous rounding rule the whole crate depends on, so
/// downstream crates can diff their optimized implementations against the oracle
/// bit-exactly. Ties (exact halves, e.g. `2.5`) round to the nearest even
/// integer (`2`), not away from zero.
///
/// Correct for finite `x` within `i32` range; out-of-range values saturate to the
/// `i32` bounds (callers clamp the result to the bit-width's grid anyway).
#[inline]
pub(crate) fn round_half_to_even(x: f32) -> i32 {
    if !x.is_finite() {
        return 0;
    }
    if x >= i32::MAX as f32 {
        return i32::MAX;
    }
    if x <= i32::MIN as f32 {
        return i32::MIN;
    }
    let floor = floor_f32(x);
    let diff = x - floor;
    if diff < 0.5 {
        floor as i32
    } else if diff > 0.5 {
        (floor + 1.0) as i32
    } else {
        // Exact half: round to the nearest even integer.
        let f = floor as i32;
        if f & 1 == 0 { f } else { f + 1 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheme::BitWidth;

    fn scheme(bits: BitWidth, symmetric: bool) -> QuantScheme {
        QuantScheme {
            bits,
            group_size: 1,
            symmetric,
        }
    }

    #[test]
    fn symmetric_scale_is_max_abs_over_qmax() {
        let s = compute_group_scale(&[-1.0, 0.5, 1.0], &scheme(BitWidth::Int8, true)).unwrap();
        assert_eq!(s.zero_point, 0);
        // qmax for Int8 is 127, so scale = 1.0/127.
        assert!((s.scale - 1.0 / 127.0).abs() < 1e-9);
    }

    #[test]
    fn all_zero_group_scale_is_zero_symmetric() {
        let s = compute_group_scale(&[0.0, 0.0, 0.0], &scheme(BitWidth::Int4, true)).unwrap();
        assert_eq!(s.scale, 0.0);
        assert_eq!(s.zero_point, 0);
    }

    #[test]
    fn single_element_group_asymmetric() {
        let s = compute_group_scale(&[3.0], &scheme(BitWidth::Int4, false)).unwrap();
        // span == 0 -> scale 1, zero_point = qmin - round(3).
        assert_eq!(s.scale, 1.0);
        assert_eq!(s.zero_point, BitWidth::Int4.qmin() - 3);
    }

    #[test]
    fn min_equals_max_group_asymmetric_degenerate() {
        let s = compute_group_scale(&[2.5, 2.5, 2.5], &scheme(BitWidth::Int3, false)).unwrap();
        assert_eq!(s.scale, 1.0);
    }

    #[test]
    fn errors_for_empty_and_zero_group() {
        assert_eq!(
            compute_group_scale(&[], &scheme(BitWidth::Int4, true)),
            Err(QuantError::EmptyInput)
        );
        let z = QuantScheme {
            bits: BitWidth::Int4,
            group_size: 0,
            symmetric: true,
        };
        assert_eq!(
            compute_group_scale(&[1.0], &z),
            Err(QuantError::ZeroGroupSize)
        );
    }

    #[test]
    fn floor_f32_known_values() {
        let cases: &[(f32, f32)] = &[
            (1.5, 1.0),
            (-1.5, -2.0),
            (2.5, 2.0),
            (3.7, 3.0),
            (-3.7, -4.0),
            (0.0, 0.0),
            (-0.0, 0.0),
            (2.0, 2.0),
            (0.2, 0.0),
            (-0.2, -1.0),
            (0.999, 0.0),
            (-0.001, -1.0),
            (123456.0, 123456.0),
            (1.0e20, 1.0e20),
        ];
        for &(x, expected) in cases {
            assert_eq!(floor_f32(x), expected, "floor_f32({x})");
        }
    }
}
