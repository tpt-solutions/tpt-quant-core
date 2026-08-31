// SPDX-License-Identifier: MIT OR Apache-2.0

//! Bit-packing primitives for the supported [`BitWidth`]s.
//!
//! [`pack_bits`] squeezes a slice of `i8` quantized values into as few bytes as
//! possible; [`unpack_bits`] reverses it. Values are written least-significant
//! bit first and the values are concatenated in order, so the first input value
//! occupies the low bits of byte 0. The final byte is zero-padded.
//!
//! These functions never panic: any size/range problem is reported via
//! [`QuantError`].

use crate::scheme::{BitWidth, QuantError};

/// Pack `values` (each within `bits`'s representable range) into `out`.
///
/// `out.len()` must be at least `ceil(values.len() * bits / 8)`, otherwise
/// [`QuantError::BufferTooSmall`] is returned. Every value must be in
/// `[bits.qmin(), bits.qmax()]`; otherwise [`QuantError::ValueOutOfRange`] is
/// returned.
///
/// # Errors
///
/// Returns [`QuantError::BufferTooSmall`] when `out` is too small, or
/// [`QuantError::ValueOutOfRange`] when a value is outside the representable
/// range for `bits`.
pub fn pack_bits(values: &[i8], bits: BitWidth, out: &mut [u8]) -> Result<(), QuantError> {
    let nbits = bits.bits();
    let need = (values.len() * nbits).div_ceil(8);
    if out.len() < need {
        return Err(QuantError::BufferTooSmall {
            needed: need,
            got: out.len(),
        });
    }

    let qmin = bits.qmin();
    let qmax = bits.qmax();
    let mask: u32 = (1u32 << nbits) - 1;

    let mut acc: u32 = 0;
    let mut acc_bits: u32 = 0;
    let mut byte_idx = 0usize;

    for &v in values {
        let v = i32::from(v);
        if v < qmin || v > qmax {
            return Err(QuantError::ValueOutOfRange {
                value: v,
                min: qmin,
                max: qmax,
            });
        }
        acc |= ((v as u32) & mask) << acc_bits;
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

/// Unpack `out.len()` values from `packed` at the given `bits` width.
///
/// At most `floor(packed.len() * 8 / bits)` values can be decoded; requesting
/// more than that returns [`QuantError::InsufficientPackedData`]. Values are
/// sign-extended back to their signed `i8` form.
///
/// This is the inverse of [`pack_bits`] for a matching value count: packing `n`
/// values and then unpacking `n` values recovers them exactly.
///
/// # Errors
///
/// Returns [`QuantError::InsufficientPackedData`] when `out` is longer than the
/// number of values that `packed` can hold at `bits`.
pub fn unpack_bits(packed: &[u8], bits: BitWidth, out: &mut [i8]) -> Result<(), QuantError> {
    let nbits = bits.bits();
    let capacity = (packed.len() * 8) / nbits;
    if out.len() > capacity {
        return Err(QuantError::InsufficientPackedData {
            needed: out.len(),
            available: capacity,
        });
    }

    let mask: u32 = (1u32 << nbits) - 1;
    let sign_bit: u32 = 1u32 << (nbits - 1);

    let mut acc: u32 = 0;
    let mut acc_bits: u32 = 0;
    let mut packed_idx = 0usize;

    for slot in out.iter_mut() {
        while acc_bits < nbits as u32 {
            let byte = if packed_idx < packed.len() {
                packed[packed_idx]
            } else {
                0
            };
            acc |= (byte as u32) << acc_bits;
            packed_idx += 1;
            acc_bits += 8;
        }
        let mut v = acc & mask;
        acc >>= nbits;
        acc_bits -= nbits as u32;
        if v & sign_bit != 0 {
            v = v.wrapping_sub(1u32 << nbits);
        }
        *slot = v as i8;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec;

    fn roundtrip(values: &[i8], bits: BitWidth) {
        let need = (values.len() * bits.bits()).div_ceil(8);
        let mut packed = vec![0u8; need];
        pack_bits(values, bits, &mut packed).unwrap();
        let mut out = vec![0i8; values.len()];
        unpack_bits(&packed, bits, &mut out).unwrap();
        assert_eq!(out, values);
    }

    #[test]
    fn roundtrip_int8() {
        roundtrip(&[-128, -1, 0, 1, 127, 42], BitWidth::Int8);
    }

    #[test]
    fn roundtrip_int4_odd_length() {
        roundtrip(&[-8, 7, -1, 0, 3], BitWidth::Int4);
    }

    #[test]
    fn roundtrip_int3_negatives() {
        roundtrip(&[-4, 3, -4, 0, 3, -2], BitWidth::Int3);
    }

    #[test]
    fn roundtrip_int2() {
        roundtrip(&[-2, 1, -2, 1, 0], BitWidth::Int2);
    }

    #[test]
    fn empty_roundtrip() {
        let mut packed = [0u8; 0];
        pack_bits(&[], BitWidth::Int4, &mut packed).unwrap();
        let mut out = [0i8; 0];
        unpack_bits(&[], BitWidth::Int4, &mut out).unwrap();
    }

    #[test]
    fn out_of_range_rejected() {
        let mut out = [0u8; 1];
        assert!(matches!(
            pack_bits(&[8], BitWidth::Int4, &mut out),
            Err(QuantError::ValueOutOfRange { .. })
        ));
    }

    #[test]
    fn too_small_buffer_rejected() {
        let mut out = [0u8; 0];
        assert_eq!(
            pack_bits(&[1, 2], BitWidth::Int8, &mut out),
            Err(QuantError::BufferTooSmall { needed: 2, got: 0 })
        );
    }

    #[test]
    fn too_many_requested_rejected() {
        // One byte holds two Int4 values; requesting three must fail.
        let packed = [0u8; 1];
        let mut out = [0i8; 3];
        assert!(matches!(
            unpack_bits(&packed, BitWidth::Int4, &mut out),
            Err(QuantError::InsufficientPackedData { .. })
        ));
    }

    #[test]
    fn packed_layout_least_significant_first() {
        // Int4 values [0b0001, 0b0010] pack as 0b0010_0001 = 0x21.
        let mut packed = [0u8; 1];
        pack_bits(&[0b0001, 0b0010], BitWidth::Int4, &mut packed).unwrap();
        assert_eq!(packed[0], 0b0010_0001);
    }
}
