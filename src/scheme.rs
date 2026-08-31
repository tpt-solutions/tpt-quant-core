// SPDX-License-Identifier: MIT OR Apache-2.0

//! The stable public vocabulary of the crate.
//!
//! Everything in this module is frozen at 1.0: the names and semantics here are
//! what downstream crates (`tpt-kv-quant`, Project 2's `quantize/awq.rs`) build
//! against. Get it right once.

use core::fmt;

/// A supported quantization bit-width.
///
/// Represented as an enum (not a raw `u8`) as a deliberate stability choice: it
/// makes "which bit-widths does this crate support" an exhaustively-matched,
/// documented set. Adding `Int1` or `Int6` later is a visible, deliberate enum
/// variant addition — never a silent widening of a `u8` that every `match` arm
/// downstream would have to be re-audited for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum BitWidth {
    /// 2-bit signed integers (range `-2..=1`).
    Int2,
    /// 3-bit signed integers (range `-4..=3`).
    Int3,
    /// 4-bit signed integers (range `-8..=7`).
    Int4,
    /// 8-bit signed integers (range `-128..=127`).
    #[default]
    Int8,
}

impl BitWidth {
    /// The number of bits each quantized value occupies.
    #[inline]
    pub const fn bits(self) -> usize {
        match self {
            BitWidth::Int2 => 2,
            BitWidth::Int3 => 3,
            BitWidth::Int4 => 4,
            BitWidth::Int8 => 8,
        }
    }

    /// The number of distinct values representable at this bit-width.
    #[inline]
    pub const fn levels(self) -> usize {
        1usize << self.bits()
    }

    /// The smallest (most negative) representable quantized integer.
    #[inline]
    pub const fn qmin(self) -> i32 {
        -(self.levels() as i32) / 2
    }

    /// The largest representable quantized integer.
    #[inline]
    pub const fn qmax(self) -> i32 {
        (self.levels() as i32) / 2 - 1
    }

    /// The smallest representable value, as an `i8`.
    #[inline]
    pub const fn min_repr(self) -> i8 {
        self.qmin() as i8
    }

    /// The largest representable value, as an `i8`.
    #[inline]
    pub const fn max_repr(self) -> i8 {
        self.qmax() as i8
    }

    /// How many values fit in a single byte at this bit-width.
    #[inline]
    pub const fn values_per_byte(self) -> usize {
        8 / self.bits()
    }
}

/// A complete description of a group-wise quantization scheme.
///
/// This is the frozen public vocabulary: a consumer packages one of these and
/// the matching [`GroupScale`](crate::scale::GroupScale) together with the raw
/// bytes, and the other side reconstructs the floats. Keep field names and
/// semantics stable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct QuantScheme {
    /// The bit-width of each quantized value.
    pub bits: BitWidth,
    /// Number of `f32` values sharing one [`GroupScale`](crate::scale::GroupScale).
    ///
    /// A group is the unit of scale computation. Every value in a group is
    /// de/quantized against that group's single scale (and, for asymmetric
    /// schemes, single zero-point).
    pub group_size: usize,
    /// `true` => symmetric quantization (zero-point is always `0`).
    /// `false` => asymmetric quantization (a learned zero-point shifts the range).
    pub symmetric: bool,
}

impl QuantScheme {
    /// The number of bytes required to pack `n` values at this scheme's bit-width.
    ///
    /// This is `ceil(n * bits / 8)`; the final byte is zero-padded.
    #[inline]
    pub const fn packed_len(self, n: usize) -> usize {
        (n * self.bits.bits()).div_ceil(8)
    }
}

impl Default for QuantScheme {
    fn default() -> Self {
        QuantScheme {
            bits: BitWidth::Int8,
            group_size: 1,
            symmetric: true,
        }
    }
}

/// Shared error type for pack / unpack / quantize / dequantize.
///
/// All fallible functions in this crate return this type rather than panicking,
/// so callers can recover from malformed buffers or out-of-range data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuantError {
    /// An output buffer was too small for the operation.
    BufferTooSmall {
        /// Number of bytes (or values) that were required.
        needed: usize,
        /// Number of bytes (or values) that were actually provided.
        got: usize,
    },
    /// An input value was outside the representable range for the bit-width.
    ValueOutOfRange {
        /// The offending value.
        value: i32,
        /// The inclusive minimum representable value.
        min: i32,
        /// The inclusive maximum representable value.
        max: i32,
    },
    /// A packed buffer did not contain enough bits to fill the requested output.
    InsufficientPackedData {
        /// Number of values requested.
        needed: usize,
        /// Number of values that could be decoded from the buffer.
        available: usize,
    },
    /// A `group_size` of zero was supplied, which is meaningless.
    ZeroGroupSize,
    /// An empty input was supplied where at least one value was required.
    EmptyInput,
}

impl fmt::Display for QuantError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QuantError::BufferTooSmall { needed, got } => {
                write!(f, "output buffer too small: needed {needed}, got {got}")
            }
            QuantError::ValueOutOfRange { value, min, max } => {
                write!(f, "value {value} out of representable range [{min}, {max}]")
            }
            QuantError::InsufficientPackedData { needed, available } => {
                write!(
                    f,
                    "packed buffer too short: need {needed} values, have {available}"
                )
            }
            QuantError::ZeroGroupSize => write!(f, "group_size must be greater than zero"),
            QuantError::EmptyInput => write!(f, "input must contain at least one value"),
        }
    }
}

#[cfg(feature = "alloc")]
impl core::error::Error for QuantError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitwidth_bounds_are_symmetric_twos_complement() {
        assert_eq!(BitWidth::Int2.qmin(), -2);
        assert_eq!(BitWidth::Int2.qmax(), 1);
        assert_eq!(BitWidth::Int3.qmin(), -4);
        assert_eq!(BitWidth::Int3.qmax(), 3);
        assert_eq!(BitWidth::Int4.qmin(), -8);
        assert_eq!(BitWidth::Int4.qmax(), 7);
        assert_eq!(BitWidth::Int8.qmin(), -128);
        assert_eq!(BitWidth::Int8.qmax(), 127);
    }

    #[test]
    fn levels_equal_two_to_the_bits() {
        assert_eq!(BitWidth::Int2.levels(), 4);
        assert_eq!(BitWidth::Int3.levels(), 8);
        assert_eq!(BitWidth::Int4.levels(), 16);
        assert_eq!(BitWidth::Int8.levels(), 256);
    }

    #[test]
    fn derived_traits_present() {
        let a = BitWidth::Int4;
        let b = a;
        assert_eq!(a, b);
        let c = BitWidth::Int8;
        assert_ne!(a, c);
        assert!(matches!(BitWidth::default(), BitWidth::Int8));

        let s = QuantScheme::default();
        assert!(matches!(s.bits, BitWidth::Int8));
        assert_eq!(s.group_size, 1);
        assert!(s.symmetric);
    }

    #[test]
    fn packed_len_formula() {
        // Int3: 3 values -> 9 bits -> 2 bytes.
        assert_eq!(
            QuantScheme {
                bits: BitWidth::Int3,
                group_size: 3,
                symmetric: true
            }
            .packed_len(3),
            2
        );
        // Int8: N values -> N bytes.
        assert_eq!(
            QuantScheme {
                bits: BitWidth::Int8,
                group_size: 1,
                symmetric: true
            }
            .packed_len(5),
            5
        );
        // Int4: 1 value -> 1 byte.
        assert_eq!(
            QuantScheme {
                bits: BitWidth::Int4,
                group_size: 1,
                symmetric: true
            }
            .packed_len(1),
            1
        );
    }
}
