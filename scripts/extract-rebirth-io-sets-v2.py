"""Extract a full IO set library from the committed bin-crawler exports —
Homecoming, Rebirth, and Thunderspy.

Reads boostsets.json + the boosts/*, set_bonus/* power-template trees that
`export_powers.py` already parsed, resolved, and committed under
exported_powers/ — no live .pigg reads — and emits
src/data/datasets/<dataset>/io-sets-raw.ts. Supersedes the retired
convert-io-sets.js (HC, hand-data) and extract-rebirth-io-sets.cjs (Rebirth).
Reproducible from the committed tree alone: re-running this script requires
nothing beyond what's already in git (previously it needed the live game
install).

Pipeline:
  1. Load boostsets.json → set metadata + BoostLists + Bonuses + levels
  2. For each piece (Boost power), look up its committed boosts/* template →
     derive aspects from effect-template attribs (damage types collapse to
     "Damage", etc.) and recover the effective aspect count from the
     enhancement scale.
  3. For each bonus (Set_Bonus power), look up its committed set_bonus/*
     template → derive effects[] entries: planner-canonical stat key + value
     (scale × per-attrib multiplier)
  4. Apply the per-dataset override pass (Rebirth: reuse HC for shared sets;
     HC: targeted hand overrides for what the binary can't reproduce)
  5. Emit TypeScript io-sets-raw

Display strings (set names, boost-piece names) arrive already resolved —
export_powers.py resolves every P-hash against clientmessages-en.bin before
writing boostsets.json/boosts/*/set_bonus/*, so this script never touches a
message table itself.

Usage:
    py -3 scripts/extract-rebirth-io-sets-v2.py --dataset homecoming
    py -3 scripts/extract-rebirth-io-sets-v2.py --dataset rebirth   # default
"""
from __future__ import annotations

import json
import re
import subprocess
import sys
from dataclasses import dataclass, field
from functools import lru_cache
from pathlib import Path

# Allow running from the project root
PROJECT_ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(PROJECT_ROOT / 'tools' / 'bin-crawler'))

from bin_crawler.parser._boostsets import BoostSetRecord, BoostListEntry, BoostBonusEntry, _resolve_category

# Each dataset's committed export tree (bin_crawler/export_powers.py output).
# 'homecoming' writes to the export root itself (no per-dataset subdir);
# rebirth/thunderspy get their own subdirs — matches export_powers.py's
# existing --output-dir convention for these three datasets.
EXPORT_DIRS = {
    'homecoming': PROJECT_ROOT / 'exported_powers',
    'rebirth':    PROJECT_ROOT / 'exported_powers' / 'rebirth',
    'thunderspy': PROJECT_ROOT / 'exported_powers' / 'thunderspy',
    'brainstorm': PROJECT_ROOT / 'exported_powers' / 'brainstorm',
}
OUTPUT_PATH = PROJECT_ROOT / 'src' / 'data' / 'datasets' / 'rebirth' / 'io-sets-raw.ts'
HC_IO_SETS_PATH = PROJECT_ROOT / 'src' / 'data' / 'datasets' / 'homecoming' / 'io-sets-raw.ts'
THUNDERSPY_IO_SETS_PATH = PROJECT_ROOT / 'src' / 'data' / 'datasets' / 'thunderspy' / 'io-sets-raw.ts'
BRAINSTORM_IO_SETS_PATH = PROJECT_ROOT / 'src' / 'data' / 'datasets' / 'brainstorm' / 'io-sets-raw.ts'


def _load_hc_sets() -> dict[str, dict]:
    """Build a setId -> full-set-entry map from HC's io-sets-raw.ts.

    HC's entry carries complete aspect lists and bonus tiers, where the Rebirth and
    Thunderspy binary extractions lose the Accuracy aspect on many pieces and resolve
    fewer tiers. For a set that exists on both servers, that entry is reused; a
    fork-only set falls back to its own binary extraction. Piece NAMES are not
    borrowed — each fork names its pieces from its own boost powers.

    Note this file is the script's own previous output for the Homecoming dataset, so
    the Homecoming paths that read `hc_sets` are self-referential: a change to how
    they are built shows up only once, on the run that rewrites the file.

    Returns a dict mapping set_id -> parsed JSON object (the full set body).
    """
    if not HC_IO_SETS_PATH.exists():
        return {}
    text = HC_IO_SETS_PATH.read_text(encoding='utf-8')
    sets: dict[str, dict] = {}
    set_pattern = re.compile(r'^  "([a-z0-9_]+)": (\{)', re.MULTILINE)
    for m in set_pattern.finditer(text):
        set_id = m.group(1)
        # Find the matching closing brace for this top-level set object.
        depth = 0
        i = m.start(2)
        end = -1
        in_string = False
        escape = False
        while i < len(text):
            c = text[i]
            if escape:
                escape = False
            elif c == '\\' and in_string:
                escape = True
            elif c == '"':
                in_string = not in_string
            elif not in_string:
                if c == '{':
                    depth += 1
                elif c == '}':
                    depth -= 1
                    if depth == 0:
                        end = i + 1
                        break
            i += 1
        if end < 0:
            continue
        body = text[m.start(2):end]
        # HC's io-sets-raw.ts is TS object syntax, not strict JSON: it allows
        # trailing commas. Strip them so json.loads can parse.
        body_clean = re.sub(r',(\s*[\]\}])', r'\1', body)
        try:
            sets[set_id] = json.loads(body_clean)
        except json.JSONDecodeError:
            continue
    return sets


# ---------------------------------------------------------------------
# Committed-export loaders (boostsets.json + boosts/*, set_bonus/*).
#
# export_powers.py already parses boostsets.bin/powers.bin once per dataset,
# resolves display strings, and commits the result under exported_powers/.
# These loaders reconstruct the same shapes build_sets() below expects
# (BoostSetRecord and a full_name-keyed power index) from that committed
# JSON instead of re-parsing the binary.
# ---------------------------------------------------------------------

def _load_boostsets(export_dir: Path) -> list[BoostSetRecord]:
    """Reconstruct BoostSetRecord objects from the committed boostsets.json.

    boostsets.json is `asdict()` of the exact same BoostSetRecord list
    parse_boostsets() used to return — a lossless round-trip through JSON,
    not a re-derivation — so this is field-for-field identical to a live
    binary parse (display_name/group_name already message-resolved, see
    export_powers.py's boost_sets resolution pass).
    """
    data = json.loads((export_dir / 'boostsets.json').read_text(encoding='utf-8'))
    return [
        BoostSetRecord(
            name=s['name'],
            display_name=s['display_name'],
            group_name=s['group_name'],
            conversion_groups=s['conversion_groups'],
            rarity=s['rarity'],
            category=s['category'],
            allowed_powers=s['allowed_powers'],
            boostlists=[BoostListEntry(boosts=bl['boosts']) for bl in s['boostlists']],
            bonuses=[
                BoostBonusEntry(min_boosts=b['min_boosts'], max_boosts=b['max_boosts'],
                                 requires=b['requires'], auto_powers=b['auto_powers'])
                for b in s['bonuses']
            ],
            min_level=s['min_level'],
            max_level=s['max_level'],
            store_product=s['store_product'],
        )
        for s in data
    ]


def _tier_gate(bonus: BoostBonusEntry) -> str | None:
    """What a bonus tier's `Requires` gates it on: `'base'`, `'pvp'`, or None to skip.

    A tier states its condition in the same RPN vocabulary a power's gate uses, and the
    corpus states exactly four shapes across the three forks (258 gated tiers of 3,238):

    - no Requires — applies on slotted-piece count alone.
    - `isPVPMap?` — the PvP-only tier. A set states a second, different bonus at each
      piece count that applies only on a PvP map, granting a `PVP_Set_Bonus` power.
    - a `PowerBoostsSlotted>` test — the global a single unique piece grants by itself,
      encoded as a tier whose condition is "that piece is slotted". SKIPPED, and not for
      want of an evaluator: the binary ALSO encodes that global on the slottable piece,
      proc-data carries it from there, and the calc grants it when the piece is slotted.
      Emitting the tier too is the pseudo-tier double-count MEZRES-1 removed.
    - a `TFChallenge.*` disjunction ANDed with `isPVPMap? !` — Rebirth's challenge-mode
      bonuses, live only under a TF's challenge settings and off a PvP map. Rebirth-only,
      granting `Challenge_Set_Bonus` powers. SKIPPED: no surface models challenge state.

    An unrecognized shape raises. A tier the game gates and this catalog emits ungated
    hands every build a bonus it has not earned, and one silently dropped is a bonus the
    set grants that the planner never shows — so a fifth shape has to be read, not guessed.
    """
    requires = bonus.requires
    if not requires:
        return 'base'
    if requires == ['isPVPMap?']:
        return 'pvp'
    if 'PowerBoostsSlotted>' in requires:
        return None
    if any(token.startswith('TFChallenge.') for token in requires):
        return None
    raise ValueError(
        f'unrecognized set-bonus tier gate {requires!r} at {bonus.min_boosts}pc — '
        'read what it gates on before emitting or skipping it (BONUS-REQ-1)'
    )


@dataclass
class _JsonTemplate:
    """The subset of an exported power.json effect template this script
    reads (see power_to_dict()'s `effects[].templates[]` in export_powers.py)."""
    attribs: list[str]
    aspect: str
    scale: float
    table: str


@dataclass
class _JsonEffectGroup:
    chance: float
    ppm: float
    templates: list[_JsonTemplate] = field(default_factory=list)


@dataclass
class _JsonPower:
    """The subset of a power.json this script reads — a Boosts.*/Set_Bonus.*
    power template, loaded from exported_powers/{boosts,set_bonus}/**."""
    full_name: str
    display_name: str
    slot_requires: str
    effects: list[_JsonEffectGroup] = field(default_factory=list)


def _load_power_json(path: Path) -> _JsonPower:
    d = json.loads(path.read_text(encoding='utf-8'))
    effects = [
        _JsonEffectGroup(
            chance=eg.get('chance', 1.0),
            ppm=eg.get('ppm', 0.0),
            templates=[
                _JsonTemplate(
                    attribs=t.get('attribs') or [],
                    aspect=t.get('aspect') or '',
                    scale=t.get('scale', 0.0),
                    table=t.get('table') or '',
                )
                for t in eg.get('templates', [])
            ],
        )
        for eg in d.get('effects', [])
    ]
    return _JsonPower(
        full_name=d['full_name'],
        display_name=d.get('display_name', ''),
        slot_requires=d.get('slot_requires', ''),
        effects=effects,
    )


def _load_boost_piece_powers(export_dir: Path) -> dict[str, _JsonPower]:
    """Index every committed Boosts.*/Set_Bonus.* power template by
    full_name — the same lookup a live powers.bin parse's power_index gave
    build_sets(), scoped to just the two boost-piece categories (this
    script never needs a player power)."""
    power_index: dict[str, _JsonPower] = {}
    for tree_name in ('boosts', 'set_bonus'):
        tree_dir = export_dir / tree_name
        if not tree_dir.exists():
            continue
        for json_path in tree_dir.glob('*/*.json'):
            if json_path.name == 'index.json':
                continue
            pw = _load_power_json(json_path)
            power_index[pw.full_name] = pw
    return power_index


# ---------------------------------------------------------------------
# Curated icon overrides for Rebirth-only sets.
# Files were copied from MRB into public/img/Enhancements/{Archetype,Event,IO Sets}.
# These names override the auto-generated `s{set_id}.png` fallback.
# ---------------------------------------------------------------------
ICON_OVERRIDES = {
    # Guardian ATOs (Archetype/)
    'guardians_gift':                  "AO_Guardian's_Gift.png",
    'superior_guardians_gift':         "SAO_Guardian's_Gift.png",
    'absolute_resolution':             'AO_Absolute_Resolution.png',
    'superior_absolute_resolution':    'SAO_Absolute_Resolution.png',
    # Halloween event sets (Event/)
    'the_haunting':                    'EO_The_Haunting.png',
    'superior_the_haunting':           'SEO_The_Haunting.png',
    'endless_nightmare':               'EO_Endless_Nightmare.png',
    'superior_endless_nightmare':      'SEO_Endless_Nightmare.png',
    'vampires_bite':                   'EO_Vampires_Bite.png',
    'superior_vampires_bite':          'SEO_Vampires_Bite.png',
    'witchcraft':                      'EO_Witchcraft.png',
    'superior_witchcraft':             'SEO_Witchcraft.png',
    'return_from_the_grave':           'EO_Return_From_The_Grave.png',
    'superior_return_from_the_grave':  'SEO_Return_From_The_Grave.png',
    # Winter event sets (Event/)
    'winter_storm':                    'EO_Winter_Storm.png',
    'superior_winter_storm':           'SEO_Winter_Storm.png',
    # Winter's Gift is the one pair where the two halves were swapped onto one
    # entry. `SEO_Winters_Gift.png` is byte-identical to MidsReborn's Rebirth
    # asset `Winters_Gift_Superior_Attuned.png`, so it is the SUPERIOR artwork,
    # and it was keyed to the base set on all three forks. The base set then had
    # the superior icon, `superior_winters_gift` (Rebirth-only, so no HC set_id
    # to inherit from) fell to the `s{set_id}.png` fallback and 404'd as
    # `ssuperior_winters_gift.png`, and `IO Sets/WintersGift.png` sat in the
    # asset library referenced by no set at all. That orphan is byte-identical to
    # Mids' shared `Assets/Enhancements/WintersGift.png`, which is the base.
    # Unlike the pairs above, the base half is not an `EO_` file: Winter's Gift
    # is category `rare` on every fork, not `event`, and only Rebirth mints a
    # superior version of it.
    'winters_gift':                    'WintersGift.png',
    'superior_winters_gift':           'SEO_Winters_Gift.png',
    # Misc Rebirth-only (IO Sets/)
    'forced_indoctrination':           'ForcedIndoctrination.png',
    'imperial_might':                  'ImperialMight.png',
    'inexhaustibility':                'Inexhaustibility.png',
    'libertys_belt':                   'Libertys_Belt.png',
    'rolling_barrage':                 'Rolling_Barrage.png',
    'synapses_agility':                'PowerOfSynapse.png',
    # HC EndMod set whose icon was never in the curated set — without this it
    # fell back to a stale value (sEfficiencyAdaptor.png), making it visually
    # indistinguishable from Efficacy Adaptor. Icon extracted from the HC
    # texture_gui.pigg (e_icon_preemptive_optimization.texture) and composited
    # into public/img/Enhancements/IO Sets/sPreemptiveOptimization.png.
    'preemptive_optimization':         'sPreemptiveOptimization.png',
    # Thunderspy-only sets rebuilt by _apply_thunderspy_overrides. They aren't on
    # HC, so the set_id icon match (hc_sets[set_id].icon) never fires and without
    # an override they fall to the bogus `s{set_id}.png` fallback — a filename
    # never in our curated asset library, so EnhancementIcon shows Unknown.png
    # (reported: "Subaluwa missing icons"). These are DISTINCT tspy sets with
    # their OWN in-game icons (NOT Overwhelming Force — that's a separate set that
    # tspy also ships, injected below). Symbols were extracted from tspy's
    # gui.pigg (texture_library/gui/icons/enhancements/e_icon_{subaluwa,
    # primalistsnature}.texture) and composited on the red E_POG_DAMAGE disc per
    # public/img/Enhancements/Components/README.md (Subaluwa = universal damage;
    # Primalist ATOs are damage attack sets — all ATOs use the red POG). Superior
    # shares the standard base composite; the planner adds the superior frame.
    'kb':                              'UD_Subaluwa.png',
    'primalists_nature':               'AO_Primalists_Nature.png',
    'superior_primalists_nature':      'SAO_Primalists_Nature.png',
}

