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
 * Source: mids-power-names.thunderspy.json, version 2026.3.346 (sha256 1bbc83348369…) — the I12 from a
 * third-party THUNDERSPY database drop in `/Thunderspy/`: Mids' Generic database with the
 * powers DB, the two level tables and SData swapped out and that fork's icons added. Not from
 * Mids Reborn's own releases, which carry Generic, Homecoming and Rebirth; who built it is
 * unrecorded. 286 of our 305 Thunderspy powersets are in it, against 272 of Homecoming's 364.
 * DATA-GAP MBDEXPORT-2.
 * Powersets paired with the export: 3453 of 3535. Remapped names: 86.
 * Reverse rows for the writer: 86, plus 0 the display join could only reach with its separators stripped.
 * Powerset paths for the writer: 0 — NONE. The thunderspy names dump predates MBDEXPORT-6 and carries folded powerset keys, so Mids' own spelling is not in it. Re-run emit_mids_names.py against that fork's I12.mhd to fill this in.
 * Mids powersets with no counterpart here: 82 — listed by the generator on stderr.
 *
 * Regenerate: node scripts/convert-mids-name-map.cjs --dataset thunderspy
 */

export const MIDS_NAME_MAP: Readonly<Record<string, Readonly<Record<string, string>>>> = {
  "blaster_ranged.assault_rifle": {
    "ignite": "Aim"
  },
  "blaster_support.radiation_manipulation": {
    "fission": "Fusional_Build_Up",
    "half-life": "Nuclear_Mutation",
    "irradiated_ground": "Choking_Cloud",
    "neutron_burst": "Fallout",
    "particle_acceleration": "Metabolic_Aura"
  },
  "blaster_support.time_manipulation": {
    "be_gone": "Future_Pain",
    "chronos": "Chronological_Selection",
    "dangerous_acceleration": "Aging_Touch",
    "stable_time_loop": "Time_Lord",
    "temporal_ablation": "Temporal_Healing"
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
  "defender_ranged.brawling": {
    "feint": "Taunt"
  },
  "defender_ranged.broad_sword": {
    "aim": "Build_Up",
    "feint": "Taunt"
  },
  "defender_ranged.claws": {
    "feint": "Taunt"
  },
  "defender_ranged.dual_blades": {
    "feint": "Taunt"
  },
  "defender_ranged.earth_assault": {
    "aim": "Power_Boost"
  },
  "defender_ranged.katana": {
    "aim": "Build_Up",
    "feint": "Taunt"
  },
  "defender_ranged.kinetic_assault": {
    "disrupting _torrent": "Disrupting_Torrent",
    "velocity_siphon": "Speed_Siphon"
  },
  "defender_ranged.martial_assault": {
    "eagles_claw": "Spinning_Kick"
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
  "dominator_assault.telekinetic_assault": {
    "psi-blade_slam": "PsiBlade_Slam",
    "psi-blade_slash": "PsiBlade_Slash",
    "psi-blade_spin": "PsiBlade_Spin",
    "psi-whip_coil": "PsiWhip_Coil",
    "psi-whip_crack": "PsiWhip_Crack",
    "psi-whip_lash": "PsiWhip_Lash",
    "psi-whip_thrash": "PsiWhip_Thrash",
    "psychokinetic_pulse": "Telekinetic_Pulse"
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
  "epic.dominator_atomic_mastery": {
    "electron_haze": "Neutron_Bomb"
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
  "tanker_defense.sacred_armor": {
    "centered": "Geomancy_Root_Bonus",
    "in_touch": "Guiding_Light_Root_Bonus",
    "radiating_light": "Guiding_Light"
  },
  "tanker_defense.super_reflexes": {
    "practiced brawler": "Practiced_Brawler"
  },
  "tanker_melee.hobo_melee": {
    "buildup": "Hard_Life",
    "cryoshot": "Birdshot",
    "dragon's_breath": "Dragon_Breath",
    "frag_12": "Grenade",
    "venom_shell": "Snakeshot"
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
export const MIDS_POWERSET_ALIAS: Readonly<Record<string, string>> = {
  "blaster_support.atomic_manipulation": "blaster_support.radiation_manipulation",
  "blaster_support.temporal_manipulation": "blaster_support.time_manipulation",
  "defender_ranged.broadsword": "defender_ranged.broad_sword",
  "defender_ranged.earth_combat": "defender_ranged.earth_assault",
  "defender_ranged.martial_combat": "defender_ranged.martial_assault",
  "defender_ranged.street_justice": "defender_ranged.brawling",
  "dominator_assault.psychokinetic_assault": "dominator_assault.telekinetic_assault",
  "epic.atomic_mastery": "epic.dominator_atomic_mastery",
  "tanker_defense.nature_armor": "tanker_defense.sacred_armor",
  "tanker_melee.hard_life": "tanker_melee.hobo_melee"
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
  "blaster_ranged.assault_rifle": {
    "aim": "Ignite"
  },
  "blaster_support.radiation_manipulation": {
    "choking_cloud": "Irradiated_Ground",
    "fallout": "Neutron_Burst",
    "fusional_build_up": "Fission",
    "metabolic_aura": "Particle_Acceleration",
    "nuclear_mutation": "Half-Life"
  },
  "blaster_support.time_manipulation": {
    "aging_touch": "Dangerous_Acceleration",
    "chronological_selection": "Chronos",
    "future_pain": "Be_Gone",
    "temporal_healing": "Temporal_Ablation",
    "time_lord": "Stable_Time_Loop"
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
  "defender_ranged.brawling": {
    "taunt": "Feint"
  },
  "defender_ranged.broad_sword": {
    "build_up": "Aim",
    "taunt": "Feint"
  },
  "defender_ranged.claws": {
    "taunt": "Feint"
  },
  "defender_ranged.dual_blades": {
    "taunt": "Feint"
  },
  "defender_ranged.earth_assault": {
    "power_boost": "Aim"
  },
  "defender_ranged.katana": {
    "build_up": "Aim",
    "taunt": "Feint"
  },
  "defender_ranged.kinetic_assault": {
    "disrupting_torrent": "Disrupting _Torrent",
    "speed_siphon": "Velocity_Siphon"
  },
  "defender_ranged.martial_assault": {
    "spinning_kick": "Eagles_Claw"
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
  "dominator_assault.telekinetic_assault": {
    "psiblade_slam": "Psi-Blade_Slam",
    "psiblade_slash": "Psi-Blade_Slash",
    "psiblade_spin": "Psi-Blade_Spin",
    "psiwhip_coil": "Psi-Whip_Coil",
    "psiwhip_crack": "Psi-Whip_Crack",
    "psiwhip_lash": "Psi-Whip_Lash",
    "psiwhip_thrash": "Psi-Whip_Thrash",
    "telekinetic_pulse": "Psychokinetic_Pulse"
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
  "epic.dominator_atomic_mastery": {
    "neutron_bomb": "Electron_Haze"
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
  "tanker_defense.sacred_armor": {
    "geomancy_root_bonus": "Centered",
    "guiding_light": "Radiating_Light",
    "guiding_light_root_bonus": "In_Touch"
  },
  "tanker_defense.super_reflexes": {
    "practiced_brawler": "Practiced Brawler"
  },
  "tanker_melee.hobo_melee": {
    "birdshot": "Cryoshot",
    "dragon_breath": "Dragon's_Breath",
    "grenade": "Frag_12",
    "hard_life": "Buildup",
    "snakeshot": "Venom_Shell"
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

/**
 * Reverse rows the display join could only reach with every separator stripped — for the
 * .mbd writer, and for it alone (DATA-GAP MBDEXPORT-8).
 *
 * Rebirth spells a power `Moonbeam` and Mids spells it `Moon_Beam`. The join above
 * folds separator RUNS to one space and stops, so that pair is a miss, and that tightness
 * is right where it is: the IMPORT matcher resolves such a pair on its own
 * all-separators-stripped ladder, and a forward row for a pair it already handles is a row
 * that is not a rotation — a chance to bind the wrong power for no gain.
 *
 * The writer has no ladder. One lookup, and ours goes out on a miss, under a name Mids
 * answers with a blank row that keeps the slots. So the width the reader needs and the
 * width the writer needs are different, and this is the writer's.
 *
 * A separate table rather than extra rows in `MIDS_NAME_REVERSE` for two reasons: that
 * one is exactly the inverse of `MIDS_NAME_MAP` and a gate holds it to that, and these
 * rows come from a looser join, which is a fact about them a reader should not have to
 * infer. Ours-already-answered is never overruled — a key here is one the tight pass left
 * empty.
 */
export const MIDS_NAME_REVERSE_LOOSE: Readonly<Record<string, Readonly<Record<string, string>>>> = {};
