//! Pure calculation over the atom dataset. No UI, no IO — the crate the whole rebuild
//! hangs off, graded corpus-wide against the TS oracle.
//!
//! [`recalculate`] is the totals entry point (the beta `calculateCharacterTotals`), built
//! pass by pass across M3: gather (Pass 0) → strength (Pass 1) → apply → inherents → caps.
//! It returns the [`GlobalBonuses`] accumulator; the totals gate grades it field-for-field
//! against the TS dump on the fields each landed pass owns.

pub mod adaptive_recharge;
pub mod adjusters;
pub mod appliers;
pub mod apply;
pub mod buff_pets;
pub mod chain;
pub mod chain_build;
pub mod chain_walk;
pub mod damage;
pub mod effect_registry;
pub mod effective;
pub mod enhancement;
pub mod expr;
pub mod finalize;
pub mod gather;
pub mod granted;
pub mod incarnates;
pub mod inherents;
pub mod movement;
pub mod perma;
pub mod procs;
pub mod projection;
pub mod purple_patch;
pub mod scaled;
pub mod set_bonuses;
pub mod stacking;
pub mod stealth;
pub mod strength;
pub mod totals;
pub mod what_if;
pub mod window_slots;

pub use finalize::{CalculatedTotals, CharacterStats};
pub use totals::{CalcError, GlobalBonuses};

use coh_data::{CharacterState, Level, PowerDatabase};

/// Recalculate a build's totals. The staged pipeline (REBUILD-PLAN §5), populated as far
/// as the landed passes reach:
///
/// * Pass 0 — gather the active, mode-resolved power list ([`gather::gather_active_powers`]).
/// * Pass 1 — collect the +Strength self-buffs into `GlobalBonuses.strength*`
///   ([`strength::collect_strength_buffs`]).
/// * Pass 2a — apply each active power's atom-native families (ToHit, Damage, Defense,
///   Resistance, MaxHP, Regen, Recovery) into the matching totals ([`apply::apply_active_power_bonuses`]).
/// * Pass 2b — the per-family Pass-2b families (recharge, absorb, mez protection/resistance,
///   stealth, …), increasingly atom-native with a `?? bag` fallback (recharge/mez-resistance/
///   stealth migrated — ATOM1/2/3), read alongside 2a; stealth resolves once after the walk.
/// * Pass 3 — the additive archetype-inherent damage (Defender Vigilance, Brute Fury) from
///   `state.combat` ([`inherents`]). Fitness (Health/Stamina) is atomized, so it flows through
///   the apply loop above rather than a dedicated pass.
/// * Pass 6 — the incarnate stat bonuses (Destiny/Hybrid/Genesis + the incarnate level shift)
///   from `state.incarnates` ([`incarnates`]). A targeted, non-atom pass like Pass 3; below
///   level 45 it suppresses everything but the Genesis-Fate exemplar buff. Alpha's enhancement
///   of OTHER powers is applied in Pass 2a instead: [`incarnates::alpha_enhancement`] resolves the
///   equipped Alpha's virtual-enhancement inputs (computed up front, below), which
///   [`apply::apply_active_power_bonuses`] folds into each power's ED aggregation via the split.
///
/// * Pass 8 — caps + projection ([`finalize`]): the purple-patch combat projection
///   (Step 9.5 → `base_to_hit` / `hit_chance` / `combat_modifier` on the accumulator) then
///   `convert_to_character_stats` (combine-by-max, resistance/HP caps, softcap threshold)
///   into the [`CalculatedTotals::stats`] the UI reads.
///
/// The effective level is `state.level` — exemplar scaling arrives with the appliers that
/// consume it (Pass 2), so M3 resolves AT tables at the build level. The purple-patch
/// content mode reads `state.combat.content_mode` — a live `CombatContext` input the
/// Combat panel controls, same as the level shift below it. The level shift is likewise
/// real: Pass 6 sums the incarnate level shift into `g.level_shift`, which nets into the
/// purple-patch `effective_level_diff` below.
pub fn recalculate(state: &CharacterState, db: &PowerDatabase) -> CalculatedTotals {
    recalculate_projecting(state, db, &[])
}

