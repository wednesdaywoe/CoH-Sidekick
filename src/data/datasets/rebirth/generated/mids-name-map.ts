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
