/**
 * Archetype definitions
 * Migrated from legacy/js/data/archetypes.js
 */

import type { Archetype, ArchetypeId, ArchetypeRegistry } from '@/types';
import { ARCHETYPE_BINARY_STATS } from './generated/archetype-stats.generated';

// Per-level HP curves, HP caps, baseHP/maxHP, resistance cap (Phase 1),
// baseThreat (Phase 2 header scalar) and damageCap (Phase 3 StrengthMax L50)
// are now binary-sourced: `ARCHETYPE_BINARY_STATS` is generated from classes.bin
// (via export_classes.py -> convert-archetypes.cjs) and spread into each
// archetype's `stats` below. Sourcing damageCap caught the hand-port under-
// capping Scrapper/Tanker/Sentinel/Corruptor/Stalker at 400% (they are 500%).
// The remaining scalars (damageModifier, buffDebuffModifier, baseEndurance,
// baseRecovery, defenseCap) stay hand-curated. NOTE: damageModifier and
// buffDebuffModifier are a LIVE table-less fallback, NOT vestigial — `damage.ts`
// reads buffDebuffModifier (`calculateBuffDebuffPercent`) and damageModifier
// (`getBaseDamage`) for effects with no {scale, table} pair. That is now the only
// reader: the display surfaces used to apply an archetype-name-derived variant of
// the same modifier, which PROD6B-2b measured dead and deleted. Most effects DO
// carry a {scale, table} pair and use the binary per-category named_tables
// (at-tables.ts), so the fallback fires only for table-less effects — but it
// does fire, so keep these maintained. They aren't single binary quantities
// (each abstracts many per-category tables), so they can't be cleanly binary-
// sourced, and some may be stale. Tanker's was corrected as the one cleanly
// confirmable case; the others are left as-is. See ARCHETYPE-DEFS-BINARY-SOURCING.md.