/// [`recalculate`], additionally projecting powers the build does NOT hold (PROD6C).
///
/// The per-power display surfaces render powers you are only hovering in the picker, and those
/// still take Alpha and the build-wide globals — but they are in no `all_selected()` walk, so
/// the projection could not reach them. `extra` names them; each is projected unslotted against
/// this same finalized accumulator, so a hovered power's numbers are computed exactly like a
/// picked one's rather than by a second calculator.
pub fn recalculate_projecting(
    state: &CharacterState,
    db: &PowerDatabase,
    extra: &[projection::PowerRef],
) -> CalculatedTotals {
    recalculate_in_cast_context(state, db, extra, &std::collections::HashMap::new())
}

/// [`recalculate_projecting`] under a per-cast ownership overlay (RB5-d).
///
/// The chain's per-cast walk resolves each activation against the state AT THAT POSITION in
/// the rotation: a charge Total Focus banked two casts ago is owned there even though no pick
/// and no declared toggle holds it, and the Hide meter's position state rides in on
/// `state.combat.hidden`. The overlay is that position's grant ledger, merged into
/// [`gather::owned_powers`]' answer by MAX exactly as the declared-toggle claims are, and for
/// the same reason: an overlay must not unsay a pick.
///
/// This is a full recalculation on purpose. The form a gate selects can change anything
/// downstream (the variant's atoms, its cast, the conditionals it re-admits), so a narrower
/// re-resolve would be a second calculator free to disagree with the one the info panel
/// renders. Callers memoize per distinct context instead: a rotation visits a handful.
pub fn recalculate_in_cast_context(
    state: &CharacterState,
    db: &PowerDatabase,
    extra: &[projection::PowerRef],
    owned_overlay: &std::collections::HashMap<String, f64>,
) -> CalculatedTotals {
    let mut g = GlobalBonuses::default();

    let gathered = gather::gather_active_powers(state, db);
    // A selection the dataset can't resolve surfaces as a per-item error while the
    // rest of the build still computes (Rule 1) — never a silently smaller total.
    g.errors.extend(gathered.unresolved);
    let powers = gathered.powers;
    let source_modes = gathered.source_modes;
    let archetype = state.archetype.id.as_deref().unwrap_or("");
    let level = state.level as i32;

    let sb = strength::collect_strength_buffs(&powers, archetype, level, db, &mut g.errors);
    g.strength_defense = sb.defense;
    g.strength_to_hit = sb.to_hit;
    g.strength_heal = sb.heal;
    g.strength_absorb = sb.absorb;
    g.strength_end_mod = sb.end_mod;
    g.strength_movement = sb.movement;
    g.strength_mez = sb.mez;

    // Set bonuses (M4 — Rule of 5). Applied EARLY, mirroring the beta `applySetBonusesToGlobal`
    // (character-totals.ts:4211): before the per-power apply loop, the incarnate pass, the
    // absorb-fraction resolve, and the purple-patch projection — a load-bearing order, since a
    // set's +MaxHP must feed the absorb-fraction base HP and its +Accuracy/+ToHit must feed the
    // hit-chance projection. Suppression walks EVERY selected power (`all_selected` — bonuses come
    // from slotted enhancements regardless of toggle state, matching the beta's `buildToBuildPowers`),
    // and the two level args match the beta exactly: exemplaring gates on `state.combat.exemplar_level`
    // (the beta `exemplarMode`); OUTSIDE exemplar mode the build level passed is a fixed 50
    // (character-totals.ts:4206), so set bonuses stay full-strength at low build levels and suppress
    // ONLY when explicitly exemplared — `state.level` must NOT drive this.
    // The set-bonus Rule-of-5 provenance for the UI (`(x/5)` counters, capped rings/banner,
    // per-stat tooltip rows). Filled from the same result that feeds the totals below, so the
    // tooltip's bucket decisions ARE the ones that produced the numbers; empty when no catalog.
    let mut set_bonus_tracking = Vec::new();
    if let Some(catalog) = db.io_sets.as_ref() {
        // Beta character-totals.ts:4206 — the non-exemplar set-bonus reference level.
        const NON_EXEMPLAR_SET_BONUS_LEVEL: Level = Level::constant(50);
        let (exemplar_level, set_bonus_level) = match state.combat.exemplar_level {
            Some(exemplar) => (Some(exemplar), exemplar),
            None => (None, NON_EXEMPLAR_SET_BONUS_LEVEL),
        };
        let set_bonus_result = set_bonuses::calculate_set_bonuses(
            state.all_selected(),
            catalog,
            exemplar_level,
            set_bonus_level,
            state.combat.pvp,
        );
        set_bonus_tracking = set_bonuses::tracking_out(&set_bonus_result.tracking);
        set_bonuses::apply_set_bonuses_to_global(&mut g, &set_bonus_result);
    }

    // The equipped Alpha incarnate's virtual-enhancement inputs (the beta's Step-5
    // `getAlphaEnhancementBonuses` / `getAlphaEdBypassBonuses`). Gated by the same
    // suppression/active checks as Pass 6; when active it drives the apply pass's ED split
    // (`combineWithAlphaED`). Reads the loadout + incarnate tables directly, so it does not
    // depend on Pass 6 having run.
    let alpha = incarnates::alpha_enhancement(&state.incarnates, state.combat.exemplar_level, db);

    // Pass 2a/2b — apply per-power families (strength from Pass 1 feeds the ToHit/Defense
    // multipliers; combat context gates suppressible defense; Alpha's split feeds the per-power
    // enhancement multiplier). Stealth radius is COLLECTED per power here, then committed once
    // below — powers sharing a suppress group don't stack.
    let mut stealth_contributions = Vec::new();
    // MaxHP-fraction absorb (Wild Bastion, Ablative) can't resolve during the walk — its HP
    // depends on the fully-summed +MaxHP — so the apply pass collects each `fraction` here and
    // Step 9.2 below resolves them against the final Max HP (character-totals.ts:4420).
    let mut absorb_fraction_contribs = Vec::new();
    // Travel buffs (run/fly/jump) are COLLECTED here and committed below — like stealth, a source's
    // contribution depends on the others (suppress-group max), so no per-power value is final.
    let mut movement_contribs = Vec::new();
    let mut movement_cap_contribs = Vec::new();
    // Self-directed −Resistance penalties (Bio Offensive Adaptation) are COLLECTED here and
    // resolved after every resistance source has summed: CoH mitigates a resistible one by the
    // caster's own resistance to that type, which is not known until then.
    let mut res_self_debuffs = Vec::new();
    // Per-power provenance for the detailed-totals breakdown: which contributor moved which
    // field, measured by diffing the accumulator around each power rather than reported by the
    // appliers (see `apply::PowerBreakdownSource`). Pass 3's archetype inherents file into the
    // same ledger further down — they are powers too, applied by a pass of their own.
    let mut power_breakdown = Vec::new();
    apply::apply_active_power_bonuses(
        &powers,
        &mut g,
        archetype,
        level,
        &sb,
        &state.combat,
        &alpha,
        &mut stealth_contributions,
        &mut absorb_fraction_contribs,
        &mut movement_contribs,
        &mut movement_cap_contribs,
        &mut res_self_debuffs,
        &mut power_breakdown,
        db,
    );

    // Mode-gated conditional contributions — the rebuild analog of the beta `expandActiveConditionals`
    // (character-totals.ts:4375). Bio Armor adaptation and other `Source.Mode?` stances gate extra
    // effects onto their armor powers (Hardened Carapace's Defensive-mode resistance); the base gather
    // drops those `gated` atoms, so they are re-admitted here as slot-less synthetic powers for the
    // modes this build actually has active. Runs in the SAME per-power phase as the real powers — before
    // the movement/stealth resolves below — so any conditional travel/stealth/absorb feeds those too.
    // Empty strength + inactive Alpha + no slots honor the mode atoms' `ignoreStrength` (the beta passes
    // `emptyStrengthBuffs()` / `{}` alpha for the same reason); the contributions SUM onto `g`.
    // What the build owns, for every caster-ownership gate downstream. Resolved ONCE and passed
    // down rather than rebuilt per consumer: it walks the dataset's whole gate corpus to learn
    // which paths are asked about, so a second construction would repeat that walk on every
    // recalculation for an answer that cannot have changed.
    let mut owned = gather::owned_powers(state, db);
    for (path, count) in owned_overlay {
        let normalized = expr::normalize_power_path(path);
        owned
            .entry(normalized)
            .and_modify(|held| *held = held.max(*count))
            .or_insert(*count);
    }
    let conditional_powers = gather::active_conditional_powers(
        &powers,
        &source_modes,
        state.combat.in_combat,
        state.combat.target_identity().1,
        &owned,
    );
    let conditional_active: Vec<gather::ActivePower> = conditional_powers
        .iter()
        .map(|(power_set, p)| gather::ActivePower {
            def: p,
            power_set,
            // A mode-gated stance effect is the parent armour's own contribution re-admitted for
            // the active mode, so it groups with that power rather than as a source of its own.
            kind: gather::PowerSourceKind::ActivePower,
            targets_hit: None,
            slots: &[],
        })
        .collect();
    apply::apply_active_power_bonuses(
        &conditional_active,
        &mut g,
        archetype,
        level,
        &strength::StrengthBuffs::default(),
        &state.combat,
        &incarnates::AlphaEnhancement::default(),
        &mut stealth_contributions,
        &mut absorb_fraction_contribs,
        &mut movement_contribs,
        &mut movement_cap_contribs,
        &mut res_self_debuffs,
        &mut power_breakdown,
        db,
    );

    // Buff-pet auras (beta Step 7.2 `expandBuffPetAuras`) — the ally buff a summoned drone
    // projects lives on the PET ENTITY, never on the summoning power, so the per-power walk
    // above cannot see it. Opt-in per pet; runs in the same per-power phase as the conditionals
    // (and before the movement/stealth commits), matching where the beta folds it in.
    let mut buff_pet_errors = Vec::new();
    let buff_pet_breakdown =
        buff_pets::apply_buff_pet_auras(state, db, &mut g, &mut buff_pet_errors);
    g.errors.extend(buff_pet_errors);

    // Pass 7 — commit the gathered travel buffs (per axis: suppress-group max + additive sum, minus
    // combat-suppressed sources). Runs right after the active-power walk and BEFORE procs/incarnates,
    // matching the beta (`character-totals.ts:4238`, before `applyProcBonuses`/`applyIncarnateBonuses`):
    // set bonuses have already written these fields, and the resolve ADDS on top.
    let movement_breakdown =
        movement::resolve_movement_totals(&movement_contribs, &mut g, state.combat.in_combat);
    // The travel CEILINGS the same powers raise (Super Speed's run cap, Fly's + Afterburner's).
    // Resolved here beside the buffs; Pass 8 measures the projected speeds against them.
    let movement_cap_bumps =
        movement::resolve_cap_bumps(&movement_cap_contribs, state.combat.in_combat);

    // Proc / global-IO pass — always-on globals (LotG +Recharge, Steadfast +Def), Proc120s,
    // PPM procs in auto/toggle (Performance Shifter → recovery), and variable procs (Reactive
    // Defenses, Might of the Tanker). The averaged Build-Up procs are the one pass NOT here;
    // they need the final global recharge and run below. Runs after set bonuses/apply and
    // before incarnates (beta character-totals.ts:4367), pushing stealth-IO radii into the same
    // resolve below and its +MaxHP into the absorb-fraction/projection math further down. Both
    // proc switches ride in on `state`, so the pass takes neither as an argument.
    // Takes the Alpha split because the movement globals are enhanceable by their own slotting
    // power (Thrust's +Run Speed), so the pass needs the same per-power enhancement the apply
    // loop resolves. Its errors land in a local Vec: `g` is already mutably borrowed.
    let mut proc_errors = Vec::new();
    let mut proc_breakdown = procs::apply_procs(
        state,
        db,
        &mut g,
        &mut stealth_contributions,
        &alpha,
        &mut proc_errors,
    );
    g.errors.extend(proc_errors);

    // Pass 2b — commit the gathered stealth radii (grouped max + additive sum, per axis).
    // Deferred past the walk because a source's contribution depends on the others (the proc pass
    // above pushes stealth-IO contributions into this same resolve, character-totals.ts:4294).
    let stealth_totals = stealth::resolve_stealth_radius(&stealth_contributions);
    g.stealth_radius_pve = stealth_totals.pve;
    g.stealth_radius_pvp = stealth_totals.pvp;
    let stealth_breakdown = stealth_totals.breakdown;

    // Pass 3 — the additive archetype-inherent damage (Defender Vigilance, Brute Fury) from the
    // combat context. BOTH are now DERIVED from the extracted inherent powers' atoms (crate::expr):
    // Vigilance from the Vigilance power's `*Uniqueness`-table steps + team-size gates, Fury from
    // Rage_Buff's `kRage source> .02 *` damage expression with the Fury meter as input (DATA-GAP
    // INHERENT-2, resolved on all three datasets). Should an export ever drop the expression, the
    // step fails loud into `g.errors` — never a silent zero or a stale constant.
    // Gather skips archetype inherents, so Pass 3 is their sole home — no
    // double-count. Fitness (Health/Stamina) is NOT here — it is atomized and flows through the
    // apply loop like any power. The add lands last, matching the beta's Step 9.1 order (flat add
    // over the accumulated base damage).
    inherents::apply_archetype_inherents(
        &mut g,
        archetype,
        level,
        &state.combat,
        db,
        &mut power_breakdown,
    );

    // Pass 6 — the incarnate stat bonuses (Destiny/Hybrid/Genesis) and the incarnate level
    // shift, from the equipped loadout. A targeted, non-atom pass like Pass 3 (incarnate
    // effects are not atoms — gather never sees them). Below level 45 it suppresses everything
    // but the Genesis-Fate exemplar buff. Runs before the Pass-8 projection so its level shift
    // feeds `effective_level_diff` and its Destiny/Genesis Max HP feeds the absorb-fraction and
    // cap math below.
    let incarnate_breakdown = incarnates::apply_incarnate_bonuses(
        &mut g,
        &state.incarnates,
        state.combat.exemplar_level,
        state.combat.destiny_time,
        state.combat.hybrid_targets_hit,
        state.combat.incarnate_level_shift,
        db,
    );

    // The what-if TEAM-BUFF layer, injected once every real global source has summed and
    // before anything is resolved, projected or clamped — so a simulated buff is
    // indistinguishable downstream from the equivalent real one. Ahead of Step 9.3 on purpose:
    // a teammate's +resistance would be in place when the deferred self −Res penalties resolve
    // against the caster's own resistance, so a what-if +resistance must be too.
    // Errors land in a local Vec because `g` is already mutably borrowed by the call, the same
    // reason the proc and self-debuff passes below use one.
    let what_if = what_if::apply(&mut g, &state.combat.what_if_buffs);
    g.errors.extend(what_if.errors.iter().cloned());

    // The fourth proc pass, deliberately detached from the three above (PPM-2). A click proc's
    // window is the host power's real recharge with the build's GLOBAL recharge multiplied back
    // in, so it has to be scored after every source of global recharge has summed — Destiny
    // Ageless lands in Pass 6, and the what-if layer just above it. Its writes are additive into
    // damage/toHit and its breakdown rows already came last, so nothing else moves.
    let mut build_up_errors = Vec::new();
    proc_breakdown.extend(procs::apply_build_up_procs(
        state,
        db,
        &mut g,
        &alpha,
        &mut build_up_errors,
    ));
    g.errors.extend(build_up_errors);

    // Step 9.3 — the deferred self −Resistance penalties, now that every resistance source
    // (powers, set bonuses, procs, accolades, incarnates) has summed. A resistible one is
    // mitigated by the caster's own resistance to that type; the displayed magnitude stays
    // nominal. Must run after the incarnate pass and before the caps/projection below.
    // Errors land in a local Vec for the same reason the proc pass above uses one: `g` is
    // already mutably borrowed by the call.
    let mut res_self_debuff_errors = Vec::new();
    apply::resolve_res_self_debuffs(
        &res_self_debuffs,
        &mut g,
        &mut power_breakdown,
        &mut res_self_debuff_errors,
    );
    g.errors.extend(res_self_debuff_errors);

    let caps = db.archetype_stats.get(archetype);
    // A selected archetype the dataset has no caps for is a real data gap. A build with NO
    // archetype is not — it is what the planner holds before the user picks one, and firing
    // here made every boot log a failure that had not happened. A fail-loud channel that
    // cries wolf is a fail-loud channel nobody reads.
    if caps.is_none() && !archetype.is_empty() {
        g.errors.push(CalcError::new(
            "resistance/maxHP/reduction caps",
            format!("no archetype caps for '{archetype}' — resistance stays uncapped, absolute HP is 0, and per-power recharge/endurance divisors go unclamped"),
        ));
    }

    // Step 9.2 — resolve the MaxHP-fraction absorb contributions (Wild Bastion, Ablative) to
    // absolute HP, now that every +MaxHP source has summed — INCLUDING Pass 6's incarnate
    // Destiny/Genesis-Socket +MaxHP, which is why the incarnate pass runs just above (accolades
    // will fold in the same way). The fraction is taken against the build's final Max HP via the
    // same `absolute_max_hp` finalize projects, so an absorb shield that shields "25% of Max HP"
    // tracks the HP the dashboard shows (character-totals.ts:4420). Fails loud (no caps) rather
    // than fabricating a baseline (Rule 1). Runs after Pass 3/6, before projection (9.5).
    if !absorb_fraction_contribs.is_empty() {
        match caps {
            Some(caps) => {
                let actual_hp = finalize::absolute_max_hp(caps, g.max_hp, level);
                for contribution in &absorb_fraction_contribs {
                    let hp = contribution.fraction * actual_hp;
                    if hp > 0.0 {
                        g.absorb += hp;
                        // Attributed here rather than by a snapshot bracket: this resolve runs
                        // outside the apply walk, so each contribution carries the identity the
                        // walk knew and the row is filed against the HP it actually added.
                        power_breakdown.push(apply::PowerBreakdownSource {
                            breakdown_key: "absorb".to_string(),
                            power_internal_name: contribution.power_internal_name.clone(),
                            power_set: contribution.power_set.clone(),
                            value: hp,
                            kind: contribution.kind,
                        });
                    }
                }
            }
            None => g.errors.push(CalcError::new(
                "absorb",
                "no archetype caps for this build — MaxHP-fraction absorb cannot resolve",
            )),
        }
    }

    // Pass 8 — caps + projection. Step 9.5 first: the purple-patch combat projection onto the
    // accumulator. `effective_level_diff = enemy_level_offset − level_shift` — Pass 6's incarnate
    // level shift now nets out here (a +1 shift raises the player's effective combat level,
    // shrinking the gap to a higher-level enemy). The defense softcap below reads
    // `enemy_level_offset` DIRECTLY, NOT netted — the beta does not net the level shift for the
    // softcap (character-totals.ts vs StatsDashboard.tsx), a deliberate asymmetry that is now
    // live (no longer moot at level shift 0) and pinned by a fixture.
    // Saturating because `enemy_level_offset` is an `i32` straight off the wire and nothing
    // bounds it: `i32::MIN` beside any earned level shift overflowed here, which is a panic in
    // a checked build and a SIGN FLIP in the release wasm the site ships — the shape Rule 1
    // calls worse than a break. Saturating changes no real reading: both lookups below clamp
    // their index into the dataset's own table, so a saturated diff resolves exactly where an
    // absurd-but-representable one already did.
    let effective_level_diff = state
        .combat
        .enemy_level_offset
        .saturating_sub(g.level_shift as i32);
    if let Err(e) = finalize::project_combat(
        &mut g,
        &db.purple_patch,
        effective_level_diff,
        state.combat.content_mode,
    ) {
        g.errors.push(e);
    }

    let defense_softcap = purple_patch::get_defense_softcap(
        &db.purple_patch,
        state.combat.enemy_level_offset,
        state.combat.content_mode,
    );
    let (stats, mut projection_errors) =
        finalize::convert_to_character_stats(&g, caps, defense_softcap, level, &movement_cap_bumps);
    g.errors.append(&mut projection_errors);

    // PROD6B-1 — the per-power non-DPS execution + perma projection. Runs last: it reads the
    // FINALIZED global recharge / endurance / accuracy / range percents for each power's "final"
    // tier, and reuses the same `alpha` the apply loop did so a power's projected recharge equals
    // the recharge that fed the totals. A pure re-walk of the selected powers — the accumulator is
    // already sealed, so this cannot perturb any total (additive, like PROD2's tracking blocks).
    let mut projection_gaps = Vec::new();
    let power_projection = projection::project_powers(
        state,
        &source_modes,
        &g,
        &alpha,
        db,
        &mut projection_gaps,
        extra,
        &owned,
    );
    g.errors.append(&mut projection_gaps);

    // Step 9.7 — the toggle drain and the net endurance rate, the last two totals, because both
    // read the projection above (and it reads the sealed globals). The net rate is the beta's
    // close verbatim: a base 100 endurance plus flat +MaxEnd, recovered over 60s, scaled by
    // +Recovery, minus what the toggles burn.
    // The drain arrives with its own per-toggle rows rather than through a `deltas_since`
    // bracket — it is summed from named powers, so the contributors are already in hand.
    let toggle_endurance = projection::toggle_endurance_total(state, &power_projection, db);
    g.toggle_end_cost = toggle_endurance.total;
    power_breakdown.extend(toggle_endurance.breakdown);
    // `netEndPerSec` is a formula over three attributed fields, not an accumulation, so it files
    // no rows of its own (`GlobalBonuses::UNATTRIBUTABLE_KEYS`).
    g.net_end_per_sec =
        ((100.0 + g.max_endurance) / 60.0) * (1.0 + g.recovery / 100.0) - g.toggle_end_cost;

    // The damage-bar scale (RB5-d). Runs after the projection because it reads the sealed global
    // damage buff the projections' own final tier used, so a power's bar and the ceiling it is
    // drawn against agree by construction.
    let damage_ceiling = projection::damage_ceiling(state, &source_modes, &g, db, &owned);

    CalculatedTotals {
        bonuses: g,
        stats,
        set_bonus_tracking,
        proc_breakdown,
        buff_pet_breakdown,
        movement_breakdown,
        stealth_breakdown,
        power_projection,
        power_breakdown,
        incarnate_breakdown,
        damage_ceiling,
        what_if,
    }
}
