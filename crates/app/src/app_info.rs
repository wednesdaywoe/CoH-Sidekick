//! The app's self-description surfaces — About, Changelog, Welcome and the donate
//! modal (the beta's `AboutModal` / `ChangelogModal` / `WelcomeModal` /
//! `DonateModal`), on the shared [`Modal`].
//!
//! All four describe the REBUILD, and the content is written against this code
//! rather than ported from the beta: the beta's About is the original author's
//! personal note, its changelog is the beta's own git history, and pasting
//! either in would describe a different program. The changelog is two lists: the
//! dated entries in `changelog.json`, added as changes ship, over 1.0.0's record
//! of what shipped at launch.
//!
//! The beta's `AnnouncementModal` is deliberately NOT ported. It is a "new
//! feature since you last looked" spotlight — a registry of featurettes,
//! auto-opened while one is unseen, dismissal persisted by id — and it needs a
//! sequence of releases to have anything to say. The rebuild ships everything
//! in 1.0.0 at once and has no cloud layer to push a new spotlight; the
//! first-run Welcome plus the changelog do the same job, and the update banner
//! (`crate::update_banner`) reopens the Welcome once after a refresh.

use crate::modal::{Modal, ModalSize};
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

/// The version the self-description surfaces state. The header's brand tag
/// carries the same number.
pub const APP_VERSION: &str = "1.0.0";

/// The brand mark at the size the About card draws it — the same 64px cut the
/// header uses, for the same reason (see `shell::BRAND_ICON`).
static ABOUT_MARK: Asset = asset!("/assets/img/favicon-64x64.png");

// ============================================================
// The first-run welcome's seen flag.
// ============================================================

const KEY_WELCOME: &str = "sk-welcome";

#[derive(Serialize, Deserialize)]
#[serde(tag = "schema_version", content = "data")]
enum StoredWelcome {
    #[serde(rename = "1")]
    V1 { seen: bool },
}

/// Persist whether the first-run welcome has been seen (fire-and-forget, on the
/// same localStorage document channel as [`crate::alert_store`]).
pub fn persist_welcome_seen(seen: bool) {
    let stored = StoredWelcome::V1 { seen };
    let Ok(json) = serde_json::to_string(&stored) else {
        return;
    };
    let js = format!("try {{ localStorage.setItem({KEY_WELCOME:?}, {json:?}); }} catch (_) {{}}");
    crate::storage::commit(js);
}

/// The persisted flag, or `None` for missing or corrupt state. The caller
/// treats `None` as unseen — the direction that cannot strand a first-run user
/// is the one that shows.
pub async fn load_welcome_seen() -> Option<bool> {
    let js = format!(
        "try {{ return localStorage.getItem({KEY_WELCOME:?}); }} catch (_) {{ return null; }}"
    );
    let value = document::eval(&js).await.ok()?;
    let text = value.as_str()?;
    let StoredWelcome::V1 { seen } = serde_json::from_str(text).ok()?;
    Some(seen)
}

// ============================================================
// Open flags, provided at the shell root and raised by the main menu's tail.
// ============================================================

#[derive(Clone, Copy)]
pub struct AboutOpen(pub Signal<bool>);

#[derive(Clone, Copy)]
pub struct ChangelogOpen(pub Signal<bool>);

#[derive(Clone, Copy)]
pub struct DonateOpen(pub Signal<bool>);

#[derive(Clone, Copy)]
pub struct WelcomeOpen(pub Signal<bool>);

// ============================================================
// The dated changelog — the beta's `changelog-manual.ts`, as JSON so the Discord
// push (`npm run changelog:push`) reads the same file the app does. Newest date
// first. An entry's `id` is permanent: the push dedups on it, so a reworded
// message is not reposted and a changed id is.
// ============================================================

#[derive(Deserialize)]
struct DatedGroup {
    date: String,
    items: Vec<DatedEntry>,
}

#[derive(Deserialize)]
struct DatedEntry {
    /// Read only by the Discord push and the test; the app has no use for it.
    #[cfg_attr(not(test), allow(dead_code))]
    id: String,
    #[serde(rename = "type")]
    kind: EntryKind,
    message: String,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
enum EntryKind {
    Feat,
    Fix,
    Update,
    KnownIssue,
}

impl EntryKind {
    fn label(self) -> &'static str {
        match self {
            EntryKind::Feat => "New",
            EntryKind::Fix => "Fix",
            EntryKind::Update => "Update",
            EntryKind::KnownIssue => "Known issue",
        }
    }

