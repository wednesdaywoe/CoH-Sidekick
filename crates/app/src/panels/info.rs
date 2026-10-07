//! Info — the selected power, rendered from the engine's own per-power projection
//! ([`coh_math::projection`]) through the SAME [`PowerView`] every other surface uses (the
//! one-source-of-truth proof).
//!
//! The panel computes nothing. Every value it shows is a resolved three-tier number the engine
//! already produced for this build's slotting and globals; the panel picks the order and the
//! headings. What it cannot show, it says: a damage component whose gate this build has not
//! answered is named as unresolved rather than left out, because a damage power showing fewer
//! numbers reads as a power that deals less (Rule 1).

use crate::build_session::BuildSession;
use crate::panels::adjusters::{AdjusterBlock, PowerAdjusters};
use crate::panels::slotted_procs::{SlottedProcBlock, SlottedProcRow};
use crate::panels::stats::BuildTotals;
use crate::pinned_powers::PermaTrackButton;
use crate::shell::Db;
use crate::shell::Selection;
use crate::view::power_view::{
    resolve_power_def, resolve_selected_view, DamageMetric, DamageView, EffectRow, PowerView,
};
use coh_math::perma::PermaInfo;
use dioxus::prelude::*;

/// Which face of a power's card is showing.
///
/// Some split is needed at all because the single-column card measured 1228px into a 791px
/// panel: a third of every power sat below the fold, and the authored description was
/// unreachable at y≈1083.
///
/// Two faces rather than Mids' three. Info is the answer — what this power hits for, what it
/// does to whoever it lands on, what it costs to fire — and it is the face that is open by
/// default, so it earns the room. Details is everything a reader goes looking for on purpose:
/// the full effect list with its Base/Slotted/Total columns, and what the slotting in this
/// power is worth. Enhance was a face of its own and did not deserve one — it is two short
/// lists, and a tab whose whole job is to hide two blocks costs a click to save nothing. The
/// slotted procs sit on Info, beside the stats that set how often they fire.
#[derive(Clone, Copy, PartialEq, Eq)]
enum InfoTab {
    Info,
    Details,
}

