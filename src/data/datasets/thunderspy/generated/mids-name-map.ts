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
