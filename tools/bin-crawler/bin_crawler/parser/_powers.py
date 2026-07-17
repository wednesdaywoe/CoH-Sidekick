"""Parser for powers.bin — the most complex binary file.

Each power record has 122+ fields that MUST be read in strict sequential order.
Supports both Parse7 (HC/Homecoming) and Parse6 (retail/Rebirth) formats.
HC added 6 extra fields not present in Parse6:
  - 35b: u4 after ai_report
  - 38:  chain_effect_array (string_array) + 38b-d: 3×u4
  - 48b: f4 after time_to_activate = TimeToRoot (captured as time_to_root)
  - 52b: u4 after idea_cost
Parse6 retains fields 53-56 (confirm dialog fields) in all records.
Parse6 has 8 bytes of box data (2×f4) instead of Parse7's 24 bytes (2×f4×3).

Post-2026 HC experimental patch added 8 bytes after chain_delay (field 41b):
  - 41b: 2×u4 (likely f4 + u4, often (1.0f, 0))
Auto-detected via _detect_field_41b.
"""

import struct
from pathlib import Path
from ._reader import open_parse7, BinReader, Parse6BinReader
from ._dataclasses import PowerRecord, EffectGroup, EffectTemplate
from ._enums import (
    BOOST_TYPE, BOOST_TYPE_REBIRTH, TARGET_TYPE, ATTRIB_NAME, ATTRIB_NAME_REBIRTH,
    ATTRIB_NAME_THUNDERSPY, EVENT_NAME, resolve_attrib,
    ATTRIB_MOD_TYPE, ATTRIB_MOD_ASPECT, ATTRIB_MOD_APPLICATION,
    ATTRIB_MOD_TARGET, ATTRIB_MOD_STACK, ATTRIB_MOD_CASTER_STACK,
    PVP_FLAG,
)


# AttribMod flag bit assignments. Empirically derived by cross-referencing
# .powers source `Flags` keywords against the u4 stored after BoostModAllowed
# (~13k AttribMod templates across all HC categories). Bits 0x100 / 0x200 /
# 0x400 / 0x800 are NOT flags — they appear as 0x500 or 0xa00 doublets in
# templates with no Flags line, almost certainly some other packed field
# (likely Resist{Magnitude,Duration} mode). Bit 0x800 *is* DeepSleep though,
# so we accept the overlap and decode it. Bits with too few samples to be
# confident (Boost, ResistDuration, ResistMagnitude, AdjustTimer, etc.) are
# omitted — they can be added later as more data appears.
_FLAG_BITS: tuple[tuple[int, str], ...] = (
    (0x000001, "NoFloaters"),
    (0x000004, "CancelOnMiss"),
    (0x000008, "NearGround"),
    (0x000010, "IgnoreStrength"),
    (0x000020, "IgnoreResistance"),
    (0x000040, "IgnoreCombatMods"),
    (0x000080, "IgnoreEfficiency"),
    (0x000800, "DeepSleep"),
    (0x002000, "HideZero"),
    (0x004000, "KeepThroughDeath"),
    (0x010000, "NoHitDelay"),
    (0x020000, "NoProjectileDelay"),
    (0x040000, "StackByAttribAndKey"),
    (0x100000, "IgnoreSuppressErrors"),
)


def _decode_flags(flags_raw: int) -> list[str]:
    return [name for bit, name in _FLAG_BITS if flags_raw & bit]


# SECOND AttribMod flags word — the u4 stored immediately after the first
# flags word (`flags_raw`). The binary splits AttribMod flags across two
# consecutive u4s; copy/pet-related keywords live in this second word, which
# the parser previously swallowed into the opaque Params tail (so CopyBoosts,
# PseudoPet, etc. were never emitted — the converter's dormant
# `template.flags.includes('CopyBoosts'/'PseudoPet')` checks never fired).
#
# Bits derived by matching ~14.5k binary templates to their `.powers` source
# `Flags` keyword sets via (table, scale, magnitude) signature, then per-bit
# precision/recall (probe in c:\tmp\copyboosts-probe3.py). All three below have
# perfect recall (fn=0):
#   0x4  CopyBoosts       tp=408 fn=0  (also 415/415 vs an entity_def-keyed cross-check)
#   0x8  CopyCreatorMods  tp=33  fn=0
#   0x20 PseudoPet        tp=68  fp=0 fn=0
# Only CopyBoosts is surfaced for now (it's all the converter consumes here, via
# summon.copyBoosts → pet enhancement application). PseudoPet (0x20) is validated
# but deferred: emitting it would flip summon.isPseudoPet on ~68 powers, changing
# their Power-Info display ("🐾 Summons" → "⚡ Creates") — a behavior change that
# belongs with the pseudo-pet resolution work, not this CopyBoosts fix.
# CopyCreatorMods (0x8) is currently unconsumed downstream. Add either by
# uncommenting once verified. Lower bits (0x1/0x2) did not map cleanly to any
# tested keyword and are left undecoded.
_FLAG2_BITS: tuple[tuple[int, str], ...] = (
    (0x000004, "CopyBoosts"),
    # (0x000008, "CopyCreatorMods"),  # validated (fn=0); unconsumed downstream
    # (0x000020, "PseudoPet"),        # validated (fp=0/fn=0); flips display — defer
)


def _decode_flags2(flags2_raw: int) -> list[str]:
    return [name for bit, name in _FLAG2_BITS if flags2_raw & bit]


# Rate-limited "we silently dropped a power's effects" warning. A swallowed
# parse failure used to be invisible — it hid the ~265-power Trip Mine
# misalignment for months. Surface the first N by name + a final total so a
# future layout shift is loud, without spamming a full export run.
_dropped_powers: list[str] = []
_DROP_WARN_CAP = 25


def _warn_dropped(full_name: str, reason: str) -> None:
    import sys
    _dropped_powers.append(full_name)
    if len(_dropped_powers) <= _DROP_WARN_CAP:
        print(f"  Warning: dropped effects for {full_name}: {reason}", file=sys.stderr)
    elif len(_dropped_powers) == _DROP_WARN_CAP + 1:
        print(f"  Warning: … further dropped-effect warnings suppressed "
              f"(see total at end)", file=sys.stderr)


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
        r._pos = save_pos
        score = 0
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