/// One power rendered from its [`PowerView`] — identity, tags, execution stats, the effects it
/// grants, its perma tracking and its slotted bonuses, across three tabs ([`InfoTab`]).
///
/// A component rather than markup inlined in [`Info`] because two surfaces show it: this
/// panel, and the pool picker's preview pane
/// ([`crate::panels::pool_picker`]). Sharing the component is what keeps the promise
/// `PowerView` exists to make — a power reads the same wherever it is shown.
///
/// `adjusters` and `slotted_procs` are what the two surfaces differ on, and both are props
/// rather than derivations so the difference is stated: the picker is deciding whether to TAKE
/// a power, and a control there would write build state for a power the build does not own.
#[component]
pub fn PowerViewCard(
    view: PowerView,
    adjusters: PowerAdjusters,
    #[props(default)] slotted_procs: Vec<SlottedProcRow>,
    /// The Track toggle in the perma tracker. The Info panel offers it (a tracked power is a
    /// build-level interest, and the panel is where the player studies one power); the pool
    /// picker's preview does not — it is deciding whether to TAKE a power, and a pin there
    /// would track a power the build does not own.
    #[props(default)]
    perma_track: bool,
    /// The target ranks to offer where damage waits on one. The Info panel passes them, since
    /// choosing a rank is a setting the player owns; the previews leave it empty and point to
    /// the Combat panel instead.
    #[props(default)]
    rank_options: Vec<crate::panels::combat::TargetOption>,
) -> Element {
    let mut tab = use_signal(|| InfoTab::Info);
    // What the slotted damage procs add to one cast on average, before the reader's switch:
    // `None` when nothing slotted here deals damage per cast, so the switch is not offered.
    let proc_per_cast = slotted_procs
        .iter()
        .any(|row| {
            row.proc
                .damage
                .as_ref()
                .is_some_and(|d| d.per_cast.is_some())
        })
        .then(|| {
            let pieces: Vec<_> = slotted_procs.iter().map(|row| row.proc.clone()).collect();
            coh_math::procs::proc_damage_per_cast(&pieces)
        });

    let has_details = view.damage.is_some()
        || !view.sections.is_empty()
        || !view.enhancement_bonuses.is_empty()
        || !view.allowed_enhancements.is_empty();

    // The chosen tab survives a change of power — someone comparing two powers' debuffs should
    // not be dropped back to Info on every click. What it must never do is show a blank pane,
    // so a tab the CURRENT power has nothing for falls back for that power alone and is still
    // the chosen one when a power that fills it comes back. Derived rather than written back
    // into the signal, which is what keeps the fallback from eating the choice.
    let active = match tab() {
        InfoTab::Details if !has_details => InfoTab::Info,
        chosen => chosen,
    };

    rsx! {
        div { class: "info-power",
            // Identity rides above the tabs, on every face: a number read without knowing which
            // power it belongs to is worse than a number not shown, and the tab strip is the one
            // control here that changes what the card says without changing what it is about.
            // The export's own ident rides in the title rather than on a line of its own. It is
            // a key for the data work — the thing `exported_powers/` and the contract are filed
            // under — and worth one hover to anyone checking a number against the source, but it
            // is not something a player reads, and it was spending a line of the card's best
            // space saying so.
            // The name leads, with the power's own art beside it. The card is the one surface
            // that answers "what IS this power", and it was the only one naming a power without
            // showing it — so a reader arriving from a row they recognized by its picture had
            // to re-identify it by its words.
            //
            // The art and the name share a line rather than the art taking one of its own: this
            // card is read in a panel the free grid lets you shrink, and every line the head
            // spends is one the numbers under it don't get.
            div { class: "info-power__head",
                img {
                    class: "info-power__icon",
                    src: crate::view::icons::power_icon_url(view.icon.as_deref()),
                    alt: "",
                }
                h3 { title: view.internal_name.clone().unwrap_or_default(), "{view.name}" }
            }
            if !view.tags.is_empty() {
                div { class: "info-tags",
                    for tag in &view.tags {
                        span { class: "info-tag", "{tag}" }
                    }
                }
            }
            // The chips ride on the Damage heading, where they answer the question the number
            // beside them raises — 46.99 of WHAT. They only get there if there is a damage block
            // to put them on; a power the converter typed but the engine resolved no damage for
            // keeps them here rather than losing them, and a malformed field is a visible marker
            // either way (Rule 1).
            match &view.damage_types {
                Ok(damage_types) if !damage_types.is_empty() && view.damage.is_none() => rsx! {
                    div { class: "damage-chips",
                        for damage_type in damage_types {
                            span { class: "damage-chip", "data-damage": "{damage_type}", "{damage_type}" }
                        }
                    }
                },
                Ok(_) => rsx! {},
                Err(e) => rsx! {
                    div { class: "load-state error", "{e}" }
                },
            }

            if let Some(fault) = &view.fault {
                div { class: "load-state error", "{fault}" }
            }

            // Above the rows they move, so a number is never read before the state it was read
            // under — the beta's order for the same reason. That is also why they stay OUTSIDE
            // the tabs: an adjuster resolves the numbers on every face, and one parked inside
            // Info would silently be the state an Effects row was read under.
            if !adjusters.is_empty() {
                AdjusterBlock { adjusters: adjusters.clone() }
            }

            // A face with nothing on it is offered disabled rather than dropped: the strip
            // keeps one shape across powers, and a greyed tab answers "does this power debuff
            // anything?" without a click (Rule 1's habit — say the absence, don't hide it).
            nav { class: "info-tabs", role: "tablist",
                button {
                    class: if active == InfoTab::Info { "info-tab is-active" } else { "info-tab" },
                    role: "tab",
                    "aria-selected": active == InfoTab::Info,
                    title: "What this power costs and deals",
                    onclick: move |_| tab.set(InfoTab::Info),
                    "Info"
                }
                button {
                    class: if active == InfoTab::Details { "info-tab is-active" } else { "info-tab" },
                    role: "tab",
                    "aria-selected": active == InfoTab::Details,
                    disabled: !has_details,
                    title: if has_details {
                        "Every effect tier by tier, and what this power's slotting is worth"
                    } else {
                        "This power grants nothing, and holds nothing"
                    },
                    onclick: move |_| tab.set(InfoTab::Details),
                    "Details"
                }
            }

            div { class: "info-pane", role: "tabpanel",
                match active {
                    // The answer, in the order the questions get asked: what does it hit for,
                    // what does it do to them, what does it cost me. Damage leads because it is
                    // the number the power is picked for; the effects under it are the ones the
                    // power's OWN authored summary names, so the lead is the designers' answer
                    // rather than this panel's guess at one.
                    //
                    // Perma sits at the foot of this face. The authored description sits below
                    // the whole pane rather than under it, because it is true on Details too.
                    InfoTab::Info => rsx! {
                        if let Some(damage) = &view.damage {
                            DamageBlock {
                                damage: damage.clone(),
                                rank_options: rank_options.clone(),
                                proc_per_cast,
                            }
                        }
                        if !view.headline_rows.is_empty() {
                            EffectLines { title: "Effects", rows: view.headline_rows.clone() }
                        }
                        for group in &view.execution_groups {
                            StatTiles { title: group.title, rows: group.rows.clone() }
                        }
                        // The slotted procs, with what each does and how often it fires here —
                        // the beta's Proc Chance row and its proc controls, as one block. On
                        // this face because "how often does my proc go off in this power" is
                        // asked of the power, not of its slotting ledger.
                        if !slotted_procs.is_empty() {
                            SlottedProcBlock { rows: slotted_procs.clone() }
                        }
                        if let Some(perma) = view.perma {
                            PermaTracker {
                                perma,
                                track: perma_track,
                                internal_name: view.internal_name.clone(),
                            }
                        }
                    },
                    // Everything the first face summarised, tier by tier, plus what is in the
                    // slots. The effect rows come back as the Base/Slotted/Total grid here —
                    // that grid is why this face exists, since the compact lines on Info state
                    // the number a build HAS and this is where "what did my slotting buy" is
                    // answered. The section ORDER is the view's, not this panel's — mez first
                    // because it decides whether the power lands at all.
                    InfoTab::Details => rsx! {
                        if let Some(damage) = &view.damage {
                            DamageTiers { damage: damage.clone() }
                        }
                        for section in &view.sections {
                            EffectTable { title: section.title, rows: section.rows.clone() }
                        }
                        if !view.enhancement_bonuses.is_empty() {
                            div { class: "info-block",
                                h4 { "Enhancement bonuses (after ED)" }
                                dl { class: "info-pairs",
                                    for (aspect , value) in &view.enhancement_bonuses {
                                        dt { "{aspect}" }
                                        dd { class: "is-buffed", "{value}" }
                                    }
                                }
                            }
                        }
                        if !view.allowed_enhancements.is_empty() {
                            div { class: "info-block",
                                h4 { "Accepts" }
                                div { class: "info-tags",
                                    for allowed in &view.allowed_enhancements {
                                        span { class: "info-tag", "{allowed}" }
                                    }
                                }
                            }
                        }
                    },
                }
            }

            // The power in its own words, at the foot — the beta's order, and for the beta's
            // reason: prose is the least dense thing on the card and the least often read
            // twice, so it yields the space above the fold to the numbers someone opened the
            // panel for. It sits OUTSIDE the pane rather than under Info's last block, because
            // it is the one piece of the card that is true on every face.
            //
            // The authored description, and not `shortHelp` beside it. `shortHelp`
            // is the designers' shorthand — `Ranged, DMG(Negative), Foe -To Hit` — and every
            // clause in it is said again, better, further up the card: `Ranged` is already a
            // tag, the damage type is a chip on the Damage heading, and the debuff is a row
            // under Effects. It still does the work it is good at, one layer down, where
            // [`PowerView`] reads it to decide WHICH effects lead. A power the export gave no
            // description keeps it as the fallback rather than showing nothing.
            match (view.description.is_empty(), &view.short_help) {
                (false, _) => rsx! {
                    div { class: "info-description",
                        for (index , paragraph) in view.description.iter().enumerate() {
                            p { key: "{index}", "{paragraph}" }
                        }
                    }
                },
                (true, Some(help)) => rsx! {
                    p { class: "short-help", "{help}" }
                },
                (true, None) => rsx! {},
            }
        }
    }
}

