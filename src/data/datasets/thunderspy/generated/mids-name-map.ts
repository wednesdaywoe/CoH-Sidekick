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
 * Source: Mids Reborn thunderspy database 2026.3.346 (sha256 1bbc83348369…)
 * Powersets paired with the export: 3441 of 3535. Remapped names: 54.
 * Reverse rows for the writer: 54.
 * Powerset paths for the writer: 0 — NONE. The thunderspy names dump predates MBDEXPORT-6 and carries folded powerset keys, so Mids' own spelling is not in it. Re-run emit_mids_names.py against that fork's I12.mhd to fill this in.
 * Mids powersets with no counterpart here: 94 — listed by the generator on stderr.
 *
 * Regenerate: node scripts/convert-mids-name-map.cjs --dataset thunderspy
 */

export const MIDS_NAME_MAP: Readonly<Record<string, Readonly<Record<string, string>>>> = {
  "blaster_ranged.assault_rifle": {
    "ignite": "Aim"
  },
  "brute_defense.super_reflexes": {
    "practiced brawler": "Practiced_Brawler"
  },
  "controller_buff.traps": {
    "auto_turret": "Time_Bomb"
  },
  "controller_control.darkness_control": {
    "spirit_host": "Shadowy_Binds"
  },
  "controller_control.gravity_control": {
    "gravity_field": "Crush"
  },
  "controller_control.water_control": {
    "tidal wave": "Tidal_Wave"
  },
  "corruptor_ranged.assault_rifle": {
    "ignite": "Aim"
  },
  "defender_ranged.assault_rifle": {
    "ignite": "Aim",
    "single_shot": "Beanbag"
  },
  "defender_ranged.battle_axe": {
    "aim": "Build_Up",
    "feint": "Taunt"
  },
  "defender_ranged.claws": {
    "feint": "Taunt"
  },
  "defender_ranged.dual_blades": {
    "feint": "Taunt"
  },
  "defender_ranged.katana": {
    "aim": "Build_Up",
    "feint": "Taunt"
  },
  "defender_ranged.kinetic_assault": {
    "disrupting _torrent": "Disrupting_Torrent",
    "velocity_siphon": "Speed_Siphon"
  },
  "defender_ranged.radiation_blast": {
    "fusion": "Aim"
  },
  "defender_ranged.savage_melee": {
    "feint": "Taunt"
  },
  "defender_ranged.staff_fighting": {
    "build_up": "Staff_Mastery",
    "feint": "Confront"
  },
  "defender_ranged.war_mace": {
    "aim": "Build_Up",
    "feint": "Taunt"
  },
  "dominator_assault.atomic_assault": {
    "neutron_bomb": "Electron_Haze",
    "proton_burst": "Proton_Volley"
  },
  "dominator_assault.kinetic_assault": {
    "disrupting _torrent": "Disrupting_Torrent"
  },
  "dominator_control.darkness_control": {
    "spirit_host": "Shadowy_Binds"
  },
  "dominator_control.electric_control": {
    "synaptic_surge": "Electric_Fence"
  },
  "dominator_control.water_control": {
    "tidal wave": "Tidal_Wave"
  },
  "inherent.inherent": {
    "tenacity": "Mez_Resistance"
  },
  "mastermind_buff.obedience_training": {
    "punish": "Backhand_Slap"
  },
  "mastermind_buff.radiation_emission": {
    "emp_pulse": "EM_Pulse",
    "enervating__field": "Enervating_Field",
    "radiation_emission": "Radiant_Aura"
  },
  "mastermind_buff.traps": {
    "auto_turret": "Trip_Mine"
  },
  "mastermind_pets.protector_3": {
    "seeker drones": "Seeker_Drones"
  },
  "mastermind_summon.knights": {
    "gash": "Beheader"
  },
  "pool.fighting": {
    "weapon_slam": "Slam",
    "weapon_strike": "Strike",
    "weapon_swing": "Swirl"
  },
  "pool.gadgetry": {
    "blaster_drone": "Wrist_Blaster",
    "drone_barrage": "Blaster_Barrage"
  },
  "pool.invisibility": {
    "intangibility": "Invisibility"
  },
  "pool.utility_belt": {
    "envenomed barrage": "Flying_Kick"
  },
  "scrapper_defense.super_reflexes": {
    "practiced brawler": "Practiced_Brawler"
  },
  "stalker_defense.invulnerability": {
    "resist_elements": "Hide",
    "resist_energies": "Resist_Forces"
  },
  "stalker_defense.spectral_aura": {
    "spectral_shift": "Apparitional_Avoidance"
  },
  "stalker_defense.super_reflexes": {
    "practiced brawler": "Practiced_Brawler"
  },
  "stalker_melee.spectral_melee": {
    "assassin's_reave": "Assassins_Reave"
  },
  "tanker_defense.ice_armor": {
    "icy_bastion": "Chilling_Embrace"
  },
  "tanker_defense.super_reflexes": {
    "practiced brawler": "Practiced_Brawler"
  },
  "tanker_melee.pale_blade": {
    "perdition": "Sunder_Bone",
    "virulent": "Build_Up"
  }
};

/**
 * Mids' `group.powerset` → ours, for the pairs that spell the group differently.
 *
 * A reader that starts from the .mbd (the importer's retired-name check) resolves through
 * this; a reader that starts from our own powers (the matcher) already holds the map's key.
 */
