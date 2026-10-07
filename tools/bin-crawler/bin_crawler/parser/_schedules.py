"""Parsers for the leveling-progression bins: schedules.bin (power picks,
enhancement-slot grants, pool/epic unlocks, inspiration slots per level) and
exemplar_handicaps.bin (the exemplar magnitude clamp curves).

Structure proven against the game's own TextParser tables in the released CoH
server source, the authoritative oracle for both files:

    Schedules            { Powers: Schedule }            (ParseSchedules,
    Schedule             { FreeBoostSlotsOnPower: int[]   power_system.c)
                           PoolPowerSet:          int[]
                           EpicPowerSet:          int[]
                           Power:                 int[]
                           AssignableBoost:       int[]
                           InspirationCol:        int[]
                           InspirationRow:        int[] }

    ExemplarHandicaps    { Limits:    f32[]              (ParseBoostExemplarTable,
                           Weights:   f32[]               boost.h)
                           PreClamp:  f32[]
                           PostClamp: f32[] }

Schedule semantics (`CountForLevel`, power_system.c): each array is a sorted
list of 0-based security levels; the number of entries <= a level is how many
of that thing (powers, boost slots, pool sets, ...) the character has at that
level. E.g. HC's Power = [0, 0, 1, 3, ...] means two power picks at level 1
(0-based 0), a third at 2, a fourth at 4. This is the binary source for the
planner's hand-authored POWER_PICK_LEVELS / SLOT_GRANTS tables (levels.ts).

ExemplarHandicaps semantics (`boost_HandicapExemplar`, boost.c): all four
curves are indexed by 0-based combat level, clamped to the last entry. A
magnitude above the PreClamp entry is capped to it; a magnitude at or above
the Limits entry is scaled by weights[combat] / weights[experience]; PostClamp
caps the result. This is the binary source for the planner's exemplar
enhancement-clamp rules.

All three shipped datasets carry both files: HC and Thunderspy as Parse7,
Rebirth as Parse6 (Files1 container) — same field layout, no strings either
way. Not to be confused with the ED schedules in dim_returns.bin
(_dim_returns.py), an unrelated use of the word.
"""

from ._dataclasses import ExemplarHandicapCurves, LevelingSchedule
from ._reader import open_parse7

# Plausibility bounds. The largest shipped array is Thunderspy's
# AssignableBoost at 71 entries; triple digits means we are reading data as a
# count. Schedule entries are 0-based security levels (max shipped level 50);
# a value past 200 is a misalignment, not a level.
_MAX_PLAUSIBLE_COUNT = 512
_MAX_PLAUSIBLE_LEVEL = 200


def _read_level_array(reader, field_name: str) -> list[int]:
    values = reader.read_u4_array()
    if len(values) > _MAX_PLAUSIBLE_COUNT:
        raise ValueError(
            f"implausible {field_name} count {len(values)} — misaligned read")
    # strict=False is the pairwise idiom, not a truncation: the second operand is the same
    # list offset by one, so it is shorter by exactly one by construction.
    for previous, current in zip(values, values[1:], strict=False):
        if current < previous:
            raise ValueError(
                f"{field_name} levels not sorted ascending ({values}) — "
                f"CountForLevel semantics require sorted arrays; misaligned read")
    if values and values[-1] > _MAX_PLAUSIBLE_LEVEL:
        raise ValueError(
            f"implausible {field_name} level {values[-1]} — misaligned read")
    return values


def parse_schedules(bin_path_or_data) -> LevelingSchedule:
    """Parse schedules.bin into a LevelingSchedule.

    Raises ValueError on any structural misalignment; every byte of the
    embedded Schedule struct and the outer block must be consumed.
    """
    r = open_parse7(bin_path_or_data)
    r.read_u4()  # data block size (validated implicitly by full consumption)

    # ParseSchedules is a single embedded struct ("Powers"), size-prefixed.
    schedule_size = r.read_u4()
    s = r.sub_reader(schedule_size)
    schedule = LevelingSchedule(
        free_boost_slots_on_power=_read_level_array(s, "FreeBoostSlotsOnPower"),
        pool_power_set=_read_level_array(s, "PoolPowerSet"),
        epic_power_set=_read_level_array(s, "EpicPowerSet"),
        power=_read_level_array(s, "Power"),
        assignable_boost=_read_level_array(s, "AssignableBoost"),
        inspiration_column=_read_level_array(s, "InspirationCol"),
        inspiration_row=_read_level_array(s, "InspirationRow"),
    )
    if s.remaining():
        raise ValueError(f"Schedule struct has {s.remaining()} unread bytes")
    r.skip(schedule_size)

    if r.remaining():
        raise ValueError(f"schedules.bin has {r.remaining()} trailing bytes")
    if not schedule.power:
        raise ValueError("schedules.bin has an empty Power schedule — "
                         "no power picks would ever be granted")
    return schedule


def _read_curve_array(reader, field_name: str) -> list[float]:
    count = reader.read_u4()
    if count > _MAX_PLAUSIBLE_COUNT:
        raise ValueError(
            f"implausible {field_name} count {count} — misaligned read")
    return [reader.read_f4() for _ in range(count)]


def parse_exemplar_handicaps(bin_path_or_data) -> ExemplarHandicapCurves:
    """Parse exemplar_handicaps.bin into ExemplarHandicapCurves.

    Raises ValueError on any structural misalignment; every byte must be
    consumed. Empty Limits/handicaps arrays are legal to the engine (it
    returns magnitudes unclamped) but no shipped dataset does that, so an
    empty curve here is treated as a misalignment signal.
    """
    r = open_parse7(bin_path_or_data)
    r.read_u4()  # data block size

    curves = ExemplarHandicapCurves(
        limits=_read_curve_array(r, "Limits"),
        handicaps=_read_curve_array(r, "Weights"),
        pre_clamp=_read_curve_array(r, "PreClamp"),
        post_clamp=_read_curve_array(r, "PostClamp"),
    )
    if r.remaining():
        raise ValueError(
            f"exemplar_handicaps.bin has {r.remaining()} trailing bytes")
    if not curves.limits or not curves.handicaps:
        raise ValueError(
            "exemplar_handicaps.bin has empty Limits/Weights curves — "
            "exemplar clamping would be a no-op; misaligned read")
    return curves
