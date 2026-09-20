#!/usr/bin/env python3
"""Export player-relevant power data from Homecoming bins as structured JSON.

Generates CoD2-compatible JSON files organized as:
  output_dir/category/powerset/power.json

Only exports the 34 player-relevant categories (out of 204 total),
filtering out NPC/critter/pet data.

Usage:
  py -3 export_powers.py [--bin-dir G:/Homecoming/assets/live/bin] [--output-dir ./exported_powers]
"""

import argparse
import json
import os
import re
import sys
from dataclasses import asdict
from pathlib import Path

# Allow running directly as a script: add the enclosing tools/bin-crawler/
# directory to sys.path so `bin_crawler` is importable.
sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from bin_crawler.parser._powers import (
    parse_powers, detect_dataset_flavor, resolve_attrib_thunderspy)
from bin_crawler.parser._powersets import parse_powersets
from bin_crawler.parser._powercats import parse_powercats
from bin_crawler.parser._classes import parse_classes, player_class_names
from bin_crawler.parser._boostsets import parse_boostsets, build_power_category_index
from bin_crawler.parser._dim_returns import parse_dim_returns, parse_boost_effect
from bin_crawler.parser._schedules import parse_schedules, parse_exemplar_handicaps
from bin_crawler.parser._messages import load_messages
from bin_crawler.parser._attrib_names import parse_mode_table, parse_stack_key_table
from bin_crawler.parser._pigg import BinResolver
from bin_crawler.assets_dir import add_source_arguments, resolve_export_source
from bin_crawler._export_fingerprint import parser_fingerprint
from bin_crawler.path_safety import safe_path_component
from bin_crawler.parser._enums import (
    POWER_TYPE, EFFECT_AREA, PVP_FLAG, CASTABLE_AFTER_DEATH,
    SHOW_POWER_SETTING,
    BOOST_TYPE, BOOST_TYPE_REBIRTH,
    ATTRIB_NAME, ATTRIB_NAME_REBIRTH, ATTRIB_NAME_THUNDERSPY,
    resolve_attrib, resolve_attrib_rebirth,
)


# StrengthsDisallowed stores raw attrib offsets, whose index space is
# per-dataset (Rebirth shifts Accuracy/Range; Thunderspy shifts the
# Recharge/Interrupt band). Every flavor routes through a band-aware resolver
# so a special-band offset surfaces as `Special(<raw>)` rather than borrowing
# a normal-band name; Thunderspy's reads the calibrated band base, since that
# base is corpus-derived per build rather than constant.
_STRENGTH_ATTRIB_RESOLVER = {
    'homecoming': resolve_attrib,
    'veracity': resolve_attrib,
    'rebirth': resolve_attrib_rebirth,
    'thunderspy': resolve_attrib_thunderspy,
}


# Categories containing player-usable powers
PLAYER_CATEGORIES = {
    # Standard AT primary/secondary
    'Blaster_Ranged', 'Blaster_Support',
    'Brute_Defense', 'Brute_Melee',
    'Controller_Buff', 'Controller_Control',
    'Corruptor_Buff', 'Corruptor_Ranged',
    'Defender_Buff', 'Defender_Ranged',
    'Dominator_Assault', 'Dominator_Control',
    'Mastermind_Buff', 'Mastermind_Summon',
    'Scrapper_Defense', 'Scrapper_Melee',
    'Sentinel_Defense', 'Sentinel_Ranged',
    'Stalker_Defense', 'Stalker_Melee',
    'Tanker_Defense', 'Tanker_Melee',
    # Rebirth Guardian — primary "Assault" sets and secondary "Composition"
    # sets (armor + utility). HC has no equivalent AT.
    'Guardian_Assault', 'Guardian_Comp',
    # Kheldians
    'Peacebringer_Defensive', 'Peacebringer_Offensive',
    'Warshade_Defensive', 'Warshade_Offensive',
    # Thunderspy Primalist — a custom Kheldian-style form-shifter AT. Primary
    # 'Feral_Might', secondary 'Primal_Gifts'; the Hunter/Prowler/Primal form
    # attack variants + per-attack lifesteal redirects live in 'Primalist_Misc'
    # (listed below with the Lore/NPC pet categories).
    #   GOTCHA: HC's powers.bin ALSO carries ~63 ORPHAN Primalist powers under
    #   these same name-prefixes — with NO backing powerset, powercat, or class.
    #   (The old comment here wrongly assumed they "won't exist" in HC; they do,
    #   and leaked into the HC export.) These categories are therefore gated at
    #   runtime by `_source_has_primalist_class` below: kept ONLY when the source
    #   actually defines a Primalist class (Thunderspy), dropped for HC/Rebirth.
    'Feral_Might', 'Primal_Gifts',
    # VEATs
    'Arachnos_Soldiers', 'Widow_Training', 'Teamwork',
    # VEAT Soldier secondaries: Training_and_Gadgets, Crab_Spider_Training,
    # Bane_Spider_Training. Without this category the Soldier secondary
    # picker is empty and branches reuse the primary as the secondary.
    'Training_Gadgets',
    # Pools, Epic, Inherent
    'Pool', 'Epic', 'Inherent',
    # Prestige — where the free travel toggles actually live. The `Inherent`
    # copies of Ninja Run / Beast Run are Auto granters whose only templates are
    # Revoke_Power + Grant_Power aimed at `Prestige.Prestige_Travel.*`; the real
    # toggle, with the movement it grants and its endurance drain, is here.
    # Athletic Run has no Inherent granter at all. Without this category the
    # planner had to hand-author all three, and the hand copy was missing their
    # MovementControl +10 / MovementFriction +2 (ATOM-BAG-6).
    'Prestige',
    # Incarnate
    'Incarnate',
    # Redirects (pseudo-pet damage sources)
    'Redirects',
    # Aux categories: hold the actual AoE/hit data for leap/charge attacks
    # (Savage Leap, Feral Charge, etc.) referenced via Execute_Power.
    'Brute_Melee_Aux', 'Dominator_Assault_Aux', 'Scrapper_Melee_Aux',
    'Stalker_Melee_Aux', 'Tanker_Melee_Aux',
    # Pets categories — host the actual damage data for player powers that
    # delegate via top-level `redirect`. Snipes are the canonical case:
    # `Sniper_Blast` has zero effects and points at `Pets.Blaster_Energy_Snipe.
    # Sniper_Blast_Normal` / `_Quick` for its Normal vs fast-snipe variants.
    # The convert script's collectRedirectTemplates resolves these via
    # `pets/<set>/<power>.json`, so they need to be on disk.
    'Pets', 'Villain_Pets',
    # Mastermind henchman powers (Plasma_Blast, Smash, etc.) and the upgrade
    # tiers (`*_2`, `*_3` powersets). Without these, convert-pet-entities can't
    # resolve any MM henchman ability.
    'Mastermind_Pets',
    # Kheldian pets (Decoy, Dwarf, etc.) and NPC/Lore pets that incarnate
    # entities can grant.
    'Kheldian_Pets', 'NPC_Pets',
    # NPC/villain-group categories that Lore incarnate pets pull abilities
    # from (the pet "mimics" enemies of that group). The dominant ones by
    # entity reference count are Rularuu (146), Objects (22), and several
    # villain-group cats. Without these the pet-entity converter drops
    # Lore Support / Cimeroran / Banished / etc. variants.
    'Rularuu', 'Objects', 'Primalist_Misc', 'Signature_Summon',
    'Cabal', 'Council', 'V_Arachnos', 'DevouringEarth',
    'Mission_Maker_Attacks', 'Crey', 'RoguesGallery', 'GenericVillains',
    'Rikti', 'V_Wailers', 'CircleOfThorns', 'Clockwork', 'Vanguard',
    'V_Miscellaneous', 'PaladinEvent',
}


