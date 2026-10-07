//! The enhancement list — the build as a shopping list.
//!
//! A finished build is a plan to buy things. This is that plan: every slotted piece, grouped
//! with its count, plus the two consumables the slotting implies — a Catalyst for each piece
//! that has to be attuned, and an Enhancement Booster for each level a piece is boosted by.
//! The beta's `EnhancementListModal`, less its auction-price column, which is Supabase and
//! belongs with the rest of the cloud work.
//!
//! # What makes two pieces one row
//!
//! A row is a PURCHASE, and two copies belong on it only when buying one is buying the other.
//! So the grouping key is the piece's identity plus every axis that changes what you buy: its
//! craft level, whether it is attuned, and how far it is boosted. The beta keys on set + piece
//! name alone, which puts a level-25 and a level-50 common IO on one line — two different items
//! on the auction house, counted as one thing to acquire.
//!
//! Splitting on those axes is also what makes the two consumable counts EXACT. Under the beta's
//! coarser key a group holds a mix of boosted and unboosted copies, so ticking one off can only
//! scale the group's boosters proportionally and round — a guess about which copy you bought.
//! Here a group's copies are identical by construction, so `remaining × per-copy` is the answer
//! and nothing is rounded.
//!
//! # The two consumables are read from the data, not from the flag
//!
//! **Catalysts.** `attuned` is true of two different things: a piece the user chose to attune,
//! and a piece from a set that is attuned by nature — [`IoSet::max_level`] `<= 1`, the ATO and
//! event sets. The second kind never consumed a Catalyst. Counting the flag (as the beta does)
//! bills the user for one per ATO piece in the build.
//!
//! **Boosters.** `boost` is a booster combine on an IO and a signed relative level on an SO or a
//! Hamidon ([`Enhancement::boost`]) — one field, two mechanics, told apart by the kind. Summing
//! it across kinds is how the beta's list reaches a build that needs −3 boosters.
//!
//! # Ticking off
//!
//! Clicking a row marks one copy acquired; clicking a finished row clears it. The state is the
//! modal's own and goes when it closes, because it is about a shopping trip rather than about
//! the build — nothing here is written back, and the `.skif` gains no field.
//!
//! The clipboard carries what is on screen, remaining counts and all. A copy button that
//! silently exports the full list while the surface shows a half-finished one is a small lie of
//! exactly the kind this codebase keeps paying for.

use crate::build_session::BuildSession;
use crate::modal::{Modal, ModalSize};
use crate::naming;
use crate::shell::Db;
use coh_data::{CharacterState, Enhancement, EnhancementKind, PowerDatabase};
use dioxus::prelude::*;
use std::collections::HashMap;

// ============================================================
// The list.
// ============================================================

/// Which run of the list a section sits in. The variant order IS the order on screen: set
/// pieces first, because they are what a build is planned around and what costs the most to
/// assemble, then the commons, then the origin families, then the vendor tiers.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum SectionRank {
    IoSet,
    Generic,
    Special,
    Origin,
}

/// One purchasable line: a piece, in one configuration, and how many of it the build wants.
#[derive(Clone, PartialEq, Debug)]
pub struct Item {
    /// A key stable across rebuilds of the list, so a tick survives an edit to an unrelated
    /// power. Derived from everything the grouping keys on, which is what makes it stable.
    pub key: String,
    /// The piece's own name. The set name is the section heading, not part of this.
    pub label: String,
    /// What distinguishes this configuration from another of the same piece
    /// ([`naming::enhancement_qualifiers`]).
    pub qualifiers: Vec<String>,
    pub count: usize,
    /// Catalysts one copy needs: 1 for a piece the user attuned, 0 otherwise.
    pub catalysts_each: usize,
    /// Enhancement Boosters one copy needs.
    pub boosters_each: usize,
    /// This piece names an IO set the loaded dataset does not carry, so whether it is attuned
    /// by nature — and therefore whether it needs a Catalyst — cannot be answered. Surfaced
    /// rather than guessed either way (Rule 1).
    pub set_unresolved: bool,
}

impl Item {
    /// The row as one line of text, at whatever count the caller is showing.
    fn line(&self, count: usize) -> String {
        let mut line = format!("{count}x {}", self.label);
        if !self.qualifiers.is_empty() {
            line.push_str(&format!(" ({})", self.qualifiers.join(", ")));
        }
        if self.set_unresolved {
            line.push_str(" — set not in this dataset");
        }
        line
    }
}

/// One heading and the rows under it — an IO set, the commons, a special family, a vendor tier.
#[derive(Clone, PartialEq, Debug)]
pub struct Section {
    pub heading: String,
    pub items: Vec<Item>,
}

