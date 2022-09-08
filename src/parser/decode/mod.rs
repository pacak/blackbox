#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_possible_truncation)]

mod elias_delta;
mod negative_14_bit;
mod tagged_16;
mod tagged_32;
mod variable;

pub use elias_delta::{elias_delta, elias_delta_signed};
pub use negative_14_bit::negative_14_bit;
pub use tagged_16::tagged_16;
pub use tagged_32::tagged_32;
pub use variable::{variable, variable_signed};

use num_enum::TryFromPrimitive;

#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive)]
#[repr(u8)]
pub enum Encoding {
    /// Signed variable byte
    VariableSigned = 0,
    /// Unsigned variable byte
    Variable = 1,
    /// Unsigned variable byte, but negated after decoding. Value fits in 14 bits
    Negative14Bit = 3,
    EliasDelta = 4,
    EliasDeltaSigned = 5,
    TaggedVariable = 6,
    Tagged32 = 7,
    /// 1 tag byte containing 4 2 bit tags, followed by 4 fields
    ///
    /// | Tag | Field width         |
    /// |-----|---------------------|
    /// | 0   | 0 (field value = 0) |
    /// | 1   | 4                   |
    /// | 2   | 8                   |
    /// | 3   | 16                  |
    Tagged16 = 8,
    /// Nothing is written to the log, assume value is 0
    Null = 9,
    EliasGamma = 10,
    EliasGammaSigned = 11,
}

fn sign_extend(from: u64, bits: u32) -> i64 {
    let unused_bits = 64 - bits;
    (from << unused_bits) as i64 >> unused_bits
}

const fn zig_zag_decode(value: u32) -> i32 {
    (value >> 1) as i32 ^ -(value as i32 & 1)
}

#[cfg(test)]
mod test {
    #[test]
    fn sign_extend() {
        use super::sign_extend;

        assert_eq!(0, sign_extend(0b00, 2));
        assert_eq!(1, sign_extend(0b01, 2));
        assert_eq!(-2, sign_extend(0b10, 2));
        assert_eq!(-1, sign_extend(0b11, 2));
    }

    #[test]
    fn zig_zag_decode() {
        use super::zig_zag_decode;

        assert_eq!(0, zig_zag_decode(0));
        assert_eq!(-1, zig_zag_decode(1));
        assert_eq!(1, zig_zag_decode(2));
        assert_eq!(-2, zig_zag_decode(3));

        assert_eq!(i32::MIN, zig_zag_decode(u32::MAX));
        assert_eq!(i32::MAX, zig_zag_decode(u32::MAX - 1));
    }
}