# Thunderspy-only Primalist categories. HC/Rebirth carry orphan powers under
# these prefixes (no powerset/powercat/class), so they must be gated on the
# source actually defining a Primalist class — see _source_has_primalist_class.
PRIMALIST_CATEGORIES = {'Feral_Might', 'Primal_Gifts', 'Primalist_Misc'}


# Boost-piece power templates: 'Boosts' (one powerset per IO piece, e.g.
# Boosts.Crafted_Absolute_Amazement_A) and 'Set_Bonus' (the AutoPowers a
# boostset's bonus tiers reference). Each carries the per-aspect fScale,
# boost-class table linkage, and proc/unique markers that boostsets.json's
# piece name refs point at but don't themselves contain. HC-1 records their
# exclusion from PLAYER_CATEGORIES as deliberate (they aren't archetype/pool
# player powers), so they're exported as a separate tree — see the pass
# after the main write in main().
BOOST_PIECE_CATEGORIES = {'Boosts', 'Set_Bonus'}


# Powersets that live inside the otherwise-excluded Temporary_Powers category but
# ARE real, player-facing content. Accolades are auto-on, gated self powers
# (`kAuto`, `ActivateRequires type char> hero eq`, IgnoreStrength/IgnoreResistance)
# carrying ordinary maxHP/maxEnd AttribMods — the beta dropped them with the rest
# of Temporary_Powers and hand-built a `src/data/accolades.ts` silo re-encoding
# their values (ACCOLADE-1). Extract the whole powerset so they derive as ordinary
# powers. The set is included WHOLE — the passive stat accolades AND the members
# that happen to be clicks/travel/temp-weapons — because filtering to "just the
# stat ones" would re-encode a curated silo (a game proper-noun include list in
# logic). Downstream decides which carry build-relevant effects.
INCLUDED_TEMPORARY_POWERSETS = {'Accolades'}


def _source_has_primalist_class(resolver) -> bool:
    """True iff the source defines a Primalist archetype (Thunderspy).

    Used to decide whether the Primalist power categories are real player
    content (Thunderspy: wired to the Primalist class) or orphan leakage
    (HC/Rebirth: powers exist in powers.bin but no class/powerset references
    them). Checks both the hero and villain class tables.
    """
    for bin_name in ('classes.bin', 'villain_classes.bin'):
        if not resolver.has(bin_name):
            continue
        try:
            for c in parse_classes(resolver.read(bin_name)):
                if 'primalist' in (getattr(c, 'name', '') or '').lower():
                    return True
        except Exception:
            continue
    return False


def _player_class_names(resolver) -> tuple[str, ...]:
    """The `Class_*` tokens of this source's player archetypes.

    An effect gate reading `arch source> Class_Scrapper eq` is base for a
    Scrapper and a branch for everyone else, and one on `arch target>` asks
    about an archetype or about a critter's rank depending on nothing but the
    class it names. `_gate_default` answers both from this list — so an export
    without it would ship a guess as data (DATA-GAP-REGISTER AT-FORK-1), which
    is why an empty one stops the export rather than degrading it. Both class
    tables are read; the villain archetypes live in the second.
    """
    names = []
    for bin_name in ('classes.bin', 'villain_classes.bin'):
        if resolver.has(bin_name):
            names.extend(player_class_names(parse_classes(resolver.read(bin_name))))
    if not names:
        raise ValueError(
            'no player archetypes found in classes.bin / villain_classes.bin — '
            'every archetype-forked effect gate would export unresolved')
    return tuple(names)


def format_duration(seconds: float) -> str:
    """Format duration as '120 seconds' or '0 seconds'.

    Rounds to 4 decimals — enough to keep authored sub-second precision
    (1.125 stays '1.125', which a 2-decimal round would corrupt to '1.12')
    while stripping float32 read noise (180.3000030517578 → '180.3').
    """
    seconds = round(seconds, 4)
    if seconds == int(seconds):
        return f"{int(seconds)} seconds"
    return f"{seconds} seconds"


