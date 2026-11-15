//! Modest Dark, after Tim Cole's Zed theme (github.com/timcole/modest-dark).
//! `rgb()` is not `const` in gpui, so these stay plain `u32` and call sites
//! wrap them.

pub(crate) const BG: u32 = 0x0f1219;
/// Panels raised off the window: a shade below `BG`, as the theme's own
/// panels match the window and would not read apart.
pub(crate) const PANEL: u32 = 0x0a0d12;
/// Quiet fills and hairlines.
pub(crate) const SURFACE: u32 = 0x1e242e;
/// `SURFACE` at half strength, for hover. Carries alpha, for `rgba()`.
pub(crate) const WASH: u32 = 0x1e242e80;
/// Behind a map label: `BG` just short of opaque, so the text reads over
/// land, lines and night alike. Carries alpha, for `rgba()`.
pub(crate) const CHIP: u32 = 0x0f1219eb;
/// Shade for the night side, with alpha, for `rgba()`.
pub(crate) const NIGHT: u32 = 0x05070a73;
/// Opaque, so an edge two zones share and both draw reads as one line.
pub(crate) const ZONE_LINE: u32 = 0x2c313a;
/// Edges and marks a step stronger than `SURFACE`.
pub(crate) const SURFACE_STRONG: u32 = 0x3e4452;
/// Text that steps back; the dimmest tier that clears 4.5:1 on `BG`.
pub(crate) const TEXT_FAINT: u32 = 0x828997;
pub(crate) const TEXT_MUTED: u32 = 0xabb2bf;
pub(crate) const TEXT: u32 = 0xd7dae0;
/// A time pinned by typing it into a row.
pub(crate) const YELLOW: u32 = 0xebc275;
/// The pin banner's fill and edge, with alpha, for `rgba()`.
pub(crate) const YELLOW_TINT: u32 = 0xebc27514;
pub(crate) const YELLOW_EDGE: u32 = 0xebc27540;
/// A time pinned by scrubbing the map: the yellow's three roles, in purple.
pub(crate) const PURPLE: u32 = 0xca72e4;
pub(crate) const PURPLE_TINT: u32 = 0xca72e414;
pub(crate) const PURPLE_EDGE: u32 = 0xca72e440;
/// Active or selected.
pub(crate) const BLUE: u32 = 0x5ab0f6;
/// The real time while another is pinned: `BLUE` at 60%, for `rgba()`.
pub(crate) const NOW_LINE: u32 = 0x5ab0f699;
/// Destructive actions.
pub(crate) const RED: u32 = 0xef5f6b;
