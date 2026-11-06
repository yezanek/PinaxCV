//! Colour contrast after WCAG 2.2 (issue #139): relative luminance and the
//! contrast ratio, success criteria 1.4.3 (text, 4.5:1) and 1.4.11
//! (graphical objects such as rules, 3:1).

/// WCAG 2.2 SC 1.4.3: normal-size text needs at least this ratio.
pub const TEXT_MIN_RATIO: f64 = 4.5;
/// WCAG 2.2 SC 1.4.11: non-text marks such as rules need at least this ratio.
pub const GRAPHIC_MIN_RATIO: f64 = 3.0;

/// An sRGB colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub const WHITE: Rgb = Rgb(0xFF, 0xFF, 0xFF);

    /// Reads `#RRGGBB` (either case); anything else is `None`.
    pub fn parse(hex: &str) -> Option<Rgb> {
        let digits = hex.strip_prefix('#')?;
        if digits.len() != 6 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let channel = |i: usize| u8::from_str_radix(&digits[i..i + 2], 16).ok();
        Some(Rgb(channel(0)?, channel(2)?, channel(4)?))
    }

    /// `#RRGGBB` in upper case.
    pub fn hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.0, self.1, self.2)
    }

    /// WCAG relative luminance, 0 (black) to 1 (white).
    pub fn luminance(self) -> f64 {
        let linear = |c: u8| {
            let c = f64::from(c) / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(self.0) + 0.7152 * linear(self.1) + 0.0722 * linear(self.2)
    }
}

/// The WCAG contrast ratio of two colours, 1 to 21.
pub fn contrast_ratio(a: Rgb, b: Rgb) -> f64 {
    let (la, lb) = (a.luminance(), b.luminance());
    let (light, dark) = if la >= lb { (la, lb) } else { (lb, la) };
    (light + 0.05) / (dark + 0.05)
}

/// The lightest shade of `colour` (same hue, every channel scaled down by the
/// same factor) with at least `min_ratio` against `background`. Black when
/// nothing lighter passes.
pub fn darker_shade(colour: Rgb, background: Rgb, min_ratio: f64) -> Rgb {
    (0..=100u8)
        .map(|step| {
            let keep = f64::from(100 - step) / 100.0;
            // In 0..=255 after scaling a u8 by a factor in 0..=1.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let scale = |c: u8| (f64::from(c) * keep).round() as u8;
            Rgb(scale(colour.0), scale(colour.1), scale(colour.2))
        })
        .find(|c| contrast_ratio(*c, background) >= min_ratio)
        .unwrap_or(Rgb(0, 0, 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn parses_hex_colours_only() {
        assert_eq!(Rgb::parse("#2b5cAB"), Some(Rgb(0x2B, 0x5C, 0xAB)));
        assert_eq!(Rgb::parse("2B5CAB"), None);
        assert_eq!(Rgb::parse("#2B5CA"), None);
        assert_eq!(Rgb::parse("#2B5CAG"), None);
        assert_eq!(Rgb::parse("#ééé"), None);
        assert_eq!(Rgb(0x2B, 0x5C, 0xAB).hex(), "#2B5CAB");
    }

    #[test]
    fn ratios_match_the_wcag_reference_values() {
        assert!(close(contrast_ratio(Rgb(0, 0, 0), Rgb::WHITE), 21.0));
        assert!(close(contrast_ratio(Rgb::WHITE, Rgb::WHITE), 1.0));
        // #767676 is the classic lightest grey that passes 4.5:1 on white.
        assert!(contrast_ratio(Rgb(0x76, 0x76, 0x76), Rgb::WHITE) >= TEXT_MIN_RATIO);
        assert!(contrast_ratio(Rgb(0x77, 0x77, 0x77), Rgb::WHITE) < TEXT_MIN_RATIO);
        // Gold on white, the issue's example.
        assert!(close(
            contrast_ratio(Rgb(0xFF, 0xD7, 0x00), Rgb::WHITE),
            1.40
        ));
        // Order does not matter.
        let (a, b) = (Rgb(0x2B, 0x5C, 0xAB), Rgb(0x12, 0x12, 0x12));
        assert!(close(contrast_ratio(a, b), contrast_ratio(b, a)));
    }

    #[test]
    fn the_darker_shade_passes_and_keeps_the_hue() {
        let gold = Rgb(0xFF, 0xD7, 0x00);
        let shade = darker_shade(gold, Rgb::WHITE, TEXT_MIN_RATIO);
        assert!(contrast_ratio(shade, Rgb::WHITE) >= TEXT_MIN_RATIO);
        assert!(
            shade.0 > shade.1 && shade.2 == 0,
            "still a yellow-brown: {shade:?}"
        );
        // One step lighter would fail, so it is the nearest passing shade.
        let lighter = Rgb(
            shade.0.saturating_add(3),
            shade.1.saturating_add(3),
            shade.2,
        );
        assert!(contrast_ratio(lighter, Rgb::WHITE) < TEXT_MIN_RATIO + 0.2);
        // A passing colour is its own shade.
        let navy = Rgb(0x1F, 0x3A, 0x68);
        assert_eq!(darker_shade(navy, Rgb::WHITE, TEXT_MIN_RATIO), navy);
        // On black, darker never helps: the answer is black.
        assert_eq!(
            darker_shade(Rgb(0x30, 0x30, 0x30), Rgb(0, 0, 0), TEXT_MIN_RATIO),
            Rgb(0, 0, 0)
        );
    }
}