def power_to_dict(pw, msgs=None, set_cats_index=None, mode_table=None,
                  stack_key_table=None, dataset_flavor='homecoming') -> dict:
    """Convert a PowerRecord to CoD2-compatible JSON dict.

    set_cats_index maps `full_name` → list of planner set-category strings
    derived from boostsets.bin's authoritative per-set power lists.

    Three states for `allowed_set_categories` in the output:
      - List with entries: the game says these categories slot here.
      - Empty list `[]`: the game has the power in boostsets but it maps
        to nothing — strict "no IO sets here" (e.g., SR Quickness).
      - `null`: boostsets wasn't loaded *or* this power isn't in the
        index. Downstream converters should fall back to inference.

    The previous behavior conflated "boostsets says nothing" with "no
    boostsets at all", which made every Rebirth power look like a strict
    no-IO power on datasets where boostsets.bin wasn't shipped.
    """
    d = {
        'name': pw.power_name,
        'full_name': pw.full_name,
        'short_name': pw.power_name,
        'type': POWER_TYPE.get(pw.power_type, f'Unknown({pw.power_type})'),
        'display_name': pw.display_name,
        'display_fullname': pw.full_name.replace('.', ' ').replace('_', ' ') if '.' in pw.full_name else pw.display_name,
        'display_help': pw.display_help,
        'display_short_help': pw.short_help,
        'icon': pw.icon.lower().replace('.tga', '.png') if pw.icon else '',
        'auto_issue': pw.auto_issue,
        'auto_issue_keeps_level': pw.auto_issue_keeps_level,
        'free': pw.free,
        'accuracy': round(pw.accuracy, 6),
        'effect_area': EFFECT_AREA.get(pw.effect_area, f'Unknown({pw.effect_area})'),
        'max_targets_hit': pw.max_targets_hit,
        'radius': pw.radius,
        'arc': pw.arc,
        'range': pw.range,
        'range_secondary': pw.range_secondary,
        'activation_time': round(pw.time_to_activate, 4),
        # TimeToRoot (Parse7 field 48b) — animation-lock duration. Emitted only
        # when nonzero: 0 means the .powers def omits it, and Parse6 datasets
        # (Rebirth/Thunderspy) don't serialize the field at all.
        **({'time_to_root': round(pw.time_to_root, 4)} if pw.time_to_root else {}),
        'recharge_time': round(pw.recharge_time, 4),
        'activate_period': round(pw.activate_period, 4),
        'endurance_cost': round(pw.endurance_cost, 4),
        'interrupt_time': round(pw.interrupt_time, 4),
        'target_type': pw.target_type_table.get(pw.target_type, f'Unknown({pw.target_type})'),
        'target_type_secondary': pw.target_type_table.get(pw.target_type_secondary, None),
        'targets_autohit': [pw.target_type_table.get(v, f'Unknown({v})') for v in pw.targets_autohit],
        'targets_affected': [pw.target_type_table.get(v, f'Unknown({v})') for v in pw.targets_affected],
        'boosts_allowed': pw.boosts_allowed,
        'allowed_set_categories': (
            # `set_cats_index.get(pw.full_name)` returns:
            #   list  — power found, empty list means "no sets" (strict)
            #   None  — power not in index OR no boostsets.bin loaded
            #           → emit null so downstream can infer.
            set_cats_index.get(pw.full_name) if set_cats_index else None
        ),
        'cast_through': pw.cast_through,
        'toggle_ignore': pw.toggle_ignore,
        'num_allowed': pw.num_allowed,
        # Usage-limit / lifetime block (parse-table fields 56-65, LIFETIME-1).
        # `lifetime` is wall-clock seconds from grant to removal;
        # `lifetime_in_game` counts only time in play (power_CheckUsageLimits).
        # Each field is emitted only when nonzero: absence is a stated 0 — no
        # limit — matching the parse-table defaults. destroy_on_limit /
        # stacking_usage are read by the game only under an active limit
        # (ExpirePowers_Tick), so they ride along only when some limit field is
        # present; destroy_on_limit's authored default is TRUE.
        **({'num_charges': pw.num_charges} if pw.num_charges else {}),
        **({'max_num_charges': pw.max_num_charges} if pw.max_num_charges else {}),
        **({'usage_time': round(pw.usage_time, 4)} if pw.usage_time else {}),
        **({'max_usage_time': round(pw.max_usage_time, 4)} if pw.max_usage_time else {}),
        **({'lifetime': round(pw.lifetime, 4)} if pw.lifetime else {}),
        **({'max_lifetime': round(pw.max_lifetime, 4)} if pw.max_lifetime else {}),
        **({'lifetime_in_game': round(pw.lifetime_in_game, 4)} if pw.lifetime_in_game else {}),
        **({'max_lifetime_in_game': round(pw.max_lifetime_in_game, 4)} if pw.max_lifetime_in_game else {}),
        **({
            'destroy_on_limit': pw.destroy_on_limit,
            'stacking_usage': pw.stacking_usage,
        } if (pw.num_charges or pw.usage_time or pw.lifetime or pw.lifetime_in_game) else {}),
        'requires': pw.requires,
        'activate_requires': pw.activate_requires,
        'target_requires': pw.target_requires,
        # Slot-requires expression — populated on Boost (IO piece) power
        # records with a "BoostsSlotted>X <= 0" constraint when the piece
        # is unique within its slot pool (purples, ATOs, ATIO globals).
        'slot_requires': pw.slot_requires,
        'attack_types': pw.attack_types,
    }

    # Effects (recursive — each effect group can have nested child groups,
    # which the convert script reads as `child_effects`).
    def _eg_to_dict(eg):
        out = {
            'chance': eg.chance,
            'ppm': eg.ppm,
            'delay': round(eg.delay, 4),
            'radius_inner': eg.radius_inner,
            'radius_outer': eg.radius_outer,
            'requires_expression': eg.requires_expression,
            # Effect Tag(s) — the global-chance-mod bucket (Dual Pistols ammo
            # tag groups, etc.). Captured so the converter can attribute
            # tag-gated effects to their mode instead of folding into base.
            'tags': getattr(eg, 'tags', []),
            'flags': eg.flags,
            'is_pvp': eg.is_pvp,
            'requires_pv': eg.requires_pv,
            'requires_default': eg.requires_default,
            'eval_flags': eg.eval_flags,
            'templates': [],
        }
        # Only an archetype-forked gate has one, and only there does it mean
        # anything: the archetypes the gate IS satisfied for, resolving the fork
        # `requires_default` names (AT-FORK-1). Omitted elsewhere so the field
        # appears on the groups it decides rather than on all 700k.
        if getattr(eg, 'requires_archetypes', None):
            out['requires_archetypes'] = list(eg.requires_archetypes)
        for t in eg.templates:
            tmpl_dict = {
                'attribs': t.attribs,
                'type': t.type,
                'application_type': t.application_type,
                'aspect': t.aspect,
                'target': t.target,
                'table': t.table,
                'scale': round(t.scale, 6),
                'duration': format_duration(t.duration),
                'magnitude': t.magnitude,
                'delay': round(t.delay, 4),
                'duration_expression': t.duration_expression,
                'magnitude_expression': t.magnitude_expression,
                'application_period': t.application_period,
                'tick_chance': t.tick_chance,
                'tick_mag_multiplier': t.tick_mag_multiplier,
                'tick_mag_additive': t.tick_mag_additive,
                'jit_requires': t.jit_requires,
                'caster_stack': t.caster_stack,
                'stack': t.stack,
                'stack_limit': t.stack_limit,
                # Resolve the StackKeys-registry ID (attrib_names.bin) to its
                # key name — 'TravelBuff', 'StealthToggle', etc. Falls back to
                # a stable 'Key<N>' placeholder when the registry is missing
                # or the ID is out of range, so suppress-group GROUPING stays
                # correct even unresolved. (The pre-2026-07 exports decoded
                # this field as a string offset, yielding garbage suffixes of
                # the string table's first entry: 'ictusFX' = TravelBuff.)
                'stack_key': (
                    (stack_key_table or {}).get(t.stack_key_id, f'Key{t.stack_key_id}')
                    if t.stack_key_id else None
                ),
            }
            # Marker-target info (marker_names/marker_count) — Marker-targeted
            # mods only; already in CoD2's target_info shape.
            if t.target_info:
                tmpl_dict['target_info'] = t.target_info
            if t.params:
                tmpl_dict['params'] = t.params
            # Per-template Messages / FX from the decoded AttribMod tail.
            # Message values are stored as message-store keys in the binary;
            # resolve them to display text the same way power display_name is.
            if t.messages:
                tmpl_dict['messages'] = {
                    k: (msgs.resolve(v) if msgs and v else v)
                    for k, v in t.messages.items()
                }
            if t.fx:
                tmpl_dict['fx'] = t.fx
            # Cancel/required/suppress events from the AttribMod tail. Only
            # emit when non-empty to keep the JSON small (most templates have
            # none of the three).
            if t.cancel_events:
                tmpl_dict['cancel_events'] = t.cancel_events
            if t.required_events:
                tmpl_dict['required_events'] = t.required_events
            if t.suppress_events:
                tmpl_dict['suppress_events'] = t.suppress_events
            # Decoded flag names (e.g. ["IgnoreStrength", "IgnoreResistance"])
            # plus both raw bitmask words for any bits we haven't named yet
            # (flags2_raw's bits are attrib-contextual — see _FLAG2_BITS_BY_ATTRIB).
            if t.flags:
                tmpl_dict['flags'] = t.flags
            if t.flags_raw:
                tmpl_dict['flags_raw'] = t.flags_raw
            if t.flags2_raw:
                tmpl_dict['flags2_raw'] = t.flags2_raw
            if t.boost_mod_allowed_id:
                tmpl_dict['boost_mod_allowed_id'] = t.boost_mod_allowed_id
            # Resolve a Set_Mode/Unset_Mode template's opaque magnitude (a mode
            # index) to its mode name via attrib_names.bin, e.g. magnitude 46 ->
            # mode "Domination_Active", magnitude 1 -> "Peacebringer_Blaster_Mode"
            # (Bright Nova). Gated on magnitude >= 1: mode index 0 is the
            # ServerTrayOverride system slot (no gameplay meaning). The old
            # mag >= 2 gate was a workaround for the attrib-118 misdecode
            # (kXPDebtProtection / kSetCostume collapsing onto Set_Mode with a
            # placeholder magnitude of 1); that misdecode is now fixed at the
            # source (resolve_attrib), so genuine mode-1 sets resolve correctly.
            if mode_table and ('Set_Mode' in t.attribs or 'Unset_Mode' in t.attribs):
                idx = int(round(t.magnitude))
                if idx >= 1:
                    mode = mode_table.get(idx)
                    if mode:
                        tmpl_dict['mode_name'] = mode
            out['templates'].append(tmpl_dict)
        children = getattr(eg, 'child_groups', None) or []
        if children:
            out['child_effects'] = [_eg_to_dict(c) for c in children]
        return out

    d['effects'] = [_eg_to_dict(eg) for eg in pw.effects]
    if pw.activation_effects:
        d['activation_effects'] = [_eg_to_dict(eg) for eg in pw.activation_effects]
    if pw.redirects:
        d['redirect'] = pw.redirects
    # ChainEff — per-jump chain-continue chance expression (e.g. Chain Induction,
    # Jolting Chain). Sparse (only chain powers), so emit only when present.
    if pw.chain_eff_expression:
        d['chain_eff_expression'] = pw.chain_eff_expression
    # ChainTarget — next-target selection weighting (Electrical Affinity circuits:
    # Rejuvenating/Energizing/Empowering/Insulating_Circuit, Chain_Lightning, …).
    # Parse7 field 43b; verified against the HC `.powers` oracle. Sparse.
    if pw.chain_target_expression:
        d['chain_target_expression'] = pw.chain_target_expression
    # MaxTargetsExpr — RPN target-cap (Gauntlet attacks, the circuits). Parse7
    # field 38 (HC-only); verified via GauntletTargetCap. Sparse.
    if pw.max_targets_expression:
        d['max_targets_expression'] = pw.max_targets_expression

    # WS6 / HC-2 field-capture backlog — all census-verified against the
    # `.powers` oracle 2026-07-21. Sparse: emitted only when they differ from
    # the parse-table default. Parse6 datasets decode the i24-original slice
    # of this set (MaxBoosts, StrengthsDisallowed; CastableAfterDeath and
    # ChainDelay come from the record head) and never carry the HC-added
    # OverCap block or MaxToggleTime. ProcAllowed is the exception among the
    # HC additions: Thunderspy carries that word too (TSPY-5) and reads 0
    # corpus-wide, while Rebirth's stock tail has no slot for it.
    if pw.castable_after_death:
        d['castable_after_death'] = CASTABLE_AFTER_DEATH.get(
            pw.castable_after_death, f'Unknown({pw.castable_after_death})')
    if pw.chain_delay:
        d['chain_delay'] = round(pw.chain_delay, 4)
    if pw.over_cap_trigger or pw.over_cap_multiplier != 1.0 or pw.over_cap_exponential:
        d['over_cap_trigger'] = pw.over_cap_trigger
        d['over_cap_multiplier'] = round(pw.over_cap_multiplier, 6)
        d['over_cap_exponential'] = pw.over_cap_exponential
    if pw.max_toggle_time:
        d['max_toggle_time'] = round(pw.max_toggle_time, 4)
    if pw.max_boosts != 6:
        d['max_boosts'] = pw.max_boosts
    # ProcAllowed: the only authored value is `ProcAllowed kNone` (raw 1) —
    # emitted as an explicit false so consumers read "no proc fires here". It
    # is a FIRING rule, not a slotting one: the flagged powers still author
    # their set categories, and the calc reads it in the two rolling proc
    # passes (PPM-4). Any other nonzero raw would be a new HC variant —
    # surface it.
    if pw.proc_allowed_raw:
        if pw.proc_allowed_raw != 1:
            print(f"  Warning: {pw.full_name}: unrecognized ProcAllowed raw "
                  f"value {pw.proc_allowed_raw} (expected 0 or 1=kNone)")
        d['procs_allowed'] = False
    if pw.strengths_disallowed:
        attrib_resolver = _STRENGTH_ATTRIB_RESOLVER[dataset_flavor]
        d['strengths_disallowed'] = [
            attrib_resolver(v) for v in pw.strengths_disallowed]
    if pw.global_strengths_disallowed:
        attrib_resolver = _STRENGTH_ATTRIB_RESOLVER[dataset_flavor]
        d['global_strengths_disallowed'] = [
            attrib_resolver(v) for v in pw.global_strengths_disallowed]
    # ProcMainTargetOnly / AnimMainTargetOnly: sparse-true, matching how the
    # `.powers` source authors them (`kTrue` or absent). The proc one drives the
    # PPM area factor: the game returns 1.0 outright for a flagged power, before it
    # looks at radius. Parsed here since HC-3, read by the calc since PPM-3 (both
    # in docs/gaps/procs-ppm.md). Parse7 only — the Parse6 tail reader stops
    # at StrengthsDisallowed, so the forks' zero is this reader's, not their bins'.
    if pw.proc_main_target_only:
        d['procs_only_on_main_target'] = True
    if pw.anim_main_target_only:
        d['anim_main_target_only'] = True
    # ShowInInventory / ShowInManage / ShowInInfo (SHOWFLAGS-1, both layouts):
    # sparse against the parse-table defaults (Default / true / true), so
    # absence states the default the same way the authored defs do. The
    # parser's value guards close the vocabulary (enum <= 4, bools <= 1);
    # `show_in_manage: false` is what separates a set-mechanic grant from a
    # real auto-power pick, and the converter's mechanicType classifier reads
    # all three fields under these exact names.
    if pw.show_in_inventory_raw != 1:
        d['show_in_inventory'] = SHOW_POWER_SETTING[pw.show_in_inventory_raw]
    if not pw.show_in_manage:
        d['show_in_manage'] = False
    if not pw.show_in_info:
        d['show_in_info'] = False
    # The two unnamed tail bools read [0, 1] on every authored player power;
    # only the ~50 NPC records that deviate are worth carrying (see
    # `_parse_power_tail`). Emitting the default would put a redundant pair on
    # all 26k powers.
    if pw.tail_unknown_bools_raw and pw.tail_unknown_bools_raw != [0, 1]:
        d['tail_unknown_bools_raw'] = list(pw.tail_unknown_bools_raw)

    # Power-level mode gates (ModesRequired / ModesDisallowed / ModesSuspended)
    # resolved to mode names via the same registry Set_Mode magnitudes index.
    # These gate a power to/from a caster mode: `modes_required` fires it only
    # in those modes (Domination, Titan Weapons FastMode/Momentum, DP LethalAmmo
    # swaps, Primalist Hunter/Prowler forms), `modes_disallowed` blocks it, and
    # `modes_suspended` auto-detoggles it (Stone Armor toggles under Granite,
    # Kheldian travel powers in Blaster/Tanker forms). Resolved-and-sparse: only
    # emitted when non-empty; unresolved indices are dropped, and the whole block
    # is skipped when no mode table was loaded (non-HC sources) so raw per-server
    # indices never leak into the export. Mirrors `EffectTemplate.mode_name`.
    if mode_table:
        for field_name, idxs in (
            ('modes_required', pw.modes_required),
            ('modes_disallowed', pw.modes_disallowed),
            ('modes_suspended', pw.modes_suspended),
        ):
            names = [mode_table[i] for i in idxs if i in mode_table]
            if names:
                d[field_name] = names

    # Per-power free-slot schedule (FreeBoostSlotsOnPower) — 0-based
    # levels-owned offsets overriding the global schedules.bin grant (see
    # PowerRecord for semantics). Level data, not mode indices, so it needs
    # no mode table. Sparse: Rebirth Health/Stamina only in shipped data.
    if pw.free_boost_slots_on_power:
        d['free_boost_slots_on_power'] = pw.free_boost_slots_on_power

    return d


