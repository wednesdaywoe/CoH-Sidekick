//! The power surfaces, split out from the identity spine (`panels/identity.rs`) so choosing
//! *who the character is* and choosing *what powers they take* are distinct surfaces,
//! mirroring the beta's split of build identity from the power grid.
//!
//! Three components, each its own draggable panel ([`crate::grid::PanelKind`]):
//! - [`PowersPanel`] — the **loadout**: the picked powers as slotted cards, each with its
//!   enhancement slots and the slot picker (the beta `SelectedPowers`). Two arrangements of
//!   the same cards, chosen by [`PowersLayout`]: grouped by set here, or laid out over the
//!   levels they were picked at ([`crate::panels::powers_by_level`]).
//! - [`AvailablePanel`] — the **selection surface**: every power of the primary and secondary
//!   as a level-ordered numbered row (the beta `AvailablePowers` list); clicking a row adds
//!   the power to the loadout.
//! - [`PoolsPanel`] — the same rows for the **pools**, under the two triggers that choose
//!   which pools the build holds (the [pool picker](crate::panels::pool_picker)). Its own
//!   surface since LAY6, because the pools grow by a list every time one is taken and the
//!   powersets do not.
//!
//! All three read the same session build, and every edit goes through [`BuildSession::commit`]
//! so it is undoable and persisted.

use crate::build_session::BuildSession;
use crate::enhancement_tools::under_craft_level;
use crate::modal::{Modal, ModalSize};
use crate::panels::power_group::PowerGroup;
use crate::picker_defaults::{CraftBand, PickerDefaults};
use crate::shell::Db;
use crate::view::power_view::resolve_power_def;
use coh_data::slot_levels::{self, SlotCategory};
use coh_data::Level;
use dioxus::prelude::*;
use serde_json::{Map, Value};
use std::ops::Range;

/// The build's level-gated enhancement-slot budget: how many placeable slots are spent vs
/// available at the current level (the beta `slotBudget`/`currentSlotCount`). A plain snapshot
/// the shell-root memo produces and [`SlotCounter`] renders.
#[derive(Clone, Copy, PartialEq, Default)]
pub struct SlotBudget {
    pub used: usize,
    pub available: usize,
}

/// Context handle for the one slot-budget memo, provided at the shell root beside
/// [`BuildTotals`](crate::panels::stats::BuildTotals). A newtype so it never collides with
/// another `Memo<_>` in context.
#[derive(Clone, Copy)]
pub struct BuildBudget(pub Memo<SlotBudget>);

/// Every slot's level, solved once at the shell root ([`coh_data::slot_levels::slot_levels`])
/// and read by each card. `None` when the "Show slot levels" option is off, or before the
/// dataset loads.
#[derive(Clone, Copy)]
pub struct SlotLevelsView(pub Memo<Option<std::rc::Rc<coh_data::slot_levels::SlotLevels>>>);

/// The "Show slot levels" option — whether the level each slot was added at is drawn under it.
/// A preference held at the shell root and saved by [`crate::slot_level_store`]; independent of
/// level-up mode, since knowing when a slot comes in matters to a build planned whole too.
#[derive(Clone, Copy)]
pub struct ShowSlotLevels(pub Signal<bool>);

/// The "Hit Chance Alert" option — whether a picked power that rolls to hit and lands under the
/// 95% cap against the combat panel's target is badged with its chance. Saved by
/// [`crate::hit_chance_alert_store`].
#[derive(Clone, Copy)]
pub struct HitChanceAlert(pub Signal<bool>);

/// What one slot's level badge says.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SlotLevelBadge {
    /// Granted at this character level.
    At(u8),
    /// The schedule has no grant left this slot could take — the build wants more slots at
    /// this power's level or later than the game issues.
    Unplaced,
}

/// How [`PowersPanel`] arranges the picked powers — the beta's `powerViewMode`. Two
/// arrangements of the same cards: grouped by where a power came from, or laid out over
/// the levels it was taken at ([`crate::panels::powers_by_level`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum PowersLayout {
    /// Four tracks: primary, secondary, the pools stacked, then the epic pool and the
    /// granted inherents.
    ByPowerset,
    /// One cell per power pick the character earns, in level order.
    ByLevel,
}

/// Context handle for the chosen [`PowersLayout`], provided at the shell root so the
/// choice survives the panel remounting (a dataset switch, a drag) and so the shell owns
/// its persistence. A newtype so it never collides with another `Signal<_>` in context.
#[derive(Clone, Copy)]
pub struct PowersLayoutMode(pub Signal<PowersLayout>);

/// The slot the enhancement picker is open on — the beta's global `openEnhancementPicker`
/// target, held as shared UI state. `None` = the picker is closed.
///
/// The picker is a *single* modal hosted at the shell root ([`EnhancementPickerHost`]), not one
/// rendered per slot, because a slot lives inside a free-grid surface whose `transform` becomes
/// the containing block for any `position:fixed` descendant — an inline modal's full-viewport
/// backdrop would be clipped to the Powers panel. Hosting it above both layout roots (where no
/// ancestor is transformed) lets the backdrop fill the viewport, exactly how the FG `DragOverlay`
/// escapes the same trap. A slot sets this target; the host reads it and renders the one modal.
#[derive(Clone, PartialEq)]
pub struct PickerTarget {
    pub powerset_id: String,
    pub power_internal_name: String,
    pub slot_index: u8,
    pub destination: PickerDestination,
}

/// Where a picked enhancement lands. The picker itself is destination-blind — every tab builds
/// its piece and hands it to one routing point — so a surface that wants picks without build
/// writes (a Compare Slotting row) states so here instead of growing a second picker.
#[derive(Clone, Copy, PartialEq)]
pub enum PickerDestination {
    /// The live build, through [`crate::build_session::BuildSession::commit`] (one undo step).
    Build,
    /// One Compare Slotting row — scratch state that must never touch the build or its history.
    CompareCopy { copy_id: u32 },
}

/// Context handle for the open picker target, provided once at the shell root. A newtype (like
/// [`Db`]) so it never collides with another `Signal<Option<_>>` in context.
#[derive(Clone, Copy)]
pub struct PickerOpen(pub Signal<Option<PickerTarget>>);

/// The slot whose hover tooltip is showing, plus the viewport point (the cursor at mouse-enter)
/// to anchor it near. Carries the slotted [`coh_data::Enhancement`] by value; the host resolves
/// any set def it needs from the database, and the owning power (by identity) from the live build
/// to count same-set pieces for the set-bonus block — so the target stays a plain data snapshot.
#[derive(Clone, PartialEq)]
pub struct SlotTooltipTarget {
    pub enhancement: coh_data::Enhancement,
    /// The owning power's identity, so the host can find its sibling slots in the live build
    /// (the beta passes the power's `slots` as a prop; the host reads them fresh instead).
    pub power_internal_name: String,
    pub powerset_id: String,
    pub anchor_x: f64,
    pub anchor_y: f64,
    /// The slots to grade the set bonuses against in place of the live power's. The picker
    /// passes its destination's, because a Compare row's slots are not the build's.
    pub slots: Option<Vec<Option<coh_data::Enhancement>>>,
    /// The closing line: what a click does where the tooltip was raised.
    pub hint: &'static str,
}

/// Context handle for the hovered-slot tooltip, provided once at the shell root (like
/// [`PickerOpen`]). `None` = no slot is hovered. A newtype so it never collides with another
/// `Signal<Option<_>>` in context.
#[derive(Clone, Copy)]
pub struct SlotTooltip(pub Signal<Option<SlotTooltipTarget>>);

/// The picked power whose ⋮ menu is open, and the corner of the ⋮ it hangs from: the
/// button's bottom-right, in viewport pixels. Held at the shell root for the same escape the
/// tooltip needs — a menu drawn inside the card is clipped by the free-grid surface.
#[derive(Clone, PartialEq)]
pub struct PowerMenuTarget {
    pub power_internal_name: String,
    pub powerset_id: String,
    pub anchor_x: f64,
    pub anchor_y: f64,
}

/// Context handle for the ⋮ menu, provided once at the shell root. `None` = closed.
#[derive(Clone, Copy)]
pub struct PowerMenu(pub Signal<Option<PowerMenuTarget>>);

/// Pull an open ⋮ menu back inside the window. It hangs leftward from the button, so on a
/// narrow screen the card at the left edge or the bottom row is the case this exists for.
///
/// Placed in window pixels, then written back through the UI scale's `zoom` on `<html>`
/// ([`crate::ui_scale`]): the anchor and every rect are window pixels, but `left`/`top` and a
/// translate inside the zoomed root are multiplied by it, so at 120% an unconverted menu hung
/// a fifth of the way across the window from its button.
const KEEP_POWER_MENU_ON_SCREEN: &str = "\
document.querySelectorAll('.power-menu').forEach(function (menu) {\
  var z = parseFloat(getComputedStyle(document.documentElement).zoom) || 1;\
  var rect = menu.getBoundingClientRect();\
  var ax = parseFloat(menu.style.left), ay = parseFloat(menu.style.top);\
  var margin = 8;\
  var x = ax - rect.width, y = ay + 4;\
  if (x < margin) { x = margin; }\
  if (x + rect.width > window.innerWidth - margin) { x = window.innerWidth - margin - rect.width; }\
  if (y + rect.height > window.innerHeight - margin) { y = window.innerHeight - margin - rect.height; }\
  menu.style.transform = 'translate(' + (x / z - ax) + 'px, ' + (y / z - ay) + 'px)';\
});";

/// Flip the slot tooltip to the other side of the cursor when it would run past the window's
/// right or bottom edge, and pin it inside an 8px margin when neither side fits. Flipping rather
/// than sliding keeps the card off the pointer, which would otherwise sit on top of its text.
///
/// The cursor anchor is the inline `left`/`top`, in window pixels. Like the ⋮ menu above, the
/// card is placed in window pixels and divided by the UI scale's `zoom` on the way out; without
/// that, every flip at 120% overshot by a fifth and the card's foot ran off the bottom.
///
/// A set piece's card runs to about 480px at 100% scale, past a laptop window's height at 150%.
/// The card ignores the pointer, so it can't scroll; when it won't fit, its set part moves
/// beside the rest (`is-split`), which costs width the window has and saves about a third of
/// the height. The class is cleared first because the host reuses one card element per hover.
const KEEP_SLOT_TOOLTIP_ON_SCREEN: &str = "\
document.querySelectorAll('.slot-tooltip').forEach(function (tip) {\
  var z = parseFloat(getComputedStyle(document.documentElement).zoom) || 1;\
  tip.classList.remove('is-split');\
  var rect = tip.getBoundingClientRect();\
  if (rect.height > window.innerHeight - 16 && tip.querySelector('.slot-tooltip-set-part')) {\
    tip.classList.add('is-split');\
    rect = tip.getBoundingClientRect();\
  }\
  var ax = parseFloat(tip.style.left), ay = parseFloat(tip.style.top);\
  var offset = 14, margin = 8;\
  var x = ax + offset, y = ay + offset;\
  if (x + rect.width > window.innerWidth - margin) { x = ax - offset - rect.width; }\
  if (x < margin) { x = Math.max(margin, window.innerWidth - margin - rect.width); }\
  if (y + rect.height > window.innerHeight - margin) { y = ay - offset - rect.height; }\
  if (y < margin) { y = Math.max(margin, window.innerHeight - margin - rect.height); }\
  tip.style.transform = 'translate(' + (x / z - ax) + 'px, ' + (y / z - ay) + 'px)';\
});";

/// The pin on a picked power's card: one click locks the Info panel to this power, a second
/// unlocks it — the original Sidekick's per-row info toggle, and the same act as right-clicking
/// the card or pressing L. Always drawn, never hover-only, so a finger reaches it as easily as a
/// pointer does; the pressed state carries the word in its label and title, not only its hue.
#[component]
fn PinToggle(target: crate::shell::PowerRef) -> Element {
    let info_lock = use_context::<crate::shell::InfoLock>();
    let pinned = info_lock.holds(&target.powerset_id, &target.power);
    let label = if pinned {
        "Unpin from the Info panel"
    } else {
        "Pin to the Info panel"
    };
    rsx! {
        button {
            class: if pinned { "power-pin is-pinned" } else { "power-pin" },
            r#type: "button",
            title: label,
            "aria-label": label,
            "aria-pressed": pinned,
            onclick: move |_| info_lock.toggle_on(target.clone()),
            svg {
                class: "power-pin__mark",
                view_box: "0 0 16 16",
                width: "12",
                height: "12",
                "aria-hidden": "true",
                fill: if pinned { "currentColor" } else { "none" },
                stroke: "currentColor",
                stroke_width: "1.5",
                stroke_linejoin: "round",
                stroke_linecap: "round",
                path { d: "M5.5 1.75h5M6.5 1.75v4L4 8.75h8L9.5 5.75v-4" }
                path { d: "M8 8.75v5.5", fill: "none" }
            }
        }
    }
}

/// The one ⋮ menu for the whole app: the per-power acts that otherwise sit behind a drag. Pinning
/// is not here: it has its own one-click toggle on the card ([`PinToggle`]), as the original
/// Sidekick had, and a second road to the same act behind two clicks was only clutter. Every
/// entry closes the menu, since a menu left standing over its own result reads as not having
/// worked.
#[component]
pub fn PowerMenuHost(database: Db) -> Element {
    let mut menu = use_context::<PowerMenu>().0;
    let session = use_context::<BuildSession>();
    let Some(target) = menu() else {
        return rsx! {};
    };
    // The power went away under an open menu (undo, a new build): there is nothing to act on.
    // The removable band is the stepper's: past the free base slot, short of the inherent ones.
    let Some((filled_slots, placed_slots)) = session
        .build
        .read()
        .selected_power(&target.powerset_id, &target.power_internal_name)
        .map(|power| {
            (
                power.slots.iter().flatten().count(),
                power
                    .slots
                    .len()
                    .saturating_sub(1 + power.inherent_slot_count as usize),
            )
        })
    else {
        return rsx! {};
    };
    let power_ref = crate::shell::PowerRef {
        powerset_id: target.powerset_id.clone(),
        power: target.power_internal_name.clone(),
    };
    let for_clear = power_ref.clone();
    let for_remove = power_ref;

    rsx! {
        div { class: "popover-backdrop", onclick: move |_| menu.set(None) }
        div {
            class: "power-menu main-menu",
            role: "menu",
            tabindex: "-1",
            style: "left: {target.anchor_x}px; top: {target.anchor_y}px;",
            onmounted: move |evt| async move {
                document::eval(KEEP_POWER_MENU_ON_SCREEN);
                let _ = evt.set_focus(true).await;
            },
            onkeydown: move |evt| {
                if evt.key() == Key::Escape {
                    menu.set(None);
                }
            },
            button {
                class: "main-menu__item",
                role: "menuitem",
                disabled: filled_slots == 0,
                onclick: move |_| {
                    let powerset_id = for_clear.powerset_id.clone();
                    let power_id = for_clear.power.clone();
                    session.commit(move |state| {
                        clear_all_enhancements(state, &powerset_id, &power_id);
                    });
                    menu.set(None);
                },
                span { class: "main-menu__label", "Empty all slots" }
            }
            // The stepper dragged to its floor, in one undo step.
            button {
                class: "main-menu__item",
                role: "menuitem",
                disabled: placed_slots == 0,
                onclick: move |_| {
                    let powerset_id = for_remove.powerset_id.clone();
                    let power_id = for_remove.power.clone();
                    let database = database.clone();
                    session.commit(move |state| {
                        let leveling = database.leveling_schedule.as_ref();
                        for _ in 0..placed_slots {
                            remove_slot_from_power(state, &powerset_id, &power_id, leveling);
                        }
                    });
                    menu.set(None);
                },
                span { class: "main-menu__label", "Remove all slots" }
            }
        }
    }
}

/// The one enhancement picker for the whole app (the beta's single `<EnhancementPicker>` driven
/// by `useUIStore`). Rendered at the shell root — OUTSIDE the free grid's transformed surfaces —
/// so the modal's `position:fixed` backdrop fills the viewport instead of being contained by a
/// surface's `transform` and clipped to the panel. Renders nothing while the picker is closed.
#[component]
pub fn EnhancementPickerHost(database: Db) -> Element {
    let mut open = use_context::<PickerOpen>().0;
    match open() {
        None => rsx! {},
        Some(target) => rsx! {
            EnhancementPickerModal {
                key: "{target.powerset_id}-{target.power_internal_name}-{target.slot_index}",
                database,
                slot_index: target.slot_index,
                power_internal_name: target.power_internal_name.clone(),
                powerset_id: target.powerset_id.clone(),
                destination: target.destination,
                on_close: move |_| open.set(None),
            }
        },
    }
}

/// One effect line inside a set-bonus tier row (the beta's per-`e` span). Precomputed so the
/// markup below stays flat: the spliced human-readable description, the Rule-of-5 `x/5` count
/// when the tier is active and the stat fired at least once, whether that `(stat, value)` is
/// capped (over-cap styling), and whether the raw stat was unrecognized (a fail-loud marker —
/// the tooltip's one departure from the beta, which renders it silently).
#[derive(Clone, PartialEq)]
pub(crate) struct BonusEffectRow {
    pub(crate) description: String,
    count: Option<u32>,
    capped: bool,
    /// Whether THIS power's copy is the one the Rule of 5 refused — a different question from
    /// `capped`, which is true of every copy in a capped bucket (see
    /// [`coh_math::set_bonuses::bonus_instance_rejected`]). The tooltip wants the bucket's
    /// answer, since it is describing the bonus; a list of what a build GETS wants this one.
    pub(crate) refused: bool,
    unknown: bool,
}

/// One set-bonus tier row (the beta's per-`bonus` line): its piece threshold, whether it is
/// active at the current same-set slotting, and its PvE effect lines. A tier whose effects are
/// all PvP-only is dropped before it reaches here (the beta returns `null`).
#[derive(Clone, PartialEq)]
pub(crate) struct BonusTierRow {
    pieces: u8,
    pub(crate) active: bool,
    pub(crate) effects: Vec<BonusEffectRow>,
}

/// The live set-bonus block for the tooltip: which same-set pieces are slotted in this power,
/// the set's total piece count, and the tier rows. Built for any slotted io-set piece whose set
/// resolves — even a lone piece, so the tooltip can show the (all-grey) tier ladder and the
/// piece list. Callers that only want a block worth *printing* (Compare Slotting, the forum
/// export) apply their own `slotted >= 2` threshold, the beta `EnhancementTooltip`'s gate.
#[derive(Clone, PartialEq)]
pub(crate) struct SetBonusBlock {
    pub(crate) slotted: usize,
    /// `piece_num`s of the same-set pieces in the owning power, for marking the set's piece
    /// list. Same membership the `slotted` count measures (a piece can't repeat in one power).
    pub(crate) slotted_piece_nums: Vec<u8>,
    pub(crate) total_pieces: usize,
    pub(crate) tiers: Vec<BonusTierRow>,
}

/// One row of the tooltip's set piece list: the authored piece label, marked when that piece
/// sits in the hovered power (the Mids-style you-have-these highlight).
#[derive(Clone, PartialEq)]
struct SetPieceRow {
    label: String,
    slotted: bool,
}

/// Build the set-bonus block for a hovered io-set piece — the beta `EnhancementTooltip`'s bottom
/// section. `slotted` counts every same-set piece in the owning power (raw, exactly the beta's
/// `slots.filter(setId).length` — no exemplar filter; the Rule-of-5 counter carries the live,
/// exemplar-aware provenance instead). A tier is active when its threshold ≤ `slotted`; each
/// active effect's `x/5` and capped flag come from the shared build tracking. Pure over its
/// inputs — the set def, the owning power, and the projected tracking.
pub(crate) fn build_set_bonus_block(
    set: &coh_data::IoSet,
    power: &coh_data::SelectedPower,
    set_id: &str,
    tracking: &[coh_math::set_bonuses::SetBonusStatTracking],
) -> SetBonusBlock {
    let slotted_piece_nums: Vec<u8> = power
        .slots
        .iter()
        .flatten()
        .filter_map(|slot| match &slot.kind {
            coh_data::EnhancementKind::IoSet {
                set_id: other,
                piece_num,
                ..
            } if other == set_id => Some(*piece_num),
            _ => None,
        })
        .collect();
    let slotted = slotted_piece_nums.len();

    let tiers = set
        .bonuses
        .iter()
        .filter_map(|bonus| {
            let active = usize::from(bonus.pieces) <= slotted;
            let effects: Vec<BonusEffectRow> = bonus
                .effects
                .iter()
                .filter(|effect| !effect.pvp)
                .map(|effect| bonus_effect_row(effect, active, tracking, power, bonus.pieces))
                .collect();
            // A tier with no PvE effect is omitted entirely (beta `if (pveEffects.length === 0)`).
            if effects.is_empty() {
                return None;
            }
            Some(BonusTierRow {
                pieces: bonus.pieces,
                active,
                effects,
            })
        })
        .collect();

    SetBonusBlock {
        slotted,
        slotted_piece_nums,
        total_pieces: set.pieces.len(),
        tiers,
    }
}

/// One PvE effect line — splice the precise value into its description, and (only for an active
/// tier) resolve its Rule-of-5 count/capped from the tracking. An inactive tier shows the
/// description alone (grey), matching the beta's `isActive ? … : null` on the counter.
fn bonus_effect_row(
    effect: &coh_data::SetBonusEffect,
    active: bool,
    tracking: &[coh_math::set_bonuses::SetBonusStatTracking],
    power: &coh_data::SelectedPower,
    pieces: u8,
) -> BonusEffectRow {
    let description = format_bonus_desc(&effect.desc, &effect.stat, effect.value);
    if !active {
        return BonusEffectRow {
            description,
            count: None,
            capped: false,
            refused: false,
            unknown: false,
        };
    }
    match coh_math::set_bonuses::effect_stat_key(&effect.stat) {
        coh_math::set_bonuses::EffectStatKey::Tracked(key) => {
            let (count, capped) =
                coh_math::set_bonuses::bonus_tracking_lookup(tracking, key, effect.value);
            BonusEffectRow {
                description,
                // The beta hides the counter at zero (`totalCount > 0`).
                count: (count > 0).then_some(count),
                capped,
                refused: coh_math::set_bonuses::bonus_instance_rejected(
                    tracking,
                    key,
                    effect.value,
                    &power.internal_name,
                    &power.powerset,
                    pieces,
                ),
                unknown: false,
            }
        }
        coh_math::set_bonuses::EffectStatKey::KnownUntracked => BonusEffectRow {
            description,
            count: None,
            capped: false,
            refused: false,
            unknown: false,
        },
        coh_math::set_bonuses::EffectStatKey::Unknown => BonusEffectRow {
            description,
            count: None,
            capped: false,
            refused: false,
            unknown: true,
        },
    }
}

/// The one slot hover-tooltip for the whole app, rendered at the shell root — OUTSIDE the free
/// grid's `transform`ed surfaces and their `overflow` clip — so a slot's rich tooltip escapes
/// the panel exactly as the picker modal does ([`EnhancementPickerHost`]): a slot lives inside a
/// surface whose `transform` both contains a `position:fixed` child and clips it, so an inline
/// tooltip can't reach the viewport. A slot sets the [`SlotTooltip`] target on hover; the host
/// reads it and renders one `position:fixed`, `pointer-events:none` card anchored at the cursor.
/// Renders nothing while no slot is hovered.
///
/// Shows the static facts (name, set + piece position, type, level/attuned/boost/unique) plus,
/// for any io-set piece, the set's piece list with the slotted ones marked and the live
/// set-bonus tier list with the Rule-of-5 `x/5` counters read from the one shared
/// [`BuildTotals`](crate::panels::stats::BuildTotals) recompute (never a per-card
/// `recalculate`). A lone piece shows the ladder all-grey rather than nothing, so the tooltip
/// answers "what would slotting deeper buy" as well as "what do I have".
#[component]
pub fn SlotTooltipHost(database: Db) -> Element {
    // Context handles are read unconditionally (hook order); the totals memo and the build are
    // only *read* (subscribed) below the no-hover guard, so an idle host doesn't re-render on
    // every build edit.
    let tip = use_context::<SlotTooltip>().0;
    let totals = use_context::<crate::panels::stats::BuildTotals>().0;
    let session = use_context::<BuildSession>();
    // Re-placed on every new hover, after the card has laid out at its new size.
    use_effect(move || {
        if tip.read().is_some() {
            document::eval(KEEP_SLOT_TOOLTIP_ON_SCREEN);
        }
    });
    let Some(target) = tip() else {
        return rsx! {};
    };
    let enh = &target.enhancement;

    // An io-set piece's total piece count, resolved from the catalog for the `n/total` marker.
    // Absent ⇒ show the set name without a count rather than a fabricated total (Rule 1).
    let set_piece_count = match &enh.kind {
        coh_data::EnhancementKind::IoSet { set_id, .. } => database
            .io_sets
            .as_ref()
            .and_then(|catalog| catalog.get(set_id))
            .map(|set| set.pieces.len()),
        _ => None,
    };

    // The level / attuned line, mirroring the beta tooltip's four cases. Computed here so the
    // markup below reads flat.
    let level_text: Option<String> = match (enh.level, enh.attuned) {
        (Some(level), true) => Some(format!("Lv {level} (Attuned)")),
        (Some(level), false) => Some(format!("Lv {level}")),
        (None, true) => Some("Attuned".to_string()),
        (None, false) => None,
    };

    // The slot's under-level pip, said in words. The pip can only carry the number the piece is
    // at; the ceiling it is short of, and what to do about it, need the room a tooltip has.
    let under_level_text: Option<String> =
        CraftBand::from_boost_index(database.boost_index.as_ref())
            .and_then(|band| under_craft_level(enh, &band, database.io_sets.as_ref()))
            .map(|ceiling| {
                format!(
                    "Can be crafted to Lv {ceiling} — Enhancement Tools re-levels the whole build"
                )
            });

    // The live set details: resolve the set def + the owning power from the live build, then
    // grade each bonus tier against the shared tracking and mark which of the set's pieces sit
    // in this power (the Mids-style piece list). Any resolvable io-set piece produces the pair —
    // a lone piece still shows the full ladder greyed, so the player sees what slotting deeper
    // would buy.
    let set_details = match &enh.kind {
        coh_data::EnhancementKind::IoSet { set_id, .. } => {
            let build = session.build.read();
            let tracking = totals().set_bonus_tracking;
            database
                .io_sets
                .as_ref()
                .and_then(|catalog| catalog.get(set_id))
                .zip(build.selected_power(&target.powerset_id, &target.power_internal_name))
                .map(|(set, power)| {
                    let mut power = power.clone();
                    if let Some(slots) = &target.slots {
                        power.slots = slots.clone();
                    }
                    let block = build_set_bonus_block(set, &power, set_id, &tracking);
                    let pieces: Vec<SetPieceRow> = set
                        .pieces
                        .iter()
                        .map(|piece| SetPieceRow {
                            label: piece_label(piece),
                            slotted: block.slotted_piece_nums.contains(&piece.num),
                        })
                        .collect();
                    (block, pieces)
                })
        }
        _ => None,
    };

    // What the piece itself enhances, at its own level and booster, before ED meets the
    // power's other slots. An attuned piece reads at the build level, as the calc reads it.
    let aspect_lines: Vec<(String, f64)> = database
        .enhancement_curves
        .as_ref()
        .zip(Level::from_i64(i64::from(session.build.read().level)))
        .and_then(|(curves, level)| {
            coh_math::enhancement::single_enhancement_values(
                enh,
                level,
                database.io_sets.as_ref(),
                curves,
            )
        })
        .unwrap_or_default();

    // The proc's own account of itself, only from this exact piece's entry.
    let proc_info = match &enh.kind {
        coh_data::EnhancementKind::IoSet {
            set_id,
            set_name,
            is_proc: true,
            ..
        } => {
            let sole_proc_in_set = database
                .io_sets
                .as_ref()
                .and_then(|catalog| catalog.get(set_id))
                .is_some_and(|set| set.pieces.iter().filter(|piece| piece.proc).count() == 1);
            database
                .procs
                .find_for_piece(&enh.name, set_name, sole_proc_in_set)
                .map(|proc| {
                    let rate = match (proc.proc_type, proc.ppm) {
                        (coh_data::ProcType::Proc, Some(ppm)) => format!("{ppm} PPM"),
                        (coh_data::ProcType::Proc, None) => String::new(),
                        (coh_data::ProcType::Global | coh_data::ProcType::Proc120s, _) => {
                            "Always on".to_string()
                        }
                    };
                    (proc.mechanics.clone().unwrap_or_default(), rate)
                })
        }
        _ => None,
    };

    rsx! {
        div {
            class: "slot-tooltip",
            style: "left: {target.anchor_x}px; top: {target.anchor_y}px;",
            // Two parts so a card too tall for the window can stand them side by side
            // (`KEEP_SLOT_TOOLTIP_ON_SCREEN`): this piece, then its set.
            div { class: "slot-tooltip-main",
            div { class: "slot-tooltip-name", "{crate::naming::enhancement_name(enh)}" }
            if let coh_data::EnhancementKind::IoSet { set_name, piece_num, .. } = &enh.kind {
                div { class: "slot-tooltip-set",
                    "{set_name}"
                    if let Some(total) = set_piece_count {
                        span { class: "slot-tooltip-pieces", " ({piece_num}/{total})" }
                    }
                }
            }
            div { class: "slot-tooltip-meta",
                span {
                    class: "slot-tooltip-type slot-tooltip-type--{enhancement_type_class(&enh.kind)}",
                    "{enhancement_type_label(&enh.kind)}"
                }
                if let Some(text) = &level_text {
                    span { "{text}" }
                }
                if enh.boost > 0 {
                    span { class: "slot-tooltip-boost", "+{enh.boost} Boosted" }
                }
                if enh.boost < 0 {
                    span { class: "slot-tooltip-underlevel", "{enh.boost} Under Level" }
                }
                if let Some(text) = &under_level_text {
                    span { class: "slot-tooltip-undercraft", "{text}" }
                }
                if matches!(&enh.kind, coh_data::EnhancementKind::IoSet { is_unique: true, .. }) {
                    span { class: "slot-tooltip-unique", "Unique" }
                }
            }
            if !aspect_lines.is_empty() {
                div { class: "slot-tooltip-aspects",
                    for (i , (aspect, value)) in aspect_lines.iter().enumerate() {
                        div { key: "{i}", class: "slot-tooltip-aspect",
                            span { "{aspect}" }
                            span { class: "slot-tooltip-aspect-value", "+{value * 100.0:.1}%" }
                        }
                    }
                }
            }
            if let Some((mechanics, rate)) = &proc_info {
                div { class: "slot-tooltip-proc",
                    span { class: "slot-tooltip-proc-head", "Proc" }
                    if !rate.is_empty() {
                        span { class: "slot-tooltip-proc-rate", " · {rate}" }
                    }
                    if !mechanics.is_empty() {
                        div { "{mechanics}" }
                    }
                }
            }
            }
            if let Some((block, pieces)) = &set_details {
                div { class: "slot-tooltip-set-part",
                div { class: "slot-tooltip-piecelist",
                    div { class: "slot-tooltip-bonuses-head",
                        "Set Pieces ({block.slotted}/{block.total_pieces} slotted)"
                    }
                    for (i , piece) in pieces.iter().enumerate() {
                        div {
                            key: "{i}",
                            class: if piece.slotted { "slot-tooltip-piece slotted" } else { "slot-tooltip-piece" },
                            "{piece.label}"
                        }
                    }
                }
                div { class: "slot-tooltip-bonuses",
                    div { class: "slot-tooltip-bonuses-head", "Set Bonuses" }
                    SetBonusTierList { tiers: block.tiers.clone() }
                }
                }
            }
            div { class: "slot-tooltip-hint", "{target.hint}" }
        }
    }
}

