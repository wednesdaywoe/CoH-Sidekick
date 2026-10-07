//! Runtime theming: one CSS custom-property sheet per theme, switched by setting
//! `data-theme` on `<html>` and persisted in localStorage. Components never name a
//! color — they consume the tier-2 semantic tokens (see assets/tokens.css).
//!
//! The manifest drives the picker. NOTE (M1 honesty): the manifest and the theme
//! stylesheets are compiled in via `include_str!`/`asset!`, so ADDING a theme still
//! needs a `dx build` — the no-rebuild goal is met at the CSS architecture level
//! (themes are data files, components are token-only) and the loader becomes fully
//! runtime in a later milestone.

use dioxus::prelude::*;
use serde::Deserialize;
use std::sync::LazyLock;

pub const MANIFEST_JSON: &str = include_str!("../assets/themes/manifest.json");

#[derive(Debug, Clone, Deserialize)]
pub struct ThemeManifest {
    pub default: String,
    pub themes: Vec<ThemeEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ThemeEntry {
    pub id: String,
    pub label: String,
    pub tagline: String,
    pub swatch: Swatch,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Swatch {
    #[allow(dead_code)] // preview chips land with the full theme picker
    pub base: String,
    #[allow(dead_code)]
    pub panel: String,
    pub accent: String,
}

/// Parsed once. `Err` instead of a panic: `manifest()` is reachable from render, and a
/// WASM panic aborts the whole app — the theme switcher turns the `Err` into a visible
/// error node instead (Rule 1's UI edge).
static MANIFEST: LazyLock<Result<ThemeManifest, String>> = LazyLock::new(|| {
    serde_json::from_str(MANIFEST_JSON).map_err(|e| format!("themes/manifest.json: {e}"))
});

pub fn manifest() -> &'static Result<ThemeManifest, String> {
    &MANIFEST
}

/// Set `data-theme` and persist the choice. Works identically in the webview and
/// the browser (both go through the document's JS context).
pub fn apply_theme(id: &str) {
    // Split in two because the halves belong to different windows: `data-theme` is this
    // document's, and the save is the owning window's (see [`crate::storage`]).
    document::eval(&format!("document.documentElement.dataset.theme = {id:?};"));
    crate::storage::commit(format!(
        "try {{ localStorage.setItem('sk-theme', {id:?}); }} catch (_) {{}}"
    ));
}

/// On startup: persisted theme if valid, else the manifest default. A manifest that
/// doesn't parse leaves the default stylesheet untouched — the switcher renders the
/// error, so the failure is visible without a startup abort. A save under a theme's old
/// id is carried to its new one ('weld' became 'astoria').
pub fn apply_saved_theme() {
    let Ok(m) = manifest() else { return };
    let ids: Vec<String> = m.themes.iter().map(|t| t.id.clone()).collect();
    let js = format!(
        "const valid = {ids:?};\
         let t = null;\
         try {{ t = localStorage.getItem('sk-theme'); }} catch (_) {{}}\
         if (t === 'weld') t = 'astoria';\
         if (!valid.includes(t)) t = {default:?};\
         document.documentElement.dataset.theme = t;",
        default = m.default,
    );
    document::eval(&js);
}
