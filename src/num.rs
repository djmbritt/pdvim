//! Numeric conversions the standard library has no lossless form for.
//!
//! The project forbids bare `as` casts. These few conversions cannot be
//! written any other way, so they live here, each with its behaviour stated.

/// Converts a count to floating point.
///
/// Exact up to 2^24, far beyond any page count or pixel size handled here.
#[expect(
    clippy::as_conversions,
    clippy::cast_precision_loss,
    reason = "no lossless std conversion from u32 to f32 exists"
)]
pub const fn float(value: u32) -> f32 {
    value as f32
}

/// Rounds to the nearest whole number. Negative values and values that are
/// not a number become zero, and values beyond the range of `u32` become its
/// upper limit.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a float-to-integer `as` cast saturates, which is the intent"
)]
pub const fn round(value: f32) -> u32 {
    value.round() as u32
}

/// The size of a collection as a `u32`, capped at its upper limit.
pub fn len(length: usize) -> u32 {
    u32::try_from(length).unwrap_or(u32::MAX)
}

/// A `u32` as an index. Saturates on platforms where `usize` is narrower.
pub fn index(value: u32) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounding_saturates_and_tolerates_nan() {
        assert_eq!(round(2.5), 3);
        assert_eq!(round(-4.0), 0);
        assert_eq!(round(f32::INFINITY), u32::MAX);
        assert_eq!(round(f32::NAN), 0);
    }

    #[test]
    fn counts_convert_exactly() {
        assert!((float(1_000_000) - 1_000_000.0).abs() < f32::EPSILON);
        assert_eq!(len(3), 3);
        assert_eq!(index(7), 7);
    }
}