/// The damage block — what this power deals against the build's chosen target.
///
/// Four groups, never merged. The certain rows and their total are what every landed hit
/// deals. The conditional rows (a critical, an inert component) sit under their own heading
/// with the reason on each, because a hit-time roll averaged into the number above would be a
/// figure no single hit ever deals. The situational rows are components whose condition only the
/// fight can answer (the foe is held, it carries a debuff), stated as what they add when it
/// holds. The unresolved lines are what is left: components whose amount is unknowable too
/// (Rule 1: shown, not dropped).
///
/// A row that belongs to a named mechanic says so, in the game's own words for it — the
/// annotation reads `CritLarge · ScrapperCrit_ST · 10% chance` rather than `10% chance`. The
/// panel never looks the name up; it renders what `power_view::mechanic_names` took verbatim
/// from the export.
///
/// Slotted damage procs ride beside the hit, never inside it: their damage is flat, so a damage
/// buff or the archetype cap that moved the headline does not move them, and folding them in
/// would make the delta credit the build for damage it did not scale. They get their own line,
/// their own colour on the bar, and a switch (the beta's "Procs") that also governs the attack
/// chain.
#[component]
fn DamageBlock(
    damage: DamageView,
    rank_options: Vec<crate::panels::combat::TargetOption>,
    /// Average slotted proc damage per cast, or `None` when the power holds no damage proc.
    #[props(default)]
    proc_per_cast: Option<f64>,
) -> Element {
    let session = use_context::<crate::build_session::BuildSession>();
    let target_class = session.build.read().combat.target_class.clone();
    // The per-component rows only earn their space where there is more than one component to
    // tell apart — with a single one the headline number IS that row, and printing it again
    // under itself reads as a second hit rather than the same one named. `total` is `Some`
    // exactly when there were two or more, which is the same question, already answered.
    let itemised = damage.total.is_some();
    // The reading is a shell-wide preference so it holds across powers, like the beta's. A
    // surface mounted outside the shell keeps a local one rather than failing.
    let local_metric = use_signal(DamageMetric::default);
    let mut metric = try_use_context::<crate::damage_metric_store::DamageMetricPref>()
        .map_or(local_metric, |pref| pref.0);
    let hero = damage.hero(metric());
    let local_procs = use_signal(|| true);
    let mut include_procs = try_use_context::<crate::damage_metric_store::ProcDamagePref>()
        .map_or(local_procs, |pref| pref.0);
    // Per cast, in damage units — what the bar draws. Zero when switched off or nothing fires.
    let procs_shown = proc_per_cast.filter(|_| include_procs()).unwrap_or(0.0);
    let proc_reading = damage
        .proc_reading(metric(), procs_shown)
        .filter(|v| *v > 0.0);
    rsx! {
        div { class: "info-block",
            div { class: "info-block__head",
                h4 {
                    "Damage"
                    if damage.capped {
                        span { class: "info-flag", "at the archetype cap" }
                    }
                }
                // 46.99 of WHAT. They sit on the same line as the word that raises the
                // question, and they are read off the components the 46.99 was read off — one
                // source, so the label can never describe a different build state than the
                // number beside it (CHIPTYPE-1).
                if !damage.types.is_empty() {
                    div { class: "damage-chips",
                        for damage_type in damage.types.iter() {
                            span { class: "damage-chip", "data-damage": "{damage_type}", "{damage_type}" }
                        }
                    }
                }
                if proc_per_cast.is_some() {
                    button {
                        r#type: "button",
                        class: "damage-procs",
                        "aria-pressed": include_procs(),
                        title: "Include the slotted damage procs' average damage, here and in the attack chain",
                        onclick: move |_| {
                            let on = include_procs();
                            include_procs.set(!on);
                        },
                        "Procs"
                    }
                }
                // The beta's DMG / DPA / DPS / DPE switch. It changes the hero line only: the
                // bar and the component rows stay in damage per hit, which is the one unit every
                // power shares and the bar's common scale is measured in.
                div {
                    class: "damage-metric",
                    role: "radiogroup",
                    "aria-label": "Damage reading",
                    for choice in DamageMetric::ALL {
                        button {
                            key: "{choice.label()}",
                            r#type: "button",
                            role: "radio",
                            class: if metric() == choice { "damage-metric__choice is-active" } else { "damage-metric__choice" },
                            "aria-checked": metric() == choice,
                            title: choice.title(),
                            onclick: move |_| metric.set(choice),
                            "{choice.label()}"
                        }
                    }
                }
            }
            // The hit this build lands, the hit the game ships, and the gap. One line, because
            // they are one number read three ways — the base is an anchor for the headline, not
            // a second figure competing with it, and the weights say so.
            match hero {
                Ok(hero) => rsx! {
                    div { class: "damage-hero",
                        // The card's biggest scan target takes the hue the dashboard gives the same
                        // number, so a damage figure looks like a damage figure in both places.
                        span { class: "damage-hero__value", "{hero.headline}" }
                        span { class: "damage-hero__base",
                            "base "
                            span { class: "damage-hero__base-value", "{hero.base_text}" }
                        }
                        if let Some(delta) = &hero.delta {
                            span { class: "damage-hero__delta", "{delta}" }
                        }
                    }
                },
                Err(why) => rsx! {
                    div { class: "damage-hero",
                        span { class: "damage-hero__value", "—" }
                        span { class: "damage-hero__base", "{why}" }
                    }
                },
            }
            if let Some(reading) = proc_reading {
                div {
                    class: "damage-proc",
                    title: "Average damage the slotted procs add per activation: each proc's hit times its chance. Flat, so damage buffs and the damage cap do not touch it.",
                    "+{reading:.1} proc"
                }
            }
            if damage.reference > 0.0 {
                DamageBar { damage: damage.clone(), procs: procs_shown }
            }
            if itemised {
                h5 { "Components" }
                div { class: "info-lines",
                    for row in damage.certain.iter() {
                        InfoLine { key: "{row.key}", row: row.clone() }
                    }
                }
            }
            if !damage.conditional.is_empty() {
                h5 { "Only on a roll" }
                div { class: "info-lines",
                    for row in damage.conditional.iter() {
                        InfoLine { key: "{row.key}", row: row.clone() }
                    }
                }
            }
            if !damage.situational.is_empty() {
                h5 { "Only in some situations" }
                div { class: "info-lines",
                    for row in damage.situational.iter() {
                        InfoLine { key: "{row.key}", row: row.clone() }
                    }
                }
            }
            // Components that turn on the enemy's rank (the Scrapper critical, chiefly) wait on a
            // choice the planner can make, so the choice is offered here rather than the gate
            // printed. It writes the same target the Combat panel's Target row does.
            if damage.waiting_on_rank > 0 {
                RankPrompt {
                    count: damage.waiting_on_rank,
                    options: rank_options.clone(),
                    chosen: target_class,
                }
            }
            for (index , note) in damage.unresolved.iter().enumerate() {
                p { key: "{index}", class: "info-gap", "{note}" }
            }
        }
    }
}