# ---------------------------------------------------------------------
# Curated aspects for the Rebirth-only ATO sets, whose binary extraction drops
# the Accuracy aspect on every piece that has one. Fields only: each piece keeps
# the name its own boost power gives it.
# ---------------------------------------------------------------------
def _ato_pieces() -> list[dict]:
    return [
        {'num': 1, 'aspects': ['Accuracy', 'Damage'], 'proc': False, 'unique': True},
        {'num': 2, 'aspects': ['Damage', 'Recharge'], 'proc': False, 'unique': True},
        {'num': 3, 'aspects': ['Accuracy', 'Damage', 'Recharge'], 'proc': False, 'unique': True},
        {'num': 4, 'aspects': ['Damage', 'Endurance', 'Recharge'], 'proc': False, 'unique': True},
        {'num': 5, 'aspects': ['Accuracy', 'Damage', 'Endurance', 'Recharge'],
         'proc': False, 'unique': True},
        {'num': 6, 'aspects': ['Recharge'], 'proc': True, 'unique': True},
    ]

PIECE_OVERRIDES = {
    'guardians_gift':               _ato_pieces(),
    'superior_guardians_gift':      _ato_pieces(),
    'absolute_resolution':          _ato_pieces(),
    'superior_absolute_resolution': _ato_pieces(),
}

# Per-piece aspect overrides for cases where the binary template carries
# attribs that don't match the in-game piece per the Rebirth wiki / in-game
# enhancement window. Each entry replaces the parsed aspect list outright
# (after `_collapse_aspects` runs) — the piece name is regenerated from the
# overridden list.
#
# Forced Indoctrination piece 5: binary template includes
# `EnduranceDiscount/Strength` alongside Acc/Dmg/Rech/mez, but the wiki and
# in-game tooltip describe this as Acc/Dmg/Rech/Ctrl (no End Reduction).
# Tracking the upstream discrepancy via this override so the planner shows
# what the player actually slots; revisit if Rebirth changes the binary.
REBIRTH_PIECE_ASPECT_OVERRIDES: dict[str, dict[int, list[str]]] = {
    'forced_indoctrination': {
        5: ['Accuracy', 'Damage', 'Recharge', 'Mez'],
    },
}

# Patch specific fields on a Rebirth-only piece the binary can't characterize.
# Format: set_id → {piece_num: {field: value}}.
#   Inexhaustibility's single piece carries only a `Set_Mode` marker template
#   (no real attribs, no chance/ppm group), so it extracts proc=false. It is a
#   special Rest enhancement; restore the flag.
#
#   VERIFIED 2026-06-11 (verify-don't-assume): `proc=true` is right. The set's
#   effect lives in the periodic-proc power Set_Bonus.Challenge_Set_Bonus.
#   Inexhaustibility (activate_period=10, chance=0.5 → Heal 2.0 / +End 0.10 /
#   +Regen 2.0), NOT an always-on set bonus. So the set's empty `bonuses` is
#   CORRECT — emitting those values as a static bonus would over-count a +2.0
#   Regen that only procs on a 50%/10s tick. The proc itself is already captured
#   (proc-data.ts + proc-residual-effects.ts, category 'Special' like the other
#   bespoke Rebirth self-procs). Nothing is missing.
#
#   The name is no longer patched. That entry was justified by the piece's
#   display_name being an unresolvable P-hash (P3179408089); the committed export
#   now resolves it to "Inexhaustibility: Out of Combat +Hit Points/Endurance",
#   which is both readable and better than the curated "Inexhaustibility".
REBIRTH_PIECE_PATCHES: dict[str, dict[int, dict]] = {
    # Liberty's Belt F "Resistance/Global Damage Bonus": a hybrid piece — a
    # real resist enhancement PLUS an always-on +7.5% damage global. The
    # global lives in a PowerBoostsSlotted-gated auto (BOOST-4), not on the
    # boost power, so nothing on the piece itself marks it; same shape as
    # HC's hand-patched hybrids (Steadfast Resistance/Defense, LotG
    # Defense/+Recharge). The flag is what admits the piece to the engine's
    # always-on pass and adds the hidden segment to its enhancement
    # dilution (BOOST-5 step 2 adjudication).
    'libertys_belt': {
        6: {'proc': True},
    },
    # (inexhaustibility #1's flag is structural now: its boost power is a
    # pure Null × Strength marker, recognized by _collapse_aspects.)
}

# ---------------------------------------------------------------------
# Homecoming-only overrides (HC IS the source for shared sets, so the
# Rebirth shared-set reuse / Rebirth piece curation above do NOT apply).
# These cover the handful of cases the binary genuinely can't reproduce;
# the bonus/whole-set data is taken from HC's existing hand-curated
# io-sets-raw.ts (parsed by _load_hc_sets), which is correct for them.
# ---------------------------------------------------------------------

# Sets the binary skips entirely (rarity=ECUniversalDamage has its own
# multi-thousand-power pool and no planner rarity mapping) — copy the whole
# hand entry. These wide-pool universal-damage sets slot into nearly every
# attack power.
HC_WHOLESET_SETS = {'cupids_crush', 'overwhelming_force'}

# Per-piece aspect overrides for the handful of pieces the binary mis-extracts.
# Each replaces the parsed aspect list; the piece keeps the name its boost power
# gave it, which is free to describe the piece differently.
#   - hypersonic #4: a "+Fly Magnitude" special aspect alongside Fly the binary
#     template doesn't surface.
#   - blessing_of_the_zephyr / winters_gift: the all-three-movement-speeds
#     collapse mislabels these travel BUFF pieces as a Slow debuff (plus a
#     spurious Range); the curated single "Move Speed" aspect is correct.
#   - sudden_acceleration #6: the KB→Knockdown converter is a single negative-
#     scale Knockback template (correctly excluded as a non-enhancement), but
#     the curated data names the special "KnockToKnockDown" aspect.
HC_PIECE_ASPECT_OVERRIDES: dict[str, dict[int, list[str]]] = {
    'hypersonic': {
        4: ['Fly', '+Fly Magnitude'],
    },
    'blessing_of_the_zephyr': {
        1: ['Move Speed'],
        2: ['Endurance', 'Move Speed'],
    },
    'winters_gift': {
        1: ['Move Speed'],
        2: ['Endurance', 'Move Speed'],
    },
    'sudden_acceleration': {
        6: ['KnockToKnockDown'],
    },
}

# The `proc` flag for global/special pieces the binary doesn't characterize as
# procs. These carry an always-on global (Luck of the Gambler +Recharge, Steadfast
# +Def, the +Run Speed / +Perception / +Jump Height travel globals) or a
# Grant_Power proc, encoded as a Null / Grant_Power / Current / Maximum template
# that ISN'T a plain enhancement aspect — so proc detection misses it whenever the
# piece also has a real enhancement aspect. The calc gates global/proc application
# on this flag (findProcData resolves the effect by set name), so a piece without
# it drops its global from both the slot UI and the character totals.
#
# Flags only. The piece's name comes from its boost power like every other
# piece's, and the global's identity reads far better there than in anything
# assembled from templates: the game calls Luck of the Gambler #6 "Defense/
# Increased Global Recharge Speed" and Reactive Defenses #6 "Scaling Damage
# Resistance". Enhancement aspects and scale-derived totalAspects come from the
# binary unchanged.
HC_PIECE_PATCHES: dict[str, dict[int, dict]] = {
    'luck_of_the_gambler':       {6: {'proc': True}},
    'gift_of_the_ancients':      {6: {'proc': True}},
    'steadfast_protection':      {2: {'proc': True},
                                  3: {'proc': True}},
    'reactive_defenses':         {6: {'proc': True}},
    'thrust':                    {4: {'proc': True}},
    # Travel-set +Stealth globals. The stealth grant is a Create_Entity template,
    # which proc detection doesn't read as one. findProcData resolves the effect
    # by set name: Celerity → "Buff Stealth"; the others have explicit
    # "<Set>: +Stealth" PROC_DATABASE keys.
    'celerity':                  {3: {'proc': True}},
    'freebird':                  {3: {'proc': True}},
    'timespace_manipulation':    {3: {'proc': True}},
    'unbounded_leap':            {3: {'proc': True}},
    # An always-on +Run Speed global the bin tags proc:false.
    'synapses_shock':            {6: {'proc': True}},
    'warp':                      {4: {'proc': True}},
    'launch':                    {4: {'proc': True}},
    # Stupefy #6 is a Chance-for-Knockback proc, not a global. Its name matters
    # beyond display: Stupefy has two PROC_DATABASE entries, and findProcData's
    # set fallback hands back whichever sits first ("Chance for Stun") unless the
    # name picks the right one. The boost power names it "Chance for Knockback".
    'stupefy':                   {6: {'proc': True}},
    'assassins_mark':            {6: {'proc': True}},
    'superior_assassins_mark':   {6: {'proc': True}},
    'essence_transfer':          {6: {'proc': True}},
    'superior_essence_transfer': {6: {'proc': True}},
    # ATO passive-global 6th pieces — the special is an ALWAYS-ON global (not a
    # chance-proc), so the binary tags them proc:false and the global is dropped
    # from BOTH the slot UI and the character totals. (Reported via Scrapper's
    # Strike, @Redlynne 2026-06-18.) Effect values binary-sourced from
    # Set_Bonus.Global_Bonus.* — except Scrapper's Strike's +Crit, a special crit
    # mechanic with no Global_Bonus power, hand-curated in proc-data.ts
    # (std +2%/+4%, sup +3%/+6%, both confirmed in-game). findProcData resolves
    # each by set name, so the piece label is cosmetic; each set has exactly one
    # global proc-data entry so the set fallback is unambiguous.
    'scrappers_strike':                   {6: {'proc': True}},
    'superior_scrappers_strike':          {6: {'proc': True}},
    'command_of_the_mastermind':          {6: {'proc': True}},
    'superior_command_of_the_mastermind': {6: {'proc': True}},
    'kheldians_grace':                    {6: {'proc': True}},
    'superior_kheldians_grace':           {6: {'proc': True}},
    'mark_of_supremacy':                  {6: {'proc': True}},
    'superior_mark_of_supremacy':         {6: {'proc': True}},
    'spiders_bite':                       {6: {'proc': True}},
    'superior_spiders_bite':              {6: {'proc': True}},
}

# ---------------------------------------------------------------------
# Rarity → planner category
# ---------------------------------------------------------------------
EC_RARITY_TO_PLANNER = {
    'ECCommon':   'uncommon',
    'ECUncommon': 'uncommon',
    'ECRare':     'rare',
    'ECVeryRare': 'purple',
    'ECPvP':      'pvp',
    'ECPVP':      'pvp',
    'ECWinter':   'event',
    'ECSWinter':  'event',
    'ECHalloween':  'event',
    'ECSHalloween': 'event',
    'ECSummer':   'event',
    'ECATO':      'ato',
    'ECSATO':     'ato',
    'ECATO2':     'ato',
    'ECSATO2':    'ato',
    'ECUltraRare': 'purple',
    # Rebirth-specific oddballs
    'LibertysBelt':         'event',
    'ImperialMight':        'event',
    'ForcedIndoctrination': 'event',
    'ECSpeedRun':           'event',
    '':                     'event',
}

# Binary rarity tiers whose enhancement values run 25% hot: the very-rare
# (purple) tiers plus the catalyzed Superior variants. The engine twins
# (getSetRarityMultiplier / set_rarity_multiplier) carry the same vocabulary,
# pinned TS<->Rust by the enhancement fixtures.
SUPERIOR_RARITIES = {'ECVeryRare', 'ECUltraRare', 'ECSATO', 'ECSATO2', 'ECSWinter', 'ECSHalloween'}

# ---------------------------------------------------------------------
# Damage-type attribs that collapse into a single "Damage" aspect when
# all 8 are present. Boost pieces enhance every damage type at once.
# ---------------------------------------------------------------------
DAMAGE_ATTRIBS = {
    'Smashing_Dmg', 'Lethal_Dmg', 'Fire_Dmg', 'Cold_Dmg',
    'Energy_Dmg', 'Negative_Energy_Dmg', 'Toxic_Dmg', 'Psionic_Dmg',
}
# Resistance attribs same idea — all 8 → "Damage Resistance".
RESISTANCE_ATTRIBS = DAMAGE_ATTRIBS
# Mez attribs — all 6 of these → "Mez" (universal mez aspect, matches
# Controller / Dominator ATO convention in Will of the Controller etc.).
# Without the collapse, Forced Indoctrination's pieces would surface as
# "Damage/Confuse/Fear/Hold/Immobilize/Sleep/Stun" — accurate but unwieldy.
# Recognised as "Mez" by the planner's universal-mez expansion (every mez
# type gets the same Schedule A value, see UNIVERSAL_MEZ_KEYS in
# enhancement-values.ts).
MEZ_ATTRIBS = {'Held', 'Stunned', 'Sleep', 'Confused', 'Terrorized', 'Immobilized'}

