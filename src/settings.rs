//! View preferences and the window's place, beside the saved list in
//! `key = value` lines so they stay hand-editable. Like the list, garbage
//! degrades to defaults: unknown keys and bad values are skipped.

use gpui::{Pixels, Point, Size, point, px, size};

use crate::app::View;
use crate::clock::Format;
use crate::saved;

const HEADER: &str = "# elsewhere settings, one key = value per line";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Settings {
    pub view: View,
    /// Ordered by UTC offset rather than by hand.
    pub by_clock: bool,
    pub format: Format,
    /// The windowed size, kept while maximized.
    pub size: Option<Size<Pixels>>,
    /// Wayland never tells a window where it is, so there this stays as it
    /// was opened and the compositor places the window.
    pub origin: Option<Point<Pixels>>,
}

/// A coordinate or length, as written; whole pixels read best by hand.
fn number(value: &str) -> Option<f32> {
    value.parse::<f32>().ok().filter(|n| n.is_finite())
}

fn parse(text: &str) -> Settings {
    let mut settings = Settings::default();
    let (mut width, mut height, mut x, mut y) = (None, None, None, None);
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "view" => match value {
                "list" => settings.view = View::List,
                "map" => settings.view = View::Map,
                "grid" => settings.view = View::Grid,
                _ => {}
            },
            "order" => match value {
                "hand" => settings.by_clock = false,
                "clock" => settings.by_clock = true,
                _ => {}
            },
            "format" => match value {
                "24" => settings.format = Format::TwentyFour,
                "12" => settings.format = Format::Twelve,
                _ => {}
            },
            "width" => width = number(value).filter(|n| *n > 0.),
            "height" => height = number(value).filter(|n| *n > 0.),
            "x" => x = number(value),
            "y" => y = number(value),
            _ => {}
        }
    }
    // Half a size or half a place is no use; the other half falls back.
    if let (Some(width), Some(height)) = (width, height) {
        settings.size = Some(size(px(width), px(height)));
    }
    if let (Some(x), Some(y)) = (x, y) {
        settings.origin = Some(point(px(x), px(y)));
    }
    settings
}

fn render(settings: &Settings) -> String {
    let view = match settings.view {
        View::List => "list",
        View::Map => "map",
        View::Grid => "grid",
    };
    let order = if settings.by_clock { "clock" } else { "hand" };
    let format = match settings.format {
        Format::TwentyFour => "24",
        Format::Twelve => "12",
    };
    let mut out = format!("{HEADER}\nview = {view}\norder = {order}\nformat = {format}\n");
    if let Some(size) = settings.size {
        let (width, height) = (
            f32::from(size.width).round(),
            f32::from(size.height).round(),
        );
        out.push_str(&format!("width = {width}\nheight = {height}\n"));
    }
    if let Some(origin) = settings.origin {
        let (x, y) = (f32::from(origin.x).round(), f32::from(origin.y).round());
        out.push_str(&format!("x = {x}\ny = {y}\n"));
    }
    out
}

/// Defaults when there is no file. An error when one exists but cannot be
/// read, in which case the session must not save over it.
pub fn load() -> Result<Settings, String> {
    let Some(path) = saved::config_file("settings") else {
        return Ok(Settings::default());
    };
    Ok(saved::read_text(&path)?
        .map(|text| parse(&text))
        .unwrap_or_default())
}

/// Failures come back for the app to show.
pub fn save(settings: &Settings) -> Result<(), String> {
    match saved::config_file("settings") {
        Some(path) => saved::write_text(&path, &render(settings)),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_key() {
        let text = "view = grid\norder = clock\nformat = 12\n\
                    width = 472\nheight = 590\nx = -1200\ny = 40\n";
        assert_eq!(
            parse(text),
            Settings {
                view: View::Grid,
                by_clock: true,
                format: Format::Twelve,
                size: Some(size(px(472.), px(590.))),
                origin: Some(point(px(-1200.), px(40.))),
            }
        );
    }

    #[test]
    fn an_empty_file_is_the_defaults() {
        assert_eq!(parse(""), Settings::default());
    }

    #[test]
    fn skips_comments_unknown_keys_and_bad_values() {
        let text = "# hand-edited\ntheme = latte\nview = table\nformat=12\n\
                    order\nwidth = wide\nheight = 590\nx = NaN\ny = 3\n";
        assert_eq!(
            parse(text),
            Settings {
                format: Format::Twelve,
                ..Settings::default()
            }
        );
    }

    #[test]
    fn a_size_must_be_positive() {
        assert_eq!(parse("width = 0\nheight = 590\n").size, None);
        assert_eq!(parse("width = 400\nheight = -5\n").size, None);
    }

    #[test]
    fn the_last_of_a_repeated_key_wins() {
        assert_eq!(parse("view = map\nview = list\n").view, View::List);
    }

    #[test]
    fn render_round_trips_through_parse() {
        let settings = Settings {
            view: View::Map,
            by_clock: true,
            format: Format::Twelve,
            size: Some(size(px(472.), px(590.))),
            origin: Some(point(px(100.), px(-20.))),
        };
        assert_eq!(parse(&render(&settings)), settings);
        assert_eq!(parse(&render(&Settings::default())), Settings::default());
    }

    #[test]
    fn renders_whole_pixels() {
        let settings = Settings {
            size: Some(size(px(471.6), px(590.2))),
            ..Settings::default()
        };
        assert!(render(&settings).contains("width = 472\nheight = 590\n"));
    }
}
