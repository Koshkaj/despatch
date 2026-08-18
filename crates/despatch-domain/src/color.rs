use std::fmt::{Display, Formatter};

use serde::de::{Error as DeserializeError, Unexpected};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ColorViolation {
    #[error("must be a hex color such as #4F9DFF")]
    Malformed,
}

/// A user-chosen label color for a group or a category.
///
/// The value is stored in the shared config as a `#rrggbb` string so the file stays reviewable in
/// Git, and it is kept as three channels here so an invalid color cannot be represented at all.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct LabelColor {
    red: u8,
    green: u8,
    blue: u8,
}

impl LabelColor {
    #[must_use]
    pub const fn from_rgb(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }

    /// Parses `#rgb` or `#rrggbb`, with or without the leading `#`.
    pub fn parse(value: &str) -> Result<Self, ColorViolation> {
        let digits = value.trim().trim_start_matches('#');
        if !digits.chars().all(|digit| digit.is_ascii_hexdigit()) {
            return Err(ColorViolation::Malformed);
        }
        let channels: [u8; 3] = match digits.len() {
            3 => {
                let mut channels = [0_u8; 3];
                for (channel, digit) in channels.iter_mut().zip(digits.chars()) {
                    let value = hex_digit(digit)?;
                    *channel = value * 17;
                }
                channels
            }
            6 => {
                let mut channels = [0_u8; 3];
                let bytes = digits.as_bytes();
                for (index, channel) in channels.iter_mut().enumerate() {
                    let high = hex_digit(char::from(bytes[index * 2]))?;
                    let low = hex_digit(char::from(bytes[index * 2 + 1]))?;
                    *channel = high * 16 + low;
                }
                channels
            }
            _ => return Err(ColorViolation::Malformed),
        };
        Ok(Self::from_rgb(channels[0], channels[1], channels[2]))
    }

    #[must_use]
    pub const fn rgb(self) -> [u8; 3] {
        [self.red, self.green, self.blue]
    }

    /// The `#rrggbb` form written to the config file.
    #[must_use]
    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.red, self.green, self.blue)
    }
}

fn hex_digit(digit: char) -> Result<u8, ColorViolation> {
    digit
        .to_digit(16)
        .and_then(|value| u8::try_from(value).ok())
        .ok_or(ColorViolation::Malformed)
}

impl Display for LabelColor {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.to_hex())
    }
}

impl Serialize for LabelColor {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for LabelColor {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(|_| {
            DeserializeError::invalid_value(Unexpected::Str(&value), &"a hex color such as #4f9dff")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{ColorViolation, LabelColor};

    #[test]
    fn hex_colors_are_accepted_with_and_without_a_leading_hash() {
        let expected = LabelColor::from_rgb(0x4F, 0x9D, 0xFF);

        assert_eq!(LabelColor::parse("#4F9DFF"), Ok(expected));
        assert_eq!(LabelColor::parse("4f9dff"), Ok(expected));
        assert_eq!(LabelColor::parse("  #4f9dff  "), Ok(expected));
    }

    #[test]
    fn the_three_digit_shorthand_expands_each_channel() {
        assert_eq!(
            LabelColor::parse("#f0a"),
            Ok(LabelColor::from_rgb(0xFF, 0x00, 0xAA))
        );
    }

    #[test]
    fn anything_that_is_not_a_hex_color_is_rejected() {
        for value in ["", "#", "#12345", "#gggggg", "rebeccapurple", "#1234567"] {
            assert_eq!(
                LabelColor::parse(value),
                Err(ColorViolation::Malformed),
                "{value} must not parse"
            );
        }
    }

    #[test]
    fn a_color_round_trips_through_its_config_representation() {
        let color = LabelColor::from_rgb(0x30, 0xD1, 0x58);
        let json = serde_json::to_string(&color).expect("serialize");

        assert_eq!(json, "\"#30d158\"");
        assert_eq!(
            serde_json::from_str::<LabelColor>(&json).expect("deserialize"),
            color
        );
        assert!(serde_json::from_str::<LabelColor>("\"nope\"").is_err());
    }
}
