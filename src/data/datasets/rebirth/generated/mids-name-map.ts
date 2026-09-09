/**
 * Mids internal name → this dataset's internal name — AUTO-GENERATED, DO NOT EDIT.
 *
 * Keyed by OUR `group.powerset` (lower-cased), then by the Mids internal name
 * (lower-cased). The value is this dataset's internal name for the SAME power, joined on
 * the display name — the identity that survived HC's internal-name rotations. See
 * DATA-GAP MBDIMPORT-2.
 *
 * The key is ours rather than Mids' because Mids' group segment drifts too
 * (`Guardian_Composition` for our `Guardian_Comp`, MBDIMPORT-7). `MIDS_POWERSET_ALIAS`
 * below carries those pairs so a reader holding the .mbd's own path can reach the same row.
 *
 * Source: Mids Reborn rebirth database 2023.7.445 (sha256 d4c0b142ba76…)
 * Powersets paired with the export: 3459 of 3547. Remapped names: 36.
 * Reverse rows for the writer: 36.
 * Mids powersets with no counterpart here: 88 — listed by the generator on stderr.
 *
 * Regenerate: node scripts/convert-mids-name-map.cjs --dataset rebirth
 */

export const MIDS_NAME_MAP: Readonly<Record<string, Readonly<Record<string, string>>>> = {
  "brute_defense.super_reflexes": {
    "practiced brawler": "Practiced_Brawler"
  },
  "controller_buff.force_field": {
    "repulsion_field2": "Repulsion_Field_New"
  },
  "controller_control.water_control": {
    "tidal wave": "Tidal_Wave"
  },
  "corruptor_buff.force_field": {
    "repulsion_field2": "Repulsion_Field_New"
  },
  "defender_buff.force_field": {
    "repulsion_field2": "Repulsion_Field_New"
  },
  "dominator_assault.kinetic_assault": {
    "disrupting _torrent": "Disrupting_Torrent"
  },
  "dominator_assault.ninja_assault": {
    "gambler's_cut": "Gamblers_Cut",
    "scorpion's_sting": "Scorpions_Sting",
    "the lotus drops": "The_Lotus_Drops"
  },
  "dominator_control.water_control": {
    "tidal wave": "Tidal_Wave"
  },
  "epic.martial_mastery": {
    "shukuchi": "Shukuchi",
    "warrior's_mark": "Warriors_Mark"
  },
  "guardian_assault.electricity_assault": {
    "havok_punch": "Havoc_Punch"
  },
  "guardian_assault.icy_assault": {
    "greater_ice_sword": "Ice_Slash"
  },
  "guardian_assault.kinetic_assault": {
    "disrupting _torrent": "Disrupting_Torrent"
  },
  "guardian_assault.ninja_assault": {
    "gambler's_cut": "Gamblers_Cut",
    "scorpion's_sting": "Scorpions_Sting",
    "the lotus drops": "The_Lotus_Drops"
  },
  "guardian_comp.atmospheric_composition": {
    "groundeding_shield": "Grounding_Shield"
  },
  "guardian_comp.fiery_composition": {
    "power_of_the_phoenix": "Phoenix_Awakening"
  },
  "guardian_comp.force_composition": {
    "repulsion_field": "Containment_Shell"
  },
  "guardian_comp.stone_composition": {
    "gaia's_blessing": "Gaias_Blessing"
  },
  "mastermind_buff.force_field": {
    "repulsion_field2": "Repulsion_Field_New"
  },
  "mastermind_pets.protector_3": {
    "seeker drones": "Seeker_Drones"
  },
  "pets.brute_savage_melee": {
    "rending_flurry_large": "Rending_Flurry_Normal"
  },
  "pets.scrapper_savage_melee": {
    "rending_flurry_large": "Rending_Flurry_Normal"
  },
  "pool.gadgetry": {
    "energy gauntlet": "Wrist_Blaster",
    "force barrier": "Force_Barrier",
    "gauntlet barrage": "Blaster_Barrage",
    "nano net": "Nano_Net"
  },
  "scrapper_defense.super_reflexes": {
    "practiced brawler": "Practiced_Brawler"
  },
  "stalker_defense.super_reflexes": {
    "practiced brawler": "Practiced_Brawler"
  },
  "stalker_melee.ice_melee": {
    "assassins_icicle": "Assassins_Strike"
  },
  "tanker_defense.super_reflexes": {
    "practiced brawler": "Practiced_Brawler"
  },
  "temporary_powers.temporary_powers": {
    "incarnate_reward_dark_astoria": "Incarnate_Reward_Genesis"
  },
  "warshade_defensive.umbral_aura": {
    "starless_gateway": "Shadow_Slip"
  }
};

