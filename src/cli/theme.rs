//! Terminal color capability and brand color emission.
//!
//! A [`ColorLevel`] is resolved once per run from `--color` and the environment, then
//! brand RGB colors are emitted as truecolor, nearest-256, or nothing accordingly.

use std::sync::OnceLock;

/// How much color the terminal can show.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorLevel {
    None,
    Ansi256,
    TrueColor,
}

static LEVEL: OnceLock<ColorLevel> = OnceLock::new();

/// Set the process-wide color level (first call wins).
pub fn set_level(level: ColorLevel) {
    let _ = LEVEL.set(level);
}

/// The active color level (defaults to truecolor if never set).
pub fn level() -> ColorLevel {
    LEVEL.get().copied().unwrap_or(ColorLevel::TrueColor)
}

/// A foreground escape for an RGB brand color at the active level (empty when `None`).
pub fn rgb(r: u8, g: u8, b: u8) -> String {
    match level() {
        ColorLevel::None => String::new(),
        ColorLevel::TrueColor => format!("\x1b[38;2;{r};{g};{b}m"),
        ColorLevel::Ansi256 => format!("\x1b[38;5;{}m", nearest_256(r, g, b)),
    }
}

/// When color is enabled, decide truecolor vs 256 from the environment.
pub fn detect_enabled_level() -> ColorLevel {
    detect_level_from(
        std::env::var("COLORTERM").ok().as_deref(),
        std::env::var("FORCE_COLOR").ok().as_deref(),
    )
}

/// Pure resolver for truecolor vs 256 (testable without touching the environment).
pub fn detect_level_from(colorterm: Option<&str>, force_color: Option<&str>) -> ColorLevel {
    if let Some(force) = force_color {
        match force.trim() {
            "3" => return ColorLevel::TrueColor,
            "2" | "1" => return ColorLevel::Ansi256,
            _ => {}
        }
    }
    match colorterm {
        Some(c) if c.contains("truecolor") || c.contains("24bit") => ColorLevel::TrueColor,
        _ => ColorLevel::Ansi256,
    }
}

/// Map an RGB color to the nearest xterm-256 palette index, considering both the
/// 6×6×6 color cube and the grayscale ramp.
pub fn nearest_256(r: u8, g: u8, b: u8) -> u8 {
    const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    let cube_index = |v: u8| -> usize {
        let mut best = 0;
        let mut best_d = i32::MAX;
        for (i, &l) in LEVELS.iter().enumerate() {
            let d = (l as i32 - v as i32).abs();
            if d < best_d {
                best_d = d;
                best = i;
            }
        }
        best
    };
    let (ri, gi, bi) = (cube_index(r), cube_index(g), cube_index(b));
    let cube_code = 16 + 36 * ri + 6 * gi + bi;

    // Grayscale candidate (indices 232..=255 map to values 8,18,...,238).
    let gray = (r as i32 + g as i32 + b as i32) / 3;
    let gidx = ((gray - 8).clamp(0, 238) / 10).clamp(0, 23);
    let gray_code = 232 + gidx;
    let gray_val = 8 + 10 * gidx;

    let dist = |cr: i32, cg: i32, cb: i32| {
        (cr - r as i32).pow(2) + (cg - g as i32).pow(2) + (cb - b as i32).pow(2)
    };
    let cube_d = dist(LEVELS[ri] as i32, LEVELS[gi] as i32, LEVELS[bi] as i32);
    let gray_d = dist(gray_val, gray_val, gray_val);

    if gray_d < cube_d {
        gray_code as u8
    } else {
        cube_code as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_truecolor_from_colorterm() {
        assert_eq!(
            detect_level_from(Some("truecolor"), None),
            ColorLevel::TrueColor
        );
        assert_eq!(detect_level_from(Some("24bit"), None), ColorLevel::TrueColor);
        assert_eq!(detect_level_from(Some("256"), None), ColorLevel::Ansi256);
        assert_eq!(detect_level_from(None, None), ColorLevel::Ansi256);
    }

    #[test]
    fn force_color_overrides() {
        assert_eq!(detect_level_from(None, Some("3")), ColorLevel::TrueColor);
        assert_eq!(detect_level_from(Some("truecolor"), Some("1")), ColorLevel::Ansi256);
    }

    #[test]
    fn nearest_256_maps_primaries() {
        assert_eq!(nearest_256(0, 0, 0), 16); // cube black
        assert_eq!(nearest_256(255, 255, 255), 231); // cube white
        assert_eq!(nearest_256(255, 0, 0), 196); // pure red
        assert_eq!(nearest_256(0, 255, 0), 46); // pure green
        assert_eq!(nearest_256(0, 0, 255), 21); // pure blue
    }

    #[test]
    fn nearest_256_prefers_grayscale_for_grays() {
        let code = nearest_256(128, 128, 128);
        assert!((232..=255).contains(&code), "expected grayscale, got {code}");
    }

    #[test]
    fn rgb_emits_per_level() {
        // default level is truecolor when unset
        assert!(rgb(28, 155, 240).contains("38;2;28;155;240"));
    }
}