/// Everything the build wants, grouped.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct ShoppingList {
    pub sections: Vec<Section>,
    /// Every slotted piece, counted once — the figure the header's `remaining/total` is out of.
    pub total_pieces: usize,
}

/// The axes that make two copies the same purchase. Ordering is the on-screen order, so the
/// grouping map's own iteration is the sort.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct GroupKey {
    rank: SectionRank,
    heading: String,
    /// Where the piece sits in its set. Set pieces list in set order rather than alphabetically,
    /// because that is the order the picker and the set-bonus tooltip both show them in; every
    /// other kind leaves it 0 and sorts by name.
    piece_num: u8,
    label: String,
    /// `None` is a real state, not a missing level: an attuned piece has no craft level, and a
    /// piece with no stored level defers to the build's global IO level.
    level: Option<u8>,
    attuned: bool,
    boost: i8,
}

impl ShoppingList {
    /// Every enhancement in the build, grouped.
    ///
    /// Walks [`CharacterState::all_selected`] rather than naming the buckets, so a pick that
    /// lives somewhere the beta's five-bucket walk did not reach — a granted inherent — is on
    /// the list like any other.
    pub fn build(build: &CharacterState, database: &PowerDatabase) -> Self {
        let mut groups: std::collections::BTreeMap<GroupKey, Item> =
            std::collections::BTreeMap::new();
        let mut total_pieces = 0;

        for power in build.all_selected() {
            for enhancement in power.slots.iter().flatten() {
                total_pieces += 1;
                let (key, item) = describe(enhancement, database);
                groups
                    .entry(key)
                    .and_modify(|existing| existing.count += 1)
                    .or_insert(item);
            }
        }

        let mut sections: Vec<Section> = Vec::new();
        for (key, item) in groups {
            match sections.last_mut() {
                Some(section) if section.heading == key.heading => section.items.push(item),
                _ => sections.push(Section {
                    heading: key.heading,
                    items: vec![item],
                }),
            }
        }

        ShoppingList {
            sections,
            total_pieces,
        }
    }

    fn items(&self) -> impl Iterator<Item = &Item> {
        self.sections
            .iter()
            .flat_map(|section| section.items.iter())
    }
}

/// One slotted piece as its group key and the row it opens.
fn describe(enhancement: &Enhancement, database: &PowerDatabase) -> (GroupKey, Item) {
    let label = naming::enhancement_name(enhancement).to_string();
    let qualifiers = naming::enhancement_qualifiers(enhancement);

    let (rank, heading, piece_num, set_unresolved, inherently_attuned) = match &enhancement.kind {
        EnhancementKind::IoSet {
            set_id,
            set_name,
            piece_num,
            ..
        } => {
            let set = database
                .io_sets
                .as_ref()
                .and_then(|catalog| catalog.get(set_id));
            let heading = if set_name.is_empty() {
                set_id.clone()
            } else {
                set_name.clone()
            };
            (
                SectionRank::IoSet,
                heading,
                *piece_num,
                set.is_none(),
                set.is_some_and(|set| set.attuned_only),
            )
        }
        EnhancementKind::GenericIo { .. } => (
            SectionRank::Generic,
            "Generic IOs".to_string(),
            0,
            false,
            false,
        ),
        EnhancementKind::Special { category, .. } => (
            SectionRank::Special,
            special_family_label(category, database),
            0,
            false,
            false,
        ),
        EnhancementKind::Origin { tier, .. } => (
            SectionRank::Origin,
            format!("{tier} Enhancements"),
            0,
            false,
            false,
        ),
    };

    // A booster is a thing you buy; a relative level is where you happen to be standing.
    let boosters_each = match &enhancement.kind {
        EnhancementKind::IoSet { .. } | EnhancementKind::GenericIo { .. } => {
            usize::try_from(enhancement.boost).unwrap_or(0)
        }
        EnhancementKind::Special { .. } | EnhancementKind::Origin { .. } => 0,
    };

    let catalysts_each = usize::from(enhancement.attuned && !inherently_attuned && !set_unresolved);

    let key = GroupKey {
        rank,
        heading: heading.clone(),
        piece_num,
        label: label.clone(),
        level: enhancement.level.map(|level| level.get()),
        attuned: enhancement.attuned,
        boost: enhancement.boost,
    };
    let item = Item {
        key: format!(
            "{rank:?}\u{0}{heading}\u{0}{label}\u{0}{}",
            qualifiers.join("\u{1}")
        ),
        label,
        qualifiers,
        count: 1,
        catalysts_each,
        boosters_each,
        set_unresolved,
    };
    (key, item)
}