# Defense attribs — a Defense BUFF piece carries Base_Defense plus the typed +
# positional defense scalars; a Defense DEBUFF piece carries Base_Defense alone.
# Both collapse to a single aspect, distinguished by the set's slot GROUP
# (verified against HC's hand-curated Aegis / Achilles' Heel / etc.).
DEFENSE_ATTRIBS = {
    'Base_Defense', 'Smashing', 'Lethal', 'Fire', 'Cold', 'Energy',
    'Negative_Energy', 'Psionic', 'Toxic', 'Melee', 'Ranged', 'Area',
}
DEFENSE_DEBUFF_GROUPS = {'Defense Debuff', 'Accurate Defense Debuff'}

# Movement-speed attribs. A Slow set debuffs ALL movement, so its pieces carry
# all three speeds → single "Slow" aspect. Travel sets buff one mode → Run /
# Fly / Jump individually (JumpHeight rides along with JumpingSpeed).
MOVEMENT_SPEED_ATTRIBS = {'RunningSpeed', 'FlyingSpeed', 'JumpingSpeed'}

# ToHit is one binary attrib; the set's slot group decides buff vs debuff label,
# matching HC's hand data ("ToHit" for buff sets, "ToHit Debuff" for debuff).
TOHIT_DEBUFF_GROUPS = {'To Hit Debuff', 'Accurate To-Hit Debuff'}

# Map of bin attrib → planner aspect label (for boost pieces).
ATTRIB_TO_ASPECT = {
    'Accuracy':           'Accuracy',
    'RechargeTime':       'Recharge',
    'EnduranceDiscount':  'Endurance',
    'Range':              'Range',
    'Knockback':          'Knockback',
    'Stunned':            'Stun',
    'Held':               'Hold',
    'Sleep':              'Sleep',
    'Confused':           'Confuse',
    'Terrorized':         'Terrorize',
    'Immobilized':        'Immobilize',
    'HitPoints':          'Heal',
    # In CoH every Heal-boosting enhancement also boosts Absorb, and the binary
    # encodes Absorb as its OWN Strength attrib on healing pieces (verified:
    # Panacea/Numina/etc. each carry HitPoints AND a distinct Absorb attrib).
    # Map the real attrib through rather than dropping it so Absorb surfaces as
    # its own enhanced stat (and a regen doesn't strip the Absorb the hand-data
    # carries). NOTE: Heal and Absorb do NOT dilute each other — they're the
    # same enhancement category (one boost, two attributes), so the planner
    # collapses the pair into a single aspect slot (getEffectiveAspectCount).
    # A pure Heal piece is 1 aspect (42.4% @ L50), not 2.
    'Absorb':             'Absorb',
    'Endurance':          'EndMod',
    'DamageType':         'Damage',
    # Travel-speed buffs (single mode). All three speeds present = a Slow set
    # instead ('Slow'), handled in _collapse_aspects.
    'RunningSpeed':       'Run',
    'FlyingSpeed':        'Fly',
    'JumpingSpeed':       'Jump',
    'JumpHeight':         'Jump',
    # Taunt/placate enhancement → Threat (Mocking Beratement, Perfect Zinger…).
    'Taunt':              'Threat',
    'Placate':            'Threat',
    # The snipe sets' interrupt-reduction aspect. The engine's aspect
    # vocabulary accepts "InterruptTime" (enhancement.rs, schedule C via the
    # Melee_Boosts_40 table) — dropping the attrib here left that slot fed by
    # nobody, so the six Interrupt pieces enhanced nothing and leaned on a
    # totalAspects stamp for their dilution (BOOST-5 step 2, the beta led).
    'InterruptTime':      'InterruptTime',
    'Unknown(91)':        'InterruptTime',
    # 'ToHit' is intentionally absent — mapped contextually in
    # _collapse_aspects (buff vs debuff by set group).
    # Unknown indices we've seen in boost pieces — map to the most common
    # CoH boost-type meaning. These are heuristics; refine when the
    # binary parser maps them properly.
    # Note: index 85 (Accuracy on Rebirth) and 116 (Create_Entity on Rebirth)
    # are now mapped in ATTRIB_NAME_REBIRTH, so the parser surfaces them
    # under their real names and these `Unknown(N)` keys never fire. Kept
    # here only as a safety net in case the parser regresses.
    'Unknown(85)':        'Accuracy',      # Rebirth's Accuracy slot (HC: 84)
    'Unknown(86)':        'Interrupt',
    'Unknown(116)':       None,            # Special / proc trigger — used as a marker
}


def _requires_text(requires) -> str:
    """A `Requires` expression as one string, for asking whether a token appears.

    The export states a requires clause as its RPN TOKEN LIST (COND-8); older
    trees stated the same clause pre-joined. Both are accepted, and the joined
    form is built here only to be searched — never re-split, because a token can
    itself contain a space.
    """
    if requires is None:
        return ''
    if isinstance(requires, str):
        return requires
    if isinstance(requires, list):
        return ' '.join(str(t) for t in requires)
    raise TypeError(f'unhandled Requires shape {type(requires).__name__}: {requires!r}')


def _collapse_aspects(attribs: list[str], set_group: str = '') -> tuple[list[str], bool]:
    """Collapse a piece's attribs into planner aspect labels.

    Returns (aspects, is_proc). is_proc=True when the piece carries a
    proc-marker attrib (Unknown(116) or similar). Aspects are returned
    in CoH community order (Accuracy, Damage, Endurance, Recharge, then
    others) rather than the binary's, so the list reads the way players
    talk about a piece — "Accuracy/Damage" not "Damage/Accuracy".

    `set_group` is the set's slot group — `_resolve_category`, i.e. the record's
    `GroupName` ("Resist Damage", "Melee Damage", …). In CoH, boost pieces with
    the 8 damage-type attribs always have aspect=Strength in the binary — the
    slotted power decides which "Strength" scalar that buffs. For a Resist Damage
    set the relevant scalar is the power's resistance scale, so the planner
    should label those pieces "Damage Resistance" rather than "Damage" to match
    HC's hand-curated convention (Aegis, Impervium Armor, etc.).

    This keys on GroupName and not on the record's `Category`, which the game
    leaves BLANK for every PvP, purple, event and ATO set — 31 of Homecoming's
    227, 100 of Rebirth's 233. Gladiator's Armor (PvP, all three forks) and
    Rebirth's Liberty's Belt (event) are Resist Damage sets with no category, so
    the old test labelled their resist pieces "Damage" and the engine dropped
    their resistance out of the totals while the power window (reading the
    hand-curated table) showed it — reported 2026-08-17. GroupName is stated by
    every record and is the only name the game's own boostset path tests
    (`_resolve_category`, BOOST-2); keying all three labels on it changes exactly
    those four sets and nothing else on any fork.
    """
    aspects: list[str] = []
    is_proc = False
    distinct = set(attribs)

    # A piece whose only attrib is Null carries no enhancement at all — the
    # Null × Strength × scale=1 template is a pure special-piece marker (the
    # third marker shape, alongside Create_Entity × Current and Null ×
    # Absolute). Reactive Defenses F, Inexhaustibility A and Synapse's
    # Agility F all state their always-on global this way; the global itself
    # lives in a separate PowerBoostsSlotted-gated auto (BOOST-4), so the
    # boost power shows only the marker. Pure markers only: a piece that
    # ALSO enhances (Experienced Marksman F's Range) keeps its aspect list
    # and is not flipped here — its dilution is stated by its own scale.
    if distinct == {'Null'}:
        return [], True

    if DAMAGE_ATTRIBS.issubset(distinct):
        aspects.append('Damage Resistance' if set_group == 'Resist Damage' else 'Damage')
        distinct -= DAMAGE_ATTRIBS

    if MEZ_ATTRIBS.issubset(distinct):
        aspects.append('Mez')
        distinct -= MEZ_ATTRIBS

    # Defense buff/debuff — Base_Defense marks a defense piece (buffs also carry
    # the typed + positional scalars). One aspect, labelled by set category.
    if 'Base_Defense' in distinct:
        aspects.append('Defense Debuff' if set_group in DEFENSE_DEBUFF_GROUPS else 'Defense')
        distinct -= DEFENSE_ATTRIBS

    # Slow — a Slow set debuffs all movement, so all three speeds are present.
    # (Single-mode travel buffs fall through to Run/Fly/Jump via the map below.)
    if MOVEMENT_SPEED_ATTRIBS.issubset(distinct):
        aspects.append('Slow')
        distinct -= MOVEMENT_SPEED_ATTRIBS
        distinct.discard('JumpHeight')

    # ToHit — one binary attrib; the set category decides buff vs debuff label.
    if 'ToHit' in distinct:
        aspects.append('ToHit Debuff' if set_group in TOHIT_DEBUFF_GROUPS else 'ToHit')
        distinct.discard('ToHit')

    for a in sorted(distinct):
        mapped = ATTRIB_TO_ASPECT.get(a)
        if mapped is None:
            if a == 'Unknown(116)':
                is_proc = True
            continue
        if mapped not in aspects:
            aspects.append(mapped)

    return _sort_aspects_canonical(aspects), is_proc


# CoH community / Mids canonical piece-name ordering. The four most-
# common attack-IO aspects come first in this fixed order; anything
# else (procs, exotic mez aspects) follows alphabetically. Used by
# `_collapse_aspects`, so a piece carrying {Damage, Accuracy} lists them in a
# stable order rather than the binary's.
#
# This orders the ASPECT LIST only; piece names come from the boost power and
# follow the game's own ordering, which is not this one. Recharge is last here;
# Accuracy first; the
# "type" aspects (Damage/Resistance/Defense) and EndMod precede Endurance;
# Heal/Absorb sit just after Endurance; mez/movement/utility aspects follow,
# before Recharge. Any aspect NOT listed falls to the end alphabetically, so
# this list must stay comprehensive.
_ASPECT_CANONICAL_ORDER = [
    'Accuracy', 'Damage', 'Damage Resistance', 'Defense', 'Defense Debuff',
    'EndMod', 'Endurance', 'Heal', 'Absorb',
    'Hold', 'Mez', 'Confuse', 'Immobilize',
    'Knockback', 'KnockToKnockDown', 'Range',
    'Recharge',
    # HC's hand-naming places these AFTER Recharge (e.g. "Recharge/Stun",
    # "Accuracy/Recharge/Sleep", "Endurance/Recharge/ToHit Debuff").
    'Stun', 'Sleep', 'Terrorize', 'Fear',
    'Run', 'Fly', 'Jump', 'Slow', 'Move Speed',
    'Threat', 'ToHit', 'ToHit Debuff', 'ToHit Buff', 'Interrupt', 'InterruptTime',
]


def _sort_aspects_canonical(aspects: list[str]) -> list[str]:
    canonical = [a for a in _ASPECT_CANONICAL_ORDER if a in aspects]
    rest = sorted(a for a in aspects if a not in _ASPECT_CANONICAL_ORDER)
    return canonical + rest


# Map proc-effect attribs to short human-readable labels for piece
# naming. These are NOT enhancement aspects (which use Strength) — they
# describe what the proc does when it triggers. Damage types get
# collapsed by category in `_proc_effect_labels` below.
_PROC_EFFECT_LABEL = {
    'Terrorized':         'Fear',
    'Held':               'Hold',
    'Stunned':            'Stun',
    'Sleep':              'Sleep',
    'Confused':           'Confuse',
    'Immobilized':        'Immobilize',
    'HitPoints':          'Heal',
    'Endurance':          'Endurance',
    'Recovery':           '+Recovery',
    'Regeneration':       '+Regeneration',
    'Smashing_Dmg':       'Smashing Damage',
    'Lethal_Dmg':         'Lethal Damage',
    'Fire_Dmg':           'Fire Damage',
    'Cold_Dmg':           'Cold Damage',
    'Energy_Dmg':         'Energy Damage',
    'Negative_Energy_Dmg': 'Negative Energy Damage',
    'Toxic_Dmg':          'Toxic Damage',
    'Psionic_Dmg':        'Psionic Damage',
    'Heal_Dmg':           'Heal',
    # Travel-set +Stealth globals (Celerity / Freebird / Unbounded Leap / Time &
    # Space Manipulation) carry their stealth as StealthRadius_PVE/PVP (aspect
    # Current, scale 30/300). Both collapse to one "Stealth" label.
    'StealthRadius_PVE':  'Stealth',
    'StealthRadius_PVP':  'Stealth',
    # NB: Create_Entity is intentionally NOT mapped. It is a summon/FX/resurrect
    # pseudopet marker whose real identity (Energy Font, Self Resurrect, the
    # stealth FX entities) comes from HC_PIECE_PATCHES / PROC_DATABASE, never
    # this heuristic. The |scale|=1.0 summon/resurrect markers are already
    # dropped at the template level above; the leftover positive-scale stealth
    # FX entities (0.5/0.8) used to fall through here and mislabel the piece
    # "Chance for Resurrect" — now they're simply unlabeled and ignored.
}


def _proc_effect_labels(attribs: list[str]) -> list[str]:
    """Build short labels describing what a proc piece does when it
    triggers. Used to surface "Recharge/Chance for Fear, Psionic Damage"
    instead of a bare "Recharge/Chance". Deduplicates while preserving
    insertion order so the binary order shows through.
    """
    seen: set[str] = set()
    out: list[str] = []
    for a in attribs:
        label = _PROC_EFFECT_LABEL.get(a)
        if not label or label in seen:
            continue
        seen.add(label)
        out.append(label)
    return out


def _piece_name_from_aspects(aspects: list[str]) -> str:
    """Build a piece display name from its aspects.

    "Accuracy/Damage", "Damage/Recharge", "Recharge/Chance for Resolve".
    """
    if not aspects:
        return 'Empty'
    return '/'.join(aspects)


