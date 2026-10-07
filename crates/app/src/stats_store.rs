//! Dashboard persistence — the panels the user built, what each is called and which stats each
//! holds, kept across reloads on the same `localStorage` document channel as
//! [`crate::layout_store`], [`crate::picker_store`], and [`crate::build_store`].
//!
//! Deliberately NOT part of the build envelope: how you like reading a build is a property of
//! you, not of the character, so it survives switching builds and datasets exactly as the grid
//! layout does.
//!
//! **This is one half of a pair, and it is the authoritative half.** The other is
//! [`crate::layout_store`], which holds each panel's rectangle. Two keys written by two paths
//! will disagree — a panel made in one tab, a layout saved in another — and the resolution is
//! that the roster says which surfaces exist and the layout is reconciled against it on load.
//! Which makes the boot order load-bearing: [`crate::shell`] reads this store BEFORE it measures
//! and loads the grid, because a layout reconciled against an unloaded roster is reconciled
//! against the defaults.
//!
//! Missing or corrupt state keeps the fresh defaults. A stat id naming a row this build no
//! longer has is dropped by the roster's own read path
//! ([`From<StoredDashboards>`](crate::panels::dashboards::Dashboards)) rather than discarding
//! the panel it was on.

use crate::panels::dashboards::Dashboards;
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

pub(crate) const KEY_STATS: &str = "sk-stats-config";

#[derive(Serialize, Deserialize)]
#[serde(tag = "schema_version", content = "data")]
enum StoredStatsConfig {
    /// The dashboard roster. Holds every panel, including one with no stats on it: an empty
    /// panel is something the user deliberately made, and a store that tidied it away would be
    /// deleting a decision.
    ///
    /// There is no V1 arm. V1 held a flat list of visible stat ids — the model where a stat's
    /// panel was decided at compile time — and there is no honest reading of it here: it names
    /// no panels, so any migration would be this module inventing an arrangement and
    /// attributing it to the user. A V1 document fails to parse, [`load`] answers `None`, and
    /// the fresh defaults apply. That costs the population that holds one (1.0 is not live, so:
    /// this repo's own browsers) a single reset of a config that is four clicks to rebuild.
    #[serde(rename = "2")]
    V2(Dashboards),
}

/// The roster as a stored document, or `None` if it will not serialize.
fn encode(dashboards: &Dashboards) -> Option<String> {
    serde_json::to_string(&StoredStatsConfig::V2(dashboards.clone())).ok()
}

/// A stored document as a roster, or `None` for anything this build cannot read as one.
///
/// Split out from [`load`] so the half that decides what a document MEANS is reachable without
/// a DOM: `load` is an `eval` round trip and nothing about it is gradeable, while this is where
/// a version that should be refused could be quietly accepted instead.
fn parse(text: &str) -> Option<Dashboards> {
    let StoredStatsConfig::V2(dashboards) = serde_json::from_str(text).ok()?;
    Some(dashboards)
}

/// Persist the roster (fire-and-forget, like [`crate::picker_store::persist`]).
pub fn persist(dashboards: &Dashboards) {
    let Some(json) = encode(dashboards) else {
        return;
    };
    let js = format!("try {{ localStorage.setItem({KEY_STATS:?}, {json:?}); }} catch (_) {{}}");
    crate::layout_sync::commit_tracked(KEY_STATS, js);
}

/// The persisted roster, or `None` (→ keep the defaults) for missing or unreadable state.
///
/// An EMPTY roster is honest state, not corruption: deleting every panel is a thing the user can
/// do, and answering `None` there would rebuild the default dashboard on the next reload over
/// the top of a decision they made.
pub async fn load() -> Option<Dashboards> {
    let js = format!(
        "try {{ return localStorage.getItem({KEY_STATS:?}); }} catch (_) {{ return null; }}"
    );
    let value = document::eval(&js).await.ok()?;
    parse(value.as_str()?)
}