/// A special family's display label, from the catalog's own family list — the same labels the
/// picker's tabs carry, so the list and the picker cannot name a family differently. The raw
/// tag is the fallback, for the reason every fallback here is the id.
fn special_family_label(category: &str, database: &PowerDatabase) -> String {
    database
        .enhancements
        .as_ref()
        .and_then(|catalog| {
            catalog
                .special_families()
                .into_iter()
                .find(|(_, tag, _)| *tag == category)
                .map(|(label, _, _)| label.to_string())
        })
        .unwrap_or_else(|| category.to_string())
}

// ============================================================
// What is left to buy.
// ============================================================

/// How many of each row the user has ticked off, keyed by [`Item::key`].
type Acquired = HashMap<String, usize>;

/// The three running totals under the header, over the rows still outstanding.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
struct Outstanding {
    pieces: usize,
    catalysts: usize,
    boosters: usize,
}

impl ShoppingList {
    /// What is still to be acquired. `acquired` empty is the whole list.
    fn outstanding(&self, acquired: &Acquired) -> Outstanding {
        let mut out = Outstanding::default();
        for item in self.items() {
            let remaining = item.count - acquired_count(acquired, item);
            out.pieces += remaining;
            out.catalysts += remaining * item.catalysts_each;
            out.boosters += remaining * item.boosters_each;
        }
        out
    }

    /// The list as text, at the counts on screen. Finished rows are dropped rather than struck
    /// through: strikethrough is a mark this destination does not have, and a line that says
    /// `0x` reads as an item you need none of rather than one you already own.
    fn to_text(&self, acquired: &Acquired) -> String {
        let mut lines: Vec<String> = Vec::new();
        for section in &self.sections {
            let live: Vec<(&Item, usize)> = section
                .items
                .iter()
                .map(|item| (item, item.count - acquired_count(acquired, item)))
                .filter(|(_, remaining)| *remaining > 0)
                .collect();
            if live.is_empty() {
                continue;
            }
            lines.push(section.heading.clone());
            for (item, remaining) in live {
                lines.push(format!("  {}", item.line(remaining)));
            }
            lines.push(String::new());
        }

        let out = self.outstanding(acquired);
        lines.push(format!("Catalysts needed: {}", out.catalysts));
        lines.push(format!("Enhancement Boosters needed: {}", out.boosters));
        lines.join("\n")
    }
}

/// How many copies of a row are ticked off, never more than the row holds — a build edited
/// while the modal is open can shrink a row under its own tick count.
fn acquired_count(acquired: &Acquired, item: &Item) -> usize {
    acquired
        .get(&item.key)
        .copied()
        .unwrap_or(0)
        .min(item.count)
}

// ============================================================
// The modal.
// ============================================================

/// The list's open state, held at the shell root for the containment reason every modal here
/// shares: a `fixed` backdrop is contained by the grid's `transform`ed surfaces (see
/// [`crate::modal`]).
#[derive(Clone, Copy)]
pub struct EnhancementListOpen(pub Signal<bool>);

/// Mounted by the shell above both layout roots. Renders nothing while closed.
#[component]
pub fn EnhancementListHost(database: Option<Db>) -> Element {
    let mut open = use_context::<EnhancementListOpen>().0;
    if !open() {
        return rsx! {};
    }
    rsx! {
        Modal {
            title: "Enhancement list".to_string(),
            size: ModalSize::Lg,
            on_close: move |_| open.set(false),
            EnhancementListBody { database }
        }
    }
}