/// The rank chooser for damage that depends on who is being hit. Reads and writes
/// [`coh_data::CombatContext::target_class`], so it and the Combat panel's Target row are one
/// setting shown in two places.
#[component]
fn RankPrompt(
    count: usize,
    options: Vec<crate::panels::combat::TargetOption>,
    chosen: Option<String>,
) -> Element {
    let session = use_context::<crate::build_session::BuildSession>();
    let what = if count == 1 {
        "One damage roll depends"
    } else {
        "Some damage rolls depend"
    };
    if options.is_empty() {
        return rsx! {
            div { class: "info-rank",
                "{what} on the enemy's rank. Set the Target in the Combat panel to see it."
            }
        };
    }
    rsx! {
        div { class: "info-rank",
            span { "{what} on the enemy's rank. Choose one to see it:" }
            select {
                class: "select-compact",
                "aria-label": "Target rank",
                value: chosen.unwrap_or_default(),
                onchange: move |evt: Event<FormData>| {
                    let chosen = evt.value();
                    session.commit(move |s| {
                        s.combat.target_class = (!chosen.is_empty()).then(|| chosen.clone());
                    });
                },
                option { value: "", "Choose a rank" }
                for option in options {
                    option { value: "{option.class}", "{option.label}" }
                }
            }
        }
    }
}

/// Effect rows at their most compact: one line each, stating the number this build HAS.
///
/// The three-tier grid this replaces on the Info face asks the reader to walk four columns to
/// reach one answer, and on the leading face the answer is the whole point — the columns are
/// still there, one tab over, for the reader who came to ask what their slotting bought.
///
/// The base is printed beside the value only where the build MOVED it. A row nothing has touched
/// showing "−9.38% base −9.38%" spends a line saying the same number twice; a row that did move
/// needs the anchor, and its presence is itself the signal that something here is enhanced.
#[component]
fn EffectLines(title: String, rows: Vec<EffectRow>) -> Element {
    rsx! {
        div { class: "info-block",
            h4 { "{title}" }
            div { class: "info-lines",
                for row in rows.iter() {
                    InfoLine { key: "{row.key}", row: row.clone() }
                }
            }
        }
    }
}

