"""Parser for powers.bin — the most complex binary file.

Each power record has 122+ fields that MUST be read in strict sequential order.
Supports both Parse7 (HC/Homecoming) and Parse6 (retail/Rebirth) formats.
HC added 6 extra fields not present in Parse6:
  - 35b: u4 after ai_report
  - 38:  MaxTargetsExpr (string_array) + 38b-d: the OverCap block
         (OverCapTrigger u4 / OverCapMultiplier f4 / OverCapExponential u4)
  - 48b: f4 after time_to_activate = TimeToRoot (captured as time_to_root)
  - 52b: f4 after idea_cost = MaxToggleTime (captured as max_toggle_time)
Parse6 retains fields 53-56 (confirm dialog fields) in all records.
Parse6 has 8 bytes of box data (2×f4) instead of Parse7's 24 bytes (2×f4×3).

Post-2026 HC patch added 8 bytes before chain_delay (field 41b):
  - 41b: AreaFactor override (f4, 0 = none) + PPMMod (f4, default 1.0)
Auto-detected via _detect_format.
"""

import struct
from collections import Counter
from typing import NamedTuple
from ._reader import open_parse7, BinReader, Parse6BinReader
from ._dataclasses import PowerRecord, EffectGroup, EffectTemplate
from ._gate_default import evaluate as gate_evaluate
from ._requires import entity_scope
from ._enums import (
    BOOST_TYPE, BOOST_TYPE_REBIRTH, ATTRIB_NAME,
    EVENT_NAME, EVENT_NAME_PARSE6, resolve_attrib,
    resolve_attrib_rebirth, select_event_table, event_id_from_name,
    ATTRIB_MOD_TYPE, ATTRIB_MOD_ASPECT, ATTRIB_MOD_APPLICATION,
    ATTRIB_MOD_TARGET, ATTRIB_MOD_STACK, ATTRIB_MOD_CASTER_STACK,
    KNOCK_VEC_POSITION, NOTIFY_EVENT,
    SPECIAL_ATTRIB_MIN_REBIRTH,
    special_attrib_table, select_special_attrib_base, thunderspy_attrib_table,
)


# AttribMod flag bit assignments. Empirically derived by matching .powers
# source `Flags` keyword sets against the u4 stored after BoostModAllowed
# (12.5k def↔binary template pairs matched on power name + attrib + table +
# scale + magnitude, 2026-07-20 census; every bit below has recall 1.0 and
# ≤5 mismatched samples).
#
# Three bits the census couldn't see (thin .powers text coverage for
# incarnate/boost content) were named via the CoD2 oracle, which prints each
# template's flags with the game enum's index — e.g. "Boost (11)". CoD2's
# numbering matches these bit positions exactly below index 7 and runs one
# LOWER from there up, because its table lacks HC's IgnoreEfficiency at
# bit 7 (43 exported templates carry bit 7; CoD2 names none of them).
# Validated over 42.5k export↔CoD2 paired templates, 2026-07-20:
#   bit 1  BoostIgnoreDiminishing — CoD2 (1),  106 agree / 0 contradictions
#   bit 12 Boost                  — CoD2 (11), 252 agree / 0 contradictions
#   bit 19 StackExactPower        — CoD2 (18), positionally locked between
#          StackByAttribAndKey (bit 18, 496 agree) and IgnoreSuppressErrors
#          (bit 20, 24 agree); no in-corpus pair carries it (it lives almost
#          entirely on boost powers, outside exported_powers).
#
# Bits 8–11 (0x100/0x200/0x400/0x800) are the EFFECTIVE resist/combat-mod
# mode, baked in by the .powers compiler rather than authored as keywords:
# bit 8 ResistMagnitude, bit 9 ResistDuration, bit 10 CombatModMagnitude,
# bit 11 CombatModDuration. Magnitude-aspect mods default to 0x500,
# duration-aspect mods to 0xa00, and Ignore* keywords clear the matching
# resist bit — which is why they show up as doublets on templates with no
# Flags line. They are deliberately NOT decoded into `flags`: they're
# present on virtually every template, so naming them would bury the
# meaningful flags in noise. Consumers wanting effective resistibility read
# them from `flags_raw`. An earlier decode mistook 0x800 for DeepSleep —
# every genuine DeepSleep template is a duration mod, so recall looked
# perfect while the flag false-fired on every Hold/Stun/plain-Sleep in the
# corpus. Real DeepSleep lives in the second flags word (see below).
_FLAG_BITS: tuple[tuple[int, str], ...] = (
    (0x000001, "NoFloaters"),
    # This template's enhancement contribution bypasses ED (the 2/3-ignores-ED
    # split on very-rare incarnate alpha boosts is two templates, one with and
    # one without this bit).
    (0x000002, "BoostIgnoreDiminishing"),
    (0x000004, "CancelOnMiss"),
    (0x000008, "NearGround"),
    (0x000010, "IgnoreStrength"),
    (0x000020, "IgnoreResistance"),
    # The source enum (and CoD2) call this bit IgnoreLevelDifference; the
    # .powers Flags keyword is IgnoreCombatMods. Same semantics — don't scale
    # by attacker/target level difference — so keep the authored keyword name
    # here. Not a mislabel; don't "fix" it to the enum name.
    (0x000040, "IgnoreCombatMods"),
    (0x000080, "IgnoreEfficiency"),
    # Marks a boost (enhancement) template — Parse6 stores the same fact as
    # its ninth per-template bool, "BoostTemplate".
    (0x001000, "Boost"),
    (0x002000, "HideZero"),
    (0x004000, "KeepThroughDeath"),
    (0x008000, "DelayEval"),
    (0x010000, "NoHitDelay"),
    (0x020000, "NoProjectileDelay"),
    (0x040000, "StackByAttribAndKey"),
    (0x080000, "StackExactPower"),
    (0x100000, "IgnoreSuppressErrors"),
    (0x400000, "AlwaysPersist"),
)


def _decode_flags(flags_raw: int) -> list[str]:
    return [name for bit, name in _FLAG_BITS if flags_raw & bit]


# SECOND AttribMod flags word — the u4 stored immediately after the first
# flags word (`flags_raw`). The binary splits AttribMod flags across two
# consecutive u4s; attrib-specific keywords live in this second word, which
# the parser previously swallowed into the opaque Params tail.
#
# Unlike the first word, the second word's bits are a per-attrib-class UNION:
# the same bit encodes a different keyword depending on the template's attrib
# (bit 0 is DeepSleep on a Sleep mod but RevokeAll on a Revoke_Power mod), so
# decoding is gated on the attrib. Mapping from the 2026-07-20 def↔binary
# census (12.5k matched templates, recall 1.0 for every keyword below;
# per-class counts: DeepSleep 29, RevokeAll 52, SetTimer 24, AdjustTimer 33,
# Cooldown 30, DoNotTintCostume 83, CopyBoosts 577, CopyCreatorMods 42,
# ProcSeparately 10, PseudoPet 81, CopyCreatorCostume 6, VanishEntOnTimeout 1).
# Attrib classes not listed are left undecoded — the raw word is exported as
# `flags2_raw` so nothing is lost. (Repel's per-template booleans/floats do
# NOT live here — they're in its 40-byte Params record; the pre-FLAGS-2
# misaligned read of that record's length prefix as "flags2 = 0x28" is what
# once made it look like Repel packed bits into this word.)
_FLAG2_BITS_BY_ATTRIB: dict[str, tuple[tuple[int, str], ...]] = {
    "Create_Entity": (
        (0x000001, "VanishEntOnTimeout"),
        (0x000002, "DoNotTintCostume"),
        (0x000004, "CopyBoosts"),
        (0x000008, "CopyCreatorMods"),
        # CoD2 "NoCreatorModFX (36)" — flag indices ≥32 are second-word
        # bit (index − 32). Only carrier in CoD2's corpus is
        # Nictus.Inky_Summon (suppress the creator's power-customization FX
        # on the summoned entity).
        (0x000010, "NoCreatorModFX"),
        (0x000020, "PseudoPet"),
        (0x000100, "CopyCreatorCostume"),
    ),
    "Execute_Power": (
        (0x000004, "CopyBoosts"),
        (0x000008, "ProcSeparately"),
    ),
    "Grant_Power": (
        (0x000004, "CopyBoosts"),
        (0x000100, "CopyCreatorCostume"),
    ),
    # The boosted-grant variant of Grant_Power; only CopyBoosts is attested
    # (Redirects.Willpower.Resurgence pair, and 0x4 = CopyBoosts across all
    # three grant/summon/execute classes above).
    "Grant_Boosted_Power": ((0x000004, "CopyBoosts"),),
    "Recharge_Power": (
        (0x000001, "SetTimer"),
        (0x000002, "AdjustTimer"),
        (0x000004, "Cooldown"),
    ),
    "Sleep": ((0x000001, "DeepSleep"),),
    "Revoke_Power": ((0x000001, "RevokeAll"),),
    # CoD2 "NoTokenTime (32)": don't stamp the mode-token's grant time —
    # sole carrier is Objects.Lambda_Sector_SibylSummon.Summon_Sibyl, which
    # is also the corpus's only Set_Mode template with a second-word bit.
    "Set_Mode": ((0x000001, "NoTokenTime"),),
}


def _decode_flags2(flags2_raw: int, attribs: list[str]) -> list[str]:
    names: list[str] = []
    for attrib in attribs:
        for bit, name in _FLAG2_BITS_BY_ATTRIB.get(attrib, ()):
            if flags2_raw & bit and name not in names:
                names.append(name)
    return names


# Rate-limited "we dropped parsed data" warning. A swallowed parse failure
# used to be invisible — it hid the ~265-power Trip Mine misalignment for
# months. Every handler that discards a record, group, template, or redirect
# on exception routes through here: surface the first N by name + a final
# total so a future layout shift is loud, without spamming a full export run.
_dropped_powers: list[str] = []
_DROP_WARN_CAP = 25

# A Chance outside [0,1] is a value we can't read, not a record we lost. The
# source oracle rolls `fRand < fChance` with fRand drawn from [0,1), so a
# negative can never fire and a value above 1 always does — yet all three forks
# author the SAME out-of-range numbers on the SAME templates (Fiery Melee's
# -0.2, Deafening Wave's -0.80/-0.90/-0.93/-0.95), which rules out misalignment
# and leaves them data whose meaning is undecoded. They carry to the export as
# read; this counts them so a future decode has a number to chase and the
# unread state can't go quiet. Deliberately NOT _warn_dropped, whose contract is
# a record that was lost: nothing here is dropped, and inflating the drop count
# would assert a falsehood.
_out_of_range_chances: list[str] = []
_CHANCE_SAMPLE_CAP = 5

# Format detection scores the first 20 records under deliberately wrong
# layouts; the resulting parse failures are the detection mechanism, not
# data loss. Suppress warnings while probing so they can't masquerade as
# real drops.
_probing = False


def _warn_dropped(full_name: str, reason: str) -> None:
    import sys
    if _probing:
        return
    _dropped_powers.append(full_name)
    if len(_dropped_powers) <= _DROP_WARN_CAP:
        print(f"  Warning: {full_name or '<unknown power>'}: {reason}", file=sys.stderr)
    elif len(_dropped_powers) == _DROP_WARN_CAP + 1:
        print("  Warning: … further dropped-data warnings suppressed "
              "(see total at end)", file=sys.stderr)


def _note_chances(full_name: str, group_chance: float, templates) -> None:
    """Record any Chance the roll can't consume, on the group or its templates."""
    if _probing:
        return
    candidates = [('group', group_chance)]
    candidates += [('template', t.tick_chance) for t in templates]
    for where, value in candidates:
        if value is None or 0.0 <= value <= 1.0:
            continue
        _out_of_range_chances.append(
            f"{full_name or '<unknown power>'} ({where} chance {value})")


def _detect_format(r: BinReader) -> tuple[bool, bool, bool, bool]:
    """Detect Parse7 powers.bin layout variant.

    Tests the first 20 records across HC's 4 combinations of (has_41b, has_45b),
    Thunderspy's Parse6-derived layout, and Veracity's variant. Wrong layout
    shifts reads by 4+ bytes, producing implausible numeric values. Returns
    (has_field_41b, has_field_45b, is_thunderspy, is_veracity). is_thunderspy /
    is_veracity are mutually exclusive with the HC flags and each other; when
    either is True, the HC flags are False.
    """
    save_pos = r.pos

    def _score(parser) -> int:
        global _probing
        r._pos = save_pos
        score = 0
        _probing = True
        try:
            for _ in range(20):
                rec_len = r.read_u4()
                sub = r.sub_reader(rec_len)
                try:
                    pw = parser(sub)
                    if (0 <= pw.range <= 500
                            and 0 <= pw.recharge_time <= 3600
                            and 0 <= pw.endurance_cost <= 500
                            and 0 <= pw.time_to_activate <= 30):
                        score += 1
                except Exception:
                    pass
                r.skip(rec_len)
        finally:
            _probing = False
        return score

    best = (False, False, False, False)
    best_score = -1
    # Try the 4 HC combos
    for has_41b in (False, True):
        for has_45b in (False, True):
            score = _score(lambda sub, a=has_45b, b=has_41b:
                           _parse_power(sub, has_field_45b=a, has_field_41b=b))
            if score > best_score:
                best_score = score
                best = (has_41b, has_45b, False, False)
    # Try Thunderspy layout
    score = _score(lambda sub: _parse_power_parse6(sub, thunderspy=True))
    if score > best_score:
        best_score = score
        best = (False, False, True, False)
    # Try Veracity layout
    score = _score(lambda sub: _parse_power_parse6(sub, veracity=True))
    if score > best_score:
        best_score = score
        best = (False, False, False, True)
    r._pos = save_pos
    return best


def detect_dataset_flavor(bin_path_or_data) -> str:
    """Classify a powers.bin by dataset flavor: 'rebirth' (Parse6),
    'thunderspy', 'veracity', or 'homecoming' (stock Parse7 layouts).

    Exposes the same detection parse_powers runs internally, for callers
    (the export layer) that must pick dataset-correct enum tables when
    parsing sibling bins that carry no flavor marker of their own
    (dim_returns.bin references boost/attrib enums but is layout-identical
    across flavors).
    """
    r = open_parse7(bin_path_or_data)
    if isinstance(r, Parse6BinReader):
        return "rebirth"
    r.read_u4()  # block size
    r.read_u4()  # record count
    _, _, is_thunderspy, is_veracity = _detect_format(r)
    if is_thunderspy:
        return "thunderspy"
    if is_veracity:
        return "veracity"
    return "homecoming"


def parse_powers(bin_path_or_data, *, player_classes=()) -> list[PowerRecord]:
    r = open_parse7(bin_path_or_data)
    is_parse6 = isinstance(r, Parse6BinReader)

    # Unused by name, load-bearing by position: read_u4 advances the cursor. See the same
    # note in _powercats.py — deleting this line misaligns every power that follows.
    _block_size = r.read_u4()
    count = r.read_u4()

    if is_parse6:
        parser = lambda sub: _parse_power_parse6(sub, player_classes=player_classes)
        is_thunderspy = is_veracity = False
    else:
        # Auto-detect HC format version by testing the first few records.
        # Two known post-release additions that shift subsequent field offsets:
        #   - field 45b (u4 between box_size and range) added in a past HC patch
        #   - field 41b (8 bytes before chain_delay) added in a 2026 patch
        # Wrong layout produces implausible values (negative range, huge recharge).
        has_41b, has_45b, is_thunderspy, is_veracity = _detect_format(r)
        if is_thunderspy:
            # Thunderspy: Parse7 wrapper, Parse6-derived schema with HC-style
            # box (24 bytes) and no field 43b. Reuses _parse_power_parse6.
            parser = lambda sub: _parse_power_parse6(sub, thunderspy=True,
                                                     player_classes=player_classes)
        elif is_veracity:
            # Veracity: Parse7 wrapper, base retail field set + Dictionary,
            # 43b present, box 24, HC-style EffectGroup effects.
            parser = lambda sub: _parse_power_parse6(sub, veracity=True,
                                                     player_classes=player_classes)
        else:
            parser = lambda sub: _parse_power(sub, has_field_45b=has_45b, has_field_41b=has_41b,
                                              player_classes=player_classes)

    global _undecoded_message_fx_tails, _undecoded_params_tails, _undecoded_parse6_tails
    global _undecoded_thunderspy_tails, _misaligned_thunderspy_stack

    records_start = r._pos

    def _read_all_records():
        """One full pass over the record block. Re-runnable from the top."""
        global _undecoded_message_fx_tails, _undecoded_params_tails
        global _undecoded_parse6_tails, _undecoded_thunderspy_tails
        global _misaligned_thunderspy_stack
        r._pos = records_start
        _dropped_powers.clear()
        _out_of_range_chances.clear()
        _undecoded_message_fx_tails = 0
        _undecoded_params_tails = 0
        _undecoded_parse6_tails = 0
        _undecoded_thunderspy_tails = 0
        _misaligned_thunderspy_stack = 0
        out = []
        dropped = 0
        for i in range(count):
            rec_len = r.read_u4()
            sub = r.sub_reader(rec_len)

            try:
                out.append(parser(sub))
            except Exception as e:
                # Log but continue — don't let one bad record stop everything
                dropped += 1
                if dropped <= 5:
                    import sys
                    print(f"  Warning: record {i} parse error: {e}", file=sys.stderr)

            r.skip(rec_len)
        return out, dropped

    if is_thunderspy:
        # The special-attrib band's base is sizeof(CharacterAttributes), which a
        # fork moves by adding an attrib — Thunderspy did on 2026-07-30. Nothing
        # in the file announces it (the header checksum is content-derived and
        # the LAYOUT is unchanged, so _detect_format still passes), so calibrate
        # from the corpus: parse once, look at the raw attrib values actually
        # used, then re-parse if they say the band moved.
        #
        # A second pass is needed rather than a post-hoc rename because the base
        # also drives the summon marker, and that scan decides which templates
        # EXIST — it cannot be corrected after the fact.
        _set_tspy_band_base(SPECIAL_ATTRIB_MIN_REBIRTH)
        _TSPY_RAW_ATTRIB_COUNTS.clear()
        records, dropped_record_count = _read_all_records()
        calibrated = select_special_attrib_base(_TSPY_RAW_ATTRIB_COUNTS)
        if calibrated != _TSPY_BAND_BASE:
            import sys
            print(f"  Note: Thunderspy special-attrib band calibrated to base "
                  f"{calibrated} (default {_TSPY_BAND_BASE}); re-parsing",
                  file=sys.stderr)
            _set_tspy_band_base(calibrated)
            _TSPY_RAW_ATTRIB_COUNTS.clear()
            records, dropped_record_count = _read_all_records()
    else:
        records, dropped_record_count = _read_all_records()

    if dropped_record_count:
        import sys
        print(f"  Warning: {dropped_record_count} power record(s) failed to parse "
              f"and were dropped entirely", file=sys.stderr)
    if _dropped_powers:
        import sys
        print(f"  Warning: {len(_dropped_powers)} dropped-data incident(s) across "
              f"{len(set(_dropped_powers))} power(s) due to parse failures "
              f"(see GAME-DATA-PRINCIPLES §5)", file=sys.stderr)
    if _out_of_range_chances:
        import sys
        print(f"  Warning: {len(_out_of_range_chances)} Chance value(s) outside [0,1]; "
              f"carried to the export as read and left undecoded", file=sys.stderr)
        for sample in _out_of_range_chances[:_CHANCE_SAMPLE_CAP]:
            print(f"    {sample}", file=sys.stderr)
    if _undecoded_message_fx_tails:
        import sys
        print(f"  Warning: {_undecoded_message_fx_tails} template(s) had a Messages/FX "
              f"tail that didn't match the validated layout; left undecoded in the "
              f"params tail (see _read_template_messages_and_fx)", file=sys.stderr)
    if _undecoded_params_tails:
        import sys
        print(f"  Warning: {_undecoded_params_tails} template(s) had a Params union "
              f"that didn't match the validated layouts; fell back to the heuristic "
              f"scanner (see _read_params_union)", file=sys.stderr)
    if _undecoded_parse6_tails:
        import sys
        print(f"  Warning: {_undecoded_parse6_tails} Parse6 template(s) had a "
              f"post-magnitude tail that didn't match the validated layout; fell "
              f"back to the heuristic scanner (see _read_parse6_template_tail)",
              file=sys.stderr)
    if _undecoded_thunderspy_tails:
        import sys
        print(f"  Warning: {_undecoded_thunderspy_tails} Thunderspy sub-record(s) "
              f"had a post-stack tail that didn't match the validated layout; "
              f"flags read at the fixed offset and Params left undecoded "
              f"(see _read_thunderspy_template_tail)", file=sys.stderr)
    if _misaligned_thunderspy_stack:
        import sys
        print(f"  Warning: {_misaligned_thunderspy_stack} Thunderspy sub-record(s) "
              f"had a DelayedRequires count with no room for the stack quadruple "
              f"after it; caster_stack/stack/limit/key left undecoded rather than "
              f"read from a shifted offset", file=sys.stderr)

    if not (is_parse6 or is_thunderspy or is_veracity):
        recalibrate_event_names(records)

    _resolve_self_only_gates(records, player_classes)

    return records


def _resolve_self_only_gates(records, player_classes) -> None:
    """Re-read `entref target> entref source> eq` gates on Self-only powers.

    A group's verdict is taken without the power around it, so the gate
    evaluator assumes the recipient is not the caster. That is wrong exactly
    when the power affects nobody else: there the clause always holds.

    Boosts are excluded. A boost's templates land on whatever its host power
    affects, so its own `TargetsAffected Self` says nothing about the recipient
    (Power Transfer's self-heal gate is the case that shows it).
    """
    for rec in records:
        if rec.full_name.lower().startswith("boosts."):
            continue
        affected = [rec.target_type_table.get(v) for v in rec.targets_affected]
        if affected != ["Self"]:
            continue
        for g in _iter_groups(list(rec.effects) + list(rec.activation_effects)):
            if "entref" not in [t.lower() for t in g.requires_expression]:
                continue
            gate = gate_evaluate(g.requires_expression, player_classes=player_classes,
                                 recipient_is_caster=True)
            g.requires_default = gate.verdict
            g.requires_archetypes = list(gate.archetypes)


def _iter_groups(groups):
    """Yield every EffectGroup in `groups`, including nested children."""
    for g in groups or []:
        yield g
        yield from _iter_groups(getattr(g, "child_groups", None))


def recalibrate_event_names(records) -> str | None:
    """Re-resolve HC event names against the generation this build actually uses.

    Templates are named at parse time with the live table, which is a guess:
    HC silently renumbers the event enum when a patch inserts an event, and a
    stale table keeps resolving — to the NEIGHBOURING name. Wrong-but-plausible
    output is worse than an error, so once the whole corpus is parsed we look
    at which ids actually occur and re-resolve with the table that explains
    them (see select_event_table).

    Runs on HC layouts only; Parse6/Thunderspy/Veracity resolve through
    EVENT_NAME_PARSE6, which has its own numbering.

    Returns the chosen generation name, or None when the build carries no
    events at all. Rewrites names in place; a no-op when the live table wins,
    so live exports are byte-identical.
    """
    from collections import Counter

    counts: Counter = Counter()
    templates = []
    for pw in records:
        for group in _iter_groups(getattr(pw, "effects", None)):
            for t in getattr(group, "templates", None) or []:
                templates.append(t)
                for name in t.cancel_events:
                    eid = event_id_from_name(name)
                    if eid is not None:
                        counts[eid] += 1
                for rec in (*t.suppress_events, *t.required_events):
                    eid = rec.get("event_id")
                    if eid is not None:
                        counts[eid] += 1

    if not counts:
        return None

    gen_name, table = select_event_table(counts)
    if table is EVENT_NAME:
        return gen_name  # live numbering — parse-time names already correct

    def rename(eid: int) -> str:
        return table.get(eid, f"Event_{eid}")

    for t in templates:
        t.cancel_events = [
            rename(eid) if (eid := event_id_from_name(n)) is not None else n
            for n in t.cancel_events
        ]
        for rec in (*t.suppress_events, *t.required_events):
            eid = rec.get("event_id")
            if eid is not None:
                rec["event"] = rename(eid)

    import sys
    print(f"  Event enum: '{gen_name}' generation detected; "
          f"{len(templates)} template(s) re-resolved.", file=sys.stderr)
    return gen_name


def _resolve_offset(strtab_data, strtab_base, offset: int) -> str | None:
    """Resolve a u4 string-table offset to a printable string, or None if invalid.

    Requires the offset to land on a string boundary (the previous byte must be a
    null terminator, or the offset must be 0 within the table). Without that
    check, random u4 values that happen to point mid-string look like valid
    short strings and pollute downstream heuristics (e.g. an offset pointing 5
    bytes into "5thColumnEndgame.NictusFX" yields "lumnEndgame.NictusFX" — same
    shape as a real entity def, but garbage).
    """
    if offset == 0:
        return None
    abs_pos = strtab_base + offset
    if abs_pos <= 0 or abs_pos >= len(strtab_data):
        return None
    # Boundary check: previous byte must be a null terminator.
    if strtab_data[abs_pos - 1] != 0:
        return None
    end = abs_pos
    limit = min(abs_pos + 256, len(strtab_data))
    while end < limit and strtab_data[end] != 0:
        end += 1
    if end >= limit and (limit < len(strtab_data) and strtab_data[end] != 0):
        return None
    try:
        s = bytes(strtab_data[abs_pos:end]).decode('ascii')
    except UnicodeDecodeError:
        return None
    if not s:
        return None
    if not all(0x20 <= ord(c) < 0x7f for c in s):
        return None
    return s