#[component]
fn EnhancementListBody(database: Option<Db>) -> Element {
    let session = use_context::<BuildSession>();
    let mut acquired = use_signal(Acquired::new);
    let mut copied = use_signal(|| Option::<Result<(), String>>::None);

    // Every hook runs before either early return below. A `use_memo` reached only on some
    // renders shifts the hook index of everything after it, and BOTH of those branches flip
    // during a session: the dataset resolves under an already-open modal, and clearing the
    // build empties a list that was full a moment ago.
    let loading = database.is_none();
    // A memo, not a signal: the list is a pure function of the build and the dataset, and
    // derived state that is STORED is state that can be stale.
    let list = use_memo(move || match &database {
        Some(database) => ShoppingList::build(&session.build.read(), &database.0),
        None => ShoppingList::default(),
    });
    let out = use_memo(move || list.read().outstanding(&acquired.read()));
    // What the build wants with nothing ticked off — the denominator of all three tallies, and
    // a whole walk of the list, so it is memoized rather than recomputed per tally per render.
    let full = use_memo(move || list.read().outstanding(&Acquired::new()));

    if loading {
        return rsx! {
            div { class: "load-state", "Loading the dataset…" }
        };
    }
    if list.read().total_pieces == 0 {
        return rsx! {
            p { class: "enh-list__empty", "Nothing is slotted in this build yet." }
        };
    }

    rsx! {
        div { class: "enh-list",
            div { class: "enh-list__totals",
                Tally {
                    label: "Pieces",
                    remaining: out().pieces,
                    total: list.read().total_pieces,
                }
                Tally {
                    label: "Catalysts",
                    remaining: out().catalysts,
                    total: full().catalysts,
                }
                Tally {
                    label: "Boosters",
                    remaining: out().boosters,
                    total: full().boosters,
                }
                div { class: "enh-list__actions",
                    button {
                        class: "seg",
                        r#type: "button",
                        disabled: acquired.read().is_empty(),
                        onclick: move |_| {
                            acquired.write().clear();
                            copied.set(None);
                        },
                        "Reset ticks"
                    }
                    button {
                        class: "seg is-primary",
                        r#type: "button",
                        onclick: move |_| async move {
                            let text = list.read().to_text(&acquired.read());
                            copied.set(Some(crate::clipboard::copy(&text).await));
                        },
                        "Copy the list"
                    }
                }
            }

            p { class: "enh-list__hint",
                "Click a row to mark one off; click a finished row to put it back. These are not "
                "saved with the build."
            }

            match copied() {
                Some(Ok(())) => rsx! {
                    p { class: "enh-list__status", "Copied — what you see is what was copied." }
                },
                Some(Err(reason)) => rsx! {
                    p { class: "enh-list__status is-error",
                        "Nothing was copied: {reason}."
                    }
                },
                None => rsx! {},
            }

            div { class: "enh-list__body",
                for section in list.read().sections.iter() {
                    div { key: "{section.heading}", class: "enh-list__section",
                        h3 { class: "enh-list__heading", "{section.heading}" }
                        ul { class: "enh-list__rows",
                            for item in section.items.iter() {
                                ItemRow {
                                    key: "{item.key}",
                                    item: item.clone(),
                                    acquired: acquired_count(&acquired.read(), item),
                                    on_tick: move |key: String| {
                                        let mut ticks = acquired.write();
                                        let entry = ticks.entry(key).or_insert(0);
                                        *entry += 1;
                                        copied.set(None);
                                    },
                                    on_clear: move |key: String| {
                                        acquired.write().remove(&key);
                                        copied.set(None);
                                    },
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// One `remaining / total` readout. The total is shown only once it differs from what is left,
/// so a fresh list reads as three plain numbers rather than three identical fractions.
#[component]
fn Tally(label: String, remaining: usize, total: usize) -> Element {
    rsx! {
        div { class: "enh-list__tally",
            span { class: "enh-list__tally-label", "{label}" }
            span { class: "enh-list__tally-value", "{remaining}" }
            if remaining != total {
                span { class: "enh-list__tally-total", "/ {total}" }
            }
        }
    }
}

/// One row. A `<button>` inside the `<li>` rather than a click handler on the row itself: this
/// is an action, and an action has to be reachable from the keyboard.
#[component]
fn ItemRow(
    item: Item,
    acquired: usize,
    on_tick: EventHandler<String>,
    on_clear: EventHandler<String>,
) -> Element {
    let remaining = item.count - acquired;
    let done = remaining == 0;
    let key = item.key.clone();

    rsx! {
        li { class: if done { "enh-list__row is-done" } else { "enh-list__row" },
            button {
                class: "enh-list__tick",
                r#type: "button",
                title: if done { "Put this row back on the list" } else { "Tick one off" },
                onclick: move |_| {
                    if done {
                        on_clear.call(key.clone());
                    } else {
                        on_tick.call(key.clone());
                    }
                },
                span { class: "enh-list__count",
                    if done { "✓" } else { "{remaining}×" }
                }
                span { class: "enh-list__label", "{item.label}" }
                if !item.qualifiers.is_empty() {
                    span { class: "enh-list__qualifiers", "{item.qualifiers.join(\", \")}" }
                }
                if acquired > 0 && !done {
                    span { class: "enh-list__of", "of {item.count}" }
                }
                // The consumables are told apart by their own word, not by a colour: a stat hue
                // here would say "this is a stat" about a count of things you go and buy.
                if item.catalysts_each > 0 && !done {
                    span { class: "enh-list__need",
                        "{remaining * item.catalysts_each} catalyst"
                    }
                }
                if item.boosters_each > 0 && !done {
                    span { class: "enh-list__need",
                        "{remaining * item.boosters_each} boosters"
                    }
                }
                if item.set_unresolved {
                    span { class: "enh-list__unresolved", "set not in this dataset" }
                }
            }
        }
    }
}