/**
 * Mids' `group.powerset` → ours, for the pairs that spell the group differently.
 *
 * A reader that starts from the .mbd (the importer's retired-name check) resolves through
 * this; a reader that starts from our own powers (the matcher) already holds the map's key.
 */
export const MIDS_POWERSET_ALIAS: Readonly<Record<string, string>> = {
  "guardian_composition.atmospheric_composition": "guardian_comp.atmospheric_composition",
  "guardian_composition.fiery_composition": "guardian_comp.fiery_composition",
  "guardian_composition.force_composition": "guardian_comp.force_composition",
  "guardian_composition.stone_composition": "guardian_comp.stone_composition"
};

/**
 * The same join backwards — THIS dataset's internal name (lower-cased) → Mids' own, for
 * the .mbd writer (DATA-GAP MBDEXPORT-3).
 *
 * Not derivable from `MIDS_NAME_MAP` above, and that is the point of emitting it. The
 * forward map keys on a folded spelling because its reader is matching; the writer is
 * producing, and Mids resolves a `PowerName` by ordinal `==` against its own database
 * string. Case and inner whitespace are load-bearing on this side and discarded on that
 * one — Rebirth spells one power `"Shukuchi "`, trailing space and all.
 *
 * One row per forward row, minus any withdrawn: two Mids names landing on one power of
 * ours is answerable forwards and not backwards, so that name gets no row and the writer
 * reports it instead of picking.
 */
export const MIDS_NAME_REVERSE: Readonly<Record<string, Readonly<Record<string, string>>>> = {
  "brute_defense.super_reflexes": {
    "practiced_brawler": "Practiced Brawler"
  },
  "controller_buff.force_field": {
    "repulsion_field_new": "Repulsion_Field2"
  },
  "controller_control.water_control": {
    "tidal_wave": "Tidal Wave"
  },
  "corruptor_buff.force_field": {
    "repulsion_field_new": "Repulsion_Field2"
  },
  "defender_buff.force_field": {
    "repulsion_field_new": "Repulsion_Field2"
  },
  "dominator_assault.kinetic_assault": {
    "disrupting_torrent": "Disrupting _Torrent"
  },
  "dominator_assault.ninja_assault": {
    "gamblers_cut": "Gambler's_Cut",
    "scorpions_sting": "Scorpion's_Sting",
    "the_lotus_drops": "The Lotus Drops"
  },
  "dominator_control.water_control": {
    "tidal_wave": "Tidal Wave"
  },
  "epic.martial_mastery": {
    "shukuchi": "Shukuchi ",
    "warriors_mark": "Warrior's_Mark"
  },
  "guardian_assault.electricity_assault": {
    "havoc_punch": "Havok_Punch"
  },
  "guardian_assault.icy_assault": {
    "ice_slash": "Greater_Ice_Sword"
  },
  "guardian_assault.kinetic_assault": {
    "disrupting_torrent": "Disrupting _Torrent"
  },
  "guardian_assault.ninja_assault": {
    "gamblers_cut": "Gambler's_Cut",
    "scorpions_sting": "Scorpion's_Sting",
    "the_lotus_drops": "The Lotus Drops"
  },
  "guardian_comp.atmospheric_composition": {
    "grounding_shield": "Groundeding_Shield"
  },
  "guardian_comp.fiery_composition": {
    "phoenix_awakening": "Power_of_the_Phoenix"
  },
  "guardian_comp.force_composition": {
    "containment_shell": "Repulsion_Field"
  },
  "guardian_comp.stone_composition": {
    "gaias_blessing": "Gaia's_Blessing"
  },
  "mastermind_buff.force_field": {
    "repulsion_field_new": "Repulsion_Field2"
  },
  "mastermind_pets.protector_3": {
    "seeker_drones": "Seeker Drones"
  },
  "pets.brute_savage_melee": {
    "rending_flurry_normal": "Rending_Flurry_Large"
  },
  "pets.scrapper_savage_melee": {
    "rending_flurry_normal": "Rending_Flurry_Large"
  },
  "pool.gadgetry": {
    "blaster_barrage": "Gauntlet Barrage",
    "force_barrier": "Force Barrier",
    "nano_net": "Nano Net",
    "wrist_blaster": "Energy Gauntlet"
  },
  "scrapper_defense.super_reflexes": {
    "practiced_brawler": "Practiced Brawler"
  },
  "stalker_defense.super_reflexes": {
    "practiced_brawler": "Practiced Brawler"
  },
  "stalker_melee.ice_melee": {
    "assassins_strike": "Assassins_Icicle"
  },
  "tanker_defense.super_reflexes": {
    "practiced_brawler": "Practiced Brawler"
  },
  "temporary_powers.temporary_powers": {
    "incarnate_reward_genesis": "Incarnate_Reward_Dark_Astoria"
  },
  "warshade_defensive.umbral_aura": {
    "shadow_slip": "Starless_Gateway"
  }
};
