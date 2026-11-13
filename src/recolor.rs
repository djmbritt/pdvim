//! Recolouring rendered pages, for reading on a dark background.
//!
//! A page is rendered black on white. Recolouring maps white to one chosen
//! colour and black to another, and everything between proportionally, one
//! channel at a time.

use std::str::FromStr;

/// A colour as red, green and blue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgb(pub u8, pub u8, pub u8);

/// A colour was not written as `#rrggbb`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("`{0}` is not a colour: write it as #rrggbb")]
pub struct InvalidColour(String);

impl FromStr for Rgb {
    type Err = InvalidColour;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let invalid = || InvalidColour(text.to_owned());
        let digits = text.strip_prefix('#').ok_or_else(invalid)?;
        if digits.len() != 6 || !digits.is_ascii() {
            return Err(invalid());
        }
        let value = u32::from_str_radix(digits, 16).map_err(|_| invalid())?;
        let [_, red, green, blue] = value.to_be_bytes();
        Ok(Self(red, green, blue))
    }
}

/// The two colours a recoloured page is drawn in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Palette {
    /// What white paper becomes.
    pub paper: Rgb,
    /// What black ink becomes.
    pub ink: Rgb,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            paper: Rgb(0x1a, 0x1a, 0x1a),
            ink: Rgb(0xdd, 0xdd, 0xdd),
        }
    }
}

impl Palette {
    /// The recoloured value of a pixel.
    pub fn apply(self, pixel: Rgb) -> Rgb {
        Rgb(
            blend(self.ink.0, self.paper.0, pixel.0),
            blend(self.ink.1, self.paper.1, pixel.1),
            blend(self.ink.2, self.paper.2, pixel.2),
        )
    }
}

/// The value a fraction `position / 255` of the way from `ink` to `paper`.
fn blend(ink: u8, paper: u8, position: u8) -> u8 {
    let ink = u32::from(ink);
    let paper = u32::from(paper);
    let position = u32::from(position);
    let inverse = 255_u32.saturating_sub(position);
    // Each product is at most 255 * 255, so neither the sum nor the rounding
    // term can overflow, and the quotient is at most 255.
    let weighted = ink
        .saturating_mul(inverse)
        .saturating_add(paper.saturating_mul(position))
        .saturating_add(127);
    u8::try_from(weighted.checked_div(255).unwrap_or(0)).unwrap_or(u8::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_parse_from_hex() {
        assert_eq!("#1a2B3c".parse(), Ok(Rgb(0x1a, 0x2b, 0x3c)));
        assert_eq!("#000000".parse(), Ok(Rgb(0, 0, 0)));
        assert!("1a2b3c".parse::<Rgb>().is_err());
        assert!("#1a2b3".parse::<Rgb>().is_err());
        assert!("#1a2b3cc".parse::<Rgb>().is_err());
        assert!("#gggggg".parse::<Rgb>().is_err());
        assert!("#ééé".parse::<Rgb>().is_err());
    }

    #[test]
    fn white_becomes_paper_and_black_becomes_ink() {
        let palette = Palette {
            paper: Rgb(10, 20, 30),
            ink: Rgb(200, 210, 220),
        };
        assert_eq!(palette.apply(Rgb(255, 255, 255)), palette.paper);
        assert_eq!(palette.apply(Rgb(0, 0, 0)), palette.ink);
    }

    #[test]
    fn greys_land_in_between() {
        let palette = Palette {
            paper: Rgb(0, 0, 0),
            ink: Rgb(255, 255, 255),
        };
        // An inverting palette turns each value into its complement.
        assert_eq!(palette.apply(Rgb(64, 128, 255)), Rgb(191, 127, 0));
    }
}