export const ARCHETYPES: ArchetypeRegistry = {
  // ============================================
  // HERO ARCHETYPES
  // ============================================

  blaster: {
    name: 'Blaster',
    side: 'hero',
    description: 'Ranged damage specialist with high offensive power but low defenses',
    inherent: {
      name: 'Defiance',
      description: 'Attacking grants stacking damage bonus. First two Primary and first Secondary power usable while mezzed.',
      icon: 'inherent_blasterdesperation.png',
    },
    stats: {
      ...ARCHETYPE_BINARY_STATS['blaster'],
      baseEndurance: 100,
      baseRecovery: 1.67,
      damageModifier: {
        melee: 0.5,
        ranged: 1.125,
        aoe: 1.0,
      },
      buffDebuffModifier: 0.625,
      defenseCap: 0.45,     // 45%
    },
    primarySets: [
      'blaster/archery',
      'blaster/assault-rifle',
      'blaster/beam-rifle',
      'blaster/dark-blast',
      'blaster/dual-pistols',
      'blaster/electrical-blast',
      'blaster/energy-blast',
      'blaster/fire-blast',
      'blaster/ice-blast',
      'blaster/psychic-blast',
      'blaster/radiation-blast',
      'blaster/seismic-blast',
      'blaster/sonic-attack',
      'blaster/storm-blast',
      'blaster/water-blast',
    ],
    secondarySets: [
      'blaster/darkness-manipulation',
      'blaster/earth-manipulation',
      'blaster/electricity-manipulation',
      'blaster/energy-manipulation',
      'blaster/fire-manipulation',
      'blaster/devices',
      'blaster/ice-manipulation',
      'blaster/martial-combat',
      'blaster/mental-manipulation',
      'blaster/ninja-training',
      'blaster/plant-manipulation',
      'blaster/atomic-manipulation',
      'blaster/sonic-manipulation',
      'blaster/tactical-arrow',
      'blaster/temporal-manipulation',
    ],
  },

  controller: {
    name: 'Controller',
    side: 'hero',
    description: 'Mezzes enemies and buffs/debuffs with strong team support',
    inherent: {
      name: 'Containment',
      description: 'Double damage vs Held, Immobilized, Slept, or Disoriented targets. Applied after enhancements.',
      icon: 'inherent_blasterdesperation.png',
    },
    stats: {
      ...ARCHETYPE_BINARY_STATS['controller'],
      baseEndurance: 100,
      baseRecovery: 1.67,
      damageModifier: {
        melee: 0.55,
        ranged: 0.55,
        aoe: 0.5,
      },
      buffDebuffModifier: 1.0,
      defenseCap: 0.45,     // 45%
    },
    primarySets: [
      'controller/arsenal-control',
      'controller/darkness-control',
      'controller/earth-control',
      'controller/electric-control',
      'controller/fire-control',
      'controller/gravity-control',
      'controller/ice-control',
      'controller/illusion-control',
      'controller/mind-control',
      'controller/plant-control',
      'controller/pyrotechnic-control',
      'controller/symphony-control',
      'controller/wind-control',
    ],
    secondarySets: [
      'controller/cold-domination',
      'controller/darkness-affinity',
      'controller/empathy',
      'controller/force-field',
      'controller/kinetics',
      'controller/marine-affinity',
      'controller/nature-affinity',
      'controller/pain-domination',
      'controller/poison',
      'controller/radiation-emission',
      'controller/electrical-affinity',
      'controller/sonic-resonance',
      'controller/storm-summoning',
      'controller/thermal-radiation',
      'controller/time-manipulation',
      'controller/traps',
      'controller/trick-arrow',
    ],
  },

  defender: {
    name: 'Defender',
    side: 'hero',
    description: 'Support specialist with powerful buffs, debuffs, and healing',
    inherent: {
      name: 'Vigilance',
      description: 'Solo/small teams: +6-30% damage (scales with level). Endurance discount when teammates are injured. 3+ teammates = no damage bonus.',
      icon: 'inherentpeacebringer_interspaciallink.png',
    },
    stats: {
      ...ARCHETYPE_BINARY_STATS['defender'],
      baseEndurance: 100,
      baseRecovery: 1.67,
      damageModifier: {
        melee: 0.55,
        ranged: 0.65,
        aoe: 0.5,
      },
      buffDebuffModifier: 1.25,
      defenseCap: 0.45,     // 45%
    },
    primarySets: [
      'defender/cold-domination',
      'defender/dark-miasma',
      'defender/empathy',
      'defender/force-field',
      'defender/kinetics',
      'defender/marine-affinity',
      'defender/nature-affinity',
      'defender/pain-domination',
      'defender/poison',
      'defender/radiation-emission',
      'defender/electrical-affinity',
      'defender/sonic-resonance',
      'defender/storm-summoning',
      'defender/thermal-radiation',
      'defender/time-manipulation',
      'defender/traps',
      'defender/trick-arrow',
    ],
    secondarySets: [
      'defender/archery',
      'defender/assault-rifle',
      'defender/beam-rifle',
      'defender/dark-blast',
      'defender/dual-pistols',
      'defender/electrical-blast',
      'defender/energy-blast',
      'defender/fire-blast',
      'defender/ice-blast',
      'defender/psychic-blast',
      'defender/radiation-blast',
      'defender/seismic-blast',
      'defender/sonic-attack',
      'defender/storm-blast',
      'defender/water-blast',
    ],
  },

  scrapper: {
    name: 'Scrapper',
    side: 'hero',
    description: 'Melee damage dealer with good survivability through defense/resistance',
    inherent: {
      name: 'Critical Hit',
      description: '5% crit chance vs minions (double damage), 10% vs lieutenants/bosses. Average +5-10% damage bonus.',
      icon: 'inherent_blasterdesperation.png',
    },
    stats: {
      ...ARCHETYPE_BINARY_STATS['scrapper'],
      baseEndurance: 100,
      baseRecovery: 1.67,
      damageModifier: {
        melee: 1.125,
        ranged: 0.5,
        aoe: 0.8,
      },
      buffDebuffModifier: 1.0,
      defenseCap: 0.45,     // 45%
    },
    primarySets: [
      'scrapper/battle-axe',
      'scrapper/sonic-melee',
      'scrapper/street-justice',
      'scrapper/broad-sword',
      'scrapper/claws',
      'scrapper/dark-melee',
      'scrapper/dual-blades',
      'scrapper/electrical-melee',
      'scrapper/energy-melee',
      'scrapper/fiery-melee',
      'scrapper/ice-melee',
      'scrapper/katana',
      'scrapper/kinetic-melee',
      'scrapper/martial-arts',
      'scrapper/psionic-melee',
      'scrapper/spines',
      'scrapper/radiation-melee',
      'scrapper/savage-melee',
      'scrapper/staff-fighting',
      'scrapper/stone-melee',
      'scrapper/titan-weapons',
      'scrapper/war-mace',
    ],
    secondarySets: [
      'scrapper/bio-armor',
      'scrapper/dark-armor',
      'scrapper/electric-armor',
      'scrapper/energy-aura',
      'scrapper/fiery-aura',
      'scrapper/ice-armor',
      'scrapper/invulnerability',
      'scrapper/ninjitsu',
      'scrapper/psionic-armor',
      'scrapper/radiation-armor',
      'scrapper/regeneration',
      'scrapper/shield-defense',
      'scrapper/stone-armor',
      'scrapper/super-reflexes',
      'scrapper/willpower',
    ],
  },

  tanker: {
    name: 'Tanker',
    side: 'hero',
    description: 'Extremely tough with highest HP and strong defensive powers',
    inherent: {
      name: 'Gauntlet',
      description: 'PunchVoke: ST attacks taunt target + 4 nearby, AoE taunts all. +50% AoE radius/range, +50% cone arc. PBAoE hits bonus targets at 33% damage.',
      icon: 'inherent_blasterdesperation.png',
    },
    stats: {
      ...ARCHETYPE_BINARY_STATS['tanker'],
      baseEndurance: 100,
      baseRecovery: 1.67,
      // Corrected from the pre-2020 0.8/0.5 to the HC values (2020-01-23 Tanker
      // rework: melee 0.8->0.95, ranged 0.5->0.8); confirmed by the binary
      // Melee/Ranged_Damage tables (52.831/55.61=0.95, 44.489/55.61=0.80). aoe
      // has no binary table to confirm, left as-is. (Rebirth keeps 0.8/0.5.)
      damageModifier: {
        melee: 0.95,
        ranged: 0.8,
        aoe: 0.7,
      },
      buffDebuffModifier: 1.0,
      defenseCap: 0.45,     // 45%
    },
    primarySets: [
      'tanker/bio-armor',
      'tanker/dark-armor',
      'tanker/electric-armor',
      'tanker/energy-aura',
      'tanker/fiery-aura',
      'tanker/ice-armor',
      'tanker/invulnerability',
      'tanker/psionic-armor',
      'tanker/radiation-armor',
      'tanker/regeneration',
      'tanker/shield-defense',
      'tanker/stone-armor',
      'tanker/super-reflexes',
      'tanker/willpower',
    ],
    secondarySets: [
      'tanker/battle-axe',
      'tanker/sonic-melee',
      'tanker/street-justice',
      'tanker/broad-sword',
      'tanker/claws',
      'tanker/dark-melee',
      'tanker/dual-blades',
      'tanker/electrical-melee',
      'tanker/energy-melee',
      'tanker/fiery-melee',
      'tanker/ice-melee',
      'tanker/katana',
      'tanker/kinetic-melee',
      'tanker/martial-arts',
      'tanker/psionic-melee',
      'tanker/radiation-melee',
      'tanker/savage-melee',
      'tanker/spines',
      'tanker/staff-fighting',
      'tanker/stone-melee',
      'tanker/super-strength',
      'tanker/titan-weapons',
      'tanker/war-mace',
    ],
  },

  sentinel: {
    name: 'Sentinel',
    side: 'hero',
    description: 'Homecoming exclusive: Ranged damage with built-in armor for survivability',
    inherent: {
      name: 'Opportunity',
      description: 'Build meter by attacking in combat. Grants scaling critical hit chance on primary attacks (5% at empty, 40% at full meter). Crits deal 40% bonus damage. Vulnerability applies -15% Res, -11.25% Def, -150ft Stealth to targets.',
      icon: 'inherent_targetlock.png',
    },
    stats: {
      ...ARCHETYPE_BINARY_STATS['sentinel'],
      baseEndurance: 100,
      baseRecovery: 1.67,
      damageModifier: {
        melee: 0.65,
        ranged: 0.95,
        aoe: 0.8,
      },
      buffDebuffModifier: 1.4,
      defenseCap: 0.45,     // 45%
    },
    primarySets: [
      'sentinel/archery',
      'sentinel/assault-rifle',
      'sentinel/beam-rifle',
      'sentinel/dark-blast',
      'sentinel/dual-pistols',
      'sentinel/electrical-blast',
      'sentinel/energy-blast',
      'sentinel/fire-blast',
      'sentinel/ice-blast',
      'sentinel/psychic-blast',
      'sentinel/radiation-blast',
      'sentinel/seismic-blast',
      'sentinel/sonic-attack',
      'sentinel/storm-blast',
      'sentinel/water-blast',
    ],
    secondarySets: [
      'sentinel/bio-armor',
      'sentinel/dark-armor',
      'sentinel/electric-armor',
      'sentinel/energy-aura',
      'sentinel/fiery-aura',
      'sentinel/ice-armor',
      'sentinel/invulnerability',
      'sentinel/ninjitsu',
      'sentinel/psionic-armor',
      'sentinel/radiation-armor',
      'sentinel/regeneration',
      'sentinel/stone-armor',
      'sentinel/super-reflexes',
      'sentinel/willpower',
    ],
  },

  // ============================================
  // VILLAIN ARCHETYPES
  // ============================================

  brute: {
    name: 'Brute',
    side: 'villain',
    description: 'High damage melee fighter that builds fury through combat',
    inherent: {
      name: 'Fury',
      description: 'Build fury (0-100) by attacking and being attacked. Each fury point grants +2% damage, up to +200% at max fury.',
      icon: 'inherent_blasterdesperation.png',
    },
    stats: {
      ...ARCHETYPE_BINARY_STATS['brute'],
      baseEndurance: 100,
      baseRecovery: 1.67,
      damageModifier: {
        melee: 0.75,
        ranged: 0.75,
        aoe: 0.65,
      },
      buffDebuffModifier: 1.0,
      defenseCap: 0.45,     // 45%
    },
    primarySets: [
      'brute/battle-axe',
      'brute/sonic-melee',
      'brute/street-justice',
      'brute/broad-sword',
      'brute/claws',
      'brute/dark-melee',
      'brute/dual-blades',
      'brute/electrical-melee',
      'brute/energy-melee',
      'brute/fiery-melee',
      'brute/ice-melee',
      'brute/katana',
      'brute/kinetic-melee',
      'brute/martial-arts',
      'brute/psionic-melee',
      'brute/radiation-melee',
      'brute/savage-melee',
      'brute/spines',
      'brute/staff-fighting',
      'brute/stone-melee',
      'brute/super-strength',
      'brute/titan-weapons',
      'brute/war-mace',
    ],
    secondarySets: [
      'brute/bio-armor',
      'brute/dark-armor',
      'brute/electric-armor',
      'brute/energy-aura',
      'brute/fiery-aura',
      'brute/ice-armor',
      'brute/invulnerability',
      'brute/psionic-armor',
      'brute/radiation-armor',
      'brute/regeneration',
      'brute/shield-defense',
      'brute/stone-armor',
      'brute/super-reflexes',
      'brute/willpower',
    ],
  },

  corruptor: {
    name: 'Corruptor',
    side: 'villain',
    description: 'Ranged damage dealer with debuffs and support abilities',
    inherent: {
      name: 'Scourge',
      description: 'Chance for double damage when enemies are below 50% HP. 2.5% per 1% below 50%, guaranteed at 10% HP. ~30% avg damage bonus.',
      icon: 'inherent_blasterdesperation.png',
    },
    stats: {
      ...ARCHETYPE_BINARY_STATS['corruptor'],
      baseEndurance: 100,
      baseRecovery: 1.67,
      damageModifier: {
        melee: 0.55,
        ranged: 0.75,
        aoe: 0.6,
      },
      buffDebuffModifier: 0.75,
      defenseCap: 0.45,     // 45%
    },
    primarySets: [
      'corruptor/archery',
      'corruptor/assault-rifle',
      'corruptor/beam-rifle',
      'corruptor/dark-blast',
      'corruptor/dual-pistols',
      'corruptor/electrical-blast',
      'corruptor/energy-blast',
      'corruptor/fire-blast',
      'corruptor/ice-blast',
      'corruptor/psychic-blast',
      'corruptor/radiation-blast',
      'corruptor/seismic-blast',
      'corruptor/sonic-attack',
      'corruptor/storm-blast',
      'corruptor/water-blast',
    ],
    secondarySets: [
      'corruptor/cold-domination',
      'corruptor/dark-miasma',
      'corruptor/empathy',
      'corruptor/force-field',
      'corruptor/kinetics',
      'corruptor/marine-affinity',
      'corruptor/nature-affinity',
      'corruptor/pain-domination',
      'corruptor/poison',
      'corruptor/radiation-emission',
      'corruptor/electrical-affinity',
      'corruptor/sonic-resonance',
      'corruptor/storm-summoning',
      'corruptor/thermal-radiation',
      'corruptor/time-manipulation',
      'corruptor/traps',
      'corruptor/trick-arrow',
    ],
  },

  dominator: {
    name: 'Dominator',
    side: 'villain',
    description: 'Control specialist with strong offensive capabilities',
    inherent: {
      name: 'Domination',
      description: 'Build meter by attacking, activate at 90%+ for 2× mez magnitude, 1.5× mez duration, mez protection, and full endurance. Lasts 90s.',
      // Click power on a recharge/duration cycle (subject to global recharge),
      // NOT a passive — this drives the perma-tracker in the info panel.
      powerType: 'Click',
      effects: {
        recharge: 200,
        buffDuration: 90,
        // Full endurance restore on activation. Marks Domination as a self-buff
        // for perma eligibility; the 2×/1.5× mez and mez protection are modelled
        // separately in getDominationInfo(). Excluded from always-on totals by
        // the archetype-inherent category filter, so it never pollutes Recovery.
        enduranceGain: 100,
      },
      icon: 'inherent_buffeffects.png',
    },
    stats: {
      ...ARCHETYPE_BINARY_STATS['dominator'],
      baseEndurance: 100,
      baseRecovery: 1.67,
      damageModifier: {
        melee: 0.75,
        ranged: 0.75,
        aoe: 0.65,
      },
      buffDebuffModifier: 0.9,
      defenseCap: 0.45,     // 45%
    },
    primarySets: [
      'dominator/arsenal-control',
      'dominator/darkness-control',
      'dominator/earth-control',
      'dominator/electric-control',
      'dominator/fire-control',
      'dominator/gravity-control',
      'dominator/ice-control',
      'dominator/illusion-control',
      'dominator/mind-control',
      'dominator/plant-control',
      'dominator/pyrotechnic-control',
      'dominator/symphony-control',
      'dominator/wind-control',
    ],
    secondarySets: [
      'dominator/arsenal-assault',
      'dominator/dark-assault',
      'dominator/earth-assault',
      'dominator/electricity-assault',
      'dominator/energy-assault',
      'dominator/fiery-assault',
      'dominator/icy-assault',
      'dominator/martial-assault',
      'dominator/psionic-assault',
      'dominator/radioactive-assault',
      'dominator/savage-assault',
      'dominator/sonic-assault',
      'dominator/thorny-assault',
    ],
  },

  mastermind: {
    name: 'Mastermind',
    side: 'villain',
    description: 'Pet commander with support abilities for minions',
    inherent: {
      name: 'Supremacy',
      description: 'Henchmen within 60ft gain +25% Damage and +10% ToHit. Bodyguard Mode (Defensive/Follow) splits damage: 66% to you, 33% to pets.',
      icon: 'inherentpeacebringer_interspaciallink.png',
    },
    stats: {
      ...ARCHETYPE_BINARY_STATS['mastermind'],
      baseEndurance: 100,
      baseRecovery: 1.67,
      damageModifier: {
        melee: 0.55,
        ranged: 0.55,
        aoe: 0.5,
      },
      buffDebuffModifier: 0.75,
      defenseCap: 0.45,     // 45%
    },
    primarySets: [
      'mastermind/beast-mastery',
      'mastermind/demon-summoning',
      'mastermind/mercenaries',
      'mastermind/necromancy',
      'mastermind/ninjas',
      'mastermind/robotics',
      'mastermind/thugs',
    ],
    secondarySets: [
      'mastermind/cold-domination',
      'mastermind/dark-miasma',
      'mastermind/empathy',
      'mastermind/force-field',
      'mastermind/kinetics',
      'mastermind/marine-affinity',
      'mastermind/nature-affinity',
      'mastermind/pain-domination',
      'mastermind/poison',
      'mastermind/radiation-emission',
      'mastermind/electrical-affinity',
      'mastermind/sonic-resonance',
      'mastermind/storm-summoning',
      'mastermind/thermal-radiation',
      'mastermind/time-manipulation',
      'mastermind/traps',
      'mastermind/trick-arrow',
    ],
  },

  stalker: {
    name: 'Stalker',
    side: 'villain',
    description: 'Stealthy assassin with critical strikes from hide',
    inherent: {
      name: 'Assassination',
      description: 'From Hide: 100% critical (double damage). Outside: 10% base + 3% per teammate. Assassin\'s Focus grants up to +100% crit for Assassin\'s Strike.',
      icon: 'inherent_blasterdesperation.png',
    },
    stats: {
      ...ARCHETYPE_BINARY_STATS['stalker'],
      baseEndurance: 100,
      baseRecovery: 1.67,
      damageModifier: {
        melee: 1.0,
        ranged: 0.6,
        aoe: 0.7,
      },
      buffDebuffModifier: 1.0,
      defenseCap: 0.45,     // 45%
    },
    primarySets: [
      'stalker/sonic-melee',
      'stalker/street-justice',
      'stalker/broad-sword',
      'stalker/claws',
      'stalker/dark-melee',
      'stalker/dual-blades',
      'stalker/electrical-melee',
      'stalker/energy-melee',
      'stalker/fiery-melee',
      'stalker/ice-melee',
      'stalker/kinetic-melee',
      'stalker/martial-arts',
      'stalker/ninja-blade',
      'stalker/psionic-melee',
      'stalker/radiation-melee',
      'stalker/savage-melee',
      'stalker/spines',
      'stalker/staff-fighting',
      'stalker/stone-melee',
    ],
    secondarySets: [
      'stalker/bio-armor',
      'stalker/dark-armor',
      'stalker/electric-armor',
      'stalker/energy-aura',
      'stalker/fiery-aura',
      'stalker/ice-armor',
      'stalker/invulnerability',
      'stalker/ninjitsu',
      'stalker/psionic-armor',
      'stalker/radiation-armor',
      'stalker/regeneration',
      'stalker/shield-defense',
      'stalker/stone-armor',
      'stalker/super-reflexes',
      'stalker/willpower',
    ],
  },

  // ============================================
  // EPIC ARCHETYPES - KHELDIANS (Hero)
  // ============================================

  peacebringer: {
    name: 'Peacebringer',
    side: 'hero',
    description:
      'Kheldian shapeshifter with access to multiple forms. Can transform between human, nova (ranged), and dwarf (melee/tank) forms.',
    inherent: {
      name: 'Cosmic Balance',
      description:
        'Peacebringers bring balance to their team. Damage increases per nearby Tanker/Mastermind/Corruptor/Defender. Damage Resistance increases per nearby Scrapper/Sentinel/Brute/Stalker/Blaster. Control protection per nearby Controller/Dominator. Slow resistance per nearby Kheldian/Arachnos teammate.',
      icon: 'inherentpeacebringer_interspaciallink.png',
    },
    stats: {
      ...ARCHETYPE_BINARY_STATS['peacebringer'],
      baseEndurance: 100,
      baseRecovery: 1.67,
      damageModifier: {
        melee: 0.85,
        ranged: 0.8,
        aoe: 0.7,
      },
      buffDebuffModifier: 1.0,
      defenseCap: 0.45,     // 45%
    },
    primarySets: ['peacebringer/luminous-blast'],
    secondarySets: ['peacebringer/luminous-aura'],
  },

  warshade: {
    name: 'Warshade',
    side: 'hero',
    description:
      'Kheldian shapeshifter that feeds on defeated enemies. Can summon pets from fallen foes and transform between forms.',
    inherent: {
      name: 'Dark Sustenance',
      description:
        'Warshades draw on the power of their teammates. Damage Resistance increases per nearby Tanker/Mastermind/Corruptor/Defender. Damage increases per nearby Scrapper/Sentinel/Stalker/Brute/Blaster. Control protection per nearby Controller/Dominator. Slow resistance per nearby Kheldian/Arachnos teammate.',
      icon: 'inherentpeacebringer_interspaciallink.png',
    },
    stats: {
      ...ARCHETYPE_BINARY_STATS['warshade'],
      baseEndurance: 100,
      baseRecovery: 1.67,
      damageModifier: {
        melee: 0.85,
        ranged: 0.8,
        aoe: 0.7,
      },
      buffDebuffModifier: 1.0,
      defenseCap: 0.45,     // 45%
    },
    primarySets: ['warshade/umbral-blast'],
    secondarySets: ['warshade/umbral-aura'],
  },

  // ============================================
  // EPIC ARCHETYPES - ARACHNOS (Villain)
  // ============================================

  'arachnos-soldier': {
    name: 'Arachnos Soldier',
    side: 'villain',
    description:
      'Versatile soldier with branching power choices. Can specialize as a Crab Spider (pets/support) or Bane Spider (stealth/melee).',
    inherent: {
      name: 'Conditioning',
      description: 'Increased maximum HP and inherent resistance to status effects.',
      icon: 'inherent_blasterdesperation.png',
    },
    stats: {
      ...ARCHETYPE_BINARY_STATS['arachnos-soldier'],
      baseEndurance: 100,
      baseRecovery: 1.67,
      damageModifier: {
        melee: 0.75,
        ranged: 0.75,
        aoe: 0.65,
      },
      buffDebuffModifier: 1.0,
      defenseCap: 0.45,     // 45%
    },
    primarySets: ['arachnos-soldier/arachnos-soldier'],
    secondarySets: ['arachnos-soldier/training-and-gadgets'],
    branches: {
      'bane-spider': {
        name: 'Bane Spider',
        primarySet: 'arachnos-soldier/bane-spider-soldier',
        secondarySet: 'arachnos-soldier/bane-spider-training',
      },
      'crab-spider': {
        name: 'Crab Spider',
        primarySet: 'arachnos-soldier/crab-spider-soldier',
        secondarySet: 'arachnos-soldier/crab-spider-training',
      },
    },
  },

  'arachnos-widow': {
    name: 'Arachnos Widow',
    side: 'villain',
    description:
      'Versatile operative with branching power choices. Can specialize as a Fortunata (psychic powers) or Night Widow (melee assassin).',
    inherent: {
      name: 'Conditioning',
      description: 'Increased maximum HP and inherent resistance to status effects.',
      icon: 'inherent_blasterdesperation.png',
    },
    stats: {
      ...ARCHETYPE_BINARY_STATS['arachnos-widow'],
      baseEndurance: 100,
      baseRecovery: 1.67,
      damageModifier: {
        melee: 0.85,
        ranged: 0.65,
        aoe: 0.7,
      },
      buffDebuffModifier: 1.0,
      defenseCap: 0.45,     // 45%
    },
    primarySets: ['arachnos-widow/widow-training'],
    secondarySets: ['arachnos-widow/teamwork'],
    branches: {
      'night-widow': {
        name: 'Night Widow',
        primarySet: 'arachnos-widow/night-widow-training',
        secondarySet: 'arachnos-widow/widow-teamwork',
      },
      fortunata: {
        name: 'Fortunata',
        primarySet: 'arachnos-widow/fortunata-training',
        secondarySet: 'arachnos-widow/fortunata-teamwork',
      },
    },
  },
};

