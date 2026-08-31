// SPDX-License-Identifier: MIT OR Apache-2.0

//! Fuzz target: arbitrary bytes fed to `pack_bits` / `unpack_bits` must never
//! panic, go out of bounds, or trigger UB — for every `BitWidth`.
//!
//! This mirrors the fuzz-target convention used in `tpt-zero-bytes` and
//! `tpt-local-ai`: a single `libfuzzer`-driven target that hammers the
//! bit-manipulation primitives (where off-by-one / edge-case bugs hide) with
//! fully untrusted input and only asserts "does not crash".
//!
//! Run with `cargo +nightly fuzz run fuzz_pack_unpack` (cargo-fuzz).

#![no_main]

use libfuzzer_sys::fuzz_target;
use tpt_quant_core::pack::{pack_bits, unpack_bits};
use tpt_quant_core::scheme::BitWidth;

fn pick_bits(tag: u8) -> BitWidth {
    match tag % 4 {
        0 => BitWidth::Int2,
        1 => BitWidth::Int3,
        2 => BitWidth::Int4,
        _ => BitWidth::Int8,
    }
}

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }
    let bits = pick_bits(data[0]);

    // Exercise unpacking with an arbitrary packed buffer and an arbitrary
    // requested value count. unpack_bits must either succeed or return
    // `InsufficientPackedData` — never panic / never read out of bounds.
    let want = (data.get(1).copied().unwrap_or(0) as usize) % 4096;
    let mut out = vec![0i8; want];
    let _ = unpack_bits(&data[2..], bits, &mut out);

    // Exercise packing with arbitrary (un-clamped) values. pack_bits must
    // either succeed or return `ValueOutOfRange` / `BufferTooSmall` — never
    // panic. Then a clean round-trip on clamped values must be exact.
    let raw: Vec<i8> = data.iter().map(|&b| b as i8).collect();
    let need = (raw.len() * bits.bits()).div_ceil(8);
    let mut packed = vec![0u8; need];
    let _ = pack_bits(&raw, bits, &mut packed);

    let clamped: Vec<i8> = raw
        .iter()
        .map(|&v| v.clamp(bits.min_repr(), bits.max_repr()))
        .collect();
    let mut packed2 = vec![0u8; need];
    if pack_bits(&clamped, bits, &mut packed2).is_ok() {
        let mut back = vec![0i8; clamped.len()];
        if unpack_bits(&packed2, bits, &mut back).is_ok() {
            // The bit-packing itself is a bijection on the representable grid.
            assert_eq!(back, clamped, "pack/unpack must round-trip exactly");
        }
    }
});