def _piece_name_from_display(display: str) -> str | None:
    """The game's own name for a boost piece, read off the boost power.

    A boost power's display_name is "<qualifiers and set name>: <piece name>" —
    "Aegis: Resistance/Endurance", "(Blaster) Defiant Barrage: Accuracy/Damage".
    The split is on the LAST colon, because the head is not a dependable copy of
    the set's own name and can hold a colon of its own: the binary spells one set
    "Ascendancy" and its pieces "Ascendency", Rebirth's "Numina's Convalesence"
    pieces say "Convalescence", and three Superior ATO pieces read "Superior:
    Brute's Fury: ...". No piece name on any fork contains a colon, so the last
    one is always the separator.

    None means the export carries no name here, and the caller keeps whatever it
    derived. Returning the whole string instead would salvage nothing: the only
    separator-less display names in any fork are the two Thunderspy Scourging
    Blast procs, whose display_name is an unresolved clientmessages id
    ("P455782297") — a hash to print at the player, not a name.
    """
    text = (display or '').strip()
    _head, sep, tail = text.rpartition(':')
    if not sep:
        return None
    return tail.strip() or None


# Multi-aspect dilution modifier — mirrors getMultiAspectModifier() in
# src/utils/calculations/enhancement-values.ts: an IO that enhances N aspects
# delivers each at this fraction of the single-aspect base.
_MULTI_ASPECT_MODIFIER = {1: 1.0, 2: 0.625, 3: 0.5, 4: 0.4375}


def _derive_effective_aspect_count(enh_scales: list[float], rarity_mult: float) -> int | None:
    """Recover a piece's effective aspect count from its enhancement scale.

    The binary stores the already-diluted enhancement magnitude:
    `scale = getMultiAspectModifier(count) × set-rarity-multiplier`. So a piece
    whose Recharge enhancement reads scale 0.4375 is a 4-aspect IO (0.4375 is
    the 4-aspect modifier); Luck of the Gambler's Defense at 0.625 is a 2-aspect
    IO. Inverting this is the *authoritative* effective count — more reliable
    than counting aspect-list entries or name segments, which under-count proc/
    global pieces (the LotG +Recharge global, ATO "#6" Recharge/Chance pieces).

    `rarity_mult` is 1.25 for purple/Superior sets (matching getSetRarityMultiplier),
    else 1.0. Returns the count, or None when no enhancement scale cleanly matches
    a modifier (pure-proc or odd-scale global pieces).

    Try the set's rarity multiplier first, then fall back to 1.0: a few Superior
    ATO "#6" proc pieces store their incidental Recharge at the un-scaled 0.4375
    (the proc, not the enhancement, carries the rarity bonus), so dividing by
    1.25 would miss the modifier. 1.0 is only reached when 1.25 finds no match,
    so a genuine ×1.25 piece (e.g. 0.78125→2) is never misread.
    """
    mults = [rarity_mult, 1.0] if rarity_mult != 1.0 else [1.0]
    for mult in mults:
        best = None
        for sc in enh_scales:
            m = sc / mult
            for count, mod in _MULTI_ASPECT_MODIFIER.items():
                if abs(m - mod) < 0.02:
                    best = count if best is None else max(best, count)
        if best is not None:
            return best
    return None


# ---------------------------------------------------------------------
# Set_Bonus power → planner bonus effect entry
# ---------------------------------------------------------------------
# A Set_Bonus power grants a buff via effect templates of (attrib, aspect,
# scale). We turn each into the planner's {stat, value} entry, where:
#   - `stat` is the planner-canonical key the set-bonus calc understands
#     (see STAT_NAME_MAP in src/utils/calculations/set-bonuses.ts). It is
#     NOT a free-form name: a key the planner doesn't recognise is silently
#     dropped from the build's totals, so these MUST match.
#   - `value` is the displayed percentage, = scale × a per-attrib multiplier
#     (see _bonus_multiplier). The binary stores `scale`; the game multiplies
#     it by an attrib-specific modifier to get the % shown in Mids.
#
# Two conventions matter for matching the planner's expectations exactly:
#   1. Paired damage types (S/L, F/C, E/N, P/T) — the planner auto-applies a
#      bonus to BOTH members of a pair (PAIRED_STATS). So a "+6% Fire/Cold"
#      bonus must emit ONLY ONE member (the alphabetically-first, e.g. cold),
#      or the value double-counts. Same for typed defence (S/L, F/C, E/N).
#   2. "All resistance"/"all mez resistance" collapse to a single
#      damage_resistance_(all) / mez_resistance_(all) key.
# Both are handled in _resolve_bonus_effects below.

# Damage-type Strength attribs that collapse to one "damage" bonus (the
# binary encodes "+X% Damage" as 8 parallel per-type templates).
_DMG_STRENGTH = {(a, 'Strength') for a in DAMAGE_ATTRIBS}
# All-8 damage Resistance → damage_resistance_(all).
_DMG_RESIST = {(a, 'Resistance') for a in DAMAGE_ATTRIBS}
# All-6 mez Resistance → mez_resistance_(all). Only the all-6 collapse produces a
# recognised key; a lone mez type has no per-type bonus stat in the vocabulary.
# The collapse is lossy on its own — `(all)` names no types, and the calc has six
# separate per-type accumulators to spend it into — so the consumed types ride
# along on the emitted effect (`mez_types`). See _MEZ_ATTRIB_TO_TYPE.
_MEZ_RESIST = {(m, 'Resistance') for m in MEZ_ATTRIBS}

# Mez attrib → the planner's lowercase mez-type key (the `effects.mezResistance`
# vocabulary a power's own mez resistance already uses, so a set bonus and a power
# feed the same six accumulators). Terrorized is spelled `fear` there.
_MEZ_ATTRIB_TO_TYPE = {
    'Held': 'hold',
    'Stunned': 'stun',
    'Immobilized': 'immobilize',
    'Sleep': 'sleep',
    'Confused': 'confuse',
    'Terrorized': 'fear',
}


def _binary_attuned_only(set_id: str, ctx: dict | None) -> bool:
    """`attunedOnly` for a set whose entry is copied from the hand file wholesale.

    Those copies exist because `build_sets` skips the record for its rarity tier,
    not because the record is unreadable — so the piece membership that states
    attunement is right there, the same as `rarity` and the piece names. Reading
    it here rather than inheriting the copied value also keeps the hand file from
    being its own source: the entry it copies is the last run's output.
    """
    record = _binary_record(set_id, ctx)
    if record is None:
        raise SystemExit(
            f"whole-set copy '{set_id}': no binary record, so nothing states whether "
            f'the set is attuned-only'
        )
    return _attuned_only(record)


def _binary_record(set_id: str, ctx: dict | None):
    """The boostset record a set id came from, or None."""
    if not ctx:
        return None
    return next(
        (s for s in ctx['sets'] if s.name.lower().replace('-', '').replace('__', '_') == set_id),
        None,
    )


def _binary_piece_names(set_id: str, ctx: dict | None) -> dict[int, str]:
    """Piece number → the game's name for it, read straight from the boost powers.

    For the sets whose hand-curated entry replaces the binary one wholesale. Those
    are copied because `build_sets` skips the record (no planner rarity for its
    tier), not because the record is unreadable — its boost powers name their
    pieces like every other set's.
    """
    if not ctx:
        return {}
    record = next(
        (s for s in ctx['sets'] if s.name.lower().replace('-', '').replace('__', '_') == set_id),
        None,
    )
    if record is None:
        return {}
    names: dict[int, str] = {}
    for index, entry in enumerate(record.boostlists):
        if not entry.boosts:
            continue
        name = _piece_name_from_display(_power_display_name(ctx['power_index'].get(entry.boosts[0])))
        if name:
            names[index + 1] = name
    return names


def _binary_mez_types_by_tier(set_id: str, ctx: dict | None) -> dict[int, list[str]]:
    """Tier (piece threshold) → mez types, read straight from the binary set record's
    bonus auto-powers. For the sets whose hand-curated entry replaces the binary
    bonuses wholesale, the binary record is the only surviving witness to which types
    an `(all)` label covers."""
    if not ctx:
        return {}
    record = next(
        (
            s
            for s in ctx['sets']
            if s.name.lower().replace('-', '').replace('__', '_') == set_id
        ),
        None,
    )
    if record is None:
        return {}
    by_tier: dict[int, list[str]] = {}
    for bonus in record.bonuses:
        if _tier_gate(bonus) != 'base':
            continue
        for auto_power in (bonus.auto_powers or []):
            power = ctx['power_index'].get(auto_power)
            types = _mez_types_of(power) if power else []
            if types:
                by_tier[bonus.min_boosts] = types
    return by_tier


def _mez_types_of(set_bonus_power) -> list[str]:
    """The mez types a Set_Bonus power resists, as planner keys — or [] unless it
    resists all six. Partial coverage has no `(all)` warrant and no per-type bonus
    stat to carry it, so it is reported as absent rather than guessed at."""
    attribs: set[str] = set()
    for eg in (set_bonus_power.effects or []):
        for t in (eg.templates or []):
            if (t.aspect or '') == 'Resistance':
                attribs |= set(t.attribs or []) & MEZ_ATTRIBS
    if attribs != MEZ_ATTRIBS:
        return []
    return sorted(_MEZ_ATTRIB_TO_TYPE[a] for a in attribs)

# (attrib, aspect) → planner-canonical bonus stat key. Keys match the statNameMap
# in contract/set-bonus-stat-vocab.json. Paired members are emitted as their own
# key here (e.g. both Fire_Dmg and Cold_Dmg → damage_resistance_(fire/cold));
# the de-dup in _resolve_bonus_effects keeps only the alpha-first of a pair.
LEGACY_ATTRIB_TO_BONUS_STAT = {
    # Damage resistance, per type.
    ('Smashing_Dmg',       'Resistance'): 'damage_resistance_(smashing)',
    ('Lethal_Dmg',         'Resistance'): 'damage_resistance_(lethal)',
    ('Fire_Dmg',           'Resistance'): 'damage_resistance_(fire)',
    ('Cold_Dmg',           'Resistance'): 'damage_resistance_(cold)',
    ('Energy_Dmg',         'Resistance'): 'damage_resistance_(energy)',
    ('Negative_Energy_Dmg','Resistance'): 'damage_resistance_(negative)',
    ('Psionic_Dmg',        'Resistance'): 'damage_resistance_(psionic)',
    ('Toxic_Dmg',          'Resistance'): 'damage_resistance_(toxic)',
    # Defense by position / type (aspect=Current).
    ('Melee',        'Current'): 'defense_(melee)',
    ('Ranged',       'Current'): 'defense_(ranged)',
    ('Area',         'Current'): 'defense_(area)',
    ('Smashing',     'Current'): 'defense_(smashing)',
    ('Lethal',       'Current'): 'defense_(lethal)',
    ('Fire',         'Current'): 'defense_(fire)',
    ('Cold',         'Current'): 'defense_(cold)',
    ('Energy',       'Current'): 'defense_(energy)',
    ('Negative_Energy', 'Current'): 'defense_(negative)',
    ('Psionic',      'Current'): 'defense_(psionic)',
    ('Toxic',        'Current'): 'defense_(toxic)',
    # HP / Endurance maximums.
    ('HitPoints',    'Maximum'):  'maximum_hitpoints',
    ('Endurance',    'Maximum'):  'maximum_endurance',
    # Recovery / Regeneration. HC encodes via Strength on Endurance/HitPoints;
    # Rebirth via the dedicated Recovery/Regeneration attribs (Current). Both
    # map to the same planner stats.
    ('Endurance',    'Strength'): 'recovery',
    ('HitPoints',    'Strength'): 'regeneration',
    ('Recovery',     'Strength'): 'recovery',
    ('Regeneration', 'Strength'): 'regeneration',
    ('Recovery',     'Current'):  'recovery',
    ('Regeneration', 'Current'):  'regeneration',
    # Common offensive / utility stats.
    ('RechargeTime', 'Strength'): 'recharge',
    ('ToHit',        'Strength'): 'tohit',
    ('Accuracy',     'Strength'): 'accuracy',
    ('Range',        'Strength'): 'range',
    ('PerceptionRadius', 'Current'): 'perception',
    ('EnduranceDiscount', 'Strength'): 'endurance_discount',
    ('Heal_Dmg',     'Strength'): 'healing_strength',
    # All 8 damage types × Strength → a single "damage" bonus (the dedup in
    # _resolve_bonus_effects squashes the 8 identical entries into one).
    ('Smashing_Dmg',        'Strength'): 'damage',
    ('Lethal_Dmg',          'Strength'): 'damage',
    ('Fire_Dmg',            'Strength'): 'damage',
    ('Cold_Dmg',            'Strength'): 'damage',
    ('Energy_Dmg',          'Strength'): 'damage',
    ('Negative_Energy_Dmg', 'Strength'): 'damage',
    ('Toxic_Dmg',           'Strength'): 'damage',
    ('Psionic_Dmg',         'Strength'): 'damage',
    # Mez duration buffs — extend the duration of YOUR mez attacks on enemies.
    # Per-type (no collapse): each maps to a distinct stat.
    ('Confused',     'Strength'): 'confuse_duration',
    ('Held',         'Strength'): 'hold_duration',
    ('Stunned',      'Strength'): 'stun_duration',
    ('Immobilized',  'Strength'): 'immobilize_duration',
    ('Sleep',        'Strength'): 'sleep_duration',
    ('Terrorized',   'Strength'): 'terror_duration',
    # Movement-speed buffs collapse to a single "increased_movement" bonus
    # (encoded as up to 4 parallel templates; Current and Strength both seen).
    ('RunningSpeed', 'Current'):  'increased_movement',
    ('FlyingSpeed',  'Current'):  'increased_movement',
    ('JumpingSpeed', 'Current'):  'increased_movement',
    ('JumpHeight',   'Current'):  'increased_movement',
    ('RunningSpeed', 'Strength'): 'increased_movement',
    ('FlyingSpeed',  'Strength'): 'increased_movement',
    ('JumpingSpeed', 'Strength'): 'increased_movement',
    ('JumpHeight',   'Strength'): 'increased_movement',
    # Movement debuff (slow) resistance.
    ('RunningSpeed', 'Resistance'): '+res(slow)',
    ('FlyingSpeed',  'Resistance'): '+res(slow)',
    # Recharge debuff resistance.
    ('RechargeTime', 'Resistance'): '+res(recharge_debuff)',
    # Knockback protection (magnitude points) and knockback strength buff.
    ('Knockback',    'Current'):  'knockback_protection',
    ('Knockup',      'Current'):  'knockback_protection',
    ('Knockback',    'Strength'): 'knockback_strength',
    ('Knockup',      'Strength'): 'knockback_strength',
}

