//! Loope's brand palette expressed as ratatui colors, so the TUI matches the CLI's
//! visual identity (Claude blue, Codex orange, pass/fail green/red).

use ratatui::style::Color;

pub const BRAND: Color = Color::Rgb(28, 155, 240); // Loope / Claude blue
pub const CLAUDE: Color = Color::Rgb(28, 155, 240);
pub const CODEX: Color = Color::Rgb(240, 150, 40);
pub const OPENCODE: Color = Color::Rgb(160, 120, 240);
pub const PASS: Color = Color::Rgb(60, 200, 120);
pub const FAIL: Color = Color::Rgb(230, 90, 90);
pub const DIM: Color = Color::Rgb(140, 140, 140);

/// Accent color for an adapter by its display name (e.g. "Claude").
pub fn adapter_color(name: &str) -> Color {
    match name.to_ascii_lowercase().as_str() {
        "claude" => CLAUDE,
        "codex" => CODEX,
        "opencode" => OPENCODE,
        _ => DIM,
    }
}
