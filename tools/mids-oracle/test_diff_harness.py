#!/usr/bin/env python3
"""Regression checks for the DSH5 comparator's canonicalizations (PROV-4).

The harness itself needs a local Mids `.mhd` and the canonical export cache; these
are pure-function checks on the record layer, so they run anywhere.

Run:
  python3 tools/mids-oracle/test_diff_harness.py
"""

from __future__ import annotations

import unittest

import diff_harness as D


def oe(effect_type, *, et_modifies="None", damage_type="None", mez_type="None",
       resistible=False, table="Melee_Stun", aspect="Str"):
    """A minimal oracle effect — every field `oracle_records` reads."""
    return {"effect_type": effect_type, "et_modifies": et_modifies,
            "damage_type": damage_type, "mez_type": mez_type, "resistible": resistible,
            "modifier_table": table, "pv_mode": "Any", "aspect": aspect,
            "attrib_type": "Magnitude", "scale": 0.66}


def keys(recs):
    return sorted(D.details_of(recs))


class TestEnhancedAttrib(unittest.TestCase):
    """et_modifies -> the attribute the Enhancement record enhances, in export vocab."""

    def test_mez_and_defense_keep_enhancement_and_take_their_vector(self):
        self.assertEqual(D.enhanced_attrib(oe("Enhancement", et_modifies="Mez",
                                              mez_type="Held")), ("Enhancement", "Held"))
        self.assertEqual(D.enhanced_attrib(oe("Enhancement", et_modifies="Defense",
                                              damage_type="Melee")), ("Enhancement", "Melee"))

    def test_defense_with_no_vector_is_the_bare_all_record(self):
        # Mids writes all-defense as damage_type=None; the export writes Base_Defense
        # -> subType 'All'. norm_sub folds both to ''.
        self.assertEqual(D.enhanced_attrib(oe("Enhancement", et_modifies="Defense")),
                         ("Enhancement", ""))
        self.assertEqual(D.norm_sub("All"), "")

    def test_damage_strength_is_a_damage_buff_not_dealt_damage(self):
        self.assertEqual(D.enhanced_attrib(oe("Enhancement", et_modifies="Damage",
                                              damage_type="Fire")), ("DamageBuff", "Fire"))

    def test_every_movement_axis_folds_to_bare_movement(self):
        for axis in ("SpeedRunning", "SpeedFlying", "SpeedJumping", "JumpHeight"):
            self.assertEqual(D.enhanced_attrib(oe("Enhancement", et_modifies=axis)),
                             ("Movement", ""), axis)

    def test_scalars_keep_their_own_type(self):
        for etm, want in (("ToHit", "ToHit"), ("Heal", "Heal"), ("Absorb", "Absorb"),
                          ("Endurance", "Endurance"), ("RechargeTime", "RechargeTime"),
                          ("Accuracy", "Accuracy"), ("Range", "Range")):
            self.assertEqual(D.enhanced_attrib(oe("Enhancement", et_modifies=etm)),
                             (want, ""), etm)

    def test_an_unmappable_et_modifies_reports_instead_of_guessing(self):
        et, why = D.enhanced_attrib(oe("Enhancement", et_modifies="InterruptTime"))
        self.assertIsNone(et)
        self.assertIn("InterruptTime", why)


class TestOracleRecords(unittest.TestCase):
    def test_power_boost_enhancements_no_longer_share_one_key(self):
        """The PROV-4 collapse: three different enhanced attributes, one key."""
        effects = [oe("Enhancement", et_modifies="SpeedRunning"),
                   oe("Enhancement", et_modifies="SpeedFlying"),
                   oe("Enhancement", et_modifies="Defense"),
                   oe("Enhancement", et_modifies="ToHit"),
                   oe("Enhancement", et_modifies="Mez", mez_type="Held")]
        self.assertEqual(keys(D.oracle_records({"effects": effects})),
                         [("Enhancement|", "U"), ("Enhancement|Held", "U"),
                          ("Movement", "U"), ("ToHit|", "U")])

    def test_the_fold_census_names_what_it_could_not_route(self):
        from collections import Counter
        census = Counter()
        D.oracle_records({"effects": [oe("Enhancement", et_modifies="ToHit"),
                                      oe("Enhancement", et_modifies="HitPoints"),
                                      oe("Enhancement", et_modifies="Rage")]}, census)
        self.assertEqual(census["ToHit -> ToHit"], 1)
        self.assertEqual(census["HitPoints -> MaxHP (not comparable)"], 1)
        self.assertEqual(census["UNROUTED et_modifies=Rage"], 1)

    def test_a_plain_oracle_record_is_untouched_by_the_fold(self):
        self.assertEqual(keys(D.oracle_records({"effects": [
            oe("Damage", damage_type="Fire", aspect="Abs", resistible=True)]})),
            [("Damage|Fire", "R")])


class TestFoldComplete(unittest.TestCase):
    def test_both_complete_sets_in_one_group_fold(self):
        """PROV-4 puts defense strength in `Enhancement` beside mez strength, so one
        (effectType, resistible) group can hold all 8 damage types AND all 3
        positions. Folding only the first match left the other vector expanded."""
        effects = [oe("Enhancement", et_modifies="Defense", damage_type=d)
                   for d in sorted(D.COMPLETE_DAMAGE | D.COMPLETE_POSITION)]
        recs = D.oracle_records({"effects": effects})
        self.assertEqual([r["sub"] for r in recs], ["", ""])

    def test_an_incomplete_set_stays_expanded(self):
        effects = [oe("Enhancement", et_modifies="Defense", damage_type=d)
                   for d in sorted(D.COMPLETE_DAMAGE - {"Toxic"})]
        self.assertEqual(sorted(r["sub"] for r in D.oracle_records({"effects": effects})),
                         sorted(D.COMPLETE_DAMAGE - {"Toxic"}))

    def test_the_folded_record_takes_a_named_representative_not_an_arbitrary_one(self):
        """The representative donates the folded record's table; picking it by set
        iteration order moved the advisory INV5 count between runs of one tree."""
        effects = [oe("Enhancement", et_modifies="Defense", damage_type=d,
                      table=f"Tbl_{d}") for d in sorted(D.COMPLETE_DAMAGE)]
        self.assertEqual([r["table"] for r in D.oracle_records({"effects": effects})],
                         [f"Tbl_{min(D.COMPLETE_DAMAGE)}"])

    def test_the_resistible_twin_folds_independently(self):
        effects = ([oe("Enhancement", et_modifies="Defense", damage_type=d)
                    for d in sorted(D.COMPLETE_DAMAGE)]
                   + [oe("Enhancement", et_modifies="Defense", damage_type=d, resistible=True)
                      for d in sorted(D.COMPLETE_DAMAGE)])
        self.assertEqual(keys(D.oracle_records({"effects": effects})),
                         [("Enhancement|", "R"), ("Enhancement|", "U")])


if __name__ == "__main__":
    unittest.main(verbosity=2)