_BRIDGE_CLI = PROJECT_ROOT / 'scripts' / 'bridge-attrib-one.cjs'


@lru_cache(maxsize=1024)
def _bridge_attrib(attrib: str, aspect: str, table: str = '') -> dict:
    req = json.dumps({'attrib': attrib, 'aspect': aspect, 'table': table})
    out = subprocess.check_output(
        ['node', str(_BRIDGE_CLI), req],
        cwd=PROJECT_ROOT,
        text=True,
    )
    return json.loads(out)


def _bonus_stat_from_bridge(attrib: str, aspect: str, table: str = '') -> str | None:
    # Explicit edge cases that bridgeAttrib intentionally leaves context-neutral.
    legacy = LEGACY_ATTRIB_TO_BONUS_STAT.get((attrib, aspect))
    if legacy in {'healing_strength', '+res(slow)', '+res(recharge_debuff)'}:
        return legacy

    br = _bridge_attrib(attrib, aspect, table)
    et = br.get('effectType')
    sub = (br.get('subType') or '').lower()

    if et == 'DamageBuff':
        return 'damage'
    if et == 'Resistance':
        if sub == 'all':
            return 'damage_resistance_(all)'
        return f'damage_resistance_({sub})' if sub else None
    if et == 'Defense':
        if sub == 'all':
            return 'defense_(all)'
        return f'defense_({sub})' if sub else None
    if et == 'MaxHP':
        return 'maximum_hitpoints'
    if et == 'MaxEndurance':
        return 'maximum_endurance'
    if et == 'Recovery':
        return 'recovery'
    if et == 'Regeneration':
        return 'regeneration'
    if et == 'RechargeTime':
        return 'recharge'
    if et == 'ToHit':
        return 'tohit'
    if et == 'Accuracy':
        return 'accuracy'
    if et == 'Range':
        return 'range'
    if et == 'Perception':
        return 'perception'
    if et == 'EnduranceDiscount':
        return 'endurance_discount'
    if et == 'Movement':
        return 'increased_movement'
    if et == 'MezResist':
        # `mez_resistance_(all)` is emitted by the family collapse in
        # _resolve_bonus_effects, which only fires when all six mez types are present —
        # that issubset check IS the warrant for the `(all)` label. Reaching here means a
        # single mez type arrived on its own, so answering `(all)` would fabricate the
        # warrant the collapse withheld, and the row would carry no mez_types for the calc
        # to spend (the MEZRES-1 shape). Repel is the one such type with a set-bonus
        # carrier and a global of its own (MEZRES-3); everything else returns None and is
        # reported as unmapped rather than guessed at.
        return 'repel_resistance' if sub == 'repel' else None
    if et == 'Mez' and sub in {'knockback', 'knockup'}:
        return 'knockback_protection'
    if et == 'Knockback':
        return 'knockback_strength'

    # Fallback preserves current behavior for any unmapped legacy tuples.
    return LEGACY_ATTRIB_TO_BONUS_STAT.get((attrib, aspect))

# Paired stats the planner's PAIRED_STATS auto-expands to BOTH members. When
# both members of a pair appear in one tier we keep only the alpha-first key
# (matching HC's hand-data convention) so the value isn't double-counted.
_BONUS_STAT_PAIRS = [
    ('damage_resistance_(cold)',    'damage_resistance_(fire)'),
    ('damage_resistance_(lethal)',  'damage_resistance_(smashing)'),
    ('damage_resistance_(energy)',  'damage_resistance_(negative)'),
    ('damage_resistance_(psionic)', 'damage_resistance_(toxic)'),
    ('defense_(cold)',   'defense_(fire)'),
    ('defense_(lethal)', 'defense_(smashing)'),
    ('defense_(energy)', 'defense_(negative)'),
    # The planner pairs recharge-debuff resistance with slow resistance
    # (PAIRED_STATS: debuffresistrecharge → [recharge, slow]). A bonus that
    # buffs both (e.g. Avalanche: RunningSpeed+FlyingSpeed+RechargeTime all
    # Resistance) must emit only +res(recharge_debuff) so slow isn't doubled.
    ('+res(recharge_debuff)', '+res(slow)'),
]

# Per-attrib scale→value multiplier. Empirically derived and cross-validated
# against all 225 shared HC hand-data sets (scripts: see HC-IO-SETS-BINARY-
# SOURCING.md): every shared tier reproduces the hand value within float
# rounding. The game multiplies the stored `scale` by an attrib-specific
# modifier to get the displayed %:
#   - damage buff (damage-type attribs, aspect=Strength)
#     on HC's SetBonusPetShare table                     → ×250
#   - max HP (HitPoints/Maximum)                         → ×10
#   - max endurance (Endurance/Maximum)                  → ×1 (scale is already %)
#   - everything else (resistance, defence, recharge,
#     recovery, regen, movement, mez-res, durations, …)  → ×100
#
# The damage ×2.5 belongs to HC's SetBonusPetShare authoring, not to the
# damage attribs: Rebirth/Thunderspy re-authored their damage tiers on
# Melee_Ones with the ×2.5 folded into the scale (Increased_Damage_5 is
# HC 0.012×SetBonusPetShare and Rebirth 0.03×Melee_Ones — the same 3%),
# while every non-damage tier kept its scale unchanged across the table
# move. Keying the ×250 on the attrib alone inflated the fork-authored
# damage tiers ×2.5 (BOOST-5).
def _bonus_multiplier(attrib: str, aspect: str, table: str) -> float:
    if aspect == 'Strength' and attrib in DAMAGE_ATTRIBS:
        return 250.0 if table == 'SetBonusPetShare' else 100.0
    if aspect == 'Maximum':
        if attrib == 'HitPoints':
            return 10.0
        if attrib == 'Endurance':
            return 1.0
    return 100.0


# (attrib, aspect) tuples that should be ignored entirely — not bonuses,
# just power-state metadata that happens to appear in Set_Bonus effect
# templates. Suppresses noise in the unmapped-pairs diagnostic.
_BONUS_LOOKUP_IGNORE: set[tuple[str, str]] = {
    ('Set_Mode', 'Absolute'),
}

# Populated during _resolve_bonus_effects when a (attrib, aspect) tuple has no
# bridge-resolved (or legacy-fallback) stat mapping. Printed at end-of-run so
# the next silently-dropped bonus surfaces immediately instead of vanishing.
_UNMAPPED_BONUS_PAIRS: dict[tuple[str, str], int] = {}

# Pieces where the scale-derived effective aspect count is BELOW the extracted
# aspect-list length — a signal that the binary surfaced a spurious enhancement
# aspect (e.g. travel sets where the movement-speed collapse mislabels a buff).
# Printed at end-of-run as override candidates; not silently emitted.
_ASPECT_COUNT_UNDERSHOOTS: list[str] = []

# Hand-curated `mez_resistance_(all)` tiers the binary had no matching tier to carry
# types across from (see _apply_homecoming_overrides). Reported at end-of-run: the
# calc needs the types to spend the bonus, so an entry here is a bonus that would
# reach the planner uncarried, not a cosmetic gap.
_HAND_BONUS_UNTYPED: list[str] = []

# Pieces whose boost power carries no readable display_name, so the game's own
# name for them is unavailable and the derived one stands. Reported at end-of-run
# because the derived name is a guess wherever it differs from the game's, and a
# growing list here means the message table is resolving worse than it used to.
_PIECE_NAME_UNAVAILABLE: list[str] = []

# Shared sets where this fork's own build resolved NO bonus tiers, so HC's stood
# in (see _reuse_hand_entry). Reported at end-of-run: each entry is a set whose
# shipped bonuses are another fork's authoring, standing where the fork's own
# resolution came up empty — a resolution gap to close, not a convention.
_SHARED_BONUS_FALLBACK: list[str] = []


def _resolve_bonus_effects(set_bonus_power: _JsonPower) -> list[dict]:
    """Build the planner's bonus effects[] list from a Set_Bonus power's
    effect templates.

    Templates are grouped by mapped identity (planner stat key), not by value,
    so tiny float splits in one family can't leak duplicate/misaligned keys.
    All-damage/all-resistance/all-mez families are collapsed by identity first,
    then each key selects one representative value (max abs). Paired
    resistance/defence members are de-duped to the alpha-first key so the
    planner's PAIRED_STATS expansion doesn't double-count.
    """
    # Flat entries: (attrib, aspect, table, value)
    entries: list[tuple[str, str, str, float]] = []
    for eg in set_bonus_power.effects:
        for t in eg.templates:
            aspect = t.aspect or ''
            table = t.table or ''
            for a in (t.attribs or []):
                value = round(abs(t.scale) * _bonus_multiplier(a, aspect, table), 4)
                entries.append((a, aspect, table, value))

    out: list[dict] = []
    attset = {(a, asp) for a, asp, _, _ in entries}
    key_values: dict[str, list[float]] = {}

    # Family collapses by identity (independent of float/value splits).
    family_consumed: set[tuple[str, str]] = set()
    if _DMG_STRENGTH.issubset(attset):
        vals = [v for a, asp, _, v in entries if (a, asp) in _DMG_STRENGTH]
        if vals:
            key_values.setdefault('damage', []).append(max(vals))
        family_consumed |= _DMG_STRENGTH
    if _DMG_RESIST.issubset(attset):
        vals = [v for a, asp, _, v in entries if (a, asp) in _DMG_RESIST]
        if vals:
            key_values.setdefault('damage_resistance_(all)', []).append(max(vals))
        family_consumed |= _DMG_RESIST
    key_types: dict[str, list[str]] = {}
    if _MEZ_RESIST.issubset(attset):
        vals = [v for a, asp, _, v in entries if (a, asp) in _MEZ_RESIST]
        if vals:
            key_values.setdefault('mez_resistance_(all)', []).append(max(vals))
        # The types the `(all)` label stands for, so the calc spends the bonus into
        # the same per-type accumulators a power's mez resistance feeds rather than
        # re-deriving what "all" means. The issubset guard above is the warrant: the
        # key is only emitted when every one of the six is present.
        key_types['mez_resistance_(all)'] = sorted(
            _MEZ_ATTRIB_TO_TYPE[a] for a, asp in _MEZ_RESIST
        )
        family_consumed |= _MEZ_RESIST

    # Map remaining entries per-attrib through bridge/fallback resolver.
    for a, aspect, table, value in entries:
        if (a, aspect) in family_consumed:
            continue
        key = _bonus_stat_from_bridge(a, aspect, table)
        if key is None:
            if (a, aspect) not in _BONUS_LOOKUP_IGNORE:
                _UNMAPPED_BONUS_PAIRS[(a, aspect)] = _UNMAPPED_BONUS_PAIRS.get((a, aspect), 0) + 1
            continue
        key_values.setdefault(key, []).append(value)

    keys = sorted(key_values.keys())

    # Drop the alpha-later member of any present pair (planner re-pairs it).
    for keep, drop in _BONUS_STAT_PAIRS:
        if keep in keys and drop in keys:
            keys = [k for k in keys if k != drop]

    # Emit one effect per key with a stable representative value.
    for key in keys:
        value = max(key_values.get(key, [0]))
        if key == 'knockback_protection':
            value = abs(value)
        value = round(value, 4)
        desc = f'+{value}% {key.replace("_", " ").title()}'
        effect = {'stat': key, 'value': value, 'desc': desc}
        if key in key_types:
            effect['mez_types'] = key_types[key]
        out.append(effect)
    return out


# ---------------------------------------------------------------------
# Main extraction
# ---------------------------------------------------------------------