# Attribs that carry a Params Power { Power Redirects.X } block — the ones
# the planner's redirect-following logic in convert-powerset.cjs cares about.
# Source: scripts/convert-powerset.cjs:188 (execute_power follow), :594 (create_entity).
# Union across both parser paths: HC/Parse7 (resolve_attrib) now emits the
# byte-granular names — Revoke_Power (raw 482) and Cancel_Mods (raw 511) — while
# the Parse6/Rebirth path still uses the collapsed ATTRIB_NAME_REBIRTH labels
# (Grant_Power for both grant+revoke, Cancel_Effects for 127). Keep the legacy
# 'cancel_effects' key so Rebirth param extraction isn't silently dropped until
# its sub-index decode is fixed too (HOMECOMING_PARSER.md deferred item 2).
_POWER_PARAM_ATTRIBS = frozenset({
    'execute_power', 'grant_power', 'revoke_power', 'recharge_power',
    'add_behavior', 'cancel_mods', 'cancel_effects', 'global_chance_mod', 'set_mode',
})

# Heuristic: a string looks like a power name if it has dotted form
# `Category.Powerset.Power` (3 parts, identifier characters, leading uppercase).
_PWRNAME_RE = None  # set below to avoid re-import


def _looks_like_power_name(s: str) -> bool:
    global _PWRNAME_RE
    if _PWRNAME_RE is None:
        import re
        _PWRNAME_RE = re.compile(r'^[A-Z][A-Za-z0-9_]*\.[A-Z][A-Za-z0-9_]*\.[A-Za-z0-9_]+$')
    return bool(_PWRNAME_RE.match(s))


def _looks_like_entity_def(s: str) -> bool:
    # Entity defs in CoH look like `MastermindPets_Howler_Wolf` or
    # `Class_Tarantula_Pet` — identifier characters, leading uppercase,
    # no spaces, no dots. (Power names use dots; entity defs use underscores.)
    if not s or ' ' in s or '.' in s:
        return False
    if len(s) < 4 or len(s) > 80:
        return False
    if not s[0].isupper():
        return False
    return all(c.isalnum() or c == '_' for c in s)


def _is_message_key(s: str) -> bool:
    """A `P<digits>` clientmessages key (e.g. `P1985334123` → "You call forth a
    {PowerName}!"). These are the EntCreate's float/combat-text DisplayMessage,
    NOT an entity reference — but they pass `_looks_like_entity_def` (leading 'P',
    all-alnum) and lead the Params block, so the heuristic otherwise grabs the
    message key as `entity_def` and pushes the real EntityDef down into
    `priority_list`. Filter them out so entity_def/priority_list land on the real
    strings. See HOMECOMING_PARSER "P-hash entity_def — ROOT SOLVED" (2026-06-12)."""
    return len(s) > 1 and s[0] == 'P' and s[1:].isdigit()


def _extract_params(tail_bytes: bytes, attribs: list[str],
                    strtab_base: int, strtab_data) -> dict | None:
    """Fallback scanner for an AttribMod tail region that resisted
    structural decode (`_read_params_union` / `_read_template_messages_and_fx`
    rewound). Scans u4-aligned slots for offsets that resolve to strings
    matching the patterns we expect for known attribs:

      * Execute_Power-family attribs → Params Power { Power Redirects.X }
        → emit {type: 'Power', power_names: [...]}
      * Create_Entity attribs → Params EntCreate { EntityDef ... }
        → emit {type: 'EntCreate', entity_def, display_name, redirects, priority_list}

    Returns None if no relevant Params content is found.
    """
    if not attribs or not tail_bytes:
        return None
    attrib_lc = {a.lower() for a in attribs}

    # Pull every plausible string out of the tail in order.
    found: list[str] = []
    seen: set[str] = set()
    for off in range(0, len(tail_bytes) - 3, 4):
        val = struct.unpack_from('<I', tail_bytes, off)[0]
        s = _resolve_offset(strtab_data, strtab_base, val)
        if s and s not in seen:
            found.append(s)
            seen.add(s)

    is_power_attrib = bool(attrib_lc & _POWER_PARAM_ATTRIBS)
    is_create_entity = 'create_entity' in attrib_lc

    if is_power_attrib:
        power_names = [s for s in found if _looks_like_power_name(s)]
        if power_names:
            return {'type': 'Power', 'power_names': power_names}
        return None

    if is_create_entity:
        # Drop float/combat-text message keys (P<digits>) — the EntCreate's
        # DisplayMessage, not the entity. Otherwise the message key (which leads
        # the Params block) is grabbed as entity_def and the real EntityDef is
        # pushed into priority_list. See HOMECOMING_PARSER (2026-06-12).
        ent = [s for s in found if not _is_message_key(s)]
        # Entity defs typically lead the Params block. DisplayName is free text
        # (often title-cased with spaces), PriorityList is short identifier.
        entity_def = next((s for s in ent if _looks_like_entity_def(s)), None)
        power_names = [s for s in ent if _looks_like_power_name(s)]
        # DisplayName: any short string that isn't a power-name and has a space
        # or just looks like a tooltip.
        display_name = next(
            (s for s in ent
             if s != entity_def and not _looks_like_power_name(s)
             and ' ' in s and len(s) < 80),
            None,
        )
        # PriorityList: leftover short non-dotted identifier (e.g. "Pet").
        priority_list = next(
            (s for s in ent
             if s != entity_def and s != display_name
             and '.' not in s and len(s) < 40
             and all(c.isalnum() or c == '_' for c in s)),
            None,
        )
        # Only emit a result if we found a real entity_def. Create_Entity is now
        # resolved cleanly at the source (raw 469, distinct from its former index-117
        # siblings Translucency/Silent_Kill/Clear_Damagers — see resolve_attrib), so
        # this no longer guards against a misdecode; it just rejects malformed/empty
        # EntCreate tails whose only string is a stray P-hash the convert script
        # would otherwise treat as a real Pet PriorityList.
        if not entity_def:
            return None
        result = {'type': 'EntCreate', 'entity_def': entity_def}
        if display_name: result['display_name'] = display_name
        if power_names: result['redirects'] = power_names
        if priority_list: result['priority_list'] = priority_list
        return result

    return None


def _extract_params_parse6(tail_bytes: bytes, attribs: list[str]) -> dict | None:
    """Fallback scanner for a Parse6 post-magnitude tail that resisted the
    structural decode (`_read_parse6_template_tail` raised and rewound).

    Parse6 stores strings inline as `u16(len) + chars + 4-byte-align pad`
    (vs Parse7's u4 offset into a separate string table). Scans the tail
    for inline pascal strings that pass the same `_looks_like_power_name`
    shape filter Parse7 uses. Returns `{'type': 'Power', 'power_names':
    [...]}` when at least one match found; None otherwise.

    Also handles Create_Entity (pet/summon) templates: when the attrib is
    create_entity, the same string scan picks up the `Pets_X` entity def
    that the binary stores in the Params block. Without this, Rebirth's
    pool attack powers (Dive Attack, Blink Blitz) and pet-summoning
    archetype powers couldn't be linked to their `Pets_X` entity for
    downstream damage extraction.
    """
    if not attribs or not tail_bytes:
        return None
    attrib_lc = {a.lower() for a in attribs}
    # Same attrib gate as Parse7 — only emit when the template's attrib
    # actually uses a Power param. Plus 'null' (Parse6 lowers Grant_Power
    # to a Null-attrib placeholder when the binary stores the granted
    # power separately from the attribs list — see audit notes in
    # MULTI_DATASET_PLAN.md → "Vacuum-style pet conditional gates").
    is_power_attrib = bool(attrib_lc & _POWER_PARAM_ATTRIBS) or 'null' in attrib_lc
    is_create_entity = 'create_entity' in attrib_lc
    if not is_power_attrib and not is_create_entity:
        return None

    # Inline-pascal scan: walk the tail looking for plausible string
    # records. A valid record has a length within reason, all printable
    # ASCII, and post-length padded to 4-byte alignment.
    found: list[str] = []
    seen: set[str] = set()
    pos = 0
    while pos + 2 <= len(tail_bytes):
        slen = struct.unpack_from('<H', tail_bytes, pos)[0]
        if 4 <= slen <= 200 and pos + 2 + slen <= len(tail_bytes):
            chars = tail_bytes[pos + 2:pos + 2 + slen]
            if all(32 <= b < 127 or b == 0 for b in chars):
                s = chars.rstrip(b'\x00').decode('ascii', errors='replace')
                if s and s not in seen and all(
                    c.isalnum() or c in '._- ' for c in s
                ):
                    found.append(s)
                    seen.add(s)
                # Advance past the string + alignment pad regardless of
                # whether we recorded it — the bytes were a valid pascal
                # string, so skipping byte-by-byte would re-decode it.
                pos += 2 + slen
                pos += (4 - (pos % 4)) % 4
                continue
        pos += 1

    if is_create_entity:
        # Drop float/combat-text message keys (P<digits>) — see the Parse7
        # branch + HOMECOMING_PARSER (2026-06-12).
        ent = [s for s in found if not _is_message_key(s)]
        entity_def = next((s for s in ent if _looks_like_entity_def(s)), None)
        if not entity_def:
            return None
        result: dict = {'type': 'EntCreate', 'entity_def': entity_def}
        power_names = [s for s in ent if _looks_like_power_name(s)]
        display_name = next(
            (s for s in ent
             if s != entity_def and not _looks_like_power_name(s)
             and ' ' in s and len(s) < 80),
            None,
        )
        priority_list = next(
            (s for s in ent
             if s != entity_def and s != display_name
             and '.' not in s and len(s) < 40
             and all(c.isalnum() or c == '_' for c in s)),
            None,
        )
        if display_name: result['display_name'] = display_name
        if power_names: result['redirects'] = power_names
        if priority_list: result['priority_list'] = priority_list
        return result

    power_names = [s for s in found if _looks_like_power_name(s)]
    if not power_names:
        return None
    return {'type': 'Power', 'power_names': power_names}


def _read_event_records(r: BinReader, *, event_names: dict[int, str] = EVENT_NAME,
                        seconds_as_int: bool = False) -> list[dict]:
    """Read one event-condition struct_array (RequiredEvents or Suppress).

    Record shape, shared by both arrays and confirmed against CoD2's
    required_events/suppress_events fields: [rec_len u4][event u4]
    [post_event_duration f4][always u4], rec_len 12 corpus-wide. The
    rec_len-driven skip keeps alignment even if a variant adds fields.
    `duration` here is the post-event window in seconds, not the
    AttribMod's buff duration.

    `event_names` selects the enum generation to resolve ids through, because
    the numbering is per-fork: Homecoming renumbers this enum on patch — which
    is why `recalibrate_event_names` exists — so resolving a fork that never
    had HC's numbering through `EVENT_NAME` names the neighbouring event.
    Thunderspy passes `EVENT_NAME_PARSE6`, the numbering it shares with the
    rest of the Parse6 lineage (WRAP-3 derives it there). The id is the datum;
    an id absent from the chosen table stays `Event_<n>`.

    `seconds_as_int` reads the post-event window as a u4 rather than an f4, the
    encoding the Parse6 lineage uses (`_read_parse6_template_tail` reads its own
    copy of this record the same way). Homecoming writes Super Speed's
    suppression window as the float 4.0; Thunderspy writes the integer 4 in the
    same slot of the same record.
    """
    count = r.read_u4()
    if count > 64:
        raise ValueError(
            f"implausible event-record count {count} — misaligned template?"
        )
    records: list[dict] = []
    for _ in range(count):
        rec_len = r.read_u4()
        rec = r.sub_reader(rec_len)
        event_id = rec.read_u4()
        if rec_len < 8:
            post_duration = 0.0
        else:
            post_duration = float(rec.read_u4()) if seconds_as_int else rec.read_f4()
        always = rec.read_u4() if rec_len >= 12 else 0
        records.append({
            "event": event_names.get(event_id, f"Event_{event_id}"),
            "event_id": event_id,
            "duration": post_duration,
            "always": always,
        })
        r.skip(rec_len)
    return records


def _read_target_info(r: BinReader) -> dict | None:
    """Read the TargetInfo struct_array that sits between `target` and `table`.

    Populated only on Marker-targeted mods — the encounter-power pattern where
    Create_Entity mods spawn at named map markers laid down by a companion
    Locator power (Airstrike strafes, Magisterium portals/god-beams, Shivan
    meteors). Every other template stores count 0, which is byte-identical to
    the lone u4 this read replaces. Record shape: MarkerNames (string_array) +
    MarkerCount (u4_array). Validated 1:1 against CoD2's per-template
    `target_info` object: exactly 53 records corpus-wide on HC, all with
    target Marker, names and counts matching field-for-field.
    """
    count = r.read_u4()
    if count == 0:
        return None
    if count > 8:
        raise ValueError(
            f"implausible TargetInfo count {count} — misaligned template?"
        )
    marker_names: list[str] = []
    marker_count: list[int] = []
    for _ in range(count):
        rec_len = r.read_u4()
        rec = r.sub_reader(rec_len)
        marker_names += rec.read_string_array()
        marker_count += rec.read_u4_array()
        if rec.remaining():
            raise ValueError(
                f"TargetInfo record left {rec.remaining()} byte(s) undecoded"
            )
        r.skip(rec_len)
    return {"marker_names": marker_names, "marker_count": marker_count}


# Authored `Messages { ... }` block field order (raw defs census 2026-07-20:
# DisplayAttackerHit 12754 / DisplayVictimHit 12070 / DisplayFloat 2295 /
# DisplayAttribDefenseFloat 1 / DisplayInfo 1296). The binary record is five
# u4 string offsets in exactly this order; the sole authored
# DisplayAttribDefenseFloat carrier (Sentinel Repelling_Force "Repelled") is
# also the sole binary slot-3 carrier, pinning that slot. CoD2 exposes the
# first four as messages.display_* (it names slot 3 display_defense_float)
# and does not expose DisplayInfo at all. Values are message-store keys
# (P-hashes / 26-char IDs) resolved at export via clientmessages-en.bin.
_TEMPLATE_MESSAGE_FIELDS = (
    "display_attacker_hit",
    "display_victim_hit",
    "display_float",
    "display_attrib_defense_float",
    "display_info",
)

# Thunderspy's Messages record is 16 bytes, not 20 — one slot shorter than
# Parse7. Slot occupancy across the fork's 43,382 records identifies the first
# three by the same shape Homecoming shows (attacker-hit and victim-hit heavy,
# float rare): 17,314 / 23,397 / 2,671, against HC's 12,754 / 12,070 / 2,295.
# Slot 3 is empty on every record in the corpus, so which of HC's last two
# fields tspy dropped is unobservable — the name below is HC's field order
# continued, and it labels nothing either way.
_TSPY_TEMPLATE_MESSAGE_FIELDS = _TEMPLATE_MESSAGE_FIELDS[:4]

# Templates whose Messages/FX region didn't match the validated layout and was
# left undecoded (raw bytes still reach the params scanner). Corpus-validated
# counts are strictly 0-or-1, so anything else is a layout-variant signal that
# must surface at the end of the parse rather than vanish.
_undecoded_message_fx_tails = 0


def _read_template_messages_and_fx(
    r: BinReader,
    message_fields: tuple[str, ...] = _TEMPLATE_MESSAGE_FIELDS,
) -> tuple[dict | None, dict | None, bool]:
    """Decode the Messages and FX struct_arrays that follow the flags words.

    Validated corpus-wide vs CoD2 (2026-07-20, 94,816 HC templates): Messages
    and FX counts are strictly 0 or 1; the Messages record is 20 bytes (five
    string offsets, `_TEMPLATE_MESSAGE_FIELDS`); the FX record is
    [continuing_bits u4_array][continuing_fx string][conditional_bits
    u4_array][conditional_fx string] (16 bytes empty, longer per bit). FX
    decode agreed with CoD2 on 17,105 of 17,127 paired carriers.

    Returns (messages, fx, decoded). On any implausible count or record
    length the reader is rewound to where it started and decoded is False —
    the bytes stay in the tail for the params scanner exactly as before this
    decode existed (the Params union can't be located either, since it sits
    after the undecoded region), and the anomaly is counted so parse_powers
    can report it loudly. Rewinding (rather than raising) matters because a
    raise here would silently drop the whole template at the effect-group
    level.
    """
    global _undecoded_message_fx_tails
    start = r._pos

    def rewind() -> tuple[None, None, bool]:
        global _undecoded_message_fx_tails
        r._pos = start
        _undecoded_message_fx_tails += 1
        return None, None, False

    if r.remaining() < 4:
        return None, None, True
    message_count = r.read_u4()
    if message_count > 1:
        return rewind()
    messages: dict | None = None
    if message_count == 1:
        if r.remaining() < 4:
            return rewind()
        record_length = r.read_u4()
        if record_length != 4 * len(message_fields) or r.remaining() < record_length:
            return rewind()
        record = r.sub_reader(record_length)
        messages = {name: record.read_string() or None
                    for name in message_fields}
        r.skip(record_length)
        if not any(messages.values()):
            messages = None

    if r.remaining() < 4:
        return rewind() if message_count else (messages, None, True)
    fx_count = r.read_u4()
    if fx_count > 1:
        return rewind()
    fx: dict | None = None
    if fx_count == 1:
        if r.remaining() < 4:
            return rewind()
        record_length = r.read_u4()
        if record_length < 16 or r.remaining() < record_length:
            return rewind()
        record = r.sub_reader(record_length)
        try:
            continuing_bits = record.read_u4_array()
            continuing_fx = record.read_string() if record.remaining() >= 4 else ""
            conditional_bits = record.read_u4_array() if record.remaining() >= 4 else []
            conditional_fx = record.read_string() if record.remaining() >= 4 else ""
        except ValueError:
            # A garbage bits-array count overruns the record boundary — this
            # isn't the validated FX shape, so leave the region undecoded.
            return rewind()
        if record.remaining() != 0:
            return rewind()
        r.skip(record_length)
        fx = {
            "continuing_bits": list(continuing_bits),
            "continuing_fx": continuing_fx or None,
            "conditional_bits": list(conditional_bits),
            "conditional_fx": conditional_fx or None,
        }
        if not (fx["continuing_bits"] or fx["continuing_fx"]
                or fx["conditional_bits"] or fx["conditional_fx"]):
            fx = None
    return messages, fx, True


# Params-union type ids, from the corpus-wide census (2026-07-20, 11,019
# typed payloads): each id correlates 1:1 with one attrib family and one
# authored `Params <Kind>` block. Ids are contiguous 1..12; 0 = no params.
_PARAMS_TYPE_NAMES = {
    1: "Costume", 2: "Reward", 3: "EntCreate", 4: "Power", 5: "Phase",
    6: "Teleport", 7: "Behavior", 8: "SZEValue", 9: "Token",
    10: "EffectFilter", 11: "Knock", 12: "ScriptNotify",
}

# Templates whose Params union didn't match the validated layouts and fell
# back to the heuristic scanner. Same loud-at-parse-end contract as
# `_undecoded_message_fx_tails`.
_undecoded_params_tails = 0


def _read_params_payload(kind: str, r: BinReader, *,
                         thunderspy: bool = False) -> dict:
    """Decode one Params payload. Field layouts pinned against the CoD2
    oracle 2026-07-20 (order-insensitive pair check over 11,019 typed
    payloads: zero structural disagreements; residual value diffs all
    live-binary version skew clustered in post-snapshot patched sets).

    `thunderspy` selects the two payload shapes that fork diverges on
    (EntCreate and Knock); every other kind is byte-identical to Homecoming's,
    which is what lets this one reader serve both (WRAP-1).

    Raises ValueError on any structural violation (array overrun, garbage
    count, unconsumed bytes) — the caller falls back to the scanner.
    """
    def string_or_none() -> str | None:
        return r.read_string() or None

    def checked_string_array() -> list[str]:
        if r.remaining() < 4:
            raise ValueError("string_array overruns payload")
        count = struct.unpack_from("<I", r._data, r._pos)[0]
        if count > 1000 or 4 + 4 * count > r.remaining():
            raise ValueError(f"implausible string_array count {count}")
        return r.read_string_array()

    def checked_u4_array() -> list[int]:
        if r.remaining() < 4:
            raise ValueError("u4_array overruns payload")
        count = struct.unpack_from("<I", r._data, r._pos)[0]
        if count > 1000 or 4 + 4 * count > r.remaining():
            raise ValueError(f"implausible u4_array count {count}")
        return r.read_u4_array()

    if kind == "Costume":
        out = {"costume_name": string_or_none(), "priority": r.read_u4()}
    elif kind == "Reward":
        out = {"rewards": checked_string_array()}
    elif kind == "EntCreate" and thunderspy:
        # Thunderspy's EntCreate is 36 bytes against Homecoming's 40: the same
        # six leading strings, then THREE name arrays where HC has four. The
        # string half is pinned by slot occupancy — across all 2,523 records
        # only slots 0 and 4 are ever set, and they hold an entity def
        # (`5thColumn_Boss_Nightwolf_Lycan`) and a priority list
        # (`PL_FightPreferMelee`), which is where HC puts EntityDef and
        # PriorityList. Every array is empty corpus-wide, so WHICH of HC's four
        # this fork dropped cannot be read off the data; naming them would be a
        # guess, so a non-empty one raises instead.
        out = {
            "entity_def": string_or_none(),
            "class_name": string_or_none(),
            "costume_name": string_or_none(),
            "display_name": string_or_none(),
            "priority_list": string_or_none(),
            "ai_config": string_or_none(),
        }
        for _ in range(3):
            if checked_string_array():
                raise ValueError(
                    "thunderspy EntCreate name array is non-empty — the field "
                    "it belongs to is unidentified (empty on every record the "
                    "shape was pinned against)"
                )
    elif kind == "Knock" and thunderspy:
        # Same eight fields and same 40 bytes as Homecoming, with the PYR
        # adjustment vector moved from third position to last. Pinned by what
        # each mez authors rather than by shape: Knockup reads height 6.0 /
        # height_mag 1.0 with vel and vel_mag both zero, Repel reads vel 0.5 /
        # vel_mag 0.02 with both height fields zero, and Knockback reads all
        # four. Under HC's order those same words land in `vec_adjust_pyr` and
        # `priority`, which is how the misread announced itself: priority came
        # back 1077936128, the bit pattern of 3.0f.
        _vec_start_raw = r.read_u4()
        _vec_end_raw = r.read_u4()
        out = {
            "vec_start": KNOCK_VEC_POSITION.get(_vec_start_raw, f"Unknown({_vec_start_raw})"),
            "vec_end": KNOCK_VEC_POSITION.get(_vec_end_raw, f"Unknown({_vec_end_raw})"),
            "priority": r.read_u4(),
            "vel": r.read_f4(),
            "vel_mag": r.read_f4(),
            "height": r.read_f4(),
            "height_mag": r.read_f4(),
            "vec_adjust_pyr": [r.read_f4(), r.read_f4(), r.read_f4()],
        }
    elif kind == "EntCreate":
        # Authored field order: EntityDef, Class, Costume, DisplayName,
        # PriorityList, AIConfig, then category/powerset/power name arrays,
        # then the redirect power list. Key names below keep the legacy
        # export contract (`priority_list`, `redirects`) that
        # convert-powerset.cjs already consumes.
        out = {
            "entity_def": string_or_none(),
            "class_name": string_or_none(),
            "costume_name": string_or_none(),
            "display_name": string_or_none(),
            "priority_list": string_or_none(),
            "ai_config": string_or_none(),
            "category_names": checked_string_array(),
            "powerset_names": checked_string_array(),
            "power_names": checked_string_array(),
            "redirects": checked_string_array(),
        }
    elif kind == "Power":
        out = {
            "category_names": checked_string_array(),
            "powerset_names": checked_string_array(),
            "power_names": checked_string_array(),
            "count": r.read_u4(),
        }
    elif kind == "Phase":
        out = {
            "combat_phases": checked_u4_array(),
            "vision_phases": checked_u4_array(),
            "exclusive_vision_phase": r.read_s4(),
        }
    elif kind == "Teleport":
        out = {"destination": string_or_none()}
    elif kind == "Behavior":
        out = {"behaviors": checked_string_array()}
    elif kind == "SZEValue":
        # Both fields are RPN token lists, kept as lists like every other
        # expression field (see _dataclasses).
        out = {
            "script_id": checked_string_array(),
            "script_value": checked_string_array(),
        }
    elif kind == "Token":
        out = {"tokens": checked_string_array()}
    elif kind == "EffectFilter":
        out = {
            "category_names": checked_string_array(),
            "powerset_names": checked_string_array(),
            "power_names": checked_string_array(),
            "tags": checked_string_array(),
        }
    elif kind == "Knock":
        _vec_start_raw = r.read_u4()
        _vec_end_raw = r.read_u4()
        out = {
            "vec_start": KNOCK_VEC_POSITION.get(_vec_start_raw, f"Unknown({_vec_start_raw})"),
            "vec_end": KNOCK_VEC_POSITION.get(_vec_end_raw, f"Unknown({_vec_end_raw})"),
            "vec_adjust_pyr": [r.read_f4(), r.read_f4(), r.read_f4()],
            "priority": r.read_u4(),
            "vel": r.read_f4(),
            "vel_mag": r.read_f4(),
            "height": r.read_f4(),
            "height_mag": r.read_f4(),
        }
    elif kind == "ScriptNotify":
        out = {
            "channel": string_or_none(),
            "notify_event": [NOTIFY_EVENT.get(v, f"NotifyEvent_{v}")
                             for v in checked_u4_array()],
        }
    else:
        raise ValueError(f"unhandled params kind {kind}")

    if r.remaining() != 0:
        raise ValueError(f"{r.remaining()} unconsumed payload bytes")
    return out


