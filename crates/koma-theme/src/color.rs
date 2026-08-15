//! RGBA color with hex (CSS-style) serialization.
//!
//! Themes use CSS-style hex colors (`#RGB`, `#RRGGBB`, `#RRGGBBAA`) because
//! they are user-editable and shareable. Values are stored as RGBA8.

use std::fmt;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::theme::ThemeError;

/// An RGBA8 color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const BLACK: Color = Color {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };
    pub const WHITE: Color = Color {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };

    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Parse `#RGB`, `#RRGGBB`, or `#RRGGBBAA`.
    pub fn from_hex(s: &str) -> Result<Self, ThemeError> {
        let hex = s
            .strip_prefix('#')
            .ok_or_else(|| ThemeError::Invalid(format!("color `{s}` must start with `#`")))?;
        let (r, g, b, a) = match hex.len() {
            3 => (
                nibble(hex, 0)? * 17,
                nibble(hex, 1)? * 17,
                nibble(hex, 2)? * 17,
                255,
            ),
            6 => (byte(hex, 0)?, byte(hex, 2)?, byte(hex, 4)?, 255),
            8 => (byte(hex, 0)?, byte(hex, 2)?, byte(hex, 4)?, byte(hex, 6)?),
            n => {
                return Err(ThemeError::Invalid(format!(
                    "color `{s}` has {n} hex digits (expected 3, 6, or 8)"
                )));
            }
        };
        Ok(Self { r, g, b, a })
    }

    /// Serialize as `#RRGGBB` (opaque) or `#RRGGBBAA` (with alpha).
    pub fn to_hex(self) -> String {
        if self.a == 255 {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
        }
    }

    /// Override the alpha channel.
    pub fn with_alpha(self, a: u8) -> Self {
        Self { a, ..self }
    }
}

impl Default for Color {
    fn default() -> Self {
        Self::BLACK
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl Serialize for Color {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let hex = String::deserialize(d)?;
        Color::from_hex(&hex).map_err(D::Error::custom)
    }
}

fn nibble(hex: &str, i: usize) -> Result<u8, ThemeError> {
    let c = hex.as_bytes()[i] as char;
    c.to_digit(16)
        .map(|d| d as u8)
        .ok_or_else(|| ThemeError::Invalid(format!("color `#{hex}` is not valid hex")))
}

fn byte(hex: &str, i: usize) -> Result<u8, ThemeError> {
    let hi = nibble(hex, i)?;
    let lo = nibble(hex, i + 1)?;
    Ok((hi << 4) | lo)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_hex_lengths() {
        assert_eq!(Color::from_hex("#fff").unwrap(), Color::rgb(255, 255, 255));
        assert_eq!(
            Color::from_hex("#0b0e14").unwrap(),
            Color::rgb(0x0b, 0x0e, 0x14)
        );
        assert_eq!(
            Color::from_hex("#00000080").unwrap(),
            Color::rgba(0, 0, 0, 0x80)
        );
    }

    #[test]
    fn rejects_bad_hex() {
        assert!(Color::from_hex("fff").is_err()); // missing #
        assert!(Color::from_hex("#zzz").is_err());
        assert!(Color::from_hex("#12345").is_err());
    }

    #[test]
    fn round_trips_through_hex() {
        for c in [
            Color::rgb(0x0b, 0x0e, 0x14),
            Color::rgba(1, 2, 3, 0x80),
            Color::WHITE,
        ] {
            assert_eq!(Color::from_hex(&c.to_hex()).unwrap(), c);
        }
    }
}