export const MIDS_POWERSET_ALIAS: Readonly<Record<string, string>> = {};

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
  "blaster_ranged.assault_rifle": {
    "aim": "Ignite"
  },
  "brute_defense.super_reflexes": {
    "practiced_brawler": "Practiced Brawler"
  },
  "controller_buff.traps": {
    "time_bomb": "Auto_Turret"
  },
  "controller_control.darkness_control": {
    "shadowy_binds": "Spirit_Host"
  },
  "controller_control.gravity_control": {
    "crush": "Gravity_Field"
  },
  "controller_control.water_control": {
    "tidal_wave": "Tidal Wave"
  },
  "corruptor_ranged.assault_rifle": {
    "aim": "Ignite"
  },
  "defender_ranged.assault_rifle": {
    "aim": "Ignite",
    "beanbag": "Single_Shot"
  },
  "defender_ranged.battle_axe": {
    "build_up": "Aim",
    "taunt": "Feint"
  },
  "defender_ranged.claws": {
    "taunt": "Feint"
  },
  "defender_ranged.dual_blades": {
    "taunt": "Feint"
  },
  "defender_ranged.katana": {
    "build_up": "Aim",
    "taunt": "Feint"
  },
  "defender_ranged.kinetic_assault": {
    "disrupting_torrent": "Disrupting _Torrent",
    "speed_siphon": "Velocity_Siphon"
  },
  "defender_ranged.radiation_blast": {
    "aim": "Fusion"
  },
  "defender_ranged.savage_melee": {
    "taunt": "Feint"
  },
  "defender_ranged.staff_fighting": {
    "confront": "Feint",
    "staff_mastery": "Build_Up"
  },
  "defender_ranged.war_mace": {
    "build_up": "Aim",
    "taunt": "Feint"
  },
  "dominator_assault.atomic_assault": {
    "electron_haze": "Neutron_Bomb",
    "proton_volley": "Proton_Burst"
  },
  "dominator_assault.kinetic_assault": {
    "disrupting_torrent": "Disrupting _Torrent"
  },
  "dominator_control.darkness_control": {
    "shadowy_binds": "Spirit_Host"
  },
  "dominator_control.electric_control": {
    "electric_fence": "Synaptic_Surge"
  },
  "dominator_control.water_control": {
    "tidal_wave": "Tidal Wave"
  },
  "inherent.inherent": {
    "mez_resistance": "Tenacity"
  },
  "mastermind_buff.obedience_training": {
    "backhand_slap": "Punish"
  },
  "mastermind_buff.radiation_emission": {
    "em_pulse": "EMP_Pulse",
    "enervating_field": "Enervating__Field",
    "radiant_aura": "Radiation_Emission"
  },
  "mastermind_buff.traps": {
    "trip_mine": "Auto_Turret"
  },
  "mastermind_pets.protector_3": {
    "seeker_drones": "Seeker Drones"
  },
  "mastermind_summon.knights": {
    "beheader": "Gash"
  },
  "pool.fighting": {
    "slam": "Weapon_Slam",
    "strike": "Weapon_Strike",
    "swirl": "Weapon_Swing"
  },
  "pool.gadgetry": {
    "blaster_barrage": "Drone_Barrage",
    "wrist_blaster": "Blaster_Drone"
  },
  "pool.invisibility": {
    "invisibility": "Intangibility"
  },
  "pool.utility_belt": {
    "flying_kick": "Envenomed Barrage"
  },
  "scrapper_defense.super_reflexes": {
    "practiced_brawler": "Practiced Brawler"
  },
  "stalker_defense.invulnerability": {
    "hide": "Resist_Elements",
    "resist_forces": "Resist_Energies"
  },
  "stalker_defense.spectral_aura": {
    "apparitional_avoidance": "Spectral_Shift"
  },
  "stalker_defense.super_reflexes": {
    "practiced_brawler": "Practiced Brawler"
  },
  "stalker_melee.spectral_melee": {
    "assassins_reave": "Assassin's_Reave"
  },
  "tanker_defense.ice_armor": {
    "chilling_embrace": "Icy_Bastion"
  },
  "tanker_defense.super_reflexes": {
    "practiced_brawler": "Practiced Brawler"
  },
  "tanker_melee.pale_blade": {
    "build_up": "Virulent",
    "sunder_bone": "Perdition"
  }
};

/**
 * OUR `group.powerset` (lower-cased) → Mids' own spelling of it, for the .mbd writer
 * (DATA-GAP MBDEXPORT-6).
 *
 * The first two segments of a `PowerName`, read out of Mids' database rather than
 * composed. The writer used to build them from an archetype table and the powerset's ICON
 * filename, and neither is a read of what Mids calls the set: a Rebirth Guardian went out
 * as `Guardian_Comp.Electric_Armor` where Mids holds
 * `Guardian_Composition.Atmospheric_Composition`, with all nine power names already
 * right inside it.
 *
 * Case is load-bearing here for the same reason it is in `MIDS_NAME_REVERSE`, and it is
 * not reconstructible: `Epic.VEAT_Mace_Mastery`, `Pool.Force_of_Will` and
 * `Epic.Dark_Mastery_TankBrute` are none of them what title-casing produces.
 *
 * A set absent from this table is one this pairing could not reach. The writer reports it
 * rather than composing a path, because Mids answers a `group.set` it cannot resolve
 * with a blank row that still holds the power's slots.
 */
export const MIDS_POWERSET_PATH: Readonly<Record<string, string>> = {};