// ============================================
// HELPER FUNCTIONS
// ============================================

/**
 * Get an archetype by ID
 */
export function getArchetype(id: ArchetypeId): Archetype | undefined {
  return ARCHETYPES[id];
}

/**
 * Get all archetype IDs
 */
export function getArchetypeIds(): ArchetypeId[] {
  return Object.keys(ARCHETYPES) as ArchetypeId[];
}

/**
 * Get archetypes filtered by faction
 */
export function getArchetypesByFaction(faction: 'hero' | 'villain'): Archetype[] {
  return Object.values(ARCHETYPES).filter((at) => at.side === faction);
}

/**
 * Epic archetype IDs (Kheldians and Arachnos)
 */
export const EPIC_ARCHETYPE_IDS: ArchetypeId[] = [
  'peacebringer',
  'warshade',
  'arachnos-soldier',
  'arachnos-widow',
];

/**
 * Standard archetype IDs (non-epic)
 */
export const STANDARD_ARCHETYPE_IDS: ArchetypeId[] = [
  'blaster',
  'controller',
  'defender',
  'scrapper',
  'tanker',
  'sentinel',
  'brute',
  'corruptor',
  'dominator',
  'mastermind',
  'stalker',
];

/**
 * Check if an archetype is an epic archetype
 */
export function isEpicArchetype(id: ArchetypeId): boolean {
  return EPIC_ARCHETYPE_IDS.includes(id);
}

/**
 * Get all epic archetypes
 */
export function getEpicArchetypes(): Archetype[] {
  return EPIC_ARCHETYPE_IDS.map((id) => ARCHETYPES[id]).filter(
    (at): at is Archetype => at !== undefined,
  );
}

/**
 * Get all standard (non-epic) archetypes
 */
export function getStandardArchetypes(): Archetype[] {
  return STANDARD_ARCHETYPE_IDS.map((id) => ARCHETYPES[id]).filter(
    (at): at is Archetype => at !== undefined,
  );
}
