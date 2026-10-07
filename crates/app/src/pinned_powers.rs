//! Pinned powers — perma tracking (beta `permaTrackedPowers` + `PinnedPowersBar`).
//!
//! The tracked set is in-memory UI state, exactly as in the beta: it is not part of the
//! build, so it is not persisted and a dataset switch leaves it alone. The strip shows a
//! tracked power's perma progress from the engine's own projection row
//! (`BuildTotals.power_projection`), so the number is the one the Info panel's tracker
//! already shows — never a second calculation.

use crate::panels::stats::BuildTotals;
use crate::shell::Db;
use crate::view::marks;
use dioxus::prelude::*;

/// The powers the player is tracking toward perma, by internal name.
#[derive(Clone, Copy)]
pub struct PermaTracked(pub Signal<Vec<String>>);

/// The Track / Tracking toggle in the Info panel's perma tracker (beta `InfoPanel`'s
/// perma-tracker button).
#[component]
pub fn PermaTrackButton(internal_name: String) -> Element {
    let mut tracked = use_context::<PermaTracked>().0;
    let is_tracked = tracked.read().contains(&internal_name);
    let toggle = move |_| {
        let mut list = tracked.write();
        if let Some(pos) = list.iter().position(|name| name == &internal_name) {
            list.remove(pos);
        } else {
            list.push(internal_name.clone());
        }
    };
    rsx! {
        button {
            class: if is_tracked { "perma-track is-tracked" } else { "perma-track" },
            r#type: "button",
            title: if is_tracked {
                "Stop tracking this power's perma progress"
            } else {
                "Track this power's perma progress on the pinned strip"
            },
            onclick: toggle,
            if is_tracked { "Tracking" } else { "Track" }
        }
    }
}

/// The strip: the tracked powers the build still holds, each with its perma progress and an
/// unpin. A tracked power that has left the build has no projection row, and the beta hides
/// it the same way (filtering through the build's power map) — so an empty intersection
/// renders nothing rather than a stale name.
#[component]
pub fn PinnedPowersStrip(database: Option<Db>) -> Element {
    let tracked = use_context::<PermaTracked>().0;
    let totals = use_context::<BuildTotals>().0;

    let rows: Vec<TrackedRow> = {
        let tracked = tracked();
        if tracked.is_empty() {
            return rsx! {};
        }
        let totals = totals();
        tracked
            .iter()
            .filter_map(|name| {
                totals
                    .power_projection
                    .iter()
                    .find(|projection| &projection.power_internal_name == name)
                    // The readout beside the name: PERMA once the gap is closed, the
                    // projection's own `perma_percent` while it is not, N/A when the row
                    // carries no perma at all.
                    .map(|projection| {
                        // The loop variable is the tracked key — the internal name the set is
                        // stored by. Keep it as `internal` BEFORE resolving the display name,
                        // or unpin compares the wrong string and matches nothing.
                        let internal = name.clone();
                        let (pct, is_perma) = match projection.perma {
                            Some(ref perma) if perma.is_perma => ("PERMA".to_string(), true),
                            Some(ref perma) => (format!("{:.0}%", perma.perma_percent), false),
                            None => ("N/A".to_string(), false),
                        };
                        // Display name and icon from the def, falling back to the internal
                        // name and the unknown icon when the def is unresolvable (Rule 1: a
                        // wrong or broken image is worse than the unknown placeholder).
                        // `resolve_power_def`, not `find_power`: a tracked pool or epic power
                        // lives in a flat partition, and the powerset-only lookup comes back
                        // absent for it (the door the crate grades on).
                        let def = database.as_ref().and_then(|db| {
                            crate::view::power_view::resolve_power_def(
                                db,
                                &projection.power_set,
                                &projection.power_internal_name,
                            )
                        });
                        let display = def
                            .map(|d| d.name.clone())
                            .unwrap_or_else(|| internal.clone());
                        let icon = crate::view::icons::power_icon_url(
                            def.and_then(|d| d.extra.get("icon"))
                                .and_then(|v| v.as_str()),
                        );
                        TrackedRow {
                            internal,
                            name: display,
                            icon,
                            pct,
                            is_perma,
                        }
                    })
            })
            .collect()
    };
    if rows.is_empty() {
        return rsx! {};
    }

    rsx! {
        div { class: "pinned-powers",
            span { class: "pinned-powers__label", "Perma" }
            for row in rows {
                div { class: "pinned-powers__chip", title: "{row.name}",
                    img { class: "pinned-powers__icon", src: "{row.icon}", alt: "{row.name}" }
                    span { class: "pinned-powers__name", "{row.name}" }
                    span {
                        class: if row.is_perma {
                            "pinned-powers__pct is-perma"
                        } else {
                            "pinned-powers__pct"
                        },
                        "{row.pct}"
                    }
                    button {
                        class: "pinned-powers__unpin",
                        r#type: "button",
                        "aria-label": "Stop tracking {row.name}",
                        onclick: move |_| untrack(tracked, &row.internal),
                        {marks::close()}
                    }
                }
            }
        }
    }
}

/// One chip's data, read from the engine's projection row.
struct TrackedRow {
    /// The internal name — the key the tracked set is stored by, so unpin matches it.
    internal: String,
    /// The display name, from the def (falls back to the internal name).
    name: String,
    icon: String,
    pct: String,
    is_perma: bool,
}

/// Drop one power from the tracked set. `name` is the internal key, not the display name —
/// the set is stored by internal name, so a display-name compare matches nothing.
fn untrack(mut tracked: Signal<Vec<String>>, name: &str) {
    tracked.write().retain(|n| n != name);
}