def build_sets(
    sets: list[BoostSetRecord],
    power_index: dict[str, _JsonPower],
    hc_sets: dict[str, dict],
) -> tuple[dict[str, dict], list[str]]:
    """Build the raw binary io-sets shape (one entry per boostset) from the
    parsed bins. Dataset-agnostic: the per-set extraction is identical for HC
    and Rebirth — the differences live entirely in the post-build override
    passes (_apply_rebirth_overrides / _apply_homecoming_overrides).

    `hc_sets` is HC's hand-curated io-sets-raw (parsed by _load_hc_sets); it is
    consulted only for the icon fallback here (icons aren't in the binary).
    Returns (out_sets, skipped).
    """
    out_sets: dict[str, dict] = {}
    skipped: list[str] = []
    for s in sets:
        rarity = EC_RARITY_TO_PLANNER.get(s.rarity)
        if not rarity:
            skipped.append(f'{s.name}: unmapped rarity {s.rarity!r}')
            continue
        # The same read `build_power_category_index` makes, so a set's `type` and
        # the categories its powers list are the one field rather than two
        # derivations of it — the picker matches them as strings.
        type_ = _resolve_category(s)

        # display_name arrives pre-resolved from export_powers.py (P-hash ->
        # text, see boost_sets resolution pass in main()).
        display = s.display_name or s.name.replace('_', ' ')

        # Build pieces.
        pieces = []
        for i, bl in enumerate(s.boostlists):
            # Pick the first boost variant — Crafted/Attuned share aspects.
            piece_power = None
            for boost_name in bl.boosts:
                pp = power_index.get(boost_name)
                if pp:
                    piece_power = pp
                    break
            if not piece_power:
                continue
            # Collect enhancement-aspect attribs across the piece's effect
            # templates, separating proc-trigger markers and proc-effect
            # templates from the enhancement aspects.
            #
            # Enhancement aspects (Recharge, Accuracy, Sleep, Hold, ...) always
            # use aspect=Strength — they buff that stat's effectiveness on the
            # slotted power. Proc effects (apply X when triggered) use Current
            # or Absolute. Proc trigger markers also use Current/Absolute and
            # match specific (attrib, aspect, scale) signatures observed in
            # Rebirth (verified on Endless Nightmare, Witchcraft, Vampire's
            # Bite, The Haunting, Guardian's Gift, etc.):
            #   - Create_Entity × Current × |scale|=1.0   (ATO + summon procs)
            #   - Null × Absolute × scale=1.0             (Halloween procs)
            # On HC the proc marker surfaces as Unknown(116) and is handled
            # by `_collapse_aspects`; Rebirth's index 116 is named
            # Create_Entity per ATTRIB_NAME_REBIRTH, so we detect it here at
            # the template level instead.
            attribs: list[str] = []
            proc_effect_attribs: list[str] = []  # Attribs from proc-effect templates (used for piece naming)
            enh_scales: list[float] = []  # scales of the enhancement (Strength, +scale) templates
            is_proc_marker = False
            has_proc_group = False
            for eg in piece_power.effects:
                # A chance-gated or PPM effect group is a proc (GAME-DATA-
                # PRINCIPLES §3) — the cross-server signal, since group `chance`
                # and `ppm` are populated for both Parse7 (HC) and Parse6
                # (Rebirth, derived from tick_chance).
                if (eg.ppm or 0) > 0 or (eg.chance is not None and eg.chance < 0.999):
                    has_proc_group = True
                for t in eg.templates:
                    if not t.attribs:
                        continue
                    if len(t.attribs) == 1 and abs(abs(t.scale) - 1.0) < 0.01:
                        a0 = t.attribs[0]
                        if a0 == 'Create_Entity' and t.aspect == 'Current':
                            is_proc_marker = True
                            continue
                        if a0 == 'Null' and t.aspect == 'Absolute':
                            is_proc_marker = True
                            continue
                    # Enhancement aspects are aspect=Strength with a POSITIVE,
                    # non-zero scale. Excluded (→ proc effects): aspect≠Strength
                    # (proc payload, Current/Absolute), negative-scale Strength
                    # (proc debuffs — e.g. Winter's Bite's -Recharge/-Slow; sign
                    # distinguishes buff from debuff, §3), and scale==0 Strength
                    # meta-templates (the engine's strength-definition rows, §3).
                    if t.aspect == 'Strength' and t.scale > 0.001:
                        attribs.extend(t.attribs)
                        enh_scales.append(round(t.scale, 4))
                    else:
                        proc_effect_attribs.extend(t.attribs)
            aspects, is_proc = _collapse_aspects(attribs, type_)
            is_proc = is_proc or is_proc_marker or has_proc_group
            # Fallback proc detection: when the piece has NO Strength
            # templates (no enhancement aspects to collapse) but DOES carry
            # Current/Absolute templates with damage-type attribs, it's a
            # bare proc piece — the damage template itself is the effect.
            # Forced Indoctrination piece 6 is the canonical case: a single
            # Psionic_Dmg/Absolute/Magnitude template with no Null marker
            # alongside it. Without this branch, _collapse_aspects sees an
            # empty attrib list and the piece exports as `name="Empty"`.
            if not is_proc and not attribs and proc_effect_attribs:
                is_proc = True
            derived_name = _piece_name_from_aspects(aspects) or 'Special'
            if is_proc:
                # A "Chance for X" label assembled from whatever proc effects we
                # saw (Terrorized -> Fear, Psionic_Dmg -> Psionic Damage). It
                # degrades to a bare "Chance" whenever the effect attribs aren't
                # in the label map, which is the whole reason the game's own name
                # below outranks it.
                effect_labels = _proc_effect_labels(proc_effect_attribs)
                effect_phrase = ', '.join(effect_labels)
                if 'Recharge' in aspects:
                    derived_name = f'Recharge/Chance for {effect_phrase}' if effect_phrase else 'Recharge/Chance'
                else:
                    derived_name = f'Chance for {effect_phrase}' if effect_phrase else 'Chance'
            # The game names every piece, on the piece's own boost power. That is
            # the string the player reads in-game and the one Mids prints, so it
            # wins over anything assembled here from attribs. Only the display
            # name moves: `aspects` and `totalAspects` stay binary-derived, and
            # they alone feed the enhancement math.
            piece_display = _piece_name_from_display(piece_power.display_name)
            if not piece_display:
                _PIECE_NAME_UNAVAILABLE.append(
                    f'{s.name}#{i + 1}: {piece_power.display_name!r} -> kept derived {derived_name!r}')
                piece_display = derived_name
            # Authoritative unique flag: the piece's `slot_requires`
            # contains a `BoostsSlotted>X <= 0` constraint when the game
            # enforces uniqueness for that piece (purples, ATOs, ATIO
            # globals, etc.). Empty / level-only slot_requires means the
            # piece is freely slottable across multiple powers.
            slot_req = _requires_text(piece_power.slot_requires).lower()
            is_unique = 'boostsslotted>' in slot_req
            # Effective aspect count, recovered from the enhancement scale (the
            # game's authoritative dilution). Emit `totalAspects` only when it
            # exceeds the aspect-list length — i.e. the piece carries hidden
            # global/proc segments that dilute its named aspect (LotG +Recharge,
            # ATO "#6" Recharge/Chance). When it's <= len(aspects) the aspect
            # list already accounts for the dilution, so no override is needed;
            # a derived count BELOW the list length signals a spurious extracted
            # aspect (logged for review, not emitted as a misleading override).
            rarity_mult = 1.25 if s.rarity in SUPERIOR_RARITIES else 1.0
            eff_count = _derive_effective_aspect_count(enh_scales, rarity_mult)
            total_aspects = None
            if eff_count is not None and eff_count > len(aspects):
                total_aspects = eff_count
            elif eff_count is not None and eff_count < len(aspects):
                _ASPECT_COUNT_UNDERSHOOTS.append(
                    f'{s.name}#{i + 1}: derived {eff_count} < {len(aspects)} aspects {aspects}')
            piece = {
                'num': i + 1,
                'name': piece_display,
                'aspects': aspects,
                'proc': is_proc,
                'unique': is_unique,
            }
            if total_aspects is not None:
                piece['totalAspects'] = total_aspects
            pieces.append(piece)

        # Build bonuses. A tier may reference MULTIPLE auto-powers (e.g. the
        # six per-mez-type duration powers behind a "+Mez Duration" ATO bonus,
        # each a separate Set_Bonus.* power at a different scale); their effects
        # are additive, so aggregate across ALL of them — not just the first —
        # de-duping identical (stat, value) entries.
        #
        # A PvP set states TWO tiers at one piece count — the PvE bonus and the PvP-only
        # one — and the planner's tier is keyed by count, so both land in one entry with
        # the PvP half flagged. The flag is what the calc gates on, so it belongs in the
        # de-dup key too: the same (stat, value) can legitimately appear as both halves.
        bonuses_by_pieces: dict[int, list[dict]] = {}
        seen_by_pieces: dict[int, set[tuple[str, float, bool]]] = {}
        for bn in s.bonuses:
            gate = _tier_gate(bn)
            if gate is None:
                continue
            tier_effects = bonuses_by_pieces.setdefault(bn.min_boosts, [])
            seen_effects = seen_by_pieces.setdefault(bn.min_boosts, set())
            for ap in bn.auto_powers:
                ap_power = power_index.get(ap) or power_index.get(f'Set_Bonus.Set_Bonus.{ap}')
                if not ap_power:
                    continue
                for eff in _resolve_bonus_effects(ap_power):
                    if gate == 'pvp':
                        eff['pvp'] = True
                    key = (eff['stat'], eff['value'], eff.get('pvp', False))
                    if key in seen_effects:
                        continue
                    seen_effects.add(key)
                    tier_effects.append(eff)
        bonuses_out = [
            {'pieces': pieces, 'effects': effects}
            for pieces, effects in bonuses_by_pieces.items()
            if effects
        ]

        # Build the set entry.
        set_id = s.name.lower().replace('-', '').replace('__', '_')
        out_sets[set_id] = {
            'name': display,
            'category': rarity,
            'rarity': s.rarity,
            'type': type_,
            'minLevel': s.min_level or 1,
            'maxLevel': s.max_level or 50,
            'attunedOnly': _attuned_only(s),
            'bonuses': bonuses_out,
            'pieces': pieces,
            'icon': _resolve_icon(set_id, hc_sets.get(set_id, {}).get('icon')),
        }

    return out_sets, skipped


def _attuned_only(record: BoostSetRecord) -> bool:
    """Whether the game ships this set ONLY attuned — no craftable variant exists.

    The export states it per piece and nowhere else: every BoostList member is a
    boost record named `Crafted_*`, `Attuned_*` or `Superior_Attuned_*`, and a set
    with no `Crafted_*` member has nothing to slot at a fixed level. Uniform across
    a set's pieces on all four datasets, and asserted so, because a set that is
    half-craftable would mean the fact belongs on the piece and not here.

    This replaces two guesses. `maxLevel <= 1` was the marker the planner read, and
    it is right for most of the roster but misses the reward sets that keep a 10-50
    range; the hand list that patched those was keyed on DISPLAY NAME, so it called
    Winter's Gift attuned-only (it is not, on any fork — MBDEXPORT-14). A display
    name cannot carry the fact anyway: Thunderspy prints "Subaluwa" for two
    different records, its craftable `KB` set and an `Overwhelming_Force` one that
    is attuned-only, and they differ on exactly this field.
    """
    crafted_by_piece = []
    for i, bl in enumerate(record.boostlists):
        if not bl.boosts:
            continue
        kinds = set()
        for full in bl.boosts:
            name = full.split('.')[-1]
            low = name.lower()
            if low.startswith('crafted_'):
                kinds.add(False)
            elif low.startswith('attuned_') or low.startswith('superior_attuned_'):
                kinds.add(True)
            else:
                raise SystemExit(
                    f"{record.name} piece {i + 1}: boost '{name}' carries no "
                    f'crafted/attuned prefix, so the export no longer states whether '
                    f'the set is attuned-only'
                )
        crafted_by_piece.append(False in kinds)
    if not crafted_by_piece:
        raise SystemExit(
            f'{record.name}: states no piece membership, so the export says nothing '
            f'about whether it is attuned-only'
        )
    if len(set(crafted_by_piece)) > 1:
        raise SystemExit(
            f'{record.name}: pieces disagree on whether a crafted variant exists '
            f'({crafted_by_piece}) — attunement is a per-PIECE fact on this set, '
            f'not a per-set one'
        )
    return not crafted_by_piece[0]


def _resolve_icon(set_id: str, inherited: str | None) -> str:
    """The set's icon filename, or a hard failure saying which set has none.

    Icons aren't in the binary, so every one is curated: an ICON_OVERRIDES entry,
    or the icon HC's registry already states for the same set_id. There used to be
    a third arm, `f's{set_id}.png'`, and its own comment called it bogus — the name
    is in no asset library, so EnhancementIcon resolved it to a 404. It shipped one:
    Rebirth's `superior_winters_gift` requested `ssuperior_winters_gift.png` from
    the day the set was added until 2026-08-20, because the fallback answered
    plausibly instead of saying it had no answer.

    Rule 1: a soft-wrong value ships a broken icon as authoritative and nobody hears
    about it for months. Raising means the regen stops on the set that needs a
    curated icon and names it, which is a bug report rather than a lie.
    """
    icon = ICON_OVERRIDES.get(set_id) or inherited
    if not icon:
        raise SystemExit(
            f"no curated icon for set '{set_id}': icons are not in the binary, so add an "
            f"ICON_OVERRIDES entry pointing at a file under public/img/Enhancements/ "
            f"(the folder is chosen from the filename prefix by getIOSetFolder)"
        )
    return icon


def _reuse_hand_entry(out_sets: dict[str, dict], set_id: str, hc_entry: dict) -> None:
    """Borrow Homecoming's hand entry for a set this fork also ships.

    What it is borrowed FOR is the piece aspect lists: this fork's binary
    extraction drops the Accuracy aspect on many pieces, and HC's entry carries
    them. Everything the fork states for itself goes back on top: its icon
    override, its own rarity token, its own slotting `type`, its own piece
    names — and its own BONUS TIERS. The fork's game computes set bonuses from
    the fork's own defs, and the two authorings genuinely differ (BOOST-5
    step 2: HC re-rounded its defense scales to 4dp — 0.625 vs 0.63 — and its
    damage tiers carry a +0.025 offset — 2.525 vs the forks' 2.5), so shipping
    HC's values on a fork misstates what that fork's players see. A set where
    the fork resolved NO tiers keeps HC's as a stand-in and is reported loudly
    at end-of-run (_SHARED_BONUS_FALLBACK) rather than passing silently.

    The `type` has to survive. The forks spell four headings differently from
    Homecoming — "PBAoE Damage" for "Melee AoE Damage", "Targeted AoE Damage" for
    "Ranged AoE Damage", "Taunt" for "Threat Duration", "Rez Sets" for the rez set —
    and a set left carrying HC's spelling matches no power on this fork, so the
    picker offers it nowhere. That is the shape of the bug BOOST-2 closed.

    The names have to survive for the same reason one level down: a fork names its
    own pieces, and the strings differ. Homecoming's ATO pieces carry an archetype
    qualifier the other forks' don't ("Accuracy/Damage (Stalker)"), Rebirth calls
    one piece "Chance for Buildup" where Homecoming writes "Chance for Build Up",
    and Rebirth's Annoyance #1 is "Taunt" against Homecoming's "Taunt/Placate".
    """
    preserved_icon = ICON_OVERRIDES.get(set_id)
    binary_rarity = out_sets[set_id].get('rarity')
    binary_type = out_sets[set_id].get('type')
    binary_attuned_only = out_sets[set_id].get('attunedOnly')
    binary_names = {p.get('num'): p.get('name') for p in out_sets[set_id].get('pieces', [])}
    binary_bonuses = out_sets[set_id].get('bonuses')
    out_sets[set_id] = dict(hc_entry)
    if binary_bonuses:
        out_sets[set_id]['bonuses'] = binary_bonuses
    else:
        _SHARED_BONUS_FALLBACK.append(set_id)
    if preserved_icon:
        out_sets[set_id]['icon'] = preserved_icon
    if binary_rarity is not None:
        out_sets[set_id]['rarity'] = binary_rarity
    if binary_type:
        out_sets[set_id]['type'] = binary_type
    # Same reason as `rarity`: the fork's own binary owns it. The forks agree with
    # HC on every shared set today, so this preserves rather than changes — but the
    # fork is the one that gets to say.
    if binary_attuned_only is not None:
        out_sets[set_id]['attunedOnly'] = binary_attuned_only
    # Copied per piece, because `dict(hc_entry)` shares HC's piece dicts and the
    # later override passes mutate them in place.
    pieces = [dict(piece) for piece in out_sets[set_id].get('pieces', [])]
    for piece in pieces:
        own_name = binary_names.get(piece.get('num'))
        if own_name:
            piece['name'] = own_name
    out_sets[set_id]['pieces'] = pieces