    fn badge_class(self) -> &'static str {
        match self {
            EntryKind::Feat => "changelog__badge--new",
            EntryKind::Fix => "changelog__badge--fix",
            EntryKind::Update => "changelog__badge--update",
            EntryKind::KnownIssue => "changelog__badge--known",
        }
    }
}

const DATED_CHANGELOG_JSON: &str = include_str!("../changelog.json");

/// A malformed file is caught by the test below, so the app shows an empty list rather than
/// taking the process down over its own release notes.
static DATED_CHANGELOG: std::sync::LazyLock<Vec<DatedGroup>> =
    std::sync::LazyLock::new(|| serde_json::from_str(DATED_CHANGELOG_JSON).unwrap_or_default());

/// `2026-09-30` as `Sep 30, 2026`. The file's own text on anything that does not parse.
fn format_date(date: &str) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let mut parts = date.split('-');
    let (Some(year), Some(month), Some(day), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return date.to_string();
    };
    match (month.parse::<usize>(), day.parse::<u32>()) {
        (Ok(m @ 1..=12), Ok(d)) => format!("{} {d}, {year}", MONTHS[m - 1]),
        _ => date.to_string(),
    }
}

fn dated_entries(group: &DatedGroup) -> Element {
    rsx! {
        ul { class: "changelog__list",
            for entry in &group.items {
                li { class: "changelog__row",
                    span { class: "changelog__badge {entry.kind.badge_class()}", "{entry.kind.label()}" }
                    span { "{entry.message}" }
                }
            }
        }
    }
}

// ============================================================
// The changelog — 1.0.0's own history, curated rather than generated.
//
// The beta injects its git log at build time; the rebuild's single release has
// one honest history, and it is written here, grouped by area. Every message
// names a surface that exists in this tree, so the list cannot outgrow the app.
// ============================================================

struct Entry {
    kind: &'static str,
    message: &'static str,
}

struct Section {
    title: &'static str,
    entries: &'static [Entry],
}

const CHANGELOG: [Section; 5] = [
    Section {
        title: "Data & engine",
        entries: &[
            Entry {
                kind: "New",
                message: "Three game forks read from the binary export: Homecoming, Rebirth and Thunderspy",
            },
            Entry {
                kind: "New",
                message: "Atom-native calculation — a power is the flat list of effects the game ships, interpreted rather than re-derived",
            },
            Entry {
                kind: "New",
                message: "The full stat engine: damage, defense, resistances, movement, status, procs, set bonuses, incarnates, endurance and exemplar scaling",
            },
        ],
    },
    Section {
        title: "Building a build",
        entries: &[
            Entry {
                kind: "New",
                message: "Powerset, pool and epic-pool picking, with granted inherents and power-gated grants",
            },
            Entry {
                kind: "New",
                message: "The incarnate picker and crafting — the tree, the craft ladder and the salvage list",
            },
            Entry { kind: "New", message: "Accolade picking" },
            Entry {
                kind: "New",
                message: "Combat, stance, form and exemplar controls, with per-power adjusters",
            },
        ],
    },
    Section {
        title: "Analysis",
        entries: &[
            Entry {
                kind: "New",
                message: "Eight stat panels and the Info panel, wired to the engine's per-power projection",
            },
            Entry {
                kind: "New",
                message: "The detailed totals sheet, the set-bonus finder and the build's set-bonus totals",
            },
            Entry {
                kind: "New",
                message: "The Attack-Chain builder — rotation DPS, dead time and endurance burn",
            },
            Entry {
                kind: "New",
                message: "Powerset compare, compare-slotting and the what-if team-buff layer",
            },
            Entry {
                kind: "New",
                message: "The enhancement list and tools — re-level, re-attune, re-boost a whole build in one act",
            },
        ],
    },
    Section {
        title: "Keeping & sharing",
        entries: &[
            Entry {
                kind: "New",
                message: "Builds saved as .skif (v5); v2–v4 legacy files still open",
            },
            Entry {
                kind: "New",
                message: "Import from the game — a /buildsave export or a share link, by file or by paste",
            },
            Entry {
                kind: "New",
                message: "Forum export — BBCode, Markdown or plain",
            },
            Entry {
                kind: "New",
                message: "Image export of the build poster",
            },
        ],
    },
    Section {
        title: "Chrome",
        entries: &[
            Entry {
                kind: "New",
                message: "A free grid you arrange yourself — drag, resize, fold, hide, remembered",
            },
            Entry {
                kind: "New",
                message: "The quickbar — pin the tools you reach for",
            },
            Entry {
                kind: "New",
                message: "Two themes: the original magenta and Astoria",
            },
            Entry {
                kind: "New",
                message: "A mobile layout under 900px",
            },
        ],
    },
];

