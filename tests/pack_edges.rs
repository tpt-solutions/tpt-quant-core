// SPDX-License-Identifier: MIT OR Apache-2.0

//! Exhaustive edge cases for the pack/unpack primitives: odd-length arrays,
//! group sizes that don't divide evenly, the minimum/maximum representable value
//! for every bit-width, and empty inputs.

use tpt_quant_core::pack::{pack_bits, unpack_bits};
use tpt_quant_core::scheme::{BitWidth, QuantError};

#[test]
fn minimum_representable_per_width() {
    for bits in [
        BitWidth::Int2,
        BitWidth::Int3,
        BitWidth::Int4,
        BitWidth::Int8,
    ] {
        let v = [bits.min_repr()];
        let mut packed = [0u8; 1];
        pack_bits(&v, bits, &mut packed).unwrap();
        let mut out = [0i8; 1];
        unpack_bits(&packed, bits, &mut out).unwrap();
        assert_eq!(out[0], bits.min_repr());
    }
}

#[test]
fn maximum_representable_per_width() {
    for bits in [
        BitWidth::Int2,
        BitWidth::Int3,
        BitWidth::Int4,
        BitWidth::Int8,
    ] {
        let v = [bits.max_repr()];
        let mut packed = [0u8; 1];
        pack_bits(&v, bits, &mut packed).unwrap();
        let mut out = [0i8; 1];
        unpack_bits(&packed, bits, &mut out).unwrap();
        assert_eq!(out[0], bits.max_repr());
    }
}

#[test]
fn full_range_roundtrip_int2() {
    // Every Int2 value, multiple times, plus a trailing partial group.
    roundtrip_full_range(BitWidth::Int2);
}

#[test]
fn full_range_roundtrip_int3() {
    roundtrip_full_range(BitWidth::Int3);
}

#[test]
fn full_range_roundtrip_int4() {
    roundtrip_full_range(BitWidth::Int4);
}

#[test]
fn full_range_roundtrip_int8() {
    roundtrip_full_range(BitWidth::Int8);
}

/// Pack the entire representable range (with repeats) and a trailing partial
/// value, then unpack back, asserting exact recovery.
fn roundtrip_full_range(bits: BitWidth) {
    let mut vals: Vec<i8> = Vec::new();
    for _ in 0..4 {
        for v in bits.min_repr()..=bits.max_repr() {
            vals.push(v);
        }
    }
    // Add an odd/partial tail so the final byte is not fully used.
    vals.push(bits.min_repr());
    vals.push(bits.max_repr());

    let need = (vals.len() * bits.bits()).div_ceil(8);
    let mut packed = vec![0u8; need];
    pack_bits(&vals, bits, &mut packed).unwrap();
    // Trailing unpacked padding bits occupy the HIGH bits of the last byte and
    // must be zero.
    let total_value_bits = vals.len() * bits.bits();
    let used_in_last = total_value_bits - (need - 1) * 8;
    let pad = 8 - used_in_last;
    let pad_mask = if pad == 0 {
        0u8
    } else {
        ((1u8 << pad) - 1) << used_in_last
    };
    assert_eq!(
        packed[need - 1] & pad_mask,
        0,
        "trailing bits must be zero-padded"
    );

    let mut out = vec![0i8; vals.len()];
    unpack_bits(&packed, bits, &mut out).unwrap();
    assert_eq!(out, vals);
}

#[test]
fn odd_length_int4_three_values() {
    // 3 Int4 values = 12 bits = 2 bytes, with 4 zero-padding bits.
    let vals = [-8i8, 0, 7];
    let mut packed = [0u8; 2];
    pack_bits(&vals, BitWidth::Int4, &mut packed).unwrap();
    let mut out = [0i8; 3];
    unpack_bits(&packed, BitWidth::Int4, &mut out).unwrap();
    assert_eq!(out, vals);
}

#[test]
fn group_size_not_dividing_evenly_is_just_a_count() {
    // pack/unpack only care about value counts, not group_size; a group of 5
    // Int3 values packs into 2 bytes (15 bits) and round-trips for all 5.
    let vals = [-4i8, 3, -1, 0, 2];
    let mut packed = [0u8; 2];
    pack_bits(&vals, BitWidth::Int3, &mut packed).unwrap();
    let mut out = [0i8; 5];
    unpack_bits(&packed, BitWidth::Int3, &mut out).unwrap();
    assert_eq!(out, vals);
}

#[test]
fn unpack_rejects_overlong_request() {
    // 2 bytes of Int3 holds 5 values; requesting 6 must error.
    let packed = [0u8; 2];
    let mut out = [0i8; 6];
    assert_eq!(
        unpack_bits(&packed, BitWidth::Int3, &mut out),
        Err(QuantError::InsufficientPackedData {
            needed: 6,
            available: 5
        })
    );
}

#[test]
fn pack_rejects_undersized_buffer() {
    // 2 Int8 values need 2 bytes.
    let mut out = [0u8; 1];
    assert_eq!(
        pack_bits(&[1, 2], BitWidth::Int8, &mut out),
        Err(QuantError::BufferTooSmall { needed: 2, got: 1 })
    );
}

#[test]
fn pack_rejects_out_of_range() {
    assert_eq!(
        pack_bits(&[8], BitWidth::Int4, &mut [0u8; 1]),
        Err(QuantError::ValueOutOfRange {
            value: 8,
            min: -8,
            max: 7
        })
    );
    assert_eq!(
        pack_bits(&[-9], BitWidth::Int4, &mut [0u8; 1]),
        Err(QuantError::ValueOutOfRange {
            value: -9,
            min: -8,
            max: 7
        })
    );
}

#[test]
fn empty_input_roundtrips() {
    let mut packed = [0u8; 0];
    pack_bits(&[], BitWidth::Int4, &mut packed).unwrap();
    let mut out = [0i8; 0];
    unpack_bits(&packed, BitWidth::Int4, &mut out).unwrap();
}

#[test]
fn int8_is_byte_identity() {
    let vals: Vec<i8> = (-128..=127).collect();
    let mut packed = vec![0u8; vals.len()];
    pack_bits(&vals, BitWidth::Int8, &mut packed).unwrap();
    assert_eq!(packed, vals_as_bytes(&vals));
    let mut out = vec![0i8; vals.len()];
    unpack_bits(&packed, BitWidth::Int8, &mut out).unwrap();
    assert_eq!(out, vals);
}

fn vals_as_bytes(vals: &[i8]) -> Vec<u8> {
    vals.iter().map(|&v| v as u8).collect()
}