def _apply_rebirth_overrides(out_sets: dict[str, dict], hc_sets: dict[str, dict], ctx: dict | None = None) -> dict:
    """Rebirth post-build passes: reuse HC's hand entry for shared sets, then
    layer the Rebirth-only piece curation. Returns a small stats dict."""
    # Override shared sets with HC's hand-curated entry, for the aspect lists
    # this fork's binary extraction drops (see _reuse_hand_entry).
    shared_overridden = 0
    for set_id in list(out_sets.keys()):
        hc_entry = hc_sets.get(set_id)
        if hc_entry:
            _reuse_hand_entry(out_sets, set_id, hc_entry)
            shared_overridden += 1

    # Apply curated aspects for Rebirth-only sets where the binary extraction is
    # incomplete (the Guardian ATOs lose their Accuracy aspect). Per-field, so a
    # piece keeps the name its boost power gave it.
    pieces_overridden = 0
    for set_id, pieces in PIECE_OVERRIDES.items():
        entry = out_sets.get(set_id)
        if not entry:
            continue
        by_num = {p['num']: p for p in pieces}
        for existing in entry.get('pieces', []):
            patch = by_num.get(existing.get('num'))
            if patch:
                existing.update(patch)
        pieces_overridden += 1

    # Apply per-piece aspect overrides (for wiki/in-game vs binary
    # discrepancies). Each override replaces the aspect list outright. The name
    # is left alone: it comes from the boost power, and the two are allowed to
    # disagree — the game names Cupid's Crush #1 "Damage/Recharge" over a piece
    # that also carries a PowerChanceMod aspect.
    for set_id, piece_overrides in REBIRTH_PIECE_ASPECT_OVERRIDES.items():
        entry = out_sets.get(set_id)
        if not entry:
            continue
        for p in entry.get('pieces', []):
            new_aspects = piece_overrides.get(p.get('num'))
            if new_aspects:
                p['aspects'] = _sort_aspects_canonical(list(new_aspects))

    # Apply field-level patches for pieces the binary can't characterize.
    for set_id, patches in REBIRTH_PIECE_PATCHES.items():
        entry = out_sets.get(set_id)
        if not entry:
            continue
        for p in entry.get('pieces', []):
            patch = patches.get(p.get('num'))
            if patch:
                p.update(patch)

    return {'shared_overridden': shared_overridden, 'pieces_overridden': pieces_overridden}


def _apply_homecoming_overrides(out_sets: dict[str, dict], hc_sets: dict[str, dict], ctx: dict | None = None) -> dict:
    """Homecoming post-build passes. HC IS the source, so there's no shared-set
    reuse — only the targeted overrides for what the binary can't reproduce:
      - whole-set: cupids_crush / overwhelming_force (binary skips them).
      - per-piece aspect: hypersonic #4 (+Fly Magnitude special).
      - global/special proc name + flag: pieces the binary can't characterize
        (LotG +Recharge, Steadfast +Def, +Run Speed/+Perception globals, …).
    All hand data is read from the existing HC io-sets-raw (hc_sets)."""
    stats = {'wholeset': 0, 'pieces_overridden': 0, 'missing': []}

    # Whole-set: bring in sets the binary doesn't yield at all. Their binary
    # records DO carry a rarity (the two-member ECUniversalDamage tier) —
    # stamp it so the copied hand entry matches the binary field.
    for set_id in HC_WHOLESET_SETS:
        hand = hc_sets.get(set_id)
        if hand:
            out_sets[set_id] = dict(hand)
            out_sets[set_id]['rarity'] = 'ECUniversalDamage'
            out_sets[set_id]['attunedOnly'] = _binary_attuned_only(set_id, ctx)
            out_sets[set_id]['bonuses'] = json.loads(json.dumps(hand.get('bonuses', [])))
            # The record is skipped for its rarity, not for its readability: its
            # boost powers name their pieces, so the hand entry's names give way.
            own_names = _binary_piece_names(set_id, ctx)
            out_sets[set_id]['pieces'] = [
                {**piece, 'name': own_names.get(piece.get('num'), piece.get('name'))}
                for piece in hand.get('pieces', [])
            ]
            # The hand entry names no mez types. The binary record is skipped for its
            # rarity, not for its bonuses, so its auto-powers still say which types an
            # `(all)` tier covers — read them rather than leave the calc guessing.
            by_tier = _binary_mez_types_by_tier(set_id, ctx)
            for bonus in out_sets[set_id]['bonuses']:
                for effect in bonus.get('effects', []):
                    if effect.get('stat') != 'mez_resistance_(all)' or effect.get('pvp'):
                        continue
                    types = by_tier.get(bonus.get('pieces'))
                    if types:
                        effect['mez_types'] = types
                    else:
                        _HAND_BONUS_UNTYPED.append(f'{set_id} {bonus.get("pieces")}pc (wholeset)')
            stats['wholeset'] += 1
        else:
            stats['missing'].append(f'{set_id} (wholeset)')

    # Per-piece aspect overrides. Aspects only — the name stays the game's, and
    # the two are allowed to disagree: the game calls Sudden Acceleration #6
    # "Knockback to Knockdown" over a piece whose aspect is the raw attrib token
    # KnockToKnockDown, and names Cupid's Crush by the aspects a player enhances
    # rather than every attrib the template carries.
    for set_id, piece_overrides in HC_PIECE_ASPECT_OVERRIDES.items():
        entry = out_sets.get(set_id)
        if not entry:
            stats['missing'].append(f'{set_id} (piece override target absent)')
            continue
        for p in entry.get('pieces', []):
            new_aspects = piece_overrides.get(p.get('num'))
            if new_aspects:
                p['aspects'] = _sort_aspects_canonical(list(new_aspects))
                stats['pieces_overridden'] += 1

    # The `proc` flag for global/special pieces (HC_PIECE_PATCHES).
    for set_id, patches in HC_PIECE_PATCHES.items():
        entry = out_sets.get(set_id)
        if not entry:
            stats['missing'].append(f'{set_id} (proc patch target absent)')
            continue
        for p in entry.get('pieces', []):
            patch = patches.get(p.get('num'))
            if patch:
                p.update(patch)
                stats['proc_restored'] = stats.get('proc_restored', 0) + 1

    return stats


# ---------------------------------------------------------------------------
# Thunderspy-only set derivation (display-name based)
#
# Thunderspy's powers.bin uses an older AttribMod schema that DOESN'T carry the
# enum `aspect` field HC/Rebirth store — so build_sets' aspect-based derivation
# discards every tspy boost piece ("Chance"/"Empty", no aspects) and every set
# bonus resolves empty. But Thunderspy DOES ship the authoritative display
# strings in clientmessages, and those fully specify the data:
#   - each boost piece's display_name → "SetName: Accuracy/Damage/Endurance"
#   - each Set_Bonus power's internal name → the standard CoH bonus + its scale.
# So for the (few) Thunderspy-only sets we rebuild pieces + bonuses from those
# strings instead of the missing aspect enum. Shared sets still reuse HC's
# curated entry, so this path only runs for genuinely tspy-only sets.
# ---------------------------------------------------------------------------

# Enhancement-aspect vocabulary (matches HC piece `aspects` tokens).
_TSPY_ASPECT_TOKENS = {
    'accuracy': 'Accuracy', 'damage': 'Damage', 'endurance': 'Endurance',
    'recharge': 'Recharge', 'defense': 'Defense', 'tohit': 'ToHit',
    'range': 'Range', 'heal': 'Healing', 'mez': 'Mez',
}

# Thunderspy stores each set bonus' scale on the 'Ones'/'HealSelf' pseudo-attrib;
# the actual stat lives only in the Set_Bonus power's internal name. Map the
# name stem (tier suffix _N stripped) → planner (stat_key, multiplier). Values
# are scale × multiplier, cross-checked against HC's standard set bonuses:
# Increased_Damage_5 (.03 → +3% Damage), Increased_Health_3 (.15×10 → +1.5% Max
# HP), Increased_Energy_Neg_Ranged_Def_3 (.025 → +2.5% E/N def + 1.25% ranged,
# matching Overwhelming Force), Improved_Recharge_Time_7 (.1 → +10% recharge).
# ranged-def is half the energy value (×50). Endurance-drain-resistance and
# offensive-knockback have no planner stat key (harmless — the planner ignores
# unmapped keys); KB_Combo is the Slammed combo counter, no stat.
_TSPY_BONUS_STEMS: dict[str, list[tuple[str, float]]] = {
    'Increased_Damage':                [('damage', 100.0)],
    'Increased_Health':                [('maximum_hitpoints', 10.0)],
    'Increased_Energy_Neg_Ranged_Def': [('defense_(energy)', 100.0), ('defense_(ranged)', 50.0)],
    'Improved_Recharge_Time':          [('recharge', 100.0)],
    # Only the alpha-first of a paired resist (lethal↔smashing) — the planner's
    # set-bonus math auto-adds the paired member, so emitting both double-counts.
    'Lethal_Smash_Mez_Res':            [('damage_resistance_(lethal)', 100.0),
                                        ('mez_resistance_(all)', 100.0)],
    'Endurance_Drain_Resistance':      [('endurance_drain_resistance', 100.0)],
    'Improved_Knockback':              [('knockback_strength', 100.0)],
    # 'KB_Combo' — Slammed combo counter, no stat effect (intentionally absent).
}

_TSPY_STAT_DESC = {
    'damage': 'Damage',
    'maximum_hitpoints': 'Maximum HitPoints',
    'defense_(energy)': 'Energy and Negative Energy',
    'defense_(ranged)': 'Ranged Defense',
    'recharge': 'Recharge Time',
    'damage_resistance_(lethal)': 'Smashing and Lethal Resistance',
    'damage_resistance_(smashing)': 'Smashing Resistance',
    'mez_resistance_(all)': 'Mez Resistance',
    'endurance_drain_resistance': 'Endurance Drain Resistance',
    'knockback_strength': 'Knockback',
}

# Sets no other fork ships, so no hand-curated Homecoming entry can supply their
# pieces or bonus values. They take the rebuild path below, which reads both out of
# tspy's own display strings and scales. Everything else about them — the slotting
# `type` included — is an ordinary read of the record.
_TSPY_ONLY_SETS: frozenset[str] = frozenset({
    'kb',
    'primalists_nature',
    'superior_primalists_nature',
})


def _power_display_name(power) -> str:
    # display_name arrives pre-resolved from export_powers.py.
    return getattr(power, 'display_name', '') if power else ''


def _tspy_piece_from_boost(boost_full_name: str, num: int, power_index) -> dict | None:
    """Build one io-set piece from a Thunderspy boost power's authoritative
    display name (e.g. 'Subaluwa: Accuracy/Damage/Endurance' → aspects
    [Accuracy, Damage, Endurance]). Proc / special-global pieces ('Chance for
    Knockback', 'Recharge/Primal Energy Bonus') are flagged proc=true."""
    disp = _power_display_name(power_index.get(boost_full_name))
    part = _piece_name_from_display(disp)
    if not part:
        _PIECE_NAME_UNAVAILABLE.append(f'{boost_full_name} (tspy-only set #{num}): {disp!r}')
        return None
    low = part.lower()
    is_proc = ('chance for' in low) or low.endswith('bonus')
    aspects: list[str] = []
    for tok in part.split('/'):
        key = _TSPY_ASPECT_TOKENS.get(tok.strip().lower())
        if key:
            aspects.append(key)
    return {
        'num': num,
        'name': part,
        'aspects': _sort_aspects_canonical(aspects) if aspects else [],
        'proc': is_proc,
        'unique': False,
    }


def _tspy_bonus_effects(bonus_full_name: str, power_index) -> list[dict]:
    """Derive a Thunderspy set bonus' planner effects from its internal name
    (the stat) + its readable binary scale (the value). Returns [] for
    special/unmapped bonuses (e.g. the KB_Combo counter)."""
    p = power_index.get(bonus_full_name)
    if not p:
        return []
    stem = re.sub(r'_\d+$', '', bonus_full_name.split('.')[-1])
    mapping = _TSPY_BONUS_STEMS.get(stem)
    if not mapping:
        return []
    scale = 0.0
    for eg in (p.effects or []):
        for t in (eg.templates or []):
            if t.scale:
                scale = abs(t.scale)
                break
        if scale:
            break
    if not scale:
        return []
    effects = []
    for stat, mult in mapping:
        value = round(scale * mult, 4)
        label = _TSPY_STAT_DESC.get(stat, stat.replace('_', ' ').title())
        effect = {'stat': stat, 'value': value, 'desc': f'+{value}% {label}'}
        # The stat came from the name stem, but the types it stands for still come
        # from the power's own attribs — same warrant as the main collapse (all six
        # present), so there is one rule for what `(all)` means, not two.
        if stat == 'mez_resistance_(all)':
            types = _mez_types_of(p)
            if types:
                effect['mez_types'] = types
        effects.append(effect)
    return effects