# The attrib a power delegates its work through, by fork spelling. Homecoming
# and Thunderspy author `Execute_Power`; Rebirth authors `Power_Redirect` and
# carries no Execute_Power template at all, so reading one fork's spelling
# reports the other as having no wrappers (DATA-GAP WRAP-2).
WRAPPER_ATTRIBS = ('Execute_Power', 'Power_Redirect')

GRANT_ATTRIBS = ('Grant_Power',)


def _collect_referenced_targets(powers, attribs, prefix=None):
    """Collect the dotted power names `powers` reference through `attribs`.

    Walks the full effect tree — top-level groups, child_groups, and
    activation_effects — since a reference can nest anywhere. `prefix` restricts
    the result to one category (a lowercased dotted prefix); without it every
    referenced name is returned. Returns a set of lowercased full names.
    """
    wanted = set(attribs)
    targets: set[str] = set()

    def walk(groups):
        for g in groups or []:
            for t in getattr(g, 'templates', []) or []:
                if not wanted & set(getattr(t, 'attribs', None) or []):
                    continue
                params = getattr(t, 'params', None) or {}
                for name in params.get('power_names') or []:
                    if prefix is None or name.lower().startswith(prefix):
                        targets.add(name.lower())
            walk(getattr(g, 'child_groups', None))

    for pw in powers:
        walk(getattr(pw, 'effects', None))
        walk(getattr(pw, 'activation_effects', None))
    return targets