def parse_powers(bin_path_or_data) -> list[PowerRecord]:
    r = open_parse7(bin_path_or_data)
    is_parse6 = isinstance(r, Parse6BinReader)

    block_size = r.read_u4()
    count = r.read_u4()

    if is_parse6:
        parser = _parse_power_parse6
    else:
        # Auto-detect HC format version by testing the first few records.
        # Two known post-release additions that shift subsequent field offsets:
        #   - field 45b (u4 between box_size and range) added in a past HC patch
        #   - field 41b (8 bytes after chain_delay) added in 2026 experimental patch
        # Wrong layout produces implausible values (negative range, huge recharge).
        has_41b, has_45b, is_thunderspy, is_veracity = _detect_format(r)
        if is_thunderspy:
            # Thunderspy: Parse7 wrapper, Parse6-derived schema with HC-style
            # box (24 bytes) and no field 43b. Reuses _parse_power_parse6.
            parser = lambda sub: _parse_power_parse6(sub, thunderspy=True)
        elif is_veracity:
            # Veracity: Parse7 wrapper, base retail field set + Dictionary,
            # 43b present, box 24, HC-style EffectGroup effects.
            parser = lambda sub: _parse_power_parse6(sub, veracity=True)
        else:
            parser = lambda sub: _parse_power(sub, has_field_45b=has_45b, has_field_41b=has_41b)

    _dropped_powers.clear()
    records = []
    for i in range(count):
        rec_len = r.read_u4()
        sub = r.sub_reader(rec_len)

        try:
            pw = parser(sub)
            records.append(pw)
        except Exception as e:
            # Log but continue — don't let one bad record stop everything
            if i < 5:
                import sys
                print(f"  Warning: record {i} parse error: {e}", file=sys.stderr)

        r.skip(rec_len)

    if _dropped_powers:
        import sys
        print(f"  Warning: dropped effects on {len(_dropped_powers)} power(s) due to "
              f"parse failures (see GAME-DATA-PRINCIPLES §5)", file=sys.stderr)

    return records


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
    """Scan the AttribMod template tail for Params data the planner cares about.

    The full tail layout (8 fields: CancelEvents, RequiredEvent, Suppress,
    BoostModAllowed, Flags, Messages, FX, Params) hasn't been fully reverse-
    engineered. As a stopgap, we scan u4-aligned slots for offsets that
    resolve to strings matching the patterns we expect for known attribs:

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
    """Parse6-equivalent of `_extract_params`.

    Parse6 stores strings inline as `u16(len) + chars + 4-byte-align pad`
    (vs Parse7's u4 offset into a separate string table). The AttribMod
    tail in Parse6 isn't fully reverse-engineered, but the Power slot
    (granted-power dotted name) is the only field we currently care
    about — it surfaces Stalker Assassin's Focus, Radiation Melee
    Contaminated, Beast Mastery Pack Mentality, etc. on chance-procs
    that would otherwise fall back to a generic "state" label.

    Strategy: scan the tail for inline pascal strings that pass the
    same `_looks_like_power_name` shape filter Parse7 uses. Returns
    `{'type': 'Power', 'power_names': [...]}` when at least one match
    found; None otherwise.

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

    # Unknown field between target and table (possibly near_ground or another flag)
    r.read_u4()

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
    # 1) + 1)"). Joined with spaces to match the group-level requires_expression
    # representation. Previously discarded, which left every Expression-type
    # effect with a blank magnitude_expression in the export.
    dur_expr = " ".join(r.read_string_array())
    mag_expr = " ".join(r.read_string_array())
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

    # DelayedRequires — Ghidra descriptor (0x1408ed5a0 row 17) types this as
    # string_array (0x500009), not a single string. Empty for most templates
    # (count=0 → 4 bytes, equivalent to old single-string read). For Create_Entity
    # / Summon / AoE-control templates with delayed conditionals (e.g.
    #   `DelayedRequires arch target> Class_Henchman_Lt eq … 18 tokens`)
    # the array contains the compiled token offsets. The old single-string
    # read shifted every downstream field by 4*N bytes, breaking stack reads
    # on ~29 pet-summon templates (Summon_Wolves, Paralyzing_Blast, etc.).
    r.read_u4_array()  # delayed_requires_tokens
    jit_requires = ''  # legacy field name kept to avoid churn; tokens not resolved here

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
    #   CancelEvents, RequiredEvent, Suppress, BoostModAllowed, Flags,
    #   Messages, FX, Params
    # Sizes verified empirically against Hide.def + Pool/Invisibility/Stealth.def
    # in 2026-04-22 audit. Tail-byte param scan is kept as a fallback for the
    # later fields (Flags / FX / Params) until those are fully decoded.

    # CancelEvents: u4_array of event enum IDs (see EVENT_NAME)
    cancel_event_ids = r.read_u4_array()
    cancel_events = [EVENT_NAME.get(v, f"Event_{v}") for v in cancel_event_ids]
    # RequiredEvent: single u4 (0 = none)
    _required_event_id = r.read_u4()
    required_events = (
        [EVENT_NAME.get(_required_event_id, f"Event_{_required_event_id}")]
        if _required_event_id != 0 else []
    )
    # Suppress: struct_array, each record 12 bytes (event_id u4, duration f4, flag u4).
    # NB: don't reuse the local `duration` name here — that's the AttribMod's
    # buff duration read earlier and used in the EffectTemplate constructor.
    _supp_count = r.read_u4()
    suppress_events: list[dict] = []
    try:
        for _ in range(_supp_count):
            rec_len = r.read_u4()
            sub_supp = r.sub_reader(rec_len)
            ev_id = sub_supp.read_u4()
            supp_duration = sub_supp.read_f4() if rec_len >= 8 else 0.0
            flag = sub_supp.read_u4() if rec_len >= 12 else 0
            suppress_events.append({
                "event": EVENT_NAME.get(ev_id, f"Event_{ev_id}"),
                "event_id": ev_id,
                "duration": supp_duration,
                "flag": flag,
            })
            r._pos += rec_len
    except Exception:
        # Defensive — if struct_array misalignment surfaces, stop parsing
        # the suppress array and let downstream still get whatever we have.
        pass

    # Tail layout after Suppress (verified against Hide.def + Granite_Armor +
    # Summon_Demonlings):
    #   BoostModAllowed (u4) — small enum index, 0 for ~99% of templates
    #   Flags (u4) — bitmask. Bit meanings not yet decoded; observed values
    #     suggest 32-bit flag word with combinations like 0x420 (most Hide
    #     AttribMods, ".def Flags IgnoreResistance"), 0x430 (Hide.def
    #     "Flags IgnoreStrength IgnoreResistance"). Stored raw for now.
    #   Then 2-3 zero-filled u4s of unknown semantic (Messages? Mode? counts?)
    #   Then FX struct_array (count + records) — count > 0 for templates
    #     with .def FX block; record sizes vary (16 bytes for Hide simple
    #     ContinuingFX, 40+ bytes for Create_Entity templates).
    #   Then Params (count + variable data, decoded heuristically below).
    # We capture BMA + raw Flags reliably and leave the rest to the
    # tail-byte heuristic scan that already works for Params (Power refs,
    # EntCreate entity defs).
    boost_mod_allowed_id = r.read_u4() if r.remaining() >= 4 else 0
    flags_raw = r.read_u4() if r.remaining() >= 4 else 0
    # Second flags word (CopyBoosts / PseudoPet / CopyCreatorMods live here).
    # Read it explicitly so it's decoded rather than left in the opaque Params
    # tail. The trailing tail (FX / Params) still starts after it; the param
    # scanner is a heuristic string-offset sweep that never depended on this
    # word (a small bitmask, never a valid string offset).
    flags2_raw = r.read_u4() if r.remaining() >= 4 else 0

    # Legacy fields preserved for downstream consumers — BMA exposed as a name
    # via the simple int → str conversion (won't conflict with any consumer
    # since it's never been populated before).
    boost_mod_allowed = str(boost_mod_allowed_id) if boost_mod_allowed_id else ""
    flags: list[str] = _decode_flags(flags_raw) + _decode_flags2(flags2_raw)
    mode_name = None

    tail_bytes = bytes(r._data[r._pos:r._end])
    r.skip_to_end()
    params = _extract_params(tail_bytes, attribs, r._strtab_base, r._strtab_data)

    return EffectTemplate(
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
        params=params,
    )


def _parse_effect_group(r: BinReader, *, veracity: bool = False) -> EffectGroup:
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
    req_parts = r.read_string_array()
    requires = " ".join(req_parts) if req_parts else ""

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
    if veracity and is_pvp == "EITHER":
        if "target> player eq" in requires:
            is_pvp = "PVP_ONLY"
        elif "target> critter eq" in requires:
            is_pvp = "PVE_ONLY"

    # Templates struct_array
    templates = []
    tmpl_count = r.read_u4()
    for _ in range(tmpl_count):
        tmpl_len = r.read_u4()
        tmpl_reader = r.sub_reader(tmpl_len)
        try:
            tmpl = _parse_effect_template(tmpl_reader, veracity=veracity)
            templates.append(tmpl)
        except Exception:
            pass  # Skip unparseable templates
        r.skip(tmpl_len)

    # Nested effect groups (recursive). The .def grammar allows `Effect { ... }`
    # inside another `Effect { ... }` (e.g. Chance/Requires-gated sub-effects);
    # the binary mirrors this with a struct_array of child groups right after
    # the templates array. Without this read, ~1700 powers (35%) lost the
    # AttribMods buried inside nested Effects (BS_Bash, Crowd_Control, …).
    child_groups = []
    try:
        child_count = r.read_u4()
        for _ in range(child_count):
            child_len = r.read_u4()
            child_reader = r.sub_reader(child_len)
            try:
                child = _parse_effect_group(child_reader, veracity=veracity)
                child_groups.append(child)
            except Exception:
                pass
            r.skip(child_len)
    except Exception:
        pass  # No nested-groups field — older or empty effect group

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
        eval_flags=eval_flags,
        tags=tags,
        templates=templates,
        child_groups=child_groups,
    )


