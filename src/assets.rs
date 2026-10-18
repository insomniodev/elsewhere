//! The icons, baked into the binary. gpui resolves an `svg()` path through
//! the app's `AssetSource`.

use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(match path {
            "icons/trash.svg" => Some(Cow::Borrowed(include_bytes!("../assets/icons/trash.svg"))),
            "icons/milestone.svg" => Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/milestone.svg"
            ))),
            "icons/clock-12.svg" => Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/clock-12.svg"
            ))),
            "icons/map.svg" => Some(Cow::Borrowed(include_bytes!("../assets/icons/map.svg"))),
            "icons/rows.svg" => Some(Cow::Borrowed(include_bytes!("../assets/icons/rows.svg"))),
            "icons/columns.svg" => {
                Some(Cow::Borrowed(include_bytes!("../assets/icons/columns.svg")))
            }
            "icons/x.svg" => Some(Cow::Borrowed(include_bytes!("../assets/icons/x.svg"))),
            "icons/minus.svg" => Some(Cow::Borrowed(include_bytes!("../assets/icons/minus.svg"))),
            "icons/maximize-2.svg" => Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/maximize-2.svg"
            ))),
            "icons/minimize-2.svg" => Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/minimize-2.svg"
            ))),
            _ => None,
        })
    }

    fn list(&self, _path: &str) -> Result<Vec<SharedString>> {
        Ok(Vec::new())
    }
}