# Dataset-correct enum tables for resolving dim_returns.bin references
# (Rebirth diverges on both enums; Thunderspy only on the attrib band).
_CURVE_ENUMS_BY_FLAVOR = {
    'rebirth':     (BOOST_TYPE_REBIRTH, ATTRIB_NAME_REBIRTH),
    'thunderspy':  (BOOST_TYPE, ATTRIB_NAME_THUNDERSPY),
    'veracity':    (BOOST_TYPE, ATTRIB_NAME),
    'homecoming':  (BOOST_TYPE, ATTRIB_NAME),
}


def _export_enhancement_curves(resolver: BinResolver, powers_data,
                               output_dir: Path) -> None:
    """Write enhancement_curves.json: the ED tier curves + boost-type→attrib
    schedule assignment from dim_returns.bin, and the three boost_effect_*
    effectiveness tables. Absence of a bin is surfaced (warning + null in the
    output); a malformed bin raises, killing the export run."""
    if not resolver.has('dim_returns.bin'):
        print('  Warning: dim_returns.bin not found — no ED curves exported',
              file=sys.stderr)
        return

    flavor = detect_dataset_flavor(powers_data)
    boost_map, attrib_map = _CURVE_ENUMS_BY_FLAVOR[flavor]

    print('Parsing dim_returns.bin...', flush=True)
    dim_sets = parse_dim_returns(resolver.read('dim_returns.bin'),
                                 boost_type=boost_map, attrib_name=attrib_map)

    effect_tables: dict[str, list[float] | None] = {}
    for key, bin_name in (('above', 'boost_effect_above.bin'),
                          ('below', 'boost_effect_below.bin'),
                          ('boosters', 'boost_effect_boosters.bin')):
        if resolver.has(bin_name):
            effect_tables[key] = parse_boost_effect(resolver.read(bin_name))
        else:
            print(f'  Warning: {bin_name} not found', file=sys.stderr)
            effect_tables[key] = None

    output_dir.mkdir(parents=True, exist_ok=True)
    out_path = output_dir / 'enhancement_curves.json'
    with open(out_path, 'w') as f:
        json.dump({
            'source': 'dim_returns.bin + boost_effect_*.bin',
            'dataset_flavor': flavor,
            'dim_returns': [asdict(s) for s in dim_sets],
            'boost_effectiveness': effect_tables,
        }, f, indent=2)
    print(f'  {len(dim_sets)} ED curve sets -> {out_path}', flush=True)