fn badge_class(kind: &str) -> &'static str {
    match kind {
        "New" => "changelog__badge--new",
        _ => "changelog__badge--update",
    }
}

// ============================================================
// The hosts. Each owns only its open flag; the shell provides them and mounts
// the hosts outside both layout roots, like every other modal.
// ============================================================

#[component]
pub fn AboutHost() -> Element {
    let mut open = use_context::<AboutOpen>().0;
    if !open() {
        return rsx! {};
    }

    rsx! {
        Modal {
            title: "About Sidekick".to_string(),
            size: ModalSize::Md,
            on_close: move |_| open.set(false),
            div { class: "about",
                img { class: "about__mark", src: ABOUT_MARK, alt: "Sidekick" }
                h3 { class: "about__title", "CoH Sidekick {APP_VERSION}" }
                p { class: "about__body", "A ground-up rebuild of the Sidekick planner — the beta's data and its promise, in Rust and Dioxus. Every number on screen is read from the game's own export and interpreted by the engine; none is re-derived by hand." }
                p { class: "about__body", "Data: Homecoming, Rebirth and Thunderspy, read from the binary export." }
                p { class: "about__credit", "The original Sidekick planner was made by @wednesdaywoe." }
            }
        }
    }
}

#[component]
pub fn ChangelogHost() -> Element {
    let mut open = use_context::<ChangelogOpen>().0;
    if !open() {
        return rsx! {};
    }

    rsx! {
        Modal {
            title: "Changelog".to_string(),
            size: ModalSize::Lg,
            on_close: move |_| open.set(false),
            div { class: "changelog",
                for group in DATED_CHANGELOG.iter() {
                    div { class: "changelog__section",
                        h3 { class: "changelog__title", "{format_date(&group.date)}" }
                        {dated_entries(group)}
                    }
                }
                h3 { class: "changelog__release", "At launch — {APP_VERSION}" }
                for section in &CHANGELOG {
                    div { class: "changelog__section",
                        h3 { class: "changelog__title", "{section.title}" }
                        ul { class: "changelog__list",
                            for entry in section.entries {
                                li { class: "changelog__row",
                                    span { class: "changelog__badge {badge_class(entry.kind)}", "{entry.kind}" }
                                    span { "{entry.message}" }
                                }
                            }
                        }
                    }
                }
                p { class: "changelog__foot", "Supersedes the Sidekick beta — the same planner, ground-up in Rust and Dioxus." }
            }
        }
    }
}

#[component]
pub fn WelcomeHost() -> Element {
    let mut open = use_context::<WelcomeOpen>().0;
    let mut changelog = use_context::<ChangelogOpen>().0;
    if !open() {
        return rsx! {};
    }

    rsx! {
        Modal {
            title: "Welcome to Sidekick".to_string(),
            size: ModalSize::Lg,
            on_close: move |_| {
                open.set(false);
                persist_welcome_seen(true);
            },
            div { class: "welcome",
                p { class: "welcome__lead", "Sidekick {APP_VERSION} is a ground-up rebuild of the Sidekick planner, in Rust and Dioxus. Three things to know before the first build:" }
                ul { class: "welcome__points",
                    li { "File at the left of the header holds the file acts; Options and Help sit at the right, with the preferences and these pages. On a phone the ☰ at the bottom holds all three." }
                    li { "The quickbar under it holds what you do to the build — pin whatever you reach for." }
                    li { "Pick a power and the Info panel reads it out of the engine's own projection: base, slotted, total." }
                }
                if let Some(latest) = DATED_CHANGELOG.first() {
                    div { class: "welcome__news",
                        h3 { class: "changelog__title", "What's new — {format_date(&latest.date)}" }
                        {dated_entries(latest)}
                    }
                }
                div { class: "welcome__actions",
                    button {
                        class: "seg",
                        r#type: "button",
                        onclick: move |_| {
                            open.set(false);
                            persist_welcome_seen(true);
                            changelog.set(true);
                        },
                        "Full changelog"
                    }
                    button {
                        class: "seg is-primary",
                        r#type: "button",
                        onclick: move |_| {
                            open.set(false);
                            persist_welcome_seen(true);
                        },
                        "Got it"
                    }
                }
            }
        }
    }
}