def _parse_redirects(r: BinReader) -> list[dict]:
    """Parse the Redirect struct_array from a power record.

    Each element: power_name (string) + requires token list (string_array) +
    show_in_info (u4). The condition_expression is:
      - 'Always' when requires is empty or a tautology (`['1']`). This is the
        CoD2 convention the downstream converter matches via `=== 'Always'`.
      - Otherwise the space-joined RPN tokens (e.g.
        'Redirects.X source.ownPower? !') — the downstream converter only
        uses condition_expression for two sentinel checks ('Always' to prefer
        the default, and 'kHitPoints' substring to skip dead-state
        conditionals), so full infix normalization isn't required.

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
            power_name = er.read_string()
            req_tokens = er.read_string_array()
            show_raw = er.read_u4()
            if not req_tokens or req_tokens == ['1']:
                cond = 'Always'
            else:
                cond = ' '.join(req_tokens)
            out.append({
                'name': power_name,
                'condition_expression': cond,
                'show_in_info': bool(show_raw & 0xff),
            })
        except Exception:
            pass
        r.skip(elem_len)
    return out


def _parse_effects(r: BinReader, *, veracity: bool = False) -> tuple[list[EffectGroup], list[EffectGroup]]:
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
            eg = _parse_effect_group(eff_reader, veracity=veracity)
            effects.append(eg)
        except Exception:
            pass  # Skip unparseable effect groups
        r.skip(eff_len)

    # Veracity's power-record tail (parse6-derived) has no ActivationEffect
    # struct_array — effects are the final structure. Reading a second array
    # would consume garbage; stop here.
    if veracity:
        return effects, []

    # ActivationEffects struct_array — same layout as regular effects.
    # Wrapped in try since not every power has this field present (older or
    # alternate layouts), and we don't want to abort parsing the rest of the
    # record over a missing optional structure.
    activation_effects = []
    try:
        act_count = r.read_u4()
        for _ in range(act_count):
            eff_len = r.read_u4()
            eff_reader = r.sub_reader(eff_len)
            try:
                eg = _parse_effect_group(eff_reader)
                activation_effects.append(eg)
            except Exception:
                pass
            r.skip(eff_len)
    except Exception:
        pass

    return effects, activation_effects


def _parse_cast_flags(r: BinReader) -> tuple[list[str], list[str]]:
    """Parse cast_flags structure (52 bytes); return (cast_through, toggle_ignore).

    cast_through  = mez states the power can be ACTIVATED through (Blaster Defiance).
    toggle_ignore = mez states that DON'T detoggle the power (the toggle keeps
                    running while you're Held/Slept/Stunned).
    """
    # Layout: near_ground(4) + target_near_ground(4) + 1+3 bytes bitfield
    #   + cast_through_hold(4) + cast_through_sleep(4) + cast_through_stun(4)
    #   + cast_through_terrorize(4) + toggle_ignore_hold(4) + toggle_ignore_sleep(4)
    #   + toggle_ignore_stun(4) + ignore_level_bought(4) + shoot_through_untouchable(4)
    #   + interrupt_like_sleep(4)
    # Total: 4+4 + 1+3 + 10×4 = 52 bytes
    r.skip(8)   # near_ground + target_near_ground
    r.skip(4)   # bitfield (castable_dead_or_alive etc.)
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
    return cast_through, toggle_ignore


def _parse_power(r: BinReader, *, has_field_45b: bool = True, has_field_41b: bool = False) -> PowerRecord:
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
    r.read_bool()
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
    requires_parts = r.read_string_array()
    requires = " ".join(requires_parts) if requires_parts else ""
    # 27. activate_requires (string_array)
    activate_requires_parts = r.read_string_array()
    activate_requires = " ".join(activate_requires_parts) if activate_requires_parts else ""
    # 28. slot_requires (string_array). Boost (IO piece) records carry
    # `BoostsSlotted>X <= 0` constraints here when the piece is unique
    # within a slot pool — used by the IO-set extractor to detect uniqueness.
    slot_requires_parts = r.read_string_array()
    slot_requires = " ".join(slot_requires_parts) if slot_requires_parts else ""
    # 29. target_requires (string_array)
    target_requires_parts = r.read_string_array()
    target_requires = " ".join(target_requires_parts) if target_requires_parts else ""
    # 30. reward_requires (string_array)
    r.read_string_array()
    # 31. auction_requires (string_array)
    r.read_string_array()
    # 32. reward_fallback (string)
    r.read_string()
    # 33. accuracy (f4)
    accuracy = r.read_f4()
    # 34. cast_flags (52 bytes)
    cast_through, toggle_ignore = _parse_cast_flags(r)
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
    max_targets_expr = " ".join(r.read_string_array())
    # 38b-d. HC extra fields (3 × u4)
    r.skip(12)
    # 39. radius (f4)
    radius = r.read_f4()
    # 40. arc (f4)
    arc = r.read_f4()
    # 41. chain_delay (f4)
    r.read_f4()
    # 41b. HC experimental 2026: 8 bytes (likely f4 + u4, often (1.0f, 0))
    if has_field_41b:
        r.skip(8)
    # 42. ChainEff (string_array) — per-jump chain-continue chance expression.
    # VERIFIED on Veracity/Parse6: `@ChainJump`/`minmax` content resolves here.
    chain_eff_expr = " ".join(r.read_string_array())
    # 43. HC string_array, previously mislabeled "chain_fork" (the real ChainFork
    # is a u4 int-array, not a string_array). On Parse6 the same slot resolves to
    # VisualFX/power refs (…NictusFX), NOT the ChainTarget selection expression
    # (`prevdistance`/`maintarget>`, which is absent from Veracity entirely). So
    # this is most likely an FX/ChainIntoPower array, and ChainTarget lives
    # elsewhere in Parse7. UNVERIFIED for HC — captured neutrally for the probe.
    field43_str = " ".join(r.read_string_array())
    # 43b. ChainTarget — next-target selection weighting for the Electrical Affinity
    # circuits (`… kHitPoints% target> - … maintarget> … prevdistance / +`). Stored
    # as a u4_array of string-table offsets — byte-identical to a string_array, so we
    # read it as one to resolve the RPN tokens (no desync). VERIFIED 2026-07-01
    # against the HC `.powers` oracle (circuits match exactly; 55 powers). Empty
    # (count 0) on non-chain powers.
    chain_target_expr = " ".join(r.read_string_array())
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
    # 52b. HC extra field (u4)
    r.read_u4()
    # 53. time_to_confirm (u4)
    r.read_u4()
    # 54. self_confirm (u4)
    r.read_u4()
    # 55. confirm_requires (string_array)
    r.read_string_array()
    # 56. destroy_on_limit (bool)
    r.read_bool()
    # 57. stacking_usage (bool)
    r.read_bool()
    # 58. num_charges (u4)
    r.read_u4()
    # 59. max_num_charges (u4)
    r.read_u4()
    # 60. usage_time (f4)
    r.read_f4()
    # 61. max_usage_time (f4)
    r.read_f4()
    # 62. lifetime (f4)
    r.read_f4()
    # 63. max_lifetime (f4)
    r.read_f4()
    # 64. lifetime_in_game (f4)
    r.read_f4()
    # 65. max_lifetime_in_game (f4)
    r.read_f4()
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
        redirects = _parse_redirects(r)
    except Exception as e:
        # Fail LOUD: a redirect misparse here misaligns the effects read and
        # silently empties the power's effects (the ~265-power Trip Mine class).
        # Surface it so the next layout shift announces itself instead of hiding.
        redirects = []
        _warn_dropped(full_name, f"redirect parse failed ({e}); effects may be lost")
        r.skip_to_end()

    # Parse effects
    try:
        effects, activation_effects = _parse_effects(r)
    except Exception as e:
        effects = []
        activation_effects = []
        _warn_dropped(full_name, f"effects parse failed ({e})")

    # Skip remaining metadata after effects
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

# Front-attrib `Ones` is Thunderspy's catch-all magnitude token on the `*_Ones`
# unit tables; the REAL modified stat lives only in the post-`requires` index
# array. These are the attribs we recover from that index array when the front
# is exactly `['Ones']` — the high-confidence resource/recharge buffs+debuffs
# that route unambiguously by SIGN alone (positive→buff, negative→debuff) with no
# magnitude/aspect semantics the Thunderspy schema can't supply. Deliberately
# EXCLUDED (left as unclassifiable `['Ones']`, i.e. no behaviour change): damage
# types (on `Ones` fronts these are AttackType/combo TAG templates on non-damage
# powers like Aim/Assault — relabeling would inject phantom damage), mez
# (needs a magnitude the `Ones` template doesn't carry — `magnitude` is a flat
# 1.0), knockback (sign-vs-protection nuance), and exotic mechanic attribs
# (Set_Mode/Rage/Taunt/Create_Entity/…). See THUNDERSPY_TODO.md item 1.
_TSPY_ONES_RECOVERABLE = frozenset(
    {"RechargeTime", "Recovery", "Regeneration", "Endurance"}
)

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


# Create_Entity (pet/pseudo-pet summon) marker in Thunderspy's effect elements.
# Unlike HC/Rebirth — which carry Create_Entity as the template's *front* attrib
# (enum 469 / 116) and one AttribMod per pet — Thunderspy packs the summon list
# into a NESTED struct-array inside a single effect element. Each nested EntCreate
# sub-entry LEADS with the byte-granular raw value 465 ( = index 116 << 2 | 1:
# Rebirth's -1 Create_Entity index shift PLUS HC's byte-granular +1 special-region
# sub-index) and carries its own `Ranged_Ones`/`Ranged_Level` table + the EntityDef
# / PriorityList / redirect string offsets. The element's *front* attrib is a bare
# `Ones`/`Level` summon shell. Verified against the binary: the count of 465 markers
# equals the pet count exactly (Summon_Wolves 3, Rally_The_Militia 6, Hell_on_Earth
# 10, Haunt 2 shades), and 1863 elements across 1512 player-relevant powers carry
# it. The affected-attrib index array (`_idx_attribs`) never names Create_Entity for
# these — the marker lives in the nested params block, not the index array — which is
# why the recharge/mez index-array recovery above leaves summons untouched.
_TSPY_CREATE_ENTITY_MARKER = 465

# The `Ones`/`Level`(minus) front attribs on a summon element are pure summon
# SHELLS (the level-setting placeholder), carrying no player-facing effect of their
# own — HC emits only the Create_Entity template(s), no shell. So for these fronts we
# REPLACE the shell with the extracted Create_Entity templates. A summon element with
# any OTHER front (a real Damage/Knockback/… effect that also summons — e.g. an attack
# with an incidental pet) keeps its header template AND gains the Create_Entity ones.
_TSPY_SUMMON_SHELL_FRONTS = frozenset(
    {"Ones", "Level", "Levelminus", "Levelminus2"}
)

# Front-attrib / table / control tokens that resolve as entity-def-shaped strings
# (leading uppercase, underscores, no dots) but are NOT summoned EntityDefs. The
# table prefixes are filtered separately via `_TSPY_TABLE_PREFIXES`.
_TSPY_NON_ENTITYDEF = frozenset(
    {"Ones", "Damage", "Level", "Levelminus", "Levelminus2", "Heal", "Endurance",
     "PFX", "Combat", "Pet"}
)


def _extract_thunderspy_summons(strtab_data, start: int, end: int,
                                strtab_base: int, front: str, table: str) -> list[dict]:
    """Extract EntCreate params from a Thunderspy summon element.

    Splits the element `[start, end)` into one region per `465` Create_Entity
    marker and pulls each nested sub-entry's EntityDef (+ PriorityList + redirect
    power names) — mirroring HC's `_extract_params` string-scan, but scoped per
    region so a multi-pet summon yields one `{type:'EntCreate', entity_def, ...}`
    per pet (matching HC's one-template-per-pet convention that the converter
    counts). Returns [] when the element carries no marker.

    The EntityDef is the region's first entity-def-shaped string that isn't the
    front attrib, the modifier table, a `PL_` priority list, a `P<digits>` combat-
    text message key, or a known non-entity token; PriorityList is the first `PL_`
    / `Pet` identifier after it; redirects are 3-part dotted power names that
    aren't FX asset paths (`…FX` / `…Fail_Safe` / `PFX`). Verified clean over all
    809 player-category summon sub-entries (0 missing EntityDef); the residual
    mismatches are NPC/critter powers dropped by the export's PLAYER_CATEGORIES.
    """
    markers = [off for off in range(start, end - 3, 4)
               if struct.unpack_from('<I', strtab_data, off)[0] == _TSPY_CREATE_ENTITY_MARKER]
    if not markers:
        return []
    bounds = markers + [end]
    out: list[dict] = []
    for i, mpos in enumerate(markers):
        r_start, r_end = mpos, bounds[i + 1]
        entity_def = None
        priority_list = None
        redirects: list[str] = []
        for off in range(r_start, r_end - 3, 4):
            u = struct.unpack_from('<I', strtab_data, off)[0]
            s = _resolve_offset(strtab_data, strtab_base, u)
            if not s:
                continue
            if '.' in s:
                # power-name redirect (3-part dotted) vs FX asset path — skip FX
                if (s.count('.') == 2 and not s.endswith(('FX', 'Fail_Safe'))
                        and 'PFX' not in s and s not in redirects):
                    redirects.append(s)
                continue
            if s.startswith('PL_') or s == 'Pet':
                if priority_list is None and entity_def is not None:
                    priority_list = s
                continue
            if _is_message_key(s):
                continue
            if (entity_def is None and _looks_like_entity_def(s)
                    and s not in _TSPY_NON_ENTITYDEF
                    and not s.startswith(_TSPY_TABLE_PREFIXES)
                    and s != front and s != table):
                entity_def = s
        if not entity_def:
            continue  # NPC edge cases (digit-leading / classref) — dropped downstream
        params: dict = {'type': 'EntCreate', 'entity_def': entity_def}
        if priority_list:
            params['priority_list'] = priority_list
        if redirects:
            params['redirects'] = redirects
        out.append(params)
    return out


# Defense-class attrib indices: positional Ranged(26)/Melee(27)/Area(28) + the eight
# damage-type defenses Smashing(29)…Toxic(36). Used to relocate a Buff_Def template's
# affected-attrib array when a requires/redirect block displaces it (see below).
_TSPY_DEFENSE_IDX = frozenset(range(26, 37))


def _tspy_scan_defense_arrays(strtab_data, start: int, end: int) -> list[list[str]]:
    """Every count-prefixed array in [start, end) whose entries are ALL defense-class
    attrib indices. Thunderspy stores a defense template's affected attribs as a
    `[count][count × attribIndex*4]` array. Simple toggles keep it at the fixed
    post-`requires` offset the header read below targets; but powers with a
    requires/redirect block carry only a 1-type SHIM there and push the real
    multi-type array ~140 bytes downstream. Returns each qualifying array's decoded
    attrib-name list, in file order."""
    out: list[list[str]] = []
    i = start
    while i < end - 4:
        k = struct.unpack_from('<I', strtab_data, i)[0]
        if 1 <= k <= 24 and i + 4 + k * 4 <= end:
            idxs = struct.unpack_from('<%dI' % k, strtab_data, i + 4)
            if all(v % 4 == 0 and (v // 4) in _TSPY_DEFENSE_IDX for v in idxs):
                out.append([ATTRIB_NAME_THUNDERSPY[v // 4] for v in idxs])
                i += 4 + k * 4
                continue
        i += 4
    return out


def _parse_effect_template_thunderspy(r: BinReader, strtab_data, strtab_base,
                                      *, incarnate_scope: str | None = None) -> EffectTemplate:
    """Parse a Thunderspy AttribMod template.

    Thunderspy predates the enum-coded AttribMod format used by HC (Parse7) and
    Parse6/Rebirth. Attribs are stored as STRING NAMES (string-table offsets)
    rather than enum indices, and the tail block beyond `requires` has a
    variable layout the source spec doesn't document publicly.

    Strategy: parse the well-defined header (attribs, magnitude-default,
    duration defaults, requires-RPN) deterministically, then SCAN the remaining
    bytes for the modifier table name (`Melee_Damage`, `Ranged_Damage`, …) and
    pull the table-scale + duration that immediately follow it. This loses some
    of the rarely-used flags but recovers the load-bearing damage/heal/mez
    numbers the planner needs.
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

    # Attribs (front, string-offset form): populated for damage / single-attrib
    # templates whose attrib carries a string name (Damage, Knockback, Ones, …).
    attrib_offs = r.read_u4_array()
    attribs = [n for n in (_resolve_str(o) for o in attrib_offs) if n]

    # Magnitude (default — table-scale found later usually overrides for damage)
    magnitude = r.read_f4()
    # Two unknown u4s — often 0; likely Aspect + Target enums in some records
    r.read_u4()
    r.read_u4()
    # Duration defaults — instant attacks use -1.0 sentinel
    duration_default = r.read_f4()
    max_duration_default = r.read_f4()
    # Requires expression (RPN string tokens)
    req_offs = r.read_u4_array()
    req_parts = [n for n in (_resolve_str(o) for o in req_offs) if n]
    jit_requires = ' '.join(req_parts) if req_parts else ''

    # Attribs (index form) — the CANONICAL affected-attribute list. Right after
    # the requires array Thunderspy stores [pad, pad, marker, someval, count,
    # count×(attribIndex*4)] (attrib index encoded as byte offset into the
    # 4-byte-per-entry attrib table, per GAME-DATA-PRINCIPLES). Multi-type buffs
    # — most importantly the +DEF(All) defense toggles (Maneuvers, Weave, Hover,
    # armor sets) — leave the front string-attrib array EMPTY and list their
    # affected attribs (Smashing, Lethal, Ranged, Melee, …) only here. Without
    # reading it, those toggles lose every defense attrib and contribute 0 to the
    # planner's Defense totals. Verified: 295/295 empty-front-attrib tspy
    # templates carry this array; 273 decode fully to known attribs, the rest to
    # known attribs plus a few unmapped-exotic indices. Read as a FALLBACK only
    # (never overriding a resolved front string attrib), and PEEK it (no reader
    # advance) so the table tail-scan below is unaffected.
    _idx_attribs = []
    _idx_count = 0  # raw entry count (before dropping unmapped indices)
    try:
        ipos = r._pos + 16  # skip pad, pad, marker, someval
        icount = struct.unpack_from('<I', strtab_data, ipos)[0]
        if 0 < icount <= 32 and ipos + 4 + icount * 4 <= r._end:
            _idx_count = icount
            idxs = struct.unpack_from('<%dI' % icount, strtab_data, ipos + 4)
            _idx_attribs = [ATTRIB_NAME_THUNDERSPY[v // 4] for v in idxs
                            if v % 4 == 0 and (v // 4) in ATTRIB_NAME_THUNDERSPY]
    except Exception:
        pass
    if not attribs:
        attribs = _idx_attribs
    elif (attribs == ['Ones'] and _idx_count == 1 and len(_idx_attribs) == 1
          and _idx_attribs[0] in _TSPY_ONES_RECOVERABLE):
        # `Ones` front + a lone recoverable index attrib → surface the real stat
        # (recharge / recovery / regen / endurance) so the converter can classify
        # it. Requiring the index array to name EXACTLY ONE attrib keeps this off
        # the multi-attrib mez-protection / status-block templates that also carry
        # `Ones` fronts. This replaces the shortHelp-driven `recoverThunderspyOnesBuffs`
        # converter workaround with the actual binary datum.
        attribs = _idx_attribs

    # Scan the remainder for the first table-name string, then read the
    # canonical CoH AttribMod numeric block that follows it. Thunderspy shares
    # HC's Parse7 post-table layout *exactly* (verified by hand-decoding Gloom +
    # surveying 2,672 DoT damage templates against the Parse7 reader above):
    #
    #   table(str) scale(f4) duration(f4) magnitude(f4)
    #   dur_expr(u4_array) mag_expr(u4_array)   ← almost always empty (count=0)
    #   delay(f4) application_period(f4) tick_chance(f4) ...
    #
    # The two empty expression arrays are why slots table+16 / table+20 read 0
    # across the whole dataset; delay lands at table+24, the *period* (DoT tick
    # interval) at table+28, tick_chance at table+32. Reading the arrays as
    # arrays (not fixed offsets) keeps the period aligned if a template ever
    # carries a non-empty expr list. Recovering application_period is the fix
    # for "every Thunderspy DoT loses its tickRate" — without it the converter's
    # `dmg.tickRate` is never set and the calc can't compute tick count.
    tail_start = r._pos
    tail = bytes(strtab_data[tail_start:r._end])
    table = ''
    scale = 0.0
    duration = 0.0
    app_period = 0.0
    delay = 0.0
    # Post-table magnitude (HC Parse7 layout: table scale duration MAGNITUDE).
    # For damage this is the ~1.0 per-tick multiplier; for MEZ it is the real
    # applied Magnitude (Mag 3 hold, Mag 4 taunt, …) — the template-level
    # `magnitude` read from the header above is only a flat placeholder on this
    # schema. Adopted below when we relabel a mez template to its index attrib.
    mag_post_table = 0.0
    for k in range(0, len(tail) - 4, 4):
        u = struct.unpack_from('<I', tail, k)[0]
        candidate = _resolve_str(u)
        if not candidate or not candidate.startswith(_TSPY_TABLE_PREFIXES):
            continue
        table = candidate

        def _f4(off: int) -> float:
            return struct.unpack_from('<f', tail, off)[0] if off + 4 <= len(tail) else 0.0

        def _skip_u4_array(off: int) -> int:
            """Return the offset past a [u4 count][count×u4] block at `off`."""
            if off + 4 > len(tail):
                return off + 4
            count = struct.unpack_from('<I', tail, off)[0]
            # A bogus count means we're misreading; treat as empty so the fixed
            # post-table offsets still apply (expr arrays are empty in practice).
            if count > 64:
                return off + 4
            return off + 4 + count * 4

        scale = _f4(k + 4)
        duration = _f4(k + 8)
        mag_post_table = _f4(k + 12)  # real applied Magnitude (see note above)
        # k+12 is the post-table magnitude; walk past it to the expression arrays.
        pos = k + 16
        pos = _skip_u4_array(pos)   # dur_expr tokens (count=0 → +4)
        pos = _skip_u4_array(pos)   # mag_expr tokens (count=0 → +4)
        delay = _f4(pos)
        app_period = _f4(pos + 4)   # application_period — the DoT tick interval
        break

    # Sentinel -1 means "instant" — collapse to 0 so downstream doesn't read
    # it as a real duration value.
    if duration_default == -1.0:
        duration_default = 0.0
    if duration < 0:
        duration = 0.0
    if app_period < 0:
        app_period = 0.0
    if delay < 0:
        delay = 0.0
    # Defense-buff templates: the front string-attrib is either empty or a bogus
    # 'Buff_Def' meta-token, but the AFFECTED positional/type defense attribs
    # (Melee, Ranged, Smashing, …) — what the converter needs to build the
    # power's defenseBuff — live in the index array. Prefer it here so armor
    # sets (Super Reflexes, Shield, Energy Aura, …) and the +DEF(All) toggles
    # actually carry their defense. (Verified: 660/678 'Buff_Def'-front defense
    # templates carry clean positional defense types in the index array; the
    # converter's defense-position/type filter ignores any stray non-defense
    # attrib, so this is safe for the handful that don't.)
    if table and 'Buff_Def' in table:
        # The fixed post-`requires` index read (above) lands on the real affected-attrib
        # array for simple defense toggles (Maneuvers, Cloak of Darkness — count>=2
        # there). But powers with a requires/redirect block (Mind Link / Link Minds,
        # Fade, Farsight, Invincibility, most armor passives) carry only a 1-type SHIM
        # at that offset, with the real multi-type defense array pushed ~140 bytes
        # downstream — so the fixed read alone rendered ~165 tspy powers as single-type
        # defense (Mind Link → Def(Melee) instead of Def(All)). When the read yields the
        # shim (<=1 decoded type), recover the real array by unioning the shim with the
        # WIDEST count-prefixed defense array in the element. Zero-regression: a
        # multi-type read (count>=2) IS the real array and is used byte-for-byte as
        # before. Verified vs the HC structural oracle: 226/241 exact-or-subset; the
        # residual diffs are systematic tspy rebalances (Ice Shield, Dodge, Focused
        # Senses), not misreads.
        if _idx_attribs and len(_idx_attribs) >= 2:
            attribs = _idx_attribs
        else:
            _def_arrs = _tspy_scan_defense_arrays(strtab_data, _elem_start, r._end)
            _widest = max(_def_arrs, key=len, default=None)
            if _widest and len(_widest) >= 2:
                _union = set(_idx_attribs) | set(_widest)
                attribs = [ATTRIB_NAME_THUNDERSPY[i] for i in sorted(_TSPY_DEFENSE_IDX)
                           if ATTRIB_NAME_THUNDERSPY[i] in _union]
            elif _idx_attribs:
                attribs = _idx_attribs

    # Resistance-armor / ToHit-buff templates: same shape as Buff_Def above — the
    # front string-attrib is only the enhancement CATEGORY token (`Res_DMG`,
    # `Buff_ToHit`), while the AFFECTED attribs (`Smashing_Dmg`, `Lethal_Dmg`, …
    # for resistance; `ToHit` for the buff) live in the index array. Without this,
    # every tspy resistance-armor toggle (Mind Over Body, Willpower, …) and the
    # ToHit half of team buffs (Link Minds) drop to zero effects. Surface the index
    # attribs here; the resistance aspect is synthesized below (after the `aspect`
    # reset). ToHit needs no aspect — a bare positive `ToHit` attrib is the
    # converter's plain tohitBuff. (Verified 2026-07-09 against the raw tspy bin:
    # `Res_DMG`-front templates carry per-type `*_Dmg` index attribs on `*_Res_DMG`
    # tables; `Buff_ToHit` → [`ToHit`].)
    _tspy_res_surfaced = False
    if table and 'Res_DMG' in table and attribs == ['Res_DMG'] and _idx_attribs:
        attribs = _idx_attribs
        _tspy_res_surfaced = True
    elif table and 'Buff_ToHit' in table and attribs == ['Buff_ToHit'] and _idx_attribs:
        attribs = _idx_attribs

    # For damage attacks, table-scale supersedes the template-level magnitude
    # default (which is the unscaled "1.0" placeholder). Magnitude only matters
    # for non-table effects like raw mez magnitudes.
    final_scale = scale if table else magnitude

    # Synthesized aspect. Thunderspy leaves the AttribMod aspect all-zero, so the
    # header carries no aspect and this stays '' for the vast majority of
    # templates (buff-vs-resistance / self-vs-foe are recovered by the converter
    # via table/target context). The scoped hybrid relabel below is the one place
    # the parser can safely synthesize it from the front category.
    aspect = ""

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
    if (incarnate_scope in ("hybrid", "hybrid_melee") and len(attribs) == 1
            and attribs[0] in _TSPY_HYBRID_GENERIC_FRONTS
            and table.endswith("_Ones") and _idx_attribs and final_scale > 0):
        _melee = incarnate_scope == "hybrid_melee"
        _classes = {_tspy_hybrid_aspect_class(a, melee=_melee) for a in _idx_attribs}
        if len(_classes) == 1 and None not in _classes:
            attribs = _idx_attribs
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
    if incarnate_scope == "destiny" and len(attribs) == 1:
        _front = attribs[0]
        _idxset = set(_idx_attribs)
        if _front == "Ones" and _idx_attribs:
            if _idxset <= _TSPY_HYBRID_DEFENSE_ATTRIBS and final_scale > 0:
                attribs = _idx_attribs
                aspect = "Current"
            elif _idxset <= _TSPY_MEZ_INDEX and final_scale < 0:
                attribs = _idx_attribs
                aspect = "Current"
            elif _idxset <= _TSPY_KB_OFFENSIVE and final_scale < 0:
                attribs = _idx_attribs
                aspect = "Current"
        elif _front == "Res_Boolean" and _idx_attribs and _idxset <= _TSPY_MEZ_INDEX:
            attribs = _idx_attribs
            aspect = "Resistance"
        elif _front == "Tempdamage" and _idx_attribs == ["Heal_Dmg"]:
            attribs = _idx_attribs
            aspect = "Absolute"
        elif _front == "ArchVillain_Res":
            aspect = "Resistance"

    # Applied mez: relabel the front (an enhancement/duration category) to the
    # index array's real mez attrib and adopt the post-table Magnitude. Fires for
    # both `Ones` fronts (Tesla Cage) and mismatched real-mez fronts (Blind's
    # `Immobilize`-front Hold). `Res_Boolean` tables are protection thresholds, not
    # applied mez, so they're excluded; the self/ally target-trap is vetoed in the
    # converter (this schema drops the per-template target). `scale` here is the
    # duration multiplier, left untouched — only the Magnitude is corrected.
    if (_idx_count == 1 and len(_idx_attribs) == 1
            and _idx_attribs[0] in _TSPY_MEZ_INDEX
            and 'Res_Boolean' not in table
            and final_scale > 0):
        attribs = _idx_attribs
        if mag_post_table > 0:
            magnitude = mag_post_table
    # Offensive knockback/knockup: `Ones`-front, lone KB index, instant + positive
    # → the attack's knockdown/knockup. Durational / negative / protection KB stays
    # `Ones` (excluded). Front-real `Knockback` already routes through the converter.
    elif (attribs == ['Ones'] and _idx_count == 1 and len(_idx_attribs) == 1
            and _idx_attribs[0] in _TSPY_KB_OFFENSIVE
            and duration == 0 and final_scale > 0):
        attribs = _idx_attribs

    r.skip_to_end()
    return EffectTemplate(
        attribs=attribs,
        type="",  # Thunderspy schema doesn't expose this consistently
        application_type="",
        aspect=aspect,
        target="",
        table=table,
        scale=final_scale,
        duration=duration or duration_default,
        magnitude=magnitude,
        delay=delay,
        application_period=app_period,
        jit_requires=jit_requires,
    )


def _parse_effect_template_parse6(r: BinReader, *, thunderspy: bool = False) -> EffectTemplate:
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
    """
    # Header strings (5)
    _name = r.read_string()
    _attacker_msg = r.read_string()
    _victim_msg = r.read_string()
    _float_msg = r.read_string()
    _adef_float_msg = r.read_string()

    # Display flags + core attrib/aspect/target enums
    _show_floaters = r.read_bool()
    attrib_raw = r.read_u4()
    # Parse6 stores Attrib as a single u4 = index*4 (same scaling as HC), but
    # the index space diverges in the meta/scripting block (Rebirth puts
    # Create_Entity at 116, HC at 117). Use the Rebirth-specific map so
    # entity-spawn templates resolve and their `params.entity_def` gets
    # populated for the downstream summon wiring.
    # Thunderspy uses HC's ATTRIB_NAME map (predates the Parse6/Rebirth +1 shift
    # at Create_Entity and Accuracy); Rebirth uses ATTRIB_NAME_REBIRTH.
    attrib_map = ATTRIB_NAME if thunderspy else ATTRIB_NAME_REBIRTH
    attribs = [attrib_map.get(attrib_raw // 4, f"Unknown({attrib_raw // 4})")]
    aspect_raw = r.read_u4()
    # Parse6 uses value*4 encoding (4-byte aspect-table entries) vs HC's
    # value*8. Distribution across 21,559 Rebirth records: top values
    # are 0, 8, 16, 12, 4 — all multiples of 4 — confirming the older
    # 4-byte runtime layout.
    aspect = ATTRIB_MOD_ASPECT.get(aspect_raw // 4, f"Unknown({aspect_raw})")
    _boost_ignore_diminishing = r.read_bool()
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

    # CancelOnMiss + CancelEvents
    cancel_on_miss = r.read_bool()
    cancel_event_ids = r.read_u4_array()
    cancel_events = [EVENT_NAME.get(v, f"Event_{v}") for v in cancel_event_ids]

    # Boolean flag block — 9 bools per descriptor:
    #   NearGround, AllowStrength, AllowResistance,
    #   UseMagnitudeResistance, UseDurationResistance,
    #   AllowCombatMods, UseMagnitudeCombatMods, UseDurationCombatMods,
    #   BoostTemplate
    bool_block = [r.read_bool() for _ in range(9)]

    # AttribMod flags. Parse6 stores them as the inverse `Allow*` bools (+ the
    # CancelOnMiss bool above) instead of HC's Parse7 packed bitmask — but the
    # downstream `EffectTemplate.flags` is HC-shaped, so map them to the same
    # keywords the calc already honors (`IgnoreStrength` splits enhanced buffs to
    # an Unenhanced bucket; see GAME-DATA-PRINCIPLES §4). Validated against the HC
    # binary (Hide → IgnoreResistance; Jab → none) 2026-06-12. CopyBoosts/PseudoPet
    # live in the not-yet-decoded post-magnitude tail, NOT here — still deferred.
    flags: list[str] = []
    if cancel_on_miss:
        flags.append("CancelOnMiss")
    if bool_block[0]:               # NearGround
        flags.append("NearGround")
    if not bool_block[1]:           # AllowStrength=False → IgnoreStrength
        flags.append("IgnoreStrength")
    if not bool_block[2]:           # AllowResistance=False → IgnoreResistance
        flags.append("IgnoreResistance")
    if not bool_block[5]:           # AllowCombatMods=False → IgnoreCombatMods
        flags.append("IgnoreCombatMods")

    # Requires (RPN tokens) + two unused string_arrays
    requires_tokens = r.read_string_array()
    jit_requires = ' '.join(requires_tokens) if requires_tokens else ''
    _primary_str_list = r.read_string_array()
    _secondary_str_list = r.read_string_array()

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
    # 8 carry a duration_expression. Joined with spaces to match the
    # group-level requires_expression representation, exactly as the Parse7
    # path does.
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
    dur_expr = " ".join(r.read_string_array())  # DurationExpr (RPN tokens; usually empty)
    magnitude = r.read_f4()
    mag_expr = " ".join(r.read_string_array())  # MagnitudeExpr (RPN tokens; usually empty)

    # Tail: RadiusInner/Outer, Suppress, ContinuingFX, ConditionalFX,
    # Power, Reward, Params, EntityDef, PriorityList[Passive], display-
    # only flags, BoostModAllowed, ProcsPerMinute. The full layout isn't
    # fully reverse-engineered for Parse6, but the Power slot is the only
    # field the planner cares about (it carries the granted-power dotted
    # name for Grant_Power / Null-attrib chance procs — Stalker Assassin's
    # Focus, Radiation Melee Contaminated, Beast Mastery Pack Mentality,
    # etc.). Scan the remaining bytes for inline-pascal strings that look
    # like power names; mirror's HC's `_extract_params` heuristic but
    # adapted to Parse6's inline string layout.
    tail_start = r._pos
    tail_end = r._end
    tail_bytes = bytes(r._data[tail_start:tail_end])
    r.skip_to_end()
    params = _extract_params_parse6(tail_bytes, attribs)

    return EffectTemplate(
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
        flags=flags,
        params=params,
    )


def _parse_effects_parse6(r: BinReader, *, thunderspy: bool = False,
                          incarnate_scope: str | None = None) -> tuple[list[EffectGroup], list[EffectGroup]]:
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
            if thunderspy:
                tmpl = _parse_effect_template_thunderspy(
                    elem_reader, r._data, r._strtab_base, incarnate_scope=incarnate_scope
                )
            else:
                tmpl = _parse_effect_template_parse6(elem_reader)
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
            req = tmpl.jit_requires or ''
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
            self_exclusion = 'entref target> entref source> eq !' in req.lower()
            if tmpl.target == 'Self' and self_exclusion:
                is_pvp = 'EITHER'
            elif 'target> player eq' in req:
                is_pvp = 'PVP_ONLY'
            elif 'target> critter eq' in req:
                is_pvp = 'PVE_ONLY'
            else:
                is_pvp = 'EITHER'
            effects.append(EffectGroup(
                chance=tmpl.tick_chance if tmpl.tick_chance > 0 else 1.0,
                requires_expression=req,
                is_pvp=is_pvp,
                templates=[tmpl],
            ))
            # Thunderspy summon (Create_Entity) expansion. Thunderspy packs the
            # EntCreate list into a nested struct-array the single-template
            # parser above doesn't surface, so summon powers carry only a bare
            # `Ones`/`Level` shell and lose their pet linkage. Emit one
            # Create_Entity template (with `params` EntCreate) per nested pet —
            # matching HC's shape so the existing converter/display path
            # (`params.entity_def` → PET_ENTITIES) works unchanged. See
            # `_extract_thunderspy_summons` + THUNDERSPY_PARSER.md.
            if thunderspy:
                summons = _extract_thunderspy_summons(
                    r._data, elem_start, elem_start + elem_len,
                    r._strtab_base, tmpl.attribs[0] if tmpl.attribs else '',
                    tmpl.table,
                )
                if summons:
                    front = tmpl.attribs[0] if tmpl.attribs else ''
                    if front in _TSPY_SUMMON_SHELL_FRONTS:
                        effects.pop()  # drop the summon shell (HC emits no shell)
                    for params in summons:
                        ce = EffectTemplate(
                            attribs=['Create_Entity'],
                            type='Magnitude',
                            application_type='Immediate',
                            table=tmpl.table,
                            scale=tmpl.scale,
                            duration=tmpl.duration,
                            magnitude=tmpl.magnitude or 1.0,
                            jit_requires=tmpl.jit_requires,
                            params=params,
                        )
                        effects.append(EffectGroup(
                            chance=tmpl.tick_chance if tmpl.tick_chance > 0 else 1.0,
                            requires_expression=req,
                            is_pvp=is_pvp,
                            templates=[ce],
                        ))
        except Exception:
            pass  # Skip unparseable templates
        r.skip(elem_len)
    return effects, []


def _parse_power_parse6(r: BinReader, *, thunderspy: bool = False,
                        veracity: bool = False) -> PowerRecord:
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
    r.read_bool()  # free
    display_name = r.read_string()
    display_help = r.read_string()
    short_help = r.read_string()
    for _ in range(9): r.read_string()  # 13-21
    icon = r.read_string()  # 22
    power_type = r.read_u4()  # 23
    num_allowed = r.read_u4()  # 24
    attack_types = r.read_u4_array()  # 25
    requires_parts = r.read_string_array()  # 26
    requires = " ".join(requires_parts) if requires_parts else ""
    activate_requires_parts = r.read_string_array()  # 27
    activate_requires = " ".join(activate_requires_parts) if activate_requires_parts else ""
    # 28. slot_requires (string_array). Boost (IO piece) records carry
    # `BoostsSlotted>X <= 0` constraints here when the piece is unique
    # within a slot pool — read by the IO-set extractor for per-piece uniqueness.
    slot_requires_parts = r.read_string_array()  # 28
    slot_requires = " ".join(slot_requires_parts) if slot_requires_parts else ""
    target_requires_parts = r.read_string_array()  # 29
    target_requires = " ".join(target_requires_parts) if target_requires_parts else ""
    r.read_string_array()  # 30
    r.read_string_array()  # 31
    if veracity:
        r.read_string_array()  # 31b Dictionary (Veracity only)
    r.read_string()  # 32 reward_fallback
    accuracy = r.read_f4()  # 33
    cast_through, toggle_ignore = _parse_cast_flags(r)  # 34
    r.read_u4()  # 35 ai_report
    # (NO 35b in Parse6)
    effect_area = r.read_u4()  # 36
    max_targets_hit = r.read_u4()  # 37
    # (NO 38 chain_effect_array or 38b-d in Parse6)
    radius = r.read_f4()  # 39
    arc = r.read_f4()  # 40
    r.read_f4()  # 41 chain_delay
    chain_eff_expr = " ".join(r.read_string_array())  # 42 ChainEff (verified:
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
    r.read_bool()  # 56
    r.read_bool()  # 57
    r.read_u4(); r.read_u4()  # 58-59
    for _ in range(6): r.read_f4()  # 60-65
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
    # 2026-07-07 (see streams/OPEN_ITEMS.md §3). Field 74 (GroupMembership/
    # exclusion) was consumed in the loop above; what follows differs per
    # flavor:
    #
    #   Rebirth (Parse6):  ModesRequired, ModesDisallowed, ModesSuspended,
    #                      AIGroups (string_array, inline pascal), Effects.
    #                      Redirects live in the post-effects tail (REB3),
    #                      not here.
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
    modes_suspended: list[int] = []
    try:
        modes_required = r.read_u4_array()
        modes_disallowed = r.read_u4_array()
        if thunderspy:
            r.read_string_array()  # AIGroups (e.g. kHeavyGroup on NPC powers)
            try:
                redirects = _parse_redirects(r)
            except Exception as e:
                # Same loud path as the HC parser: a redirect misparse here
                # misaligns the effects read — surface it, don't hide it.
                redirects = []
                _warn_dropped(full_name,
                              f"redirect parse failed ({e}); effects may be lost")
                r.skip_to_end()
        else:
            modes_suspended = r.read_u4_array()
            r.read_string_array()  # AIGroups (e.g. kEarlyBattle on pet powers)
        try:
            if veracity:
                # Veracity uses HC-style EffectGroup effects (no Redirect /
                # ActivationEffect precede them), reached directly here.
                effects, activation_effects = _parse_effects(r, veracity=True)
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
                    r, thunderspy=thunderspy, incarnate_scope=incarnate_scope)
        except Exception:
            effects = []
            activation_effects = []
    except Exception:
        pass

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
        # Parse6 has no MaxTargetsExpr (field 38, HC-only) and no ChainTarget
        # (field 43 is an FX array here — see the read above). Only ChainEff.
        chain_eff_expression=chain_eff_expr,
        effects=effects,
        activation_effects=activation_effects,
        redirects=redirects,
        modes_required=modes_required,
        modes_disallowed=modes_disallowed,
        modes_suspended=modes_suspended,
        exclusion_groups=exclusion_groups,
    )
