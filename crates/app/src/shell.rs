//! The app shell: header (the ☰ main menu, then identity, Build Settings + inline toggles,
//! level, undo-redo, Tools, Display, then the brand), the quickbar (`crate::quickbar` — the
//! row of whatever the user pinned), then the dual-layout grid.
//! Desktop (≥900px): free 2D grid of absolutely-positioned surfaces (`grid::view`).
//! Mobile (<900px): independent flat MobileOrder with reorder menu overlay.
//!
//! The header holds controls that describe the BUILD (who it is, what level, undo) plus the
//! one menu that describes the APP (`crate::main_menu`); the quickbar holds what you do to the
//! build on screen. That split is why the analysis modals and the stat config are not up here.
//!
//! Row 1's own reorganisation is
//! HM, and it is done: HM1 landed the ☰ menu,
//! HM2 moved Dataset into identity, HM3 renamed Combat to Build Settings with a derived drift
//! summary and inline toggles, and HM4/HM5 split the quickbar's old More roster into the Tools
//! and Display menus on this row — which is why Reset to default and Reorder now fold into Display
//! rather than sitting as loose buttons here.

use crate::build_bar::BuildBar;
use crate::build_session::BuildSession;
use crate::build_store;
use crate::data_source;
use crate::grid::{self, PanelKind};
use crate::history::History;
use crate::layout_store;
use crate::mobile_nav::MobileNav;
use crate::mobile_order::MobileOrder;
use crate::panels;
use crate::picker_store;
use crate::pinned_powers::{PermaTracked, PinnedPowersStrip};
use crate::reorder_menu;
use coh_data::{build_store as envelope, CharacterState, DatasetId, PowerDatabase, StoredBuilds};
use dioxus::prelude::*;
use std::sync::Arc;

/// The Sidekick mark, at the size the header draws it. The 64px cut rather than the 1080px
/// master (`img/icon_sidekick.png`): a mark this small (see `.brand-mark`) needs no more than
/// this even at 2x, and the master is 80x the bytes.
static BRAND_ICON: Asset = asset!("/assets/img/favicon-64x64.png");

/// Document-level shortcuts, skipped while an editable field holds focus so typing keeps its
/// native keys. Posts back to Rust via `dioxus.send`:
///   - undo/redo (beta `useUndoRedoKeyboard`): Ctrl/Cmd+Z undoes, Ctrl/Cmd+Y or
///     Ctrl/Cmd+Shift+Z redoes;
///   - `"lock"` (beta `useInfoPanelLockHotkey`): a bare L, no modifier, toggles the Info panel's
///     lock — see [`InfoLock`].
const SHELL_KEYS: &str = "\
document.addEventListener('keydown', (e) => {\
  const t = e.target;\
  const tag = t && t.tagName;\
  if (t && (t.isContentEditable || tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT')) return;\
  if ((e.key === 'l' || e.key === 'L') && !e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey) { dioxus.send('lock'); return; }\
  if (!(e.ctrlKey || e.metaKey)) return;\
  const z = e.key === 'z' || e.key === 'Z';\
  const y = e.key === 'y' || e.key === 'Y';\
  if (z && !e.shiftKey) { e.preventDefault(); dioxus.send('undo'); }\
  else if (y || (z && e.shiftKey)) { e.preventDefault(); dioxus.send('redo'); }\
});";

/// The selected power's identity: powerset `id` + the power's resolution identity
/// (`Power::ident`). Identity, not indices — a dataset switch swaps the database under
/// a live selection, and stale indices into the new dataset's powersets are an
/// out-of-bounds panic (a WASM abort). Identity re-resolves per render: the same power
/// in the new dataset when it exists, a visible "not in this dataset" state when not.
#[derive(Debug, Clone, PartialEq)]
pub struct PowerRef {
    pub powerset_id: String,
    pub power: String,
}

pub type Selection = Option<PowerRef>;

/// The Info panel's selected-power target, held at the shell root and provided as context so a
/// picked-power card can drive the Info panel on hover (beta `setInfoPanelContent`) without the
/// grid threading the signal through every surface. The Info panel reads the same signal, so a
/// power can never show two different things across surfaces (the single-projection guarantee).
#[derive(Clone, Copy)]
pub struct PowerSelection(pub Signal<Selection>);

/// The Info panel's lock (beta `infoPanel.locked` / `lockedContent`): a power held in the panel
/// so that hovering other powers stops replacing it. The Info panel shows this when it is set
/// and [`PowerSelection`] otherwise. Hover keeps writing the selection underneath, so unlocking
/// lands on whatever was hovered last — the beta's behaviour too.
///
/// A separate signal rather than a flag on the selection, because the two answer different
/// questions: "what is the pointer on" keeps changing while locked, and the lock needs to
/// remember the power it was set on. Not persisted — the beta resets it on load as well.
#[derive(Clone, Copy)]
pub struct InfoLock(pub Signal<Selection>);

impl InfoLock {
    /// A right-click on a power: lock the panel to it, or unlock if it is already the locked
    /// power. A DIFFERENT power replaces the lock in one step, with no unlock between.
    pub fn toggle_on(self, target: PowerRef) {
        let mut lock = self.0;
        let current = lock.peek().clone();
        lock.set(if current.as_ref() == Some(&target) {
            None
        } else {
            Some(target)
        });
    }

    /// The L key: unlock when locked, otherwise lock to whatever the panel is showing now.
    pub fn toggle_current(self, selection: Signal<Selection>) {
        let mut lock = self.0;
        let current = lock.peek().clone();
        lock.set(match current {
            Some(_) => None,
            None => selection.peek().clone(),
        });
    }

    /// Whether `target` is the locked power — the rows and cards mark themselves with it.
    pub fn holds(self, powerset_id: &str, power: &str) -> bool {
        self.0
            .read()
            .as_ref()
            .is_some_and(|locked| locked.powerset_id == powerset_id && locked.power == power)
    }
}

/// Prop-friendly database handle: Dioxus memoizes props by `PartialEq`, and a loaded
/// dataset only ever changes by being a different `Arc` — pointer equality is exact.
#[derive(Clone)]
pub struct Db(pub Arc<PowerDatabase>);