def _read_params_union(r: BinReader, *,
                       thunderspy: bool = False) -> tuple[dict | None, bool]:
    """Decode the Params tagged union that ends the AttribMod tail:
    u4 type id (0 = none), u4 payload length, payload.

    Corpus framing is exact (2026-07-20): type 0 is always the final 4 tail
    bytes, and a typed payload's declared length always equals the bytes
    remaining. Anything else — unknown type id, length mismatch, structural
    violation inside the payload — rewinds, counts the anomaly for the
    parse-end warning, and returns decoded=False so the caller hands the
    region to the heuristic scanner instead of dropping it.
    """
    global _undecoded_params_tails
    start = r._pos

    def rewind() -> tuple[None, bool]:
        global _undecoded_params_tails
        r._pos = start
        _undecoded_params_tails += 1
        return None, False

    if r.remaining() == 0:
        return None, True
    if r.remaining() < 4:
        return rewind()
    ptype = r.read_u4()
    if ptype == 0:
        return (None, True) if r.remaining() == 0 else rewind()
    kind = _PARAMS_TYPE_NAMES.get(ptype)
    if kind is None or r.remaining() < 4:
        return rewind()
    payload_length = r.read_u4()
    if payload_length != r.remaining():
        return rewind()
    payload = r.sub_reader(payload_length)
    try:
        out = _read_params_payload(kind, payload, thunderspy=thunderspy)
    except ValueError:
        return rewind()
    r.skip(payload_length)
    # Drop empty-value keys except in fixed-shape records (Knock, Phase,
    # Costume, Power count) where zero/None is data — matches the lean style
    # the scanner-era export used for optional EntCreate fields.
    if kind in ("EntCreate", "Power", "EffectFilter"):
        out = {k: v for k, v in out.items() if v or k == "count"}
    out["type"] = kind
    return out, True