/// One set's bonus tier rows — the threshold, the active state at the current same-set count,
/// and each PvE effect line with its Rule-of-5 `x/5` and capped marks. Extracted from the slot
/// tooltip so Compare Slotting renders the identical rows for its hypothetical slottings; only
/// the heading above differs per surface (the tooltip says slotted counts, the compare modal
/// names the set).
#[component]
pub(crate) fn SetBonusTierList(tiers: Vec<BonusTierRow>) -> Element {
    rsx! {
        for (tier_index , tier) in tiers.iter().enumerate() {
            div {
                key: "{tier_index}",
                class: if tier.active { "slot-bonus-tier active" } else { "slot-bonus-tier" },
                span { class: "slot-bonus-pieces", "{tier.pieces}pc:" }
                " "
                for (i , effect) in tier.effects.iter().enumerate() {
                    span {
                        class: if effect.capped { "slot-bonus-effect capped" } else { "slot-bonus-effect" },
                        if i > 0 { ", " }
                        "{effect.description}"
                        if effect.unknown {
                            span { class: "slot-bonus-unknown", " (unrecognized stat)" }
                        }
                        if let Some(count) = effect.count {
                            span {
                                class: if effect.capped { "slot-bonus-count capped" } else { "slot-bonus-count" },
                                " ({count}/5)"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The loadout surface — the picked powers as slotted cards. Reads the session's chosen
/// powersets; the powerset *choice* lives in the header's Build Identity popover, and the
/// powers to add come from the Available panel.
#[component]
pub fn PowersPanel(database: Db) -> Element {
    let session = use_context::<BuildSession>();
    let build = session.build;

    // Powersets are chosen in the header's Build Identity popover; without them there is no
    // loadout to show.
    let has_sets = build.read().primary.id.is_some() || build.read().secondary.id.is_some();
    if !has_sets {
        return rsx! {
            div { class: "powers",
                p { class: "hint",
                    "Choose an archetype and powersets in the Build Identity menu to start picking powers."
                }
            }
        };
    }

    let has_picked = !build.read().primary.powers.is_empty()
        || !build.read().secondary.powers.is_empty()
        || !build.read().pools.is_empty()
        || build
            .read()
            .epic_pool
            .as_ref()
            .is_some_and(|ep| !ep.powers.is_empty());

    // The by-level layout renders its own empty cells, so it stays up before anything is
    // picked — the ladder of unspent picks IS the useful view there. The by-powerset
    // layout has nothing to draw until a power is picked, so it keeps the hint.
    let layout = use_context::<PowersLayoutMode>().0;

    rsx! {
        div { class: "powers",
            div { class: "powers-toolbar",
                SlotCounter {}
                PowersLayoutToggle {}
            }
            match layout() {
                PowersLayout::ByPowerset if has_picked => rsx! {
                    // FOUR fixed tracks — Primary, Secondary, every pool stacked, then the
                    // epic pool and the granted inherents. Fixed because a track per set made
                    // the grid ragged in proportion to how many pools the build held (up to
                    // `maxPowerPools` + 3 = eight tracks on the fork that allows five), and
                    // because it asserted a parity that isn't real: a primary contributes ~9
                    // picks, a pool contributes one to four. Stacking the pools into one track
                    // sizes them by the weight they actually carry, and the track count no
                    // longer moves as pools are added.
                    div { class: "power-tracks",
                        div { class: "power-track",
                            PickedPowersGroupPrimary { database: database.clone() }
                        }
                        div { class: "power-track",
                            PickedPowersGroupSecondary { database: database.clone() }
                        }
                        div { class: "power-track",
                            if build.read().pools.is_empty() {
                                p { class: "hint", "No power pools yet." }
                            }
                            // Keyed by the pool's id, not its position: a group now carries
                            // fold state, and keying by index would hand a removed pool's
                            // fold to whichever pool slid up into its place.
                            for pool in build.read().pools.clone().iter() {
                                PickedPowersGroupPool {
                                    key: "pool-{pool.id}",
                                    pool: pool.clone(),
                                    database: database.clone(),
                                }
                            }
                        }
                        div { class: "power-track",
                            if let Some(epic) = build.read().epic_pool.clone() {
                                PickedPowersGroupEpic { pool: epic, database: database.clone() }
                            }
                            crate::panels::inherents_section::InherentsSection {
                                database: database.clone(),
                                stacked: true,
                            }
                        }
                    }
                },
                PowersLayout::ByPowerset => rsx! {
                    p { class: "hint", "Pick powers from the Available panel to build your loadout." }
                    crate::panels::inherents_section::InherentsSection {
                        database: database.clone(),
                        stacked: false,
                    }
                },
                // The by-level ladder has one cell per pick the character earns, and a granted
                // power spends no pick — so grants and inherents stay sections below the grid,
                // the one place that is true in that arrangement.
                PowersLayout::ByLevel => rsx! {
                    crate::panels::powers_by_level::PowersByLevel { database: database.clone() }
                    GrantedPowersSection { database: database.clone() }
                    crate::panels::inherents_section::InherentsSection {
                        database: database.clone(),
                        stacked: false,
                    }
                },
            }
        }
    }
}

/// The layout switch (the beta `ViewModeToggle`): a two-option radio group over
/// [`PowersLayout`]. Sets the shell-root signal; the shell persists it.
#[component]
fn PowersLayoutToggle() -> Element {
    let mut layout = use_context::<PowersLayoutMode>().0;
    let option_class = |option: PowersLayout| {
        if layout() == option {
            "powers-layout-option is-active"
        } else {
            "powers-layout-option"
        }
    };

    rsx! {
        div { class: "powers-layout", role: "radiogroup", "aria-label": "Power layout",
            button {
                class: option_class(PowersLayout::ByPowerset),
                role: "radio",
                "aria-checked": layout() == PowersLayout::ByPowerset,
                title: "Group powers by the set they came from",
                onclick: move |_| layout.set(PowersLayout::ByPowerset),
                "By Powerset"
            }
            button {
                class: option_class(PowersLayout::ByLevel),
                role: "radio",
                "aria-checked": layout() == PowersLayout::ByLevel,
                title: "Lay powers out over the levels they are picked at",
                onclick: move |_| layout.set(PowersLayout::ByLevel),
                "By Level"
            }
        }
    }
}

/// The build-wide enhancement-slot counter (beta `StatsDashboard` "Slots N/M"). Flags
/// over-budget with a danger class — spending past the level's pool means slots the build
/// can't actually place.
///
/// It sits on the loadout, not the stats dashboard: this is where slots are *spent*, and the
/// same budget already bounds how far every card's stepper can travel (see `add_slot_to_power`),
/// so the readout and the control it explains are one glance apart.
#[component]
fn SlotCounter() -> Element {
    let budget = use_context::<BuildBudget>().0;
    let SlotBudget { used, available } = budget();
    let over_budget = used > available;

    rsx! {
        div {
            class: if over_budget { "slot-counter slot-counter--over" } else { "slot-counter" },
            span { class: "slot-counter-label", "Slots" }
            span { class: "slot-counter-value mono", "{used} / {available}" }
        }
    }
}

/// Which of the build's two archetype power lists a pick belongs to.
///
/// A VEAT branch set has no bucket of its own — its picks live in the base role's list under
/// their own set id — so the surface offering one has to say which of the two that is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildRole {
    Primary,
    Secondary,
}

/// Where a group of available powers comes from. The three sources differ in exactly two
/// ways — which partition of the dataset holds the powers, and what floor the pool itself
/// puts under their unlock levels — so they share one group component rather than three.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SetOrigin {
    /// The build's primary or secondary powerset.
    Powerset,
    /// One of the standard power pools.
    Pool,
    /// The epic / patron pool.
    Epic,
}

/// Every power a chosen set offers, whichever partition of the dataset holds it.
///
/// Pool and epic powers are NOT powersets: the bundle keeps them in flat partitions tagged
/// by the aggregate that owns them, and an epic power's identity is unique only within its
/// mastery (every archetype's masteries republish the same internal names), which is why the
/// filter is on `set_id` and never on the name alone.
pub fn powers_of_set(database: &Db, set_id: &str, origin: SetOrigin) -> Vec<coh_data::Power> {
    if origin == SetOrigin::Powerset {
        return database
            .find_powerset(set_id)
            .map(|powerset| powerset.powers.clone())
            .unwrap_or_default();
    }
    let partition = match origin {
        SetOrigin::Epic => &database.epic_powers,
        _ => &database.pool_powers,
    };
    partition
        .iter()
        .filter(|entry| entry.set_id == set_id)
        .map(|entry| entry.power.clone())
        .collect()
}

/// The level below which nothing in this set can be taken, whatever the individual powers'
/// own unlock levels say. A powerset has none; the pools have one each, from the schedule
/// (`schedules.bin`), overridden by an aggregate that declares its own.
pub fn set_unlock_floor(database: &Db, set_id: &str, origin: SetOrigin) -> u8 {
    if origin == SetOrigin::Powerset {
        return 1;
    }
    let declared = database
        .pool_catalog
        .find(set_id)
        .and_then(|pool| pool.min_level);
    let scheduled = database
        .leveling_schedule
        .as_ref()
        .map(|schedule| match origin {
            SetOrigin::Epic => schedule.epic_pool_level,
            _ => schedule.pool_unlock_level,
        });
    declared.or(scheduled).unwrap_or(1)
}

/// Whether the archetype bucket this set's picks land in already owns its level-1 pick.
///
/// Level 1 is character creation: exactly one primary pick and one secondary pick. Which
/// power fills each is the player's choice among whatever the data opens at 1 — Homecoming
/// stopped forcing the secondary's first power in i27 page 5, so every level-1 power in a
/// set is a real option — but the count still holds: once a bucket owns a level-1 pick,
/// the set's remaining rows floor at 2. Pools and epics have floors of their own and no
/// bucket here.
fn bucket_owns_level_one(
    state: &coh_data::CharacterState,
    set_id: &str,
    branch_role: Option<BuildRole>,
) -> bool {
    let bucket = match branch_role {
        Some(BuildRole::Primary) => &state.primary,
        Some(BuildRole::Secondary) => &state.secondary,
        None if state.primary.id.as_deref() == Some(set_id) => &state.primary,
        None if state.secondary.id.as_deref() == Some(set_id) => &state.secondary,
        None => return false,
    };
    bucket
        .powers
        .iter()
        .any(|power| !power.is_locked && power.level == 1)
}

/// One VEAT branch set as the available surface offers it: which set, which of the build's two
/// role lists its picks belong to, and the set-level refusal that stops them being taken.
#[derive(Clone, PartialEq)]
pub struct BranchSetOffer {
    pub set_id: String,
    pub role: BuildRole,
    /// `Some(reason)` when the set is listed but nothing in it can be taken yet.
    pub blocked: Option<String>,
}

/// The VEAT branch sets to offer beside the base pair, and how each renders.
///
/// A branch is not declared, it is BOUGHT INTO. The game confers a powerset on purchase
/// (`character_OwnsPowerSet`), and each branch set's `SpecializeRequires` names the OTHER
/// branches' sets — so both branches are offered until the first pick out of one, which is
/// what closes the others. That is why nothing here writes a branch into the build: there is
/// no branch field to write, and inventing one would put a second answer beside the picks.
///
/// The three renderings are the game's, matching the two surfaces `set_gate` documents:
/// `Open` is offered, `NotYet` is listed but unbuyable (`uiLevelPower.c:243` lists a
/// specialization set barred only by level), and `BranchClosed` is listed nowhere
/// (`uiPowers.c:395`). An unreadable gate is listed and refused with its fault (Rule 1).
pub fn branch_offers(database: &Db, state: &coh_data::CharacterState) -> Vec<BranchSetOffer> {
    let Some(archetype_id) = state.archetype.id.as_deref() else {
        return Vec::new();
    };
    let Ok(catalog) = database.archetypes() else {
        return Vec::new();
    };
    let Some(archetype) = catalog.get(archetype_id) else {
        return Vec::new();
    };
    archetype
        .branches
        .iter()
        .flat_map(|branch| {
            [
                (BuildRole::Primary, branch.primary_set.clone()),
                (BuildRole::Secondary, branch.secondary_set.clone()),
            ]
        })
        .filter_map(|(role, set_id)| Some((role, set_id?)))
        .filter_map(|(role, set_id)| {
            let verdict = database.find_powerset(&set_id).map(|powerset| {
                (
                    powerset.name.clone(),
                    coh_data::set_gate(
                        &powerset.buy_requires,
                        &powerset.buy_requires_failed,
                        powerset.specialize_at,
                        &powerset.specialize_requires,
                        state,
                        Some(archetype_id),
                        &database.set_paths,
                    ),
                )
            });
            branch_offer(set_id, role, verdict)
        })
        .collect()
}

/// How one branch set renders, from its set-level verdict. `None` means listed nowhere.
///
/// Split out because every rendering rule lives here and none of them is reachable from a
/// corpus: no fork ships a branch set that is refused by `SetBuyRequires`, names a set the
/// dataset does not have, or carries an expression that will not read. Inside the iterator
/// those arms could only be graded by the two cases real data happens to produce.
///
/// `verdict` is `None` when the dataset ships no such set — a fault worth seeing rather than
/// a set to drop quietly, since dropping it makes a broken catalog look like a short one.
fn branch_offer(
    set_id: String,
    role: BuildRole,
    verdict: Option<(String, Result<coh_data::SetGate, coh_data::RequiresError>)>,
) -> Option<BranchSetOffer> {
    let blocked = match verdict {
        None => Some("not in this dataset".to_string()),
        Some((_, Ok(coh_data::SetGate::Open))) => None,
        Some((name, Ok(coh_data::SetGate::NotYet { at }))) => {
            Some(format!("{name} unlocks at level {at}"))
        }
        Some((_, Ok(coh_data::SetGate::Closed { reason }))) => Some(reason),
        Some((_, Ok(coh_data::SetGate::BranchClosed))) => return None,
        Some((_, Err(error))) => Some(format!("Prerequisite unreadable: {error}")),
    };
    Some(BranchSetOffer {
        set_id,
        role,
        blocked,
    })
}

/// The selection surface — every power of the build's PRIMARY and SECONDARY, as a
/// level-ordered numbered row. Clicking an unpicked row adds the power to the loadout shown on
/// the [`PowersPanel`].
///
/// **Every power, not every unpicked one** (LAY3). The rail used to drop a power the moment it
/// was taken, which made it a list of remaining purchases rather than a view of a powerset —
/// and it contradicted the habit one line down, where a power whose prerequisites are unmet
/// keeps its row precisely so a build can be planned toward it. Being already bought is a
/// reason a row is out of reach like any other, so it gets the same treatment: shown, marked,
/// not offered.
///
/// The groups flow into as many columns as the panel's width pays for rather than one stack,
/// which is what the four grid cells of LAY2 bought: primary and secondary side by side is the
/// arrangement that lets the rail be read as the two sets it is.
///
/// **The pools left this panel in LAY6** for [`PoolsPanel`] and
/// [`PanelKind::Pools`](crate::grid::PanelKind::Pools). They were here on the argument that a
/// pool is chosen by wanting a power out of it, so the choice and the picking are one act — and
/// that argument is untouched: it says the triggers and the pool lists belong on ONE surface,
/// which is exactly what the new panel is. What it never said is that the surface has to be
/// this one. Sharing it cost the pools a placement of their own, and they are the half of the
/// rail whose height is a thing the user does to the build — a fourth pool is four more rows
/// pushing the powersets up — while the powersets' height is fixed the moment the archetype is.
///
/// Since LAY4 the primary and secondary choices are here too, on the heads of their own columns.
///
/// The rail's order, top to bottom, is still the order a build is decided in: the incarnate
/// sockets and the accolade strip (loadouts you toggle rather than browse), then the two
/// powersets. The incarnate row arrived here when `PanelKind::Incarnates` was retired — see
/// [`IncarnateSockets`](crate::panels::incarnate_picker::IncarnateSockets).
#[component]
pub fn AvailablePanel(database: Db) -> Element {
    let session = use_context::<BuildSession>();
    let build = session.build;

    // The archetype is still the popover's, and it gates everything here — every roster, every
    // pool, every gate expression reads it — so without one there is nothing this surface can
    // offer and the hint names the one menu that can fix it. With one, the rail is complete:
    // since LAY4 the powerset choices are on its own column heads, so an archetype with no sets
    // chosen is an empty rail with two live selects, not a sentence about somewhere else.
    let has_archetype = build.read().archetype.id.is_some();
    if !has_archetype {
        return rsx! {
            div { class: "powers",
                p { class: "hint",
                    "Choose an archetype in the Build Identity menu to start picking powers."
                }
            }
        };
    }

    // Picked powers across every set, with the level each was taken at — a picked row stays in
    // its set's list and reads as taken (LAY3), so this marks rather than filters.
    let picked_levels: Vec<(String, u8)> = build
        .read()
        .all_selected()
        .map(|p| (p.internal_name.clone(), p.level))
        .collect();
    // The pick levels already spent, so each row can resolve the level its own pick would
    // land at (and disable itself when the build has none left it could legally take).
    let taken_levels: Vec<u8> = build.read().picked_powers().map(|p| p.level).collect();

    let primary_id = build.read().primary.id.clone();
    let secondary_id = build.read().secondary.id.clone();
    let current_level = build.read().level;
    // The branch offers read what the build has picked (a Crab pick is what closes Bane), so
    // they are recomputed with the build, not memoized on the archetype alone.
    let branches = use_memo(use_reactive!(|database| branch_offers(
        &database,
        &build.read()
    )));

    rsx! {
        div { class: "powers",
            // The rail reads top to bottom in the order a build is actually decided
            // (2026-09-13, user-directed): the two loadouts you toggle rather than browse —
            // incarnates, then accolades — then the powersets, then the pools. The pool
            // triggers used to lead the panel, which put the LAST decision first: a pool is
            // chosen late, after the primary and secondary have taken the picks they are
            // going to take.
            crate::panels::incarnate_picker::IncarnateSockets { database: database.clone() }
            crate::panels::accolade_picker::AccoladeStrip { database: database.clone() }
            // Available powers — level-ordered numbered rows, one group per set.
            //
            // Two flow containers rather than one, because the pool triggers sit between
            // them. Each is its own `auto-fit` grid, so the powersets seat side by side and
            // the pools seat side by side, and a build holding four pools no longer pushes
            // the secondary powerset out of the primary's row.
            div { class: "available-powers",
                // The two powerset columns lead, always both, chosen or not: the head IS the
                // select (LAY4), so an unchosen set is an empty column offering itself rather
                // than a column that does not exist yet.
                AvailablePowersGroup {
                    key: "avail-primary",
                    database: database.clone(),
                    set_id: primary_id.unwrap_or_default(),
                    origin: SetOrigin::Powerset,
                    select_role: BuildRole::Primary,
                    current_level,
                    taken_levels: taken_levels.clone(),
                    picked_levels: picked_levels.clone(),
                }
                AvailablePowersGroup {
                    key: "avail-secondary",
                    database: database.clone(),
                    set_id: secondary_id.unwrap_or_default(),
                    origin: SetOrigin::Powerset,
                    select_role: BuildRole::Secondary,
                    current_level,
                    taken_levels: taken_levels.clone(),
                    picked_levels: picked_levels.clone(),
                }
                // The VEAT branch sets, beside the base pair rather than in place of it — the
                // branch adds two sets to a build that keeps everything it already took.
                for offer in branches().iter() {
                    AvailablePowersGroup {
                        key: "avail-{offer.set_id}",
                        database: database.clone(),
                        set_id: offer.set_id.clone(),
                        origin: SetOrigin::Powerset,
                        branch_role: offer.role,
                        set_blocked: offer.blocked.clone(),
                        current_level,
                        taken_levels: taken_levels.clone(),
                        picked_levels: picked_levels.clone(),
                    }
                }
            }
        }
    }
}

/// The pools surface — the two pool triggers over the pool and epic lists they fill.
///
/// Split out of [`AvailablePanel`] in LAY6. What moved is exactly what was already one block
/// there: [`SetAdders`] and the `available-powers--pools` flow under it, unchanged, because the
/// reason they sit together is the reason the panel exists. A pool is chosen by wanting a power
/// out of it, so the control and its result are one read.
///
/// It is its OWN surface rather than a section because its height is the user's. The powersets
/// on the rail are as tall as the archetype makes them and no taller; the pools grow by a list
/// every time one is taken, which on the shared rail pushed the powersets off the top of a
/// panel whose whole job was to show them. A surface that grows on its own schedule needs a
/// rectangle it can be given, and the grid is where rectangles are handed out.
///
/// The archetype guard is [`AvailablePanel`]'s, restated rather than shared: the pool catalogue,
/// the epic offers and every gate expression read the archetype, so without one this panel has
/// nothing to offer either. It names the same menu, because there is one place to fix it.
#[component]
pub fn PoolsPanel(database: Db) -> Element {
    let session = use_context::<BuildSession>();
    let build = session.build;

    let has_archetype = build.read().archetype.id.is_some();
    if !has_archetype {
        return rsx! {
            div { class: "powers",
                p { class: "hint",
                    "Choose an archetype in the Build Identity menu to start picking powers."
                }
            }
        };
    }

    // The same three reads the rail makes, for the same three reasons: a picked row stays in
    // its list and reads as taken (LAY3), each row resolves the level its own pick would land
    // at, and a gate is answered against the level on screen.
    let picked_levels: Vec<(String, u8)> = build
        .read()
        .all_selected()
        .map(|p| (p.internal_name.clone(), p.level))
        .collect();
    let taken_levels: Vec<u8> = build.read().picked_powers().map(|p| p.level).collect();
    let current_level = build.read().level;
    let pools = build.read().pools.clone();
    let epic = build.read().epic_pool.clone();

    rsx! {
        div { class: "powers",
            SetAdders { database: database.clone() }
            div { class: "available-powers available-powers--pools",
                for pool in pools.iter() {
                    AvailablePowersGroup {
                        key: "avail-{pool.id}",
                        database: database.clone(),
                        set_id: pool.id.clone(),
                        origin: SetOrigin::Pool,
                        current_level,
                        taken_levels: taken_levels.clone(),
                        picked_levels: picked_levels.clone(),
                    }
                }
                if let Some(epic) = epic {
                    AvailablePowersGroup {
                        key: "avail-{epic.id}",
                        database: database.clone(),
                        set_id: epic.id.clone(),
                        origin: SetOrigin::Epic,
                        current_level,
                        taken_levels,
                        picked_levels,
                    }
                }
            }
        }
    }
}

/// The two pool triggers, over the pool lists they add to: open the picker on the standard
/// pools, or on the epic ones.
///
/// They led the Available rail until 2026-09-13, then sat above the pool lists on it, and since
/// LAY6 head [`PoolsPanel`] — one move each time, always toward the lists they fill. Directly
/// above them is both the plainer read — the control and its result are one block — and the
/// honest order: a pool is the last thing a build decides, after the primary and secondary have
/// taken the picks they are going to take.
///
/// They are buttons onto the [pool picker](crate::panels::pool_picker) rather than the
/// `select` menus this surface first shipped with, because a menu can only offer names and a
/// pool is not what anyone is choosing — a build takes Fighting because it wants Tough. The
/// powers have to be readable before the pool is committed, which is a browsing surface, not
/// a dropdown.
///
/// The standard trigger reports the schedule's
/// [`max_power_pools`](coh_data::LevelingSchedule::max_power_pools) as a count rather than
/// disappearing at the cap: "3 / 4 taken" tells a build where it stands, where a vanished
/// control tells it nothing. The epic trigger names the pool the build holds, so this row
/// still reads as the build's standing state at a glance.
#[component]
fn SetAdders(database: Db) -> Element {
    let session = use_context::<BuildSession>();
    let build = session.build;
    let mut picker = use_context::<crate::panels::pool_picker::PoolPickerOpen>().0;

    let held = build.read().pools.len();
    let epic_name = build
        .read()
        .epic_pool
        .as_ref()
        .map(|epic| crate::naming::pool_name(&epic.id, &database));
    let has_archetype = build.read().archetype.id.is_some();
    let pool_cap = database
        .leveling_schedule
        .as_ref()
        .map_or(0, |schedule| schedule.max_power_pools as usize);
    let pools_full = held >= pool_cap;

    rsx! {
        div { class: "set-adders",
            button {
                class: "set-adder-button",
                disabled: pools_full,
                title: if pools_full {
                    "Every power pool slot is taken — remove one to add another"
                } else {
                    "Browse the power pools and their powers"
                },
                onclick: move |_| {
                    picker.set(Some(crate::panels::pool_picker::PoolPickerMode::Pool))
                },
                span { class: "set-adder-button__label", "Power pools" }
                span { class: "set-adder-button__value mono", "{held} / {pool_cap}" }
            }

            button {
                class: "set-adder-button",
                disabled: !has_archetype,
                title: if has_archetype {
                    "Browse the epic and patron pools open to this archetype"
                } else {
                    "Choose an archetype first — the epic pools open to a build depend on it"
                },
                onclick: move |_| {
                    picker.set(Some(crate::panels::pool_picker::PoolPickerMode::Epic))
                },
                span { class: "set-adder-button__label", "Epic pool" }
                match &epic_name {
                    Some(name) => rsx! {
                        span { class: "set-adder-button__value", "{name}" }
                    },
                    None => rsx! {
                        span { class: "set-adder-button__value is-empty", "none" }
                    },
                }
            }

            if epic_name.is_some() {
                button {
                    class: "set-adder-clear",
                    title: "Drop the epic pool and the powers picked from it",
                    onclick: {
                        let database = database.clone();
                        move |_| {
                            let database = database.clone();
                            session.commit(move |state| {
                                remove_epic_pool(state);
                                crate::granted_powers::sync(state, &database);
                            });
                        }
                    },
                    "✕"
                }
            }
        }
    }
}

/// Whether the game's own prerequisite expression lets this build take this power.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum PickGate {
    /// Pickable now.
    Open,
    /// The game grants this power; it is never picked.
    Granted,
    /// A prerequisite the build does not meet.
    Closed,
    /// The expression could not be read — a visible fault, never a silent yes or no.
    Unreadable(String),
}

/// Read a power's `requires` against the build (see [`coh_data::pick_rules`]). This is the
/// whole of the prerequisite logic: intra-set prior-pick counts, mutual-exclusion locks and
/// archetype gates all live in that one expression, per power, per fork.
pub fn pick_gate(
    power: &coh_data::Power,
    state: &coh_data::CharacterState,
    archetype_id: Option<&str>,
    sets: &coh_data::SetPaths,
) -> PickGate {
    let expression = coh_data::granted_powers::requires(power).unwrap_or_default();
    // A hidden set-mechanic power (SHOWFLAGS-1) is handed out and taken back by its set's
    // own machinery — Seismic Pressure granting Shockwaves for 45s — so it is never bought,
    // whatever its level term says. `Granted` is the verdict that keeps it out of the pick
    // rows. An unreadable classification stays a visible row, never a silent re-listing.
    //
    // The hidden class alone does not settle it, and reading it that way was SHOWFLAGS-2.
    // `ShowInManage kFalse` means "no row on the enhancement Manage screen", which every
    // slotless power carries whether or not the game sells it — Bio Armor's Adaptation and
    // Staff Mastery are hidden there and still cost a pick. `free` is the axis that
    // separates them: the game's own tally of what a build has bought skips exactly the
    // powers it hands over.
    match power.mechanic_type() {
        Ok(Some(coh_data::MechanicType::HiddenPassive | coh_data::MechanicType::HiddenAuto))
            if power.is_free() || power.is_auto_issued() =>
        {
            return PickGate::Granted
        }
        Ok(_) => {}
        Err(error) => return PickGate::Unreadable(error),
    }
    // A granted power is never picked whichever way its gate resolves, and at any level —
    // the level term decides when the game hands it over, not whether it is ever bought. But
    // the gate must still READ: an unreadable grant gate is one the reconcile refused to
    // materialize on ([`coh_data::sync_granted_powers`]), and this row is where that failure
    // is visible rather than the power quietly vanishing from every surface (Rule 1).
    if power.is_auto_issued() {
        return match coh_data::requires_met(&expression, state, archetype_id, sets) {
            Ok(_) => PickGate::Granted,
            Err(error) => PickGate::Unreadable(error.to_string()),
        };
    }
    match coh_data::requires_met(&expression, state, archetype_id, sets) {
        Ok(true) => PickGate::Open,
        Ok(false) => PickGate::Closed,
        Err(error) => PickGate::Unreadable(error.to_string()),
    }
}

/// One set's worth of available (not-yet-picked) powers, as level-ordered numbered rows. The
/// wire orders a set's `powers` ascending by unlock level, so row order is the powers' own
/// order — no sort needed.
#[component]
fn AvailablePowersGroup(
    database: Db,
    set_id: String,
    origin: SetOrigin,
    /// Set for a VEAT branch set, which has no bucket of its own: the role list its picks
    /// belong to. `None` for every set the build actually holds.
    branch_role: Option<BuildRole>,
    /// Set for the build's own primary and secondary: the head is a
    /// [`PowersetSelect`](crate::panels::powerset_select::PowersetSelect) rather than a name,
    /// so the column that displays a set is the one that chooses it (LAY4). `None` for pools,
    /// the epic pool and the branch offers, none of which is chosen this way.
    select_role: Option<BuildRole>,
    /// Why nothing in this set can be taken yet, when the set-level gate refuses it.
    set_blocked: Option<String>,
    current_level: u8,
    taken_levels: Vec<u8>,
    /// Every power the build holds, with the level it was taken at — build state, not a
    /// prerequisite verdict, which is why it rides beside [`PickGate`] rather than inside it.
    picked_levels: Vec<(String, u8)>,
) -> Element {
    let session = use_context::<BuildSession>();
    let build = session.build;
    let mut collapsed = use_signal(|| false);

    let set_name = match origin {
        SetOrigin::Powerset => database
            .find_powerset(&set_id)
            .map(|powerset| powerset.name.clone()),
        _ => database
            .pool_catalog
            .find(&set_id)
            .map(|pool| pool.name.clone()),
    }
    .unwrap_or_else(|| {
        // An unchosen powerset column (LAY4) has no id to fall back to, so the role names it —
        // this reaches the fold control's label, which has to say which column it folds.
        match (set_id.is_empty(), select_role) {
            (true, Some(BuildRole::Primary)) => "primary".to_string(),
            (true, Some(BuildRole::Secondary)) => "secondary".to_string(),
            _ => set_id.clone(),
        }
    });

    let archetype_id = build.read().archetype.id.clone();
    let unlock_floor = set_unlock_floor(&database, &set_id, origin);

    let bucket_holds_level_one = bucket_owns_level_one(&build.read(), &set_id, branch_role);

    // The set, whole. A power the build has picked keeps its row and reads as taken; one whose
    // prerequisites are unmet keeps its row and reads as gated — same reasoning for both, and
    // it is the panel's own: seeing WHY a power is out of reach is how a build gets planned
    // toward it, and "you already have it" is a reason like any other. Only a granted power is
    // absent, because it is not a pick at all and no amount of planning buys it.
    let rows: Vec<(coh_data::Power, PickGate, Option<u8>)> =
        powers_of_set(&database, &set_id, origin)
            .into_iter()
            .map(|power| {
                let gate = pick_gate(
                    &power,
                    &build.read(),
                    archetype_id.as_deref(),
                    &database.set_paths,
                );
                let picked_at = picked_levels
                    .iter()
                    .find(|(name, _)| name == power.ident())
                    .map(|(_, level)| *level);
                (power, gate, picked_at)
            })
            .filter(|(_, gate, _)| *gate != PickGate::Granted)
            .collect();
    // The count says how much of the set the build holds. It used to be how many rows were
    // still takeable, which was the same sentence as the list's own length; now that the list
    // is the whole set, the length says that and the count has to say something else.
    let picked_count = rows.iter().filter(|(_, _, at)| at.is_some()).count();
    let row_count = rows.len();

    let removable = matches!(origin, SetOrigin::Pool | SetOrigin::Epic);

    rsx! {
        section { class: "powerset-group",
            header { class: "powerset-group__header",
                // Two heads, and the difference is only what names the set. A pool names itself
                // in a label that also folds the group; a powerset names itself in the select
                // that chooses it (LAY4), which cannot sit inside a button — so the caret
                // becomes its own small target and the count moves out beside it.
                //
                // The remove control is its own button either way, never nested in the fold,
                // because dropping a pool discards its picked powers and must not be reachable
                // by a stray click on the fold.
                if let Some(role) = select_role {
                    button {
                        class: if collapsed() {
                            "powerset-group__fold caret--folded"
                        } else {
                            "powerset-group__fold"
                        },
                        "aria-expanded": !collapsed(),
                        "aria-label": "Fold {set_name}",
                        onclick: move |_| collapsed.toggle(),
                        "▼"
                    }
                    crate::panels::powerset_select::PowersetSelect {
                        database: database.clone(),
                        role,
                        class: "powerset-group__select".to_string(),
                        placeholder: match role {
                            BuildRole::Primary => "— primary —".to_string(),
                            BuildRole::Secondary => "— secondary —".to_string(),
                        },
                    }
                    if !set_id.is_empty() {
                        span {
                            class: "powerset-group__count",
                            title: "{picked_count} of {row_count} picked",
                            "{picked_count}/{row_count}"
                        }
                    }
                } else {
                    button {
                        class: "powerset-group__toggle",
                        "aria-expanded": !collapsed(),
                        onclick: move |_| collapsed.toggle(),
                        span {
                            class: if collapsed() {
                                "powerset-group__caret caret--folded"
                            } else {
                                "powerset-group__caret"
                            },
                            "▼"
                        }
                        span { class: "powerset-group__name", "{set_name}" }
                        span {
                            class: "powerset-group__count",
                            title: "{picked_count} of {row_count} picked",
                            "{picked_count}/{row_count}"
                        }
                    }
                }
                if removable {
                    button {
                        class: "powerset-group__remove",
                        "aria-label": "Remove {set_name} and its picked powers",
                        title: "Remove {set_name} and its picked powers",
                        onclick: {
                            let set_id = set_id.clone();
                            let database = database.clone();
                            move |_| {
                                let set_id = set_id.clone();
                                let database = database.clone();
                                session.commit(move |state| {
                                    match origin {
                                        SetOrigin::Epic => remove_epic_pool(state),
                                        _ => remove_pool(state, &set_id),
                                    }
                                    crate::granted_powers::sync(state, &database);
                                });
                            }
                        },
                        "✕"
                    }
                }
            }
            if !collapsed() && rows.is_empty() {
                // Three empties, and they are different facts. No set chosen is the LAY4 case
                // — the head above is the fix, so the body says so and points at nothing else.
                // A blocker names a set the build cannot buy from. Otherwise the list is the
                // whole set (LAY3), so an empty one holds nothing buyable at all — never "you
                // have them all", which is what this said before the list stopped shrinking.
                p { class: "hint",
                    if set_id.is_empty() {
                        "Choose a set above."
                    } else if let Some(reason) = set_blocked.clone() {
                        "{reason}"
                    } else {
                        "Nothing in this set is picked, not granted."
                    }
                }
            }
            if !collapsed() && !rows.is_empty() {
                div { class: "power-rows",
                    for (power, gate, picked_at) in rows.iter() {
                        AvailablePowerRow {
                            key: "{power.ident()}",
                            database: database.clone(),
                            power: power.clone(),
                            set_id: set_id.clone(),
                            gate: gate.clone(),
                            picked_at: *picked_at,
                            branch_role,
                            set_blocked: set_blocked.clone(),
                            unlock_floor: if bucket_holds_level_one {
                                unlock_floor.max(2)
                            } else {
                                unlock_floor
                            },
                            current_level,
                            taken_levels: taken_levels.clone(),
                        }
                    }
                }
            }
        }
    }
}

/// Everything an available row's verdict depends on. Grouped because the answer is a
/// precedence over all of them, and a five-argument call reads as five unrelated facts.
struct RowInputs<'a> {
    power_name: &'a str,
    unlock_level: u8,
    /// The level the build already took this power at, when it holds it.
    picked_at: Option<u8>,
    /// Why the whole SET refuses this pick, when it does.
    set_blocked: Option<&'a str>,
    /// What this one power's own `requires` says.
    gate: &'a PickGate,
    /// The level this pick would land at, or `None` when the build has no pick left for it.
    pick_level: Option<u8>,
    /// The level a Level Up-mode build would have to reach first.
    level_gated_pick: Option<u8>,
}

/// What a click on an available row does.
///
/// An action rather than a permission, because since LAY3 stopped the rail hiding picked
/// powers there are two of them: a row is a toggle, and "can this be clicked" no longer says
/// which way. `Copy` so one value can both grade the row's class and move into its handler.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum RowAction {
    /// Take it, at this level — the earliest pick the schedule still has free at or above the
    /// power's unlock level.
    Pick(u8),
    /// Give it back. The same row that took the power returns it, which is what makes the
    /// rail a place you can undo a pick from and not only add to.
    Drop,
    /// Nothing happens, and the row's title says why.
    Refused,
}

/// What an available row's click does, and what the row says about itself.
///
/// Lives outside the component because the answer is a PRECEDENCE and precedence is where
/// this goes wrong: already holding the power outranks everything (nothing else is worth
/// saying about a row the build has already bought), then a set-level refusal outranks the
/// power's own gate (the power is fine, the set is not sold), which in turn outranks having no
/// pick left. Inside `rsx!` none of that could be graded.
fn row_verdict(inputs: RowInputs<'_>) -> (RowAction, String) {
    let RowInputs {
        power_name,
        unlock_level,
        picked_at,
        set_blocked,
        gate,
        pick_level,
        level_gated_pick,
    } = inputs;
    // Picked leads. A held power's set-level blocker and its own prerequisite are both answers
    // to a question nobody is asking any more, and a row saying "needs prerequisites" about a
    // power on the build's own card list is simply false.
    //
    // It leads as an ACTION for the same reason, and the precedence is load-bearing twice
    // over: the only thing left to do with a power the build already holds is give it back,
    // and neither louder reason can refuse that. A set the build may no longer buy from, or a
    // prerequisite that has since closed behind an exclusive pick, is a reason not to SELL a
    // power — never a reason to make one unreturnable. Graded the other way, a pick taken
    // under a rule that later changed would be stuck on the surface that made it.
    if let Some(level) = picked_at {
        return (
            RowAction::Drop,
            format!(
                "{power_name} — picked at level {level}. Click to give it back; \
                 its slots and enhancements go with it."
            ),
        );
    }
    if let Some(reason) = set_blocked {
        return (RowAction::Refused, reason.to_string());
    }
    // The same three conditions the row has always had to meet to be bought, now naming the
    // level they agree on rather than answering yes: the handler needs the pick level, and
    // resolving it here is what keeps the row from re-deciding it a second way.
    let action = match (gate, pick_level, level_gated_pick) {
        (PickGate::Open, Some(level), None) => RowAction::Pick(level),
        _ => RowAction::Refused,
    };
    let title = match (gate, pick_level, level_gated_pick) {
        (PickGate::Closed, _, _) => {
            format!("{power_name} needs prerequisites this build hasn't taken yet")
        }
        (_, _, Some(level)) => format!(
            "This pick belongs to level {level} — advance to {level} to take it, \
             or turn off Level Up mode to plan ahead"
        ),
        (_, Some(level), _) => {
            format!("Unlocks at level {unlock_level} — takes the level {level} pick")
        }
        (_, None, _) => format!("No power pick left at or above level {unlock_level}"),
    };
    (action, title)
}

/// A single available-power row — the beta `PowerItem`: a level badge (the unlock level),
/// an icon, and the power name. Clicking picks the power into its set, at the level the game
/// would grant that pick; clicking a row the build already holds gives the power back.
///
/// The row is a toggle because since LAY3 it no longer leaves the list when it is taken, and a
/// row that stays put while the only control that undoes it lives on another panel is a dead
/// end for the one gesture a reader will actually try. Both surfaces built out of this
/// component get it at once — the Available rail's powersets and the Pools panel's lists.
#[component]
fn AvailablePowerRow(
    database: Db,
    power: coh_data::Power,
    set_id: String,
    /// What the power's own `requires` says about taking it now.
    gate: PickGate,
    /// The level the build took this power at, when it holds it. Distinct from `gate`, which
    /// only ever speaks for the game's prerequisite expression — being picked is build state.
    picked_at: Option<u8>,
    /// Set for a VEAT branch set: the role list this pick lands in, since no bucket names it.
    branch_role: Option<BuildRole>,
    /// Why the SET refuses every pick from it, when it does — distinct from `gate`, which is
    /// this one power's own prerequisite.
    set_blocked: Option<String>,
    /// The floor the owning set puts under this power's unlock level (pools unlock as a
    /// whole before any power in them does).
    unlock_floor: u8,
    /// The build's current level — a power unlocking above it is still pickable (you plan the
    /// whole build ahead), so this only tints the badge.
    current_level: u8,
    /// The pick levels the build has already spent, for resolving this row's own pick level.
    taken_levels: Vec<u8>,
) -> Element {
    let session = use_context::<BuildSession>();
    let power_name = power.name.clone();
    let power_internal = power.ident().to_string();
    let unlock_level = power.unlock_level().max(unlock_floor);

    // The Info panel follows this row too (beta `AvailablePowers` `handlePowerHover`): hovering
    // shows the power before it is picked, unslotted but with the build's globals, which is what
    // `projection_for` already produces for a power the build does not hold. Right-click locks
    // it there. Sticky like the picked card: nothing clears it on leave.
    let mut selection = use_context::<crate::shell::PowerSelection>().0;
    let info_lock = use_context::<crate::shell::InfoLock>();
    let this_power = crate::shell::PowerRef {
        powerset_id: set_id.clone(),
        power: power_internal.clone(),
    };
    let info_locked = info_lock.holds(&set_id, &power_internal);
    let on_hover = {
        let this_power = this_power.clone();
        move |_| selection.set(Some(this_power.clone()))
    };
    let on_lock = {
        let this_power = this_power.clone();
        move |evt: Event<MouseData>| {
            evt.prevent_default();
            info_lock.toggle_on(this_power.clone());
        }
    };

    // The row's art, from the def's `icon` (rides in `Power.extra`, the same field
    // [`PickedPowerCard`] reads) — so one power wears one picture on both sides of the app. A
    // def with no icon resolves to `Unknown.png` rather than a broken image (Rule 1).
    let icon_url = crate::view::icons::power_icon_url(
        power.extra.get("icon").and_then(|value| value.as_str()),
    );

    // The level this pick lands at — the earliest of the schedule's power picks still free at
    // or above the power's unlock level. `None` means the build has no such pick left, so the
    // row disables: a picked power carries the level the game granted it, never a substituted
    // one (a dataset with no schedule yields `None` too — fail loud, like the slot budget).
    let pick_level = database
        .leveling_schedule
        .as_ref()
        .and_then(|schedule| schedule.next_pick_level(&taken_levels, unlock_level));

    // A power that unlocks above the build's current level is still pickable — you plan the
    // whole build ahead of time — so `locked` only tints the badge to say "not in-game yet",
    // it never disables the row. At the default level 50 nothing is locked.
    let locked = unlock_level > current_level;

    // …unless level-up mode is on, where planning ahead is exactly what the mode exists to
    // prevent.
    let level_up_mode = use_context::<crate::level_control::LevelUpMode>().0;
    let level_gated_pick =
        crate::level_control::pick_beyond_level(level_up_mode(), pick_level, current_level);

    let (action, title) = row_verdict(RowInputs {
        power_name: &power_name,
        unlock_level,
        picked_at,
        set_blocked: set_blocked.as_deref(),
        gate: &gate,
        pick_level,
        level_gated_pick,
    });

    // An unreadable prerequisite is its own state: the row shows the fault and refuses the
    // pick rather than guessing which way the expression would have gone.
    // It outranks `is-picked` rather than being swallowed by it — a build that holds a power
    // whose `requires` cannot be read still has an unreadable `requires` to report, and it is
    // the one picked row that is not a give-back toggle: a row whose job this render is to
    // report a fault should not also be a button. The pick is still removable from its card on
    // the Powers panel, which is where removal lived before this row offered it at all.
    if let PickGate::Unreadable(error) = &gate {
        let faulted_class = if picked_at.is_some() {
            "power-row is-faulted is-picked"
        } else {
            "power-row is-faulted"
        };
        let faulted_class = if info_locked {
            format!("{faulted_class} is-info-locked")
        } else {
            faulted_class.to_string()
        };
        return rsx! {
            div {
                class: faulted_class,
                title: "{error}",
                onmouseenter: on_hover,
                oncontextmenu: on_lock,
                span { class: "power-row__level", "!" }
                span { class: "power-row__icon", "⚠" }
                span { class: "power-row__name", "{power_name}" }
            }
        };
    }

    // Picked leads for the same reason it leads in `row_verdict`: it is the one state that
    // answers the row completely, and the others describe a purchase that already happened.
    let row_class = match (
        picked_at.is_some(),
        matches!(action, RowAction::Pick(_)),
        locked,
        &gate,
    ) {
        (true, _, _, _) => "power-row is-picked",
        (_, true, true, _) => "power-row is-locked",
        (_, true, false, _) => "power-row",
        (_, false, _, PickGate::Closed) => "power-row is-gated",
        _ => "power-row is-unpickable",
    };
    let row_class = if info_locked {
        format!("{row_class} is-info-locked")
    } else {
        row_class.to_string()
    };
    rsx! {
        button {
            class: row_class,
            title: "{title}",
            // `aria-disabled`, not `disabled`: a disabled button gets no pointer events in some
            // engines, so a refused row could never be hovered into the Info panel or
            // right-clicked to lock — and a refused power is exactly one a player wants to read
            // about. The click already does nothing for `Refused`, so only the attribute moves.
            "aria-disabled": action == RowAction::Refused,
            onmouseenter: on_hover,
            oncontextmenu: on_lock,
            onclick: {
                let set_id = set_id.clone();
                let power_internal = power_internal.clone();
                let database = database.clone();
                move |_| {
                    let set_id = set_id.clone();
                    let power_internal = power_internal.clone();
                    let database = database.clone();
                    match action {
                        RowAction::Pick(level) => session.commit(move |state| {
                            add_power(state, &set_id, &power_internal, level, branch_role);
                            // The pick may be another power's grant gate; the grant lands in
                            // the same edit, so one user action is one undo step.
                            crate::granted_powers::sync(state, &database);
                        }),
                        // Addressed by THIS ROW'S set, not by the name alone: `internalName`
                        // collides across archetypes (Build_Up appears ×64), so a name-only
                        // removal is a removal from whichever bucket matches first. The row
                        // knows its set, and a branch pick that no bucket claims carries the
                        // set on itself — `remove_selected_power` finds it either way.
                        //
                        // Dropping a pick takes its slots and its enhancements with it, which
                        // is the whole reason this was kept off a one-click target before. It
                        // commits as ONE step, and that is what makes the gesture affordable:
                        // the same edit that removes the power takes back anything it was
                        // granting, so a single undo restores the lot.
                        RowAction::Drop => session.commit(move |state| {
                            remove_selected_power(state, &set_id, &power_internal);
                            crate::granted_powers::sync(state, &database);
                        }),
                        RowAction::Refused => {}
                    }
                }
            },
            // The badge is the level THIS row is at, and a picked row is at the level the build
            // took it. Not its unlock level: that is floored by `unlock_floor` into the level a
            // future pick would land at, so a power taken at 1 out of a set holding the level-1
            // pick would read 2 — a prospective number about a purchase that already happened,
            // and one the Powers panel contradicts on the same screen.
            span { class: "power-row__level",
                if let Some(level) = picked_at { "{level}" } else { "{unlock_level}" }
            }
            // The chip holds the power's own art, so a row is recognizable before it is read —
            // the same picture the picked card and the pinned bar show.
            //
            // Nothing is composited on top of it. The art briefly wore a corner tick to say
            // "held", and a tick that has to read against 2,991 different pictures is a badge
            // fighting the one thing the chip is for. The state moved to the level badge
            // instead (see `.power-row.is-picked` in app.css), a surface the app owns outright,
            // already the place the eye goes for a level, and one that carries the difference
            // in the number itself: a picked row's badge is the level the build TOOK the power
            // at, where every other row's is a level it might.
            span { class: "power-row__icon",
                img { class: "power-row__art", src: "{icon_url}", alt: "" }
            }
            span { class: "power-row__name", "{power_name}" }
        }
    }
}

/// Picked powers group for primary powerset.
#[component]
fn PickedPowersGroupPrimary(database: Db) -> Element {
    let session = use_context::<BuildSession>();
    let build = session.build;
    let primary = build.read().primary.clone();
    let bucket_id = primary.id.clone().unwrap_or_default();
    let db = database.clone();

    rsx! {
        PowerGroup { title: "Primary".to_string(), count: primary.powers.len(),
            for (i, power) in primary.powers.iter().enumerate() {
                PickedPowerCard {
                    key: "primary-{i}",
                    database: db.clone(),
                    power: power.clone(),
                    powerset_id: owning_set(power, &bucket_id),
                }
            }
        }
    }
}

/// The set a pick belongs to: its own, falling back to the list it sits in.
///
/// The two differ for a VEAT branch pick, which lives in a base role's list under the branch
/// set's id — addressing it by the list's set would look up its definition in the wrong set
/// and remove it from nothing.
fn owning_set(power: &coh_data::SelectedPower, bucket_id: &str) -> String {
    if power.powerset.is_empty() {
        bucket_id.to_string()
    } else {
        power.powerset.clone()
    }
}

/// Picked powers group for secondary powerset.
#[component]
fn PickedPowersGroupSecondary(database: Db) -> Element {
    let session = use_context::<BuildSession>();
    let build = session.build;
    let secondary = build.read().secondary.clone();
    let bucket_id = secondary.id.clone().unwrap_or_default();
    let db = database.clone();

    rsx! {
        PowerGroup { title: "Secondary".to_string(), count: secondary.powers.len(),
            for (i, power) in secondary.powers.iter().enumerate() {
                PickedPowerCard {
                    key: "secondary-{i}",
                    database: db.clone(),
                    power: power.clone(),
                    powerset_id: owning_set(power, &bucket_id),
                }
            }
        }
    }
}

/// Picked powers group for a pool.
#[component]
fn PickedPowersGroupPool(database: Db, pool: coh_data::PoolSelection) -> Element {
    let powerset_id = pool.id.clone();
    let pool_name = crate::naming::pool_name(&pool.id, &database);
    let db = database.clone();

    rsx! {
        PowerGroup { title: pool_name, count: pool.powers.len(),
            for (i, power) in pool.powers.iter().enumerate() {
                PickedPowerCard {
                    key: "pool-power-{i}",
                    database: db.clone(),
                    power: power.clone(),
                    powerset_id: powerset_id.clone(),
                }
            }
        }
    }
}

/// The power-gated grants as their own section — the by-level ladder's counterpart to the
/// inherents section. The ladder has one cell per pick and a grant spends none, so this is
/// the one arrangement where grants need a surface of their own; the by-powerset tracks
/// already show each grant inside the group that granted it. Renders nothing while the
/// build holds no grant.
#[component]
fn GrantedPowersSection(database: Db) -> Element {
    let session = use_context::<BuildSession>();
    let grants: Vec<coh_data::SelectedPower> = session
        .build
        .read()
        .all_selected()
        .filter(|power| power.is_locked && power.inherent_category.is_none())
        .cloned()
        .collect();
    if grants.is_empty() {
        return rsx! {};
    }

    rsx! {
        div { class: "power-columns",
            PowerGroup { title: "Granted".to_string(), count: grants.len(),
                for (i, power) in grants.iter().enumerate() {
                    PickedPowerCard {
                        key: "granted-{i}",
                        database: database.clone(),
                        power: power.clone(),
                        powerset_id: power.powerset.clone(),
                    }
                }
            }
        }
    }
}

/// Picked powers group for epic pool.
#[component]
fn PickedPowersGroupEpic(database: Db, pool: coh_data::PoolSelection) -> Element {
    let powerset_id = pool.id.clone();
    let pool_name = crate::naming::pool_name(&pool.id, &database);
    let db = database.clone();

    rsx! {
        PowerGroup { title: pool_name, count: pool.powers.len(),
            for (i, power) in pool.powers.iter().enumerate() {
                PickedPowerCard {
                    key: "epic-power-{i}",
                    database: db.clone(),
                    power: power.clone(),
                    powerset_id: powerset_id.clone(),
                }
            }
        }
    }
}

/// Below this much travel a press is a tap (`+1`), not a drag.
const SLOT_DRAG_THRESHOLD_PX: f64 = 4.0;

/// Arm a slot drag in the DOM, and report the pixels of handle travel that step the slot count
/// by one. Two jobs in one round trip because both are the same DOM visit and both have to
/// happen at the press:
///
/// - **Pointer capture** on the pressed handle, so `pointermove`/`pointerup` keep arriving after
///   the pointer leaves an 18px target. Without it a drag dies a few pixels in.
/// - **The step**, measured off the live capsule rather than pinned, because both halves of that
///   pitch are stylesheet values that move — `--slot-size` is 24px for a mouse and 34px under
///   `(pointer: coarse)`. A constant would be right for one pointer and ten pixels per slot wrong
///   for the other, which is the pointer the drag exists for.
///
/// Done through `eval` — the DOM call this repo already makes from both platforms (see
/// [`crate::build_store`]) — and not through `web-sys`, which is a browser-only door. Behind it
/// the desktop webview got neither half: the capture was a compiled-out no-op and the step was a
/// permanent `None`, which pins `steps` at zero for the whole gesture. The drag armed, lit the
/// capsule, and then counted nothing however far it travelled; only the tap survived.
///
/// The step is the distance BETWEEN the first two cells, not one cell's width: the flex row puts
/// a gap after each cell and that gap is an eighth of the step. It measures CELLS, never the
/// handle — the two were the same width only for as long as the control was drawn at the slot
/// diameter, and `--slot-control` ended that.
///
/// `None` (an unrendered capsule, a wrapped row, a zero-width cell) leaves the drag pinned to a
/// tap rather than counting in a made-up step: a gesture that moves the wrong number of slots is
/// worse than one that only ever does what a tap would.
async fn arm_slot_drag(capsule_key: String, pointer_id: i32) -> Option<f64> {
    // Both layout roots are in the document at once (CSS shows one and hides the other), so this
    // key is on TWO capsules and the hidden one measures zero. Picking by rendered width is what
    // makes the reading come from the capsule the user actually pressed. Keyed on a `data-`
    // attribute rather than an `id` for the same reason — the duplicate is by design here.
    let js = format!(
        r#"
        const capsules = document.querySelectorAll('[data-slot-capsule=' + JSON.stringify({capsule_key:?}) + ']');
        const capsule = Array.from(capsules).find(c => c.getBoundingClientRect().width > 0);
        if (!capsule) return null;
        // Capture first: the measurement is allowed to fail, the gesture is not.
        try {{ capsule.querySelector('.slot-handle').setPointerCapture({pointer_id}); }} catch (_) {{}}
        const cells = capsule.querySelectorAll('.slot-capsule__cells .enhancement-slot');
        if (!cells.length) return null;
        const first = cells[0].getBoundingClientRect();
        if (cells.length > 1) {{
            const second = cells[1].getBoundingClientRect();
            // A card too narrow for one row wraps the cells, and the second one is then BELOW
            // the first, where the distance between them is no longer a horizontal step.
            if (second.x > first.x) return second.x - first.x;
        }}
        // A single-slot power has no second cell. Its width is the step minus the gap after it,
        // so the drag reaches its first slot a few pixels early — the harmless direction.
        return first.width;
        "#
    );
    document::eval(&js)
        .await
        .ok()?
        .as_f64()
        .filter(|step| *step > 0.0)
}

/// A live slot drag on one power card (the PC-DEFER-DRAG prototype, now bidirectional: press the
/// `‹›` handle and drag right to add slots, left to take them back). `delta` is the signed number
/// release will apply, clamped to `max_add` on one side and `max_remove` on the other, so no
/// gesture can overshoot the global budget, the power's own cap, or the base and inherent slots
/// that aren't the user's to remove. `moved` gates both the preview and the commit rule — a press
/// that never travelled is a tap, and a tap adds one. Card-local: each card owns its own signal,
/// so a drag on one card never previews on another.
#[derive(Clone, Copy, PartialEq)]
struct SlotDrag {
    origin_x: f64,
    /// The cell pitch this drag counts in, filled by [`arm_slot_drag`] once its DOM round trip
    /// lands. `None` until then, and permanently `None` if the pitch could not be read — the drag
    /// stays put rather than counting in a made-up step (see [`arm_slot_drag`]). Arriving a frame
    /// late costs nothing: `origin_x` is pinned at the press, so the first step the drag counts is
    /// measured from where the gesture actually began, not from where the pitch landed.
    step_px: Option<f64>,
    moved: bool,
    delta: isize,
    max_add: usize,
    max_remove: usize,
}

/// The count a mid-drag handle shows, signed the way the gesture reads: `+3` for three slots
/// coming, `−2` for two going. Built here rather than inlined so the sign and the magnitude are
/// decided in one place — `delta` is signed, and its `-` is a hyphen where the label wants the
/// same minus the rest of the card's chrome is drawn with.
fn slot_delta_label(delta: isize) -> String {
    if delta < 0 {
        format!("−{}", delta.unsigned_abs())
    } else {
        format!("+{delta}")
    }
}

/// Which slot indices a leftward drag of `delta` would take on release — the preview's half of a
/// claim [`remove_slot_from_power`] owns the other half of. That function pops from just before
/// the trailing inherent block, one slot per call, so a run of N removals takes the N cells
/// ending there. A non-negative `delta` yields an empty range rather than a special case.
///
/// Held as a function, rather than inlined where the cells are rendered, so the two halves can be
/// put against each other in a test: a preview that marks a different cell than the one that goes
/// is a lie no amount of looking at the screen will catch, because both look correct alone.
fn doomed_slot_range(slot_count: usize, inherent_slots: usize, delta: isize) -> Range<usize> {
    let user_slot_end = slot_count.saturating_sub(inherent_slots);
    user_slot_end.saturating_sub(delta.min(0).unsigned_abs())..user_slot_end
}

/// A single picked power card — beta layout (top: level, name, toggle, tools, remove; bottom: icon, slots).
/// The one card both [`PowersLayout`]s render, so a power is identical in either arrangement.
#[component]
pub fn PickedPowerCard(
    database: Db,
    power: coh_data::SelectedPower,
    powerset_id: String,
    /// Stands in for the `L{level}` badge — the by-level grid passes a drag handle here.
    level_badge: Option<Element>,
) -> Element {
    let session = use_context::<BuildSession>();
    let power_id = power.internal_name.clone();

    // Slot affordances gate on BOTH the per-power cap and the build-wide level-gated
    // budget (the beta `slotRemaining`). The budget reads the live build (so every
    // card's stepper stops travelling right the instant the pool is spent anywhere) and
    // the dataset's sourced `LevelingSchedule`; a dataset without a schedule blocks adds
    // (fail-loud — never silently unbounded). Removal frees a budget slot and needs only
    // a user-placed slot to remove.
    // The power definition, resolved from whichever partition holds it. Read up front: the
    // slot affordances below are gated on what the def says this power accepts.
    let power_def = resolve_power_def(&database, &powerset_id, &power_id);

    let inherent_slots = power.inherent_slot_count as usize;
    let slot_budget = {
        let build = session.build.read();
        database
            .leveling_schedule
            .as_ref()
            .map(|schedule| schedule.total_slots_at_level(build.level))
    };
    let slots_used = coh_data::placed_budget_slots(&session.build.read());
    // How many slots this power can still gain: the smaller of its per-power room and the
    // remaining global budget. The rightward half of the stepper drag's clamp — a dataset
    // without a schedule yields 0 (fail-loud: no unbounded adds).
    //
    // The per-power ceiling is the power's OWN `maxSlots` where the export states one, not the
    // flat six: several powers accept four, and the ones the game grants without slotting
    // accept none. `MAX_USER_SLOTS_PER_POWER` remains the fallback for a power whose wire
    // omits the field, and the structural cap on anything the picker can reach.
    let stated_cap = power_def
        .and_then(|def| def.max_slots())
        .map_or(coh_data::MAX_USER_SLOTS_PER_POWER, |cap| {
            usize::from(cap).min(coh_data::MAX_USER_SLOTS_PER_POWER)
        });
    // A power that accepts no enhancement category takes no slots at all, whatever its stated
    // cap — which is how the archetype inherents are marked (an explicit empty
    // `allowedEnhancements`, their `maxSlots` being absent rather than zero).
    let slottable = power_def.is_none_or(|def| def.accepts_enhancements());
    let per_power_room = if slottable {
        (stated_cap + inherent_slots).saturating_sub(power.slots.len())
    } else {
        0
    };
    let budget_room = slot_budget.map_or(0, |budget| budget.saturating_sub(slots_used));
    let max_add = per_power_room.min(budget_room);
    // The leftward half. Only the band between the free base slot and the trailing inherent
    // auto-slots is the user's to give back, which is the same band `remove_slot_from_power`
    // pops from, one slot per call.
    let max_remove = power.slots.len().saturating_sub(1 + inherent_slots);
    // How many of this power's slots hold a piece — the bulk clear's only gate, and the
    // count that decides whether it is offered at all.
    let filled_slots = power.slots.iter().flatten().count();
    let slot_budget = slot_budget.unwrap_or(0);
    // Every slot edit keeps the build's slot levels. Level-up mode adds one rule: a slot the
    // schedule has no grant for is refused rather than placed and marked.
    let leveling_on = *use_context::<crate::level_control::LevelUpMode>().0.read();
    let slot_level_view = use_context::<SlotLevelsView>().0;
    let power_slot_levels: Option<Vec<Option<u8>>> = slot_level_view().and_then(|all| {
        let category = SlotCategory::of(&session.build.read(), &powerset_id, &power_id)?;
        all.get(&(category, power_id.clone())).cloned()
    });
    // The live slot drag on this card (card-local; `None` when no drag is in flight).
    let mut slot_drag = use_signal(|| Option::<SlotDrag>::None);
    // What an in-flight drag will do on release: positive slots to add, previewed as ghost
    // circles after the last cell; negative to remove, previewed by marking the cells that would
    // go. Zero while nothing is dragging, and zero until the press passes the tap threshold.
    let pending_delta = slot_drag()
        .filter(|drag| drag.moved)
        .map_or(0, |drag| drag.delta);
    let doomed = doomed_slot_range(power.slots.len(), inherent_slots, pending_delta);
    // Names this card's slot capsule for [`arm_slot_drag`] to find. The power's identity, because
    // that is what is unique across the whole build — a card renders once per layout root, and the
    // drag has to reach the copy that is on screen rather than either copy in particular.
    let capsule_key = format!("{powerset_id}/{power_id}");

    // The card icon derives from the def's `icon` (rides in `Power.extra`, like `shortHelp`);
    // an unresolved def or missing name falls back to `Unknown.png` (Rule 1, no broken image).
    let power_icon_url = crate::view::icons::power_icon_url(
        power_def
            .and_then(|def| def.extra.get("icon"))
            .and_then(|v| v.as_str()),
    );

    // Only powers that land a persistent effect on the caster get an ON/OFF pill (beta
    // `shouldShowToggle`); a pure attack shows none. A power whose def can't be resolved shows
    // no pill — the gate is undecidable without its `effects` bag.
    //
    // Which switch the pill throws depends on where that effect lives, and the two are
    // partitioned so one pill never carries two meanings. A power that buffs the caster
    // directly is switched by its own `is_active`. A power whose only caster-facing payload is
    // the aura of something it summons has nothing on itself to switch: the fold is gated on
    // the per-pet opt-in instead, so the pill writes THAT key — the same one the Info panel's
    // adjuster row writes, so the two surfaces cannot disagree. Both sides are read off the
    // export (Rule 0).
    let pill = if power_def.is_some_and(|def| {
        // The toggle gate reads the atom router's slot projection (the authored `effects` bag is
        // leaving the contract), so the roster of class tokens the unanimous-fork pass needs is
        // derived from the dataset here and threaded in.
        // Hold the catalog alive for the lifetime of the `&str` tokens below.
        //
        // A dataset that will not parse shows NO PILL rather than taking the process with it -
        // The `expect` here was the odd one out: every other caller of
        // `archetypes()` in this crate already handles the error (`powers.rs:773` with this same
        // `let Ok(..) else`, `inherents.rs:80` with `.ok()?`, `powerset_compare.rs:584`). And the
        // fallback is not invented for the occasion - it is what this block's own comment above
        // already prescribes for the neighbouring undecidable case: "a power whose def can't be
        // resolved shows no pill - the gate is undecidable without its `effects` bag."
        let Ok(catalog) = database.archetypes() else {
            return false;
        };
        let classes: Vec<&str> = catalog
            .all()
            .iter()
            .filter_map(|at| database.class_name_of(&at.id))
            .collect();
        let build = session.build.read();
        // The caster state the build has declared counts too. A finisher's self-buff is a
        // conditional row (Sky Splitter's resistance under a finished Perfection of Body), gated
        // out of the base projection, so without this a build in that form could see the
        // finisher's damage move and never switch its buff into the totals.
        let declared = coh_math::window_slots::slots_over(def, build.dataset, |atom| {
            atom.conditional_id
                .as_deref()
                .is_some_and(|id| build.combat.global_conditionals.get(id) == Some(&true))
        });
        should_show_toggle(
            def,
            &coh_math::window_slots::bag_slots(def, build.dataset, &classes),
        ) || should_show_toggle(def, &declared)
    }) {
        Some(PowerPill::Active)
    } else if power_def
        .is_some_and(|def| !coh_math::buff_pets::buff_pet_sources(def, &database.0).is_empty())
    {
        Some(PowerPill::BuffPet(
            coh_math::buff_pets::buff_pet_toggle_key(&powerset_id, &power_id),
        ))
    } else {
        None
    };
    let pill_on = match &pill {
        Some(PowerPill::Active) => power.is_active,
        Some(PowerPill::BuffPet(key)) => session
            .build
            .read()
            .combat
            .power_state
            .get(key)
            .copied()
            .unwrap_or(false),
        None => false,
    };

    // Perma ring — only perma-eligible click powers (a self-buff/pet/self-penalty with an
    // achievable recharge/duration gap; beta `isPermaEligible`) get a ring around the icon, filled
    // by how close the build is to keeping the power up permanently. The slotted-recharge and
    // global-recharge inputs come from the same engine seams the stats panels read — the per-power
    // `power_enhancement` (Alpha split included) and the shared `BuildTotals` memo — so the ring can
    // never disagree with the dashboard. Both reads sit inside the eligibility guard, so an
    // ineligible card subscribes to neither and never re-renders on an unrelated build edit.
    let build_totals = use_context::<crate::panels::stats::BuildTotals>().0;
    // Whether perma is reachable at all is measured against the archetype's recharge ceiling, so
    // that bound is needed BEFORE the eligibility guard. Held as a memo rather than read inline so
    // the guard keeps its purpose: it re-runs on any build edit but only wakes this card when the
    // archetype — and so the bound — actually changes.
    let clamp_db = database.clone();
    let recharge_clamp = use_memo(move || {
        coh_math::projection::ReductionClamps::for_build(&session.build.read(), &clamp_db.0)
            .recharge
    });
    let perma = power_def
        .filter(|def| {
            coh_math::perma::is_perma_eligible(def, recharge_clamp(), session.build.read().dataset)
        })
        .and_then(|def| {
            let state = session.build.read();
            let mut errors = Vec::new();
            let alpha = coh_math::incarnates::alpha_enhancement(
                &state.incarnates,
                state.combat.exemplar_level,
                &database.0,
            );
            let enhancement = coh_math::apply::power_enhancement(
                &power.slots,
                def,
                &alpha,
                state.level as i32,
                &state.combat,
                &database.0,
                &mut errors,
            );
            let global_recharge = build_totals.read().stats.recharge / 100.0;
            coh_math::perma::calculate_perma_info(
                def,
                power.targets_hit,
                enhancement.get("recharge"),
                global_recharge,
                recharge_clamp(),
                state.dataset,
            )
        });

    // Hit Chance Alert — the engine's own per-power hit chance against the combat panel's
    // target, shown only on a power that rolls to hit a foe (accuracy alone can't say: self
    // toggles carry one too) and only under the 95% cap. The totals are read only while the
    // option is on, so an off option wakes no card on a build edit.
    let hit_alert_on = *use_context::<HitChanceAlert>().0.read();
    let hit_alert = if hit_alert_on && power_def.is_some_and(coh_data::Power::rolls_to_hit_foe) {
        build_totals
            .read()
            .power_projection
            .iter()
            .find(|p| p.power_set == powerset_id && p.power_internal_name == power_id)
            .and_then(|p| p.hit_chance)
            .filter(|hit| hit.chance < HIT_CAP - 1e-9)
    } else {
        None
    };

    // The stance this pick heads, if it heads one — derived from the export the same way the
    // Combat popover derives its list ([`coh_data::caster_state`]), so no set is named here.
    let stance =
        coh_data::stance_group_for(&session.build.read(), &database, &powerset_id, &power_id);

    // The ⌂ marker's tooltip. An inherent is granted by the game itself; a power-gated
    // grant names the pick holding it open, read from its own gate.
    let granted_marker_title = if power.inherent_category.is_some() {
        "Granted, not picked".to_string()
    } else {
        granted_title(&database, &powerset_id, power_def)
    };

    // Hovering the card drives the Info panel's `PowerView` to this power (beta
    // `setInfoPanelContent`) via the shared selection signal — sticky, matching the beta (no clear
    // on leave). The Info panel reads the same signal, so the projection stays single-source.
    let mut selection = use_context::<crate::shell::PowerSelection>().0;
    // Right-click locks the Info panel to this power (beta `onContextMenu` → `lockInfoPanel`).
    // The slot cells inside keep right-click for clearing a slot and stop it there, so a
    // right-click on a slot never also locks.
    let info_lock = use_context::<crate::shell::InfoLock>();
    let power_menu = use_context::<PowerMenu>().0;
    let mut tools_button = use_signal(|| Option::<std::rc::Rc<MountedData>>::None);
    let menu_open = power_menu.read().as_ref().is_some_and(|open| {
        open.powerset_id == powerset_id && open.power_internal_name == power_id
    });
    let card_class = if info_lock.holds(&powerset_id, &power_id) {
        "picked-power-card is-info-locked"
    } else {
        "picked-power-card"
    };

    rsx! {
        div { class: card_class,
            oncontextmenu: {
                let target = crate::shell::PowerRef {
                    powerset_id: powerset_id.clone(),
                    power: power_id.clone(),
                };
                move |evt: Event<MouseData>| {
                    evt.prevent_default();
                    info_lock.toggle_on(target.clone());
                }
            },
            onmouseenter: {
                let powerset_id = powerset_id.clone();
                let power_id = power_id.clone();
                move |_| {
                    selection.set(Some(crate::shell::PowerRef {
                        powerset_id: powerset_id.clone(),
                        power: power_id.clone(),
                    }));
                }
            },
            // Top row: level, name, toggle, tools, remove.
            div { class: "power-card-top",
                // A granted power has no pick level to show — `level` is 0 on one, and "L0"
                // would read as a level the game grants at.
                if power.is_locked {
                    span {
                        class: "power-level power-level--granted",
                        title: "{granted_marker_title}",
                        "⌂"
                    }
                } else if let Some(badge) = level_badge {
                    {badge}
                } else {
                    span { class: "power-level", "L{power.level}" }
                }
                span { class: "power-name", "{power_def.map(|p| p.name.as_str()).unwrap_or(&power_id)}" }
                if let Some(hit) = hit_alert {
                    span {
                        class: "hit-alert",
                        title: "{hit_alert_title(hit)}",
                        // Floored, so 94.9% never reads as the cap it is short of.
                        "{(hit.chance * 100.0).floor()}%"
                    }
                }

                // ON/OFF pill — rendered only for powers that buff the caster (beta
                // `shouldShowToggle`). The real checkbox stays for accessibility and the
                // `evt.checked()` commit path (see dioxus-checkbox-idiom); CSS hides the box and
                // styles the label as the pill (status dot + ON/OFF text).
                if let Some(pill) = pill.clone() {
                    label {
                        class: if pill_on { "power-toggle power-toggle--on" } else { "power-toggle" },
                        title: pill.title(pill_on),
                        input {
                            r#type: "checkbox",
                            class: "power-toggle-input",
                            checked: pill_on,
                            onchange: {
                                let power_id = power_id.clone();
                                let powerset_id = powerset_id.clone();
                                move |evt: Event<FormData>| {
                                    // Dioxus checkbox `value()` is "true"/"false", never "on";
                                    // read the boolean directly (see dioxus-checkbox-idiom).
                                    let checked = evt.checked();
                                    let power_id = power_id.clone();
                                    let powerset_id = powerset_id.clone();
                                    let pill = pill.clone();
                                    session.commit(move |state| match pill {
                                        PowerPill::Active => {
                                            if let Some(sp) = state.selected_power_mut(&powerset_id, &power_id) {
                                                sp.is_active = checked;
                                            }
                                        }
                                        PowerPill::BuffPet(key) => {
                                            state.combat.power_state.insert(key, checked);
                                        }
                                    });
                                }
                            },
                        }
                        span { class: "power-toggle-dot" }
                        span { class: "power-toggle-label", if pill_on { "ON" } else { "OFF" } }
                    }
                }

                PinToggle {
                    target: crate::shell::PowerRef {
                        powerset_id: powerset_id.clone(),
                        power: power_id.clone(),
                    },
                }

                // Opens this power's menu at the shell root (see `PowerMenuHost`). Anchored to the
                // button's own box rather than the click point, so Enter on a focused ⋮ opens it in
                // the same place a click does.
                button {
                    class: "power-tools",
                    r#type: "button",
                    title: "More for this power",
                    "aria-label": "More for this power",
                    "aria-haspopup": "menu",
                    "aria-expanded": menu_open,
                    onmounted: move |evt| tools_button.set(Some(evt.data())),
                    onclick: {
                        let power_id = power_id.clone();
                        let powerset_id = powerset_id.clone();
                        move |_| {
                            let power_id = power_id.clone();
                            let powerset_id = powerset_id.clone();
                            async move {
                                let mut menu = power_menu;
                                if menu_open {
                                    menu.set(None);
                                    return;
                                }
                                let Some(button) = tools_button.peek().clone() else {
                                    return;
                                };
                                let Ok(rect) = button.get_client_rect().await else {
                                    return;
                                };
                                menu.set(Some(PowerMenuTarget {
                                    power_internal_name: power_id,
                                    powerset_id,
                                    anchor_x: rect.max_x(),
                                    anchor_y: rect.max_y(),
                                }));
                            }
                        }
                    },
                    "⋮"
                }

                // Remove button — absent on a granted power, which cannot be given back.
                if !power.is_locked {
                    button {
                        class: "power-remove",
                        onclick: {
                            let power_id = power_id.clone();
                            let powerset_id = powerset_id.clone();
                            let database = database.clone();
                            move |_| {
                                let power_id = power_id.clone();
                                let powerset_id = powerset_id.clone();
                                let database = database.clone();
                                session.commit(move |state| {
                                    remove_selected_power(state, &powerset_id, &power_id);
                                    // Dropping an enabler takes its grants back in the same
                                    // undo step.
                                    crate::granted_powers::sync(state, &database);
                                });
                            }
                        },
                        "✕"
                    }
                }
            }

            // Bottom row: power icon, slots.
            div { class: "power-card-bottom",
                // A perma-eligible power wraps its icon in a progress ring; every other power
                // renders the bare icon. `--perma-percent` drives the conic fill; the `title`
                // carries the recharge/duration facts (the beta's hover tooltip, distilled).
                if let Some(perma) = perma {
                    div {
                        class: if perma.is_perma { "perma-ring perma-ring--perma" } else { "perma-ring" },
                        style: "--perma-percent: {perma.perma_percent};",
                        title: "{perma_title(&perma)}",
                        img { class: "power-icon", src: "{power_icon_url}", alt: "" }
                    }
                } else {
                    img { class: "power-icon", src: "{power_icon_url}", alt: "" }
                }

                // The slot capsule (PC-DEFER-DRAG prototype): a rounded container around the
                // power's slot cells + the stepper handle, giving the drag a visible track. Lights
                // up while a drag is armed (`--active`); the handle-hover highlight is pure CSS
                // (`:has`). Ghost circles preview the slots a drag will add, and marked cells the
                // ones it will take.
                div {
                    class: if slot_drag().is_some() { "slot-capsule slot-capsule--active" } else { "slot-capsule" },
                    // How `arm_slot_drag` finds this capsule to capture the pointer on and measure
                    // against. An attribute, not an `id`: both layout roots render every card, so
                    // the mark is on two elements at once and an `id` would be an invalid document.
                    "data-slot-capsule": "{capsule_key}",
                    // The cells, and only the cells, are what wraps when the card is too narrow
                    // to hold a full row. The controls below sit outside this box on purpose —
                    // see the note there.
                    div { class: "slot-capsule__cells",
                    for (i, slot) in power.slots.iter().enumerate() {
                        EnhancementSlot {
                            key: "slot-{i}",
                            slot_index: i as u8,
                            enhancement: slot.clone(),
                            // A slot is user-removable when it's past the free base slot (index 0)
                            // and before the trailing inherent auto-slots — the same band
                            // `remove_slot_at` enforces, mirrored here so the empty cell's
                            // right-click hint only shows on a slot it can actually remove.
                            removable_slot: i >= 1 && i + inherent_slots < power.slots.len(),
                            // Standing in the run a leftward drag would take back, so the cell
                            // shows what release costs before release commits it.
                            doomed: doomed.contains(&i),
                            slot_level: power_slot_levels.as_ref().and_then(|levels| levels.get(i)).map(|level| {
                                level.map_or(SlotLevelBadge::Unplaced, SlotLevelBadge::At)
                            }),
                            power_internal_name: power_id.clone(),
                            powerset_id: powerset_id.clone(),
                            database: database.clone(),
                        }
                    }
                    // Ghost previews: the slots this drag will add, shown only once it passes the
                    // tap threshold (a tap adds one with no preview). Inside the cells box, because
                    // a ghost stands where a real cell will, and has to wrap the way one would.
                    for ghost in 0..pending_delta.max(0) {
                        div { key: "ghost-{ghost}", class: "slot-ghost" }
                    }
                    }

                    // The control, held out of the wrapping flow. It is a DRAG handle, and a
                    // handle that can be re-flowed mid-gesture is not one: with the control in the
                    // same wrapping row as the cells, each ghost the drag added pushed the handle
                    // right until it wrapped, at which point it jumped a row down and back to the
                    // far left — out from under the pointer that was still holding it. The drag
                    // went on working (the pointer is captured), but every signal a user reads to
                    // know that had moved somewhere else, which is indistinguishable from broken.
                    div { class: "slot-capsule__controls",
                    // One stepper, not a `−`/`+` pair: the gesture is a single axis, so the
                    // control is a single ghost cell reading `‹›` and the direction of travel says
                    // which way. `pointerdown` arms a card-local drag and captures the pointer (so
                    // move/up keep coming after the pointer leaves the handle); `pointermove`
                    // walks the signed count by travel; `pointerup` commits the whole run in one
                    // undoable step. Rendered whenever EITHER direction has room, so a power at
                    // its cap can still give slots back.
                    if max_add >= 1 || max_remove >= 1 {
                        button {
                            class: "slot-stepper slot-handle",
                            title: "Drag right to add slots, left to remove",
                            "aria-label": "Add or remove slots on this power",
                            onpointerdown: {
                                let capsule_key = capsule_key.clone();
                                move |evt: Event<PointerData>| {
                                    evt.prevent_default();
                                    slot_drag.set(Some(SlotDrag {
                                        origin_x: evt.client_coordinates().x,
                                        step_px: None,
                                        moved: false,
                                        delta: 0,
                                        max_add,
                                        max_remove,
                                    }));
                                    // The DOM half of arming the drag — pointer capture and the
                                    // step — is a round trip, so it lands on the signal a beat
                                    // later. Returned as a future for Dioxus to drive rather than
                                    // handed to a bare `spawn`: spawning from inside a pointer
                                    // handler tore the wasm runtime down on the first press
                                    // ("memory access out of bounds"), and this is the same shape
                                    // the rest of the app's async handlers already use.
                                    let capsule_key = capsule_key.clone();
                                    let pointer_id = evt.pointer_id();
                                    async move {
                                        let step = arm_slot_drag(capsule_key, pointer_id).await;
                                        // Through `as_mut` so a gesture that is already over (a
                                        // fast tap) is not resurrected by its own measurement.
                                        slot_drag.with_mut(|active| {
                                            if let Some(active) = active.as_mut() {
                                                active.step_px = step;
                                            }
                                        });
                                    }
                                }
                            },
                            onpointermove: move |evt: Event<PointerData>| {
                                let x = evt.client_coordinates().x;
                                slot_drag.with_mut(|active| {
                                    let Some(active) = active.as_mut() else { return };
                                    let dx = x - active.origin_x;
                                    if !active.moved && dx.abs() > SLOT_DRAG_THRESHOLD_PX {
                                        active.moved = true;
                                    }
                                    // Truncation toward zero is what makes the two directions
                                    // symmetric: a full cell of travel has to be crossed either
                                    // way before the count leaves 0.
                                    let steps = match active.step_px {
                                        Some(step) => (dx / step) as isize,
                                        None => 0,
                                    };
                                    active.delta = steps.clamp(
                                        -(active.max_remove as isize),
                                        active.max_add as isize,
                                    );
                                });
                            },
                            onpointerup: {
                                let database = database.clone();
                                let power_id = power_id.clone();
                                let powerset_id = powerset_id.clone();
                                move |_evt| {
                                    // A press that never travelled is a tap, and a tap adds the one
                                    // slot the `+` it replaced added — unless there is no room to
                                    // add, in which case the handle is only here for the other
                                    // direction and a tap does nothing.
                                    let delta = slot_drag.peek().map_or(0, |drag| {
                                        if drag.moved {
                                            drag.delta
                                        } else if drag.max_add >= 1 {
                                            1
                                        } else {
                                            0
                                        }
                                    });
                                    slot_drag.set(None);
                                    if delta == 0 {
                                        return;
                                    }
                                    let power_id = power_id.clone();
                                    let powerset_id = powerset_id.clone();
                                    // One commit = one undo step for the whole drag; each step
                                    // re-checks its own bound (the budget and per-power cap on the
                                    // way up, the removable band on the way down), so an overshoot
                                    // can't exceed either.
                                    let database = database.clone();
                                    session.commit(move |state| {
                                        let leveling = database.leveling_schedule.as_ref();
                                        for _ in 0..delta.unsigned_abs() {
                                            if delta > 0 {
                                                add_slot_to_power(state, &powerset_id, &power_id, slot_budget, leveling, leveling_on);
                                            } else {
                                                remove_slot_from_power(state, &powerset_id, &power_id, leveling);
                                            }
                                        }
                                    });
                                }
                            },
                            onpointercancel: move |_| slot_drag.set(None),
                            // Losing capture ends the drag whatever took it — the handle can no
                            // longer see where the pointer goes, so there is no gesture left to
                            // finish. Without this the card keeps a preview it will never commit,
                            // and a stranded REMOVE preview reads as slots about to vanish. Fires
                            // after `pointerup` on a normal release too, where clearing an already
                            // cleared drag is a no-op.
                            onlostpointercapture: move |_| slot_drag.set(None),
                            onkeydown: {
                                let database = database.clone();
                                let power_id = power_id.clone();
                                let powerset_id = powerset_id.clone();
                                move |evt: Event<KeyboardData>| {
                                    // The drag's two directions, given keys: the arrows step the
                                    // way they point, and Enter/Space activate the control's
                                    // primary action (add), matching the tap. Any other key is
                                    // left alone so focus and typing behave normally.
                                    let add = matches!(evt.key(), Key::Enter | Key::ArrowRight)
                                        || matches!(evt.key(), Key::Character(c) if c == " ");
                                    let remove = matches!(evt.key(), Key::ArrowLeft);
                                    if !add && !remove {
                                        return;
                                    }
                                    evt.prevent_default();
                                    let power_id = power_id.clone();
                                    let powerset_id = powerset_id.clone();
                                    let database = database.clone();
                                    session.commit(move |state| {
                                        let leveling = database.leveling_schedule.as_ref();
                                        if add {
                                            add_slot_to_power(state, &powerset_id, &power_id, slot_budget, leveling, leveling_on);
                                        } else {
                                            remove_slot_from_power(state, &powerset_id, &power_id, leveling);
                                        }
                                    });
                                }
                            },
                            if pending_delta != 0 {
                                span {
                                    class: if pending_delta < 0 {
                                        "slot-handle-count slot-handle-count--remove"
                                    } else {
                                        "slot-handle-count"
                                    },
                                    "{slot_delta_label(pending_delta)}"
                                }
                            } else {
                                "‹›"
                            }
                        }
                    }
                    }
                }
                // The power-scoped bulk clear, the one act in the beta's slot context menu with
                // no road anywhere in this tree (its sibling, Remove All Extra Slots, is the
                // stepper dragged to its floor). Homed on the card because its scope IS the card:
                // the beta reached it from a slot but applied it to the power, and a per-power act
                // triggered from one of six slots is a scope the control cannot state.
                //
                // Outside the capsule, and past the handle. The capsule draws the frame the
                // stepper's drag acts on — it lights up along the cells being added or taken — so
                // a button inside that frame reads as part of that gesture, which this is not: it
                // empties the slots and leaves every one of them standing. Sitting it beyond the
                // handle also keeps the handle where a drag needs it, immediately against the
                // cells it re-counts, rather than one control further out.
                //
                // No confirm, deliberately. `crate::confirm`'s own rule is that an act committing
                // through `BuildSession::commit` is one Ctrl+Z away and does not earn a modal —
                // and the stepper beside it already destroys strictly more (slots AND whatever was
                // in them) on that footing. A confirm here and none there would be the surface
                // disagreeing with itself about the same build.
                if filled_slots >= 1 {
                    button {
                        class: "slot-clear-all",
                        r#type: "button",
                        title: "Empty every slot on this power. Undoable",
                        "aria-label": "Clear all enhancements on this power",
                        onclick: {
                            let power_id = power_id.clone();
                            let powerset_id = powerset_id.clone();
                            move |_| {
                                let power_id = power_id.clone();
                                let powerset_id = powerset_id.clone();
                                session.commit(move |state| {
                                    clear_all_enhancements(state, &powerset_id, &power_id);
                                });
                            }
                        },
                        "⌫"
                    }
                }
            }

            // The forms this power hands out (Swap Ammo's ammunition, Staff Mastery's forms, Bio
            // Armor's adaptations), chosen where the power is — the game issues them as powers of
            // their own under the parent, and the Combat popover's copy of this strip is not
            // where anyone looks for a power's sub-powers.
            if let Some(group) = stance {
                div { class: "power-stance",
                    crate::panels::combat::StanceChoices { group }
                }
            }
        }
    }
}

/// A single enhancement slot. Clicking opens the shared enhancement picker on this slot by
/// setting the [`PickerOpen`] target — the modal itself is rendered once at the shell root by
/// [`EnhancementPickerHost`], never here (a slot lives inside a `transform`ed free-grid surface
/// that would clip a `position:fixed` modal to the panel; see [`PickerTarget`]). Removal is
/// right-click (a filled slot clears its enhancement; an empty user slot removes itself) — there
/// is no persistent remove button; a filled io-set piece instead wears the beta proc/unique dot.
#[component]
fn EnhancementSlot(
    slot_index: u8,
    enhancement: Option<coh_data::Enhancement>,
    /// Whether this slot is a user-placed slot the game didn't grant — a right-click on the empty
    /// cell removes it (beta `canRemoveSlot = index > 0`, excluding trailing inherent auto-slots).
    /// The parent computes it from the power's slot band; the removal fn re-checks (defense in depth).
    removable_slot: bool,
    /// Whether an in-flight leftward stepper drag would take this slot on release — the removal
    /// half of the drag preview, standing in for the ghost circles the add half draws.
    doomed: bool,
    /// The level badge under the slot; `None` when the option to show them is off.
    slot_level: Option<SlotLevelBadge>,
    power_internal_name: String,
    powerset_id: String,
    database: Db,
) -> Element {
    let picker_open = use_context::<PickerOpen>().0;
    let tip = use_context::<SlotTooltip>().0;
    let session = use_context::<BuildSession>();

    // The top-right proc/unique indicator dot (beta enhancement-outline), data-derived from the
    // export proc database. `None` for a non-io-set piece or a plain (non-proc, non-unique) piece.
    let indicator_style: Option<String> = enhancement
        .as_ref()
        .and_then(|enh| slot_indicator_style(enh, &database.procs));

    let enh_icon_url: Option<String> = enhancement
        .as_ref()
        .and_then(|enh| slot_icon_url(enh, &database));

    // The frame over the base icon — the same stack the picker draws, so a piece reads the
    // same rarity in the build as in the selection screen. The origin frame carries the
    // character's current origin (the picker's basis), so it tracks an origin change.
    let character_origin = session.build.read().origin.clone();
    let enh_frame_url: Option<String> = enhancement
        .as_ref()
        .map(|enh| slot_overlay_url(enh, &database, character_origin.as_deref()));

    // The top-left under-level pip: this piece is crafted below the ceiling it could reach, so
    // it is quietly costing the build. Asked of `enhancement_tools` rather than compared here,
    // because the tools' bulk re-level is what clears it — see `under_craft_level`.
    let under_level: Option<coh_data::Level> = enhancement.as_ref().and_then(|enh| {
        let band = CraftBand::from_boost_index(database.0.boost_index.as_ref())?;
        under_craft_level(enh, &band, database.0.io_sets.as_ref())
    });
    // Both slot states open the same picker on this slot; the clear factory backs the filled
    // cell's right-click. Each factory returns a fresh closure so the two call sites never move
    // one closure twice, and each clones its captures per invocation so the handler stays
    // re-callable.
    let open_picker = |powerset_id: String, power_internal_name: String| {
        move |_| {
            // Same stranding as the clear below, by a different route: the picker is a modal
            // over the whole surface, so the pointer stops being over this button without ever
            // travelling off it.
            let mut tip = tip;
            tip.set(None);
            let mut open = picker_open;
            open.set(Some(PickerTarget {
                powerset_id: powerset_id.clone(),
                power_internal_name: power_internal_name.clone(),
                slot_index,
                destination: PickerDestination::Build,
            }));
        }
    };
    let clear_slot = |powerset_id: String, power_internal_name: String| {
        move |evt: Event<MouseData>| {
            // Right-click: suppress the browser's native context menu so the gesture only ever
            // clears the slot (beta `onRightClick`), and stop it here so the card around it does
            // not also read it as "lock the Info panel".
            evt.prevent_default();
            evt.stop_propagation();
            // Drop the hover card with the piece it describes. `onmouseleave` is the only other
            // thing that clears it, and it belongs to the button this commit is about to unmount
            // — the filled cell and the empty one are different branches — so the pointer never
            // leaves anything and the card strands on screen forever. It is `pointer-events:
            // none`, so it cannot even be hovered away. The tooltip's own text is what tells you
            // to make this gesture.
            let mut tip = tip;
            tip.set(None);
            let powerset_id = powerset_id.clone();
            let power_internal_name = power_internal_name.clone();
            session.commit(move |state| {
                remove_enhancement_from_slot(
                    state,
                    &powerset_id,
                    &power_internal_name,
                    slot_index as usize,
                );
            });
        }
    };

    rsx! {
        div {
            class: if doomed { "enhancement-slot enhancement-slot--doomed" } else { "enhancement-slot" },
            if let Some(ref enh) = enhancement {
                // The rich hover tooltip (PC3) replaces the native `title`; a `title` here would
                // pop a competing browser tooltip over it (beta drops it too, using its own).
                button {
                    class: "slot-cell slot--{enhancement_type_class(&enh.kind)}",
                    onclick: open_picker(powerset_id.clone(), power_internal_name.clone()),
                    oncontextmenu: clear_slot(powerset_id.clone(), power_internal_name.clone()),
                    onmouseenter: {
                        let enh = enh.clone();
                        let power_internal_name = power_internal_name.clone();
                        let powerset_id = powerset_id.clone();
                        move |evt: Event<MouseData>| {
                            let mut tip = tip;
                            let point = evt.client_coordinates();
                            tip.set(Some(SlotTooltipTarget {
                                enhancement: enh.clone(),
                                power_internal_name: power_internal_name.clone(),
                                powerset_id: powerset_id.clone(),
                                anchor_x: point.x,
                                anchor_y: point.y,
                                slots: None,
                                hint: "Right-click to remove",
                            }));
                        }
                    },
                    onmouseleave: move |_| {
                        let mut tip = tip;
                        tip.set(None);
                    },
                    if let Some(url) = &enh_icon_url {
                        // The base icon under the frame that carries its rarity — the same
                        // stack the picker draws, so a piece reads the same in the build as in
                        // the selection screen.
                        span { class: "slot-chip",
                            img { class: "slot-chip-img", src: "{url}", alt: "{enh.name}" }
                            if let Some(frame) = &enh_frame_url {
                                img { class: "slot-chip-frame", src: "{frame}", alt: "", aria_hidden: "true" }
                            }
                        }
                    } else {
                        span { class: "slot-chip-label", "{slot_chip_label(enh)}" }
                    }
                }
                // The two corner badges are SIBLINGS of the cell, not children of it: the cell
                // clips to its circle (`overflow: hidden`) so the icon fills the round border,
                // and a badge parked at -2px inside that box is clipped away entirely. They
                // anchor on `.enhancement-slot`, which is `position: relative` and does not clip.
                // Top-right proc/unique dot (beta enhancement-outline) — the persistent
                // remove button's replacement. Removal is right-click now (PC2/PC12).
                if let Some(bg) = &indicator_style {
                    span { class: "slot-dot", style: "background: {bg};" }
                }
                if enh.boost > 0 {
                    span { class: "slot-boost", "{enh.boost}" }
                }
                // Top-left under-level dot, the last free corner. It carries no text: the level
                // it is at and the one it could reach are both in the tooltip, because a number
                // legible on a 24px chip is a number that covers the artwork (see the CSS). No
                // `title` either — the mark passes pointer events through to the cell so the
                // whole slot stays one click target, which also means a native tooltip on it
                // could never fire.
                if under_level.is_some() {
                    span { class: "slot-underlevel", "aria-hidden": "true" }
                }
            } else {
                button {
                    class: "slot-cell slot-cell--empty",
                    title: if removable_slot {
                        format!("Slot {} — click to add, right-click to remove", slot_index + 1)
                    } else {
                        format!("Slot {} — click to add an enhancement", slot_index + 1)
                    },
                    onclick: open_picker(powerset_id.clone(), power_internal_name.clone()),
                    // Right-click removes a user-placed empty slot (beta `onRemoveSlot`); the base
                    // and inherent slots aren't removable, so it only suppresses the native menu
                    // there. `remove_slot_at` re-checks the band, so this is safe regardless.
                    oncontextmenu: {
                        let powerset_id = powerset_id.clone();
                        let power_internal_name = power_internal_name.clone();
                        move |evt: Event<MouseData>| {
                            evt.prevent_default();
                            // A slot's right-click is the slot's, never the card's lock.
                            evt.stop_propagation();
                            if !removable_slot {
                                return;
                            }
                            let powerset_id = powerset_id.clone();
                            let power_internal_name = power_internal_name.clone();
                            let database = database.clone();
                            session.commit(move |state| {
                                let leveling = database.leveling_schedule.as_ref();
                                remove_slot_at(state, &powerset_id, &power_internal_name, slot_index as usize, leveling);
                            });
                        }
                    },
                    "{slot_index + 1}"
                }
            }
            // The level this slot was granted at, centred under the cell (the beta's slot-level
            // label). A slot the schedule cannot serve says so with `!` rather than showing a
            // level it was never granted.
            match slot_level {
                Some(SlotLevelBadge::At(level)) => rsx! {
                    span { class: "slot-level", title: "Slot granted at level {level}", "{level}" }
                },
                Some(SlotLevelBadge::Unplaced) => rsx! {
                    span {
                        class: "slot-level slot-level--unplaced",
                        title: "This slot has no level the game could grant it — the build wants more slots at this power's level or later than the schedule issues. Free one from a later power.",
                        "!"
                    }
                },
                None => rsx! {},
            }
        }
    }
}

/// Which family of enhancements the picker is showing. Each tab reads the export
/// directly and filters by the power's own allow-lists.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PickerTab {
    Generic,
    IoSets,
    Special,
    Origin,
}

impl PickerTab {
    /// The persisted token for this tab ([`crate::picker_memory::tab`]).
    fn token(self) -> &'static str {
        match self {
            PickerTab::Generic => crate::picker_memory::tab::GENERIC,
            PickerTab::IoSets => crate::picker_memory::tab::IO_SETS,
            PickerTab::Special => crate::picker_memory::tab::SPECIAL,
            PickerTab::Origin => crate::picker_memory::tab::ORIGIN,
        }
    }

    /// The tab a stored token names. An unreadable token opens on IO Sets rather than refusing:
    /// a place written by a later version is a reason to fall back to the default tab, not a
    /// reason for the picker not to open.
    fn from_token(token: &str) -> Self {
        match token {
            crate::picker_memory::tab::GENERIC => PickerTab::Generic,
            crate::picker_memory::tab::SPECIAL => PickerTab::Special,
            crate::picker_memory::tab::ORIGIN => PickerTab::Origin,
            _ => PickerTab::IoSets,
        }
    }
}

/// Context handle for the per-power picker memory ([`crate::picker_memory::PickerMemory`]),
/// provided once at the shell root. A newtype (like [`PickerOpen`]) so it never collides with
/// another `Signal` in context. It lives above the modal because the modal is unmounted between
/// opens — state whose whole job is to outlive the surface cannot live on it.
#[derive(Clone, Copy)]
pub struct PickerMemoryStore(pub Signal<crate::picker_memory::PickerMemory>);

/// The three origin tiers in picker order, each with its display label and the wire
/// tag the calc keys on (`TO`/`DO`/`SO`). Only the tier *identity* lives here — the
/// magnitude is derived by the calc from the export's tier grid, never hardcoded
/// ([`coh_data::Enhancement::origin`]).
const ORIGIN_TIERS: [(&str, &str); 3] = [
    ("Training Origin", "TO"),
    ("Dual Origin", "DO"),
    ("Single Origin", "SO"),
];

/// Enhancement picker — a modal (the beta `EnhancementPicker`, wrapped in
/// [`crate::modal::Modal`]) rather than a slot-anchored popover: CoH's option
/// inventory is too large for an inline dropdown, which the panel's height/overflow
/// clips. The beta structure is preserved — a tab-header row (one tab per family),
/// a category sidebar, a scrolling body, and a footer — each tab reading straight
/// from the export and filtered by the power's own allow-lists: Generic IO by
/// `allowedEnhancements` ([`coh_data::EnhancementCatalog`]), IO Sets by
/// `allowedSetCategories` ([`coh_data::IoSetCatalog::sets_for_power`]).
#[component]
fn EnhancementPickerModal(
    database: Db,
    slot_index: u8,
    power_internal_name: String,
    powerset_id: String,
    destination: PickerDestination,
    on_close: EventHandler<()>,
) -> Element {
    let session = use_context::<BuildSession>();
    let compare = use_context::<crate::compare_slotting::CompareSlottingStore>();
    let defaults = use_context::<PickerDefaults>();
    let memory = use_context::<PickerMemoryStore>().0;
    // Level-up mode holds the picker to what the character could actually craft: sets whose
    // `min_level` it has reached, and a craft level no higher than its own.
    let level_up_mode = use_context::<crate::level_control::LevelUpMode>().0;
    let character_level = session.build.read().level;

    // The DESTINATION's slotting, asked once and reused: the opening rule, the bulk placement's
    // target slots and the footer's clear control all have to be talking about the same power.
    // A Compare row's slots are that row's, not the build's behind it.
    let address = coh_data::power_address(&powerset_id, &power_internal_name);
    let slots = destination_slots(
        session,
        compare,
        destination,
        &powerset_id,
        &power_internal_name,
    );

    // Where this open lands, by the ranked rule in `picker_memory`: the slot's own piece, else
    // this power's remembered place, else IO Sets unfiltered. Read with `peek` — the opening is
    // a starting value, and subscribing to the memory here would re-run the modal every time it
    // records where the user just navigated to.
    let remembered = memory.peek().place(&address).cloned();
    let opening = crate::picker_memory::opening_location(
        slots.get(slot_index as usize).and_then(Option::as_ref),
        database.io_sets.as_ref(),
        remembered.as_ref(),
    );

    let mut active_tab = use_signal(|| PickerTab::from_token(&opening.place.tab));
    // Sidebar slot-category facet for the IO Sets tab; `None` = every eligible set.
    let mut set_category = use_signal(|| opening.place.set_type.clone());
    // Rarity facet — a second axis that cuts ACROSS the slot category rather than replacing it,
    // which is what lets "a purple Hold set" be asked as one question.
    let mut set_rarity = use_signal(|| opening.place.rarity.clone());
    // Set size (piece count) facet; `None` = any size.
    let mut set_size = use_signal(|| None::<usize>);
    // Only sets carrying a proc piece.
    let mut procs_only = use_signal(|| false);
    let mut set_sort = use_signal(crate::picker_sets::SetSort::default);
    // The set to scroll to on open — set only when this open is a CHANGE to a piece already in
    // the slot, where the thing being changed is the best guess at what the user is looking for.
    let jump_to_set = use_signal(|| opening.jump_to_set.clone());

    // Multi-select. `select_mode` is the one gesture that works the same on a mouse and a finger,
    // so it is the mode the other gestures fall back to rather than an accelerator beside them.
    let mut select_mode = use_signal(|| false);
    let mut queue = use_signal(Vec::<crate::picker_queue::QueuedPick>::new);
    // The in-flight drag: the set it started in, the piece it started on, and how far it has
    // reached. Held per set because a range across two sets is not a range.
    let mut drag = use_signal(|| None::<(String, usize, usize)>);
    // The set whose bonus ladder is unfolded under its pieces. Opened by the set's name, not by a
    // hover or a long-press on a piece: a name tap reads the same on a finger and a mouse, and
    // the pieces' own press is already the slot-or-drag gesture.
    let mut bonus_set = use_signal(|| None::<String>);
    let totals = use_context::<crate::panels::stats::BuildTotals>().0;
    // A piece's rich tooltip is the slot tooltip, raised from here. Placing a piece closes the
    // picker under the pointer, so no mouse-leave ever arrives to take it down.
    let mut tip = use_context::<SlotTooltip>().0;
    use_drop(move || tip.set(None));

    // Record where the user navigated to, for this power. An effect rather than a line in each
    // handler: there are six controls that move the location and a seventh would forget.
    {
        let address = address.clone();
        let mut memory = memory;
        use_effect(move || {
            let place = crate::picker_memory::PickerLocation {
                tab: active_tab().token().to_string(),
                set_type: set_category(),
                rarity: set_rarity(),
            };
            memory.write().remember(&address, place);
        });
    }

    // The power's def must resolve for any tab, or the reason shows in place
    // (Rule 1 — a visible marker, never a silently empty list). Through
    // `resolve_power_def`, never `find_power`: the latter searches `powersets` alone, so
    // every pool and epic pick — which carries its aggregate as its set id — came back
    // absent and this modal answered "not in the current dataset" for all of them.
    let Some(power_def) = resolve_power_def(&database, &powerset_id, &power_internal_name) else {
        return rsx! {
            Modal { title: "Enhancements".to_string(), size: ModalSize::Lg, on_close,
                div { class: "picker-error", "This power is not in the current dataset." }
            }
        };
    };
    let power_id = power_internal_name.clone();
    let title = format!("Enhance {}", power_def.name);

    // Whether the slot this picker is aimed at already holds a piece — off the destination's own
    // slotting, so a Compare row's footer never offers to empty a slot that is full only in the
    // build behind it.
    let slot_filled = slots.get(slot_index as usize).is_some_and(Option::is_some);
    // The slots a bulk placement would fill, and therefore how many marks can actually land.
    let open_slots = crate::picker_queue::empty_slots_from(&slots, slot_index as usize);

    let tab_class = |tab: PickerTab| {
        if active_tab() == tab {
            "picker-tab picker-tab-active"
        } else {
            "picker-tab"
        }
    };

    let io_level = defaults.io_level;
    let attuned = defaults.attuned;
    let boost = defaults.boost;
    let relative_level = defaults.relative_level;

    // The header's offset stepper serves two DIFFERENT mechanics — the catalyst booster
    // combine on IOs and the signed relative level on origin/special pieces
    // (`coh_math::enhancement::EnhancementLevelAxis`) — so the active tab decides which one
    // it is editing, and each keeps its own stored value across a tab switch. The relative
    // band comes from the dataset's own curves; a fork that attenuates neither direction
    // (Thunderspy) offers nothing to step through and gets no control.
    let relative_axis = matches!(active_tab(), PickerTab::Special | PickerTab::Origin);
    let relative_band = crate::picker_defaults::RelativeLevelBand::from_curves(
        database.enhancement_curves.as_ref(),
    );
    // The crafting levels the export names a record at. Set pieces are craftable at every
    // integer inside their own range, common IOs only at the roster's nine — so the band's ends
    // bound the control everywhere, and on the Generic tab it walks the roster entry by entry
    // and shows the level a pick would actually mint (BOOST-6).
    let craft_band =
        crate::picker_defaults::CraftBand::from_boost_index(database.boost_index.as_ref());
    // The stamp reads the floor per piece (a set's own band can clamp below it); this is the
    // half the header CAN know from the dial alone, so the control says so instead of
    // letting a boost be dialled that no pick would keep.
    let boostable = io_level() >= crate::picker_defaults::BOOSTER_LEVEL_FLOOR;

    // In level-up mode the craft level can't be raised past the character's own level — you
    // can't make a level 50 IO at level 12. The band's floor (10) still applies below that: a
    // character under 10 has no lower IO to craft, and clamping into the band is not the same
    // as inventing one. A stored default already above the ceiling is SHOWN as over-level
    // rather than silently rewritten — the number the user set stays the number they see.
    let band_top = craft_band.as_ref().map_or(io_level(), CraftBand::max);
    let band_floor = craft_band.as_ref().map_or(io_level(), CraftBand::min);
    let craft_ceiling = if level_up_mode() {
        character_level.clamp(band_floor, band_top)
    } else {
        band_top
    };
    let craft_over_level = io_level() > craft_ceiling;
    // The Generic tab mints at a roster level; a level carried in from the set tab (27 is a
    // real recipe there) crafts at the step below it, and the control says so rather than
    // rewriting what the user set.
    let on_generic_tab = matches!(active_tab(), PickerTab::Generic);
    let shown_level = match &craft_band {
        // The set tab's every-integer range is real; only the ends are clamped there, and a
        // level stored before the band was read off the export folds in on sight.
        Some(band) if on_generic_tab => band.crafted_at_or_below(io_level()),
        Some(band) => io_level().clamp(band.min(), band.max()),
        None => io_level(),
    };

    rsx! {
        Modal { title, size: ModalSize::Lg, on_close,
            div { class: "picker",
                div { class: "picker-tabs",
                    div { class: "picker-tablist",
                        button {
                            class: tab_class(PickerTab::Generic),
                            onclick: move |_| active_tab.set(PickerTab::Generic),
                            "Generic IO"
                        }
                        button {
                            class: tab_class(PickerTab::IoSets),
                            onclick: move |_| active_tab.set(PickerTab::IoSets),
                            "IO Sets"
                        }
                        button {
                            class: tab_class(PickerTab::Special),
                            onclick: move |_| active_tab.set(PickerTab::Special),
                            "Special"
                        }
                        button {
                            class: tab_class(PickerTab::Origin),
                            onclick: move |_| active_tab.set(PickerTab::Origin),
                            "Origin"
                        }
                    }
                    // Global slotting controls (beta EnhancementPicker header): the craft level,
                    // attunement, and catalyst booster stamped onto whatever piece is picked.
                    // Attuned pieces scale with character level, so Lv greys out while it's on.
                    div { class: "picker-controls",
                        div {
                            class: match (attuned(), craft_over_level) {
                                (true, _) => "picker-control picker-control-off",
                                (false, true) => "picker-control picker-control-over",
                                (false, false) => "picker-control",
                            },
                            title: if craft_over_level {
                                format!("Craft level {} is above this character's level {character_level} — Level Up mode won't raise it further", io_level())
                            } else if craft_ceiling < band_top {
                                format!("Crafting level for IOs. Level Up mode caps it at this character's level ({craft_ceiling}).")
                            } else {
                                format!(
                                    "Crafting level for IOs ({band_floor}–{band_top}). Higher = stronger, but exemplaring below it disables the IO."
                                )
                            },
                            span { class: "picker-control-label", "Lv" }
                            button {
                                class: "picker-step",
                                disabled: attuned() || craft_band.is_none(),
                                onclick: {
                                    let band = craft_band.clone();
                                    move |_| {
                                        if let Some(band) = band.as_ref() {
                                            defaults.adjust_io_level(-1, band, on_generic_tab);
                                        }
                                    }
                                },
                                "−"
                            }
                            span { class: "picker-step-value", "{shown_level}" }
                            button {
                                class: "picker-step",
                                disabled: attuned() || craft_band.is_none() || shown_level >= craft_ceiling,
                                onclick: {
                                    let band = craft_band.clone();
                                    move |_| {
                                        if let Some(band) = band.as_ref() {
                                            defaults.adjust_io_level(1, band, on_generic_tab);
                                        }
                                    }
                                },
                                "+"
                            }
                        }
                        label {
                            class: "picker-toggle",
                            title: "Attuned IOs scale with your current level, and are never lost \
                                    when exemplaring.",
                            input {
                                r#type: "checkbox",
                                checked: attuned(),
                                onchange: move |_| {
                                    let mut attuned = attuned;
                                    let next = !*attuned.peek();
                                    attuned.set(next);
                                },
                            }
                            span { "Attuned" }
                        }
                        if let (true, Some(band)) = (relative_axis, relative_band.filter(|b| !b.is_even_only())) {
                            div {
                                class: "picker-control",
                                title: "Relative level ({band.min} to +{band.max}): shifts the \
                                        enhancement relative to the character level.",
                                span { class: "picker-control-label", "Rel" }
                                button {
                                    class: "picker-step",
                                    disabled: relative_level() <= band.min,
                                    onclick: move |_| defaults.adjust_relative_level(-1, band),
                                    "−"
                                }
                                span {
                                    class: if relative_level() != 0 { "picker-step-value picker-step-value-on" } else { "picker-step-value" },
                                    if relative_level() > 0 { "+{relative_level}" } else { "{relative_level}" }
                                }
                                button {
                                    class: "picker-step",
                                    disabled: relative_level() >= band.max,
                                    onclick: move |_| defaults.adjust_relative_level(1, band),
                                    "+"
                                }
                            }
                        } else if !relative_axis {
                            div {
                                class: "picker-control",
                                title: if boostable {
                                    "Catalyst boosters (+0 to +5) combined into an IO."
                                } else {
                                    "Catalyst boosters (+0 to +5) combined into an IO. Boosters only combine into a level-50+ IO — raise the craft level to use them."
                                },
                                span { class: "picker-control-label", "Boost" }
                                button {
                                    class: "picker-step",
                                    disabled: !boostable,
                                    onclick: move |_| defaults.adjust_boost(-1),
                                    "−"
                                }
                                span {
                                    class: if boost() > 0 && boostable { "picker-step-value picker-step-value-on" } else { "picker-step-value" },
                                    if boost() > 0 { "+{boost}" } else { "0" }
                                }
                                button {
                                    class: "picker-step",
                                    disabled: !boostable,
                                    onclick: move |_| defaults.adjust_boost(1),
                                    "+"
                                }
                            }
                        }
                    }
                    // Multi-select. Marking is its own mode rather than only a modifier
                    // because a finger has no shift key, and the mode is what gives touch the
                    // same reach the keyboard gives a mouse. Shift+click and a drag across a
                    // strip both work without it; both turn it on, so the bar that spends the
                    // marks is on screen either way.
                    label {
                        class: if select_mode() { "picker-toggle picker-toggle-on" } else { "picker-toggle" },
                        title: "Mark several enhancements, then slot them all at once. Clicking an IO, \
                                special or origin enhancement again marks another copy; right-click \
                                takes one off. Shift-click, or a drag across a set, marks without this toggle.",
                        input {
                            r#type: "checkbox",
                            checked: select_mode(),
                            onchange: move |_| {
                                let next = !*select_mode.peek();
                                select_mode.set(next);
                                // Leaving the mode drops the marks rather than hiding them —
                                // a queue with no bar is a pending act with no way to see it.
                                if !next {
                                    queue.write().clear();
                                }
                            },
                        }
                        span { "Select" }
                    }
                }
                div { class: "picker-main",
                    match active_tab() {
                        // The accepted generic IOs come straight from the export — the dataset's
                        // common-IO types intersected with this power's `allowedEnhancements`.
                        // No sidebar: generic IOs aren't categorized.
                        PickerTab::Generic => match database.enhancements.as_ref() {
                            None => rsx! {
                                div { class: "picker-body",
                                    div { class: "picker-error", "Enhancement catalog unavailable for this dataset." }
                                }
                            },
                            Some(catalog) => {
                                // Each stat's icon under the plain IO frame, as on the power card. A
                                // `None` icon keeps the text button — never a broken image (Rule 1).
                                let generic_ios: Vec<(String, Option<String>)> = catalog
                                    .generic_ios_for_power(power_def.allowed_enhancements.as_deref())
                                    .into_iter()
                                    .map(|stat| (stat.to_string(), crate::view::icons::stat_icon_url(stat)))
                                    .collect();
                                rsx! {
                                    div { class: "picker-body",
                                        if generic_ios.is_empty() {
                                            div { class: "picker-empty", "This power accepts no generic enhancements." }
                                        }
                                        div { class: "picker-icon-row",
                                        for (stat, icon_url) in generic_ios {
                                            {
                                                let pick = crate::picker_queue::QueuedPick::Generic { stat: stat.clone() };
                                                let queued_at = crate::picker_queue::badge(&queue.read(), &pick);
                                                rsx! {
                                                    button {
                                                        key: "{stat}",
                                                        class: if icon_url.is_some() { "picker-icon-item" } else { "picker-item picker-piece" },
                                                        title: "{stat} IO",
                                                        class: if queued_at.is_some() { "is-queued" } else { "" },
                                                        oncontextmenu: {
                                                            let pick = pick.clone();
                                                            move |evt: Event<MouseData>| unmark_one(evt, &mut queue, &mut select_mode, &pick)
                                                        },
                                                        onclick: {
                                                            let stat = stat.clone();
                                                            let pick = pick.clone();
                                                            let power_id = power_id.clone();
                                                            let powerset_id = powerset_id.clone();
                                                            let slot_idx = slot_index as usize;
                                                            let crafted_level = shown_level;
                                                            move |evt: Event<MouseData>| {
                                                                // In select mode a click MARKS rather than spends, so a
                                                                // power can be filled with several commons in one visit.
                                                                // A second click marks a second copy.
                                                                if evt.modifiers().shift() || *select_mode.peek() {
                                                                    crate::picker_queue::press(&mut queue.write(), pick.clone());
                                                                    let marked = !queue.read().is_empty();
                                                                    select_mode.set(marked);
                                                                    return;
                                                                }
                                                                // Generic IOs always take the picker's craft level and booster
                                                                // (they're never attuned — the toggle governs set pieces only).
                                                                // The level is the roster's, not the dial's raw value: a common
                                                                // IO exists only at the levels the export names (BOOST-6).
                                                                let enhancement = coh_data::Enhancement::generic_io(
                                                                    stat.clone(),
                                                                    Level::new(crafted_level),
                                                                    *boost.peek(),
                                                                );
                                                                place_pick(session, compare, destination, &powerset_id, &power_id, slot_idx, enhancement);
                                                                on_close.call(());
                                                            }
                                                        },
                                                        if let Some(url) = &icon_url {
                                                            span { class: "picker-enh-icon",
                                                                img { src: "{url}", alt: "{stat} IO" }
                                                                img {
                                                                    src: crate::view::icons::plain_io_frame_url(),
                                                                    alt: "",
                                                                    aria_hidden: "true",
                                                                }
                                                            }
                                                            span { class: "picker-icon-caption", "{stat}" }
                                                        } else {
                                                            "{stat} IO"
                                                        }
                                                        if let Some(at) = &queued_at {
                                                            span { class: "picker-queue-badge", "{at}" }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        }
                                    }
                                }
                            }
                        },
                        // The eligible sets come straight from the export — the dataset's IO sets
                        // whose `type` is one of this power's `allowedSetCategories`. Every facet
                        // below narrows that list and none of them replaces it; the arithmetic
                        // lives in `picker_sets` so the composition can be asked questions.
                        PickerTab::IoSets => match database.io_sets.as_ref() {
                            None => rsx! {
                                div { class: "picker-body",
                                    div { class: "picker-error", "IO set catalog unavailable for this dataset." }
                                }
                            },
                            Some(io_catalog) => {
                                let eligible = io_catalog
                                    .sets_for_power(power_def.allowed_set_categories.as_deref().unwrap_or(&[]));
                                let facets = crate::picker_sets::SetFacets {
                                    set_type: set_category(),
                                    rarity: set_rarity(),
                                    size: set_size(),
                                    procs_only: procs_only(),
                                    sort: set_sort(),
                                    // Level-up mode withholds sets the character couldn't craft
                                    // yet — the set's own `min_level`, not a table.
                                    craftable_at: level_up_mode().then(|| i64::from(character_level)),
                                };
                                // An unknown majority marks nothing, rather than marking every row.
                                let usual_size = io_catalog.most_common_piece_count();
                                let visible = crate::picker_sets::offered_sets(&eligible, &facets, usual_size);
                                let type_counts = crate::picker_sets::set_type_counts(&eligible, &facets);
                                let rarity_counts = crate::picker_sets::rarity_counts(&eligible, &facets);
                                let size_counts = crate::picker_sets::size_counts(&eligible, &facets);
                                let total = crate::picker_sets::total_offered(&eligible, &facets);
                                // The scope the build-wide single-copy rule is asked in. A Compare
                                // row is one power's hypothetical, so it is graded against its own
                                // slots alone (`coh_data::slotting_rules`).
                                let build = session.build.read();
                                let unique_scope = match destination {
                                    PickerDestination::Build => coh_data::slotting_rules::UniqueScope::Build {
                                        state: &build,
                                        catalog: io_catalog,
                                    },
                                    PickerDestination::CompareCopy { .. } => {
                                        coh_data::slotting_rules::UniqueScope::Isolated
                                    }
                                };
                                let attuned_frame = attuned();
                                rsx! {
                                    aside { class: "picker-sidebar",
                                        div { class: "picker-sidebar-heading", "Set category" }
                                        button {
                                            class: if set_category().is_none() { "picker-cat picker-cat-active" } else { "picker-cat" },
                                            onclick: move |_| set_category.set(None),
                                            "All sets ({total})"
                                        }
                                        for facet in type_counts {
                                            button {
                                                key: "{facet.value}",
                                                class: if set_category().as_deref() == Some(facet.value.as_str()) { "picker-cat picker-cat-active" } else { "picker-cat" },
                                                onclick: {
                                                    let value = facet.value.clone();
                                                    move |_| set_category.set(Some(value.clone()))
                                                },
                                                "{facet.value} ({facet.count})"
                                            }
                                        }
                                        // Rarity cuts ACROSS the slot categories above, so it gets
                                        // its own headed section instead of extending that run.
                                        // Derived from the export's own `category` vocabulary, so
                                        // a fork that ships a seventh tier gets a button for it
                                        // rather than having its sets fall out of the sidebar.
                                        if rarity_counts.len() > 1 {
                                            div { class: "picker-sidebar-heading", "Rarity" }
                                            button {
                                                class: if set_rarity().is_none() { "picker-cat picker-cat-active" } else { "picker-cat" },
                                                onclick: move |_| set_rarity.set(None),
                                                "Any rarity ({total})"
                                            }
                                            for facet in rarity_counts {
                                                button {
                                                    key: "{facet.value}",
                                                    class: if set_rarity().as_deref() == Some(facet.value.as_str()) { "picker-cat picker-cat-active" } else { "picker-cat" },
                                                    onclick: {
                                                        let value = facet.value.clone();
                                                        move |_| set_rarity.set(Some(value.clone()))
                                                    },
                                                    "{crate::picker_sets::rarity_label(&facet.value)} ({facet.count})"
                                                }
                                            }
                                        }
                                        // Absent when every set here is one size — nothing to
                                        // choose between, and a lone button reads as a filter
                                        // that does nothing.
                                        if size_counts.len() > 1 {
                                            div { class: "picker-sidebar-heading", "Set size" }
                                            button {
                                                class: if set_size().is_none() { "picker-cat picker-cat-active" } else { "picker-cat" },
                                                onclick: move |_| set_size.set(None),
                                                "Any size ({total})"
                                            }
                                            for (size, count) in size_counts.iter().copied() {
                                                button {
                                                    key: "{size}",
                                                    class: if set_size() == Some(size) { "picker-cat picker-cat-active" } else { "picker-cat" },
                                                    title: "Only {size}-piece sets, the full set \
                                                            fits in {size} slots",
                                                    onclick: move |_| set_size.set(Some(size)),
                                                    "{size} pieces ({count})"
                                                }
                                            }
                                        }
                                        div { class: "picker-sidebar-heading", "Show" }
                                        button {
                                            class: if procs_only() { "picker-cat picker-cat-active" } else { "picker-cat" },
                                            title: "Only sets carrying a proc",
                                            onclick: move |_| {
                                                let next = !*procs_only.peek();
                                                procs_only.set(next);
                                            },
                                            "Procs only"
                                        }
                                        div { class: "picker-sidebar-heading", "Sort" }
                                        for (label, sort) in [
                                            ("Name", crate::picker_sets::SetSort::Name),
                                            ("Level", crate::picker_sets::SetSort::Level),
                                        ] {
                                            button {
                                                key: "{label}",
                                                class: if set_sort() == sort { "picker-cat picker-cat-active" } else { "picker-cat" },
                                                onclick: move |_| set_sort.set(sort),
                                                "{label}"
                                            }
                                        }
                                    }
                                    div { class: "picker-body",
                                        if visible.is_empty() {
                                            // "None left after the filters" is a different
                                            // statement from "this power accepts none", and the
                                            // second would read as a data gap.
                                            if eligible.is_empty() {
                                                div { class: "picker-empty", "This power accepts no IO sets." }
                                            } else {
                                                div { class: "picker-empty",
                                                    "No IO set matches these filters. Clear one to widen the list."
                                                }
                                            }
                                        }
                                        for offer in visible {
                                            div {
                                                key: "{offer.id}",
                                                class: "picker-set-group",
                                                "data-set-id": "{offer.id}",
                                                // The set the picker opened onto scrolls itself
                                                // into view. Only the set being CHANGED carries
                                                // this, so nothing scrolls on an ordinary open.
                                                onmounted: {
                                                    let mine = jump_to_set().as_deref() == Some(offer.id);
                                                    let id = offer.id.to_string();
                                                    move |_| {
                                                        if mine {
                                                            document::eval(&format!(
                                                                "const el = document.querySelector('[data-set-id={id:?}]'); \
                                                                 if (el) el.scrollIntoView({{ block: 'center' }});"
                                                            ));
                                                        }
                                                    }
                                                },
                                                button {
                                                    r#type: "button",
                                                    class: "picker-set-name",
                                                    aria_expanded: "{bonus_set().as_deref() == Some(offer.id)}",
                                                    title: "Show this set's bonuses",
                                                    onclick: {
                                                        let id = offer.id.to_string();
                                                        move |_| {
                                                            let open = bonus_set.peek().as_deref() == Some(id.as_str());
                                                            bonus_set.set((!open).then(|| id.clone()));
                                                        }
                                                    },
                                                    // Only the minority size is marked, so a short
                                                    // set is found by scanning for ink instead of
                                                    // reading every name.
                                                    if offer.off_size {
                                                        span {
                                                            class: "picker-set-size",
                                                            title: "{offer.set.pieces.len()}-piece \
                                                                    set, the full set fits in \
                                                                    {offer.set.pieces.len()} slots",
                                                            "{offer.set.pieces.len()}pc"
                                                        }
                                                    }
                                                    "{offer.set.name}"
                                                    span { class: "picker-set-meta", "lv {offer.set.min_level}–{offer.set.max_level}" }
                                                }
                                                div { class: "picker-icon-row",
                                                    // Legality per piece, computed once for the
                                                    // whole strip: a single tile needs its own
                                                    // answer to draw disabled, and a DRAG needs
                                                    // every tile's, because a range sweeps past
                                                    // pieces it never lands a pointer on.
                                                    {
                                                        let placeable: Vec<bool> = offer.set.pieces
                                                            .iter()
                                                            .map(|piece| coh_data::slotting_rules::piece_slottable_now(
                                                                offer.id, piece, &slots, &unique_scope,
                                                            ))
                                                            .collect();
                                                        rsx! {
                                                    for (piece_index, piece) in offer.set.pieces.iter().enumerate() {
                                                        {
                                                            let pick = crate::picker_queue::QueuedPick::SetPiece {
                                                                set_id: offer.id.to_string(),
                                                                piece_index,
                                                            };
                                                            let queued_at = crate::picker_queue::badge(&queue.read(), &pick);
                                                            let slottable = placeable[piece_index];
                                                            // Inside the in-flight drag's span —
                                                            // shown before the pointer comes up,
                                                            // so a range is seen before it commits.
                                                            let in_drag = drag()
                                                                .is_some_and(|(set_id, from, to)| {
                                                                    set_id == offer.id
                                                                        && crate::picker_queue::drag_span(from, to)
                                                                            .contains(&piece_index)
                                                                });
                                                            let icon = crate::view::icons::io_set_icon_url(&offer.set.icon);
                                                            let label = piece_label(piece);
                                                            rsx! {
                                                                button {
                                                                    key: "{piece_index}",
                                                                    class: if icon.is_some() { "picker-icon-item" } else { "picker-item picker-piece" },
                                                                    class: if queued_at.is_some() { "is-queued" } else { "" },
                                                                    class: if in_drag { "is-dragging" } else { "" },
                                                                    disabled: !slottable,
                                                                    // A disabled button gets no mouse events, so a
                                                                    // refused piece keeps the native title saying why.
                                                                    title: if !slottable {
                                                                        format!("{} — {label}: already slotted, or a unique this build already holds", offer.set.name)
                                                                    },
                                                                    onmouseenter: {
                                                                        let pick = pick.clone();
                                                                        let database = database.clone();
                                                                        let powerset_id = powerset_id.clone();
                                                                        let power_id = power_id.clone();
                                                                        let slots = slots.clone();
                                                                        move |evt: Event<MouseData>| {
                                                                            let Some(enhancement) = build_queued_pick(
                                                                                &database, &pick, defaults, &session,
                                                                            ) else {
                                                                                return;
                                                                            };
                                                                            let point = evt.client_coordinates();
                                                                            tip.set(Some(SlotTooltipTarget {
                                                                                enhancement,
                                                                                power_internal_name: power_id.clone(),
                                                                                powerset_id: powerset_id.clone(),
                                                                                anchor_x: point.x,
                                                                                anchor_y: point.y,
                                                                                slots: Some(slots.clone()),
                                                                                hint: "Click to slot · drag to mark several",
                                                                            }));
                                                                        }
                                                                    },
                                                                    onmouseleave: move |_| tip.set(None),
                                                                    onpointerdown: {
                                                                        let set_id = offer.id.to_string();
                                                                        move |evt: Event<PointerData>| {
                                                                            // Shift and select mode both mean
                                                                            // "mark", so neither arms a drag.
                                                                            if evt.modifiers().shift() || *select_mode.peek() {
                                                                                return;
                                                                            }
                                                                            drag.set(Some((set_id.clone(), piece_index, piece_index)));
                                                                        }
                                                                    },
                                                                    onpointerenter: {
                                                                        let set_id = offer.id.to_string();
                                                                        move |_| {
                                                                            let reaching = drag
                                                                                .peek()
                                                                                .clone()
                                                                                .filter(|(from_set, _, _)| from_set == &set_id);
                                                                            if let Some((from_set, from, _)) = reaching {
                                                                                drag.set(Some((from_set, from, piece_index)));
                                                                            }
                                                                        }
                                                                    },
                                                                    // A touch that turns into a scroll is
                                                                    // cancelled by the browser rather than
                                                                    // completed, so the gesture that would
                                                                    // otherwise slot a piece under a moving
                                                                    // thumb never reaches the `up` arm.
                                                                    onpointercancel: move |_| drag.set(None),
                                                                    onpointerup: {
                                                                        let set_id = offer.id.to_string();
                                                                        let pick = pick.clone();
                                                                        let database = database.clone();
                                                                        let powerset_id = powerset_id.clone();
                                                                        let power_id = power_id.clone();
                                                                        let placeable = placeable.clone();
                                                                        move |evt: Event<PointerData>| {
                                                                            let span = drag.peek().clone();
                                                                            drag.set(None);
                                                                            if evt.modifiers().shift() || *select_mode.peek() {
                                                                                crate::picker_queue::toggle(&mut queue.write(), pick.clone());
                                                                                // The toggle tracks the queue rather than
                                                                                // the gesture that filled it: a shift-click
                                                                                // marks without touching the mode, and the
                                                                                // NEXT plain click would then place and
                                                                                // close, throwing the marks away with no
                                                                                // sign they had been there. Unmarking the
                                                                                // last one leaves the mode the same way.
                                                                                let marked = !queue.read().is_empty();
                                                                                select_mode.set(marked);
                                                                                return;
                                                                            }
                                                                            match span {
                                                                                // A drag that covered more than
                                                                                // its origin MARKS the range and
                                                                                // shows the bar, rather than
                                                                                // writing the slots outright.
                                                                                // Six slots is too much to spend
                                                                                // on a gesture that cannot be
                                                                                // seen before it lands — the
                                                                                // same call the pool picker
                                                                                // made about taking a power.
                                                                                Some((from_set, from, to))
                                                                                    if from_set == set_id && from != to =>
                                                                                {
                                                                                    select_mode.set(true);
                                                                                    let mut queue = queue.write();
                                                                                    for index in crate::picker_queue::drag_span(from, to) {
                                                                                        // The range sweeps pieces the
                                                                                        // pointer never stopped on,
                                                                                        // including the refused ones
                                                                                        // drawn between its endpoints.
                                                                                        // Marking those put a badge on
                                                                                        // a tile that cannot be spent.
                                                                                        if !placeable.get(index).copied().unwrap_or(false) {
                                                                                            continue;
                                                                                        }
                                                                                        crate::picker_queue::mark(
                                                                                            &mut queue,
                                                                                            crate::picker_queue::QueuedPick::SetPiece {
                                                                                                set_id: set_id.clone(),
                                                                                                piece_index: index,
                                                                                            },
                                                                                        );
                                                                                    }
                                                                                }
                                                                                // A plain click still slots one
                                                                                // piece and closes — the single
                                                                                // pick stays one action.
                                                                                _ => {
                                                                                    if let Some(enhancement) = build_queued_pick(
                                                                                        &database, &pick, defaults, &session,
                                                                                    ) {
                                                                                        place_pick(
                                                                                            session, compare, destination,
                                                                                            &powerset_id, &power_id,
                                                                                            slot_index as usize, enhancement,
                                                                                        );
                                                                                        on_close.call(());
                                                                                    }
                                                                                }
                                                                            }
                                                                        }
                                                                    },
                                                                    if let Some(url) = &icon {
                                                                        // The set's base icon under the frame
                                                                        // that carries its rarity — without it
                                                                        // a purple reads like a common.
                                                                        span { class: "picker-enh-icon",
                                                                            img { src: "{url}", alt: "{label}" }
                                                                            img {
                                                                                src: crate::view::icons::io_set_overlay_url(
                                                                                    &offer.set.category, &offer.set.icon,
                                                                                    attuned_frame || offer.set.attuned_only,
                                                                                ),
                                                                                alt: "",
                                                                                aria_hidden: "true",
                                                                            }
                                                                        }
                                                                        span { class: "picker-icon-caption", "{label}" }
                                                                    } else {
                                                                        "{label}"
                                                                    }
                                                                    if let Some(at) = &queued_at {
                                                                        // Which empty slot this mark will land
                                                                        // in, shown before it is spent.
                                                                        span { class: "picker-queue-badge", "{at}" }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                        }
                                                    }
                                                }
                                                // The unfolded bonus ladder, graded against the
                                                // destination's own slots: a Compare row's tiers
                                                // are that row's. Its Rule-of-5 counts are the
                                                // build's, so a Compare row shows the tiers alone.
                                                if bonus_set().as_deref() == Some(offer.id) {
                                                    div { class: "picker-set-bonuses",
                                                        match build.selected_power(&powerset_id, &power_id) {
                                                            Some(power) => {
                                                                let mut power = power.clone();
                                                                power.slots = slots.clone();
                                                                let tracking = match destination {
                                                                    PickerDestination::Build => totals.read().set_bonus_tracking.clone(),
                                                                    PickerDestination::CompareCopy { .. } => Vec::new(),
                                                                };
                                                                let block = build_set_bonus_block(offer.set, &power, offer.id, &tracking);
                                                                rsx! {
                                                                    div { class: "slot-tooltip-bonuses-head",
                                                                        "Set Bonuses ({block.slotted}/{block.total_pieces} slotted in this power)"
                                                                    }
                                                                    SetBonusTierList { tiers: block.tiers }
                                                                }
                                                            }
                                                            None => rsx! {
                                                                div { class: "picker-error", "This power is not in the build, so its set bonuses can't be read." }
                                                            },
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        },
                        // The special families come straight from the export — each is offered when
                        // one of its aspects is in this power's `allowedEnhancements` (the beta
                        // `filterSpecialEnhancements`). No sidebar: families are their own sections.
                        PickerTab::Special => match database.enhancements.as_ref() {
                            None => rsx! {
                                div { class: "picker-body",
                                    div { class: "picker-error", "Enhancement catalog unavailable for this dataset." }
                                }
                            },
                            Some(catalog) => {
                                let allowed = power_def.allowed_enhancements.as_deref();
                                // Each entry carries its derived icon URL and the beta's tooltip
                                // line (name + per-aspect values) precomputed, so the markup below
                                // reads flat. A `None` icon keeps the text button — never a broken
                                // image (Rule 1).
                                type SpecialEntry = (String, coh_data::SpecialEnhancementDef, Option<String>, String);
                                let families: Vec<(&'static str, &'static str, Vec<SpecialEntry>)> = catalog
                                    .special_families()
                                    .into_iter()
                                    .map(|(label, category, map)| {
                                        let entries = map
                                            .iter()
                                            .filter(|(_, def)| coh_data::EnhancementCatalog::special_available(def, allowed))
                                            .map(|(id, def)| {
                                                let icon = crate::view::icons::special_icon_url(category, id);
                                                let aspects = def
                                                    .aspects
                                                    .iter()
                                                    .map(|a| format!("{} +{}%", a.stat, a.value))
                                                    .collect::<Vec<_>>()
                                                    .join(", ");
                                                let title = format!("{}: {aspects}", def.name);
                                                (id.clone(), def.clone(), icon, title)
                                            })
                                            .collect();
                                        (label, category, entries)
                                    })
                                    .collect();
                                let any = families.iter().any(|(_, _, entries)| !entries.is_empty());
                                rsx! {
                                    div { class: "picker-body",
                                        if !any {
                                            div { class: "picker-empty", "This power accepts no special enhancements." }
                                        }
                                        for (label, category, entries) in families {
                                            if !entries.is_empty() {
                                                div { key: "{category}", class: "picker-set-group",
                                                    div { class: "picker-set-name", "{label}" }
                                                    div { class: "picker-icon-row",
                                                        for (id, def, icon_url, item_title) in entries {
                                                            {
                                                            let pick = crate::picker_queue::QueuedPick::Special {
                                                                id: id.clone(),
                                                                category: category.to_string(),
                                                            };
                                                            let queued_at = crate::picker_queue::badge(&queue.read(), &pick);
                                                            rsx! {
                                                            button {
                                                                key: "{id}",
                                                                class: if icon_url.is_some() { "picker-icon-item" } else { "picker-item picker-piece" },
                                                                class: if queued_at.is_some() { "is-queued" } else { "" },
                                                                title: "{item_title}",
                                                                oncontextmenu: {
                                                                    let pick = pick.clone();
                                                                    move |evt: Event<MouseData>| unmark_one(evt, &mut queue, &mut select_mode, &pick)
                                                                },
                                                                onclick: {
                                                                    let id = id.clone();
                                                                    let def = def.clone();
                                                                    let pick = pick.clone();
                                                                    let power_id = power_id.clone();
                                                                    let powerset_id = powerset_id.clone();
                                                                    let slot_idx = slot_index as usize;
                                                                    move |evt: Event<MouseData>| {
                                                                        if evt.modifiers().shift() || *select_mode.peek() {
                                                                            crate::picker_queue::press(&mut queue.write(), pick.clone());
                                                                            let marked = !queue.read().is_empty();
                                                                            select_mode.set(marked);
                                                                            return;
                                                                        }
                                                                        // Specials are never attuned and carry no craft level;
                                                                        // the header's Rel stepper is their level offset.
                                                                        let enhancement = coh_data::Enhancement::special(
                                                                            &id, &def, category, defaults.relative_level(),
                                                                        );
                                                                        place_pick(session, compare, destination, &powerset_id, &power_id, slot_idx, enhancement);
                                                                        on_close.call(());
                                                                    }
                                                                },
                                                                if let Some(url) = &icon_url {
                                                                    // The beta's composited enhancement icon: family base
                                                                    // image under the family's overlay frame.
                                                                    span { class: "picker-enh-icon",
                                                                        img { src: "{url}", alt: "{def.name}" }
                                                                        img {
                                                                            src: crate::view::icons::special_overlay_url(category),
                                                                            alt: "",
                                                                            aria_hidden: "true",
                                                                        }
                                                                    }
                                                                } else {
                                                                    "{def.name}"
                                                                }
                                                                if let Some(at) = &queued_at {
                                                                    span { class: "picker-queue-badge", "{at}" }
                                                                }
                                                            }
                                                            }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        },
                        // Origins accept the same aspect vocabulary as generic IOs (the beta passes
                        // `availableGenericIOs` to the Origin tab); each tier is its own section. The
                        // magnitude is derived by the calc from the export's tier grid, not shown here.
                        PickerTab::Origin => match database.enhancements.as_ref() {
                            None => rsx! {
                                div { class: "picker-body",
                                    div { class: "picker-error", "Enhancement catalog unavailable for this dataset." }
                                }
                            },
                            Some(catalog) => {
                                // Origins share the generic stat icons (the beta rule); the tier
                                // lives in the overlay frame, not the base image. A `None` icon
                                // keeps the text button — never a broken image (Rule 1).
                                let stats: Vec<(String, Option<String>)> = catalog
                                    .generic_ios_for_power(power_def.allowed_enhancements.as_deref())
                                    .into_iter()
                                    .map(|stat| {
                                        let icon = crate::view::icons::stat_icon_url(stat);
                                        (stat.to_string(), icon)
                                    })
                                    .collect();
                                // Read once per render rather than per icon — every button and
                                // overlay frame below wants the same build fact.
                                let character_origin = session.build.read().origin.clone();
                                rsx! {
                                    div { class: "picker-body",
                                        if stats.is_empty() {
                                            div { class: "picker-empty", "This power accepts no origin enhancements." }
                                        } else {
                                            for (label, tier) in ORIGIN_TIERS {
                                                div { key: "{tier}", class: "picker-set-group",
                                                    div { class: "picker-set-name", "{label} ({tier})" }
                                                    div { class: "picker-icon-row",
                                                        for (stat, icon_url) in stats.iter() {
                                                            {
                                                            let pick = crate::picker_queue::QueuedPick::Origin {
                                                                stat: stat.clone(),
                                                                tier: tier.to_string(),
                                                            };
                                                            let queued_at = crate::picker_queue::badge(&queue.read(), &pick);
                                                            rsx! {
                                                            button {
                                                                key: "{stat}",
                                                                class: if icon_url.is_some() { "picker-icon-item" } else { "picker-item picker-piece" },
                                                                class: if queued_at.is_some() { "is-queued" } else { "" },
                                                                title: "{stat} {tier}",
                                                                oncontextmenu: {
                                                                    let pick = pick.clone();
                                                                    move |evt: Event<MouseData>| unmark_one(evt, &mut queue, &mut select_mode, &pick)
                                                                },
                                                                onclick: {
                                                                    let stat = stat.clone();
                                                                    let pick = pick.clone();
                                                                    let power_id = power_id.clone();
                                                                    let powerset_id = powerset_id.clone();
                                                                    let slot_idx = slot_index as usize;
                                                                    let character_origin = character_origin.clone();
                                                                    move |evt: Event<MouseData>| {
                                                                        if evt.modifiers().shift() || *select_mode.peek() {
                                                                            crate::picker_queue::press(&mut queue.write(), pick.clone());
                                                                            let marked = !queue.read().is_empty();
                                                                            select_mode.set(marked);
                                                                            return;
                                                                        }
                                                                        // The calc's value stays origin-independent — origin only
                                                                        // decides the SO's overlay flavor below.
                                                                        let enhancement = coh_data::Enhancement::origin(
                                                                            stat.clone(), tier, character_origin.clone(), defaults.relative_level(),
                                                                        );
                                                                        place_pick(session, compare, destination, &powerset_id, &power_id, slot_idx, enhancement);
                                                                        on_close.call(());
                                                                    }
                                                                },
                                                                if let Some(url) = icon_url {
                                                                    // Stat base image under the tier's overlay frame; the DO/SO
                                                                    // frame carries the build's own origin, defaulting to
                                                                    // Natural when none is set (the beta default).
                                                                    span { class: "picker-enh-icon",
                                                                        img { src: "{url}", alt: "{stat} {tier}" }
                                                                        img {
                                                                            src: crate::view::icons::origin_overlay_url(tier, character_origin.as_deref()),
                                                                            alt: "",
                                                                            aria_hidden: "true",
                                                                        }
                                                                    }
                                                                    span { class: "picker-icon-caption", "{stat}" }
                                                                } else {
                                                                    "{stat} {tier}"
                                                                }
                                                                if let Some(at) = &queued_at {
                                                                    span { class: "picker-queue-badge", "{at}" }
                                                                }
                                                            }
                                                            }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        },
                    }
                }
                // The marks, and what spending them would do. Present only while something is
                // marked, so the picker's resting state is unchanged. It states the slot count
                // before the act rather than reporting it after, because the marks can outnumber
                // the slots and the surplus is silently dropped — which is fine to do and not
                // fine to do quietly.
                if !queue.read().is_empty() {
                    div { class: "picker-queue-bar",
                        span { class: "picker-queue-count",
                            "{queue.read().len()} marked"
                        }
                        if open_slots.len() < queue.read().len() {
                            span { class: "picker-queue-warn",
                                "only {open_slots.len()} empty slot(s) — the rest won't fit"
                            }
                        }
                        // Takes back the newest mark. The touch route for removing one copy of a
                        // repeated IO, where a second tap adds rather than unmarks; right-click
                        // on the tile is the desktop accelerator.
                        button {
                            class: "picker-clear",
                            r#type: "button",
                            onclick: move |_| {
                                queue.write().pop();
                                let marked = !queue.peek().is_empty();
                                select_mode.set(marked);
                            },
                            "Undo last"
                        }
                        button {
                            class: "picker-clear",
                            r#type: "button",
                            onclick: move |_| {
                                queue.write().clear();
                                select_mode.set(false);
                            },
                            "Clear marks"
                        }
                        button {
                            class: "picker-queue-commit",
                            r#type: "button",
                            disabled: open_slots.is_empty(),
                            onclick: {
                                let powerset_id = powerset_id.clone();
                                let power_id = power_id.clone();
                                let open_slots = open_slots.clone();
                                let database = database.clone();
                                move |_| {
                                    let picks = queue.read().clone();
                                    let built = placeable_picks(
                                        &database, &picks, &slots, destination, defaults, &session,
                                    );
                                    let placements: Vec<(usize, coh_data::Enhancement)> = open_slots
                                        .iter()
                                        .copied()
                                        .zip(built)
                                        .collect();
                                    if placements.is_empty() {
                                        return;
                                    }
                                    place_picks(
                                        session,
                                        compare,
                                        destination,
                                        &powerset_id,
                                        &power_id,
                                        placements,
                                    );
                                    on_close.call(());
                                }
                            },
                            "Slot {open_slots.len().min(queue.read().len())} into this power"
                        }
                    }
                }
                div { class: "picker-footer",
                    // The slot's clear route, and the ONLY one a touch device has: the
                    // desktop right-click that used to be the sole way to empty a slot never
                    // fires on a finger, so the picker a tap already opens is where the act
                    // belongs. Right-click survives beside it as a desktop accelerator, and
                    // both go through `clear_pick`, so the two cannot disagree about where
                    // the clear lands. Absent on an empty slot rather than dead — there is
                    // nothing to empty, and a control that cannot act is a control that has
                    // to explain itself.
                    if slot_filled {
                        button {
                            class: "picker-clear",
                            r#type: "button",
                            onclick: {
                                let powerset_id = powerset_id.clone();
                                let power_internal_name = power_internal_name.clone();
                                move |_| {
                                    clear_pick(
                                        session,
                                        compare,
                                        destination,
                                        &powerset_id,
                                        &power_internal_name,
                                        slot_index as usize,
                                    );
                                    on_close.call(());
                                }
                            },
                            "Empty this slot"
                        }
                    }
                    button {
                        class: "picker-close",
                        onclick: move |_| on_close.call(()),
                        "Close"
                    }
                }
            }
        }
    }
}

/// The CSS class suffix naming an enhancement's type — the slot chip's frame and the
/// tooltip's type colour key on it (`.slot--io-set`, `.slot-tooltip-type--io-set`, …).
/// Exhaustive by construction: a new [`coh_data::EnhancementKind`] variant must add its
/// token here or the build breaks, never falling through to a default.
pub(crate) fn enhancement_type_class(kind: &coh_data::EnhancementKind) -> &'static str {
    match kind {
        coh_data::EnhancementKind::IoSet { .. } => "io-set",
        coh_data::EnhancementKind::GenericIo { .. } => "generic",
        coh_data::EnhancementKind::Special { .. } => "special",
        coh_data::EnhancementKind::Origin { .. } => "origin",
    }
}

/// The human label for an enhancement's family — the tooltip's type line (beta
/// `ENHANCEMENT_TYPE_LABEL`). The beta appends the border color to the label
/// (`"IO Set (yellow border)"`); we drop that — the label is colored by its
/// `--enh-*` token, which a theme can remap, so a hardcoded color word would rot.
/// Exhaustive like [`enhancement_type_class`]: a new [`coh_data::EnhancementKind`]
/// variant must name itself here or the build breaks (Rule 1).
fn enhancement_type_label(kind: &coh_data::EnhancementKind) -> &'static str {
    match kind {
        coh_data::EnhancementKind::IoSet { .. } => "IO Set",
        coh_data::EnhancementKind::GenericIo { .. } => "Generic IO",
        coh_data::EnhancementKind::Special { .. } => "Special",
        coh_data::EnhancementKind::Origin { .. } => "Origin",
    }
}

/// The slotted enhancement's base icon (beta `SlottedEnhancementIcon`): an io-set piece uses
/// its set's icon (resolved from the catalog), a generic/origin piece its stat icon, a special
/// its family icon (derived from the def id its `id` carries behind the `{category}-` prefix —
/// the beta id shape). `None` for an unresolvable set/stat/id — the caller keeps its text chip,
/// never a wrong or broken image (Rule 1). Derived from the def, not stored on the slot. Shared
/// by the power cards and the Compare Slotting rows, so a piece looks the same in both.
pub(crate) fn slot_icon_url(enhancement: &coh_data::Enhancement, database: &Db) -> Option<String> {
    match &enhancement.kind {
        coh_data::EnhancementKind::IoSet { set_id, .. } => database
            .io_sets
            .as_ref()
            .and_then(|catalog| catalog.get(set_id))
            .and_then(|set| crate::view::icons::io_set_icon_url(&set.icon)),
        coh_data::EnhancementKind::GenericIo { stat, .. }
        | coh_data::EnhancementKind::Origin { stat, .. } => crate::view::icons::stat_icon_url(stat),
        coh_data::EnhancementKind::Special { category, .. } => enhancement
            .id
            .strip_prefix(&format!("{category}-"))
            .and_then(|def_id| crate::view::icons::special_icon_url(category, def_id)),
    }
}

/// The overlay frame a slotted enhancement composites over its base icon — the same frame the
/// picker draws, so a piece reads the same rarity in the build as in the selection screen.
/// Mirrors the beta `getOverlayPath` per kind: an io-set piece its set's rarity frame (attuned
/// when the piece is attuned or the set is attuned-only), a generic IO the plain frame, a
/// special its family frame, an origin the tier's frame carrying the character's origin. Only
/// rendered when the base icon resolves — the caller gates on [`slot_icon_url`].
pub(crate) fn slot_overlay_url(
    enhancement: &coh_data::Enhancement,
    database: &Db,
    character_origin: Option<&str>,
) -> String {
    match &enhancement.kind {
        coh_data::EnhancementKind::IoSet { set_id, .. } => {
            match database
                .io_sets
                .as_ref()
                .and_then(|catalog| catalog.get(set_id))
            {
                Some(set) => crate::view::icons::io_set_overlay_url(
                    &set.category,
                    &set.icon,
                    enhancement.attuned || set.attuned_only,
                ),
                // A set the dataset doesn't carry has no base icon either, so this frame is
                // never shown; the plain frame is the beta's fallback for an unknown rarity.
                None => crate::view::icons::plain_io_frame_url(),
            }
        }
        coh_data::EnhancementKind::GenericIo { .. } => crate::view::icons::plain_io_frame_url(),
        coh_data::EnhancementKind::Special { category, .. } => {
            crate::view::icons::special_overlay_url(category)
        }
        coh_data::EnhancementKind::Origin { tier, .. } => {
            crate::view::icons::origin_overlay_url(tier, character_origin)
        }
    }
}

/// The stat-color token that hue-codes a proc effect's `category` — the beta
/// `getProcOutlineColor` ([enhancement-outline.ts](../../CoH-Sidekick/src/utils/enhancement-outline.ts)),
/// mapped onto the `--stat-*` tokens (which already equal the beta's per-category hex, so the dot
/// re-skins with a theme instead of carrying raw hex). Matches on the export `ProcEffectCategory`
/// strings, never a power proper noun (Rule 0); an unknown/absent category recedes to neutral
/// rather than inventing a hue.
fn proc_category_color(category: &str) -> &'static str {
    match category {
        "Damage" => "var(--stat-damage)",
        "Heal" | "Regeneration" | "Absorb" => "var(--stat-heal)",
        "Endurance" | "Recovery" => "var(--stat-endurance)",
        "Defense" => "var(--stat-defense)",
        "Resistance" => "var(--stat-resistance)",
        "Control" => "var(--stat-mez)",
        "ToHit" => "var(--stat-tohit)",
        "Recharge" => "var(--stat-movement)",
        _ => "var(--stat-neutral)",
    }
}

/// The `background` value for a slotted io-set piece's top-right indicator dot — the beta's
/// `getEnhancementOutline`, the replacement for the old persistent remove button (removal is now
/// right-click only). A **proc** takes its primary effect category's color, with a 135° split to a
/// distinct secondary color when the proc lands two effects (Aegis = Resistance + Mez); a proc with
/// no resolvable data recedes to neutral. A non-proc **unique** takes 50%-opacity gold (beta
/// `rgba(251,191,36,0.5)` = `--stat-special` half-mixed). Everything else has no dot. The proc's
/// category is read from the export proc database, so the color is data-derived, never guessed.
/// `None` ⇒ render no dot.
pub(crate) fn slot_indicator_style(
    enhancement: &coh_data::Enhancement,
    procs: &coh_data::ProcDatabase,
) -> Option<String> {
    let coh_data::EnhancementKind::IoSet {
        set_name,
        is_proc,
        is_unique,
        ..
    } = &enhancement.kind
    else {
        return None;
    };
    if *is_proc {
        let effects = procs
            .find(&enhancement.name, set_name)
            .map(|data| &data.effects);
        let primary = effects
            .and_then(|effects| effects.first())
            .map_or("var(--stat-neutral)", |effect| {
                proc_category_color(&effect.category)
            });
        let secondary = effects
            .and_then(|effects| effects.get(1))
            .map(|effect| proc_category_color(&effect.category));
        Some(match secondary {
            Some(secondary) if secondary != primary => {
                format!("linear-gradient(135deg, {primary} 50%, {secondary} 50%)")
            }
            _ => primary.to_string(),
        })
    } else if *is_unique {
        Some("color-mix(in srgb, var(--stat-special) 50%, transparent)".to_string())
    } else {
        None
    }
}

/// Effect-bag keys whose presence implies a persistent buff/effect applied to the *caster* — a
/// click power that carries one of these (or a self-directed penalty) gets a toggle. Ported
/// field-for-field from the beta `CASTER_BUFF_KEYS` ([power-row-utils.ts](../../CoH-Sidekick/src/components/powers/power-row-utils.ts));
/// every entry is an export schema field name, never a power proper noun (Rule 0). The
/// `*Buff`-suffixed fields plus the unsuffixed top-level fields some powers use in their place
/// (Healing Flames stores `resistance`, not `resistanceBuff`), the `+Strength` self-buff
/// container (`specialBuff` — toggled to push its strength onto other powers), movement, stealth,
/// and mez/debuff-resistance keys.
const CASTER_BUFF_KEYS: &[&str] = &[
    // Standard *Buff fields
    "tohitBuff",
    "tohitBuffUnenhanced",
    "damageBuff",
    "defenseBuff",
    "defenseBuffSuppressible",
    "rechargeBuff",
    "recoveryBuff",
    "recoveryBuffUnenhanced",
    "regenBuff",
    "regenBuffUnenhanced",
    "speedBuff",
    "enduranceBuff",
    "enduranceGain",
    "maxHPBuff",
    "maxEndBuff",
    "rangeBuff",
    "enduranceDiscount",
    "threatBuff",
    "perceptionBuff",
    "absorb",
    // Unsuffixed top-level fields (used by some powers in place of *Buff)
    "defense",
    "resistance",
    // +Strength self-buff container (Power Boost family) — no flat *Buff fields, but toggling
    // it applies its strength to your other powers, so it must be activatable.
    "specialBuff",
    // Movement buffs
    "runSpeed",
    "flySpeed",
    "jumpHeight",
    "jumpSpeed",
    "fly",
    "movementControl",
    "movementFriction",
    // Stealth
    "stealthPvE",
    "stealthPvP",
    "translucency",
    // Mez/debuff resistance
    "mezResistance",
    "debuffResistance",
];

/// `targetType` values where the power can't be cast on self — buffs land on allies only, so the
/// caster gets no persistent effect and no toggle (beta `ALLY_ONLY_TARGETS`).
const ALLY_ONLY_TARGETS: &[&str] = &["ally", "ally (alive)"];

/// True when the power directly deals damage to enemies (beta `isDamagingAttack`). Used to skip
/// the per-cast `damageBuff` (a Defiance/Containment proc, not a persistent self-buff) and
/// `rangeBuff` (the Fast Snipe range bump) on attacks. Heal-type damage entries (Dull Pain, Dark
/// Regeneration) are one-shot heals, not attacks, so they don't count.
fn is_damaging_attack(extra: &Map<String, Value>) -> bool {
    let entries: Vec<&Value> = match extra.get("damage") {
        Some(Value::Array(a)) => a.iter().collect(),
        Some(other @ Value::Object(_)) => vec![other],
        _ => return false,
    };
    entries.iter().any(|d| {
        let is_heal = d.get("type").and_then(Value::as_str) == Some("Heal");
        let scale = d.get("scale").and_then(Value::as_f64).unwrap_or(0.0);
        !is_heal && scale > 0.0
    })
}

/// Which switch a card's ON/OFF pill throws.
///
/// One pill, two writers, because a power's caster-facing payload lives in one of two places
/// and the state that gates it differs. A self-buff is switched by its own `is_active`. A
/// summon's aura is not: a click summon is never "active", so the fold is gated on the per-pet
/// opt-in ([`coh_math::buff_pets`]) and the pill writes that key instead. The two are exclusive
/// by construction — the pet form is offered only where the bag gate found nothing — so the
/// pill never means both at once.
#[derive(Clone, PartialEq)]
enum PowerPill {
    Active,
    BuffPet(String),
}

impl PowerPill {
    fn title(&self, on: bool) -> &'static str {
        match (self, on) {
            (PowerPill::Active, true) => "Power active, stats included in calculations",
            (PowerPill::Active, false) => "Power inactive, click to include in stat calculations",
            (PowerPill::BuffPet(_), true) => {
                "Standing in this power's summoned aura, its buffs are in your totals"
            }
            (PowerPill::BuffPet(_), false) => {
                "Summoned aura not counted, click to include its buffs"
            }
        }
    }
}

/// Whether a picked power should show the ON/OFF toggle pill — the beta `shouldShowToggle`
/// ([power-row-utils.ts](../../CoH-Sidekick/src/components/powers/power-row-utils.ts)), ported as
/// a pure fn over the export bag (`Power.extra`):
/// - every toggle power, always;
/// - a click power that lands a persistent buff/penalty on the *caster* — one of
///   [`CASTER_BUFF_KEYS`] present in its `effects`, or a self-directed penalty;
/// - excluded: ally-only buffs (Speed Boost, Fortitude), one-shot damage/heal clicks (Inferno,
///   Dark Regeneration), and the per-cast `damageBuff`/`rangeBuff` procs on attacks.
///
/// It reads only schema field names — `powerType`, `targetType`, `damage` — and the atom
/// router's slot projection (`bag_slots`), never a power proper noun (Rule 0). A power whose bag
/// omits `powerType` (neither toggle nor click) gets no toggle.
fn should_show_toggle(
    power: &coh_data::Power,
    slots: &coh_math::window_slots::WindowSlots,
) -> bool {
    let extra = &power.extra;
    match extra
        .get("powerType")
        .and_then(Value::as_str)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("toggle") => return true,
        Some("click") => {}
        _ => return false,
    }
    let target_type = extra.get("targetType").and_then(Value::as_str);
    // affectsCaster: an ally-only buff never touches the caster.
    if target_type.is_some_and(|t| ALLY_ONLY_TARGETS.contains(&t.to_ascii_lowercase().as_str())) {
        return false;
    }
    // A self-directed penalty (a debuff the caster takes) is a persistent caster state — the same
    // game rule the perma tracker keys on, so it lives once in the engine.
    if slots.has_self_directed_penalty() {
        return true;
    }
    let is_self = target_type.is_none_or(|t| t.eq_ignore_ascii_case("self"));
    let damaging = is_damaging_attack(extra);
    CASTER_BUFF_KEYS.iter().any(|&key| {
        if !slots.present.contains(key) {
            return false;
        }
        // A damage attack's damageBuff/rangeBuff is a per-cast proc, not a persistent self-buff.
        if damaging && (key == "damageBuff" || key == "rangeBuff") {
            return false;
        }
        // `specialBuff` implies a caster buff only when self-targeted; a Foe-targeted positive
        // `specialBuff` is a legacy -Special foe debuff (Benumb, Weaken), not a self-buff.
        if !is_self && key == "specialBuff" {
            return false;
        }
        true
    })
}

/// The ⌂ marker's tooltip for a power-gated grant: which pick holds it open, read from the
/// grant's own gate. Each dotted path's trailing segment resolves to its display name in
/// the same set; a gate naming several enablers joins them by its own operator (`||` reads
/// "or", `&&` "and"). A def with no power path — or none resolvable — falls back to the
/// inherents' static wording rather than inventing an enabler.
fn granted_title(database: &Db, powerset_id: &str, power_def: Option<&coh_data::Power>) -> String {
    let Some(gate) = power_def.and_then(coh_data::granted_powers::requires) else {
        return "Granted, not picked".to_string();
    };
    let enablers: Vec<String> = gate
        .iter()
        .map(|t| &**t)
        .filter(|token| token.contains('.'))
        .filter_map(|path| path.rsplit('.').next())
        .map(|ident| {
            resolve_power_def(database, powerset_id, ident)
                .map(|def| def.name.clone())
                .unwrap_or_else(|| ident.to_string())
        })
        .collect();
    if enablers.is_empty() {
        return "Granted, not picked".to_string();
    }
    let joiner = if gate.iter().any(|t| &**t == "&&") {
        " and "
    } else {
        " or "
    };
    format!("Granted by {}", enablers.join(joiner))
}

/// The perma ring's hover text — the beta `PermaRing` tooltip distilled to a native `title`:
/// whether the power is perma, its enhanced recharge against its duration, and (when short) the
/// +recharge in hand versus the +recharge needed to close the gap.
/// The game's hit-chance ceiling.
const HIT_CAP: f64 = 0.95;

fn hit_alert_title(hit: coh_math::projection::HitChance) -> String {
    format!(
        "{}% chance to hit a {} target, under the 95% cap. More Accuracy or ToHit raises it.",
        format_precision(hit.chance * 100.0, 1),
        crate::view::power_view::level_gap_label(hit.level_diff),
    )
}

fn perma_title(info: &coh_math::perma::PermaInfo) -> String {
    let effective = format_precision(info.effective_recharge, 1);
    let duration = format_precision(info.duration, 1);
    if info.is_perma {
        format!("Perma — {effective}s recharge ≤ {duration}s duration")
    } else {
        let percent = format_precision(info.perma_percent, 0);
        let have = format_precision(info.total_recharge * 100.0, 0);
        let needed = format_precision(info.recharge_needed * 100.0, 0);
        format!(
            "{percent}% to perma — {effective}s recharge vs {duration}s duration (have +{have}%, need +{needed}%)"
        )
    }
}

/// The single display-rounding primitive (the beta `formatPrecision`): round to `max_decimals`,
/// then strip trailing zeros so `4.0` reads `4` and `1.125` reads `1.125`. Rounding precedes
/// stripping so `scale × AT-table` float noise never leaks into the display; a value that
/// collapses to `-0`/`""` normalizes to `0` (the beta's `parseFloat`).
fn format_precision(value: f64, max_decimals: usize) -> String {
    let rounded = format!("{value:.max_decimals$}");
    let trimmed = if rounded.contains('.') {
        rounded.trim_end_matches('0').trim_end_matches('.')
    } else {
        rounded.as_str()
    };
    if trimmed.is_empty() || trimmed == "-0" {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Splice the precise set-bonus value into its pre-rendered description — the beta
/// `formatBonusDesc` + `formatBonusValue` (set bonuses display to 3 decimals). The export's
/// `desc` was rounded at extraction (e.g. `"+4.0% Recovery"`), so its leading `+X.X%` is
/// replaced with the exact value; a description that doesn't start with that pattern is left
/// unchanged (never mangled), and an empty one falls back to `"<stat> +<value>%"`.
///
/// Shared with [`crate::panels::set_bonus_finder`], which describes the same bonuses from the
/// catalog side: one splice, so a tooltip and a search result can never phrase the same bonus
/// two ways.
pub(crate) fn format_bonus_desc(desc: &str, stat: &str, value: f64) -> String {
    let value_str = format_precision(value, 3);
    if desc.is_empty() {
        return format!("{stat} +{value_str}%");
    }
    match strip_leading_signed_percent(desc) {
        Some(rest) => format!("+{value_str}%{rest}"),
        None => desc.to_string(),
    }
}

/// Strip a leading `+<digits/dots>%` (the beta regex `^\+[\d.]+%`), returning the remainder, or
/// `None` when the description doesn't open with that pattern.
fn strip_leading_signed_percent(desc: &str) -> Option<&str> {
    let after_plus = desc.strip_prefix('+')?;
    let digits_len = after_plus
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(after_plus.len());
    if digits_len == 0 {
        return None;
    }
    after_plus[digits_len..].strip_prefix('%')
}

/// A ≤3-char aspect tag for a slot chip: the piece's primary enhancing aspect, truncated for the
/// 24px cell. The fallback inner when no real icon resolves (a special, or an unmapped stat) —
/// the resolvable kinds show their real icon (PC-ICON). Display-only; the full name lands in the
/// hover tooltip (PC3). A pure-proc or power-grant piece carries no flat aspect, so it falls back
/// to a mark rather than a fabricated stat name (never invent data).
pub(crate) fn slot_chip_label(enhancement: &coh_data::Enhancement) -> String {
    let primary: Option<&str> = match &enhancement.kind {
        coh_data::EnhancementKind::GenericIo { stat, .. } => Some(stat),
        coh_data::EnhancementKind::Origin { stat, .. } => Some(stat),
        coh_data::EnhancementKind::IoSet { aspects, .. } => aspects.first().map(String::as_str),
        coh_data::EnhancementKind::Special { aspects, category } => aspects
            .first()
            .map(|a| a.stat.as_str())
            .or(Some(category.as_str())),
    };
    match primary {
        Some(stat) if !stat.is_empty() => stat.chars().take(3).collect(),
        _ => "✦".to_string(),
    }
}

/// Label for a set piece in the picker: its authored piece name, or a `Piece N`
/// fallback for the rare piece the export leaves unnamed (never a fabricated name).
fn piece_label(piece: &coh_data::IoSetPiece) -> String {
    if piece.name.is_empty() {
        format!("Piece {}", piece.num)
    } else {
        piece.name.clone()
    }
}

/// Add a power to whichever chosen set owns it — primary, secondary, one of the pools, or
/// the epic pool — at `level`, the pick the schedule grants it (resolved by the caller, see
/// [`AvailablePowerRow`]), not the build's current level.
///
/// A `powerset_id` no chosen set claims lands in `unclaimed_role` when the caller names one.
/// That is the VEAT branch case and the only one: a branch's sets are offered beside the base
/// pair, nothing chooses them, and their picks live in the base role's list carrying their own
/// set id — which is what makes the build own the branch set
/// ([`coh_data::set_gate`] reads the pick's own set, not its bucket's). Without a role, an
/// unclaimed set adds nothing, since every other row the picker offers comes from a set the
/// build already holds.
pub fn add_power(
    state: &mut coh_data::CharacterState,
    powerset_id: &str,
    internal_name: &str,
    level: u8,
    unclaimed_role: Option<BuildRole>,
) {
    let picked = coh_data::SelectedPower::picked(internal_name, powerset_id, level);
    if state.primary.id.as_deref() == Some(powerset_id) {
        state.primary.powers.push(picked);
    } else if state.secondary.id.as_deref() == Some(powerset_id) {
        state.secondary.powers.push(picked);
    } else if let Some(pool) = state.pools.iter_mut().find(|pool| pool.id == powerset_id) {
        pool.powers.push(picked);
    } else if let Some(epic) = state
        .epic_pool
        .as_mut()
        .filter(|epic| epic.id == powerset_id)
    {
        epic.powers.push(picked);
    } else {
        match unclaimed_role {
            Some(BuildRole::Primary) => state.primary.powers.push(picked),
            Some(BuildRole::Secondary) => state.secondary.powers.push(picked),
            None => {}
        }
    }
}

/// Add a standard power pool to the build, empty. The pool cap is the schedule's
/// [`max_power_pools`](coh_data::LevelingSchedule::max_power_pools), enforced by the
/// selector that calls this (it stops offering pools once the build is full); re-adding a
/// pool the build already holds is a no-op rather than a duplicate.
pub fn add_pool(state: &mut coh_data::CharacterState, id: &str, name: &str) {
    if state.pools.iter().any(|pool| pool.id == id) {
        return;
    }
    state.pools.push(coh_data::PoolSelection {
        id: id.to_string(),
        name: name.to_string(),
        powers: Vec::new(),
    });
}

/// Drop a pool and every power picked from it, freeing those picks for other powers.
fn remove_pool(state: &mut coh_data::CharacterState, id: &str) {
    state.pools.retain(|pool| pool.id != id);
}

/// Choose the build's epic/patron pool, replacing any previous one — a build has exactly
/// one, so switching discards the old pool's picks along with it.
pub fn set_epic_pool(state: &mut coh_data::CharacterState, id: &str, name: &str) {
    if state.epic_pool.as_ref().is_some_and(|epic| epic.id == id) {
        return;
    }
    state.epic_pool = Some(coh_data::PoolSelection {
        id: id.to_string(),
        name: name.to_string(),
        powers: Vec::new(),
    });
}

/// Drop the epic pool and its picks.
fn remove_epic_pool(state: &mut coh_data::CharacterState) {
    state.epic_pool = None;
}

/// Remove a selected power from the state.
///
/// Granted inherents are deliberately unreachable here: the game grants them, so there is no
/// removing one, and the card that renders them offers no remove control.
fn remove_selected_power(
    state: &mut coh_data::CharacterState,
    powerset_id: &str,
    internal_name: &str,
) {
    // Check primary/secondary/pools/epic in order.
    if state.primary.id.as_deref() == Some(powerset_id) {
        state
            .primary
            .powers
            .retain(|p| p.internal_name != internal_name);
        return;
    }
    if state.secondary.id.as_deref() == Some(powerset_id) {
        state
            .secondary
            .powers
            .retain(|p| p.internal_name != internal_name);
        return;
    }
    for pool in state.pools.iter_mut() {
        if pool.id == powerset_id {
            pool.powers.retain(|p| p.internal_name != internal_name);
            return;
        }
    }
    if let Some(epic) = &mut state.epic_pool {
        if epic.id == powerset_id {
            epic.powers.retain(|p| p.internal_name != internal_name);
            return;
        }
    }
    // No bucket names this set, so the pick carries the set on itself — a VEAT branch power,
    // which sits in a role list under its own set id. Needs no role from the caller: the pick
    // states its set, so matching on that finds it wherever it was put.
    for role in [&mut state.primary, &mut state.secondary] {
        role.powers
            .retain(|p| p.internal_name != internal_name || p.powerset != powerset_id);
    }
}

/// The slots the picker is aiming at — the DESTINATION's, not always the build's.
///
/// A Compare Slotting row is a hypothetical arrangement of one power, so every question the
/// picker asks about "what is already in here" has to be asked of that row. Resolved once and
/// passed around, because three readers (the opening rule, the legality rule and the bulk
/// placement's target list) disagreeing about which power they mean is the whole bug class.
fn destination_slots(
    session: BuildSession,
    compare: crate::compare_slotting::CompareSlottingStore,
    destination: PickerDestination,
    powerset_id: &str,
    power_internal_name: &str,
) -> Vec<Option<coh_data::Enhancement>> {
    let build_slots = session
        .build
        .read()
        .selected_power(powerset_id, power_internal_name)
        .map(|power| power.slots.clone())
        .unwrap_or_default();
    match destination {
        PickerDestination::Build => build_slots,
        PickerDestination::CompareCopy { copy_id } => {
            let address = coh_data::power_address(powerset_id, power_internal_name);
            compare
                .0
                .read()
                .rows(&address, &build_slots)
                .iter()
                .find(|row| row.id == copy_id)
                .map(|row| row.slots.clone())
                .unwrap_or_default()
        }
    }
}

/// Build the enhancement a marked pick stands for, stamped with the header's CURRENT settings.
///
/// Late rather than at mark time, so raising the craft level after marking six pieces raises all
/// six. `None` when the pick no longer resolves against this dataset — a catalogue that changed
/// under an open picker drops that mark rather than placing a guess (Rule 1).
fn build_queued_pick(
    database: &Db,
    pick: &crate::picker_queue::QueuedPick,
    defaults: PickerDefaults,
    session: &BuildSession,
) -> Option<coh_data::Enhancement> {
    use crate::picker_queue::QueuedPick;
    match pick {
        QueuedPick::SetPiece {
            set_id,
            piece_index,
        } => {
            let set = database.io_sets.as_ref()?.get(set_id)?;
            let piece = set.pieces.get(*piece_index)?;
            Some(coh_data::Enhancement::io_set(
                set_id.clone(),
                set.name.clone(),
                *piece_index,
                piece,
                set.min_level,
                set.max_level,
                set.attuned_only,
                defaults.slotting(),
            ))
        }
        QueuedPick::Generic { stat } => Some(coh_data::Enhancement::generic_io(
            stat.clone(),
            Level::new(*defaults.io_level.peek()),
            *defaults.boost.peek(),
        )),
        QueuedPick::Special { id, category } => {
            let def = database
                .enhancements
                .as_ref()?
                .special_families()
                .into_iter()
                .find(|(_, family, _)| family == category)
                .and_then(|(_, _, map)| map.get(id).cloned())?;
            Some(coh_data::Enhancement::special(
                id,
                &def,
                category,
                defaults.relative_level(),
            ))
        }
        QueuedPick::Origin { stat, tier } => Some(coh_data::Enhancement::origin(
            stat.clone(),
            tier,
            session.build.read().origin.clone(),
            defaults.relative_level(),
        )),
    }
}

/// The marked picks that can still be placed, in order, each graded against the slots AS THEY
/// FILL rather than against the slots as they were when the mark was made.
///
/// Two windows close here. A mark can go stale while the picker is open — its twin slotted into
/// this power, a unique spent elsewhere in the build — and a drag range sweeps pieces the pointer
/// never stopped on, which is how the beta placed a Superior ATO piece that was already in the
/// power (2026-08-17). And the batch can conflict with ITSELF: a set's piece 1 and its Superior
/// twin's piece 1 are both legal against a build holding neither, and illegal together. Threading
/// the running slots through the rule is what makes the second pick see the first.
///
/// Only set pieces are graded. A generic IO, an origin and a special carry no single-copy rule —
/// two commons of the same stat in one power is a real slotting, not a mistake.
/// Right-click on a marked tile takes one copy back off the queue. Only when something is marked:
/// on an unmarked tile the browser's own menu is left alone.
fn unmark_one(
    evt: Event<MouseData>,
    queue: &mut Signal<Vec<crate::picker_queue::QueuedPick>>,
    select_mode: &mut Signal<bool>,
    pick: &crate::picker_queue::QueuedPick,
) {
    if !queue.peek().contains(pick) {
        return;
    }
    evt.prevent_default();
    crate::picker_queue::unmark_last(&mut queue.write(), pick);
    let marked = !queue.peek().is_empty();
    select_mode.set(marked);
}

fn placeable_picks(
    database: &Db,
    picks: &[crate::picker_queue::QueuedPick],
    slots: &[Option<coh_data::Enhancement>],
    destination: PickerDestination,
    defaults: PickerDefaults,
    session: &BuildSession,
) -> Vec<coh_data::Enhancement> {
    let build = session.build.read();
    let mut running = slots.to_vec();
    let mut placeable = Vec::new();
    for pick in picks {
        let Some(enhancement) = build_queued_pick(database, pick, defaults, session) else {
            continue;
        };
        if let crate::picker_queue::QueuedPick::SetPiece {
            set_id,
            piece_index,
        } = pick
        {
            let legal = database
                .io_sets
                .as_ref()
                .and_then(|catalog| {
                    let set = catalog.get(set_id)?;
                    let piece = set.pieces.get(*piece_index)?;
                    let scope = match destination {
                        PickerDestination::Build => coh_data::slotting_rules::UniqueScope::Build {
                            state: &build,
                            catalog,
                        },
                        PickerDestination::CompareCopy { .. } => {
                            coh_data::slotting_rules::UniqueScope::Isolated
                        }
                    };
                    Some(coh_data::slotting_rules::piece_slottable_now(
                        set_id, piece, &running, &scope,
                    ))
                })
                .unwrap_or(false);
            if !legal {
                continue;
            }
        }
        running.push(Some(enhancement.clone()));
        placeable.push(enhancement);
    }
    placeable
}

/// Land several picked enhancements at once, as ONE undo step.
///
/// One user action is one history entry — the rule the pool picker already holds to, where taking
/// a power adds the pool and the pick together. Spending six marks through [`place_pick`] six
/// times would leave a build the user has to undo six times to get back out of, which reads as
/// the button having fired repeatedly.
fn place_picks(
    session: BuildSession,
    mut compare: crate::compare_slotting::CompareSlottingStore,
    destination: PickerDestination,
    powerset_id: &str,
    power_internal_name: &str,
    placements: Vec<(usize, coh_data::Enhancement)>,
) {
    match destination {
        PickerDestination::Build => {
            let powerset_id = powerset_id.to_string();
            let power_internal_name = power_internal_name.to_string();
            session.commit(move |state| {
                for (slot_index, enhancement) in placements {
                    set_slot_enhancement(
                        state,
                        &powerset_id,
                        &power_internal_name,
                        slot_index,
                        enhancement,
                    );
                }
            });
        }
        PickerDestination::CompareCopy { copy_id } => {
            let address = coh_data::power_address(powerset_id, power_internal_name);
            // Re-read between writes: each `edit_slot` rewrites the row, so a stale copy of the
            // build slots would undo the placement before it.
            for (slot_index, enhancement) in placements {
                let build_slots = session
                    .build
                    .peek()
                    .selected_power(powerset_id, power_internal_name)
                    .map(|power| power.slots.clone())
                    .unwrap_or_default();
                compare.0.write().edit_slot(
                    &address,
                    &build_slots,
                    copy_id,
                    slot_index,
                    Some(enhancement),
                );
            }
        }
    }
}

/// Land a picked enhancement where the picker was aimed ([`PickerDestination`]): the live build
/// as one undo step, or one Compare Slotting row — which is scratch state, so it goes through
/// the compare store and never touches the build or its history. The row write reads the power's
/// REAL slots first because the store's row 0 is a mirror seeded from them (see
/// [`crate::compare_slotting::CompareSlottingState`]).
fn place_pick(
    session: BuildSession,
    mut compare: crate::compare_slotting::CompareSlottingStore,
    destination: PickerDestination,
    powerset_id: &str,
    power_internal_name: &str,
    slot_index: usize,
    enhancement: coh_data::Enhancement,
) {
    match destination {
        PickerDestination::Build => {
            let powerset_id = powerset_id.to_string();
            let power_internal_name = power_internal_name.to_string();
            session.commit(move |state| {
                set_slot_enhancement(
                    state,
                    &powerset_id,
                    &power_internal_name,
                    slot_index,
                    enhancement,
                );
            });
        }
        PickerDestination::CompareCopy { copy_id } => {
            let address = coh_data::power_address(powerset_id, power_internal_name);
            let build_slots = session
                .build
                .peek()
                .selected_power(powerset_id, power_internal_name)
                .map(|power| power.slots.clone())
                .unwrap_or_default();
            compare.0.write().edit_slot(
                &address,
                &build_slots,
                copy_id,
                slot_index,
                Some(enhancement),
            );
        }
    }
}

/// Empty the slot the picker is aimed at, through the same routing point
/// [`place_pick`] uses — so a clear from the picker lands where that picker's picks
/// land, and a Compare row can never be emptied into the build. The two are one act
/// in opposite directions, and writing them as one pair is what keeps the
/// destination rule stated once.
fn clear_pick(
    session: BuildSession,
    mut compare: crate::compare_slotting::CompareSlottingStore,
    destination: PickerDestination,
    powerset_id: &str,
    power_internal_name: &str,
    slot_index: usize,
) {
    match destination {
        PickerDestination::Build => {
            let powerset_id = powerset_id.to_string();
            let power_internal_name = power_internal_name.to_string();
            session.commit(move |state| {
                remove_enhancement_from_slot(state, &powerset_id, &power_internal_name, slot_index);
            });
        }
        PickerDestination::CompareCopy { copy_id } => {
            let address = coh_data::power_address(powerset_id, power_internal_name);
            let build_slots = session
                .build
                .peek()
                .selected_power(powerset_id, power_internal_name)
                .map(|power| power.slots.clone())
                .unwrap_or_default();
            compare
                .0
                .write()
                .edit_slot(&address, &build_slots, copy_id, slot_index, None);
        }
    }
}

/// Place an enhancement into a power's slot at `slot_index`, replacing whatever it
/// held. The caller builds the [`coh_data::Enhancement`] from export data (e.g.
/// [`coh_data::Enhancement::generic_io`]); this only positions it. Out-of-range
/// indices are ignored — a slot can only be edited once it exists.
fn set_slot_enhancement(
    state: &mut coh_data::CharacterState,
    powerset_id: &str,
    power_internal_name: &str,
    slot_index: usize,
    enhancement: coh_data::Enhancement,
) {
    if let Some(power) = state.selected_power_mut(powerset_id, power_internal_name) {
        if let Some(slot) = power.slots.get_mut(slot_index) {
            *slot = Some(enhancement);
        }
    }
}

/// Clear a power's slot at `slot_index` back to empty.
fn remove_enhancement_from_slot(
    state: &mut coh_data::CharacterState,
    powerset_id: &str,
    power_internal_name: &str,
    slot_index: usize,
) {
    if let Some(power) = state.selected_power_mut(powerset_id, power_internal_name) {
        if let Some(slot) = power.slots.get_mut(slot_index) {
            *slot = None;
        }
    }
}

/// Empty every slot on one power, keeping the slots themselves (the beta's
/// `handleClearAllEnhancements`, which is per-power there too despite being reached from a
/// slot). Distinct from [`remove_slot_from_power`] in exactly the way
/// [`remove_enhancement_from_slot`] is: the budget is untouched, because a slot emptied is
/// still a slot spent.
fn clear_all_enhancements(
    state: &mut coh_data::CharacterState,
    powerset_id: &str,
    power_internal_name: &str,
) {
    if let Some(power) = state.selected_power_mut(powerset_id, power_internal_name) {
        for slot in power.slots.iter_mut() {
            *slot = None;
        }
    }
}

/// Append an empty slot to a power, gated on BOTH the structural per-power cap
/// ([`coh_data::MAX_USER_SLOTS_PER_POWER`] user slots + inherent grants, which
/// [`coh_data::CharacterState::validate`] enforces) AND the build-wide
/// `slot_budget` (the level-gated pool the caller reads from the dataset's
/// [`coh_data::LevelingSchedule`]). A power's free base slot and inherent
/// auto-slots don't spend the budget — [`coh_data::placed_budget_slots`] counts
/// only user-placed slots, so the same slot the game grants for free is free here.
///
/// The cap here is the STRUCTURAL ceiling, not the operative one: a power's real limit is
/// its own exported `maxSlots` (6, 4, or none at all), which [`PickedPowerCard`] reads off
/// the def to decide whether to offer a `+` in the first place. This function takes no
/// database handle and so cannot see that; it is the backstop, not the rule.
fn add_slot_to_power(
    state: &mut coh_data::CharacterState,
    powerset_id: &str,
    power_internal_name: &str,
    slot_budget: usize,
    leveling: Option<&coh_data::LevelingSchedule>,
    refuse_unplaced: bool,
) {
    if coh_data::placed_budget_slots(state) >= slot_budget {
        return;
    }
    let Some(category) = SlotCategory::of(state, powerset_id, power_internal_name) else {
        return;
    };
    let Some(pick) = state
        .selected_power(powerset_id, power_internal_name)
        .filter(|power| {
            power.slots.len()
                < coh_data::MAX_USER_SLOTS_PER_POWER + power.inherent_slot_count as usize
        })
        .map(|power| power.level)
    else {
        return;
    };
    // The slot takes the earliest grant the build can spare. Freezing first pins every other
    // slot where it sits, so the new one cannot quietly re-house them. When no grant is left
    // the slot is still placed and wears the unplaced mark — except in level-up mode, which
    // refuses it the way the budget above refuses (the beta `addSlot`).
    let level = match leveling {
        Some(schedule) => {
            slot_levels::freeze(schedule, state);
            let level = slot_levels::next_grant_level(schedule, state, pick);
            if level.is_none() && refuse_unplaced {
                return;
            }
            level
        }
        None => None,
    };
    let Some(power) = state.selected_power_mut(powerset_id, power_internal_name) else {
        return;
    };
    // The new slot goes at the end of the user's band, ahead of the trailing inherent
    // auto-slots, so the auto-slots stay the ones the game granted.
    let index = power
        .slots
        .len()
        .saturating_sub(power.inherent_slot_count as usize)
        .max(1);
    power.slots.insert(index, None);
    slot_levels::record_added_slot(state, category, power_internal_name, index, level);
}

/// Remove the power's last user-placed slot, freeing a budget slot (the beta
/// `removeSlot`). The free base slot (index 0) is never removable, and inherent
/// auto-slots (trailing) belong to the game, not the user — so this removes the
/// slot just before the trailing inherent block, and is a no-op when the power
/// carries only its base + inherent slots. Distinct from
/// [`remove_enhancement_from_slot`], which clears a slot's enhancement but keeps
/// the slot.
fn remove_slot_from_power(
    state: &mut coh_data::CharacterState,
    powerset_id: &str,
    power_internal_name: &str,
    leveling: Option<&coh_data::LevelingSchedule>,
) {
    let Some(last_user_index) =
        state
            .selected_power(powerset_id, power_internal_name)
            .map(|power| {
                power
                    .slots
                    .len()
                    .saturating_sub(1 + power.inherent_slot_count as usize)
            })
    else {
        return;
    };
    remove_slot_at(
        state,
        powerset_id,
        power_internal_name,
        last_user_index,
        leveling,
    );
}

/// Remove the user slot at `index` — the beta's right-click-on-empty `onRemoveSlot`, gated on
/// `canRemoveSlot = index > 0`. Refuses the free base slot (index 0) and the trailing inherent
/// auto-slots (which belong to the game, not the user), so it only ever removes a slot the user
/// placed. Distinct from [`remove_slot_from_power`], which pops the *last* user slot for the card
/// stepper's leftward drag; this removes a *specific* index (the slot the user right-clicked).
fn remove_slot_at(
    state: &mut coh_data::CharacterState,
    powerset_id: &str,
    power_internal_name: &str,
    index: usize,
    leveling: Option<&coh_data::LevelingSchedule>,
) {
    if index == 0 {
        return;
    }
    let Some(category) = SlotCategory::of(state, powerset_id, power_internal_name) else {
        return;
    };
    let removable = state
        .selected_power(powerset_id, power_internal_name)
        .is_some_and(|power| {
            index
                < power
                    .slots
                    .len()
                    .saturating_sub(power.inherent_slot_count as usize)
        });
    if !removable {
        return;
    }
    // Pin every other slot's level first, so the ones left keep the level they were placed at
    // and the freed grant is what the next placement draws (the Mids behaviour).
    if let Some(schedule) = leveling {
        slot_levels::freeze(schedule, state);
    }
    if let Some(power) = state.selected_power_mut(powerset_id, power_internal_name) {
        power.slots.remove(index);
    }
    slot_levels::forget_removed_slot(state, category, power_internal_name, index);
}