/// One compact line. Its own component rather than markup inside [`EffectLines`] because the
/// damage block needs the same line under its own SUB-headings — components, and the rows that
/// only land on a roll — and reaching for `EffectLines` there would nest a titled block inside a
/// titled block, drawing that block's top rule across the middle of the damage it belongs to.
#[component]
fn InfoLine(row: EffectRow) -> Element {
    rsx! {
        div { class: "info-line",
            span { class: "info-line__label", "{row.label}" }
            // Qualifiers ride with the LABEL, not at the far edge. `(6s)` and `10% chance` say
            // which number this is rather than being numbers of their own, and parking them
            // against the right margin put them in the column the eye scans for values while
            // opening a void in the middle of every row.
            span { class: "info-line__note",
                if let Some(mechanic) = &row.mechanic {
                    span { class: "info-row__mechanic", "{mechanic}" }
                }
                if let Some(magnitude) = &row.magnitude {
                    span { "{magnitude}" }
                }
                if let Some(duration) = &row.duration {
                    span { "{duration}" }
                }
            }
            // The hairline that carries the eye across. It takes every pixel of slack, so the
            // number stays flush right at any panel width and the run between is never blank.
            span { class: "leader" }
            if row.enhanced_changed || row.final_changed {
                span { class: "info-line__base", "base {row.base}" }
            }
            // Hue says what KIND of number this is, never whether it is good news — the same
            // rule the dashboard reads by, so a defense row looks the same in both places. It
            // rides on the value rather than the label because the value is what the eye goes
            // to first, and the label beside it is already the plainer of the two.
            span { class: "info-line__value", style: hue_style(row.token),
                {row.enhanced_final.clone().unwrap_or_else(|| row.base.clone())}
            }
        }
    }
}

/// What the power costs to fire and where it reaches — a value over the word for it.
///
/// Tiles rather than rows because these are the card's independent facts: nothing here is read
/// against its neighbour the way two damage components are, so the grid can wrap to whatever
/// width the panel has instead of holding a column shape it has to defend.
///
/// Each tile states the number this build HAS, with the shipped value beneath it only where
/// slotting or a global moved it — the same grammar as the damage headline, and the same reason:
/// a tile reading "4s / base 4s" spends its second line agreeing with its first.
#[component]
fn StatTiles(title: String, rows: Vec<EffectRow>) -> Element {
    rsx! {
        div { class: "info-block",
            h4 { "{title}" }
            div { class: "stat-tiles",
                for row in rows.iter() {
                    div { key: "{row.key}", class: "stat-tile",
                        // The same `--stat-hue` channel the dashboard's rows carry, and the same
                        // rule: hue says what KIND of number this is so the eye can find the one
                        // it wants, never whether the number is good news. A family with no hue
                        // of its own sets nothing and the value falls back to plain ink, which
                        // is what keeps the coloured tiles worth finding.
                        span { class: "stat-tile__value", style: hue_style(row.token),
                            {row.enhanced_final.clone().unwrap_or_else(|| row.base.clone())}
                        }
                        span { class: "stat-tile__label", "{row.label}" }
                        if row.enhanced_changed || row.final_changed {
                            span { class: "stat-tile__base", "base {row.base}" }
                        }
                    }
                }
            }
        }
    }
}