def _parse_effect_template(r: BinReader, *, veracity: bool = False) -> EffectTemplate:
    """Parse a single attrib_mod template within an effect group.

    `veracity` selects the Veracity private-server variant, which is the HC
    Parse7 template layout plus two fields inserted immediately after
    `tick_chance`: HitsToBreak (u4) and HitBreakMinScale (f4). Confirmed by the
    server dev and verified by hand-decoding Jab/Fire_Blast (the inserted fields
    keep the downstream tick_mul/tick_add/stack reads aligned).
    """
    # Attribs: u4_array. Normal attribs are stored as enum_index * 4; the
    # special "meta/scripting" region (indices 117-128) is byte-granular, so we
    # resolve the raw value directly (resolve_attrib) instead of `// 4` — that
    # truncation was the root of the attrib-118 misdecode. See _enums.py.
    raw_attribs = r.read_u4_array()
    attribs = [resolve_attrib(v) for v in raw_attribs]

    # Aspect is encoded as value * 8 (byte offset into aspect table)
    aspect_raw = r.read_u4()
    aspect = ATTRIB_MOD_ASPECT.get(aspect_raw // 8, f"Unknown({aspect_raw})")

    # Remaining enums — preserve raw value when unmapped so we can investigate.
    # Field order is (application_type, type) — verified via Ghidra keyword
    # tables. The old parser had these labels swapped, which also explained
    # the ~20% type/application_type "mismatch" vs CoD2 (the mez "Magnitude
    # relabeled as Duration" story — CoD2 was right, parser was wrong).
    _app_raw = r.read_u4()
    app_type = ATTRIB_MOD_APPLICATION.get(_app_raw, f"Unknown({_app_raw})")
    _typ_raw = r.read_u4()
    typ = ATTRIB_MOD_TYPE.get(_typ_raw, f"Unknown({_typ_raw})")
    _target_raw = r.read_u4()
    target = ATTRIB_MOD_TARGET.get(_target_raw, f"Unknown({_target_raw})")

    # TargetInfo struct_array — marker names/counts for Marker-targeted mods,
    # count 0 for everything else (see _read_target_info). This slot was long
    # read as an unknown u4: count 0 is byte-identical, so all non-Marker
    # templates aligned while the 53 Marker templates (11 encounter powers)
    # broke 20 bytes in and were dropped.
    target_info = _read_target_info(r)

    # Table name (string) + numeric fields
    table = r.read_string()
    scale = r.read_f4()
    duration = r.read_f4()
    magnitude = r.read_f4()

    # Layout for ALL templates is dur_expr_tokens (u4_array), mag_expr_tokens
    # (u4_array), delay (f4) — NOT delay first then expressions. The same order
    # the kExpression branch uses; non-kExpression just has both arrays empty
    # in the typical case. Confirmed against Savage_Leap_AoE (kExpression,
    # Scale 0.845, Delay 0.1) and against ~227 Self_Destruct templates whose
    # Silent_Kill AttribMods have Delay set to the pet lifespan (Pets_Shade
    # 60s, Pets_Living_Hellfire 90s, Pets_Mastermind_Ghosts 300s, etc.).
    #
    # The previous non-kExpression layout (`delay = f4; dur_expr/mag_expr = string`)
    # happened to look right for templates with no Delay AND no expressions —
    # delay-slot bytes were 0 (empty u4_array count), and the two trailing
    # string reads returned empty (string offset 0). It quietly dropped Delay
    # on any template that had one. The kExpression branch had this same bug
    # historically; it was fixed by reordering tokens-before-delay. The same
    # reorder is correct for non-kExpression templates too.
    # Duration / magnitude expression RPN token lists. These are string_arrays
    # (each token a string-table offset) — byte-identical on the wire to a
    # u4_array, so reading them as strings doesn't shift alignment. Empty for
    # ~99% of templates; non-empty on kExpression effects whose magnitude scales
    # with a runtime value (e.g. Blazing Bolt's damage scaling with the caster's
    # ToHit: "@StdResult * (0.211 * minmax(4.54 * source>cur.kToHit - 3.41, -1,
    # 1) + 1)"). Previously discarded, which left every Expression-type
    # effect with a blank magnitude_expression in the export.
    dur_expr = r.read_string_array()
    mag_expr = r.read_string_array()
    delay = r.read_f4()

    # Tick fields
    app_period = r.read_f4()
    tick_chance = r.read_f4()
    # Veracity inserts HitsToBreak (u4) + HitBreakMinScale (f4) here, between
    # tick_chance and tick_mul (the "hits to break a hold/effect" mechanic). HC
    # has neither field. Read-and-discard for now — not yet consumed downstream.
    if veracity:
        r.read_u4()   # HitsToBreak
        r.read_f4()   # HitBreakMinScale
    tick_mul = r.read_f4()
    tick_add = r.read_f4()

    # The AttribMod's own `Requires` — `ppchApplicationRequires` in the source
    # table (`Common/entity/attribmod.h:692`), an application-time gate distinct
    # from the enclosing EffectGroup's cast-time one. A string_array (Ghidra
    # descriptor 0x1408ed5a0 row 17 types it 0x500009), empty on all but 339
    # Homecoming templates. Reading it as a single string shifted every
    # downstream field by 4*N bytes and broke stack reads on ~29 pet-summon
    # templates (Summon_Wolves, Paralyzing_Blast); the tokens were then resolved
    # but thrown away, which is the other half of the same lesson — Parse6 has
    # carried this field into `jit_requires` all along, so discarding it here
    # made one fork's gate vanish and the other's survive.
    jit_requires = r.read_string_array()

    # Stack info — preserve raw value when unmapped so the unknown is debuggable
    _caster_stack_raw = r.read_u4()
    caster_stack = ATTRIB_MOD_CASTER_STACK.get(_caster_stack_raw, f"Unknown({_caster_stack_raw})")
    _stack_raw = r.read_u4()
    stack = ATTRIB_MOD_STACK.get(_stack_raw, f"Unknown({_stack_raw})")
    stack_limit = r.read_u4()
    # StackKey: a u4 ID into the global StackKeys registry (attrib_names.bin
    # StackKeys sub-array; see parse_stack_key_table), NOT a string offset.
    # The old read_string() decode dereferenced the small ID into powers.bin's
    # string table head, yielding suffixes of its first entry ("5thColumn.
    # 5thColumnEndgame.NictusFX" → 'NictusFX'/'ictusFX'/'tusFX' at IDs
    # 28/29/31). ID→name (kStealthToggle/kTravelBuff/kTravelMaxBuff/...) is
    # resolved at export time; -1 (0xFFFFFFFF) = no key, normalized to 0
    # (registry[0] is the unreferenced TestStack filler, so the collision is
    # moot). Verified vs the CoD2 `.powers` oracle (kPvPMezProt=2 ...
    # kHasteBuff=35, all direct registry indices).
    _stack_key_raw = r.read_u4()
    stack_key_id = 0 if _stack_key_raw == 0xFFFFFFFF else _stack_key_raw

    # AttribMod tail. Per the Ghidra-extracted struct descriptor, layout is:
    #   CancelEvents, RequiredEvents, Suppress, BoostModAllowed, Flags,
    #   Messages, FX, Params
    # Sizes verified empirically against Hide.def + Pool/Invisibility/Stealth.def
    # in 2026-04-22 audit. Tail-byte param scan is kept as a fallback for the
    # later fields (Flags / FX / Params) until those are fully decoded.

    # CancelEvents: u4_array of event enum IDs (see EVENT_NAME)
    cancel_event_ids = r.read_u4_array()
    cancel_events = [EVENT_NAME.get(v, f"Event_{v}") for v in cancel_event_ids]
    # RequiredEvents, then Suppress: two struct_arrays sharing one record
    # shape (see _read_event_records). RequiredEvents was long misread as a
    # single u4 "event id, 0 = none" — count 0 is byte-identical to that, so
    # every template WITHOUT required events aligned perfectly, while the
    # rare event-gated templates (redirect mez/ToHit mods conditioned on
    # HelpedByOther/Healed/Repelled/Untouchable) had their record bytes
    # consumed misaligned, landing BoostModAllowed/flags_raw/flags2_raw
    # mid-record. Verified against raw bytes + the CoD2 oracle 2026-07-20:
    # Redirects.Assault_Rifle.Smoke_Confusion's Confused template reads
    # count=3, three 12-byte records (events 13/21/33, post-duration 15.0),
    # then flags_raw 0x0a00 — exactly the flags CoD2 lists.
    required_events = _read_event_records(r)
    suppress_events = _read_event_records(r)

    # Tail layout after Suppress, pinned corpus-wide against CoD2 + raw-byte
    # dumps 2026-07-20 (89% of HC templates account for every tail byte;
    # the rest differ only inside the Params payload):
    #   BoostModAllowed (u4) — small enum index, 0 for ~99% of templates
    #   Flags (u4 ×2) — the two flag words decoded via _FLAG_BITS /
    #     _FLAG2_BITS_BY_ATTRIB above
    #   Messages (struct_array, count 0/1, 20-byte record) — see
    #     _read_template_messages_and_fx
    #   FX (struct_array, count 0/1, variable record) — same helper
    #   Params (tagged union, decoded per type by _read_params_union)
    boost_mod_allowed_id = r.read_u4() if r.remaining() >= 4 else 0
    flags_raw = r.read_u4() if r.remaining() >= 4 else 0
    # Second flags word (attrib-contextual keywords: DeepSleep, RevokeAll,
    # CopyBoosts / PseudoPet / …  — see _FLAG2_BITS_BY_ATTRIB).
    flags2_raw = r.read_u4() if r.remaining() >= 4 else 0

    messages, fx, tail_decoded = _read_template_messages_and_fx(r)

    # Legacy fields preserved for downstream consumers — BMA exposed as a name
    # via the simple int → str conversion (won't conflict with any consumer
    # since it's never been populated before).
    boost_mod_allowed = str(boost_mod_allowed_id) if boost_mod_allowed_id else ""
    flags: list[str] = _decode_flags(flags_raw) + _decode_flags2(flags2_raw, attribs)
    mode_name = None

    # The Params tagged union ends the tail. Structural decode first; the
    # heuristic scanner remains only as the fallback when the union (or the
    # Messages/FX region before it, which anchors where the union starts)
    # didn't match the validated layout.
    params = None
    params_decoded = False
    if tail_decoded:
        params, params_decoded = _read_params_union(r)
    if not params_decoded:
        tail_bytes = bytes(r._data[r._pos:r._end])
        r.skip_to_end()
        params = _extract_params(tail_bytes, attribs, r._strtab_base, r._strtab_data)

    return EffectTemplate(
        attribs=attribs,
        type=typ,
        application_type=app_type,
        aspect=aspect,
        target=target,
        target_info=target_info,
        table=table,
        scale=scale,
        duration=duration,
        magnitude=magnitude,
        delay=delay,
        duration_expression=dur_expr,
        magnitude_expression=mag_expr,
        application_period=app_period,
        tick_chance=tick_chance,
        tick_mag_multiplier=tick_mul,
        tick_mag_additive=tick_add,
        jit_requires=jit_requires,
        caster_stack=caster_stack,
        stack=stack,
        stack_limit=stack_limit,
        stack_key_id=stack_key_id,
        cancel_events=cancel_events,
        suppress_events=suppress_events,
        required_events=required_events,
        boost_mod_allowed=boost_mod_allowed,
        boost_mod_allowed_id=boost_mod_allowed_id,
        flags=flags,
        flags_raw=flags_raw,
        flags2_raw=flags2_raw,
        mode_name=mode_name,
        messages=messages,
        fx=fx,
        params=params,
    )


def _parse_effect_group(r: BinReader, *, veracity: bool = False,
                        power_name: str = "",
                        player_classes=()) -> EffectGroup:
    """Parse an effect group containing templates.

    Per the EffectGroup sub-descriptor at 0x1408ea180 (Ghidra dump in
    `bin_serializer_report.txt`), the leading fields are:
      Tag (string_array), DisplayInfo (string), Chance, PPM, Delay,
      RadiusInner, RadiusOuter, Requires, Flags, EvalFlags, AttribMod,
      Effect (recursive children).

    The previous version read DisplayInfo as a u4. For HC Parse7 a
    string IS a 4-byte offset, so this happened to consume the right
    number of bytes (just lost the semantic value). For Parse6 strings
    are inline pstrings of variable length — reading as u4 there
    cascades into a 99% misalignment that empties effects across the
    whole binary. Reading as string fixes it for both formats.
    """
    tags = r.read_string_array()
    # Veracity omits the DisplayInfo string that HC writes between Tag and the
    # Chance/PPM/Delay/Radius block (verified by hand-decoding Jab/Fire_Blast:
    # the numeric block follows Tag directly). Reading it on Veracity would shift
    # every downstream field by 4 bytes and empty the group's templates.
    if not veracity:
        _display_info = r.read_string()

    # Effect header
    chance = r.read_f4()
    ppm = r.read_f4()
    delay = r.read_f4()
    radius_inner = r.read_f4()
    radius_outer = r.read_f4()

    # Requires expression
    requires = r.read_string_array()

    # Flags and eval_flags
    flags_val = r.read_u4()
    eval_flags = r.read_u4()

    # flags_val is a bitmask, not a single enum value. Bit 0 = PvEOnly,
    # bit 1 = PvPOnly; higher bits encode other Effect-level flags from
    # the .powers source (MainTargetOnly, Fallback, HideFromInfo, etc.)
    # that aren't yet decoded. The earlier exact-match lookup mis-classified
    # any combined value (e.g. PvEOnly+MainTargetOnly = 5) as EITHER.
    flags = []
    if flags_val & 1:
        flags.append("PVEOnly")
        is_pvp = "PVE_ONLY"
    elif flags_val & 2:
        flags.append("PVPOnly")
        is_pvp = "PVP_ONLY"
    else:
        is_pvp = "EITHER"

    # Veracity encodes the PvE/PvP split in the group's Requires RPN
    # (`enttype target> critter eq` / `… player eq`), parse6-style, rather than
    # in the flags bitmask — so derive is_pvp from requires when the flags
    # didn't already pin it. Mirrors the _parse_effects_parse6 logic.
    requires_pv = entity_scope(requires)
    gate = gate_evaluate(requires, player_classes=player_classes)
    if veracity and is_pvp == "EITHER" and requires_pv in ("PVE_ONLY", "PVP_ONLY"):
        is_pvp = requires_pv

    # Templates struct_array
    templates = []
    tmpl_count = r.read_u4()
    for _ in range(tmpl_count):
        tmpl_len = r.read_u4()
        tmpl_reader = r.sub_reader(tmpl_len)
        try:
            tmpl = _parse_effect_template(tmpl_reader, veracity=veracity)
            templates.append(tmpl)
        except Exception as e:
            _warn_dropped(power_name, f"effect template parse failed ({e})")
        r.skip(tmpl_len)

    _note_chances(power_name, chance, templates)

    # Nested effect groups (recursive). The .def grammar allows `Effect { ... }`
    # inside another `Effect { ... }` (e.g. Chance/Requires-gated sub-effects);
    # the binary mirrors this with a struct_array of child groups right after
    # the templates array. Without this read, ~1700 powers (35%) lost the
    # AttribMods buried inside nested Effects (BS_Bash, Crowd_Control, …).
    child_groups = []
    try:
        child_count = r.read_u4()
    except ValueError:
        child_count = 0  # No nested-groups field — older or empty effect group
    for _ in range(child_count):
        try:
            child_len = r.read_u4()
        except ValueError as e:
            # A count was present but the elements run past the record —
            # that's misalignment, not an absent field. Surface it.
            _warn_dropped(power_name, f"nested effect-group array misaligned ({e})")
            break
        child_reader = r.sub_reader(child_len)
        try:
            child = _parse_effect_group(child_reader, veracity=veracity,
                                        power_name=power_name,
                                        player_classes=player_classes)
            child_groups.append(child)
        except Exception as e:
            _warn_dropped(power_name, f"nested effect group parse failed ({e})")
        r.skip(child_len)

    # Skip any remaining effect group data (tail beyond children — usually 0).
    r.skip_to_end()

    return EffectGroup(
        chance=chance,
        ppm=ppm,
        delay=delay,
        radius_inner=radius_inner,
        radius_outer=radius_outer,
        requires_expression=requires,
        flags=flags,
        is_pvp=is_pvp,
        requires_pv=requires_pv,
        requires_default=gate.verdict,
        requires_archetypes=list(gate.archetypes),
        eval_flags=eval_flags,
        tags=tags,
        templates=templates,
        child_groups=child_groups,
    )


def _parse_redirects(r: BinReader, *, power_name: str = "") -> list[dict]:
    """Parse the Redirect struct_array from a power record.

    Each element: redirect target power name (string) + requires token list (string_array) +
    show_in_info (u4). `condition_expression` is that token list verbatim, so an
    unconditional redirect reads as the empty list or as the tautology `['1']` —
    whichever the record actually holds. This used to be flattened to the
    sentinel string 'Always', which a token list cannot carry without inventing
    a token; `_gate-default.cjs` applies the CoD2 'Always' convention instead,
    where it can see both spellings.

    Guarded against wrong-position reads: count > 1000 or elem_len > remaining
    raises so the caller can fall back to skip_to_end().
    """
    out = []
    count = r.read_u4()
    if count > 1000:
        raise ValueError(f"implausible redirect count {count} — wrong position?")
    for _ in range(count):
        elem_len = r.read_u4()
        if elem_len > (r._end - r._pos):
            raise ValueError(f"redirect elem_len {elem_len} exceeds remaining {r._end - r._pos}")
        er = r.sub_reader(elem_len)
        try:
            redirect_name = er.read_string()
            req_tokens = er.read_string_array()
            show_raw = er.read_u4()
            out.append({
                'name': redirect_name,
                'condition_expression': req_tokens,
                'show_in_info': bool(show_raw & 0xff),
            })
        except Exception as e:
            _warn_dropped(power_name, f"redirect element parse failed ({e})")
        r.skip(elem_len)
    return out


def _parse_effects(r: BinReader, *, veracity: bool = False,
                   power_name: str = "",
                   player_classes=()) -> tuple[list[EffectGroup], list[EffectGroup]]:
    """Parse the effects and activation_effects struct_arrays from a power record.

    The binary stores two parallel top-level structures:
    - regular `Effect` blocks (main effects struct_array)
    - `ActivationEffect` blocks (separate keyword block in the .def grammar;
      stored in a second struct_array immediately after the main effects)

    Returns (effects, activation_effects) as two distinct lists so the
    converter can treat them with different semantics (redirect-follow,
    self-buff filtering).

    NOTE: _parse_power MUST read the redirect pre-field + redirect struct_array
    BEFORE calling this, since they sit between modes_suspended and the effects
    struct_array in the record layout. Previously this function called
    skip_struct_array() + read_u4() here, but that was silently absorbing the
    redirect data (and happened to work by luck only when redirects were empty).
    """
    effects = []
    # Effects struct_array
    eff_count = r.read_u4()
    for _ in range(eff_count):
        eff_len = r.read_u4()
        eff_reader = r.sub_reader(eff_len)
        try:
            eg = _parse_effect_group(eff_reader, veracity=veracity,
                                     power_name=power_name,
                                     player_classes=player_classes)
            effects.append(eg)
        except Exception as e:
            _warn_dropped(power_name, f"effect group parse failed ({e})")
        r.skip(eff_len)

    # Veracity's power-record tail (parse6-derived) has no ActivationEffect
    # struct_array — effects are the final structure. Reading a second array
    # would consume garbage; stop here.
    if veracity:
        return effects, []

    # ActivationEffects struct_array — same layout as regular effects. The
    # count read is allowed to fail (not every power has this field present —
    # older or alternate layouts), but once a count is in hand, element
    # failures are misalignment and must surface.
    activation_effects = []
    try:
        act_count = r.read_u4()
    except ValueError:
        act_count = 0
    for _ in range(act_count):
        try:
            eff_len = r.read_u4()
        except ValueError as e:
            _warn_dropped(power_name, f"activation-effects array misaligned ({e})")
            break
        eff_reader = r.sub_reader(eff_len)
        try:
            eg = _parse_effect_group(eff_reader, power_name=power_name,
                                     player_classes=player_classes)
            activation_effects.append(eg)
        except Exception as e:
            _warn_dropped(power_name, f"activation effect group parse failed ({e})")
        r.skip(eff_len)

    return effects, activation_effects


def _parse_cast_flags(r: BinReader) -> tuple[list[str], list[str], int]:
    """Parse cast_flags structure (52 bytes); return
    (cast_through, toggle_ignore, castable_after_death).

    cast_through  = mez states the power can be ACTIVATED through (Blaster Defiance).
    toggle_ignore = mez states that DON'T detoggle the power (the toggle keeps
                    running while you're Held/Slept/Stunned).
    castable_after_death = eDeathCastableSetting enum (i24 parse-table order puts
                    it between TargetNearGround and CastThroughHold): AliveOnly=0 /
                    DeadOnly=1 / DeadOrAlive=2, exactly per the i24 header.
                    Census-verified 2026-07-21 against the `.powers` oracle
                    (117/123 exact; the 6 diffs are all the documented Willpower
                    Resurgence / "Up to the Challenge" HC-rework drift class).
                    Export maps it via CASTABLE_AFTER_DEATH in _enums.
    """
    # Layout: near_ground(4) + target_near_ground(4) + castable_after_death(4)
    #   + cast_through_hold(4) + cast_through_sleep(4) + cast_through_stun(4)
    #   + cast_through_terrorize(4) + toggle_ignore_hold(4) + toggle_ignore_sleep(4)
    #   + toggle_ignore_stun(4) + ignore_level_bought(4) + shoot_through_untouchable(4)
    #   + interrupt_like_sleep(4)
    # Total: 4+4+4 + 10×4 = 52 bytes
    r.skip(8)   # near_ground + target_near_ground
    castable_after_death = r.read_u4()
    cast_through_hold = r.read_bool()
    cast_through_sleep = r.read_bool()
    cast_through_stun = r.read_bool()
    cast_through_terrorize = r.read_bool()
    toggle_ignore_hold = r.read_bool()
    toggle_ignore_sleep = r.read_bool()
    toggle_ignore_stun = r.read_bool()
    r.skip(12)  # 3 remaining bools: ignore_level_bought + shoot_through + interrupt_like_sleep

    cast_through = []
    if cast_through_hold: cast_through.append("hold")
    if cast_through_sleep: cast_through.append("sleep")
    if cast_through_stun: cast_through.append("stun")
    if cast_through_terrorize: cast_through.append("terror")
    toggle_ignore = []
    if toggle_ignore_hold: toggle_ignore.append("hold")
    if toggle_ignore_sleep: toggle_ignore.append("sleep")
    if toggle_ignore_stun: toggle_ignore.append("stun")
    return cast_through, toggle_ignore, castable_after_death


# i24 iMaxBoosts parse-table default; a power record only differs when the
# authored def says `MaxBoosts N` (slot-locked powers use 0).
MAX_BOOSTS_DEFAULT = 6


class PowerTail(NamedTuple):
    """The named slice of the HC post-effects tail (see `_parse_power_tail`)."""
    max_boosts: int
    proc_allowed_raw: int
    strengths_disallowed: list[int]
    global_strengths_disallowed: list[int]
    unknown_bools_raw: list[int]
    proc_main_target_only: bool
    anim_main_target_only: bool
    # ShowInInventory / ShowInManage / ShowInInfo (SHOWFLAGS-1). Defaults are
    # the parse table's (ShowPowerSetting Default / true / true), so a failed
    # tail parse falls back to them the same way MaxBoosts falls back to 6.
    show_in_inventory_raw: int = 1
    show_in_manage: bool = True
    show_in_info: bool = True


def _parse_power_tail(r: BinReader) -> PowerTail:
    """Decode the leading slice of the HC post-effects tail.

    The tail follows the i24 ParseBasePower order (fields after `AttribMod`)
    with HC insertions. Verified 2026-07-21 against the `.powers` oracle
    (Shockwaves MaxBoosts 0 + ProcAllowed kNone, Punch StrengthsDisallowed
    kRange = attrib offset 348):

      +0x00 IgnoreStrength   +0x04 ShowBuffIcon      +0x08 ShowInInventory
      +0x0c ShowInManage     +0x10 ShowInInfo        +0x14 Deletable
      +0x18 Tradeable        +0x1c MaxBoosts (default 6)
      +0x20..+0x38 boost-flag block: DoNotSave / BoostIgnoreEffectiveness /
            BoostAlwaysCountForSet + 4 HC-added words (all-zero across the
            probe anchors; not yet named)
      +0x3c BoostTradeable (default 1)   +0x40 BoostCombinable (default 1)
      +0x44..+0x54 BoostAccountBound / BoostBoostable / BoostUsePlayerLevel /
            BoostCatalystConversion (str) / StoreProduct (str)
      +0x58 BoostLicenseLevel (default 999)  +0x5c MinSlotLevel (default -3)
      +0x60 MaxSlotLevel (49)  +0x64 MaxBoostLevel (50)
      then: Var (struct_array), ToggleDroppable (u4), ProcAllowed (HC-added
      u4; 0 = procs allowed, the only authored value is `ProcAllowed kNone`),
      StrengthsDisallowed (u4_array of attrib offsets — present in the client
      bin, contrary to HC-2's earlier server-only assumption),
      GlobalStrengthsDisallowed (u4_array, the same attrib-offset encoding),
      two unnamed HC bools, ProcMainTargetOnly, AnimMainTargetOnly.

    The four words past GlobalStrengthsDisallowed were decoded 2026-07-29
    (HC-3) against the `raw defs/**.powers` oracle over all 4,943 authored
    player powers, with GlobalStrengthsDisallowed itself confirmed two ways
    (15 authored powers, exact set AND per-power element counts, zero FP/FN —
    it is what shifted every earlier probe of this region):

      +0  unnamed bool — 0 on every authored power, 1 on 22 NPC powers
      +4  unnamed bool — 1 on every authored power, 0 on 30 NPC powers
      +8  ProcMainTargetOnly — 92 TP / 0 FP / 0 FN
      +c  AnimMainTargetOnly — 48 TP / 0 FP / 0 FN

    The two unnamed bools only ever vary on `5thColumn.Aereus_Goliath_*`
    powers, which have no authored def to name them from, so they are carried
    raw rather than discarded (fidelity rule: unknown is exportable, discarded
    is unrecoverable). Everything beyond AnimMainTargetOnly (a string_array of
    highlight/FX/position metadata) stays skipped by the caller's skip_to_end.
    Implausible values raise so the caller can drop the fields LOUDLY instead
    of shipping a misread.

    The three Show* words were skipped until SHOWFLAGS-1: ShowInInventory is
    the 5-value ShowPowerSetting enum (powers.h:436), ShowInManage/ShowInInfo
    are bools. `ShowInManage kFalse` + `MaxBoosts 0` is how the defs mark a
    set-mechanic grant (a power the game hands out and takes back) apart from
    a real auto-power pick, so skipping them made the converter's
    hiddenPassive/hiddenAuto arms unreachable and offered those grants as
    level picks.
    """
    r.skip(0x08)  # IgnoreStrength, ShowBuffIcon (the icon bool authors -1 = unset)
    show_in_inventory_raw = r.read_u4()
    # ShowPowerSetting (powers.h:436): 0 Never, 1 Default, 2 Always,
    # 3 IfUsable, 4 IfOwned. A 5th value would be a new game enum member —
    # surface it as a layout suspect rather than shipping a guess.
    if show_in_inventory_raw > 4:
        raise ValueError(
            f"implausible ShowInInventory {show_in_inventory_raw} — wrong position?")
    show_in_manage_raw = r.read_u4()
    show_in_info_raw = r.read_u4()
    for label, value in (("ShowInManage", show_in_manage_raw),
                         ("ShowInInfo", show_in_info_raw)):
        if value > 1:
            raise ValueError(f"implausible {label} {value} — wrong position?")
    r.skip(0x08)  # Deletable, Tradeable
    max_boosts = r.read_u4()
    # 999 is a real authored value (the PsionicNexus Psionic_Link NPC powers);
    # the guard only rejects values that can't be a slot count at all.
    if max_boosts > 1000:
        raise ValueError(f"implausible MaxBoosts {max_boosts} — wrong position?")
    r.skip(0x68 - 0x20)  # boost-flag block through MaxBoostLevel
    var_count = r.read_u4()
    if var_count > 64:
        raise ValueError(f"implausible Var count {var_count} — wrong position?")
    for _ in range(var_count):
        elem_len = r.read_u4()
        if elem_len > (r._end - r._pos):
            raise ValueError(f"Var elem_len {elem_len} exceeds remaining {r._end - r._pos}")
        r.skip(elem_len)
    toggle_droppable = r.read_u4()
    if toggle_droppable > 16:
        raise ValueError(f"implausible ToggleDroppable {toggle_droppable} — wrong position?")
    proc_allowed_raw = r.read_u4()
    if proc_allowed_raw > 16:
        raise ValueError(f"implausible ProcAllowed {proc_allowed_raw} — wrong position?")
    strengths_disallowed = r.read_u4_array()
    if len(strengths_disallowed) > 64 or any(
            v > 4096 or v % 4 for v in strengths_disallowed):
        raise ValueError(f"implausible StrengthsDisallowed {strengths_disallowed} — wrong position?")
    global_strengths_disallowed = r.read_u4_array()
    if len(global_strengths_disallowed) > 64 or any(
            v > 4096 or v % 4 for v in global_strengths_disallowed):
        raise ValueError(
            f"implausible GlobalStrengthsDisallowed {global_strengths_disallowed} "
            f"— wrong position?")
    unknown_bools_raw = [r.read_u4(), r.read_u4()]
    proc_main_target_only_raw = r.read_u4()
    anim_main_target_only_raw = r.read_u4()
    # All four are bools in the parse table; anything else means the layout
    # moved under us and the whole slice is suspect.
    for label, value in (("unknown tail bool 1", unknown_bools_raw[0]),
                         ("unknown tail bool 2", unknown_bools_raw[1]),
                         ("ProcMainTargetOnly", proc_main_target_only_raw),
                         ("AnimMainTargetOnly", anim_main_target_only_raw)):
        if value > 1:
            raise ValueError(f"implausible {label} {value} — wrong position?")
    return PowerTail(
        max_boosts=max_boosts,
        proc_allowed_raw=proc_allowed_raw,
        strengths_disallowed=strengths_disallowed,
        global_strengths_disallowed=global_strengths_disallowed,
        unknown_bools_raw=unknown_bools_raw,
        proc_main_target_only=bool(proc_main_target_only_raw),
        anim_main_target_only=bool(anim_main_target_only_raw),
        show_in_inventory_raw=show_in_inventory_raw,
        show_in_manage=bool(show_in_manage_raw),
        show_in_info=bool(show_in_info_raw),
    )


class Parse6PowerTail(NamedTuple):
    """The named slice of the Parse6/Thunderspy post-effects tail.

    Deliberately not `PowerTail`: Parse6 has no `GlobalStrengthsDisallowed`
    and no HC unnamed bools, and an empty list standing in for a field the
    layout does not contain is indistinguishable from one that was read and
    came back empty — the TSPY-5 shape.
    """
    max_boosts: int
    proc_allowed_raw: int
    strengths_disallowed: list[int]
    proc_main_target_only: bool
    anim_main_target_only: bool
    # ShowInInventory / ShowInManage / ShowInInfo (SHOWFLAGS-1) — stock i24
    # fields, same positions on both Parse6 flavors. Defaults are the parse
    # table's, the failed-tail fallback.
    show_in_inventory_raw: int = 1
    show_in_manage: bool = True
    show_in_info: bool = True


def _parse_power_tail_parse6(
        r: BinReader, *, rebirth_fork_words: bool,
        has_proc_allowed: bool) -> Parse6PowerTail:
    """Decode the leading slice of the Parse6/Thunderspy post-effects tail.

    The tail follows the stock i24 ParseBasePower order after `AttribMod`
    (the released-source parse table, no HC insertions):

      IgnoreStrength, ShowBuffIcon, ShowInInventory, ShowInManage,
      ShowInInfo, Deletable, Tradeable (7 u4 words),
      [Rebirth: 2 fork words bracketing MaxBoosts], MaxBoosts (default 6),
      DoNotSave, BoostIgnoreEffectiveness, BoostAlwaysCountForSet,
      BoostTradeable, BoostCombinable, BoostAccountBound, BoostBoostable,
      BoostUsePlayerLevel (8 u4 words),
      BoostCatalystConversion (string), StoreProduct (string),
      BoostLicenseLevel (default 999), MinSlotLevel (default -3),
      MaxSlotLevel (49), MaxBoostLevel (50),
      Var (struct_array), ToggleDroppable (u4),
      [Thunderspy: ProcAllowed (u4)],
      StrengthsDisallowed (u4_array of attrib offsets),
      ProcMainTargetOnly (u4 bool), AnimMainTargetOnly (u4 bool).

    Rebirth carries two fork-added words with no stock counterpart, one on
    each side of MaxBoosts (2026-07-21 corpus census, 21,559 records, zero
    guard failures: the pre word is 0 except 1 on six temp powers — a bool;
    the post word is 5 except 3 twice — an unnamed fork scalar). Thunderspy
    has neither; its fork additions are the ActivationEffect struct_array
    BEFORE this tail, which the caller consumes into `activation_effects`,
    and `ProcAllowed` — the HC-added word between ToggleDroppable and
    StrengthsDisallowed, which Thunderspy carries and stock Parse6 does not
    (TSPY-5, 2026-07-31). Everything beyond AnimMainTargetOnly
    (highlight/FX/position metadata) stays skipped by the caller's
    skip_to_end; Thunderspy has no GlobalStrengthsDisallowed, so the HC read
    is not simply inherited whole — continuing past StrengthsDisallowed under
    HC's layout breaks its guards on 299 records.

    The two main-target bools sit IMMEDIATELY after StrengthsDisallowed here
    (`powers_load.c:2192-2194`), where HC interposes GlobalStrengthsDisallowed
    and two unnamed bools. Recovered 2026-08-03 (PPM-3): the forks exported
    zero carriers because the reader stopped at StrengthsDisallowed, not
    because stock Parse6 lacks the fields — they are in the released parse
    table, so an all-zero fork was the WRAP-2 shape (suspect the reader before
    concluding a fork-wide absence). Proven four ways over the full corpus:
    both words are strictly 0/1 on all 21,559 Rebirth and 20,965 Thunderspy
    records; what follows decodes as HighlightEval's string_array (small
    counts, evaluator expressions like `cur.kToHit`); the two forks agree with
    each other on 91% of their union (191 of 209) while agreeing with HC far
    less, which is their closer ancestry; and the pairing is unambiguous
    against HC's defs-validated read — Proc↔Proc overlaps 108/150 where
    Proc↔Anim overlaps 22.

    The fork-vs-HC divergence is Gauntlet. The powers both forks flag and HC
    does not are `Tanker_Melee.*` (Battle Axe, War Mace, Brawling, Martial
    Arts, …), and `bUseNonBoostTemplatesOnMainTarget` is what scopes an i24
    Tanker attack's non-boost templates to the main target. Homecoming
    reworked Gauntlet and dropped the flag; the forks kept i24's.

    The three Show* words are stock i24 fields, so both flavors carry them at
    the same positions; read since SHOWFLAGS-1 (they were skipped, which made
    the converter's set-mechanic classification unreachable — see
    `_parse_power_tail`). Implausible values raise so the caller drops the
    fields LOUDLY instead of shipping a misread.
    """
    r.skip(0x08)  # IgnoreStrength, ShowBuffIcon
    show_in_inventory_raw = r.read_u4()
    # Same closed ShowPowerSetting vocabulary as the HC read; the value guard
    # doubles as the alignment check on both flavors, since a shifted read
    # here would be consuming Deletable/Tradeable or the Rebirth fork word.
    if show_in_inventory_raw > 4:
        raise ValueError(
            f"implausible ShowInInventory {show_in_inventory_raw} — wrong position?")
    show_in_manage_raw = r.read_u4()
    show_in_info_raw = r.read_u4()
    for label, value in (("ShowInManage", show_in_manage_raw),
                         ("ShowInInfo", show_in_info_raw)):
        if value > 1:
            raise ValueError(f"implausible {label} {value} — wrong position?")
    # Deletable, Tradeable, plus Rebirth's pre-MaxBoosts fork word.
    r.skip(4 * (3 if rebirth_fork_words else 2))
    max_boosts = r.read_u4()
    if max_boosts > 1000:
        raise ValueError(f"implausible MaxBoosts {max_boosts} — wrong position?")
    if rebirth_fork_words:
        r.skip(4)
    r.skip(4 * 8)  # DoNotSave through BoostUsePlayerLevel
    r.read_string()  # BoostCatalystConversion (non-empty on attuned boosts)
    r.read_string()  # StoreProduct
    license_level = r.read_u4()
    if license_level > 1000:
        raise ValueError(f"implausible BoostLicenseLevel {license_level} — wrong position?")
    min_slot = r.read_s4()
    if not -10 <= min_slot <= 100:
        raise ValueError(f"implausible MinSlotLevel {min_slot} — wrong position?")
    max_slot = r.read_s4()
    if not 0 <= max_slot <= 100:
        raise ValueError(f"implausible MaxSlotLevel {max_slot} — wrong position?")
    max_boost_level = r.read_s4()
    if not 0 <= max_boost_level <= 100:
        raise ValueError(f"implausible MaxBoostLevel {max_boost_level} — wrong position?")
    var_count = r.read_u4()
    if var_count > 64:
        raise ValueError(f"implausible Var count {var_count} — wrong position?")
    for _ in range(var_count):
        elem_len = r.read_u4()
        if elem_len > (r._end - r._pos):
            raise ValueError(f"Var elem_len {elem_len} exceeds remaining {r._end - r._pos}")
        r.skip(elem_len)
    toggle_droppable = r.read_u4()
    if toggle_droppable > 16:
        raise ValueError(f"implausible ToggleDroppable {toggle_droppable} — wrong position?")
    proc_allowed_raw = 0
    if has_proc_allowed:
        proc_allowed_raw = r.read_u4()
        if proc_allowed_raw > 16:
            raise ValueError(f"implausible ProcAllowed {proc_allowed_raw} — wrong position?")
    strengths_disallowed = r.read_u4_array()
    if len(strengths_disallowed) > 64 or any(
            v > 4096 or v % 4 for v in strengths_disallowed):
        raise ValueError(f"implausible StrengthsDisallowed {strengths_disallowed} — wrong position?")
    proc_main_target_only_raw = r.read_u4()
    anim_main_target_only_raw = r.read_u4()
    for label, value in (("ProcMainTargetOnly", proc_main_target_only_raw),
                         ("AnimMainTargetOnly", anim_main_target_only_raw)):
        if value > 1:
            raise ValueError(f"implausible {label} {value} — wrong position?")
    return Parse6PowerTail(
        max_boosts=max_boosts,
        proc_allowed_raw=proc_allowed_raw,
        strengths_disallowed=strengths_disallowed,
        proc_main_target_only=bool(proc_main_target_only_raw),
        anim_main_target_only=bool(anim_main_target_only_raw),
        show_in_inventory_raw=show_in_inventory_raw,
        show_in_manage=bool(show_in_manage_raw),
        show_in_info=bool(show_in_info_raw),
    )


def _parse_power(r: BinReader, *, has_field_45b: bool = True, has_field_41b: bool = False,
                 player_classes=()) -> PowerRecord:
    """Parse a single power record (HC Parse7 layout)."""

    # 1. key (string) — full_name
    full_name = r.read_string()
    # 2. crc (u4)
    r.read_u4()
    # 3. source (string)
    r.read_string()
    # 4. name (string)
    name = r.read_string()
    # 5. source_name (string)
    source_name = r.read_string()
    # 6. system (u4)
    r.read_u4()
    # 7. auto_issue (bool)
    auto_issue = r.read_bool()
    # 8. auto_issue_save_level (bool)
    auto_issue_keeps_level = r.read_bool()
    # 9. free (bool)
    free = r.read_bool()
    # 10. display (string) — display_name
    display_name = r.read_string()
    # 11. help (string) — display_help
    display_help = r.read_string()
    # 12. short_help (string)
    short_help = r.read_string()
    # 13. target_help (string)
    r.read_string()
    # 14. target_short_help (string)
    r.read_string()
    # 15. attacker_attack (string)
    r.read_string()
    # 16. attacker_attack_floater (string)
    r.read_string()
    # 17. attacker_hit (string)
    r.read_string()
    # 18. victim_hit (string)
    r.read_string()
    # 19. confirm (string)
    r.read_string()
    # 20. float_rewarded (string)
    r.read_string()
    # 21. power_defense_float (string)
    r.read_string()
    # 22. icon (string)
    icon = r.read_string()
    # 23. type (u4 enum power_type)
    power_type = r.read_u4()
    # 24. num_allowed (u4)
    num_allowed = r.read_u4()
    # 25. attack_types (attrib_array = u4 count + u4 values)
    attack_types = r.read_u4_array()
    # 26. buy_requires (string_array) — "requires"
    requires = r.read_string_array()
    # 27. activate_requires (string_array)
    activate_requires = r.read_string_array()
    # 28. slot_requires (string_array). Boost (IO piece) records carry
    # `BoostsSlotted>X <= 0` constraints here when the piece is unique
    # within a slot pool — used by the IO-set extractor to detect uniqueness.
    slot_requires = r.read_string_array()
    # 29. target_requires (string_array)
    target_requires = r.read_string_array()
    # 30. reward_requires (string_array)
    r.read_string_array()
    # 31. auction_requires (string_array)
    r.read_string_array()
    # 32. reward_fallback (string)
    r.read_string()
    # 33. accuracy (f4)
    accuracy = r.read_f4()
    # 34. cast_flags (52 bytes)
    cast_through, toggle_ignore, castable_after_death = _parse_cast_flags(r)
    # 35. ai_report (u4)
    r.read_u4()
    # 35b. HC extra field (u4)
    r.read_u4()
    # 36. effect_area (u4 enum)
    effect_area = r.read_u4()
    # 37. max_targets_hit (u4)
    max_targets_hit = r.read_u4()
    # 38. MaxTargetsExpr — HC-only string_array (absent from Parse6), the RPN
    # target-cap expression on Gauntlet attacks / the Electrical Affinity circuits.
    # VERIFIED 2026-07-01 against HC: `GauntletTargetCap` resolves here (59 powers),
    # matching the `.powers` oracle. Empty on non-capped powers.
    max_targets_expr = r.read_string_array()
    # 38b-d. HC-added OverCap block (authored next to MaxTargetsHit in the
    # `.powers` defs): OverCapTrigger (u4) + OverCapMultiplier (f4, default 1.0)
    # + OverCapExponential (u4 bool). VERIFIED 2026-07-21 against the oracle:
    # Aura_of_Insanity `OverCapTrigger 1 / OverCapMultiplier 0.9 /
    # OverCapExponential kTrue` reads 1 / 0x3F666666 / 1 here; default powers
    # read 0 / 1.0 / 0.
    over_cap_trigger = r.read_u4()
    over_cap_multiplier = r.read_f4()
    over_cap_exponential = r.read_u4() != 0
    # 39. radius (f4)
    radius = r.read_f4()
    # 40. arc (f4)
    arc = r.read_f4()
    # 41. chain_delay (f4) — per-jump delay on chain powers (Tesla_Cage 0.5,
    # Chain_Lightning 0.3; matches the `.powers` ChainDelay oracle).
    # 41b. The 2026 HC patch inserted two f4s BEFORE ChainDelay, not after it:
    # an area-factor override (0 = compute from geometry; Sonic_Boom 3.25,
    # Hypnotizing_Lights 1.0, Traps Trip_Mine 2.8) and PPMMod, a multiplier on
    # the PPM of procs rolled in this power (1.0 everywhere but the Sonic Aura
    # Sonic_Boom pseudo-pet's 2.0). Reading ChainDelay first labelled the
    # override as the delay and skipped the real one: on the live bin the third
    # word is the one carrying Chain_Lightning 0.3 / Tesla_Cage 0.5.
    area_factor_override = 0.0
    ppm_mod = 1.0
    if has_field_41b:
        area_factor_override = r.read_f4()
        ppm_mod = r.read_f4()
    chain_delay = r.read_f4()
    # 42. ChainEff (string_array) — per-jump chain-continue chance expression.
    # VERIFIED on Veracity/Parse6: `@ChainJump`/`minmax` content resolves here.
    chain_eff_expr = r.read_string_array()
    # 43. HC string_array, previously mislabeled "chain_fork" (the real ChainFork
    # is a u4 int-array, not a string_array). On Parse6 the same slot resolves to
    # VisualFX/power refs (…NictusFX), NOT the ChainTarget selection expression
    # (`prevdistance`/`maintarget>`, which is absent from Veracity entirely). So
    # this is most likely an FX/ChainIntoPower array, and ChainTarget lives
    # elsewhere in Parse7. UNVERIFIED for HC — captured neutrally for the probe.
    field43_str = r.read_string_array()
    # 43b. ChainTarget — next-target selection weighting for the Electrical Affinity
    # circuits (`… kHitPoints% target> - … maintarget> … prevdistance / +`). Stored
    # as a u4_array of string-table offsets — byte-identical to a string_array, so we
    # read it as one to resolve the RPN tokens (no desync). VERIFIED 2026-07-01
    # against the HC `.powers` oracle (circuits match exactly; 55 powers). Empty
    # (count 0) on non-chain powers.
    chain_target_expr = r.read_string_array()
    # 43c. HC extra: u4_array (boost indices for chain powers; empty for non-chain)
    r.read_u4_array()
    # 44. box_offset (f4 × 3)
    r.skip(12)
    # 45. box_size (f4 × 3)
    r.skip(12)
    # 45b. HC extra field added in ~2025 patch (u4)
    if has_field_45b:
        r.read_u4()
    # 46. range (f4)
    range_val = r.read_f4()
    # 47. range_secondary (f4)
    range_secondary = r.read_f4()
    # 48. time_to_activate (f4)
    time_to_activate = r.read_f4()
    # 48b. TimeToRoot (f4) — animation-lock/root duration, distinct from cast
    # time on ~50 powers (Stalker Assassin's Strike quick forms root 0.3–1.2s
    # vs 3.0–3.67s cast; Shadow Recall/teleporters). VERIFIED 2026-07-07 against
    # the `.powers` oracle (Bayonet 1.67, Heavy_Burst 2.5; 0 where the oracle
    # omits the field). Matches the Ghidra descriptor's TimeToRoot placement.
    # Parse6 genuinely omits this field (see _parse_power_parse6).
    time_to_root = r.read_f4()
    # 49. recharge_time (f4)
    recharge_time = r.read_f4()
    # 50. activate_period (f4)
    activate_period = r.read_f4()
    # 51. endurance_cost (f4)
    endurance_cost = r.read_f4()
    # 52. idea_cost (f4)
    r.read_f4()
    # 52b. HC-added MaxToggleTime (f4) — auto-shutoff for time-limited toggles
    # (Hibernate 30.0, Telekinesis 20.0; matches the `.powers` oracle). 0 on
    # ordinary toggles.
    max_toggle_time = r.read_f4()
    # 53. time_to_confirm (u4)
    r.read_u4()
    # 54. self_confirm (u4)
    r.read_u4()
    # 55. confirm_requires (string_array)
    r.read_string_array()
    # 56-65. Usage-limit / lifetime block. Discarded until LIFETIME-1: the
    # granted-power decay the chain's per-cast state needs (Combo_Level_1
    # authors `LifeTime 6`) was decoded here and thrown away.
    # 56. destroy_on_limit (bool)
    destroy_on_limit = r.read_bool()
    # 57. stacking_usage (bool)
    stacking_usage = r.read_bool()
    # 58. num_charges (u4)
    num_charges = r.read_u4()
    # 59. max_num_charges (u4)
    max_num_charges = r.read_u4()
    # 60. usage_time (f4)
    usage_time = r.read_f4()
    # 61. max_usage_time (f4)
    max_usage_time = r.read_f4()
    # 62. lifetime (f4)
    lifetime = r.read_f4()
    # 63. max_lifetime (f4)
    max_lifetime = r.read_f4()
    # 64. lifetime_in_game (f4)
    lifetime_in_game = r.read_f4()
    # 65. max_lifetime_in_game (f4)
    max_lifetime_in_game = r.read_f4()
    # 66. interrupt_time (f4)
    interrupt_time = r.read_f4()
    # 67. target_visibility (u4)
    target_visibility = r.read_u4()
    # 68. target (target_type = u4)
    target_type = r.read_u4()
    # 69. target_secondary (target_type = u4)
    target_type_secondary = r.read_u4()
    # 70. auto_hit (target_type_array)
    targets_autohit = r.read_u4_array()
    # 71. affected (target_type_array)
    targets_affected = r.read_u4_array()
    # 72. targets_through_vision_phase (bool)
    r.read_bool()
    # 73. boosts_allowed (boost_type_array)
    boosts_raw = r.read_u4_array()
    boosts_allowed = [BOOST_TYPE.get(v, f"Unknown({v})") for v in boosts_raw]
    # 74. mode_group_refs (u4_array of small integer indices — NOT
    # allowed_boostset_cats as this field was previously labeled). Empty for
    # ~97% of powers; non-empty on mode/stance-setting powers (Hide, Bio Armor
    # adaptations, Dual Pistols ammo modes, etc.) where the values index into
    # a mode/group enum whose table we haven't located yet. Decoding this
    # wrong as a string_array was the source of the long-standing "boostset
    # cats = olumnEndgame.NictusFX" garbage — the small ints resolved as
    # string-table offsets happened to land mid-string in FX-path data.
    #
    # `allowed_boostset_cats` does not exist in the binary at all. In the
    # live game, the categories an IO set can slot into are determined at
    # runtime from the set's own `category` string (see boostsets.bin) plus
    # the power's boost types. The planner converter does this inference —
    # see `inferAllowedSetCategories` in scripts/convert-powerset.cjs.
    r.read_u4_array()
    allowed_boostset_cats: list[str] = []

    # 75-78. exclusion_groups, modes_required, modes_disallowed, modes_suspended (u4_arrays)
    # The three mode arrays are indices into the global mode registry (same table
    # Set_Mode magnitudes index — attrib_names.bin ppchMode); captured raw here and
    # resolved to names at export. Empty on almost all powers.
    exclusion_groups = r.read_u4_array()
    modes_required = r.read_u4_array()
    modes_disallowed = r.read_u4_array()
    modes_suspended = r.read_u4_array()

    # The field here was previously read as a single u4 "redirect pre-field
    # (always 0 in samples)" — but it is actually a u4_array (a mode/recharge-
    # group list, e.g. ['kPostDeath'] on pet/entity powers). Reading it as a lone
    # u4 only worked when the array was EMPTY (count=0 reads identically); when
    # non-empty it consumed just the count and left the values in place, shifting
    # the redirect + effects reads by 4·N bytes → _parse_effects read a garbage
    # eff_count and the try/except below SILENTLY produced `effects: []`. This was
    # the root of the ~265 entity/pet powers (Trip Mine, Mastermind/Lore/Kheldian
    # pet abilities) that lost their entire effects array. Reading it as an array
    # is byte-identical for the empty case (no regression) and correct otherwise.
    # See GAME-DATA-PRINCIPLES §5 / HOMECOMING_PARSER.
    r.read_u4_array()  # mode/recharge-group array (usually empty; non-empty on pet powers)

    # Redirect struct_array — top-level Redirect{Power..Requires..} blocks from
    # the .def. Used by dual-mode powers (sniper slow/fast variants,
    # Energy_Transfer, etc.). Per-element: power_name (string) +
    # requires_tokens (string_array) + show_in_info (u4, 0xff=true).
    # On parse failure: skip to end rather than risk a hang on a garbage count.
    try:
        redirects = _parse_redirects(r, power_name=full_name)
    except Exception as e:
        # Fail LOUD: a redirect misparse here misaligns the effects read and
        # silently empties the power's effects (the ~265-power Trip Mine class).
        # Surface it so the next layout shift announces itself instead of hiding.
        redirects = []
        _warn_dropped(full_name, f"redirect parse failed ({e}); effects may be lost")
        r.skip_to_end()

    # Parse effects
    effects_aligned = True
    try:
        effects, activation_effects = _parse_effects(r, power_name=full_name,
                                                     player_classes=player_classes)
    except Exception as e:
        effects = []
        activation_effects = []
        effects_aligned = False
        _warn_dropped(full_name, f"effects parse failed ({e})")

    # Post-effects tail — the reader is only positioned at the tail when the
    # effects parse succeeded, so a misaligned record skips straight to the end
    # rather than reading garbage into named fields.
    tail = PowerTail(MAX_BOOSTS_DEFAULT, 0, [], [], [], False, False)
    if effects_aligned:
        try:
            tail = _parse_power_tail(r)
        except Exception as e:
            _warn_dropped(full_name,
                          f"post-effects tail parse failed ({e}); "
                          f"ShowIn*/MaxBoosts/ProcAllowed/StrengthsDisallowed/"
                          f"ProcMainTargetOnly lost")
            tail = PowerTail(MAX_BOOSTS_DEFAULT, 0, [], [], [], False, False)
    r.skip_to_end()

    return PowerRecord(
        full_name=full_name,
        name=name,
        source_name=source_name,
        display_name=display_name,
        display_help=display_help,
        short_help=short_help,
        icon=icon,
        power_type=power_type,
        num_allowed=num_allowed,
        auto_issue=auto_issue,
        auto_issue_keeps_level=auto_issue_keeps_level,
        free=free,
        attack_types=attack_types,
        requires=requires,
        activate_requires=activate_requires,
        target_requires=target_requires,
        effect_area=effect_area,
        max_targets_hit=max_targets_hit,
        range=range_val,
        range_secondary=range_secondary,
        radius=radius,
        arc=arc,
        time_to_activate=time_to_activate,
        time_to_root=time_to_root,
        recharge_time=recharge_time,
        activate_period=activate_period,
        endurance_cost=endurance_cost,
        interrupt_time=interrupt_time,
        destroy_on_limit=destroy_on_limit,
        stacking_usage=stacking_usage,
        num_charges=num_charges,
        max_num_charges=max_num_charges,
        usage_time=usage_time,
        max_usage_time=max_usage_time,
        lifetime=lifetime,
        max_lifetime=max_lifetime,
        lifetime_in_game=lifetime_in_game,
        max_lifetime_in_game=max_lifetime_in_game,
        accuracy=accuracy,
        target_type=target_type,
        target_type_secondary=target_type_secondary,
        target_visibility=target_visibility,
        targets_autohit=targets_autohit,
        targets_affected=targets_affected,
        boosts_allowed=boosts_allowed,
        allowed_boostset_cats=allowed_boostset_cats,
        cast_through=cast_through,
        toggle_ignore=toggle_ignore,
        slot_requires=slot_requires,
        chain_eff_expression=chain_eff_expr,
        chain_target_expression=chain_target_expr,
        max_targets_expression=max_targets_expr,
        castable_after_death=castable_after_death,
        chain_delay=chain_delay,
        area_factor_override=area_factor_override,
        ppm_mod=ppm_mod,
        over_cap_trigger=over_cap_trigger,
        over_cap_multiplier=over_cap_multiplier,
        over_cap_exponential=over_cap_exponential,
        max_toggle_time=max_toggle_time,
        max_boosts=tail.max_boosts,
        proc_allowed_raw=tail.proc_allowed_raw,
        strengths_disallowed=tail.strengths_disallowed,
        global_strengths_disallowed=tail.global_strengths_disallowed,
        tail_unknown_bools_raw=tail.unknown_bools_raw,
        proc_main_target_only=tail.proc_main_target_only,
        anim_main_target_only=tail.anim_main_target_only,
        show_in_inventory_raw=tail.show_in_inventory_raw,
        show_in_manage=tail.show_in_manage,
        show_in_info=tail.show_in_info,
        _field43_str=field43_str,
        effects=effects,
        activation_effects=activation_effects,
        redirects=redirects,
        modes_required=modes_required,
        modes_disallowed=modes_disallowed,
        modes_suspended=modes_suspended,
        exclusion_groups=exclusion_groups,
    )


# Known Thunderspy table-name prefixes used to locate the modifier table inside
# an effect template. The table sits in the middle of the variable-layout tail
# block; scanning for these prefixes is more robust than tracking offsets.
_TSPY_TABLE_PREFIXES = (
    "Melee_", "Ranged_", "Self_", "Inherent_", "Boost_", "Pet_",
    "Defense_", "Resist_", "Buff_", "Debuff_",
)

# The special-band attribs recovered from the thunderspy index array (TSPY-4).
# `Set_Mode` is the one slot whose magnitude is not a magnitude: it is the MODE
# INDEX, the datum `export_powers` resolves through `attrib_names.bin`.
_TSPY_MODE_SPECIALS = frozenset({"Set_Mode", "Unset_Mode"})

# Applied-mez attribs recoverable from the index array. Thunderspy names the
# APPLIED mez only in the post-`requires` index array — the front string is the
# enhancement/duration CATEGORY, so a Hold reads front `Immobilize`/`Sleep`, a
# Stun reads front `Stun` (verified: Blind/Freeze Ray/Fossilize front != index,
# index == HC's mez type on 415/422 shared powers). Relabel to the index type and
# adopt the post-table Magnitude (`mag_post_table`) — the flat template `magnitude`
# is a placeholder. EXCLUDED here (handled by table/target guards, not this set):
# `*_Res_Boolean` tables (mez PROTECTION thresholds, e.g. Shadowy_Binds), and the
# self/ally target-trap (an incarnate self-buff whose index names a mez) which is
# vetoed in the converter's guardThunderspyAppliedMez via `targets_affected`.
# Taunt/Placate/Untouchable/Intangible are deliberately omitted — the converter has
# no applied-effect path for them yet (relabel would be an inert no-op).
_TSPY_MEZ_INDEX = frozenset(
    {"Held", "Immobilized", "Stunned", "Sleep", "Confused", "Terrorized", "Afraid"}
)

# Offensive knockback/knockup recoverable from a `Ones`-front index array. The
# instant (duration 0), positive-scale ones are the attack knock-effects (Foot
# Stomp / Tremor / Dragon's Tail knockdown 0.67, Geyser / Tidal Wave knockup).
# A KB index carrying a DURATION or a non-positive / huge scale is anti-KB
# PROTECTION applied to the foe (immobilize's -KB, held-target ground-lock) or the
# caster's own KB protection — left as `Ones` (excluded), per GAME-DATA-PRINCIPLES
# §3. Front-real `Knockback` templates (Power Push, Energy Torrent) already flow
# through the converter's KB path, so only the `Ones`-front offensive ones need it.
_TSPY_KB_OFFENSIVE = frozenset({"Knockback", "Knockup"})

# Generic front-category tokens on Thunderspy INCARNATE HYBRID buff templates.
# The Support/Melee/… Hybrid buffs collapse every affected stat to ONE of these
# generic *category* tokens on a `*_Ones` unit table (the real per-attrib rows
# live only in the post-`requires` index array — see [[thunderspy-attrib-index-array]]).
# `Stunned` is deliberately absent: it is a lone-mez front already relabeled by
# the applied-mez path below (and is the help's inert "+Special" for team buffs).
_TSPY_HYBRID_GENERIC_FRONTS = frozenset({"Damage", "Ones", "Accuracy", "Defense"})

# Aspect classes for a hybrid buff's resolved index attribs. Thunderspy DROPS the
# AttribMod aspect (all-zero bytes), so we synthesize it from each attrib's nature:
# enhancement-strength buffs (+Damage/+Heal via any `*_Dmg`, +Accuracy, the
# "+Special" mez-strength) take aspect=Strength; positional/typed defense takes
# aspect=Current. Defense characteristics reuse the SAME indices as their damage
# twins — the bare name (no `_Dmg`) is the defense face (verified: front `Defense`
# → index `Melee`(27); front `Damage` → `Smashing_Dmg`(0)).
_TSPY_HYBRID_DEFENSE_ATTRIBS = frozenset({
    "Ranged", "Melee", "Area",                                      # positional
    "Smashing", "Lethal", "Fire", "Cold", "Energy",
    "Negative_Energy", "Psionic", "Toxic",                          # typed
    "Base_Defense",
})
_TSPY_HYBRID_MEZ_STRENGTH_ATTRIBS = frozenset({
    "Confused", "Afraid", "Terrorized", "Held", "Immobilized", "Stunned", "Sleep",
})


def _tspy_hybrid_aspect_class(attrib: str, *, melee: bool = False) -> str | None:
    """Classify a resolved hybrid index attrib into its synthesized aspect.

    Returns ``'Strength'`` for enhancement-strength buffs (+Damage/+Heal via any
    ``*_Dmg``, +Accuracy, +mez-strength), ``'Current'`` for defense
    (positional/typed defense characteristics), or ``None`` for anything else
    (mode markers, movement, summon — never a scoped hybrid buff). A template
    whose index mixes classes yields more than one class and is therefore left
    un-relabeled by the caller.

    ``melee=True`` selects the MELEE-tree reading of ``*_Dmg``: the Melee Hybrid
    (Core Genome/Graft/Embodiment) grants damage RESISTANCE, not a damage buff —
    HC and Rebirth both carry the same rows as ``Smashing_Dmg…Toxic_Dmg`` at
    aspect=Resistance, and tspy's own help text reads "+Res per nearby enemy".
    Only the Support tree's ``*_Dmg`` rows are Strength (+Damage) — labeling the
    melee rows Strength would feed a resistance buff into the converter's
    damage-buff branch.
    """
    if attrib.endswith("_Dmg"):
        return "Resistance" if melee else "Strength"
    if attrib == "Accuracy" or attrib in _TSPY_HYBRID_MEZ_STRENGTH_ATTRIBS:
        return "Strength"
    if attrib in _TSPY_HYBRID_DEFENSE_ATTRIBS:
        return "Current"
    return None


# Thunderspy's special-attrib band base. Unlike HC's and Rebirth's, which are
# constants, this one MOVES: adding an attrib slides the whole band, so it is
# calibrated from the corpus at parse time (`select_special_attrib_base`) and
# every table derived from it is re-anchored together by `_set_tspy_band_base`.
# A stale base does not fail loudly — it renames each affected template to its
# BAND NEIGHBOUR, which is a real attrib that real powers carry, so the result
# looks plausible. That is DATA-GAP TSPY-6 and TSPY-7 both.
_TSPY_BAND_BASE = 464
_TSPY_SPECIAL_BY_RAW: dict[int, str] = special_attrib_table(_TSPY_BAND_BASE)
_TSPY_ATTRIB_NAME: dict[int, str] = thunderspy_attrib_table(_TSPY_BAND_BASE)

# Raw attrib values seen in Thunderspy index arrays during the current parse —
# the evidence `select_special_attrib_base` calibrates the band base from.
_TSPY_RAW_ATTRIB_COUNTS: Counter = Counter()


def _set_tspy_band_base(base: int) -> None:
    """Re-anchor every consumer of the Thunderspy special-attrib band at once.

    The base is one fact with two consumers (the special map and the collapsed
    view in the normal table); they are set together here so neither can be
    updated without the other.
    """
    global _TSPY_BAND_BASE, _TSPY_SPECIAL_BY_RAW, _TSPY_ATTRIB_NAME
    _TSPY_BAND_BASE = base
    _TSPY_SPECIAL_BY_RAW = special_attrib_table(base)
    _TSPY_ATTRIB_NAME = thunderspy_attrib_table(base)


def resolve_attrib_thunderspy(raw: int) -> str:
    """Resolve a raw Thunderspy attrib value to its name, band-aware.

    The sibling resolvers for HC and Rebirth live in `_enums` because their
    band bases are constants; Thunderspy's is calibrated from the corpus at
    parse time (`select_special_attrib_base`), so this one has to read the
    live tables rather than a module-level snapshot — a caller using
    `ATTRIB_NAME_THUNDERSPY` directly gets whatever base the process started
    with, which is wrong for every build whose band has moved.
    """
    if raw in _TSPY_SPECIAL_BY_RAW:
        return _TSPY_SPECIAL_BY_RAW[raw]
    if raw >= _TSPY_BAND_BASE:
        return f"Special({raw})"
    return _TSPY_ATTRIB_NAME.get(raw // 4, f"Unknown({raw // 4})")

def _attach_thunderspy_redirects(templates: list) -> None:
    """Copy an element's redirect power names onto its Create_Entity templates.

    Thunderspy's EntCreate payload carries the EntityDef and the PriorityList
    but no redirect list — HC's four trailing name arrays are three here and
    every one of them is empty corpus-wide. The names are still in the element:
    they are the `power_names` of a sibling AttribMod's `Power` payload, which
    is 19 of the 21 the byte-scan this replaced used to recover (it read them
    out of the neighbouring bytes without knowing whose field they were).

    The remaining two are Wind Control's Vacuum, where the name sits inside an
    RPN expression in one of the sub-records whose tail does not decode; those
    surface through the undecoded-tail warning rather than being recovered here.

    `resolvePetLifespan` in convert-powerset.cjs reads `params.redirects` off
    the Create_Entity template to resolve a pseudopet's `*.Self_Destruct` delay,
    so the field has to live there rather than on the sibling that carries it.
    """
    redirects = [name for t in templates
                 if (t.params or {}).get('type') == 'Power'
                 for name in (t.params.get('power_names') or [])]
    if not redirects:
        return
    for t in templates:
        if (t.params or {}).get('type') == 'EntCreate':
            t.params['redirects'] = redirects


def _tspy_submods(strtab_data, elem_start: int, elem_end: int) -> list[tuple[int, int]]:
    """The (start, end) of every AttribMod sub-record in a Thunderspy effect element.

    A tspy effect element is HC's EffectGroup: header (front attribs, magnitude and
    duration defaults, requires-RPN) followed by a plain struct_array of AttribMods
    — `[count][ (len, bytes) × count ]`. The parser long read only sub-record 0,
    silently dropping 22,288 siblings across 8,537 powers (the knockback half of
    attacks, per-type damage, mez, `Set_Mode`/`Grant_Power`/`Revoke_Power`). Shape
    proven by walking it corpus-wide: the length chain closes on a constant 4-byte
    element tail for 54,999 of 55,004 elements, and adding the siblings moves the
    tspy attrib bag toward its Rebirth twin on 4,085 powers vs away on 207.

    Returns [] when the count is implausible — a misalignment signal, not data.
    """
    try:
        n = struct.unpack_from('<I', strtab_data, elem_start)[0]
        hpos = elem_start + 4 + 4 * n + 20
        rn = struct.unpack_from('<I', strtab_data, hpos)[0]
        # Two pad words sit between the requires array and the sub-record count.
        cpos = hpos + 4 + 4 * rn + 8
        count = struct.unpack_from('<I', strtab_data, cpos)[0]
    except Exception:
        return []
    if not 0 < count <= 64:
        return []
    out: list[tuple[int, int]] = []
    pos = cpos + 4
    for _ in range(count):
        if pos + 4 > elem_end:
            return []
        length = struct.unpack_from('<I', strtab_data, pos)[0]
        if length < 4 or pos + 4 + length > elem_end:
            return []
        out.append((pos + 4, pos + 4 + length))
        pos += 4 + length
    return out


# Thunderspy AttribMod sub-records whose tail past the stack quadruple didn't match
# the layout below. Same loud-at-parse-end contract as the Parse7 counters.
_undecoded_thunderspy_tails = 0

# Thunderspy sub-records whose DelayedRequires count does not leave room for the
# stack quadruple that must follow it. Such a count is a misalignment signal, not
# a long expression, so the stack fields are left undecoded rather than read from
# whatever the shifted offset happens to hold.
_misaligned_thunderspy_stack = 0


def _read_thunderspy_template_tail(
    strtab_data, strtab_base: int, start: int, end: int,
) -> dict | None:
    """Structurally decode a Thunderspy AttribMod's tail, from the byte after
    the stack quadruple to the end of the sub-record.

    Layout, pinned corpus-wide over 79,735 sub-records (99.94% account for
    every tail byte under it):

        CancelEvents     u4_array
        Suppress         struct_array   (12-byte records, HC's shape, with the
                                         Parse6 lineage's integer seconds; both
                                         arrays' ids resolve through
                                         EVENT_NAME_PARSE6, which is also this
                                         fork's numbering — see WRAP-3 there)
        BoostModAllowed  u4
        Flags            u4
        Flags2           u4
        Messages         struct_array   (16-byte record, see
                                         _TSPY_TEMPLATE_MESSAGE_FIELDS)
        FX               struct_array   (HC's shape)
        Params           tagged union   (HC's, type 0 = none)

    That is Homecoming's Parse7 tail minus `RequiredEvents` — the same single
    event array Parse6 carries. Which of HC's two arrays survives here is not a
    shape question (both are 12-byte event records): Super Speed answers it.
    Homecoming suppresses its RunningSpeed/JumpingSpeed mods on
    `ActivateAttackClick` for 4.0s and requires no event at all; Thunderspy's
    same templates read event 1, seconds 4, always 1 in this array. Reading it
    as RequiredEvents makes the converter drop them as conditional, and the
    fork's travel powers lose their speed.

    The slot after it is `BoostModAllowed`, not a second event array: on the 464
    records where it is non-zero, a Suppress count would overrun the record,
    while as a boost category it is 5 on a Hamidon enhancement's Accuracy row,
    9 on every damage row, 21 on Stunned, 20 on Sleep — one value per attrib.

    The union's own framing is the check that the walk arrived where it should:
    a typed payload's declared length equals the bytes left in the sub-record,
    and type 0 occupies exactly the final four.

    Returns None when any block violates its shape — the caller keeps the
    fixed-offset reading it had and the anomaly is counted, rather than a
    misaligned flags word shipping as data.
    """
    global _undecoded_thunderspy_tails
    r = BinReader(strtab_data, _string_table=(strtab_base, strtab_data),
                  _offset=start, _length=end - start)
    try:
        cancel_event_ids = r.read_u4_array()
        suppress_events = _read_event_records(r, event_names=EVENT_NAME_PARSE6,
                                              seconds_as_int=True)
        boost_mod_allowed_id = r.read_u4()
        flags_raw = r.read_u4()
        flags2_raw = r.read_u4()
    except (ValueError, struct.error):
        _undecoded_thunderspy_tails += 1
        return None
    messages, fx, tail_decoded = _read_template_messages_and_fx(
        r, _TSPY_TEMPLATE_MESSAGE_FIELDS
    )
    if not tail_decoded:
        _undecoded_thunderspy_tails += 1
        return None
    params, params_decoded = _read_params_union(r, thunderspy=True)
    if not params_decoded:
        _undecoded_thunderspy_tails += 1
        return None
    return {
        'cancel_events': [EVENT_NAME_PARSE6.get(v, f"Event_{v}")
                          for v in cancel_event_ids],
        'suppress_events': suppress_events,
        'boost_mod_allowed_id': boost_mod_allowed_id,
        'flags_raw': flags_raw,
        'flags2_raw': flags2_raw,
        'messages': messages,
        'fx': fx,
        'params': params,
    }


def _parse_effect_template_thunderspy(r: BinReader, strtab_data, strtab_base,
                                      *, incarnate_scope: str | None = None,
                                      sub_index: int = 0) -> tuple[EffectTemplate, dict]:
    """Parse a Thunderspy AttribMod template.

    Thunderspy predates the enum-coded AttribMod format used by HC (Parse7) and
    Parse6/Rebirth. Attribs are stored as STRING NAMES (string-table offsets)
    rather than enum indices, and the tail block beyond `requires` has a
    variable layout the source spec doesn't document publicly.

    Strategy: parse the well-defined element header (Tag-position attribs, then
    HC's EffectGroup block of Chance/PPM/Delay/RadiusInner/RadiusOuter and the
    requires-RPN) deterministically, then SCAN the remaining bytes for the
    modifier table name (`Melee_Damage`, `Ranged_Damage`, …) and pull the
    table-scale + duration that immediately follow it. This loses some of the
    rarely-used flags but recovers the load-bearing damage/heal/mez numbers the
    planner needs.

    Returns (template, group_extras): the element is HC's EffectGroup, so its
    header fields belong on the synthetic group the caller builds, not on the
    AttribMod — the same contract `_parse_effect_template_parse6` uses.
    """
    def _resolve_str(off: int) -> str | None:
        # Bound by the actual string-table length, not an arbitrary cap.
        # Thunderspy's string table is ~38 MB, so legitimate attrib strings
        # live at offsets well past any small heuristic ceiling — a 200000 cap
        # here silently dropped the `Defense` attrib (and 8k others), which is
        # why tspy defense/other-buff toggles lost their magnitude. abs_pos is
        # re-checked against the buffer below, so this only rejects the obvious
        # 0 / out-of-range cases.
        if off == 0 or off >= len(strtab_data):
            return None
        abs_pos = strtab_base + off
        if abs_pos <= 0 or abs_pos >= len(strtab_data):
            return None
        if strtab_data[abs_pos - 1] != 0:
            return None
        end = abs_pos
        limit = min(abs_pos + 256, len(strtab_data))
        while end < limit and strtab_data[end] != 0:
            end += 1
        try:
            s = bytes(strtab_data[abs_pos:end]).decode("ascii")
        except UnicodeDecodeError:
            return None
        if not s or not all(0x20 <= ord(c) < 0x7f for c in s):
            return None
        return s

    # Element bounds (r is a sub_reader scoped to this one effect element) — used by
    # the Buff_Def index-array recovery below to scan the whole element for the real
    # displaced defense array.
    _elem_start = r._pos

    # The element's name, at the position HC's Parse7 EffectGroup opens with `Tag`.
    # It is the string the global chance-mod system matches on: i24's
    # `ChanceModAccumulate` (MapServer/src/entity/character_combat.c:103) compares
    # `ptemplate->pchName` against each accumulated mod's filter name, which is the
    # role HC later moved onto the group's `Tag`. tspy descends from that schema and
    # hoists the name to the element, exactly one per element corpus-wide (79,308 of
    # 79,703 carry one, none carry two), so it rides `group_extras` onto the
    # synthetic group as `tags` — the field every consumer already asks for the
    # chance-mod bucket by.
    #
    # Read as a group-level attrib CATEGORY token until now, and used only as a
    # fallback attrib source, which is why the fork exported ZERO tagged groups
    # while carrying 218 distinct names here — `ColdDamage` 862, `ToxicDamage` 1102,
    # `FieryEmbrace` 1080, `PerfectionofBody/Mind/Soul` 430 each, i.e. precisely the
    # names this fork's own `Global_Chance_Mod` rows filter on.
    #
    # It is no longer an attrib fallback. Corpus-wide only 117 of 79,718 templates
    # decode no attrib index at all, and the 3 of those with a non-empty name all
    # read `Ones`, which is a mod name and never an attrib. The fallback's whole
    # remaining yield was one fabricated attrib.
    tag_offs = r.read_u4_array()
    tags = [n for n in (_resolve_str(o) for o in tag_offs) if n]
    attribs: list[str] = []

    # The five header floats are HC's EffectGroup header, field for field
    # (Chance/PPM/Delay/RadiusInner/RadiusOuter), not the per-AttribMod defaults
    # they were long read as (RB5-b2). What pins each one is its own corpus shape,
    # graded against the Rebirth and Homecoming exports over the aligned templates:
    #   chance        99.77% inside [0,1]; equals Rebirth's chance on 94.6%
    #   ppm           zero on 99.56%, and every non-zero one is a whole/half
    #                 procs-per-minute rate carried by a proc enhancement
    #   group_delay   zero corpus-wide
    #   radius_inner  −1.0 (HC's own default) on 96.3%
    #   radius_outer  −1.0 on 95.8%, the rest real radii in feet
    # Frag Grenade is the anchor: its Knockback element reads 0.5 here, which is
    # the `Chance 0.5` Homecoming and Rebirth both author — and which this parser
    # exported as `magnitude: 0.5` for as long as it has read the record.
    chance = r.read_f4()
    ppm = r.read_f4()
    group_delay = r.read_f4()
    radius_inner = r.read_f4()
    radius_outer = r.read_f4()
    # The ELEMENT's Requires — the cast-time gate on HC's EffectGroup, which a
    # tspy element is. It rides `group_extras` to the synthetic group the caller
    # builds; the template's own `jit_requires` is the AttribMod-level
    # DelayedRequires read further down, as it is on the other two forks.
    req_offs = r.read_u4_array()
    req_parts = [n for n in (_resolve_str(o) for o in req_offs) if n]
    group_requires = req_parts

    # This element's AttribMod sub-records; everything below reads the one
    # `sub_index` names. Falling back to the rest of the element keeps the
    # implausible-count case parsing exactly as it did before the walk existed.
    _subs = _tspy_submods(strtab_data, _elem_start, r._end)
    _sub_start, _sub_end = (_subs[sub_index] if sub_index < len(_subs)
                            else (r._pos + 16, r._end))

    # Attribs (index form) — the CANONICAL affected-attribute list, and the one this
    # AttribMod actually modifies. Each sub-record opens with
    # [count, count×(attribIndex*4)] (attrib index encoded as byte offset into the
    # 4-byte-per-entry attrib table, per GAME-DATA-PRINCIPLES). PEEKED (no reader
    # advance) so the table tail-scan below is unaffected.
    _idx_attribs = []
    _idx_specials = []
    _idx_count = 0  # raw entry count (before dropping unmapped indices)
    try:
        ipos = _sub_start
        icount = struct.unpack_from('<I', strtab_data, ipos)[0]
        if 0 < icount <= 32 and ipos + 4 + icount * 4 <= _sub_end:
            _idx_count = icount
            idxs = struct.unpack_from('<%dI' % icount, strtab_data, ipos + 4)
            _TSPY_RAW_ATTRIB_COUNTS.update(idxs)
            # A raw inside the band is a band member whatever its alignment, so
            # the byte-granular map below is asked first. The collapsed view
            # carries its own names for the eleven 4-aligned raws the band also
            # covers, and they are its neighbours' — see TSPY-7.
            _idx_attribs = [_TSPY_ATTRIB_NAME[v // 4] for v in idxs
                            if v % 4 == 0 and v not in _TSPY_SPECIAL_BY_RAW
                            and (v // 4) in _TSPY_ATTRIB_NAME]
            # The special/scripting band is BYTE-granular, so most of its entries
            # fail a 4-aligned filter and were once dropped whole. Thunderspy indexed it
            # from the same base 464 as Rebirth — both descend from the i24 source
            # whose ESpecialAttrib enum SPECIAL_ATTRIB_BAND transcribes. Established
            # against the Rebirth export as an oracle: of the tspy powers carrying
            # raw 469 that also exist on Rebirth, 64/64 set a mode there
            # (DATA-GAP TSPY-4).
            #
            # The base is NOT a constant, though — it is sizeof(CharacterAttributes),
            # and the 2026-07-30 build moved it to 468 by appending ReflectDamage.
            # So the table is the calibrated one for this build, chosen by
            # `select_special_attrib_base` from the raws collected just above,
            # and the min/max window is implicit in the map's own keys.
            _idx_specials = [_TSPY_SPECIAL_BY_RAW[v] for v in idxs
                             if v in _TSPY_SPECIAL_BY_RAW]
    except Exception:
        pass
    # The index array wins wherever it decodes. It is the mod's own attrib list;
    # the front is the element's category token. Graded against the Rebirth export
    # as an oracle over the 11,548 shared powers, this reading cuts the total
    # attrib-multiset distance from 86,966 to 11,278 and is the best of the three
    # readings on 9,441 of them (closer on 9,578, further on 224) — which is why the
    # per-band front relabels this replaced (`Ones`→recharge/recovery/regen, `Ones`/
    # `Control`→movement, TSPY-2's) each worked: they were narrow approximations of
    # this one fact.
    if _idx_attribs:
        attribs = _idx_attribs
    elif _idx_specials:
        attribs = _idx_specials

    # Scan the remainder for the first table-name string, then read the
    # canonical CoH AttribMod numeric block that follows it. Thunderspy shares
    # HC's Parse7 post-table layout *exactly* (verified by hand-decoding Gloom +
    # surveying 2,672 DoT damage templates against the Parse7 reader above):
    #
    #   table(str) scale(f4) duration(f4) magnitude(f4)
    #   dur_expr(u4_array) mag_expr(u4_array)   ← empty (count=0) on ~99% of templates
    #   delay(f4) application_period(f4) tick_chance(f4) ...
    #
    # On the ~99% with empty expr arrays, table+16 / table+20 hold a zero count,
    # so delay lands at table+24, the *period* (DoT tick interval) at table+28,
    # tick_chance at table+32. Reading them AS arrays (not fixed offsets) recovers
    # the RPN on the ~1% that carry one — Fury's `kRage source> .02 *`, the HP/
    # stack scalers (see `_read_str_array`) — AND keeps the period aligned
    # regardless. Recovering application_period is the fix for "every Thunderspy
    # DoT loses its tickRate" — without it the converter's `dmg.tickRate` is never
    # set and the calc can't compute tick count.
    tail_start = _sub_start
    tail = bytes(strtab_data[tail_start:_sub_end])
    table = ''
    scale = 0.0
    duration = 0.0
    app_period = 0.0
    delay = 0.0
    # Only reached when the tail scan finds no table at all, which is 0 records
    # corpus-wide; such a record has no scale either. The dataclass default keeps
    # that degenerate case reading as it did before the tick block was decoded.
    tick_chance = 1.0
    dur_expr = []
    mag_expr = []
    jit_requires = []
    caster_stack = ''
    stack = ''
    stack_limit = 0
    stack_key_id = 0
    flags: list[str] = []
    flags_raw = 0
    flags2_raw = 0
    cancel_events: list[str] = []
    suppress_events: list[dict] = []
    boost_mod_allowed_id = 0
    messages: dict | None = None
    fx: dict | None = None
    params: dict | None = None
    # Post-table magnitude (HC Parse7 layout: table scale duration MAGNITUDE).
    # For damage this is the ~1.0 per-tick multiplier; for MEZ it is the real
    # applied Magnitude (Mag 3 hold, Mag 4 taunt, …) — the template-level
    # `magnitude` read from the header above is only a flat placeholder on this
    # schema. Adopted below when we relabel a mez template to its index attrib.
    mag_post_table = 0.0
    # The canonical AttribMod TYPING block (aspect / application / type / target)
    # sits immediately BEFORE the table string, in HC's Parse7 field order —
    # `aspect, application, type, target, <pad>, table` — with aspect ÷8 (HC
    # encoding, NOT Parse6's ÷4) and the rest direct enums. Thunderspy shares this
    # layout; it was long thought to "leave the aspect all-zero" only because the
    # parser looked at the element-header words after the chance (PPM + Delay, a float +
    # zero pad), never at the pre-table block. Recovered 2026-07-19 (TSPY-3): read
    # backward from the located table. Corpus-wide the raw values are pure enum
    # indices — aspect ∈ {0,8,16,24,32}, target ∈ {0,1,2,4,5}, etc. — and reproduce
    # HC/Rebirth aspect/target/type at ~99% on unambiguous keys, matching the
    # parser's own prior synthesized aspect on 100% of the 803 synthesized templates
    # (so the real field SUBSUMES the synthesis workaround). See DATA-GAP TSPY-3.
    raw_aspect = ""
    raw_application = ""
    raw_type = ""
    raw_target = ""
    for k in range(0, len(tail) - 4, 4):
        u = struct.unpack_from('<I', tail, k)[0]
        candidate = _resolve_str(u)
        if not candidate or not candidate.startswith(_TSPY_TABLE_PREFIXES):
            continue
        table = candidate

        # Typing block, read backward from the table (guarded; the earliest tables
        # sit past the index array so k >= 20 in practice). Out-of-enum reads —
        # the ~0.02% where the tail-scan lands on a spurious table string — decode
        # to '' (honest "couldn't determine", never a fabricated default).
        def _typ_u4(off: int) -> int | None:
            return (struct.unpack_from('<I', tail, off)[0]
                    if 0 <= off and off + 4 <= len(tail) else None)

        _asp = _typ_u4(k - 20)
        if _asp is not None and _asp % 8 == 0:
            raw_aspect = ATTRIB_MOD_ASPECT.get(_asp // 8, "")
        _app = _typ_u4(k - 16)
        if _app is not None:
            raw_application = ATTRIB_MOD_APPLICATION.get(_app, "")
        _typ = _typ_u4(k - 12)
        if _typ is not None:
            raw_type = ATTRIB_MOD_TYPE.get(_typ, "")
        _tgt = _typ_u4(k - 8)
        if _tgt is not None:
            raw_target = ATTRIB_MOD_TARGET.get(_tgt, "")

        def _f4(off: int) -> float:
            return struct.unpack_from('<f', tail, off)[0] if off + 4 <= len(tail) else 0.0

        def _read_str_array(off: int) -> tuple[str, int]:
            """Read a [u4 count][count×u4 string-offset] block; return the RPN
            token list and the offset past the block.

            Each u4 is a string-table offset (the same convention as the front
            attrib offsets), so an Expression template's magnitude/duration RPN
            survives — the Thunderspy analogue of HC/Parse6's
            `read_string_array()` (e.g. Fury's `kRage source> .02 *`).
            Empty for ~99% of templates (count=0 → +4). The `count*4` advance
            keeps the trailing delay/period offsets aligned."""
            if off + 4 > len(tail):
                return [], off + 4
            count = struct.unpack_from('<I', tail, off)[0]
            # A bogus count means we're misreading; treat as empty so the fixed
            # post-table offsets still apply.
            if count > 64:
                return [], off + 4
            tokens = []
            p = off + 4
            for _ in range(count):
                token = _resolve_str(struct.unpack_from('<I', tail, p)[0])
                if token:
                    tokens.append(token)
                p += 4
            return tokens, p

        scale = _f4(k + 4)
        duration = _f4(k + 8)
        mag_post_table = _f4(k + 12)  # real applied Magnitude (see note above)
        # k+12 is the post-table magnitude; the two expression arrays follow.
        pos = k + 16
        dur_expr, pos = _read_str_array(pos)   # DurationExpr RPN (usually empty)
        mag_expr, pos = _read_str_array(pos)   # MagnitudeExpr RPN (Fury: kRage source> .02 *)
        delay = _f4(pos)
        app_period = _f4(pos + 4)   # application_period — the DoT tick interval

        # First word of the tick block, read rather than stepped over (RB5-b2):
        # `tick_chance` equals Homecoming's on 98.2% of the 40,142 templates the
        # two forks share, which is what identifies it as the PER-TICK roll and
        # not the group's application roll (that one is the element header's
        # `chance`, and the two disagree on 10,158 records). Fire Sword is the
        # anchor — its 0.1-scale burn reads 0.8 here against a group chance of
        # 1.0, exactly as Homecoming authors it.
        tick_chance = _f4(pos + 8)
        # Thunderspy has no TickMultiplier/TickAdditive — HC's two additions to
        # this block are absent, and `pos + 12` is already the next field.

        def _stack_u4(off: int) -> int | None:
            return (struct.unpack_from('<I', tail, off)[0]
                    if off + 4 <= len(tail) else None)

        # DelayedRequires — the AttribMod's own application-time gate
        # (`ppchApplicationRequires`), a string_array in HC's Parse7 position:
        # immediately after the tick block and immediately before the stack
        # quadruple. It is empty on 79,655 of 79,718 sub-records, which is why a
        # fixed read of the quadruple at pos+20 looked right for so long — and
        # why the 63 that carry one had their stack, limit, key, flags and whole
        # tail read 4*(1+N) bytes early. 61 surfaced as `Unknown(<string offset>)`;
        # 2 landed on a valid enum and were silently wrong. The array's own count
        # is the only thing that says where the quadruple starts.
        _dr_count = _stack_u4(pos + 12)
        # Distrust the count: it is credible only if the array AND the four words
        # that must follow it fit inside the sub-record. Anything else is a
        # misalignment, and reading the quadruple from a shifted offset is exactly
        # the failure this block exists to fix — so leave the fields undecoded and
        # say so, rather than recover into a plausible wrong answer.
        _stack_at = pos + 16 + 4 * (_dr_count or 0)
        if _dr_count is None or _stack_at + 16 > len(tail):
            global _misaligned_thunderspy_stack
            _misaligned_thunderspy_stack += 1
        else:
            if _dr_count:
                jit_requires = [
                    t for t in (_resolve_str(_stack_u4(pos + 16 + 4 * i) or 0)
                                for i in range(_dr_count)) if t]
            _caster_raw = _stack_u4(_stack_at)
            caster_stack = ATTRIB_MOD_CASTER_STACK.get(
                _caster_raw, f"Unknown({_caster_raw})")
            # `stack` is what separates a per-target INCREMENT (`Stack`) from the
            # cap that replaces it (`Replace`) — the field
            # `computeAoePerTargetPatches` keys on, and with it blank every
            # Thunderspy AoE self-buff skipped that pass entirely. Anchored on
            # Soul Drain, whose two ToHit sub-records read 0/`Stack` (scale 0.2)
            # and 3/`Replace` (scale 1.0) with stack_limit 2 — byte-for-byte what
            # its HC and Rebirth twins carry.
            _stack_raw = _stack_u4(_stack_at + 4)
            stack = ATTRIB_MOD_STACK.get(_stack_raw, f"Unknown({_stack_raw})")
            stack_limit = _stack_u4(_stack_at + 8)
            _key_raw = _stack_u4(_stack_at + 12)
            stack_key_id = 0 if _key_raw == 0xFFFFFFFF else _key_raw

        # AttribMod flags — a PACKED bitmask here, not Parse6's loose bool block.
        # Anchored on Dull Pain, whose two otherwise byte-identical +MaxHP
        # sub-records differ in this one word by exactly 0x10 (`IgnoreStrength`),
        # reproducing the enhanceable/unenhanceable split HC and Rebirth carry.
        # Corpus-wide only 2 of 79,171 reads fall outside the known bit positions.
        #
        # The word is one slot SHORT of Parse7: tspy has no bit-7 (`IgnoreEfficiency`)
        # slot, so every flag from there up sits one position lower. Graded against
        # HOMECOMING — the only oracle that can carry bit 7, since Parse6 synthesizes
        # its word from bools and never sets it — over 11,330 shared powers, this
        # renormalization lifts mean flag agreement 90.98% → 96.74%, IgnoreEfficiency
        # 57.9% → 99.6% and Boost 77.0% → 99.2%. Reading it as-read left every tspy
        # enhancement piece without its `Boost` flag.
        #
        # The word's POSITION comes from the structural walk of everything past
        # the stack quadruple (`_read_thunderspy_template_tail`), because the
        # three blocks in between are variable-length: on the 6,724 sub-records
        # that carry a CancelEvents id or a RequiredEvents record, the fixed
        # slot holds one of those instead of the flags. The fixed read stays as
        # the fallback for the tail shapes the walk rejects. Both offsets start
        # from the end of the quadruple, so a DelayedRequires array moves them
        # with it — reading from a fixed `pos` is what left the 63 records that
        # carry one with a tail the walk could not match.
        _after_stack = _stack_at + 16
        _tail = _read_thunderspy_template_tail(
            strtab_data, strtab_base, tail_start + _after_stack, _sub_end
        )
        _flags_raw = _tail['flags_raw'] if _tail else _stack_u4(_after_stack + 12)
        if _flags_raw is not None and _flags_raw < (1 << 24):
            flags_raw = (_flags_raw & 0x7F) | ((_flags_raw & ~0x7F) << 1)
            flags = _decode_flags(flags_raw)
        if _tail:
            flags2_raw = _tail['flags2_raw']
            cancel_events = _tail['cancel_events']
            suppress_events = _tail['suppress_events']
            boost_mod_allowed_id = _tail['boost_mod_allowed_id']
            messages = _tail['messages']
            fx = _tail['fx']
            params = _tail['params']
        break

    # Both chances carry exactly as read, including the 28 records outside [0,1]
    # (Fiery Melee's −0.2, the Seed of Hamidon −1/−2/−3 ladder, White Dwarf's 3.0).
    # Those are not misalignment: Rebirth authors the same −0.2 on the same
    # templates. What they MEAN is the open question RB5-b1 carries for all three
    # forks, so this path preserves the value rather than deciding it here.
    if duration < 0:
        duration = 0.0
    if app_period < 0:
        app_period = 0.0
    if delay < 0:
        delay = 0.0
    # Resistance armor: the element's category token is `Res_DMG` while the mod's own
    # attribs (`Smashing_Dmg`, `Lethal_Dmg`, …) come from the index array above. The
    # token is what identifies the rows as RESISTANCE rather than damage — the aspect
    # itself is synthesized below, after the `aspect` reset. (Verified 2026-07-09
    # against the raw tspy bin: `Res_DMG`-front templates carry per-type `*_Dmg` index
    # attribs on `*_Res_DMG` tables.)
    _tspy_res_surfaced = (bool(table) and 'Res_DMG' in table
                          and tags == ['Res_DMG'] and bool(_idx_attribs))

    # The table scale IS the scale on this schema. It used to fall back to the
    # header word for a tableless record, which the RB5-b2 decode retires — that
    # word is the group's Chance, and no record reaches here without a table.
    final_scale = scale

    # Magnitude comes from the post-table slot, where HC's field order puts it
    # (`table scale duration MAGNITUDE`). It reads 1.0 on 91.7% of templates —
    # the same unscaled placeholder Homecoming carries — with the real applied
    # values behind it (mez Mag 2/3/4, the −50/100 percentage effects). The two
    # narrow branches that used to adopt it for modes and applied mez existed
    # only because the header word held this slot; with that word identified as
    # Chance the field needs no special case.
    magnitude = mag_post_table

    # Aspect. Now read directly from the pre-table typing block (raw_aspect,
    # above) instead of left blank — Thunderspy DOES carry the aspect, it just
    # lives before the table, not in the header. The scoped synthesis blocks below
    # (resistance armor, incarnate hybrid/destiny) still run and set the same
    # value the raw read gives (verified: raw == synthesized on 100% of the
    # previously-synthesized templates), so they are harmless belt-and-suspenders
    # that also do the ATTRIB RELABEL half; the raw read is what now populates the
    # ~55k templates the synthesis never covered.
    aspect = raw_aspect

    # Resistance synthesis. Any `*_Dmg` attrib(s) riding a `*_Res_Dmg` resistance
    # table with the aspect dropped (Thunderspy zeroes it) is RESISTANCE, never
    # damage — damage never uses a resistance table. Synthesize aspect='Resistance'
    # here (after the reset above, so it isn't clobbered) so those rows route to the
    # converter's resistance branch instead of the damage branch (the
    # `_Dmg`-table-suffix trap). Sign is left to `scale`: a −resistance debuff is
    # aspect=Resistance too, split buff-vs-debuff by the converter. Covers both:
    #   • `Res_DMG`-front armor surfaced above to per-type index attribs
    #     (`_tspy_res_surfaced`) — Mind Over Body, Willpower, High Pain Tolerance;
    #   • templates whose front is ALREADY the real damage-type token — Glacial
    #     Shield's +Res(Cold), Corrosive Sap / Enervating Field's −Res(all) — which
    #     would otherwise be read as Cold damage etc.
    # The bare `Res_DMG` category token is excluded (it names no per-type attrib, so
    # the converter can't classify it — those debuffs are a separate open item).
    if _tspy_res_surfaced:
        aspect = "Resistance"
    elif (table and table.lower().endswith('_res_dmg') and attribs
            and all(a.lower().endswith('_dmg') and a.lower() != 'res_dmg' for a in attribs)):
        aspect = "Resistance"

    # Thunderspy INCARNATE HYBRID generic-token relabel. The Support/Melee/… Hybrid
    # buffs collapse every affected stat to a single generic *category* front token
    # (`Damage`/`Ones`/`Accuracy`/`Defense`) on a `*_Ones` unit table and DROP the
    # aspect — where HC/Rebirth carry the real per-attrib rows WITH their aspect (see
    # [[thunderspy-attrib-index-array]]). The real affected attribs already sit in the
    # decoded index array (`_idx_attribs`); relabel front → those attribs and
    # synthesize the aspect the category implies, so the tspy export matches HC/Rebirth
    # shape and flows through the converter's shared branches (retiring the
    # `TSPY_GENERIC_HYBRID_MAP` converter band-aid).
    #
    # Scoped to the HYBRID slot (`incarnate_scope` 'hybrid'/'hybrid_melee'): an Alpha
    # enhancement carries a byte-identical `Damage`-front + `*_Ones`-table +
    # `*_Dmg`-index shape but is a different (filename-driven) converter path — only
    # the SLOT tells them apart. Fires only when EVERY index attrib maps to ONE
    # aspect class (all Strength / all Current / all Resistance — the last only on
    # the melee tree, whose `*_Dmg` rows are damage RESISTANCE per HC/Rebirth/help,
    # see `_tspy_hybrid_aspect_class`): the empty-index Assault/Control grant shells
    # are deliberately left as the generic token.
    if (incarnate_scope in ("hybrid", "hybrid_melee") and len(tags) == 1
            and tags[0] in _TSPY_HYBRID_GENERIC_FRONTS
            and table.endswith("_Ones") and _idx_attribs and final_scale > 0):
        _melee = incarnate_scope == "hybrid_melee"
        _classes = {_tspy_hybrid_aspect_class(a, melee=_melee) for a in _idx_attribs}
        if len(_classes) == 1 and None not in _classes:
            aspect = next(iter(_classes))

    # Scoped to the DESTINY slot: the same generic-front + index-array shape carries
    # the Destiny buffs (verified against the raw tspy bin + the HC parallels by
    # (scale, duration) alignment, 2026-07-07 — see DEDUCTIVE_SCHEMA_HARNESS.md):
    #   - `Ones` + all-defense index, positive     → Barrier's +DEF (bare attrib @
    #     Current = defense, the DSH8 bridge-convergence invariant; tspy Barrier
    #     carries NO `*_Dmg` twins, i.e. defense-only, diverging from HC's def+res)
    #   - `Ones` + all-mez index, negative         → Clarion's mez PROTECTION
    #     (HC: 6-mez @ Current, PvE tier -21/-30)
    #   - `Ones` + KB/KU index, negative           → Clarion's KB protection (-10.5)
    #   - `Res_Boolean` + all-mez index            → Clarion's status-duration
    #     RESISTANCE twin; its `isPVPMap?` requires makes the converter drop it in
    #     PvE, matching HC's explicit is_pvp=PVP_ONLY on the same rows
    #   - `Tempdamage` + lone Heal_Dmg index       → Rebirth's click heal (negative
    #     Absolute Heal_Dmg, HC-identical scale -8/-7/-6/-5)
    #   - `ArchVillain_Res` front                  → Ageless Radial's debuff-
    #     resistance bundle (index = the ~20-attrib resistance list HC folds to
    #     debuffResistance); front is already real, only the aspect is synthesized.
    if incarnate_scope == "destiny" and len(tags) == 1:
        _front = tags[0]
        _idxset = set(_idx_attribs)
        if _front == "Ones" and _idx_attribs:
            if _idxset <= _TSPY_HYBRID_DEFENSE_ATTRIBS and final_scale > 0:
                aspect = "Current"
            elif _idxset <= _TSPY_MEZ_INDEX and final_scale < 0:
                aspect = "Current"
            elif _idxset <= _TSPY_KB_OFFENSIVE and final_scale < 0:
                aspect = "Current"
        elif _front == "Res_Boolean" and _idx_attribs and _idxset <= _TSPY_MEZ_INDEX:
            aspect = "Resistance"
        elif _front == "Tempdamage" and _idx_attribs == ["Heal_Dmg"]:
            aspect = "Absolute"
        elif _front == "ArchVillain_Res":
            aspect = "Resistance"

    # The post-table Magnitude adopted above is also where this schema keeps a
    # mode set/unset's MODE INDEX — the same datum HC/Rebirth carry in their
    # template magnitude, which `export_powers` resolves through
    # `attrib_names.bin`. Verified by taking the mode NAME each power's Rebirth
    # twin sets, looking it up in THUNDERSPY's own registry, and finding that
    # index in the template: 85/85 at this field (DATA-GAP TSPY-4). Without it
    # every tspy `Source.Mode?` gate is unbindable — the Primalist forms and Bio
    # Armor's adaptations alike. It is likewise the applied mez Magnitude (Tesla
    # Cage's Hold, Blind's `Immobilize`-front Hold). Both used to need their own
    # branch to reach past the header word; neither does now.

    # Second flags word — the attrib-contextual keywords Homecoming keeps in the
    # same slot, and read here with the same per-attrib gate. It is what puts
    # `CopyBoosts` on this fork's 2,144 entity-creating and executing templates
    # (HC-4's marker), `RevokeAll` on its Revoke_Power mods and the
    # SetTimer/AdjustTimer/Cooldown triple on its Recharge_Power ones; every
    # value seen is a subset of the bits those classes use on HC.
    if flags2_raw:
        flags = flags + _decode_flags2(flags2_raw, attribs)

    r.skip_to_end()
    template = EffectTemplate(
        attribs=attribs,
        type=raw_type,
        application_type=raw_application,
        aspect=aspect,
        target=raw_target,
        table=table,
        scale=final_scale,
        duration=duration,
        magnitude=magnitude,
        delay=delay,
        application_period=app_period,
        tick_chance=tick_chance,
        duration_expression=dur_expr,
        magnitude_expression=mag_expr,
        jit_requires=jit_requires,
        caster_stack=caster_stack,
        stack=stack,
        stack_limit=stack_limit,
        stack_key_id=stack_key_id,
        cancel_events=cancel_events,
        suppress_events=suppress_events,
        boost_mod_allowed=str(boost_mod_allowed_id) if boost_mod_allowed_id else "",
        boost_mod_allowed_id=boost_mod_allowed_id,
        flags=flags,
        flags_raw=flags_raw,
        flags2_raw=flags2_raw,
        messages=messages,
        fx=fx,
        params=params,
    )
    # The element's own fields, for the synthetic group the caller wraps this in
    # — the same (template, group_extras) contract the Parse6 path uses.
    group_extras = {
        'chance': chance,
        'ppm': ppm,
        'delay': group_delay,
        'radius_inner': radius_inner,
        'radius_outer': radius_outer,
        'requires_expression': group_requires,
        'tags': tags,
    }
    return template, group_extras


# Rebirth templates whose post-magnitude tail didn't match the validated
# layout and fell back to the heuristic scanner. Same loud-at-parse-end
# contract as the Parse7 counters.
_undecoded_parse6_tails = 0

# Parse6 tail bools in struct order, with the HC flag name and HC bit each
# one maps to (first flags word unless noted). Named via HC pair-validation
# 2026-07-20 (61k 1:1-paired templates): StackExactPower matched 1303/1303,
# DoNotTintCostume 74/80, VanishEntOnTimeout 44/44 + 6 fork-skew extras,
# DelayEval 15/19; KeepThroughDeath is the top correlate for its slot at
# 28/43 with the disagreements clustered in sets HC re-authored. HideZero
# never 1:1-paired (its 118 carriers are all multi-template Stalker crit
# attacks) but every HC twin of those carriers puts HideZero on the same
# crit-conditional damage template.
_PARSE6_TAIL_BOOL_FLAGS = (
    ("HideZero", 0x002000, False),
    ("StackExactPower", 0x080000, False),
    ("VanishEntOnTimeout", 0x000001, True),   # second flags word
    ("DoNotTintCostume", 0x000002, True),     # second flags word
    ("KeepThroughDeath", 0x004000, False),
    ("DelayEval", 0x008000, False),
)


def _read_parse6_template_tail(r: BinReader) -> dict:
    """Structurally decode the Parse6 AttribMod post-magnitude tail.

    Layout pinned empirically corpus-wide (2026-07-20, 122,156 Rebirth
    templates, zero anomalies) and field-validated against HC Parse7 twins
    of the same powers (suppress seconds/FX strings/bools/PPM):

      RadiusInner f4, RadiusOuter f4 (-1 = unset, matching HC group fields),
      Suppress struct_array (rec_len 12: event u4 in EVENT_NAME_PARSE6
        numbering, seconds u4 — an integer, unlike Parse7's f4 — always u4),
      IgnoreSuppressErrors pstring (dev-signature, e.g. "floyd"; HC folds it
        into flag bit 0x100000),
      ContinuingBits u4_array, ContinuingFX pstring,
      ConditionalBits u4_array, ConditionalFX pstring,
      CostumeName pstring, PowerName pstring, PhaseName pstring,
      EntityDef pstring, PriorityList pstring ×3 (identical across the
        corpus — authored once, serialized thrice),
      six bool u4s (_PARSE6_TAIL_BOOL_FLAGS),
      BoostModAllowed u4, EvalFlags u4 (Parse6 bit numbering; HC inserted a
        bit at position 0, proven by the reb→HC anchors 1→2, 2→4, 17→34),
      ProcsPerMinute f4.

    Raises ValueError on any structural violation so the caller can rewind
    to the heuristic scanner instead of shipping a misread.
    """
    out: dict = {}
    out["radius_inner"] = r.read_f4()
    out["radius_outer"] = r.read_f4()

    suppress_count = r.read_u4()
    if suppress_count > 16:
        raise ValueError(f"implausible suppress count {suppress_count}")
    suppress = []
    for _ in range(suppress_count):
        rec_len = r.read_u4()
        if rec_len != 12:
            raise ValueError(f"suppress rec_len {rec_len}")
        event_id = r.read_u4()
        seconds = r.read_u4()
        always = r.read_u4()
        suppress.append({
            "event": EVENT_NAME_PARSE6.get(event_id, f"Event_{event_id}"),
            "event_id": event_id,
            "duration": float(seconds),
            "always": always,
        })
    out["suppress"] = suppress
    out["ignore_suppress_errors"] = r.read_string()

    def checked_u4_array() -> list[int]:
        count = r.peek_u4()
        if count > 64 or 4 + 4 * count > r.remaining():
            raise ValueError(f"implausible bits-array count {count}")
        return r.read_u4_array()

    out["continuing_bits"] = checked_u4_array()
    out["continuing_fx"] = r.read_string()
    out["conditional_bits"] = checked_u4_array()
    out["conditional_fx"] = r.read_string()

    out["costume_name"] = r.read_string()
    out["power_name"] = r.read_string()
    out["phase_name"] = r.read_string()
    out["entity_def"] = r.read_string()
    priority_lists = [r.read_string() for _ in range(3)]
    if len(set(priority_lists)) != 1:
        # Authored once, serialized thrice — a divergence means misalignment,
        # not data (122k-template census: always identical).
        raise ValueError(f"priority_list triple diverges: {priority_lists}")
    out["priority_list"] = priority_lists[0]

    bools = [r.read_u4() for _ in range(len(_PARSE6_TAIL_BOOL_FLAGS))]
    if any(b not in (0, 1) for b in bools):
        raise ValueError(f"non-boolean tail flag values {bools}")
    out["bools"] = bools
    out["boost_mod_allowed_id"] = r.read_u4()
    out["eval_flags"] = r.read_u4()
    out["procs_per_minute"] = r.read_f4()
    if r.remaining() != 0:
        raise ValueError(f"{r.remaining()} unconsumed tail bytes")
    return out


def _parse6_tail_params(tail: dict) -> dict | None:
    """Map the decoded tail's param-like fields into the HC-shaped `params`
    dict downstream consumers already understand. Field-presence driven —
    no attrib gating, so a param string always survives regardless of which
    special attrib carries it. (The old "Parse6 lowers Grant_Power to Null"
    observation that motivated this was actually the collapsed special-attrib
    misdecode, fixed by resolve_attrib_rebirth — the field-presence design
    stays because it is the shape-faithful choice either way.)"""
    extras = {}
    if tail["costume_name"]:
        extras["costume_name"] = tail["costume_name"]
    if tail["phase_name"]:
        extras["phase_name"] = tail["phase_name"]
    if tail["entity_def"] or tail["priority_list"]:
        params = {"type": "EntCreate"}
        if tail["entity_def"]:
            params["entity_def"] = tail["entity_def"]
        if tail["priority_list"]:
            params["priority_list"] = tail["priority_list"]
        params.update(extras)
        return params
    if tail["power_name"]:
        return {"type": "Power", "power_names": [tail["power_name"]], **extras}
    if tail["costume_name"]:
        params = {"type": "Costume", **extras}
        return params
    if tail["phase_name"]:
        return {"type": "Phase", **extras}
    return None


def _parse_effect_template_parse6(r: BinReader, *, thunderspy: bool = False) -> tuple[EffectTemplate, dict]:
    """Parse a single AttribMod (effect template) — Parse6/Rebirth layout.

    Per the Ghidra-extracted depth=1 AttribMod descriptor at 0x1408e8a10
    (`bin_serializer_report.txt`). Critical differences from Parse7:

    - **No EffectGroup wrapper.** Parse6 stores effects as a flat
      struct_array of AttribMods directly under each Power. The
      Tag/DisplayInfo/Chance/PPM/Delay/RadiusInner/RadiusOuter/Requires/
      Flags/EvalFlags grouping was added in HC's newer schema.
    - **Different field order.** Parse6 uses the depth=1 descriptor row
      order, which puts Name/DisplayAttackerHit/DisplayVictimHit/
      DisplayFloat/DisplayAttribDefenseFloat first, then ShowFloaters,
      Attrib, Aspect, etc. This is more verbose than Parse7's depth=2
      descriptor (used inside EffectGroup).
    - **Name is a single inline string in Parse6**, not a string_array
      (despite the descriptor labeling it 0x500009). Empirically verified
      by hand-decoding multiple records.
    - **DurationExpr/MagnitudeExpr ARE present**, as string_arrays between
      Duration and Magnitude — same as Parse7, NOT an HC-only addition. They
      are empty for ~99% of templates but carry real RPN token lists for
      expression-scaled magnitudes: 900 Rebirth templates carry a
      magnitude_expression (Brute Fury's `Rage_Buff` → `kRage source> .02 *`,
      Possession's HP-differential scaling, Inner Will's mez protection, …) and
      8 carry a duration_expression. A prior version read-and-DISCARDED both
      arrays behind a "always empty in Parse6" comment, which zeroed every
      Rebirth expression magnitude (most visibly leaving Brute Fury underivable
      — DATA-GAP-REGISTER INHERENT-2). The byte consumption was already correct;
      only the value was thrown away.

    The downstream `EffectTemplate` shape is HC-shaped, so we map Parse6
    fields into the closest HC equivalents and fill the rest with
    sensible defaults.

    Returns (template, group_extras): Parse6 keeps radii/PPM/EvalFlags on
    the AttribMod where HC keeps them on the EffectGroup wrapper, so the
    caller applies `group_extras` to the synthetic group it builds.
    """
    # Header strings (5). The leading one is `AttribModTemplate.pchName` — the
    # source calls it "internal name of effect (for matching later on)"
    # (attribmod.h:202), and the matching it means is the chance-mod system:
    # `ChanceModAccumulate` (MapServer/src/entity/character_combat.c:103) walks the
    # character's and the power's accumulated `Global_Chance_Mod`s and adds each
    # one's magnitude where its filter name equals this string. That is the job HC
    # later moved onto the EffectGroup's `Tag`, and stock Parse6 has no group to
    # put it on — so it rides `group_extras` onto the synthetic group this mod is
    # wrapped in, as `tags`, which is where every consumer asks for the bucket.
    #
    # Read into a discarded `_name` for as long as this reader existed, which is
    # why Rebirth exported zero tagged groups while carrying 243 distinct names
    # (`ColdDamage` 291, `ToxicDamage` 406, `FieryEmbrace` 571, `ShapeshiftDeactive`
    # 260, `ShapeshiftActive` 16). Dual Pistols is the anchor: Rebirth's Pistols
    # names its mods Damage / Lethal / FireDamage / FireDamageDoT / ColdDamage /
    # ToxicDamage with chance 1.0 on the first two and 0.0 on the rest — the
    # authored HC def's Effect tags and chances, mod for mod.
    #
    # Most names are not chance-mod buckets at all: ~90% simply restate the mod's
    # own table (`Ones`, `Res_Dmg`, `Buff_Def`). Nothing filters on those, and the
    # converter's tag consumers are name allowlists, so carrying them is faithful
    # rather than load-bearing.
    tag = r.read_string()
    _attacker_msg = r.read_string()
    _victim_msg = r.read_string()
    _float_msg = r.read_string()
    _adef_float_msg = r.read_string()

    # Display flags + core attrib/aspect/target enums
    _show_floaters = r.read_bool()
    attrib_raw = r.read_u4()
    # Parse6 stores Attrib as a single u4 = index*4 (same scaling as HC) for
    # normal attribs, with a byte-granular special block at raw 464-507 —
    # resolve_attrib_rebirth routes both (the special block's raw values are
    # distinct attribs the collapsed //4 lookup used to merge: Set_Mode
    # surfaced as Create_Entity, Grant_Power as Null, Power_Redirect as
    # Add_Behavior — see SPECIAL_ATTRIB_BY_RAW_REBIRTH).
    # Thunderspy uses HC's ATTRIB_NAME map (predates the Parse6/Rebirth shifts
    # at Create_Entity and Accuracy).
    if thunderspy:
        attribs = [ATTRIB_NAME.get(attrib_raw // 4, f"Unknown({attrib_raw // 4})")]
    else:
        attribs = [resolve_attrib_rebirth(attrib_raw)]
    aspect_raw = r.read_u4()
    # Parse6 uses value*4 encoding (4-byte aspect-table entries) vs HC's
    # value*8. Distribution across 21,559 Rebirth records: top values
    # are 0, 8, 16, 12, 4 — all multiples of 4 — confirming the older
    # 4-byte runtime layout.
    aspect = ATTRIB_MOD_ASPECT.get(aspect_raw // 4, f"Unknown({aspect_raw})")
    boost_ignore_diminishing = r.read_bool()
    target_raw = r.read_u4()
    target = ATTRIB_MOD_TARGET.get(target_raw, f"Unknown({target_raw})")

    # Table + scale + application/type
    table = r.read_string()
    scale = r.read_f4()
    app_raw = r.read_u4()
    app_type = ATTRIB_MOD_APPLICATION.get(app_raw, f"Unknown({app_raw})")
    typ_raw = r.read_u4()
    typ = ATTRIB_MOD_TYPE.get(typ_raw, f"Unknown({typ_raw})")

    # Tick fields — note the Parse6 order is Delay/Period/Chance (no
    # TickChance/TickMultiplier/TickAdditive — those are HC additions).
    delay = r.read_f4()
    app_period = r.read_f4()
    chance = r.read_f4()

    # CancelOnMiss + CancelEvents. Event ids use the Parse6 enum, which HC
    # later shifted by inserting events (see EVENT_NAME_PARSE6) — resolving
    # through HC's EVENT_NAME here mislabeled every id ≥ 13.
    cancel_on_miss = r.read_bool()
    cancel_event_ids = r.read_u4_array()
    cancel_events = [EVENT_NAME_PARSE6.get(v, f"Event_{v}") for v in cancel_event_ids]

    # Boolean flag block — 9 bools per descriptor:
    #   NearGround, AllowStrength, AllowResistance,
    #   UseMagnitudeResistance, UseDurationResistance,
    #   AllowCombatMods, UseMagnitudeCombatMods, UseDurationCombatMods,
    #   BoostTemplate
    bool_block = [r.read_bool() for _ in range(9)]

    # AttribMod flags. Parse6 stores them as loose bools (the block above,
    # plus BoostIgnoreDiminishing and CancelOnMiss read earlier) where HC's
    # Parse7 packs a bitmask — but each bool corresponds 1:1 to a Parse7
    # first-word bit (the Allow* bools inverted to their Ignore* bits, the
    # Use*Resistance/Use*CombatMods bools to effective-mode bits 8–11,
    # BoostTemplate to bit 12 Boost). Synthesize flags_raw in Parse7 bit
    # positions so the export is uniform across paths and no bool is
    # discarded, then decode names through the shared table. Bool→bit
    # mapping validated against the HC binary (Hide → IgnoreResistance;
    # Jab → none) 2026-06-12 and the CoD2 flag-index oracle 2026-07-20.
    # More flag bools (HideZero, StackExactPower, VanishEntOnTimeout,
    # DoNotTintCostume, KeepThroughDeath, DelayEval) live in the
    # post-magnitude tail and are merged in after the tail decode below.
    flags_raw = (
        (0x000002 if boost_ignore_diminishing else 0)
        | (0x000004 if cancel_on_miss else 0)
        | (0x000008 if bool_block[0] else 0)        # NearGround
        | (0x000010 if not bool_block[1] else 0)    # ¬AllowStrength → IgnoreStrength
        | (0x000020 if not bool_block[2] else 0)    # ¬AllowResistance → IgnoreResistance
        | (0x000100 if bool_block[3] else 0)        # UseMagnitudeResistance → ResistMagnitude
        | (0x000200 if bool_block[4] else 0)        # UseDurationResistance → ResistDuration
        | (0x000040 if not bool_block[5] else 0)    # ¬AllowCombatMods → IgnoreCombatMods
        | (0x000400 if bool_block[6] else 0)        # UseMagnitudeCombatMods → CombatModMagnitude
        | (0x000800 if bool_block[7] else 0)        # UseDurationCombatMods → CombatModDuration
        | (0x001000 if bool_block[8] else 0)        # BoostTemplate → Boost
    )
    flags: list[str] = _decode_flags(flags_raw)

    # Requires (RPN tokens) + PrimaryStringList/SecondaryStringList. The
    # string lists carry the special-attrib payload the i24 engine reads per
    # attrib kind: for Power_Redirect the primary list is the redirect target
    # power's full name (the fork's whole redirect mechanism — a prior
    # version discarded both lists, which is how Rebirth's redirects went
    # missing, WS7); for Add_Behavior it's AI behavior scripts and for
    # Set_Script_Value script keys (server-side behavior with no player-calc
    # meaning — left unexported).
    requires_tokens = r.read_string_array()
    jit_requires = requires_tokens
    primary_str_list = r.read_string_array()
    # SecondaryStringList: only Set_Script_Value templates carry one (script
    # arguments, e.g. "Cutscene-Skippable") — same no-calc-meaning class as
    # the primary script keys, so it is consumed but not exported.
    r.read_string_array()

    # Stack info
    caster_stack_raw = r.read_u4()
    caster_stack = ATTRIB_MOD_CASTER_STACK.get(caster_stack_raw, f"Unknown({caster_stack_raw})")
    stack_raw = r.read_u4()
    stack = ATTRIB_MOD_STACK.get(stack_raw, f"Unknown({stack_raw})")
    stack_limit = r.read_u4()
    # StackKeys-registry ID (this path always read it as a u4 — it was never
    # subject to the Parse7 string-offset misread, though the old code leaked
    # the no-key sentinel as the literal string "4294967295"). Same
    # convention as Parse7: direct registry index, -1 = no key (normalized
    # to 0 — registry[0] is the unreferenced TestStack filler, so the
    # collision is moot). Resolved to its name at export time.
    _stack_key_raw = r.read_u4()
    stack_key_id = 0 if _stack_key_raw == 0xFFFFFFFF else _stack_key_raw

    # Duration + DurationExpr + Magnitude + MagnitudeExpr.
    #
    # The depth=1 descriptor at `0x1408e8a10` lists *Expr fields between
    # Duration and Magnitude — RPN token string_arrays, empty (count=0,
    # 4 bytes each) for ~99% of templates but NON-EMPTY for
    # expression-scaled magnitudes. They must be READ (not just skipped):
    # the value is real. 900 Rebirth templates carry a magnitude_expression
    # (Brute Fury's `Rage_Buff` → `kRage source> .02 *`, i.e. 2%
    # damage-Strength per point of the kRage meter; DATA-GAP INHERENT-2) and
    # 8 carry a duration_expression.
    #
    # Hand-decoded against Suffocate (Dominator/Water_Control): mag=3
    # (Mag-3 hold) lives at the byte that the previous parser was
    # reading as a different field, off by 8 bytes. Consuming the two
    # string_array reads here aligns it correctly (alignment was always
    # right; a prior version discarded the decoded value).
    #
    # For mez attribs (Held, Immobilized, Sleep, Stunned, …), Parse6
    # uses **Scale as the base duration in seconds** and Magnitude as
    # the mez magnitude. Damage attribs use Scale as the damage table
    # multiplier with Magnitude=0 (typical). Downstream converters
    # already understand both conventions.
    duration = r.read_f4()
    dur_expr = r.read_string_array()  # DurationExpr (RPN tokens; usually empty)
    magnitude = r.read_f4()
    mag_expr = r.read_string_array()  # MagnitudeExpr (RPN tokens; usually empty)

    # Post-magnitude tail — structural decode (see _read_parse6_template_tail
    # for the pinned layout). The inline-pascal scanner remains only as the
    # fallback for a tail that doesn't match, counted and reported loudly.
    global _undecoded_parse6_tails
    tail_start = r._pos
    tail: dict | None
    try:
        tail = _read_parse6_template_tail(r)
    except ValueError:
        r._pos = tail_start
        _undecoded_parse6_tails += 1
        tail = None

    flags2_raw = 0
    suppress_events: list[dict] = []
    fx: dict | None = None
    boost_mod_allowed_id = 0
    group_extras: dict = {"tags": [tag] if tag else []}
    if tail is None:
        tail_bytes = bytes(r._data[r._pos:r._end])
        r.skip_to_end()
        params = _extract_params_parse6(tail_bytes, attribs)
    else:
        params = _parse6_tail_params(tail)
        suppress_events = tail["suppress"]
        # strict=True is free here and worth having: `bools` is read as exactly
        # len(_PARSE6_TAIL_BOOL_FLAGS) u4s (see _parse6_tail_params), so a mismatch means
        # the tail layout moved and silently dropping the last flags would be the worst
        # available outcome.
        for (name, bit, second_word), value in zip(_PARSE6_TAIL_BOOL_FLAGS, tail["bools"], strict=True):
            if not value:
                continue
            flags.append(name)
            if second_word:
                flags2_raw |= bit
            else:
                flags_raw |= bit
        if tail["ignore_suppress_errors"]:
            flags.append("IgnoreSuppressErrors")
            flags_raw |= 0x100000
        fx = {
            "continuing_bits": tail["continuing_bits"],
            "continuing_fx": tail["continuing_fx"] or None,
            "conditional_bits": tail["conditional_bits"],
            "conditional_fx": tail["conditional_fx"] or None,
        }
        if not (fx["continuing_bits"] or fx["continuing_fx"]
                or fx["conditional_bits"] or fx["conditional_fx"]):
            fx = None
        boost_mod_allowed_id = tail["boost_mod_allowed_id"]
        group_extras = {
            "tags": [tag] if tag else [],
            "radius_inner": tail["radius_inner"],
            "radius_outer": tail["radius_outer"],
            "ppm": tail["procs_per_minute"],
            # Normalize to HC bit numbering (HC inserted a bit at position 0;
            # anchors reb→HC 1→2, 2→4, 17→34) so the exported eval_flags field
            # means the same thing across datasets.
            "eval_flags": tail["eval_flags"] << 1,
        }

    # Power_Redirect: the target power lives in PrimaryStringList (i24 redirect
    # mechanism), not in the tail's param strings — surface it in the same
    # `{type: 'Power', power_names}` shape Execute_Power uses so the downstream
    # redirect-follow logic reads both uniformly. Only `params` is special here:
    # this used to enclose the whole tail block above, which fenced Rebirth's
    # flags2/FX/suppress/PPM behind an attrib 235 of 81,913 templates carry.
    if attribs == ["Power_Redirect"] and primary_str_list:
        if params is None:
            params = {"type": "Power", "power_names": list(primary_str_list)}
        else:
            params.setdefault("power_names", list(primary_str_list))

    template = EffectTemplate(
        attribs=attribs,
        type=typ,
        application_type=app_type,
        aspect=aspect,
        target=target,
        table=table,
        scale=scale,
        duration=duration,
        magnitude=magnitude,
        delay=delay,
        duration_expression=dur_expr,
        magnitude_expression=mag_expr,
        application_period=app_period,
        tick_chance=chance,
        jit_requires=jit_requires,
        caster_stack=caster_stack,
        stack=stack,
        stack_limit=stack_limit,
        stack_key_id=stack_key_id,
        cancel_events=cancel_events,
        suppress_events=suppress_events,
        boost_mod_allowed=str(boost_mod_allowed_id) if boost_mod_allowed_id else "",
        boost_mod_allowed_id=boost_mod_allowed_id,
        flags=flags,
        flags_raw=flags_raw,
        flags2_raw=flags2_raw,
        fx=fx,
        params=params,
    )
    return template, group_extras


def _parse_effects_parse6(r: BinReader, *, thunderspy: bool = False,
                          incarnate_scope: str | None = None,
                          power_name: str = "",
                          player_classes=()) -> tuple[list[EffectGroup], list[EffectGroup]]:
    """Parse Parse6 effects: a flat struct_array of AttribMod records.

    Wraps each AttribMod in a synthetic single-template EffectGroup so
    downstream consumers (convert-powerset.cjs) see the same shape as
    HC. ActivationEffect doesn't exist as a separate field in Parse6.
    """
    effects: list[EffectGroup] = []
    eff_count = r.read_u4()
    if eff_count > 1000:
        # Sanity: an unreasonable count means we're misaligned, not a
        # power with thousands of effects. Bail rather than allocate.
        raise ValueError(f"implausible Parse6 effect count {eff_count}")
    for _ in range(eff_count):
        elem_len = r.read_u4()
        if elem_len > r.remaining():
            raise ValueError(f"Parse6 effect elem_len {elem_len} > remaining")
        elem_start = r._pos  # sub_reader is a view; r._pos stays at the element start
        elem_reader = r.sub_reader(elem_len)
        try:
            sibling_templates: list[EffectTemplate] = []
            if thunderspy:
                tmpl, group_extras = _parse_effect_template_thunderspy(
                    elem_reader, r._data, r._strtab_base, incarnate_scope=incarnate_scope
                )
                # A tspy element is HC's EffectGroup: one header over a struct_array
                # of AttribMods (see `_tspy_submods`). Parse every sibling past the
                # first — each needs its own reader, since the template parser reads
                # the shared header before seeking to its sub-record. The siblings
                # re-read that shared header, so their group_extras are the same
                # values this one already carries.
                for _sub in range(1, len(_tspy_submods(r._data, elem_start,
                                                       elem_start + elem_len))):
                    sibling_templates.append(_parse_effect_template_thunderspy(
                        r.sub_reader(elem_len), r._data, r._strtab_base,
                        incarnate_scope=incarnate_scope, sub_index=_sub,
                    )[0])
            else:
                tmpl, group_extras = _parse_effect_template_parse6(elem_reader)
            # Synthetic group — Parse6 stores conditional gates (PvE vs PvP,
            # mode-active variants, drowning checks, etc.) on the
            # per-template `jit_requires` field, since there's no
            # EffectGroup wrapper to carry them. Lift them onto the
            # synthetic group's `requires_expression` so the downstream
            # converter (which only reads the group-level field) honors
            # the same PvE/PvP filtering it does on HC.
            #
            # HC's Parse7 carries an explicit `is_pvp` flag from the
            # binary; Parse6 doesn't, but encodes the same intent in the
            # template's RPN requires expression as `enttype target>
            # player eq` (PvP) or `enttype target> critter eq` (PvE).
            # Synthesize the equivalent flag so downstream filters
            # (`if (effect.is_pvp === 'PVP_ONLY') continue;`) work.
            # Thunderspy's element IS the group and carries its own Requires, so
            # that is the group gate; the template's `jit_requires` is the
            # AttribMod-level DelayedRequires and belongs to the AttribMod alone.
            # Rebirth has no group to carry one, which is why its AttribMod gate
            # is what gets lifted here.
            req = (group_extras.pop('requires_expression') if thunderspy
                   else list(tmpl.jit_requires))
            # A SELF-targeted AttribMod whose requires carries the self-exclusion
            # clause `entref target> entref source> eq !` ("target != self") is a
            # proximity / ally-counting self-buff — it applies +X to the caster
            # for each nearby OTHER entity matching the type filter. Here the
            # trailing `enttype target> player eq` is an *ally-type filter*
            # (count nearby player teammates), NOT the PvE/PvP combat split. The
            # genuine split is foe-targeted (target=AnyAffected) and ships as a
            # *pair*: a `critter eq` copy (PvE) and a `player eq` copy (PvP) of
            # the same effect — Force Bubble's Repel/Knockback are the canonical
            # example. Phalanx Fighting (Brute/Scrapper/Tanker) is the friendly
            # case: its per-ally +Def increment is Self-targeted with the
            # self-exclusion clause and has NO critter sibling, so reading the
            # `player eq` as PvP-only made convert-powerset drop the ally scaling
            # in PvE — contradicting the power's own description and HC's explicit
            # is_pvp=EITHER for the same template. Treat these as EITHER.
            #
            # This carve-out survives the move to `entity_scope` because it is a
            # fact about the TEMPLATE, not about the expression: `target` in the
            # requires names the entity being scanned, not the AttribMod's
            # recipient, so the clause is not a combat-scope test at all and no
            # reading of the expression alone could tell.
            requires_pv = entity_scope(req)
            gate = gate_evaluate(req, player_classes=player_classes)
            # Joined only to ask a question of it. Re-splitting a joined gate is
            # what COND-8 was about; a substring probe is safe, because a token
            # boundary can neither create nor destroy this clause.
            self_exclusion = 'entref target> entref source> eq !' in ' '.join(req).lower()
            if tmpl.target == 'Self' and self_exclusion:
                is_pvp = 'EITHER'
            elif requires_pv in ('PVE_ONLY', 'PVP_ONLY'):
                is_pvp = requires_pv
            else:
                is_pvp = 'EITHER'

            def _pvp_of(t, _element_verdict=is_pvp):
                """The element's verdict, unless this AttribMod's own
                DelayedRequires names the map.

                `isPVPMap?` is the forks' other encoding of the combat split — a
                gate on the MAP rather than on the target's type — and it can sit
                on the AttribMod instead of the element. Thunderspy's Share Pain
                writes its +ToHit pair exactly that way (1.0 under `isPVPMap? !`,
                0.5 under `isPVPMap?`), and the two differ in nothing else, so
                with the field unread they arrived as indistinguishable same-slot
                duplicates and the bag's last-write-wins handed the PvE build the
                PvP value. The element gate is shared by every sibling and the
                DelayedRequires is not, which is why this cannot be decided once
                from sub-record 0 and handed to the rest.

                The 440 tspy powers stating the same token on the ELEMENT gate are
                untouched: their DelayedRequires is empty, so they fall through to
                the element verdict and reach the converter's own `isPVPMap?`
                handling (`dsh6-collapse-detector.cjs`,
                `convert-incarnate-effects.cjs`) exactly as before.

                The map gate is read on its own rather than through
                `entity_scope`, because it is not an entity-type test: it asks
                where the fight is, not who is being hit. Its two spellings are
                the bare token and the token followed by `!`, and a
                DelayedRequires is a flat conjunction, so there is no structure
                for a parse to recover.
                """
                delayed = list(t.jit_requires) if thunderspy else []
                if 'isPVPMap?' not in delayed:
                    return _element_verdict
                negated = delayed[delayed.index('isPVPMap?') + 1:][:1] == ['!']
                return 'PVE_ONLY' if negated else 'PVP_ONLY'
            # Group-level fields Parse6 stores on the AttribMod (HC moved them
            # to the EffectGroup wrapper): radii, PPM, EvalFlags.
            # Thunderspy's element IS the group, so its chance is read from the
            # element header rather than lifted off the AttribMod.
            #
            # Parse6 has no group to read one from, so the AttribMod's own
            # `Chance` serves both roles — the application roll and the per-tick
            # roll are one `fChance` on this schema, and emitting it into both
            # slots is faithful rather than a double-spend. It is NOT clamped:
            # the source oracle rolls `fRand < fChance` with fRand in [0,1), so a
            # zero never fires, and rewriting one to 1.0 turned every inert
            # component into a certain hit (RB5-b1).
            group_chance = (group_extras.pop('chance') if thunderspy
                            else tmpl.tick_chance)
            _note_chances(power_name, group_chance, [tmpl] + sibling_templates)
            effects.append(EffectGroup(
                chance=group_chance,
                requires_expression=req,
                is_pvp=_pvp_of(tmpl),
                requires_pv=requires_pv,
                requires_default=gate.verdict,
                requires_archetypes=list(gate.archetypes),
                templates=[tmpl],
                **group_extras,
            ))
            # A pet's redirect powers are not in its own EntCreate payload —
            # they are the `power_names` of a Power payload on a SIBLING
            # AttribMod of the same element, which is where the byte-scan this
            # replaced used to find them, by proximity rather than by field.
            # `resolvePetLifespan` reads them off the Create_Entity template, so
            # carry them across the element to keep that cascade whole.
            if thunderspy:
                _attach_thunderspy_redirects([tmpl] + sibling_templates)
            # Sibling AttribMods share the element header, so they carry the same
            # gate and PvE/PvP split as the first.
            for sibling in sibling_templates:
                effects.append(EffectGroup(
                    chance=group_chance,
                    requires_expression=req,
                    is_pvp=_pvp_of(sibling),
                    requires_pv=requires_pv,
                    requires_default=gate.verdict,
                    requires_archetypes=list(gate.archetypes),
                    templates=[sibling],
                    **group_extras,
                ))
        except Exception as e:
            _warn_dropped(power_name, f"AttribMod parse failed ({e})")
        r.skip(elem_len)
    return effects, []


def _parse_power_parse6(r: BinReader, *, thunderspy: bool = False,
                        veracity: bool = False,
                        player_classes=()) -> PowerRecord:
    """Parse a single power record (Parse6/Rebirth layout).

    The `thunderspy` flag selects Thunderspy's Parse7-wrapped variant of this
    schema, which differs from stock Parse6 in exactly two ways:
      - Field 43b (u4_array) is absent on Thunderspy (Parse6/Rebirth has it)
      - Box fields 44/45 are HC-style 24 bytes (2×f4×3), not Parse6's 8 bytes

    The `veracity` flag selects the Veracity private-server variant — a third
    point in the design space, reverse-engineered 2026-06-23 (see
    [[veracity-bin-recon]]). Veracity is Parse7-wrapped (string table) with the
    *base* retail field set (no HC extras), and differs from stock Parse6 by:
      - A `Dictionary` string_array inserted after the 6 requires arrays,
        immediately before reward_fallback (carries `Quick_Attack` etc.).
      - Field 43b PRESENT (like Parse6/Rebirth).
      - Box 24 bytes (HC-style, like Thunderspy).
      - Effects use the HC EffectGroup struct (Tag/Chance/PPM/Delay/Radii/
        Requires/Flags/EvalFlags/templates) — NOT the flat Parse6 AttribMod
        array — but with NO DisplayInfo field and an extra HitsToBreak/
        HitBreakMinScale pair per template. No Redirect, no ActivationEffect.
    Validated against Jab/Fire_Blast/Hand_Clap (range/recharge/end exact;
    Ranged_Damage/Melee_Damage scales + fire DoT tickRate + PvE/PvP split +
    combo/faction-conditional requires all resolve).
    """
    # 43b is present for Parse6/Rebirth AND Veracity; absent only on Thunderspy.
    has_43b = (not thunderspy) or veracity
    # Box is HC-style 24 bytes on Thunderspy and Veracity; 8 bytes on Parse6.
    box_24 = thunderspy or veracity

    # 1-9: Same as Parse7 minus HC extras
    full_name = r.read_string()
    r.read_u4()  # crc
    r.read_string()  # source
    name = r.read_string()
    source_name = r.read_string()
    r.read_u4()  # system
    auto_issue = r.read_bool()
    auto_issue_keeps_level = r.read_bool()
    free = r.read_bool()
    display_name = r.read_string()
    display_help = r.read_string()
    short_help = r.read_string()
    for _ in range(9): r.read_string()  # 13-21
    icon = r.read_string()  # 22
    power_type = r.read_u4()  # 23
    num_allowed = r.read_u4()  # 24
    attack_types = r.read_u4_array()  # 25
    requires = r.read_string_array()  # 26
    activate_requires = r.read_string_array()  # 27
    # 28. slot_requires (string_array). Boost (IO piece) records carry
    # `BoostsSlotted>X <= 0` constraints here when the piece is unique
    # within a slot pool — read by the IO-set extractor for per-piece uniqueness.
    slot_requires = r.read_string_array()  # 28
    target_requires = r.read_string_array()  # 29
    r.read_string_array()  # 30
    r.read_string_array()  # 31
    if veracity:
        r.read_string_array()  # 31b Dictionary (Veracity only)
    r.read_string()  # 32 reward_fallback
    accuracy = r.read_f4()  # 33
    cast_through, toggle_ignore, castable_after_death = _parse_cast_flags(r)  # 34
    r.read_u4()  # 35 ai_report
    # (NO 35b in Parse6)
    effect_area = r.read_u4()  # 36
    max_targets_hit = r.read_u4()  # 37
    # (NO 38 chain_effect_array or 38b-d in Parse6)
    radius = r.read_f4()  # 39
    arc = r.read_f4()  # 40
    chain_delay = r.read_f4()  # 41 (same i24 slot as the HC layout)
    chain_eff_expr = r.read_string_array()  # 42 ChainEff (verified:
    # `@ChainJump`/`minmax` content resolves here on Veracity/Parse6 data)
    # 43: string_array — NOT ChainTarget in Parse6. Verified against Veracity:
    # `prevdistance`/`maintarget>` (the ChainTarget tokens) are absent from the
    # whole bin, yet this slot returns VisualFX refs (…NictusFX). So Parse6's
    # field 43 is an FX array; the Parse6 layout simply has no ChainTarget here.
    r.read_string_array()  # 43 (FX array in Parse6, discarded)
    if has_43b:
        r.read_u4_array()  # 43b (present in Parse6/Rebirth + Veracity, absent in Thunderspy)
    if box_24:
        r.skip(24)  # 44-45 box (HC-style 2×f4×3 = 24 bytes; Thunderspy + Veracity)
    else:
        r.skip(8)  # 44-45 box (Parse6: 2×f4 = 8 bytes)
    range_val = r.read_f4()  # 46
    range_secondary = r.read_f4()  # 47
    time_to_activate = r.read_f4()  # 48
    # (NO 48b in Parse6 — Ghidra descriptor shows TimeToRoot here, but
    # adding it shifts every following record into garbage and reduces
    # parsed-record count, so Parse6 is genuinely omitting it. Likely a
    # default-suppression flag on the field. Tracked in MULTI_DATASET_PLAN.)
    recharge_time = r.read_f4()  # 49
    activate_period = r.read_f4()  # 50
    endurance_cost = r.read_f4()  # 51
    r.read_f4()  # 52 idea_cost
    # (NO 52b in Parse6 — same caveat as 48b)
    r.read_u4()  # 53
    r.read_u4()  # 54
    r.read_string_array()  # 55
    # 56-65: usage-limit / lifetime block, same order as Parse7 (LIFETIME-1).
    destroy_on_limit = r.read_bool()  # 56
    stacking_usage = r.read_bool()  # 57
    num_charges = r.read_u4()  # 58
    max_num_charges = r.read_u4()  # 59
    usage_time = r.read_f4()  # 60
    max_usage_time = r.read_f4()  # 61
    lifetime = r.read_f4()  # 62
    max_lifetime = r.read_f4()  # 63
    lifetime_in_game = r.read_f4()  # 64
    max_lifetime_in_game = r.read_f4()  # 65
    interrupt_time = r.read_f4()  # 66
    target_visibility = r.read_u4()  # 67
    target_type = r.read_u4()  # 68
    target_type_secondary = r.read_u4()  # 69
    targets_autohit = r.read_u4_array()  # 70
    targets_affected = r.read_u4_array()  # 71
    r.read_bool()  # 72
    boosts_raw = r.read_u4_array()  # 73
    # Parse6 (Rebirth) uses a different BOOST_TYPE enum than Parse7 (HC) —
    # see _enums.py BOOST_TYPE_REBIRTH for the divergence. Thunderspy predates
    # the Rebirth-only positions 10/36, so it uses HC's stock enum. Veracity
    # likewise indexes HC-style (its attribs/effects all resolve via the HC
    # tables), so it uses the stock BOOST_TYPE too.
    boost_map = BOOST_TYPE if (thunderspy or veracity) else BOOST_TYPE_REBIRTH
    boosts_allowed = [boost_map.get(v, f"Unknown({v})") for v in boosts_raw]
    # 74: u4_array of mode/group refs (see HC parser comment). Not
    # allowed_boostset_cats — that field doesn't exist in the binary.
    exclusion_groups = r.read_u4_array()
    allowed_boostset_cats: list[str] = []

    # Parse6 tail. Re-decoded byte-level against Rebirth + Thunderspy
    # 2026-07-07. Field 74 (GroupMembership/exclusion) was consumed in the
    # loop above; what follows differs per flavor:
    #
    #   Rebirth (Parse6):  ModesRequired, ModesDisallowed,
    #                      FreeBoostSlotsOnPower, AIGroups (string_array,
    #                      inline pascal), Effects. Rebirth has NO Redirect
    #                      struct anywhere in the record — it keeps the i24
    #                      mechanism, Power_Redirect AttribMods inside
    #                      Effects (synthesized onto `redirects` below). The
    #                      third u4_array was mislabeled ModesSuspended until
    #                      2026-07-21 — byte-width-identical either way, but
    #                      the released source's parse table has no
    #                      ModesSuspended token and places FreeBoostSlots
    #                      exactly here; the only non-empty carriers corpus-
    #                      wide are Health/Stamina, whose values decode as
    #                      the Rebirth bonus-slot levels (8/16, 12/22), not
    #                      modes. Real ModesSuspended data (Granite Armor)
    #                      exists on HC only; every Rebirth twin is empty.
    #   Thunderspy:        ModesRequired, ModesDisallowed, AIGroups
    #                      (string_array), Redirect (struct_array, same
    #                      element shape as HC's `_parse_redirects`),
    #                      Effects. There is NO ModesSuspended slot.
    #   Veracity:          treated like Rebirth (retail field set); byte
    #                      consumption is unchanged from the old read since
    #                      its Parse7-style string_array is width-identical
    #                      to a u4_array.
    #
    # The old read (a phantom "RechargeGroup" u4_array before the mode
    # arrays, and AIGroups/Redirect read as u4_arrays) had two bugs: every
    # mode name landed one slot early (Disable_All showed up in
    # modes_required), and any power with a non-empty AIGroups (Rebirth —
    # inline strings change the byte width) or Redirect array (Thunderspy
    # snipes) misaligned the reader and silently lost its entire effects
    # array. This was the root of the "109/1361 Rebirth pet powers have 0
    # effects" cluster.
    #
    # Mode indices resolve via the same per-server registry Set_Mode
    # magnitudes use (attrib_names.bin — `parse_mode_table`). Verified:
    # tspy White_Dwarf_Step requires mode 2 (Peacebringer_Tanker_Mode),
    # Energy_Flight is disallowed in the four form modes + Disable_All,
    # Rebirth Titan Weapons Follow_Through requires 98 (FastMode) —
    # mirroring HC's committed exports for the same powers.
    effects: list[EffectGroup] = []
    activation_effects: list[EffectGroup] = []
    redirects: list[dict] = []
    modes_required: list[int] = []
    modes_disallowed: list[int] = []
    free_boost_slots_on_power: list[int] = []
    effects_aligned = False
    try:
        modes_required = r.read_u4_array()
        modes_disallowed = r.read_u4_array()
        if thunderspy:
            r.read_string_array()  # AIGroups (e.g. kHeavyGroup on NPC powers)
            try:
                redirects = _parse_redirects(r, power_name=full_name)
            except Exception as e:
                # Same loud path as the HC parser: a redirect misparse here
                # misaligns the effects read — surface it, don't hide it.
                redirects = []
                _warn_dropped(full_name,
                              f"redirect parse failed ({e}); effects may be lost")
                r.skip_to_end()
        else:
            free_boost_slots_on_power = r.read_u4_array()
            r.read_string_array()  # AIGroups (e.g. kEarlyBattle on pet powers)
        try:
            if veracity:
                # Veracity uses HC-style EffectGroup effects (no Redirect /
                # ActivationEffect precede them), reached directly here.
                effects, activation_effects = _parse_effects(
                    r, veracity=True, power_name=full_name,
                    player_classes=player_classes)
            else:
                # Scope the Thunderspy generic-token relabels to the Incarnate
                # Hybrid/Destiny slots (Alpha/Judgement/mode-marker templates share
                # the shape but not the semantics — only the slot disambiguates
                # them). The melee tree gets its own scope: its `*_Dmg` rows are
                # damage RESISTANCE, not a damage buff (see _tspy_hybrid_aspect_class).
                incarnate_scope = None
                if thunderspy:
                    if full_name.startswith("Incarnate.Hybrid."):
                        incarnate_scope = ("hybrid_melee"
                                           if full_name.startswith("Incarnate.Hybrid.Melee_")
                                           else "hybrid")
                    elif full_name.startswith("Incarnate.Destiny."):
                        incarnate_scope = "destiny"
                effects, activation_effects = _parse_effects_parse6(
                    r, thunderspy=thunderspy, incarnate_scope=incarnate_scope,
                    power_name=full_name, player_classes=player_classes)
                if thunderspy:
                    # Thunderspy appends an ActivationEffect struct_array
                    # after Effects (same AttribMod element shape) — the
                    # redirect-activation self-buffs (Set_Mode +
                    # Execute_Power on the Titan Weapons *_Slow pets),
                    # matching HC's activation_effects. Empty (count=0) on
                    # all but 15 powers; before this read, the non-empty
                    # carriers misaligned the post-effects tail.
                    activation_effects, _ = _parse_effects_parse6(
                        r, thunderspy=True, power_name=full_name,
                        player_classes=player_classes)
            effects_aligned = True
        except Exception as e:
            effects = []
            activation_effects = []
            _warn_dropped(full_name, f"effects parse failed ({e})")
    except Exception as e:
        _warn_dropped(full_name, f"modes/effects region parse failed ({e})")

    # Rebirth redirects — the fork keeps the i24 mechanism: an AttribMod with
    # the Power_Redirect special attrib, target power in PrimaryStringList,
    # condition in the template Requires. Mirror them onto the power-level
    # `redirects` list HC/Thunderspy populate from their top-level Redirect
    # structs, so the export contract (`redirect`) and the converter's
    # redirect-follow logic see one shape across datasets. The templates also
    # stay in `effects` (faithful to the binary); `show_in_info` has no
    # Parse6 counterpart and is omitted.
    if not thunderspy and not veracity:
        for eg in effects:
            for t in eg.templates:
                if t.attribs != ["Power_Redirect"] or not t.params:
                    continue
                for target_name in t.params.get("power_names", []):
                    redirects.append({
                        'name': target_name,
                        'condition_expression': list(t.jit_requires),
                    })

    # Post-effects tail — only positioned there when the effects parse
    # succeeded; Veracity's retail tail layout is unverified, so it stays
    # skipped (not an exported dataset).
    tail = Parse6PowerTail(MAX_BOOSTS_DEFAULT, 0, [], False, False)
    if effects_aligned and not veracity:
        try:
            tail = _parse_power_tail_parse6(
                r,
                rebirth_fork_words=not thunderspy,
                has_proc_allowed=thunderspy)
        except Exception as e:
            _warn_dropped(full_name,
                          f"post-effects tail parse failed ({e}); "
                          f"ShowIn*/MaxBoosts/StrengthsDisallowed/"
                          f"ProcMainTargetOnly lost")
            tail = Parse6PowerTail(MAX_BOOSTS_DEFAULT, 0, [], False, False)
    r.skip_to_end()

    return PowerRecord(
        full_name=full_name,
        name=name,
        source_name=source_name,
        display_name=display_name,
        display_help=display_help,
        short_help=short_help,
        icon=icon,
        power_type=power_type,
        num_allowed=num_allowed,
        auto_issue=auto_issue,
        auto_issue_keeps_level=auto_issue_keeps_level,
        free=free,
        attack_types=attack_types,
        requires=requires,
        activate_requires=activate_requires,
        target_requires=target_requires,
        effect_area=effect_area,
        max_targets_hit=max_targets_hit,
        range=range_val,
        range_secondary=range_secondary,
        radius=radius,
        arc=arc,
        time_to_activate=time_to_activate,
        recharge_time=recharge_time,
        activate_period=activate_period,
        endurance_cost=endurance_cost,
        interrupt_time=interrupt_time,
        destroy_on_limit=destroy_on_limit,
        stacking_usage=stacking_usage,
        num_charges=num_charges,
        max_num_charges=max_num_charges,
        usage_time=usage_time,
        max_usage_time=max_usage_time,
        lifetime=lifetime,
        max_lifetime=max_lifetime,
        lifetime_in_game=lifetime_in_game,
        max_lifetime_in_game=max_lifetime_in_game,
        accuracy=accuracy,
        target_type=target_type,
        target_type_secondary=target_type_secondary,
        target_visibility=target_visibility,
        targets_autohit=targets_autohit,
        targets_affected=targets_affected,
        classic_target_enum=True,
        boosts_allowed=boosts_allowed,
        allowed_boostset_cats=allowed_boostset_cats,
        cast_through=cast_through,
        toggle_ignore=toggle_ignore,
        slot_requires=slot_requires,
        # Parse6 has no MaxTargetsExpr (field 38, HC-only) and no ChainTarget
        # (field 43 is an FX array here — see the read above). Only ChainEff.
        chain_eff_expression=chain_eff_expr,
        # i24-original fields shared with the HC layout. Of the HC-added
        # fields, the OverCap block and MaxToggleTime don't exist in Parse6;
        # ProcAllowed does not either, but Thunderspy carries it (TSPY-5) —
        # the tail reader gates it on the flavor and leaves it 0 elsewhere.
        castable_after_death=castable_after_death,
        chain_delay=chain_delay,
        max_boosts=tail.max_boosts,
        proc_allowed_raw=tail.proc_allowed_raw,
        strengths_disallowed=tail.strengths_disallowed,
        proc_main_target_only=tail.proc_main_target_only,
        anim_main_target_only=tail.anim_main_target_only,
        show_in_inventory_raw=tail.show_in_inventory_raw,
        show_in_manage=tail.show_in_manage,
        show_in_info=tail.show_in_info,
        effects=effects,
        activation_effects=activation_effects,
        redirects=redirects,
        modes_required=modes_required,
        modes_disallowed=modes_disallowed,
        free_boost_slots_on_power=free_boost_slots_on_power,
        exclusion_groups=exclusion_groups,
    )