/// The in-app "Support Sidekick" flow — the beta's `DonateModal`.
///
/// **The widget is embedded in a real browser and nowhere else.** The same-origin argument for
/// the iframe is sound in a browser, where BMC's code runs sandboxed by
/// origin and cannot reach this app's storage. It is not an argument about a webview shell, and
/// the shell is where this app actually runs: wry 0.53.5 has no main-frame filter, so on Windows
/// only `add_NavigationStarting` is hooked (`webview2/mod.rs:667`) — the main-frame event —
/// and the third-party page renders INSIDE the app window; on macOS the subframe load instead
/// reaches `decidePolicyForNavigationAction`, dioxus's handler, and `webbrowser::open`, so the
/// frame stays blank and the OS browser opens on its own. One embed, two different wrong
/// behaviours, and macOS is the RC's primary artifact.
///
/// So the desktop build does not embed it. It says where the page opens and opens it there, which
/// is what the macOS path was doing by accident anyway — and it removes a third-party origin
/// from inside the app window rather than arguing about what that origin can reach.
const BMC_WIDGET_URL: &str = "https://buymeacoffee.com/widget/page/Wednesdaywoe";
const BMC_PAGE_URL: &str = "https://buymeacoffee.com/Wednesdaywoe";

/// Whether the donate widget is mounted in-app. True only in a real browser.
///
/// A compile-time split rather than F54's runtime one, and for the opposite reason: there the
/// question was about a value the shell reports at runtime, here it is about which engine the
/// code was built for. `cfg!` rather than `#[cfg]` so both arms keep compiling on both targets
/// and the constant can be asserted on.
const EMBED_WIDGET: bool = cfg!(target_arch = "wasm32");

/// Lg gives the iframe room. Without it the modal holds two lines.
const DONATE_SIZE: ModalSize = if EMBED_WIDGET {
    ModalSize::Lg
} else {
    ModalSize::Md
};

#[component]
pub fn DonateHost() -> Element {
    let mut open = use_context::<DonateOpen>().0;
    if !open() {
        return rsx! {};
    }

    rsx! {
        Modal {
            title: "Support Sidekick ☕".to_string(),
            size: DONATE_SIZE,
            on_close: move |_| open.set(false),
            div { class: "donate",
                if EMBED_WIDGET {
                    iframe {
                        class: "donate__frame",
                        src: BMC_WIDGET_URL,
                        title: "Buy Sidekick a coffee",
                    }
                    p { class: "donate__fallback",
                        "Trouble loading? "
                        a {
                            href: BMC_PAGE_URL,
                            target: "_blank",
                            rel: "noopener noreferrer",
                            "Open Buy Me a Coffee in a new tab ↗"
                        }
                    }
                } else {
                    p { class: "donate__note",
                        "Buy Me a Coffee opens in your browser rather than inside Sidekick, so "
                        "their page and its scripts never run in this window."
                    }
                    a {
                        class: "donate__open",
                        href: BMC_PAGE_URL,
                        target: "_blank",
                        rel: "noopener noreferrer",
                        "Open Buy Me a Coffee ↗"
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The app swallows a bad file into an empty list, so this is where one is caught. The ids
    /// are the Discord push's dedup key: a blank or repeated one would post twice or never.
    #[test]
    fn dated_changelog_is_well_formed() {
        let groups: Vec<DatedGroup> =
            serde_json::from_str(DATED_CHANGELOG_JSON).expect("changelog.json parses");
        let mut seen = std::collections::HashSet::new();
        let mut previous: Option<&str> = None;
        for group in &groups {
            let d = group.date.as_bytes();
            assert!(
                d.len() == 10
                    && d[4] == b'-'
                    && d[7] == b'-'
                    && format_date(&group.date) != group.date,
                "date {:?} is not YYYY-MM-DD",
                group.date
            );
            if let Some(prev) = previous {
                assert!(
                    group.date.as_str() < prev,
                    "{} is not older than {prev}: newest first, one group per date",
                    group.date
                );
            }
            previous = Some(&group.date);
            assert!(!group.items.is_empty(), "{} has no entries", group.date);
            for entry in &group.items {
                assert!(
                    !entry.id.trim().is_empty(),
                    "an entry on {} has no id",
                    group.date
                );
                assert!(
                    !entry.message.trim().is_empty(),
                    "{} has no message",
                    entry.id
                );
                assert!(
                    seen.insert(entry.id.as_str()),
                    "id {:?} is used twice",
                    entry.id
                );
            }
        }
    }

    #[test]
    fn dates_read_as_words() {
        assert_eq!(format_date("2026-09-30"), "Sep 30, 2026");
        assert_eq!(format_date("2026-13-01"), "2026-13-01");
    }
}