impl PartialEq for Db {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl std::ops::Deref for Db {
    type Target = PowerDatabase;
    fn deref(&self) -> &PowerDatabase {
        &self.0
    }
}

#[component]
pub fn Shell() -> Element {
    // Before anything can persist. The shell is the only component that mounts in the main
    // window and nowhere else, which is what makes it the place to say so — see
    // [`crate::storage`] for what a write from the wrong window costs.
    //
    // An effect rather than a `use_hook`, and first in the body, because the failure is silent in
    // one direction only: `dioxus::document::document()` falls back to a no-op document when it
    // cannot find one in context, which would leave the app claiming an owner that discards every
    // write it is handed. An effect runs after the first render, when the renderer's document is
    // certainly in context; nothing persists before then, and anything that did would fall
    // through to this document, which in the main window is the same one.
    use_effect(crate::storage::claim_owner);
    // Layout writes start counting as edits at the first pointer or key press (see
    // `layout_sync`), so the app's own writes while it loads never date the layout.
    use_effect(crate::layout_sync::arm_edit_stamps);

    // The session, and who it says is signed in. Built synchronously here so no consumer needs an
    // arm for "not yet"; the storage read and the OAuth callback happen in `start`, below, after
    // mount — the same shape as `claim_owner` above and for the same reason.
    let account = use_context_provider(crate::cloud::account::Account::new);
    use_future(move || {
        let account = account.clone();
        async move { account.start().await }
    });

    let mut dataset = use_signal(|| DatasetId::Homecoming);
    // The committed desktop layout lives here, not inside `GridContainer`, so it
    // survives the grid remounting on a dataset switch (the layout is dataset-independent,
    // exactly like `mobile_order`).
    let mut desktop_layout = use_signal(grid::model::GridItem::default_layout);
    // The count that layout was arranged at, held beside it because it is half of what a
    // saved layout means — cells with no count are not an arrangement. `GridContainer` owns
    // keeping it current from its own width measurement; everything that persists a layout
    // reads it from context (see `GridColumns`).
    let mut grid_columns = use_signal(|| grid::model::GridConfig::default().columns);
    use_context_provider(|| grid::view::GridColumns(grid_columns));
    let mut mobile_order =
        use_signal(|| MobileOrder::default_order(&grid::model::DEFAULT_SURFACES));
    // The reorder overlay's open flag, owned here and offered through both the Display menu's
    // Reorder entry and the reorder overlay it drives (HM5). Local to the shell, like the
    // other popover open flags.
    let reorder_open = use_signal(|| false);
    // The confirm behind the Display menu's Reset to default row: the one destructive act in the
    // app with no Ctrl+Z behind it, so it asks rather than commits (AC2). Local to the shell,
    // like the other overlay open flags.
    let reset_confirm_open = use_signal(|| false);
    // What the user pinned to the quickbar, in their order. Held here for the same reason the
    // layout is, and kept in its OWN signal rather than folded into `desktop_layout`: whether
    // a panel has a pill and whether it is docked are two independent facts, and one store
    // holding both would give a single write path two answers to give.
    let mut quickbar_pins = use_signal(crate::quickbar::model::default_pins);
    let selection: Signal<Selection> = use_signal(|| None);
    // Provided so a picked-power card sets the Info panel's target on hover (PC7); the Info panel
    // reads the same signal, keeping one power's projection identical across surfaces.
    use_context_provider(|| PowerSelection(selection));
    let info_lock: Signal<Selection> = use_signal(|| None);
    let info_lock = use_context_provider(|| InfoLock(info_lock));
    let db = use_resource(move || async move { data_source::load(dataset()).await });

    // The live build-editing session (build + persistence envelope + undo history),
    // provided as context so any panel can edit without the grid threading three signals.
    let build = use_signal(|| CharacterState::empty(DatasetId::Homecoming));
    let stored = use_signal(StoredBuilds::empty);
    let history = use_signal(History::default);
    let session = use_context_provider(|| BuildSession {
        build,
        stored,
        history,
    });
    // The enhancement picker's global slotting defaults (level/attuned/boost); held here so
    // they persist across picker opens, matching the beta's `useUIStore` globals.
    let picker_defaults = use_context_provider(crate::picker_defaults::PickerDefaults::new);
    // Where the picker was last left, per power. Held here rather than in the modal because the
    // modal is unmounted between opens — state that has to outlive the surface cannot live on it.
    let picker_memory = use_context_provider(|| {
        crate::panels::powers::PickerMemoryStore(Signal::new(
            crate::picker_memory::PickerMemory::default(),
        ))
    })
    .0;
    // The single enhancement picker's open target (beta `openEnhancementPicker`). Held at the
    // shell root and rendered by `EnhancementPickerHost` below BOTH layout roots, so the modal's
    // fixed backdrop isn't contained by a free-grid surface's `transform` (which would clip it to
    // the panel). A slot sets this; the host reads it. See `powers::PickerTarget`.
    let picker_open = use_signal(|| Option::<panels::powers::PickerTarget>::None);
    use_context_provider(|| panels::powers::PickerOpen(picker_open));
    // The single hovered-slot tooltip target (PC3), rendered by `SlotTooltipHost` below BOTH
    // layout roots so a slot's rich tooltip escapes the free-grid surface's `transform`/overflow
    // clip — the same escape the picker modal uses. A slot sets this on hover; the host reads it.
    let slot_tooltip = use_signal(|| Option::<panels::powers::SlotTooltipTarget>::None);
    use_context_provider(|| panels::powers::SlotTooltip(slot_tooltip));
    // The one ⋮ menu on a picked power, hosted below both layout roots for the same reason.
    let power_menu = use_signal(|| Option::<panels::powers::PowerMenuTarget>::None);
    use_context_provider(|| panels::powers::PowerMenu(power_menu));
    // The single pool/epic picker, hosted below both layout roots for the same containment
    // reason as the enhancement picker. The Available panel's triggers set this; the host
    // reads it. See `pool_picker::PoolPickerOpen`.
    let pool_picker_open = use_signal(|| Option::<panels::pool_picker::PoolPickerMode>::None);
    use_context_provider(|| panels::pool_picker::PoolPickerOpen(pool_picker_open));
    // The single incarnate picker, hosted below both layout roots for the same containment
    // reason. The Incarnates surface's slot tiles set this; the host reads it. See
    // `incarnate_picker::IncarnatePickerOpen`.
    let incarnate_picker_open = use_signal(|| Option::<String>::None);
    use_context_provider(|| panels::incarnate_picker::IncarnatePickerOpen(incarnate_picker_open));
    let incarnate_crafting_open =
        use_signal(|| Option::<panels::incarnate_crafting::CraftingTab>::None);
    use_context_provider(|| {
        panels::incarnate_crafting::IncarnateCraftingOpen(incarnate_crafting_open)
    });
    // The single accolade picker, same hosting shape. The Available panel's accolade strip
    // sets this; the host reads it. See `accolade_picker::AccoladePickerOpen`.
    let accolade_picker_open = use_signal(|| false);
    use_context_provider(|| panels::accolade_picker::AccoladePickerOpen(accolade_picker_open));
    // The one whole-build recompute, lifted here so the eight stat panels, the status strip, and
    // the power-card slot tooltip share a single `recalculate` (not one per reader). Reads the
    // loaded database and the live build inside the memo, so it re-runs on any edit or dataset
    // switch; yields default totals until the dataset resolves. Provided as context
    // (`BuildTotals`); consumers read it.
    let totals = use_memo(move || match &*db.read() {
        Some(Ok(database)) => coh_math::recalculate(&session.build.read(), database),
        _ => coh_math::CalculatedTotals::default(),
    });
    use_context_provider(|| panels::stats::BuildTotals(totals));
    // Which stats the eight stat panels show, and the config modal's open target. Both live
    // here rather than in a panel: the config is one set shared by every stat panel, and the
    // modal is hosted below both layout roots (see `StatsConfigHost`).
    let stat_config = use_signal(panels::dashboards::Dashboards::defaults);
    use_context_provider(|| panels::dashboards::DashboardConfig(stat_config));
    let stats_config_open = use_signal(|| Option::<panels::stats_config::OrganizerTarget>::None);
    use_context_provider(|| panels::stats_config::StatsConfigOpen(stats_config_open));
    // The detailed stat sheet, hosted below both layout roots for the same containment reason.
    let detailed_totals_open = use_signal(|| false);
    use_context_provider(|| panels::detailed_totals::DetailedTotalsOpen(detailed_totals_open));
    // Which proc categories contribute. Only the open flag is UI — the setting itself lives on
    // the build (`CharacterState::disabled_proc_categories`), since it moves every total.
    let proc_settings_open = use_signal(|| false);
    use_context_provider(|| panels::proc_settings::ProcSettingsOpen(proc_settings_open));
    // The set-bonus finder. Reads the catalog and the build; writes neither, so its open flag is
    // the whole of its state.
    let set_bonus_finder_open = use_signal(|| false);
    use_context_provider(|| panels::set_bonus_finder::SetBonusFinderOpen(set_bonus_finder_open));
    // The Rule-of-5 alert: a persisted preference plus a per-load dismissal (see
    // `RuleOfFiveAlert`). Defaults ON until `alert_store::load` says otherwise, which is the
    // direction that cannot hide a wasted bonus.
    let rule_of_five_enabled = use_signal(|| true);
    let rule_of_five_dismissed = use_signal(|| false);
    use_context_provider(|| RuleOfFiveAlert {
        enabled: rule_of_five_enabled,
        dismissed: rule_of_five_dismissed,
    });
    // UI scale: pure app chrome, same preference standing as the Rule-of-5 alert above —
    // defaults to unscaled until `ui_scale::load` says otherwise.
    let ui_scale_pct = use_signal(|| crate::ui_scale::DEFAULT_PCT);
    use_context_provider(|| UiScale { pct: ui_scale_pct });
    // The Info card's damage reading (DMG/DPA/DPS/DPE): chrome like the scale, restored below.
    let damage_metric = use_signal(crate::view::power_view::DamageMetric::default);
    use_context_provider(|| crate::damage_metric_store::DamageMetricPref(damage_metric));
    // Whether damage procs count toward a power's damage, restored below beside the reading.
    let proc_damage = use_signal(|| true);
    use_context_provider(|| crate::damage_metric_store::ProcDamagePref(proc_damage));
    // Which surfaces are on the grid at all. The flag itself rides in `desktop_layout` (see
    // `GridItem::hidden`); only the modal's open state lives here, provided as context so the
    // Options entry and the all-hidden prompt in either layout root can both raise it.
    let panel_visibility_open = use_signal(|| false);
    use_context_provider(|| crate::panel_visibility::PanelVisibilityOpen(panel_visibility_open));
    // Which of the hidden surfaces are hidden because they are out in their own window. Not
    // persisted, and POP3's to move into the layout record once it has settled who writes it —
    // see `panel_popout`. Provided unconditionally so the surfaces that consult it need no cfg;
    // on a build with no pop-out control the set is simply always empty.
    let popped_out = use_signal(std::collections::HashSet::new);
    use_context_provider(|| crate::panel_popout::PoppedOut(popped_out));
    // Where each of those windows is, kept apart from the set above so a window drag does not
    // re-render the grid — see [`crate::panel_popout::PoppedGeometry`].
    let popped_geometry = use_signal(std::collections::HashMap::new);
    use_context_provider(|| crate::panel_popout::PoppedGeometry(popped_geometry));
    // The Attack-Chain builder's open flag — the modal itself is hosted below both layout
    // roots like the other overlays; the header button raises it.
    let attack_chain_open = use_signal(|| false);
    use_context_provider(|| panels::attack_chain::AttackChainOpen(attack_chain_open));
    // The what-if team-buff modal's open flag, hosted the same way. The layer it edits lives on
    // the build's `CombatContext`, not here — only the open state is UI.
    let what_if_open = use_signal(|| false);
    use_context_provider(|| panels::what_if::WhatIfOpen(what_if_open));
    // Build I/O: a file read but not yet decodable (its fork is still loading), and the receipt
    // for whatever the last open or save did. Both live at the root because the file outlives
    // the menu that read it — the menu closes, the dataset switches, and the text has to still
    // be here when the new bundle lands.
    let build_io_pending = use_signal(|| Option::<crate::build_io::PendingImport>::None);
    use_context_provider(|| crate::build_io::BuildIoPending(build_io_pending));
    let build_io_report = use_signal(|| Option::<crate::build_io::BuildIoOutcome>::None);
    use_context_provider(|| crate::build_io::BuildIoReport(build_io_report));
    // A file from another fork, parked while the user answers which open they meant. Rooted
    // here with the other two for the same reason: answering "open on its own fork" switches
    // the dataset, and the text has to outlive that.
    let build_io_choice = use_signal(|| Option::<crate::build_io::CrossForkChoice>::None);
    use_context_provider(|| crate::build_io::BuildIoChoice(build_io_choice));
    // The paste surface's open flag. It owns nothing else: what was pasted lives in the surface
    // and goes when it closes, and the text that survives is whatever the seam above parked.
    let paste_build_open = use_signal(|| false);
    use_context_provider(|| crate::build_io::PasteBuildOpen(paste_build_open));
    // The forum-export modal's open flag. It writes nothing: the post is derived from the build,
    // the totals and the visible stats, so the open state is the whole of what it owns.
    let forum_export_open = use_signal(|| false);
    use_context_provider(|| crate::forum_export::ForumExportOpen(forum_export_open));
    // The image-export modal's open flag, owned here for the same reason and owning as little:
    // the poster is derived from the same three things the post is.
    let export_image_open = use_signal(|| false);
    use_context_provider(|| crate::export_image::ExportImageOpen(export_image_open));
    // The enhancement list's open flag, owned here for the same reason. Its tick state is not
    // here: that belongs to a shopping trip rather than to the build, so it lives in the body
    // and goes when the modal closes.
    let enhancement_list_open = use_signal(|| false);
    use_context_provider(|| crate::enhancement_list::EnhancementListOpen(enhancement_list_open));
    // The enhancement tools' open flag. Unlike the three around it this one WRITES the build,
    // but only through `BuildSession::commit` like every other edit, so the flag is still all
    // the state it owns.
    let enhancement_tools_open = use_signal(|| false);
    use_context_provider(|| crate::enhancement_tools::EnhancementToolsOpen(enhancement_tools_open));
    // The powerset comparison's open flag. Like the two above it writes nothing to the build:
    // it projects two powersets the build does not hold against synthetic characters.
    let powerset_compare_open = use_signal(|| false);
    use_context_provider(|| crate::powerset_compare::PowersetCompareOpen(powerset_compare_open));
    // Compare Slotting: the open flag, the power under comparison, and the session-scoped
    // comparison rows. Deliberately unpersisted — see `compare_slotting::CompareSlottingState`.
    // Applying a row goes through `BuildSession::commit` like every other edit.
    let compare_slotting = use_signal(crate::compare_slotting::CompareSlottingState::default);
    use_context_provider(|| crate::compare_slotting::CompareSlottingStore(compare_slotting));
    // The shared-builds browser's open target (RB4d): `None` closed, `Browse` the list,
    // `Build(id)` a single build. Raised by the main menu row, set by the boot-time
    // `/builds/<id>` path reader, and rendered by `BrowserHost` below.
    let browser_open = use_signal(|| Option::<crate::cloud::browser::BrowserTarget>::None);
    use_context_provider(|| crate::cloud::browser::BrowserOpen(browser_open));
    // The save-to-cloud surface's open flag (RB4e). Like the forum and image exports it owns
    // nothing but the flag — the row it writes is derived from the build, and the owner token it
    // keeps goes to `localStorage` rather than into any signal.
    let save_build_open = use_signal(|| false);
    use_context_provider(|| crate::cloud::save_build::SaveBuildOpen(save_build_open));
    // The public-profile editor's open flag (RB4f). Like the save surface it owns nothing but
    // the flag — the row it edits is loaded when the modal opens and the identity it changes is
    // republished through `Account`, not held here.
    let profile_open = use_signal(|| false);
    use_context_provider(|| crate::cloud::profile::ProfileOpen(profile_open));

    // The self-description modals (About / Changelog / Donate). The main menu's tail rows
    // raise them, and each owns nothing but its open flag — they describe the app, not the
    // build, so they need no database.
    let about_open = use_signal(|| false);
    use_context_provider(|| crate::app_info::AboutOpen(about_open));
    let changelog_open = use_signal(|| false);
    use_context_provider(|| crate::app_info::ChangelogOpen(changelog_open));
    // The controls reference: the Tools menu's row raises it, and the body owns nothing but
    // its per-open toggle state — it describes the app's interactions, not the build.
    let controls_open = use_signal(|| false);
    use_context_provider(|| crate::controls::ControlsOpen(controls_open));
    // The guide. Two surfaces raise it — the footer's Help action and the main menu's Help row —
    // which is why the flag is here rather than in either of them: a signal owned by one opener
    // is a second opener with nothing to set.
    let help_open = use_signal(|| false);
    use_context_provider(|| crate::help::HelpOpen(help_open));
    let donate_open = use_signal(|| false);
    use_context_provider(|| crate::app_info::DonateOpen(donate_open));
    // The feedback form (RB4h). Held here for the same reason the guide is: the quickbar's
    // Feedback action and the main menu's row are two doors onto one form.
    let feedback_open = use_signal(|| false);
    use_context_provider(|| crate::feedback::FeedbackOpen(feedback_open));
    // The first-run welcome: closed until the store says it has not been seen. Missing or
    // corrupt storage keeps it open — the direction that cannot strand a first-run user is
    // the one that shows (see `app_info::load_welcome_seen`).
    let mut welcome_open = use_signal(|| false);
    use_context_provider(|| crate::app_info::WelcomeOpen(welcome_open));
    use_future(move || async move {
        let seen = crate::app_info::load_welcome_seen().await.unwrap_or(false);
        // The update banner's Refresh asks for one look at What's new, which the welcome leads
        // with; a returning user would otherwise never see what the reload brought.
        let refreshed = crate::update_banner::take_whats_new_request().await;
        welcome_open.set(!seen || refreshed);
    });
    // The same restore gate the picker defaults use, so the fresh defaults never overwrite a
    // saved config in the gap before `load()` resolves.
    let mut stat_config_restored = use_signal(|| false);
    // Set once the mount future has finished with BOTH stores. The roster-change effect below
    // waits on it, because reconciling — and persisting — a default layout against a restored
    // roster would overwrite the saved layout before the future has read it.
    let mut arrangement_restored = use_signal(|| false);
    // The level-gated enhancement-slot budget (used vs available), lifted here for the same
    // reason as `totals`: one derived memo over the loaded schedule + live build, read by the
    // slot counter. Yields an empty budget until the dataset resolves.
    let slot_budget = use_memo(move || match &*db.read() {
        Some(Ok(database)) => {
            let build = session.build.read();
            let available = database
                .leveling_schedule
                .as_ref()
                .map(|schedule| schedule.total_slots_at_level(build.level))
                .unwrap_or(0);
            panels::powers::SlotBudget {
                used: coh_data::placed_budget_slots(&build),
                available,
            }
        }
        _ => panels::powers::SlotBudget::default(),
    });
    use_context_provider(|| panels::powers::BuildBudget(slot_budget));
    // How the Powers panel arranges its cards (by powerset, or over the pick levels). Held
    // here, not in the panel, so the choice survives the panel remounting on a dataset
    // switch or a free-grid drag — the same reason `desktop_layout` lives here.
    let powers_layout = use_signal(|| panels::powers::PowersLayout::ByPowerset);
    use_context_provider(|| panels::powers::PowersLayoutMode(powers_layout));
    // Whether picks and enhancements are gated to the level the character has reached. Lifted
    // here because the header readout, the available-power rows, and both pickers read it; it
    // stays OUT of the build, since it describes how the planner is being used and not the
    // character (see `level_up_store`).
    let level_up_mode = use_signal(|| false);
    use_context_provider(|| crate::level_control::LevelUpMode(level_up_mode));
    // Whether each slot's level is drawn under it — an Options preference, on by default.
    let show_slot_levels = use_signal(|| true);
    use_context_provider(|| panels::powers::ShowSlotLevels(show_slot_levels));
    let mut show_slot_levels_restored = use_signal(|| false);
    use_future(move || async move {
        let mut shown = show_slot_levels;
        if let Some(saved) = crate::slot_level_store::load().await {
            shown.set(saved);
        }
        show_slot_levels_restored.set(true);
    });
    use_effect(move || {
        let shown = *show_slot_levels.read();
        if !*show_slot_levels_restored.read() {
            return;
        }
        crate::slot_level_store::persist(shown);
    });
    // Every slot's level, solved once per build change for all the cards to read, while the
    // option to show them is on.
    let slot_levels = use_memo(move || {
        if !show_slot_levels() {
            return None;
        }
        match &*db.read() {
            Some(Ok(database)) => database.leveling_schedule.as_ref().map(|schedule| {
                std::rc::Rc::new(coh_data::slot_levels::slot_levels(
                    schedule,
                    &session.build.read(),
                ))
            }),
            _ => None,
        }
    });
    use_context_provider(|| panels::powers::SlotLevelsView(slot_levels));
    // The perma-tracked powers (beta `permaTrackedPowers`), in-memory exactly as there: the
    // tracked set is UI state, not part of the build, so it is not persisted.
    let perma_tracked = use_signal(Vec::new);
    use_context_provider(|| PermaTracked(perma_tracked));
    // Gates the persist effect below until the stored defaults have been restored, so the
    // fresh defaults never overwrite saved state in the gap before `load()` resolves.
    let mut picker_restored = use_signal(|| false);
    // The same gate for the Powers-panel arrangement.
    let mut powers_layout_restored = use_signal(|| false);
    // And for level-up mode.
    let mut level_up_restored = use_signal(|| false);
    // Boot bookkeeping: `booted` gates restore until the envelope has been read from
    // storage; `loaded_for` records which dataset the working build currently reflects, so
    // the restore effect re-selects exactly once per dataset switch (never clobbering edits).
    let mut booted = use_signal(|| false);
    let mut loaded_for = use_signal(|| Option::<DatasetId>::None);

    // Restore the persisted desktop layout once on mount; invalid/corrupt keeps the
    // default, and a layout saved in an older format is migrated by `load_desktop`.
    //
    // With nothing saved, the default is fitted to the window instead of taken as authored:
    // its column height is the one number that has to come from the screen (see
    // `GridItem::default_layout`), and an authored 20 rows is a ~980px page that a 13"
    // laptop cuts the bottom off. Deliberately NOT persisted — same reasoning as a content
    // fit — so a fresh install re-measures on every load and follows the window until the
    // first real gesture writes a layout of the user's own.
    //
    // `measure_grid_space` waits for the grid element itself rather than trusting this
    // future's position in the boot order: a round-trip through the JS context does NOT
    // imply the render's DOM mutations have been applied, and measuring here without the
    // wait finds no `#desktop-grid` at all and silently keeps the authored default.
    use_future(move || async move {
        // THE ROSTER FIRST, and the order is load-bearing rather than tidy. Every completion
        // below is measured against "which surfaces exist", and that stopped being a const
        // when a dashboard panel became something the user builds. These used to be two
        // independent futures, which is a race the layout side always lost: the grid waits on
        // a DOM measurement and the roster is a single `localStorage` read, so a layout
        // reconciled before the roster landed would be reconciled against the DEFAULT roster
        // — dropping every panel the user had made and appending one they had deleted.
        let mut stat_config = stat_config;
        if let Some(saved) = crate::stats_store::load().await {
            stat_config.set(saved);
        }
        stat_config_restored.set(true);
        let roster = panels::dashboards::surfaces(&stat_config.peek());

        // Measure before asking for a save, because the column count decides which save to ask
        // for: a layout is cells, and the same cells are a different arrangement at a
        // different count. With nothing measurable (the grid is `display: none` below 900px)
        // the widest count is the honest guess — it is what `GridConfig::default` answers and
        // what every pre-V4 save was written at.
        let space = layout_store::measure_grid_space().await;
        let config = match space {
            Some(space) => grid::model::GridConfig::for_width(space.width),
            None => grid::model::GridConfig::default(),
        };
        grid_columns.set(config.columns);

        if let Some(saved) = layout_store::load_mobile(&roster).await {
            mobile_order.set(saved);
        }

        if let Some(saved) = layout_store::load_desktop(config.columns, &roster).await {
            desktop_layout.set(saved);
        } else if let Some(space) = space {
            let rows = grid::model::column_rows_for(space.height, &config);
            layout_store::mark_layout_authored();
            desktop_layout.set(grid::model::GridItem::default_layout_for(
                config.columns,
                rows,
                &roster,
            ));
        }
        arrangement_restored.set(true);
    });

    // A panel the user just made has no rectangle yet, and one they just deleted still has one.
    //
    // The roster and the two arrangements are separate state with separate stores, and the load
    // path reconciles them — but only on load. Without this, adding a panel in the organizer
    // changes the roster and nothing else: the grid has no cell to draw it in, so the panel is
    // invisible until the next reload, which reads as the button not having worked. Deleting one
    // leaves the opposite — a rectangle for a surface `SurfaceBody` can no longer resolve.
    //
    // Gated on the mount future having finished, and that gate is the whole reason this is not
    // three lines. The future restores the roster BEFORE it measures and loads the layout, so
    // between those two moments the roster is the user's and the layout is still the authored
    // default; an ungated effect would reconcile that pair, persist the result, and overwrite
    // the saved layout a few milliseconds before the future asked for it.
    //
    // Compared as sets rather than run unconditionally: `reconcile_layout` compacts, and
    // committing a compaction the user has not asked for would re-close every gap they left.
    use_effect(move || {
        if !*arrangement_restored.read() {
            return;
        }
        let roster = panels::dashboards::surfaces(&stat_config.read());

        let layout = desktop_layout.peek().clone();
        let known: Vec<grid::PanelKind> = layout.iter().map(|item| item.panel).collect();
        if known.len() != roster.len() || !roster.iter().all(|panel| known.contains(panel)) {
            // What reconcile is about to append, noted before it does. A new panel lands below
            // the visible content — the one rule reconcile has, and the right one for a surface
            // whose cell nobody chose — which on a full grid is under the fold. Creating a panel
            // and seeing nothing happen is the same experience as the button not working, so the
            // grid is scrolled to it. Only when exactly one arrived: a batch is a reconcile
            // after some other change, and scrolling to an arbitrary member of it would be a
            // guess about which one was meant.
            let arrived: Vec<grid::PanelKind> = roster
                .iter()
                .copied()
                .filter(|panel| !known.contains(panel))
                .collect();
            let items = grid::collide::reconcile_layout(layout, &roster);
            layout_store::persist_desktop(&items, grid_columns.peek().to_owned());
            desktop_layout.set(items);
            if let [only] = arrived[..] {
                crate::panel_visibility::scroll_into_view(only);
            }
        }

        let stack = mobile_order.peek().clone();
        if !stack.validate(&roster) {
            let reconciled = stack.reconcile(&roster);
            layout_store::persist_mobile(&reconciled);
            mobile_order.set(reconciled);
        }
    });

    // Restore the pinned quickbar once on mount. A saved EMPTY row is a real answer here —
    // the user unpinned everything — so `load_quickbar` returns `Some(vec![])` for it and only
    // a missing or corrupt key falls through to the default set.
    use_future(move || async move {
        if let Some(saved) = layout_store::load_quickbar().await {
            quickbar_pins.set(saved);
        }
    });

    // Restore the persisted Powers-panel arrangement once on mount, then open its gate.
    use_future(move || async move {
        let mut powers_layout = powers_layout;
        if let Some(saved) = layout_store::load_powers_layout().await {
            powers_layout.set(saved);
        }
        powers_layout_restored.set(true);
    });

    // Persist the arrangement whenever the toggle changes, once restore has run.
    use_effect(move || {
        let layout = *powers_layout.read();
        if !*powers_layout_restored.read() {
            return;
        }
        layout_store::persist_powers_layout(layout);
    });

    // Layout sync with the signed-in account (`layout_sync`). Waits for every restore above, so
    // an account copy written into storage is never overwritten by a restore's own write. When
    // the account's copy wins it is loaded into the screen the way a boot loads it: roster first,
    // because every arrangement is reconciled against it.
    let sync_account = use_context::<crate::cloud::account::Account>();
    use_future({
        let account = sync_account;
        move || {
            let account = account.clone();
            async move {
                let mut state = crate::layout_sync::SyncState::default();
                loop {
                    crate::layout_sync::pause().await;
                    let restored = *arrangement_restored.peek()
                        && *stat_config_restored.peek()
                        && *powers_layout_restored.peek();
                    if !restored || !crate::layout_sync::tick(&mut state, &account).await {
                        continue;
                    }
                    let mut stat_config = stat_config;
                    if let Some(saved) = crate::stats_store::load().await {
                        stat_config.set(saved);
                    }
                    let roster = panels::dashboards::surfaces(&stat_config.peek());
                    let columns = *grid_columns.peek();
                    if let Some(saved) = layout_store::load_desktop(columns, &roster).await {
                        desktop_layout.set(saved);
                    }
                    if let Some(saved) = layout_store::load_mobile(&roster).await {
                        mobile_order.set(saved);
                    }
                    if let Some(saved) = layout_store::load_quickbar().await {
                        quickbar_pins.set(saved);
                    }
                    let mut powers_layout = powers_layout;
                    if let Some(saved) = layout_store::load_powers_layout().await {
                        powers_layout.set(saved);
                    }
                }
            }
        }
    });

    // Restore level-up mode once on mount, then open its persist gate. The mode is restored but
    // never re-clamps the level on load: the clamp belongs to the act of switching the mode on,
    // and re-running it every reload would walk a build's level down behind the user's back.
    use_future(move || async move {
        let mut level_up_mode = level_up_mode;
        if let Some(saved) = crate::level_up_store::load().await {
            level_up_mode.set(saved);
        }
        level_up_restored.set(true);
    });

    use_effect(move || {
        let enabled = *level_up_mode.read();
        if !*level_up_restored.read() {
            return;
        }
        crate::level_up_store::persist(enabled);
    });

    // Persist the config whenever it changes, once restore has run.
    use_effect(move || {
        let config = stat_config.read().clone();
        if !*stat_config_restored.read() {
            return;
        }
        crate::stats_store::persist(&config);
    });

    // Restore the Rule-of-5 alert preference once on mount, then open its persist gate. Same
    // restore-before-persist shape as the stat config: without the gate, the fresh `true` would
    // be written back over a stored `false` in the gap before `load()` resolves.
    let mut alerts_restored = use_signal(|| false);
    use_future(move || async move {
        let mut enabled = rule_of_five_enabled;
        if let Some(saved) = crate::alert_store::load().await {
            enabled.set(saved);
        }
        alerts_restored.set(true);
    });
    use_effect(move || {
        let enabled = *rule_of_five_enabled.read();
        if !*alerts_restored.read() {
            return;
        }
        crate::alert_store::persist(enabled);
    });

    // Restore the UI-scale preference once on mount, then open its persist gate. Same
    // restore-before-persist shape as the alert above. Applying the DOM zoom is NOT gated on
    // restore: calling it with the default before the saved value lands just repaints once
    // more when it does, the same accepted flash the alert's own default takes.
    let mut ui_scale_restored = use_signal(|| false);
    use_future(move || async move {
        let mut pct = ui_scale_pct;
        if let Some(saved) = crate::ui_scale::load().await {
            pct.set(saved);
        }
        ui_scale_restored.set(true);
    });
    use_effect(move || {
        let pct = *ui_scale_pct.read();
        crate::ui_scale::apply(pct);
        if !*ui_scale_restored.read() {
            return;
        }
        crate::ui_scale::persist(pct);
    });

    // Restore the damage reading once on mount, then open its persist gate — the same
    // restore-before-persist shape as the UI scale above.
    let mut damage_metric_restored = use_signal(|| false);
    use_future(move || async move {
        let mut metric = damage_metric;
        if let Some(saved) = crate::damage_metric_store::load().await {
            metric.set(saved);
        }
        damage_metric_restored.set(true);
    });
    use_effect(move || {
        let metric = *damage_metric.read();
        if !*damage_metric_restored.read() {
            return;
        }
        crate::damage_metric_store::persist(metric);
    });
    let mut proc_damage_restored = use_signal(|| false);
    use_future(move || async move {
        let mut include = proc_damage;
        if let Some(saved) = crate::damage_metric_store::load_proc_damage().await {
            include.set(saved);
        }
        proc_damage_restored.set(true);
    });
    use_effect(move || {
        let include = *proc_damage.read();
        if !*proc_damage_restored.read() {
            return;
        }
        crate::damage_metric_store::persist_proc_damage(include);
    });

    // Restore the persisted picker defaults once on mount, then open the persist gate.
    use_future(move || async move {
        let mut defaults = picker_defaults;
        let mut memory = picker_memory;
        if let Some((io_level, attuned, boost, places)) = picker_store::load().await {
            defaults.io_level.set(io_level);
            defaults.attuned.set(attuned);
            defaults.boost.set(boost);
            memory.set(places);
        }
        picker_restored.set(true);
    });

    // Persist the picker defaults whenever they change, once restore has run. Reading all
    // three signals subscribes the effect so any edit (Lv/Attuned/Boost) re-fires it.
    use_effect(move || {
        let io_level = *picker_defaults.io_level.read();
        let attuned = *picker_defaults.attuned.read();
        let boost = *picker_defaults.boost.read();
        let memory = picker_memory.read().clone();
        if !*picker_restored.read() {
            return;
        }
        picker_store::persist(io_level, attuned, boost, memory);
    });

    // Boot pre-peek (beta `bootServerId`): read the persisted envelope, then aim the
    // initial dataset at the last-active one so the right definitions load first. The full
    // build restore waits behind the `db` resource — see the effect below.
    use_future(move || async move {
        let mut stored = stored;
        let env = build_store::load()
            .await
            .unwrap_or_else(StoredBuilds::empty);
        let active = env.active_dataset;
        stored.set(env);
        booted.set(true);
        if active != *dataset.peek() {
            dataset.set(active);
        }
    });

    // Which fork the `db` resource currently holds, as a `Memo` so the restore effect below
    // subscribes to it reliably — a bare `Resource::read()` behind an early return leaves
    // the effect subscribed only to whatever it read first, so it never re-fires when the
    // dataset resolves (the boot-doesn't-restore-until-you-switch bug). The value is the
    // loaded fork's OWN name rather than a ready flag: on a dataset switch the resource
    // keeps serving the outgoing fork until the new bundle lands, so a boolean reads
    // stale-true across the whole reload and re-notifies nothing — and restoring against
    // the wrong fork's definitions resolves the build against nothing, which deletes every
    // dataset-scoped grant (its owning set does not exist there).
    let loaded_dataset = use_memo(move || match &*db.read() {
        Some(Ok(database)) => Some(database.manifest.dataset.clone()),
        _ => None,
    });

    // Load-then-rehydrate (the ported boot-order constraint): restore the working build ONLY
    // after the active dataset has loaded, because the calc resolves the build's definitions
    // from it. Every tracked signal is read up front (before any early return) so the effect
    // is subscribed to all of them. Re-runs on every dataset switch (the reload moves
    // `loaded_dataset` to the new fork's name), selecting that dataset's build from the
    // already-current envelope — persistence is eager, so no save-before-switch is needed.
    use_effect(move || {
        let loaded = loaded_dataset();
        let is_booted = booted();
        let target = dataset();
        // Subscribe to the envelope too: `load()` populates it asynchronously, and on the
        // web the bundle fetch means it often lands AFTER `booted`/`db` have already settled.
        // Reading (not peeking) it here re-fires the restore when the envelope arrives.
        let snapshot = stored.read().clone();
        if !is_booted || loaded.as_deref() != Some(target.as_str()) {
            return;
        }
        if *loaded_for.peek() == Some(target) {
            return;
        }
        let Some(Ok(database)) = &*db.peek() else {
            return;
        };
        let mut restored = envelope::select_active(&snapshot, target);
        // Reconcile the granted inherents and power-gated grants against the dataset just
        // loaded, BEFORE the build becomes the session's: a switch resolves the same
        // identity against a different fork's grants and gates, and a build saved before
        // either was materialized has none. Doing it here rather than as an edit keeps
        // loading out of the undo history.
        crate::inherents::sync(&mut restored, database);
        crate::granted_powers::sync(&mut restored, database);
        session.load(restored);
        loaded_for.set(Some(target));
    });

    // A `.skif` opened against a fork the app was not holding: the menu parked the text and
    // switched the dataset, and this is where it lands once that fork's bundle AND the restore
    // above have both finished. Waiting on `loaded_for` rather than on the bundle is the whole
    // point — the restore overwrites the working build, so a file decoded any earlier would be
    // replaced by the build it was opened to replace.
    //
    // It lives here rather than in `build_io` because that record is the shell's: an effect
    // that captured it as a component prop would freeze at the render that created it.
    use_effect(move || {
        let mut pending = build_io_pending;
        let parked = pending.read().clone();
        let restored = loaded_for();
        let Some(parked) = parked else {
            return;
        };
        if restored != Some(parked.dataset) {
            return;
        }
        let Some(Ok(database)) = &*db.peek() else {
            return;
        };
        crate::build_io::adopt(
            &parked.text,
            &parked.file_name,
            database,
            parked.dataset,
            session,
            build_io_report,
        );
        pending.set(None);
    });

    // Keyboard shortcuts: a document-level listener that skips edit fields, mirroring the
    // beta's `useUndoRedoKeyboard` and `useInfoPanelLockHotkey`. It posts back to Rust rather
    // than driving JS state.
    use_future(move || async move {
        let mut eval = document::eval(SHELL_KEYS);
        while let Ok(msg) = eval.recv::<String>().await {
            match msg.as_str() {
                "undo" => session.undo(),
                "redo" => session.redo(),
                "lock" => info_lock.toggle_current(selection),
                _ => {}
            }
        }
    });

    rsx! {
        crate::update_banner::UpdateBanner {}
        header { class: "shell-header",
            // Controls lead, brand trails (decision 2026-07-27, user-chosen). The cluster is
            // what gets used; the mark is what gets read once.
            //
            // File leads the cluster (HM1, amended by HM8). It is the only control at this end
            // that is not about the build on screen — it is what the app DOES to it as a file —
            // and a row ordered by what each control answers has to start with the one that
            // answers "what is this thing", not with one of the answers about the build.
            //
            // HM1 put the preferences and the about-pages in here too, behind one ☰. HM8 is why
            // they are not: nineteen rows in a `max-height: 70vh` panel, under a webview whose
            // scrollbars stay hidden until a scroll begins. `Options` and `Help` now sit at the
            // trailing edge beside Tools and Display, because those are the controls about the
            // workspace and the app rather than about this build. The mobile nav's Menu tab
            // lists all three on one sheet — see `mobile_nav`.
            crate::main_menu::FileMenu {
                database: match &*db.read() {
                    Some(Ok(database)) => Some(Db(database.clone())),
                    _ => None,
                },
                dataset,
            }
            //
            // Identity and Build Settings live here rather than on the grid: both are short
            // forms the user dips into and back out of, and a whole always-visible surface each
            // was two of the grid's ten cells spent on controls that aren't being read
            // while powers are being planned.
            match &*db.read() {
                Some(Ok(database)) => rsx! {
                    IdentityPopover { database: Db(database.clone()), dataset }
                },
                _ => rsx! {},
            }
            // The readout beside the switch: which game's data is loaded, always visible.
            // Outside the `db` match on purpose — the badge names the fork the shell has
            // SELECTED, which is true while the bundle is still loading and is exactly when
            // a reader is most likely to be wondering what they are about to get.
            DatasetBadge { dataset }
            CombatPopover {
                database: match &*db.read() {
                    Some(Ok(database)) => Some(Db(database.clone())),
                    _ => None,
                },
            }
            // The one most-flipped combat input sits on the row itself, so the state a user
            // carries from fight to fight — am I in combat — doesn't ask for the popover to be
            // reopened. Same commit path as the form, so an inline flip and a form edit can
            // never disagree. Promote the most-flipped, not every toggle: a row that promoted
            // every switch would be the old pile of controls again.
            //
            // Exemplar was the second, and it went back into Build Settings when HM8 split the
            // ☰ and this row gained two triggers. It loses nothing: the form carries a checkbox
            // AND the 1..=50 level slider, where the inline toggle could only flip to a fixed
            // 50, and `panels::combat`'s drift summary already puts a `•` on the Build Settings
            // trigger whenever exemplar is on — so the state stays visible in the row without
            // spending a control on it.
            BuildInlineToggles {}
            // Level earns a permanent seat rather than a popover of its own: it is the slot
            // counter's denominator and the archetype-table context every projected number
            // resolves against, so it is read far more often than it is set.
            crate::level_control::LevelControl {
                database: match &*db.read() {
                    Some(Ok(database)) => Some(Db(database.clone())),
                    _ => None,
                },
            }
            // Level-up mode reads the same level the control beside it sets, and its readout is
            // about that number, so the two are one cluster.
            crate::level_control::LevelUpControl {
                database: match &*db.read() {
                    Some(Ok(database)) => Some(Db(database.clone())),
                    _ => None,
                },
            }
            UndoRedo { session }
            crate::quickbar::ToolsMenu {
                pins: quickbar_pins,
                layout: desktop_layout,
                database: match &*db.read() {
                    Some(Ok(database)) => Some(Db(database.clone())),
                    _ => None,
                },
            }
            crate::quickbar::DisplayMenu {
                pins: quickbar_pins,
                layout: desktop_layout,
                reorder_open,
                reset_confirm_open,
            }
            // The app-level pair, after the workspace pair: Tools and Display answer "what can I
            // open" and "what is on screen"; these two answer "how is this app set up" and "what
            // is this app". Nothing here reads or writes the build.
            crate::main_menu::OptionsMenu {}
            crate::main_menu::HelpMenu {}

            div { class: "brand",
                img { class: "brand-mark", src: BRAND_ICON, alt: "" }
                h1 { "CoH Sidekick" }
                span { class: "brand-tag mono", "1.0 · rebuild" }
            }
        }

        // The reorder overlay, mounted here rather than in the header (HM5): its only affordance
        // is the Display menu's Reorder row, and an overlay that answers a menu belongs outside
        // the row, like every other overlay in the shell.
        reorder_menu::ReorderMenu { open: reorder_open, mobile_order, layout: desktop_layout }

        // The Reset to default confirm, beside the reorder overlay it replaced as the menu's
        // arrangement act: mounted here rather than in the header, like every other overlay
        // in the shell (a `fixed` backdrop would be contained by a grid surface's `transform`).
        crate::quickbar::ResetLayoutConfirm {
            open: reset_confirm_open,
            layout: desktop_layout,
            mobile_order,
            pins: quickbar_pins,
        }

        crate::quickbar::Quickbar {
            pins: quickbar_pins,
            layout: desktop_layout,
            database: match &*db.read() {
                Some(Ok(database)) => Some(Db(database.clone())),
                _ => None,
            },
        }

        // The perma-tracked strip, above both layout roots: the rebuild's dashboard is eight
        // independently placed surfaces, so there is no single dashboard surface to host it
        // in — the shell is the one place it is always in reach (beta `PinnedPowersBar`).
        PinnedPowersStrip {
            database: match &*db.read() {
                Some(Ok(database)) => Some(Db(database.clone())),
                _ => None,
            },
        }
        // The sticky level + budget bar, mobile only (hidden ≥900px in app.css): on the
        // scrolling stack the header's level control and the Powers panel's slot counter
        // scroll out of reach, and level is the budget's denominator.
        BuildBar {
            database: match &*db.read() {
                Some(Ok(database)) => Some(Db(database.clone())),
                _ => None,
            },
        }

        StatusStrip {}
        RuleOfFiveBanner {}
        ExemplarBanner {}

        // Desktop view: free 2D grid of absolutely-positioned surfaces (≥900px)
        main { class: "grid-root desktop-grid",
            match &*db.read() {
                Some(Ok(database)) => rsx! {
                    grid::view::GridContainer { database: Db(database.clone()), selection, state: desktop_layout }
                },
                Some(Err(e)) => rsx! {
                    div { class: "load-state error", "Dataset failed to load: {e}" }
                },
                None => rsx! {
                    div { class: "load-state", "Loading {dataset().as_str()}…" }
                },
            }
        }

        // Mobile view: flat stack with independent ordering (<900px)
        main { class: "grid-root mobile-stack",
            match &*db.read() {
                Some(Ok(database)) => {
                    let db = Db(database.clone());
                    // Both folded and hidden state are read from the desktop layout rather
                    // than kept in a second signal: each is a property of the surface, not of
                    // which arrangement is on screen, so folding or hiding a panel on one
                    // holds on the other and persists through the layout it already rides in.
                    // The stack has no cells, so folding hides the body while hiding drops
                    // the whole section out of the stack.
                    let roster = panels::dashboards::surfaces(&stat_config.read());
                    let hidden = crate::panel_visibility::hidden_panels(&desktop_layout.read(), &roster);
                    let stack: Vec<PanelKind> = mobile_order()
                        .0
                        .into_iter()
                        .filter(|panel| !hidden.contains(panel))
                        .collect();
                    rsx! {
                        if stack.is_empty() {
                            crate::panel_visibility::AllPanelsHidden {}
                        }
                        for panel in stack {
                            {
                                let collapsed = desktop_layout
                                    .read()
                                    .iter()
                                    .any(|item| item.panel == panel && item.collapsed);
                                let title = panels::dashboards::title_of(panel, &stat_config.read());
                                rsx! {
                                    section { class: if collapsed { "panel is-collapsed" } else { "panel" },
                                        header { class: "panel-head",
                                            span { class: "panel-title", "{title}" }
                                        }
                                        // Outside the header for the same reason as the grid's:
                                        // `.panel-head > *` is pointer-events:none.
                                        button {
                                            class: "panel-fold",
                                            r#type: "button",
                                            "aria-expanded": !collapsed,
                                            "aria-label": if collapsed { "Expand {title} panel" } else { "Collapse {title} panel" },
                                            onclick: move |_| {
                                                let mut items = desktop_layout.peek().clone();
                                                grid::collide::toggle_collapse(&mut items, panel);
                                                layout_store::persist_desktop(&items, grid_columns());
                                                desktop_layout.set(items);
                                            },
                                            {crate::view::marks::chevron(collapsed)}
                                        }
                                        button {
                                            class: "panel-hide",
                                            r#type: "button",
                                            "aria-label": "Hide {title} panel",
                                            onclick: move |_| {
                                                crate::panel_visibility::set_panel_hidden(desktop_layout, grid_columns(), panel, true);
                                            },
                                            {crate::view::marks::close()}
                                        }
                                        if !collapsed {
                                            MobilePanel { panel, database: db.clone(), selection }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
                Some(Err(e)) => rsx! {
                    div { class: "load-state error", "Dataset failed to load: {e}" }
                },
                None => rsx! {
                    div { class: "load-state", "Loading {dataset().as_str()}…" }
                },
            }
        }

        // The fixed bottom nav, mobile only (hidden ≥900px in app.css): the beta's
        // `MobileBottomNav` tabs, re-rendering the row-1 popovers with their panels opening
        // upward. Mounted outside both layout roots, like the other fixed chrome.
        MobileNav {
            database: match &*db.read() {
                Some(Ok(database)) => Some(Db(database.clone())),
                _ => None,
            },
            dataset,
            pins: quickbar_pins,
            layout: desktop_layout,
            reorder_open,
            reset_confirm_open,
        }

        // The one Compare Slotting modal, hosted OUTSIDE both layout roots for the same
        // containment reason — and BEFORE the enhancement picker host, so the picker a
        // comparison row opens paints above this modal.
        if let Some(Ok(database)) = &*db.read() {
            crate::compare_slotting::CompareSlottingHost { database: Db(database.clone()) }
        }

        // The one enhancement picker, hosted OUTSIDE both layout roots so its fixed backdrop
        // is not contained by a free-grid surface's `transform` (see `powers::PickerTarget`).
        if let Some(Ok(database)) = &*db.read() {
            panels::powers::EnhancementPickerHost { database: Db(database.clone()) }
        }

        // The one slot hover-tooltip, hosted OUTSIDE both layout roots so its fixed card isn't
        // contained/clipped by a free-grid surface's `transform` (see `powers::SlotTooltipHost`).
        if let Some(Ok(database)) = &*db.read() {
            panels::powers::SlotTooltipHost { database: Db(database.clone()) }
        }

        // The one ⋮ menu on a picked power, hosted OUTSIDE both layout roots for the same reason.
        if let Some(Ok(database)) = &*db.read() {
            panels::powers::PowerMenuHost { database: Db(database.clone()) }
        }

        // The one pool/epic picker, hosted OUTSIDE both layout roots for the same reason.
        if let Some(Ok(database)) = &*db.read() {
            panels::pool_picker::PoolPickerHost { database: Db(database.clone()) }
        }

        // The one incarnate picker, hosted OUTSIDE both layout roots for the same reason.
        if let Some(Ok(database)) = &*db.read() {
            panels::incarnate_picker::IncarnatePickerHost { database: Db(database.clone()) }
        }
        if let Some(Ok(database)) = &*db.read() {
            panels::incarnate_crafting::IncarnateCraftingHost { database: Db(database.clone()) }
        }

        // The one accolade picker, hosted OUTSIDE both layout roots for the same reason.
        if let Some(Ok(database)) = &*db.read() {
            panels::accolade_picker::AccoladePickerHost { database: Db(database.clone()) }
        }

        // The one Attack-Chain builder modal, hosted OUTSIDE both layout roots for the same
        // containment reason.
        if let Some(Ok(database)) = &*db.read() {
            panels::attack_chain::AttackChainHost { database: Db(database.clone()) }
        }

        // The one proc-settings modal, hosted here for the same containment reason. It takes the
        // database because the categories it offers are derived from that dataset's proc data.
        if let Some(Ok(database)) = &*db.read() {
            panels::proc_settings::ProcSettingsHost { database: Db(database.clone()) }
        }

        // The what-if team-buff modal, hosted here for the same containment reason. It needs no
        // database: its vocabulary comes from the engine's accumulator and the stat registry.
        panels::what_if::WhatIfHost {}

        // Closes any popped-out panel window when this one closes, so quitting with a panel out
        // quits the app instead of leaving an orphan behind (see `crate::panel_popout`). Draws
        // nothing — it is here because this is the main window's dom, and a wry event handler
        // only ever receives events for the window it was registered from.
        crate::panel_popout::PoppedWindows {
            layout: desktop_layout,
            columns: grid_columns,
            restored: arrangement_restored,
        }

        // The one dashboard-stats config modal, hosted here for the same containment reason —
        // and unlike the three above it needs no database, since it configures display only.
        panels::stats_config::StatsConfigHost {}
        panels::detailed_totals::DetailedTotalsHost {
            database: match &*db.read() {
                Some(Ok(database)) => Some(Db(database.clone())),
                _ => None,
            },
        }
        panels::set_bonus_finder::SetBonusFinderHost {
            database: match &*db.read() {
                Some(Ok(database)) => Some(Db(database.clone())),
                _ => None,
            },
        }

        // The one panel-visibility modal, hosted here for the same containment reason. It
        // edits the layout itself, so it takes the signal rather than a database.
        crate::panel_visibility::PanelVisibilityHost { layout: desktop_layout }

        // The forum-export modal, hosted here for the same reason. It takes the database because
        // a build stores ids and the post is made of names.
        crate::forum_export::ForumExportHost {
            database: match &*db.read() {
                Some(Ok(database)) => Some(Db(database.clone())),
                _ => None,
            },
        }

        // The image-export modal, hosted here for the same reason, and taking the database for
        // the same one: the poster draws power names, and a build stores ids.
        crate::export_image::ExportImageHost {
            database: match &*db.read() {
                Some(Ok(database)) => Some(Db(database.clone())),
                _ => None,
            },
        }

        // The enhancement list, hosted here for the same reason. It takes the database because
        // a set's catalog row is what says whether its pieces cost a catalyst.
        crate::enhancement_list::EnhancementListHost {
            database: match &*db.read() {
                Some(Ok(database)) => Some(Db(database.clone())),
                _ => None,
            },
        }

        // The enhancement tools, hosted here for the same reason. It takes the database because
        // a set's own level range is what bounds a build-wide re-level, and the fork's curves
        // are what say whether relative level means anything here at all.
        crate::enhancement_tools::EnhancementToolsHost {
            database: match &*db.read() {
                Some(Ok(database)) => Some(Db(database.clone())),
                _ => None,
            },
        }

        // The powerset comparison, hosted here for the same reason. It takes the database
        // because both sides are powersets the build does not hold.
        crate::powerset_compare::PowersetCompareHost {
            database: match &*db.read() {
                Some(Ok(database)) => Some(Db(database.clone())),
                _ => None,
            },
        }

        // The shared-builds browser (RB4d), hosted here for the same containment reason and
        // taking the database for the same one the paste surface does: a shared build's
        // `build_json` is a document the import readers decode, and the decode needs the fork
        // on screen.
        crate::cloud::browser::BrowserHost {
            database: match &*db.read() {
                Some(Ok(database)) => Some(Db(database.clone())),
                _ => None,
            },
            dataset,
        }

        // The save-to-cloud surface (RB4e), hosted here for the same containment reason. It
        // takes the database because the row's search columns are display NAMES and a build
        // stores ids — the same reason the forum export takes one.
        crate::cloud::save_build::SaveBuildHost {
            database: match &*db.read() {
                Some(Ok(database)) => Some(Db(database.clone())),
                _ => None,
            },
        }

        // The public-profile editor (RB4f), hosted here for the same containment reason. It
        // takes no database: a profile is an account's, not a build's.
        crate::cloud::profile::ProfileHost {}

        // The self-description modals, hosted here for the same containment reason. They need
        // no database: they describe the app, not the build on screen.
        crate::app_info::AboutHost {}
        crate::app_info::ChangelogHost {}
        // The controls reference, hosted here for the same containment reason. It needs no
        // database: it names the app's own handlers, not the build's data.
        crate::controls::ControlsHost {}
        // The guide, beside the sheet it searches. Needs no database for the same reason: it
        // describes the app's surfaces, not the build on screen.
        crate::help::HelpHost {}
        crate::app_info::DonateHost {}
        crate::app_info::WelcomeHost {}
        // The feedback form (RB4h), hosted here for the same containment reason. It takes the
        // database because the report attaches the build, and encoding one needs the definitions
        // — the same reason the save surface above takes one.
        crate::feedback::FeedbackHost {
            database: match &*db.read() {
                Some(Ok(database)) => Some(Db(database.clone())),
                _ => None,
            },
        }

        // The floating footer cluster. Fixed-position like the hosts above, so it mounts
        // outside both layout roots for the same containment reason; its z-index keeps it
        // under any open popover or modal.

        // What a cloud act that could not complete has to say for itself, hosted here for the
        // same containment reason. Its own host rather than a branch of the one below: a build
        // file and a session are not the same subject, and the receipt below reports successes.
        crate::cloud::account::AccountReportHost {}

        // What the last open or save has to say for itself, hosted here for the same reason.
        // It takes the fork on screen because a build that named none was read against it, and
        // saying so is half of what the receipt is for.
        crate::build_io::BuildIoReportHost { dataset }

        // A build arriving on the URL, read once at boot.
        crate::build_io::BuildIoFragmentHost { dataset }

        // The clipboard's door onto the same two readers, and the desktop's only one for a
        // share link.
        crate::build_io::PasteBuildHost {
            database: match &*db.read() {
                Some(Ok(database)) => Some(Db(database.clone())),
                _ => None,
            },
            dataset,
        }

        // And the question a cross-fork file has to ask before either open can happen.
        crate::build_io::BuildIoChoiceHost {
            database: match &*db.read() {
                Some(Ok(database)) => Some(Db(database.clone())),
                _ => None,
            },
            dataset,
        }
    }
}

/// Fail-loud channel for the whole build: any contribution the interpreter couldn't derive is
/// listed here rather than vanishing into a soft-wrong total (Rule 1). Renders nothing when the
/// build is clean.
///
/// It lives in the shell — above both layout roots, outside every panel — because the stats
/// dashboard is five independently placed panels: an error routed into one of them is only as
/// visible as wherever that panel happens to sit, and a `CalcError` carries a `context` string,
/// not a stat group, so there is no honest panel to route it to anyway.
#[component]
fn StatusStrip() -> Element {
    let totals = use_context::<panels::stats::BuildTotals>().0;
    let errors = totals.read().bonuses.errors.clone();
    if errors.is_empty() {
        return rsx! {};
    }

    rsx! {
        div { class: "status-strip", role: "status",
            for err in errors {
                div { class: "load-state error", "{err.context}: {err.detail}" }
            }
        }
    }
}

/// UI scale, in percent ([`crate::ui_scale::MIN_PCT`]..=[`crate::ui_scale::MAX_PCT`]). Lives at
/// the shell root for the same reason [`RuleOfFiveAlert`] does: the control that writes it (the
/// main menu's options group) is not the only thing that would ever need to read it.
#[derive(Clone, Copy, PartialEq)]
pub struct UiScale {
    pub pct: Signal<u32>,
}

/// The setting behind [`RuleOfFiveBanner`], and its per-load dismissal. Both live at the shell
/// root: the banner sits above the layout roots, and the main menu writes the setting.
#[derive(Clone, Copy)]
pub struct RuleOfFiveAlert {
    /// Persisted, default on ([`crate::alert_store`]) — the permanent "stop telling me".
    pub enabled: Signal<bool>,
    /// Cleared on reload by design: dismissing is "not now", and a build edited into wasting a
    /// different bonus tomorrow should say so again. Turning the setting off is the way to mean
    /// it permanently, and the banner says which control does which.
    pub dismissed: Signal<bool>,
}

/// The build-wide Rule-of-5 alert (the beta `RuleOf5Banner`): how many slotted bonuses the cap
/// refused, above both layout roots.
///
/// The per-row cues came first — a warning ring on the stat, a `⚠` in the Set Bonuses panel, a
/// struck-through row in the detailed sheet — and each one is only as visible as the surface
/// holding it, which on a free grid is wherever the user parked it. The count is the one fact
/// that has to reach someone who is looking at the Powers panel, so it goes where the fail-loud
/// strip goes, for the same reason.
#[component]
fn RuleOfFiveBanner() -> Element {
    let totals = use_context::<panels::stats::BuildTotals>().0;
    let alert = use_context::<RuleOfFiveAlert>();
    let mut dismissed = alert.dismissed;

    let refused = totals.read().rule_of_five_rejections();
    if refused == 0 || !(alert.enabled)() || dismissed() {
        return rsx! {};
    }
    let subject = if refused == 1 {
        "bonus is"
    } else {
        "bonuses are"
    };

    rsx! {
        div { class: "alert-strip", role: "status", "aria-live": "polite",
            span { class: "alert-strip__label", "Rule of 5" }
            span { class: "alert-strip__body",
                "{refused} slotted {subject} past the fifth identical copy and will not give a bonus. "
                "The Set Bonuses panel indicates which one, and crossed-out in the detailed totals."
            }
            button {
                class: "alert-strip__dismiss",
                r#type: "button",
                "aria-label": "Dismiss until reload, turn the alert off in Options to stop it \
                               permanently",
                onclick: move |_| dismissed.set(true),
                "✕"
            }
        }
    }
}

/// The passive reminder that the build is being previewed under exemplar (the beta
/// `ExemplarModeBanner`). The mode silently rescales every enhancement magnitude and, below
/// the incarnate floor, drops incarnate contributions too, so a toggle left on from a session
/// ago moves every number on the dashboard without saying so — and the two controls that set
/// it (the header toggle, the Build Settings form) are both out of the way when powers are
/// being planned. Deliberately quieter than the alert strip above it: a reminder that a state
/// is on, not a fault that the build pays for. No dismissal — the only answer the banner has
/// is the one that ends the state, and the level it shows is the build's own, so it can never
/// drift from what the panels are computing.
#[component]
fn ExemplarBanner() -> Element {
    let session = use_context::<BuildSession>();
    let level = session.build.read().combat.exemplar_level;
    let Some(level) = level else {
        return rsx! {};
    };

    rsx! {
        div { class: "exemplar-strip", role: "status", "aria-live": "polite",
            span { class: "exemplar-strip__label", "Exemplar" }
            span { class: "exemplar-strip__body",
                "preview is on — stats at level {level.get()}"
            }
            button {
                class: "exemplar-strip__action",
                r#type: "button",
                onclick: move |_| session.commit(move |s| s.combat.exemplar_level = None),
                "Turn off"
            }
        }
    }
}

/// The build's identity, in the header (the beta `BuildIdentityPopover`). The trigger is a
/// live summary of the build — its name, its archetype and both powersets — so the spine is
/// legible without opening anything; the panel holds the same form the Identity surface did,
/// and the dataset switcher that used to sit with the reader's preferences.
///
/// `pub(crate)` because the mobile bottom nav ([`crate::mobile_nav`]) re-renders this same
/// popover as one of its tabs — a second seat for the same control, not a copy.
#[component]
pub(crate) fn IdentityPopover(database: Db, dataset: Signal<DatasetId>) -> Element {
    let session = use_context::<BuildSession>();
    let label = identity_label(&session.build.read(), &database);

    rsx! {
        crate::popover::Popover {
            label,
            title: "Build Identity".to_string(),
            modifier: "popover--identity".to_string(),
            panels::identity::IdentityPanel { database, dataset }
        }
    }
}

/// Which game's data is loaded, always visible (the beta `DatasetBadge`).
///
/// **Read-only.** The switch is one control to the left, inside the Identity popover, because
/// the fork is a property of the build rather than of the reader — `identity.rs` states it
/// as "a build carried to another fork is a different build". This is the readout of that
/// choice, and a badge that was also a control would be a second writer for it.
///
/// It earns a permanent seat on the row for the reason [`CombatPopover`]'s drift chip does:
/// the fork silently decides every number on screen. Until this landed, the only place that
/// said which fork was loaded was inside a popover, so a Thunderspy number read as
/// Homecoming's had nothing in the chrome to correct it.
///
/// The colour modifier comes off [`DatasetId::as_str`] rather than a `match` here — a new
/// fork gets its class without this file being edited, which is the lesson
/// `DatasetId::from_wire` records about hand tables beside a roster. The one fact the badge
/// says beyond the name comes from [`DatasetId::is_open_beta`], in the data layer for the
/// same reason.
#[component]
fn DatasetBadge(dataset: Signal<DatasetId>) -> Element {
    let id = dataset();

    rsx! {
        span {
            class: "dataset-badge dataset-badge--{id.as_str()}",
            title: dataset_note(id),
            "{id.display_name()}"
        }
    }
}

/// The badge's hover sentence. Split out so the test can read it without a renderer.
///
/// Both arms name the switch's home, because a readout whose control is elsewhere has to say
/// where — a disabled-looking pill with no route is the shape [`crate::main_menu`]'s unbuilt
/// rows exist to avoid.
fn dataset_note(id: DatasetId) -> &'static str {
    if id.is_open_beta() {
        "Loaded dataset — Homecoming's open beta, so these numbers are what live is about to \
         become rather than what live is. Switch in Build Identity."
    } else {
        "Loaded dataset — the game every number on screen is computed from. Switch in Build \
         Identity."
    }
}

/// Who this build is, in one line, degrading a step at a time rather than showing a half-empty
/// template: nothing → "New build", name only → the name, archetype only → the archetype.
///
/// The build's own `name` leads when it has one — it is the one identity field the user
/// authored rather than the app resolving, so it is the first thing the chip says. Every
/// other name comes from [`crate::naming`], which resolves it from the id rather than reading
/// the selection's stored `name` — see that module for why a stored name is blank on every
/// build that arrived from a file.
fn identity_label(build: &CharacterState, database: &PowerDatabase) -> String {
    let mut parts = Vec::new();
    if !build.name.is_empty() {
        parts.push(build.name.clone());
    }
    if let Some(archetype) = crate::naming::archetype_name(build, database) {
        parts.push(archetype);
    }
    let sets = [&build.primary, &build.secondary]
        .into_iter()
        .filter_map(|selection| crate::naming::powerset_name(selection, database))
        .collect::<Vec<_>>()
        .join(" / ");
    if !sets.is_empty() {
        parts.push(sets);
    }
    if parts.is_empty() {
        return "New build".to_string();
    }
    parts.join(" · ")
}

/// Build Settings — the live combat-context controls, in the header. The trigger carries an
/// amber drift chip while any input is off its baseline, because these controls silently move
/// every number on the dashboard — a non-default build state that isn't visible from the
/// outside is a trap. The chip is [`drift_summary`](panels::combat::drift_summary)
/// rendered verbatim, so "what is off" is derived, never hand-maintained.
///
/// The database arrives optional rather than gating the whole popover on it: the combat-state
/// inputs need no dataset, and hiding them while a bundle loads would take the header's
/// controls away for the one moment the app looks unresponsive. The derived caster-state
/// sections simply aren't there yet.
///
/// `pub(crate)` for the same reason as [`IdentityPopover`]: the mobile bottom nav re-renders
/// this popover as one of its tabs.
#[component]
pub(crate) fn CombatPopover(database: Option<Db>) -> Element {
    let session = use_context::<BuildSession>();
    let drift = panels::combat::drift_summary(&session.build.read().combat);
    let label = if drift.is_empty() {
        "Build Settings".to_string()
    } else {
        let shown = &drift[..drift.len().min(2)];
        let extra = drift.len() - shown.len();
        match extra {
            0 => shown.join(" · "),
            _ => format!("{} · +{extra} more", shown.join(" · ")),
        }
    };

    rsx! {
        crate::popover::Popover {
            label: label.to_string(),
            title: "Build Settings".to_string(),
            modifier: if drift.is_empty() { "" } else { "popover--modified" },
            panels::combat::Combat { database }
        }
    }
}

/// The combat input pulled onto the row because it flips far more often than the rest set once
/// and forget — in-combat. A row-mate for the popover beside it (build-state, moves totals),
/// sharing its commit path so an inline flip and a form edit can never disagree.
///
/// Exemplar was here too until HM8; see the call site for why it went back to the form.
#[component]
fn BuildInlineToggles() -> Element {
    let session = use_context::<BuildSession>();
    let combat = session.build.read().combat.clone();

    rsx! {
        nav { class: "build-inline-toggles",
            button {
                class: "seg build-toggle",
                "aria-pressed": combat.in_combat,
                title: "In combat, controls suppressible effects",
                onclick: move |_| session.commit(move |s| s.combat.in_combat = !s.combat.in_combat),
                "Combat"
            }
        }
    }
}

/// Undo/redo pair — the two steppers that walk the edit history. Disabled at the ends of the
/// history, which is the honest state; a button that could not act but did not say so would
/// read as a failed history rather than as one that is exhausted.
#[component]
pub(crate) fn UndoRedo(session: BuildSession) -> Element {
    rsx! {
        nav { class: "undo-redo",
            button {
                class: "seg",
                title: "Undo (Ctrl+Z)",
                disabled: !session.can_undo(),
                onclick: move |_| session.undo(),
                "↶"
            }
            button {
                class: "seg",
                title: "Redo (Ctrl+Y)",
                disabled: !session.can_redo(),
                onclick: move |_| session.redo(),
                "↷"
            }
        }
    }
}

#[component]
fn MobilePanel(panel: PanelKind, database: Db, selection: Signal<Selection>) -> Element {
    // Exhaustive, and no catch-all — see `grid::view::SurfaceBody`, whose twin this is.
    match panel {
        PanelKind::Powers => rsx! { panels::powers::PowersPanel { database } },
        PanelKind::Available => rsx! { panels::powers::AvailablePanel { database } },
        PanelKind::Pools => rsx! { panels::powers::PoolsPanel { database } },
        PanelKind::SetBonuses => rsx! { panels::set_bonus_totals::SetBonusTotals { database } },
        PanelKind::Info => rsx! { panels::info::Info { database, selection } },
        PanelKind::Dashboard(id) => rsx! { panels::stats::StatGroup { panel: id } },
    }
}
