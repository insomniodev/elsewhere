//! The app's two palettes and which one is showing. `rgb()` is not `const`
//! in gpui, so colors stay plain `u32` and call sites wrap them.

use std::sync::atomic::{AtomicBool, Ordering};

use gpui::WindowAppearance;

pub(crate) struct Palette {
    pub(crate) bg: u32,
    /// Panels raised off the window.
    pub(crate) panel: u32,
    /// Quiet fills and hairlines.
    pub(crate) surface: u32,
    /// `surface` at half strength, for hover. Carries alpha, for `rgba()`.
    pub(crate) wash: u32,
    /// Behind a map label: `bg` just short of opaque, so the text reads over
    /// land, lines and night alike. Carries alpha, for `rgba()`.
    pub(crate) chip: u32,
    /// Shade for the night side, with alpha, for `rgba()`.
    pub(crate) night: u32,
    /// Opaque, so an edge two zones share and both draw reads as one line.
    pub(crate) zone_line: u32,
    /// Edges and marks a step stronger than `surface`.
    pub(crate) surface_strong: u32,
    /// Text that steps back; the dimmest tier that clears 4.5:1 on `bg`.
    pub(crate) text_faint: u32,
    pub(crate) text_muted: u32,
    pub(crate) text: u32,
    /// A time pinned by typing it into a row.
    pub(crate) yellow: u32,
    /// The pin banner's fill and edge, with alpha, for `rgba()`.
    pub(crate) yellow_tint: u32,
    pub(crate) yellow_edge: u32,
    /// A time pinned by scrubbing the map: the yellow's three roles, in purple.
    pub(crate) purple: u32,
    pub(crate) purple_tint: u32,
    pub(crate) purple_edge: u32,
    /// Active or selected.
    pub(crate) blue: u32,
    /// Selected text: `blue` faded, for `rgba()`.
    pub(crate) selection: u32,
    /// The real time while another is pinned: `blue` faded, for `rgba()`.
    pub(crate) now_line: u32,
    /// Destructive actions.
    pub(crate) red: u32,
    /// The map's sun. Its own role, as a yellow dark enough for text on light
    /// would paint it brown.
    pub(crate) sun: u32,
}

/// Modest Dark, after Tim Cole's Zed theme (github.com/timcole/modest-dark).
pub(crate) const DARK: Palette = Palette {
    bg: 0x0f1219,
    // A shade below `bg`: the theme's own panels match the window and would
    // not read apart.
    panel: 0x0a0d12,
    surface: 0x1e242e,
    wash: 0x1e242e80,
    chip: 0x0f1219eb,
    night: 0x05070a73,
    zone_line: 0x2c313a,
    surface_strong: 0x3e4452,
    text_faint: 0x828997,
    text_muted: 0xabb2bf,
    text: 0xd7dae0,
    yellow: 0xebc275,
    yellow_tint: 0xebc27514,
    yellow_edge: 0xebc27540,
    purple: 0xca72e4,
    purple_tint: 0xca72e414,
    purple_edge: 0xca72e440,
    blue: 0x5ab0f6,
    selection: 0x5ab0f640,
    now_line: 0x5ab0f699,
    red: 0xef5f6b,
    sun: 0xebc275,
};

/// Modest Dark's light counterpart: the same hues, darkened where they are
/// text and with alphas retuned, since tints that read on dark vanish on light.
pub(crate) const LIGHT: Palette = Palette {
    // Off-white rather than white, so `panel` has somewhere to go.
    bg: 0xf4f6f9,
    // Raised reads as lighter here.
    panel: 0xffffff,
    // Darker than a light theme's usual surface, as it is also the map's land.
    surface: 0xdfe4ec,
    // Slate, as `surface` at half strength is invisible on light.
    wash: 0x5a677d17,
    // `panel` rather than `bg`: chips sit over land and night.
    chip: 0xffffffeb,
    // Navy rather than grey, so it reads as night and keeps the land visible.
    night: 0x1f3a8033,
    zone_line: 0xc4ccd8,
    surface_strong: 0xb9c2cf,
    text_faint: 0x5c6472,
    text_muted: 0x3f4755,
    text: 0x1e242e,
    yellow: 0x8c5a00,
    // The bright amber, as tinting with the text color goes muddy.
    yellow_tint: 0xf0b84238,
    yellow_edge: 0xb47a1480,
    purple: 0x9333b8,
    purple_tint: 0xca72e424,
    purple_edge: 0x9333b866,
    blue: 0x1a69bb,
    selection: 0x1a69bb38,
    now_line: 0x1a69bb99,
    red: 0xc22a39,
    sun: 0xe0a020,
};

static LIGHT_ON: AtomicBool = AtomicBool::new(false);

/// The palette showing. A process-wide switch rather than a gpui global, so
/// paint closures and helpers reach it without a context.
pub(crate) fn palette() -> &'static Palette {
    if LIGHT_ON.load(Ordering::Relaxed) {
        &LIGHT
    } else {
        &DARK
    }
}

/// Shows the palette matching the system's light or dark preference. Takes
/// effect on the next frame; the caller refreshes the window.
pub(crate) fn follow(appearance: WindowAppearance) {
    let light = matches!(
        appearance,
        WindowAppearance::Light | WindowAppearance::VibrantLight
    );
    LIGHT_ON.store(light, Ordering::Relaxed);
}
