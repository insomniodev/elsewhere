//! Catppuccin Mocha. `rgb()` is not `const` in gpui, so these stay plain `u32`
//! and call sites wrap them.

pub(crate) const BASE: u32 = 0x1e1e2e;
/// Panels raised off the window.
pub(crate) const MANTLE: u32 = 0x181825;
/// Quiet fills and hairlines.
pub(crate) const SURFACE0: u32 = 0x313244;
/// `SURFACE0` at half strength, for hover. Carries alpha, for `rgba()`.
pub(crate) const WASH: u32 = 0x31324480;
/// Shade for the night side: Catppuccin crust with alpha, for `rgba()`.
pub(crate) const NIGHT: u32 = 0x11111b99;
/// Opaque, so an edge two zones share and both draw reads as one line.
pub(crate) const ZONE_LINE: u32 = 0x45475a;
/// Edges and marks a step stronger than `SURFACE0`.
pub(crate) const SURFACE1: u32 = 0x45475a;
/// Text that steps back.
pub(crate) const OVERLAY0: u32 = 0x6c7086;
pub(crate) const SUBTEXT0: u32 = 0xa6adc8;
pub(crate) const TEXT: u32 = 0xcdd6f4;
/// A time pinned by typing it into a row.
pub(crate) const YELLOW: u32 = 0xf9e2af;
/// The pin banner's fill and edge, with alpha, for `rgba()`.
pub(crate) const YELLOW_TINT: u32 = 0xf9e2af14;
pub(crate) const YELLOW_EDGE: u32 = 0xf9e2af40;
/// A time pinned by scrubbing the map: the yellow's three roles, in mauve.
pub(crate) const MAUVE: u32 = 0xcba6f7;
pub(crate) const MAUVE_TINT: u32 = 0xcba6f714;
pub(crate) const MAUVE_EDGE: u32 = 0xcba6f740;
/// Active or selected.
pub(crate) const BLUE: u32 = 0x89b4fa;
/// Destructive actions.
pub(crate) const RED: u32 = 0xf38ba8;