def _export_leveling_schedule(resolver: BinResolver, output_dir: Path) -> None:
    """Write leveling_schedule.json (schedules.bin: power picks, slot grants,
    pool/epic unlocks, inspiration slots per level) and exemplar_handicaps.json
    (the exemplar magnitude clamp curves). Absence of a bin is surfaced as a
    warning; a malformed bin raises, killing the export run."""
    output_dir.mkdir(parents=True, exist_ok=True)

    if resolver.has('schedules.bin'):
        print('Parsing schedules.bin...', flush=True)
        schedule = parse_schedules(resolver.read('schedules.bin'))
        out_path = output_dir / 'leveling_schedule.json'
        with open(out_path, 'w') as f:
            json.dump({
                'source': 'schedules.bin',
                'schedule': asdict(schedule),
            }, f, indent=2)
        print(f'  {len(schedule.power)} power-pick levels, '
              f'{len(schedule.assignable_boost)} slot grants -> {out_path}',
              flush=True)
    else:
        print('  Warning: schedules.bin not found — no leveling schedule '
              'exported', file=sys.stderr)

    if resolver.has('exemplar_handicaps.bin'):
        print('Parsing exemplar_handicaps.bin...', flush=True)
        curves = parse_exemplar_handicaps(
            resolver.read('exemplar_handicaps.bin'))
        out_path = output_dir / 'exemplar_handicaps.json'
        with open(out_path, 'w') as f:
            json.dump({
                'source': 'exemplar_handicaps.bin',
                'curves': asdict(curves),
            }, f, indent=2)
        print(f'  {len(curves.limits)}-level exemplar clamp curves '
              f'-> {out_path}', flush=True)
    else:
        print('  Warning: exemplar_handicaps.bin not found — no exemplar '
              'clamp curves exported', file=sys.stderr)


def _export_boostsets(boost_sets, output_dir: Path) -> None:
    """Write boostsets.json: the full IO-set records from boostsets.bin —
    name/rarity/category/allowed_powers plus the trailing block (piece
    variant names, set-bonus tiers, level range) that the ad-hoc
    `_dump_boostsets.py` script never serialized. Supersedes that script;
    every dataset gets one now, not just Rebirth."""
    output_dir.mkdir(parents=True, exist_ok=True)
    out_path = output_dir / 'boostsets.json'
    with open(out_path, 'w') as f:
        json.dump([asdict(s) for s in boost_sets], f, indent=2)
    print(f'  {len(boost_sets)} IO sets -> {out_path}', flush=True)


def _write_power_tree(powers, ps_records, ps_available, msgs, set_cats_index,
                      mode_table, stack_key_table, output_dir: Path,
                      dataset_flavor='homecoming'):
    """Group powers by category/powerset and write category/powerset/power.json
    trees plus a per-powerset index.json. Shared by the main PLAYER_CATEGORIES
    pass and the boost-piece pass in main() — the on-disk format for a power
    doesn't depend on which category set filtered it in. Returns
    (total_files, grouped) so the caller can log counts or stamp a manifest."""
    grouped: dict[str, dict[str, list]] = {}
    for pw in powers:
        grouped.setdefault(pw.category, {}).setdefault(pw.powerset, []).append(pw)

    # The game engine matches power/powerset names case-insensitively, and the
    # binaries exploit that: powers.bin and powersets.bin legitimately disagree
    # on case for the same power (Rebirth `Art_of_War` vs `Art_Of_War`, its
    # `Hellfire_Assault` set vs the record's `HellFire_Assault`, Thunderspy
    # `Tough_Hide` vs `Tough_hide`). Every lookup between the two files must
    # therefore fold case, or real available levels silently default to 0.
    availability_misses = 0

    total_files = 0
    for cat in sorted(grouped):
        for ps in sorted(grouped[cat]):
            # Both halves come straight out of powers.bin and have never
            # been looked at before becoming a directory name (F78).
            ps_dir = (output_dir
                      / safe_path_component(cat.lower(), 'power category')
                      / safe_path_component(ps.lower(), 'powerset'))
            ps_dir.mkdir(parents=True, exist_ok=True)

            powers_in_set = grouped[cat][ps]

            # Write index.json for the powerset
            # Find matching powerset record
            ps_key = f"{cat}.{ps}"
            ps_rec = next(
                (r for r in ps_records if r.key.lower() == ps_key.lower()), None)

            # Sort powers by their position in the powerset's power list (game order).
            # ps_rec.powers items are full dotted names (Cat.Powerset.Power), so key
            # the order map and the sort lookup on pw.full_name — earlier code keyed
            # on pw.power_name (leaf), which never matched, leaving powers_in_set in
            # powers.bin natural (alphabetical) order and breaking same-level ties
            # like Single_Shot/Charged_Shot in HC blast sets.
            if ps_rec and ps_rec.powers:
                ps_order = {name.lower(): i for i, name in enumerate(ps_rec.powers)}
                powers_in_set.sort(
                    key=lambda pw: ps_order.get(pw.full_name.lower(), 999))

            index_data = {
                'key': ps_key,
                'display_name': ps_rec.display_name if ps_rec else ps,
                'help': ps_rec.help if ps_rec else '',
                'short_help': ps_rec.short_help if ps_rec else '',
                'icon': (ps_rec.icon.lower().replace('.tga', '.png') if ps_rec and ps_rec.icon else ''),
                # Emit full dotted names ("Cat.Powerset.Power") so downstream
                # consumers (e.g. the planner converter) can use suffix-match
                # against the per-power short name.
                'powers': [pw.full_name for pw in powers_in_set],
                'available_level': [ps_available.get(pw.full_name.lower(), 0)
                                    for pw in powers_in_set],
                # Parallel arrays of per-power display info — convenience for
                # consumers that don't want to load every power file just to
                # build a slot index (e.g. src/data/incarnates.ts).
                'power_display_names': [pw.display_name for pw in powers_in_set],
                'power_short_helps': [pw.short_help for pw in powers_in_set],
                # The SET-level gates. A power's own `requires` says whether the
                # game would sell it; these say whether the SET may be taken at
                # all — how "only one Specialized power pool" and the VEAT branch
                # choices are stated. Emitted unconditionally (empty where the set
                # states none) so a consumer can tell "no rule" from "not read".
                'buy_requires': ps_rec.buy_requires if ps_rec else [],
                'buy_requires_failed': ps_rec.buy_requires_failed if ps_rec else '',
                'specialize_at': ps_rec.specialize_at if ps_rec else 0,
                'specialize_requires': ps_rec.specialize_requires if ps_rec else [],
            }

            if msgs and ps_rec:
                index_data['display_name'] = msgs.resolve(index_data['display_name'])
                index_data['help'] = msgs.resolve(index_data['help'])
                index_data['short_help'] = msgs.resolve(index_data['short_help'])

            with open(ps_dir / 'index.json', 'w') as f:
                json.dump(index_data, f, indent=2)
            total_files += 1

            # Write individual power files
            for pw in powers_in_set:
                pw_dict = power_to_dict(pw, msgs, set_cats_index=set_cats_index,
                                        mode_table=mode_table,
                                        stack_key_table=stack_key_table,
                                        dataset_flavor=dataset_flavor)
                if pw.full_name.lower() not in ps_available:
                    availability_misses += 1
                pw_dict['available_level'] = ps_available.get(pw.full_name.lower(), 0)
                pw_dict['powerset'] = ps_key

                # Power names containing characters invalid in Windows
                # filenames (`:`, `/`, `\`, etc.) need sanitizing.
                # `Combat_Training:_Defensive` → `combat_training_defensive.json`.
                # The convert script's index.json lookup is by full power
                # name string, so the filename mismatch is benign there;
                # this just lets the file actually write.
                safe_name = re.sub(r'[<>:"/\\|?*]+', '_', pw.power_name).lower()
                # Collapse runs of underscores from the substitution.
                safe_name = re.sub(r'_+', '_', safe_name).strip('_')
                # The substitution above covers the Windows-reserved set;
                # this also catches controls, device names and traversal,
                # and is the one boundary that refuses rather than rewrites.
                fname = safe_path_component(safe_name + '.json',
                                            'power filename',
                                            permit_edge_dots=True)
                with open(ps_dir / fname, 'w') as f:
                    json.dump(pw_dict, f, indent=2)
                total_files += 1

    if availability_misses:
        print(f"  Warning: {availability_misses} exported power(s) appear in no "
              f"powerset record's available table (even case-folded) — "
              f"available_level defaulted to 0 (see GAME-DATA-PRINCIPLES §5)",
              file=sys.stderr)

    return total_files, grouped


