//! Asset overlay: dictation-gpui local icons on top of the shared Imago set.
//! `imago-gpui` is a pinned external crate — the dictation mic icon lives
//! under `assets/icons/` here and wins by name; everything else delegates to
//! `imago_gpui::assets::Assets`.
use gpui::{AssetSource, Result, SharedString};
use std::borrow::Cow;

pub struct Assets;

const LOCAL_ICONS: &[(&str, &[u8])] =
    &[("icons/mic.svg", include_bytes!("../assets/icons/mic.svg"))];

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some((_, bytes)) = LOCAL_ICONS.iter().find(|(name, _)| *name == path) {
            return Ok(Some(Cow::Borrowed(*bytes)));
        }
        imago_gpui::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = imago_gpui::assets::Assets.list(path)?;
        paths.extend(
            LOCAL_ICONS
                .iter()
                .filter(|(name, _)| name.starts_with(path))
                .map(|(name, _)| SharedString::new_static(name)),
        );
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}
