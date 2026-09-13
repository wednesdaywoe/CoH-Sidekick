#!/usr/bin/env python3
"""Smoke tests for diff_enh_oracle helper mappings.

Run:
  python3 tools/mids-oracle/test_diff_enh_oracle.py
"""

from __future__ import annotations

import unittest

import diff_enh_oracle


class TestDiffEnhOracle(unittest.TestCase):
    def test_name_aliases_canonicalize(self) -> None:
        self.assertEqual(
            diff_enh_oracle._canon_name("Ascendency of the Dominator"),
            "ascendancy of the dominator",
        )
        self.assertEqual(
            diff_enh_oracle._canon_name("Numina's Convalesence"),
            "numina's convalescence",
        )

    def test_oracle_effect_mapping_damagebuff(self) -> None:
        effect = {
            "effect_type": "DamageBuff",
            "damage_type": "Smashing",
            "aspect": "Str",
        }
        stat = diff_enh_oracle._oracle_effect_to_stat(effect)
        self.assertEqual(stat, "damage")
        # Gladiator's Javelin p4: scale 0.025 -> 2.5 = repo damage (default x100).
        val = abs(0.025) * diff_enh_oracle._bonus_multiplier(effect)
        self.assertAlmostEqual(val, 2.5, places=3)

    def test_oracle_effect_mapping_hpmax(self) -> None:
        effect = {
            "effect_type": "HitPoints",
            "damage_type": "None",
            "aspect": "Max",
        }
        stat = diff_enh_oracle._oracle_effect_to_stat(effect)
        self.assertEqual(stat, "maximum_hitpoints")
        val = abs(0.1125) * diff_enh_oracle._bonus_multiplier(effect)
        self.assertAlmostEqual(val, 1.125, places=3)

    # --- PROV-5: the comparator's stat vocabulary had drifted from the repo's ---

    def test_aoe_defense_speaks_the_repo_s_spelling(self) -> None:
        # MEZRES-1 renamed this stat defense_(area) -> defense_(aoe) on the repo
        # side; the alias stayed behind and split 115 matching rows into a
        # missing line and an extra line apiece.
        self.assertEqual(diff_enh_oracle._damage_name("AoE"), "aoe")
        effect = {"effect_type": "Defense", "damage_type": "AoE", "aspect": "Cur"}
        self.assertEqual(
            diff_enh_oracle._oracle_effect_to_stat(effect), "defense_(aoe)"
        )

    def test_repel_never_enters_the_mez_family_machinery(self) -> None:
        # Repel is not one of the six the >=6 fold collapses, so a raw key was
        # collected into mez_seen and dropped. It also must not be eligible to
        # donate its scale (10.0) as the family's representative value.
        effect = {"effect_type": "MezResist", "mez_type": "Repel", "aspect": "Res"}
        stat = diff_enh_oracle._oracle_effect_to_stat(effect)
        self.assertEqual(stat, "repel_resistance")
        self.assertFalse(stat.startswith("_mez_resist_raw_"))
        self.assertFalse(diff_enh_oracle._is_mez_family_member("_mez_resist_raw_Repel"))
        self.assertAlmostEqual(
            abs(10.0) * diff_enh_oracle._bonus_multiplier(effect), 1000.0, places=3
        )

    def test_knockback_protection_and_strength_split_on_aspect(self) -> None:
        # Same attrib, two faces: a Cur magnitude is protection, an Enhancement
        # at Str is strength. The name does not discriminate; the aspect does.
        protection = {
            "effect_type": "Mez",
            "mez_type": "Knockback",
            "aspect": "Cur",
            "scale": -3.0,
        }
        strength = {
            "effect_type": "Enhancement",
            "et_modifies": "Mez",
            "mez_type": "Knockback",
            "aspect": "Str",
        }
        self.assertEqual(
            diff_enh_oracle._oracle_effect_to_stat(protection), "knockback_protection"
        )
        self.assertEqual(
            diff_enh_oracle._oracle_effect_to_stat(strength), "knockback_strength"
        )

    def test_res_effect_reads_et_modifies_for_the_slow_triple(self) -> None:
        # Mids spells slow resistance as one ResEffect row per debuffed attrib,
        # keyed by et_modifies -- the same second field PROV-4 taught the DSH5
        # comparator to read. The repo carries one slot for the whole triple.
        for attrib in ("SpeedRunning", "SpeedFlying", "RechargeTime"):
            effect = {
                "effect_type": "ResEffect",
                "et_modifies": attrib,
                "aspect": "Res",
            }
            self.assertEqual(
                diff_enh_oracle._oracle_effect_to_stat(effect),
                "+res(recharge_debuff)",
                attrib,
            )
        # and it does not swallow every ResEffect it meets
        self.assertIsNone(
            diff_enh_oracle._oracle_effect_to_stat(
                {"effect_type": "ResEffect", "et_modifies": "Regeneration", "aspect": "Res"}
            )
        )

    def test_speedjumping_is_movement_like_its_three_siblings(self) -> None:
        # The Enhancement branch spells all four axes; the bare-effectType
        # branch below it listed three, leaving 24 rows unmapped.
        for et in ("SpeedRunning", "SpeedFlying", "SpeedJumping", "JumpHeight"):
            self.assertEqual(
                diff_enh_oracle._oracle_effect_to_stat(
                    {"effect_type": et, "aspect": "Cur"}
                ),
                "increased_movement",
                et,
            )

    def test_stat_vocabulary_reports_a_name_with_no_counterpart(self) -> None:
        oracle = {"a set": {2: {"defense_(area)": 1.88, "recovery": 1.0}}}
        repo = {"a set": {2: {"defense_(aoe)": 1.88, "recovery": 1.0}}}
        oracle_only, repo_only = diff_enh_oracle._stat_vocabularies(oracle, repo)
        self.assertEqual(oracle_only, ["defense_(area)"])
        self.assertEqual(repo_only, ["defense_(aoe)"])

    def test_stat_vocabulary_is_silent_when_the_two_sides_agree(self) -> None:
        both = {"a set": {2: {"defense_(aoe)": 1.88}}, "b set": {5: {"recovery": 1.0}}}
        self.assertEqual(diff_enh_oracle._stat_vocabularies(both, both), ([], []))

    def test_extra_proc_classification_uses_staleness_bucket_for_missing_set(self) -> None:
        bucket, reason = diff_enh_oracle._classify_extra_proc_pair(
            "absolute resolution",
            "chance for energy damage",
            {"categories": {"damage"}},
            {"aegis", "stupefy"},
            {"stupefy": {"chance for knockback"}},
        )
        self.assertEqual(bucket, "likely_oracle_set_staleness")
        self.assertIn("absent", reason)

    def test_extra_proc_classification_keeps_mapping_gap_when_set_exists(self) -> None:
        bucket, reason = diff_enh_oracle._classify_extra_proc_pair(
            "guardian's gift",
            "chance for stun",
            {"categories": {"control"}},
            {"aegis", "guardian's gift"},
            {},
        )
        self.assertEqual(bucket, "likely_mapping_gap")
        self.assertIn("triggered", reason)

    def test_extra_proc_classification_oracle_proc_staleness_for_set_present_extra(self) -> None:
        bucket, reason = diff_enh_oracle._classify_extra_proc_pair(
            "stupefy",
            "chance for stun",
            {"categories": {"control"}, "ppm": 3.5},
            {"stupefy"},
            {"stupefy": {"chance for knockback"}},
        )
        self.assertEqual(bucket, "likely_oracle_proc_staleness")
        self.assertIn("no oracle counterpart", reason)

    def test_extra_proc_classification_conversion_is_non_proc_bucket(self) -> None:
        bucket, reason = diff_enh_oracle._classify_extra_proc_pair(
            "sudden acceleration",
            "convert knockback to knockdown",
            {"categories": {"special"}, "ppm": None},
            {"sudden acceleration"},
            {},
        )
        self.assertEqual(bucket, "likely_non_proc_global_or_passive")
        self.assertIn("conversion", reason)


if __name__ == "__main__":
    unittest.main()