def main():
    ap = argparse.ArgumentParser(description='Export player power data as structured JSON')
    add_source_arguments(ap)
    ap.add_argument('--output-dir', default=None,
                    help='Output directory for JSON files (default: ./exported_powers/<assets-dir-name>, e.g. ./exported_powers/live or ./exported_powers/experimental)')
    ap.add_argument('--categories', nargs='*',
                    help='Specific categories to export (default: all player categories)')
    args = ap.parse_args()

    assets_dir = resolve_export_source(args)

    if args.output_dir is None:
        source_name = Path(assets_dir).name or 'export'
        output_dir = Path('./exported_powers') / source_name
    else:
        output_dir = Path(args.output_dir)
    categories = set(args.categories) if args.categories else PLAYER_CATEGORIES

    resolver = BinResolver(assets_dir)
    print(f'Source: {resolver.source_description}', flush=True)

    # Source-aware Primalist gating: HC/Rebirth carry ORPHAN Primalist powers
    # (no powerset/powercat/class) that otherwise leak into the export via the
    # shared whitelist. Drop the Primalist categories unless this source defines
    # a Primalist class (Thunderspy). Skip when the user passed explicit
    # --categories (advanced override — respect their choice).
    if not args.categories:
        primalist_present = categories & PRIMALIST_CATEGORIES
        if primalist_present and not _source_has_primalist_class(resolver):
            categories = categories - PRIMALIST_CATEGORIES
            print(f'  No Primalist class in source — excluding orphan Primalist '
                  f'categories: {sorted(primalist_present)}', flush=True)

    # Load message table
    msgs = None
    if resolver.has('clientmessages-en.bin'):
        print('Loading message table...', flush=True)
        msgs = load_messages(resolver.read('clientmessages-en.bin'))
        print(f'  {len(msgs)} messages loaded.', flush=True)

    # Load the mode-name table (attrib_names.bin) used to resolve Set_Mode
    # template magnitudes (mode indices) to mode names. Per-server; absent on
    # some sources (Rebirth/Thunderspy piggs that don't ship attrib_names.bin),
    # in which case Set_Mode templates simply carry no `mode` field.
    mode_table = {}
    stack_key_table = {}
    if resolver.has('attrib_names.bin'):
        print('Loading mode table (attrib_names.bin)...', flush=True)
        _attrib_names_data = resolver.read('attrib_names.bin')
        mode_table = parse_mode_table(_attrib_names_data)
        print(f'  {len(mode_table)} modes loaded.', flush=True)
        # StackKeys registry from the same file — resolves each template's
        # stack_key ID to its suppress-group name (TravelBuff, StealthToggle…).
        stack_key_table = parse_stack_key_table(_attrib_names_data)
        print(f'  {len(stack_key_table)} stack keys loaded.', flush=True)

    # Parse powers
    print('Parsing powers.bin...', flush=True)
    player_classes = _player_class_names(resolver)
    print(f'  {len(player_classes)} player archetypes for gate resolution.', flush=True)
    powers_data = resolver.read('powers.bin')
    all_powers = parse_powers(powers_data, player_classes=player_classes)
    dataset_flavor = detect_dataset_flavor(powers_data)
    print(f'  {len(all_powers)} powers loaded.', flush=True)

    # Resolve P-hashes
    if msgs:
        for pw in all_powers:
            pw.display_name = msgs.resolve(pw.display_name)
            pw.display_help = msgs.resolve(pw.display_help)
            pw.short_help = msgs.resolve(pw.short_help)

    # Parse powersets for available_level info
    ps_records = []
    ps_available = {}  # full_name -> available_level
    if resolver.has('powersets.bin'):
        print('Parsing powersets.bin...', flush=True)
        ps_records = parse_powersets(resolver.read('powersets.bin'))
        for ps in ps_records:
            # ps.powers contains full dotted names (Cat.Powerset.Power),
            # not just short names. Keys are folded to lower case because
            # powers.bin and powersets.bin disagree on case for some powers
            # (see _write_power_tree) and the game matches case-insensitively.
            for pw_name, avail in zip(ps.powers, ps.available):
                ps_available[pw_name.lower()] = avail
        print(f'  {len(ps_records)} powersets loaded.', flush=True)

    # Parse boostsets.bin — the authoritative per-IO-set list of powers each
    # set can slot into. Reversed into a power→categories index so every
    # exported power gets its `allowed_set_categories` directly from the
    # game's data rather than inference.
    set_cats_index: dict[str, list[str]] = {}
    if resolver.has('boostsets.bin'):
        print('Parsing boostsets.bin...', flush=True)
        boost_sets = parse_boostsets(resolver.read('boostsets.bin'))
        # Resolve P-hashes before the index, not after: the index is keyed on
        # `group_name`, so building it first would file every power under raw
        # message-store keys ("P2611858036" for "Melee Damage").
        if msgs:
            for s in boost_sets:
                s.display_name = msgs.resolve(s.display_name)
                s.group_name = msgs.resolve(s.group_name)
        set_cats_index = build_power_category_index(boost_sets)
        print(f'  {len(boost_sets)} IO sets loaded, '
              f'{len(set_cats_index)} powers indexed.', flush=True)
        _export_boostsets(boost_sets, output_dir)

    _export_enhancement_curves(resolver, powers_data, output_dir)
    _export_leveling_schedule(resolver, output_dir)

    # Filter to player categories
    player_powers = [pw for pw in all_powers if pw.category in categories]
    print(f'\nFiltered to {len(player_powers)} player powers from {len(categories)} categories.')

    # Referenced-target inclusion: pull in the Temporary_Powers powers that a
    # player power GRANTS via a `Grant_Power` template. These host effects the
    # granting power delivers but does not carry inline — most notably the
    # damage-over-time procs a few passives/toggles grant (Molten Embrace's Fire
    # DoT lives in Temporary_Powers.Temporary_Powers.Molten_Embrace_Proc, NOT in
    # Molten Embrace itself). Temporary_Powers as a whole is 1200+ travel/
    # accolade/event temps we don't want; only the powers actually referenced by
    # a player power's grant are relevant, so include exactly those (mirrors how
    # the Pets/*_Aux categories host Execute_Power/redirect damage targets, but
    # scoped to references instead of the whole category). The convert-time
    # resolver reads these by their dotted name; convert-all-powersets never
    # turns Temporary_Powers into a powerset (not in its category map), so these
    # are pure resolver data.
    if not args.categories:
        grant_targets = _collect_referenced_targets(
            player_powers, GRANT_ATTRIBS, prefix='temporary_powers.')
        by_full = {pw.full_name.lower(): pw for pw in all_powers}
        already = {pw.full_name.lower() for pw in player_powers}
        added = []
        for tgt in sorted(grant_targets):
            if tgt in already:
                continue
            pw = by_full.get(tgt)
            if pw is not None:
                already.add(tgt)
                player_powers.append(pw)
                added.append(pw)
        if added:
            print(f'  +{len(added)} referenced Grant_Power targets '
                  f'(Temporary_Powers grant hosts).', flush=True)

        # The other referenced edge: the child a wrapper power delegates its work
        # to. Most children sit in the Pets/*_Aux categories the filter already
        # keeps, but a few live in Temporary_Powers or Mission_Maker_Pets and so
        # reached no export at all — leaving the wrapper naming a child nothing
        # downstream could resolve (DATA-GAP HC-4). No category prefix here: a
        # wrapper's child IS that power's work, wherever the fork files it. Walked
        # to a fixpoint because a child may delegate in turn; a name the binary
        # does not define is surfaced rather than skipped.
        wrapper_added = []
        dangling: set[str] = set()
        frontier = list(player_powers)
        while frontier:
            targets = _collect_referenced_targets(frontier, WRAPPER_ATTRIBS)
            frontier = []
            for tgt in sorted(targets):
                if tgt in already:
                    continue
                already.add(tgt)
                pw = by_full.get(tgt)
                if pw is None:
                    dangling.add(tgt)
                    continue
                player_powers.append(pw)
                wrapper_added.append(pw)
                frontier.append(pw)
        if wrapper_added:
            print(f'  +{len(wrapper_added)} referenced wrapper targets '
                  f'(the powers a delegating power hands its work to).', flush=True)
        if dangling:
            print(f'  Warning: {len(dangling)} wrapper target(s) name a power this '
                  f'source does not define: {sorted(dangling)[:5]}', file=sys.stderr)

        # Whole-powerset inclusion for the real player-facing Temporary_Powers
        # powersets the category filter drops (see INCLUDED_TEMPORARY_POWERSETS).
        # Accolades group under temporary_powers/accolades/ as ordinary powers.
        accolade_added = [
            pw for pw in all_powers
            if pw.category == 'Temporary_Powers'
            and pw.powerset in INCLUDED_TEMPORARY_POWERSETS
            and pw.full_name.lower() not in already
        ]
        for pw in accolade_added:
            player_powers.append(pw)
            already.add(pw.full_name.lower())
        if accolade_added:
            print(f'  +{len(accolade_added)} Temporary_Powers powerset members '
                  f'({sorted(INCLUDED_TEMPORARY_POWERSETS)}).', flush=True)

    total_files, grouped = _write_power_tree(
        player_powers, ps_records, ps_available, msgs, set_cats_index,
        mode_table, stack_key_table, output_dir,
        dataset_flavor=dataset_flavor)

    print(f'\nExported {total_files} files to {output_dir}/')
    print(f'  Categories: {len(grouped)}')
    print(f'  Powersets: {sum(len(ps) for ps in grouped.values())}')
    print(f'  Powers: {len(player_powers)}')

    # Stamp the export-staleness manifest. This records the fingerprint of the
    # powers-exporter SOURCE that produced this tree, so a later parser change
    # that ships without a matching re-export is caught by the JS/vitest guard
    # (src/data/export-staleness.test.ts) — CI can't re-run this Python export
    # (no .pigg) so the fingerprint is the only cross-check. See
    # bin_crawler/_export_fingerprint.py. Skip when the user exported a category
    # SUBSET (`--categories`): a partial tree must not claim whole-dataset
    # currency, and stamping the current fingerprint over stale sibling
    # categories would defeat the guard.
    if not args.categories:
        manifest = {
            'schema': 'bin-crawler-export-manifest/2',
            'note': ('parser_fingerprint is the sha256 of the powers exporter '
                     '(bin_crawler/parser/**/*.py + export_powers.py) at export '
                     'time. If it disagrees with the current committed exporter '
                     'source, THIS tree is stale — re-run export_powers for this '
                     'dataset and commit. Guarded by '
                     'src/data/export-staleness.test.ts. `source` names the '
                     'assets shard the bytes were read from; guarded by '
                     'src/data/export-provenance.test.ts.'),
            'parser_fingerprint': parser_fingerprint(),
            'source': resolver.provenance(),
            'categories': len(grouped),
            'power_files': total_files,
        }
        with open(output_dir / '_export_manifest.json', 'w') as f:
            json.dump(manifest, f, indent=2)
            f.write('\n')
        print(f'  Manifest: parser_fingerprint={manifest["parser_fingerprint"][:12]}…')
    else:
        print('  Manifest: SKIPPED (partial --categories export; not stamping '
              'whole-dataset currency)')

    # Boost-piece power templates (Boosts.*/Set_Bonus.*) — written as a
    # separate tree, not folded into `categories`/`grouped` above, so the
    # manifest counts and player-power semantics (Grant_Power target
    # collection, Primalist gating) stay exactly what they were. Skipped
    # under an explicit --categories override for the same reason the
    # Primalist gating and manifest stamping are: it's an advanced override,
    # respect it as given.
    if not args.categories:
        boost_piece_powers = [pw for pw in all_powers if pw.category in BOOST_PIECE_CATEGORIES]
        if boost_piece_powers:
            bp_total, bp_grouped = _write_power_tree(
                boost_piece_powers, ps_records, ps_available, msgs,
                set_cats_index, mode_table, stack_key_table, output_dir,
                dataset_flavor=dataset_flavor)
            print(f'\nExported {bp_total} boost-piece files '
                  f'({len(boost_piece_powers)} powers, {len(bp_grouped)} categories) '
                  f'to {output_dir}/boosts/ + {output_dir}/set_bonus/')


if __name__ == '__main__':
    main()
