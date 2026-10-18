//! Catppuccin Mocha. `rgb()` is not `const` in gpui, so these stay plain `u32`
//! and call sites wrap them.

pub(crate) const BASE: u32 = 0x1e1e2e;
/// Raised panels: overlays, tooltips, the drag preview, the view switch.
pub(crate) const MANTLE: u32 = 0x181825;
/// Hover washes and hairlines; also the land on the map.
pub(crate) const SURFACE0: u32 = 0x313244;
/// `SURFACE0` at half strength, for a hovered row. Carries its alpha, so call
/// sites wrap it in `rgba()`.
pub(crate) const WASH: u32 = 0x31324480;
/// The map's dark half: Catppuccin crust with alpha, for `rgba()`.
pub(crate) const NIGHT: u32 = 0x11111b99;
/// Opaque, so an edge two zones share and both draw reads as one line.
pub(crate) const ZONE_LINE: u32 = 0x45475a;
/// The scrollbar thumb, the tooltip edge, the map's meridian ticks.
pub(crate) const SURFACE1: u32 = 0x45475a;
/// Text that steps back: the day tag, a toggle that is off.
pub(crate) const OVERLAY0: u32 = 0x6c7086;
pub(crate) const SUBTEXT0: u32 = 0xa6adc8;
pub(crate) const TEXT: u32 = 0xcdd6f4;
/// A time typed into a row is pinned. Also the sun on the map.
pub(crate) const YELLOW: u32 = 0xf9e2af;
/// The pin banner's fill and edge, with alpha, for `rgba()`.
pub(crate) const YELLOW_TINT: u32 = 0xf9e2af14;
pub(crate) const YELLOW_EDGE: u32 = 0xf9e2af40;
/// A time scrubbed to on the map is pinned: the yellow's three roles, in mauve.
pub(crate) const MAUVE: u32 = 0xcba6f7;
pub(crate) const MAUVE_TINT: u32 = 0xcba6f714;
pub(crate) const MAUVE_EDGE: u32 = 0xcba6f740;
/// Active: the shown view, a toggle that is on, a drop target.
pub(crate) const BLUE: u32 = 0x89b4fa;
/// The close button under the pointer.
pub(crate) const RED: u32 = 0xf38ba8;