def _tspy_build_only_set(set_id: str, record, power_index, prior: dict) -> dict:
    """Rebuild a Thunderspy-only set entry from authoritative display strings."""
    display = _power_display_name(
        next((power_index.get(bl.boosts[0]) for bl in record.boostlists if bl.boosts), None),
    )
    set_name = prior.get('name') or (display.split(':', 1)[0].strip() if ':' in display else record.name.replace('_', ' '))

    pieces = []
    for i, bl in enumerate(record.boostlists):
        if not bl.boosts:
            continue
        piece = _tspy_piece_from_boost(bl.boosts[0], i + 1, power_index)
        if piece:
            pieces.append(piece)

    bonuses = []
    for b in record.bonuses:
        if _tier_gate(b) != 'base':
            continue
        for ap in b.auto_powers:
            effects = _tspy_bonus_effects(ap, power_index)
            if effects:
                bonuses.append({'pieces': b.min_boosts, 'effects': effects})
            break

    return {
        'name': set_name,
        'category': prior.get('category', 'uncommon'),
        'rarity': record.rarity,
        'type': _resolve_category(record),
        'minLevel': prior.get('minLevel', record.min_level or 1),
        'maxLevel': prior.get('maxLevel', record.max_level or 50),
        # Not `prior.get` — this fork states it for itself, and the reason the
        # hand list it replaces was wrong is that it read one fork's answer onto
        # another's set of the same shape.
        'attunedOnly': _attuned_only(record),
        'bonuses': bonuses,
        'pieces': pieces,
        # ICON_OVERRIDES first: tspy-only sets have no HC set_id to inherit an
        # icon from, so without an entry here there is nothing to inherit and
        # _resolve_icon stops the run naming the set.
        'icon': _resolve_icon(set_id, prior.get('icon')),
    }


def _apply_thunderspy_overrides(out_sets: dict[str, dict], hc_sets: dict[str, dict], ctx: dict | None = None) -> dict:
    """Thunderspy post-build passes. Mirrors Rebirth's shared-set reuse (HC's
    piece aspect lists win for any set that also exists on HC; bonus tiers stay
    tspy's own where its build resolved any — BOOST-5), then REBUILDS the
    Thunderspy-only sets (Subaluwa, the Primalist ATOs) from their authoritative
    clientmessages display strings, since tspy's AttribMod format doesn't carry
    the enum aspect field build_sets relies on. The 4 sets that shouldn't be on
    Thunderspy (Sudden Acceleration, Synapse's Shock, Power Transfer, Hypersonic)
    plus 13 other HC-only sets are absent automatically — they aren't in tspy's
    boostsets.bin."""
    shared_overridden = 0
    for set_id in list(out_sets.keys()):
        hc_entry = hc_sets.get(set_id)
        if hc_entry:
            _reuse_hand_entry(out_sets, set_id, hc_entry)
            shared_overridden += 1

    rebuilt = []
    wholeset = 0
    if ctx:
        by_id = {s.name.lower().replace('-', '').replace('__', '_'): s for s in ctx['sets']}

        # Whole-set injection (mirrors _apply_rebirth_overrides). build_sets can't
        # yield the universal-damage sets: HC/Rebirth skip them as ECUniversalDamage,
        # and tspy's `Overwhelming_Force` record states no conversion groups at all,
        # so its rarity is empty and it drops as unmapped-rarity. But tspy DOES ship
        # Overwhelming Force in-game (a natively-attuned universal-damage set,
        # verified in the AH), so restore it from HC's hand entry. Gate on the record
        # actually existing in tspy's boostsets.bin — OF does, Cupid's Crush does NOT
        # (don't inject a set that isn't on the server). This is SEPARATE from
        # Subaluwa (a distinct tspy-only crafted knockback set, rebuilt below) — both
        # are real, so both must appear.
        for set_id in HC_WHOLESET_SETS:
            hand = hc_sets.get(set_id)
            if hand and set_id in by_id:
                out_sets[set_id] = dict(hand)
                # The tspy record states no conversion groups, so it carries no
                # rarity of its own; stamp the tier the set actually belongs to.
                out_sets[set_id]['rarity'] = 'ECUniversalDamage'
                # It does state a GroupName, though, so the slotting type is read
                # rather than inherited from the hand entry — same reason as
                # `_reuse_hand_entry`.
                out_sets[set_id]['type'] = _resolve_category(by_id[set_id])
                out_sets[set_id]['attunedOnly'] = _attuned_only(by_id[set_id])
                wholeset += 1

        for set_id in _TSPY_ONLY_SETS:
            record = by_id.get(set_id)
            if not record:
                continue
            out_sets[set_id] = _tspy_build_only_set(
                set_id, record, ctx['power_index'], out_sets.get(set_id, {}),
            )
            rebuilt.append(set_id)

    return {'shared_overridden': shared_overridden, 'wholeset': wholeset,
            'rebuilt_tspy_only': rebuilt}


# Per-dataset wiring: committed export dir, output path, and which override
# pass to run after build_sets().
DATASET_CONFIG = {
    'rebirth': {
        'export_dir': EXPORT_DIRS['rebirth'],
        'output': OUTPUT_PATH,
        'server': 'Rebirth',
        'apply_overrides': _apply_rebirth_overrides,
        'extra_notes': (
            ' * Includes Rebirth-only sets (Guardian\'s Gift, Absolute Resolution,\n'
            ' * Halloween + Winter event sets, Liberty\'s Belt, etc.) that aren\'t in\n'
            ' * HC\'s curated io-sets-raw. Shared sets reuse HC\'s piece aspect lists;\n'
            ' * bonus tiers are resolved from Rebirth\'s own export (BOOST-5).\n'
        ),
    },
    'homecoming': {
        'export_dir': EXPORT_DIRS['homecoming'],
        'output': HC_IO_SETS_PATH,
        'server': 'Homecoming',
        'apply_overrides': _apply_homecoming_overrides,
        'extra_notes': (
            ' * Targeted hand overrides (from the prior curated io-sets-raw) cover\n'
            ' * what the binary can\'t reproduce: the cupids_crush / overwhelming_force\n'
            ' * universal-damage sets. Bonus VALUES are binary-sourced throughout,\n'
            ' * PvP tiers included (BONUS-REQ-1).\n'
        ),
    },
    'brainstorm': {
        'export_dir': EXPORT_DIRS['brainstorm'],
        'output': BRAINSTORM_IO_SETS_PATH,
        'server': 'HC Brainstorm',
        # The same override pass as Homecoming, because it is the same game one shard over.
        # The bonus VALUES still come from brainstorm's own boostsets.bin, which is the half
        # that matters: BOOST-5 closed on shared sets shipping FORK-OWN tiers, and a beta that
        # rebalances a set must show its own numbers rather than live's.
        'apply_overrides': _apply_homecoming_overrides,
        'extra_notes': (
            ' * Homecoming\'s open beta. Same hand overrides as Homecoming (the cupids_crush /\n'
            ' * overwhelming_force universal-damage sets); every bonus value is read from this\n'
            ' * shard\'s own boostsets.bin, so a set i28p4 retunes shows the retuned tier.\n'
        ),
    },
    'thunderspy': {
        'export_dir': EXPORT_DIRS['thunderspy'],
        'output': THUNDERSPY_IO_SETS_PATH,
        'server': 'Thunderspy',
        'apply_overrides': _apply_thunderspy_overrides,
        'extra_notes': (
            ' * Shared sets reuse HC\'s hand-curated entry. The 4 sets that are NOT on\n'
            ' * Thunderspy (Sudden Acceleration, Synapse\'s Shock, Power Transfer,\n'
            ' * Hypersonic) plus 13 other HC-only sets are absent because they aren\'t\n'
            ' * in tspy\'s boostsets.bin. Thunderspy-only sets (Subaluwa, the Primalist\n'
            ' * ATOs) are rebuilt from their clientmessages display strings — tspy\'s\n'
            ' * AttribMod format lacks the enum aspect field, so piece aspects come\n'
            ' * from the boost display names and bonus values from the Set_Bonus names\n'
            ' * + binary scales (see _apply_thunderspy_overrides).\n'
        ),
    },
}


def _file_header(cfg: dict, total: int) -> str:
    export_dir_rel = cfg['export_dir'].relative_to(PROJECT_ROOT).as_posix()
    return f'''/**
 * {cfg['server']} IO Set data — extracted from the committed bin-crawler export.
 *
 * Auto-generated by `scripts/extract-rebirth-io-sets-v2.py --dataset {cfg['_id']}`.
 * Do not hand-edit.
 *
 * Source: {export_dir_rel}/ (produced by `bin_crawler.export_powers`)
 *   - boostsets.json → set metadata + piece refs + bonus refs + levels
 *   - boosts/**      → boost-piece aspects (Boosts.X.X power templates)
 *   - set_bonus/**   → bonus values (Set_Bonus.X.X power templates)
 *
 * Total sets: {total}
{cfg['extra_notes']} *
 * Set-bonus values are scale × a per-attrib multiplier (damage ×250 on HC's
 * SetBonusPetShare table and ×100 on the forks' Melee_Ones re-authoring, max
 * HP ×10, max endurance ×1, everything else ×100) and use the planner-canonical
 * stat keys (see contract/set-bonus-stat-vocab.json). Proc-piece names are
 * heuristic and may read "Chance" until resolved via the trigger attribs.
 */

interface LegacyIOSetPiece {{
  num: number;
  name: string;
  aspects: string[];
  proc: boolean;
  unique: boolean;
  totalAspects?: number;
}}

interface LegacySetBonusEffect {{
  stat: string;
  value: number;
  desc: string;
  pvp?: boolean;
  /** The six mez types a `mez_resistance_(all)` bonus covers. The stat key
   *  names none of them and only the all-6 collapse mints it, so the types
   *  ride along here. The calc doesn't need them today: it spends `(all)`
   *  into one accumulator and fans that out to all six at display time. */
  mez_types?: string[];
}}

interface LegacySetBonus {{
  pieces: number;
  effects: LegacySetBonusEffect[];
}}

// A type alias rather than an interface: TS gives an alias an implicit index
// signature, so a set entry can be passed to a helper typed
// `Record<string, unknown>`. The roster tests' `sharedStructure` does exactly
// that, and an interface won't assign to it.
type LegacyIOSet = {{
  name: string;
  category: string;
  /** Binary rarity tier from boostsets.bin (feeds getSetRarityMultiplier). */
  rarity: string;
  type: string;
  minLevel: number;
  maxLevel: number;
  /** The game ships this set only attuned — no `Crafted_*` piece exists. */
  attunedOnly: boolean;
  bonuses: LegacySetBonus[];
  pieces: LegacyIOSetPiece[];
  icon: string;
}};

type LegacyIOSetRegistry = Record<string, LegacyIOSet>;

export const IO_SETS_RAW: LegacyIOSetRegistry = '''


def _parse_dataset_arg(argv: list[str]) -> str:
    """`--dataset <id>` / `--dataset=<id>`; defaults to rebirth (the historical
    behaviour, since the script is named extract-rebirth-io-sets)."""
    for i, a in enumerate(argv):
        if a == '--dataset' and i + 1 < len(argv):
            return argv[i + 1]
        if a.startswith('--dataset='):
            return a.split('=', 1)[1]
    return 'rebirth'


def main(dataset: str | None = None) -> int:
    dataset = dataset or _parse_dataset_arg(sys.argv[1:])
    cfg = DATASET_CONFIG.get(dataset)
    if not cfg:
        print(f'Unknown dataset {dataset!r}; choose from {sorted(DATASET_CONFIG)}')
        return 2
    cfg = {**cfg, '_id': dataset}

    export_dir = cfg['export_dir']
    print(f'[{dataset}] Loading committed export from {export_dir}…')

    sets = _load_boostsets(export_dir)
    print(f'  {len(sets)} boostsets loaded')

    power_index = _load_boost_piece_powers(export_dir)
    print(f'  {len(power_index)} boost-piece/set-bonus power templates indexed')

    # HC's hand-curated io-sets-raw — used for icon fallback (both datasets),
    # Rebirth shared-set reuse, and HC's targeted bonus/whole-set overrides.
    hc_sets = _load_hc_sets()
    print(f'  {len(hc_sets)} HC hand sets loaded')

    out_sets, skipped = build_sets(sets, power_index, hc_sets)
    ctx = {'sets': sets, 'power_index': power_index}
    stats = cfg['apply_overrides'](out_sets, hc_sets, ctx)

    print(f'\n[{dataset}] Extracted {len(out_sets)} sets ({len(skipped)} skipped)')
    for k, v in stats.items():
        if k == 'missing':
            continue
        print(f'  {k}: {v}')
    if stats.get('missing'):
        print(f'  !! override targets missing: {stats["missing"]}')
    for sk in skipped[:8]:
        print(f'  skip: {sk}')

    if _UNMAPPED_BONUS_PAIRS:
        print(f'\n!! Dropped {sum(_UNMAPPED_BONUS_PAIRS.values())} bonus entries due to unmapped (attrib, aspect) pairs:')
        for (attrib, aspect), n in sorted(_UNMAPPED_BONUS_PAIRS.items(), key=lambda kv: -kv[1]):
            print(f'   ({attrib!r}, {aspect!r}) x {n}  -> add bridge/fallback mapping in _bonus_stat_from_bridge')

    if _ASPECT_COUNT_UNDERSHOOTS:
        print(f'\n!! {len(_ASPECT_COUNT_UNDERSHOOTS)} pieces: scale-derived count < extracted aspects (spurious-aspect candidates):')
        for u in _ASPECT_COUNT_UNDERSHOOTS:
            print(f'   {u}')

    if _HAND_BONUS_UNTYPED:
        print(f'\n!! {len(_HAND_BONUS_UNTYPED)} hand-curated mez_resistance_(all) tiers with no binary tier to take types from:')
        for u in _HAND_BONUS_UNTYPED:
            print(f'   {u}  -> the calc cannot spend this bonus until it names its types')

    if _PIECE_NAME_UNAVAILABLE:
        print(f'\n!! {len(_PIECE_NAME_UNAVAILABLE)} pieces have no readable display_name; the derived name stands:')
        for u in _PIECE_NAME_UNAVAILABLE:
            print(f'   {u}')

    if _SHARED_BONUS_FALLBACK:
        print(f'\n!! {len(_SHARED_BONUS_FALLBACK)} shared sets resolved NO bonus tiers of their own; HC\'s stand in:')
        for u in _SHARED_BONUS_FALLBACK:
            print(f'   {u}')

    header = _file_header(cfg, len(out_sets))
    body = json.dumps(out_sets, indent=2, sort_keys=True)
    cfg['output'].write_text(header + body + ';\n', encoding='utf-8')
    print(f'\nWrote {cfg["output"]}')
    return 0


if __name__ == '__main__':
    sys.exit(main())
