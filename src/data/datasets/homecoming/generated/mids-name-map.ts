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
 * Source: Mids Reborn homecoming database 2026.5.1337 (sha256 7568ff37f3a6…)
 * Powersets paired with the export: 3574 of 3653. Remapped names: 83.
 * Mids powersets with no counterpart here: 79 — listed by the generator on stderr.
 *
 * Regenerate: node scripts/convert-mids-name-map.cjs --dataset homecoming
 */

export const MIDS_NAME_MAP: Readonly<Record<string, Readonly<Record<string, string>>>> = {
  "blaster_ranged.seismic_blast": {
    "seismic_shockwaves": "Shockwaves"
  },
  "blaster_ranged.storm_blast": {
    "intensify": "Aim"
  },
  "blaster_support.electricity_manipulation": {
    "lightning_clap": "Lightning_Field",
    "lightning_field": "Lightning_Clap"
  },
  "blaster_support.sonic_manipulation": {
    "sound_booster": "Build_Up"
  },
  "blaster_support.tactical_arrow": {
    "gymnastics": "Quickness",
    "oil_slick_arrow": "Gymnastics"
  },
  "brute_defense.ice_armor": {
    "rime": "Rime_Ice"
  },
  "brute_defense.psionic_armor": {
    "psychokinetic_barrier": "Fortify_Mind"
  },
  "brute_defense.regeneration": {
    "reactive_regeneration": "Instant_Regeneration"
  },
  "brute_melee.sonic_melee": {
    "sound_booster": "Build_Up"
  },
  "controller_buff.marine_affinity": {
    "power_of_the_depths": "Call_Depths"
  },
  "controller_control.arsenal_control": {
    "tri_cannon": "Gun_Drone"
  },
  "controller_control.pyrotechnic_control": {
    "sparkling_chain": "Sparkling_Field"
  },
  "corruptor_buff.marine_affinity": {
    "power_of_the_depths": "Call_Depths"
  },
  "corruptor_ranged.seismic_blast": {
    "seismic_shockwaves": "Shockwaves"
  },
  "corruptor_ranged.storm_blast": {
    "intensify": "Aim"
  },
  "defender_buff.marine_affinity": {
    "power_of_the_depths": "Call_Depths"
  },
  "defender_buff.shock_therapy": {
    "defibrilate": "Defibrillate",
    "galvanic_sentinel": "Discharge"
  },
  "defender_ranged.seismic_blast": {
    "seismic_shockwaves": "Shockwaves"
  },
  "defender_ranged.storm_blast": {
    "intensify": "Aim"
  },
  "dominator_assault.arsenal_assault": {
    "elbow_strike": "Heavy_Blow"
  },
  "dominator_assault.savage_assault": {
    "unkindness": "Call_Ravens"
  },
  "dominator_control.arsenal_control": {
    "smoke_canister": "Smoke_Grenade",
    "tri_cannon": "Gun_Drone"
  },
  "dominator_control.illusion_control": {
    "phantom_army": "Decoy",
    "spectral_terrror": "Spectral_Terror",
    "superior_invisibility": "Invisibility"
  },
  "dominator_control.pyrotechnic_control": {
    "sparkling_chain": "Sparkling_Field"
  },
  "mastermind_buff.kinetics": {
    "kinetic_transfer": "Fulcrum_Shift"
  },
  "mastermind_buff.marine_affinity": {
    "power_of_the_depths": "Call_Depths"
  },
  "mastermind_buff.radiation_emission": {
    "emp_pulse": "EM_Pulse",
    "enervating__field": "Enervating_Field",
    "radiation_emission": "Radiant_Aura"
  },
  "mastermind_pets.protector_3": {
    "seeker drones": "Seeker_Drones"
  },
  "mission_maker_attacks.arsenal_assault": {
    "sniper_rifle_normal": "Sniper_Rifle"
  },
  "peacebringer_defensive.luminous_aura": {
    "quantum_maneuvers": "Quantum_Acceleration"
  },
  "pool.flight": {
    "evasive_maneuvers": "Afterburner"
  },
  "redirects.pyrotechnic_control": {
    "glitter_explosion": "GlitteringColumn_GlitterExplosion"
  },
  "redirects.storm_blast": {
    "storm_eye": "Nukenado_Skin"
  },
  "scrapper_defense.ice_armor": {
    "rime": "Rime_Ice"
  },
  "scrapper_defense.psionic_armor": {
    "psychokinetic_barrier": "Fortify_Mind"
  },
  "scrapper_defense.regeneration": {
    "reactive_regeneration": "Instant_Regeneration"
  },
  "scrapper_defense.stone_armor": {
    "minerals": "Mineral_Armor",
    "rock_armor": "Stone_Armor"
  },
  "scrapper_melee.sonic_melee": {
    "sound_booster": "Build_Up"
  },
  "scrapper_melee.stone_melee": {
    "taunt": "Confront"
  },
  "sentinel_defense.ice_armor": {
    "rime": "Rime_Ice"
  },
  "sentinel_defense.invulnerability": {
    "unyielding": "Unyeilding"
  },
  "sentinel_defense.psionic_armor": {
    "psychokinetic_barrier": "Fortify_Mind"
  },
  "sentinel_defense.stone_armor": {
    "minerals": "Mineral_Armor",
    "rock_armor": "Stone_Armor"
  },
  "sentinel_ranged.seismic_blast": {
    "seismic_shockwaves": "Shockwaves"
  },
  "sentinel_ranged.storm_blast": {
    "intensify": "Aim"
  },
  "set_bonus.pvp_set_bonus": {
    "increased_range_2": "Increased_Range_4",
    "increased_range_3": "Increased_Range_7"
  },
  "set_bonus.set_bonus": {
    "increased_range_2": "Increased_Range_4",
    "increased_range_3": "Increased_Range_7"
  },
  "stalker_defense.ice_armor": {
    "rime": "Rime_Ice"
  },
  "stalker_defense.psionic_armor": {
    "psychokinetic_barrier": "Fortify_Mind"
  },
  "stalker_defense.regeneration": {
    "reactive_regeneration": "Instant_Regeneration"
  },
  "stalker_defense.shield_defense": {
    "active_defense": "Battle_Agility",
    "battle_agility": "Deflection",
    "deflection": "Active_Defense"
  },
  "stalker_defense.stone_armor": {
    "minerals": "Mineral_Armor",
    "rock_armor": "Stone_Armor"
  },
  "stalker_defense.willpower": {
    "resurgence": "Reconstruction"
  },
  "stalker_melee.sonic_melee": {
    "assassins_whisper": "Assassins_Resonance"
  },
  "stalker_melee.stone_melee": {
    "assassins_smash": "Assassins_Rockslide",
    "seismic_mallet": "Heavy_Mallet"
  },
  "tanker_defense.energy_aura": {
    "conserve_power": "Energize",
    "power_armor": "Energy_Reserve"
  },
  "tanker_defense.ice_armor": {
    "rime": "Rime_Ice"
  },
  "tanker_defense.psionic_armor": {
    "psychokinetic_barrier": "Fortify_Mind"
  },
  "tanker_defense.regeneration": {
    "reactive_regeneration": "Instant_Healing"
  },
  "tanker_melee.sonic_melee": {
    "sound_booster": "Build_Up"
  },
  "teamwork.fortunata_teamwork": {
    "frt_fate_sealed": "Fate_Sealed"
  },
  "teamwork.widow_teamwork": {
    "nw_pain_tolerance": "Pain_Tolerance"
  },
  "temporary_powers.accolades": {
    "conqueror_of_the_labyrinth": "Labyrinth_Conqueror",
    "mark_and_recall": "MarkRecall",
    "sheer_willpower": "SFC_Accolade_Power"
  },
  "villain_pets.spirit_tree": {
    "spirit_tree": "Spirit_Tree_Taunt"
  }
};

/**
 * Mids' `group.powerset` → ours, for the pairs that spell the group differently.
 *
 * A reader that starts from the .mbd (the importer's retired-name check) resolves through
 * this; a reader that starts from our own powers (the matcher) already holds the map's key.
 */
export const MIDS_POWERSET_ALIAS: Readonly<Record<string, string>> = {
  "redirects.arsenal_assault": "mission_maker_attacks.arsenal_assault"
};
