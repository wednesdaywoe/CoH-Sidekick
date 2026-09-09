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
 * Source: Mids Reborn homecoming database 2026.5.1337 (sha256 7568ff37f3a6…) — Mids ships no brainstorm build, so a brainstorm .mbd carries homecoming's namespace
 * Powersets paired with the export: 3561 of 3653. Remapped names: 83.
 * Reverse rows for the writer: 83.
 * Mids powersets with no counterpart here: 92 — listed by the generator on stderr.
 *
 * Regenerate: node scripts/convert-mids-name-map.cjs --dataset brainstorm
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
  "blaster_ranged.seismic_blast": {
    "shockwaves": "Seismic_Shockwaves"
  },
  "blaster_ranged.storm_blast": {
    "aim": "Intensify"
  },
  "blaster_support.electricity_manipulation": {
    "lightning_clap": "Lightning_Field",
    "lightning_field": "Lightning_Clap"
  },
  "blaster_support.sonic_manipulation": {
    "build_up": "Sound_Booster"
  },
  "blaster_support.tactical_arrow": {
    "gymnastics": "Oil_Slick_Arrow",
    "quickness": "Gymnastics"
  },
  "brute_defense.ice_armor": {
    "rime_ice": "Rime"
  },
  "brute_defense.psionic_armor": {
    "fortify_mind": "Psychokinetic_Barrier"
  },
  "brute_defense.regeneration": {
    "instant_regeneration": "Reactive_Regeneration"
  },
  "brute_melee.sonic_melee": {
    "build_up": "Sound_Booster"
  },
  "controller_buff.marine_affinity": {
    "call_depths": "Power_of_the_Depths"
  },
  "controller_control.arsenal_control": {
    "gun_drone": "Tri_Cannon"
  },
  "controller_control.pyrotechnic_control": {
    "sparkling_field": "Sparkling_Chain"
  },
  "corruptor_buff.marine_affinity": {
    "call_depths": "Power_of_the_Depths"
  },
  "corruptor_ranged.seismic_blast": {
    "shockwaves": "Seismic_Shockwaves"
  },
  "corruptor_ranged.storm_blast": {
    "aim": "Intensify"
  },
  "defender_buff.marine_affinity": {
    "call_depths": "Power_of_the_Depths"
  },
  "defender_buff.shock_therapy": {
    "defibrillate": "Defibrilate",
    "discharge": "Galvanic_Sentinel"
  },
  "defender_ranged.seismic_blast": {
    "shockwaves": "Seismic_Shockwaves"
  },
  "defender_ranged.storm_blast": {
    "aim": "Intensify"
  },
  "dominator_assault.arsenal_assault": {
    "heavy_blow": "Elbow_Strike"
  },
  "dominator_assault.savage_assault": {
    "call_ravens": "Unkindness"
  },
  "dominator_control.arsenal_control": {
    "gun_drone": "Tri_Cannon",
    "smoke_grenade": "Smoke_Canister"
  },
  "dominator_control.illusion_control": {
    "decoy": "Phantom_Army",
    "invisibility": "Superior_Invisibility",
    "spectral_terror": "Spectral_Terrror"
  },
  "dominator_control.pyrotechnic_control": {
    "sparkling_field": "Sparkling_Chain"
  },
  "mastermind_buff.kinetics": {
    "fulcrum_shift": "Kinetic_Transfer"
  },
  "mastermind_buff.marine_affinity": {
    "call_depths": "Power_of_the_Depths"
  },
  "mastermind_buff.radiation_emission": {
    "em_pulse": "EMP_Pulse",
    "enervating_field": "Enervating__Field",
    "radiant_aura": "Radiation_Emission"
  },
  "mastermind_pets.protector_3": {
    "seeker_drones": "Seeker Drones"
  },
  "mission_maker_attacks.arsenal_assault": {
    "sniper_rifle": "Sniper_Rifle_Normal"
  },
  "peacebringer_defensive.luminous_aura": {
    "quantum_acceleration": "Quantum_Maneuvers"
  },
  "pool.flight": {
    "afterburner": "Evasive_Maneuvers"
  },
  "redirects.pyrotechnic_control": {
    "glitteringcolumn_glitterexplosion": "Glitter_Explosion"
  },
  "redirects.storm_blast": {
    "nukenado_skin": "Storm_Eye"
  },
  "scrapper_defense.ice_armor": {
    "rime_ice": "Rime"
  },
  "scrapper_defense.psionic_armor": {
    "fortify_mind": "Psychokinetic_Barrier"
  },
  "scrapper_defense.regeneration": {
    "instant_regeneration": "Reactive_Regeneration"
  },
  "scrapper_defense.stone_armor": {
    "mineral_armor": "Minerals",
    "stone_armor": "Rock_Armor"
  },
  "scrapper_melee.sonic_melee": {
    "build_up": "Sound_Booster"
  },
  "scrapper_melee.stone_melee": {
    "confront": "Taunt"
  },
  "sentinel_defense.ice_armor": {
    "rime_ice": "Rime"
  },
  "sentinel_defense.invulnerability": {
    "unyeilding": "Unyielding"
  },
  "sentinel_defense.psionic_armor": {
    "fortify_mind": "Psychokinetic_Barrier"
  },
  "sentinel_defense.stone_armor": {
    "mineral_armor": "Minerals",
    "stone_armor": "Rock_Armor"
  },
  "sentinel_ranged.seismic_blast": {
    "shockwaves": "Seismic_Shockwaves"
  },
  "sentinel_ranged.storm_blast": {
    "aim": "Intensify"
  },
  "set_bonus.pvp_set_bonus": {
    "increased_range_4": "Increased_Range_2",
    "increased_range_7": "Increased_Range_3"
  },
  "set_bonus.set_bonus": {
    "increased_range_4": "Increased_Range_2",
    "increased_range_7": "Increased_Range_3"
  },
  "stalker_defense.ice_armor": {
    "rime_ice": "Rime"
  },
  "stalker_defense.psionic_armor": {
    "fortify_mind": "Psychokinetic_Barrier"
  },
  "stalker_defense.regeneration": {
    "instant_regeneration": "Reactive_Regeneration"
  },
  "stalker_defense.shield_defense": {
    "active_defense": "Deflection",
    "battle_agility": "Active_Defense",
    "deflection": "Battle_Agility"
  },
  "stalker_defense.stone_armor": {
    "mineral_armor": "Minerals",
    "stone_armor": "Rock_Armor"
  },
  "stalker_defense.willpower": {
    "reconstruction": "Resurgence"
  },
  "stalker_melee.sonic_melee": {
    "assassins_resonance": "Assassins_Whisper"
  },
  "stalker_melee.stone_melee": {
    "assassins_rockslide": "Assassins_Smash",
    "heavy_mallet": "Seismic_Mallet"
  },
  "tanker_defense.energy_aura": {
    "energize": "Conserve_Power",
    "energy_reserve": "Power_Armor"
  },
  "tanker_defense.ice_armor": {
    "rime_ice": "Rime"
  },
  "tanker_defense.psionic_armor": {
    "fortify_mind": "Psychokinetic_Barrier"
  },
  "tanker_defense.regeneration": {
    "instant_healing": "Reactive_Regeneration"
  },
  "tanker_melee.sonic_melee": {
    "build_up": "Sound_Booster"
  },
  "teamwork.fortunata_teamwork": {
    "fate_sealed": "FRT_Fate_Sealed"
  },
  "teamwork.widow_teamwork": {
    "pain_tolerance": "NW_Pain_Tolerance"
  },
  "temporary_powers.accolades": {
    "labyrinth_conqueror": "Conqueror_of_the_Labyrinth",
    "markrecall": "Mark_and_Recall",
    "sfc_accolade_power": "Sheer_Willpower"
  },
  "villain_pets.spirit_tree": {
    "spirit_tree_taunt": "Spirit_Tree"
  }
};