/// The hit at a glance — three nested fills on one shared scale.
///
/// The three tiers are drawn as overlapping fills rather than a stacked bar, because they are
/// three readings of ONE hit and not three hits to add up: base sits in front of slotted, which
/// sits in front of the build's total, so each is read as how far along the same track that
/// tier reaches. Where slotting moved nothing the fills coincide and the bar reads as one
/// block, which is the honest picture of a power nothing has been spent on.
///
/// The scale is [`DamageView::reference`] — the hardest hit this build's chosen powersets can
/// produce, shared across every power — so two powers' bars can be compared directly. That is
/// the whole point of the bar: the numbers beside it already say what one power does, and only a
/// common scale says which of two hits harder without the reader doing arithmetic.
///
/// It matters that the scale is the SETS and not the build's hardest pick. A reference taken over
/// picked powers is always attained by one of them, so every build had a power pegged at full and
/// a reader learned nothing from seeing it there. Measured against the sets, a full bar is a
/// thing a build can fail to be — which is the only way a full bar carries information.
///
/// Slotted damage procs take their own segment past the final fill, in their own colour, as in
/// the beta. They are flat damage that no buff and no cap reaches, so they sit after the edge the
/// cap tick marks rather than inside it. A Build Up proc is different: its +damage is a buff, and
/// it is already inside the final fill. The scale widens to fit the procs where the hit and its
/// procs together out-reach the sets' hardest hit, so the segment is never cut off.
#[component]
fn DamageBar(damage: DamageView, #[props(default)] procs: f64) -> Element {
    let reference = damage.reference.max(damage.tiers.r#final + procs);
    let width = |value: f64| (value / reference * 100.0).clamp(0.0, 100.0);
    let base = width(damage.tiers.base);
    let enhanced = width(damage.tiers.enhanced);
    let total = width(damage.tiers.r#final);
    let proc_width = width(procs).min(100.0 - total);
    // What a full bar MEANS depends on what the track is, so the title says which. Reaching the
    // ceiling is an achievement about the build; reaching the end of a track that is only this
    // power itself (no ceiling could be stated) is not a claim about anything at all.
    let scale_note = if damage.tiers.r#final >= damage.reference {
        "full bar — nothing in these powersets hits harder, even slotted out"
    } else {
        "scaled to the hardest hit these powersets can produce"
    };
    rsx! {
        div {
            class: "damage-bar",
            role: "img",
            title: "{scale_note}",
            "aria-label": "Damage relative to the hardest hit these powersets can produce",
            div { class: "damage-bar__fill damage-bar__fill--total", style: "width: {total}%;" }
            div {
                class: "damage-bar__fill damage-bar__fill--enhanced",
                style: "width: {enhanced}%;",
            }
            div { class: "damage-bar__fill damage-bar__fill--base", style: "width: {base}%;" }
            if proc_width > 0.0 {
                div {
                    class: "damage-bar__procs",
                    style: "left: {total}%; width: {proc_width}%;",
                }
            }
            // The cap is a fact about the number, not about the length: a capped hit and an
            // uncapped one of the same size draw the same bar, so without this the reader has
            // no way to tell that more damage buff would do nothing.
            if damage.capped {
                div { class: "damage-bar__cap", style: "left: {total}%;" }
            }
        }
    }
}

/// The damage components tier by tier — what the game ships, what this power's slotting did to
/// it, and what the build's globals made of that.
///
/// The Info face states the hit as one headline number because that is the question a power is
/// opened for; this is the same hit taken apart, and it is the only place the MIDDLE tier of a
/// component survives. The headline's delta compares the two ends, and the bar draws all three
/// in aggregate — neither can say which of two components your slotting reached.
#[component]
fn DamageTiers(damage: DamageView) -> Element {
    rsx! {
        div { class: "info-block",
            h4 { "Damage" }
            if !damage.certain.is_empty() || damage.total.is_some() {
                div { class: "info-rows",
                    div { class: "info-rows__head",
                        span { "" }
                        span { "Base" }
                        span { "Slotted" }
                        span { "Total" }
                    }
                    for row in damage.certain.iter().chain(damage.total.iter()) {
                        DamageRow { row: row.clone() }
                    }
                }
            }
            if !damage.conditional.is_empty() {
                h5 { "Only on a roll" }
                div { class: "info-rows",
                    for row in damage.conditional.iter() {
                        DamageRow { row: row.clone() }
                    }
                }
            }
            if !damage.situational.is_empty() {
                h5 { "Only in some situations" }
                div { class: "info-rows",
                    for row in damage.situational.iter() {
                        DamageRow { row: row.clone() }
                    }
                }
            }
        }
    }
}

/// The `--stat-hue` declaration for a row's stat family, or none at all.
///
/// Neutral is the families' own "this number has no hue" — recharge, cast time, arc — and it
/// resolves to a DIMMER ink than body text. Setting it would push most of a card's numbers a
/// rung down to make a minority stand out, which is the opposite trade: the point of the channel
/// is that a coloured number is findable, and that only holds while the uncoloured ones are
/// still fully legible. So a neutral family declares nothing and the CSS fallback keeps plain
/// ink.
fn hue_style(token: &'static str) -> String {
    match token == crate::panels::stat_registry::StatFamily::Neutral.token() {
        true => String::new(),
        false => format!("--stat-hue: {token};"),
    }
}

/// One damage row. Shares the three-column shape of [`EffectTable`]'s rows but not its markup:
/// a damage row's annotation carries the tick arithmetic and the roll, which is more than a
/// mez magnitude, and every damage tier is always present (an em dash would read as "slotting
/// cannot move this").
#[component]
fn DamageRow(row: EffectRow) -> Element {
    rsx! {
        div { class: "info-row",
            span { class: "info-row__label", style: "color: {row.token};",
                "{row.label}"
                if let Some(mechanic) = &row.mechanic {
                    span { class: "info-row__mechanic", "{mechanic}" }
                }
                if let Some(annotation) = &row.magnitude {
                    span { class: "info-row__note", "{annotation}" }
                }
                if let Some(duration) = &row.duration {
                    span { class: "info-row__duration", "{duration}" }
                }
            }
            span { class: "info-row__base", "{row.base}" }
            span {
                class: if row.enhanced_changed { "info-row__tier is-buffed" } else { "info-row__tier" },
                {row.enhanced.clone().unwrap_or_default()}
            }
            span {
                class: if row.final_changed { "info-row__total is-global" } else { "info-row__total" },
                {row.enhanced_final.clone().unwrap_or_default()}
            }
        }
    }
}

/// One titled block of effect rows, three-tier: base · with this power's slotting · with the
/// build's globals folded in.
///
/// The three numeric columns are three CLAIMS — what the game ships, what this power's slotting
/// did, what the build gets — and a row no enhancement aspect reaches has only one claim, true
/// in all three. So it renders as one figure spanning them, carried over by a leader, rather
/// than as a base and two em dashes. Two cells saying "not here" is the same defect the
/// execution block had: the pair used to be a second `facts` list beside this one, two thirds
/// of whose cells were that dash, and PR4 answered it there by making those rows tiles
/// ([`StatTiles`]) grouped by meaning.
///
/// What does NOT follow from that is splitting a section into an enhanceable half and a fixed
/// half. That is the enhancement system wearing a heading again — the thing PR4 took OUT of the
/// execution block — and inside Debuffs it would file a −Regen away from the −Recovery a reader
/// compares it against. The rows stay in one list in the view's order; only the shape of the
/// untouched row's own cells changes.
///
/// The Details face's form, and only its form.
#[component]
fn EffectTable(title: String, rows: Vec<EffectRow>) -> Element {
    rsx! {
        div { class: "info-block",
            h4 { "{title}" }
            if !rows.is_empty() {
            div { class: "info-rows",
                div { class: "info-rows__head",
                    span { "" }
                    span { "Base" }
                    span { "Slotted" }
                    span { "Total" }
                }
                for row in rows.iter() {
                    div { key: "{row.key}", class: "info-row",
                        span { class: "info-row__label", style: "color: {row.token};",
                            "{row.label}"
                            if let Some(magnitude) = &row.magnitude {
                                span { class: "info-row__mag", "{magnitude}" }
                            }
                            if let Some(duration) = &row.duration {
                                span { class: "info-row__duration", "{duration}" }
                            }
                        }
                        // `enhanced` and `enhanced_final` are set together by `effect_row` —
                        // one `enhanceable.then()` each off the same flag — so the row is
                        // wholly tiered or wholly fixed, and this reads the pair off the
                        // first. A future shape where only one resolves would need its own
                        // arm rather than falling into the fixed one, which is why the match
                        // is on the tuple.
                        match (&row.enhanced, &row.enhanced_final) {
                            (Some(value), Some(total)) => rsx! {
                                span { class: "info-row__base", "{row.base}" }
                                span { class: if row.enhanced_changed { "info-row__tier is-buffed" } else { "info-row__tier" }, "{value}" }
                                span { class: if row.final_changed { "info-row__total is-global" } else { "info-row__total" }, "{total}" }
                            },
                            // The leader spans the columns this row has no separate answer
                            // for, and the figure lands in Total — so the column a reader
                            // scans for "what do I have" is answered on every row in the
                            // block, which is what two em dashes cost it. `title` carries the
                            // reason for anyone who wonders why the run is empty; the game's
                            // own words for it are "Ignores Buffs and Enhancements".
                            _ => rsx! {
                                span { class: "info-row__fixed", title: "Slotting cannot move this",
                                    span { class: "leader" }
                                    span { class: "info-row__total", "{row.base}" }
                                }
                            },
                        }
                    }
                }
            }
            }
        }
    }
}

/// Perma tracking for a click power whose effect outlasts — or doesn't — its own recharge.
/// Every value is [`coh_math::perma`]'s; the bar is the engine's `perma_percent`.
///
/// `track` offers the pin to the pinned-powers strip (beta `InfoPanel`'s Track button); the
/// `internal_name` it keys on is `None` only for a view that could not be resolved, which
/// has no perma row to render in the first place.
#[component]
fn PermaTracker(
    perma: PermaInfo,
    #[props(default)] track: bool,
    #[props(default)] internal_name: Option<String>,
) -> Element {
    let recharge_in_hand = (perma.total_recharge * 100.0).round();
    let recharge_needed = (perma.recharge_needed * 100.0).round();
    let gap = match perma.is_perma {
        true => "None".to_string(),
        false => format!("{:.1}s", perma.effective_recharge - perma.duration),
    };
    // The recast row states what an early re-fire buys: nothing extra (the running copy
    // just restarts its clock) or a second stacked copy for the overlap. Absent when the
    // atoms don't state a single answer — no row rather than a guess.
    let recast = perma.recast.map(|behavior| match behavior {
        coh_math::perma::RecastBehavior::Refreshes => "Refreshes (recasting early never overlaps)",
        coh_math::perma::RecastBehavior::Stacks => "Stacks (recasting early overlaps copies)",
    });
    rsx! {
        div { class: "info-block",
            div { class: "perma-track-head",
                h4 { "Perma tracker" }
                if track {
                    if let Some(internal_name) = internal_name {
                        PermaTrackButton { internal_name }
                    }
                }
            }
            div { class: "perma-bar",
                div {
                    class: "perma-bar__fill",
                    style: "width: {perma.perma_percent}%;",
                }
            }
            div { class: "perma-state",
                if perma.is_perma {
                    span { class: "is-perma", "Permanent" }
                } else {
                    span { "{perma.perma_percent:.1}% of the way" }
                }
            }
            dl { class: "info-pairs",
                dt { "+Recharge" }
                dd { "{recharge_in_hand:.0}% of {recharge_needed:.0}%" }
                dt { "Effective recharge" }
                dd { "{perma.effective_recharge:.1}s" }
                dt { "Base / duration" }
                dd { "{perma.base_recharge:.1}s / {perma.duration:.1}s" }
                dt { "Gap" }
                dd { class: if perma.is_perma { "is-buffed" } else { "is-short" }, "{gap}" }
                if let Some(recast) = recast {
                    dt { "Recast" }
                    dd { "{recast}" }
                }
            }
        }
    }
}

#[component]
pub fn Info(database: Db, selection: Signal<Selection>) -> Element {
    let session = use_context::<BuildSession>();
    let totals = use_context::<BuildTotals>().0;
    // What the panel shows: the locked power while there is one, and the hovered one otherwise
    // (beta `infoPanel.locked ? lockedContent : content`). Everything below reads `shown`, never
    // `selection` directly, so a lock holds the whole panel — view, adjusters and procs alike.
    let lock = use_context::<crate::shell::InfoLock>().0;
    let shown: Memo<Selection> = use_memo(move || lock().or(selection()));
    // Derived once per (database, build, totals, selection) change — resolving a view runs the
    // engine's projection, and for a power the build does not hold that means a whole
    // `recalculate_projecting` pass, so it belongs in a memo rather than in render.
    let selected_view: Memo<Option<Result<PowerView, String>>> =
        use_memo(use_reactive!(|database| {
            let build = session.build.read();
            let totals = totals.read();
            resolve_selected_view(&database, &build, &totals, &shown())
        }));

    // What the selected power lets this build adjust. Read off the power AS AUTHORED, not off
    // the effective one: a conditional's contribution is measured against the base effects it
    // merges into, and the effective power has already merged the active ones in. The two are
    // the same list either way — no `modeVariants`, `quickSnipe` or `formVariants` record on any
    // fork carries `conditionalEffects` of its own, which `adjuster_corpus` pins.
    let adjusters: Memo<PowerAdjusters> = use_memo(use_reactive!(|database| {
        let Some(selected) = shown() else {
            return PowerAdjusters::default();
        };
        let build = session.build.read();
        let Some(power) = resolve_power_def(&database, &selected.powerset_id, &selected.power)
        else {
            return PowerAdjusters::default();
        };
        let internal_name = power.ident().to_string();
        // The buff-pet opt-in folds into the totals, and the fold walks the build's own
        // selections — so it is offered only where it can move something. Matched on the whole
        // address, not the name: the key this control writes names a set, so a same-named pick
        // in a DIFFERENT set would offer a switch whose key the fold never reads.
        let held = build.all_selected().any(|held| {
            held.powerset == selected.powerset_id && held.internal_name == internal_name
        });
        let buff_pet_toggle = held.then(|| {
            let key =
                coh_math::buff_pets::buff_pet_toggle_key(&selected.powerset_id, &internal_name);
            let active = build.combat.power_state.get(&key).copied().unwrap_or(false);
            (key, active)
        });
        // Only a held pick has somewhere to keep the count. Matched on the whole address for
        // the same reason as the buff-pet switch above.
        let targets_hit = coh_math::stacking::stacking_slider(power).and_then(|slider| {
            let pick = build.all_selected().find(|held| {
                held.powerset == selected.powerset_id && held.internal_name == internal_name
            })?;
            Some(crate::panels::adjusters::TargetsHit {
                powerset: pick.powerset.clone(),
                internal_name: pick.internal_name.clone(),
                slider,
                value: pick.targets_hit,
            })
        });
        PowerAdjusters {
            targets_hit,
            conditionals: coh_math::adjusters::power_adjusters(
                power,
                &build,
                coh_data::caster_class_name(&build, &database),
            ),
            buff_pets: match buff_pet_toggle.is_some() {
                true => coh_math::buff_pets::buff_pet_sources(power, &database),
                false => Vec::new(),
            },
            buff_pet_toggle,
        }
    }));

    // The proc pieces slotted in the selected power, paired with what the LAST recalculate
    // measured each one contributing — so a row reports the arithmetic that ran rather than a
    // second description of it.
    let slotted_procs: Memo<Vec<SlottedProcRow>> = use_memo(use_reactive!(|database| {
        let Some(selected) = shown() else {
            return Vec::new();
        };
        let build = session.build.read();
        let Some(power) = resolve_power_def(&database, &selected.powerset_id, &selected.power)
        else {
            return Vec::new();
        };
        let pieces =
            coh_math::procs::slotted_procs(&build, &database, &selected.powerset_id, power.ident());
        crate::panels::slotted_procs::rows(pieces, &totals.read().proc_breakdown)
    }));

    rsx! {
        div { class: "info",
            // The lock control, top right (beta `InfoPanel`'s padlock). Locked, it is a button that
            // unlocks. Unlocked, it is a quiet mark that says how to lock, because right-click is
            // not something anyone discovers on their own.
            if lock().is_some() {
                button {
                    class: "info-lock info-lock--on",
                    "aria-label": "Unlock info panel",
                    title: "Locked — this panel won’t change as you hover. Click (or press L) to unlock and go back to following the pointer.",
                    onclick: move |_| {
                        let mut lock = lock;
                        lock.set(None);
                    },
                    LockMark { closed: true }
                    span { "Locked" }
                }
            } else if shown().is_some() {
                span {
                    class: "info-lock",
                    "aria-label": "Info panel follows the pointer",
                    title: "Following the pointer — the panel shows whatever power you hover. Right-click a power, or press L, to lock it here.",
                    LockMark { closed: false }
                }
            }
            match selected_view() {
                Some(Ok(view)) => rsx! {
                    PowerViewCard {
                        view,
                        adjusters: adjusters(),
                        slotted_procs: slotted_procs(),
                        perma_track: true,
                        rank_options: crate::panels::combat::target_options(&database),
                    }
                },
                Some(Err(power)) => rsx! {
                    div { class: "info-power",
                        p { class: "faint", "“{power}” isn’t in the {database.manifest.dataset} dataset." }
                    }
                },
                // Nothing pointed at. The panel names what fills it rather than sitting blank,
                // since hovering a card is not a discoverable act on its own.
                None => rsx! {
                    div { class: "empty-state",
                        "Point at a power to read it here."
                        div { class: "hint", "Its stats, effects and slotting all resolve against this build." }
                    }
                },
            }
        }
    }
}

/// The padlock the lock control draws: shut while locked, open while following the pointer.
/// Drawn rather than an emoji, so it takes the control's colour like every other mark.
#[component]
fn LockMark(closed: bool) -> Element {
    rsx! {
        svg {
            class: "info-lock__mark",
            view_box: "0 0 16 16",
            width: "12",
            height: "12",
            "aria-hidden": "true",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "1.6",
            stroke_linecap: "round",
            rect { x: "3", y: "7", width: "10", height: "7", rx: "1.5" }
            if closed {
                path { d: "M5.5 7V5a2.5 2.5 0 0 1 5 0v2" }
            } else {
                path { d: "M5.5 7V5a2.5 2.5 0 0 1 4.9-.7" }
            }
        }
    }
}
