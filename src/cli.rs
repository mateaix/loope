//! Binary-only terminal presentation: the visual identity, live renderer, and
//! report/diff printing ([`ui`]) plus color-capability detection ([`theme`]).

pub mod theme;
#[cfg(feature = "tui")]
pub mod tui;
pub mod ui;
