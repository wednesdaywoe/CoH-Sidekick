/**
 * Universal inherent powers — AUTO-GENERATED, DO NOT EDIT.
 *
 * Sprint, Rest, the free travel toggles and the prestige sprints, read from
 * THIS fork's own export. A power missing here is one brainstorm does not
 * have; see scripts/convert-basic-inherents.cjs for how each is addressed.
 * Regenerate: node scripts/convert-basic-inherents.cjs --dataset brainstorm
 *
 * Powers: 11, atoms: 95
 */

import type { Power } from '@/types';

/** A universal inherent: an ordinary Power plus the three planner-side facts. */
export type BasicInherentDef = Power & {
  isLocked?: boolean;
  category?: 'basic' | 'prestige';
  /** 1-based level the game grants it at; 'available' is the picker's marker, not this. */
  grantedAtLevel?: number;
};

export const BASIC_INHERENTS: BasicInherentDef[] = [
  {
    "name": "Brawl",
    "internalName": "Brawl",
    "fullName": "Inherent.Inherent.Brawl",
    "available": -1,
    "grantedAtLevel": 1,
    "autoIssue": true,
    "free": true,
    "isLocked": true,
    "category": "basic",
    "description": "When all else fails, you have only your two fists to depend on, and will cause smashing damage to your target. Brawl also features a synergy with the Fighting pool. If you have trained Boxing or Kick, Brawl will also reduce the target's attack speed and chance to hit. The strength of this effect increases if both Boxing and Kick are owned. If you have trained Cross Punch, Brawl will also reduce the target's regeneration and recovery.\n\nDamage: Minor.\nRecharge: Very Fast.",
    "shortHelp": "Melee, Minor DMG(Smashing), Fighting Synergy",
    "icon": "inherent_brawl.png",
    "powerType": "Click",
    "modesDisallowed": [
      "Peacebringer_Blaster_Mode",
      "Peacebringer_Tanker_Mode",
      "Warshade_Blaster_Mode",
      "Warshade_Tanker_Mode"
    ],
    "targetType": "Foe",
    "requires": [],
    "maxSlots": 6,
    "allowedEnhancements": [
      "Accuracy",
      "Damage",
      "Recharge"
    ],
    "allowedSetCategories": [
      "Melee Damage",
      "Universal Damage Sets"
    ],
    "stats": {
      "accuracy": 1,
      "range": 7,
      "recharge": 2,
      "castTime": 0.83
    },
    "effectArea": "SingleTarget",
    "damage": {
      "type": "Smashing",
      "scale": 0.36,
      "table": "Melee_Damage"
    },
    "atoms": [
      ["Damage","Smashing",0.36,1,0,"Melee_Damage","Abs","Magnitude","Target","Any",true,"Stack",2,null,null,1],
      ["Damage","Smashing",0.36,1,0,"Melee_InherentDamage","Abs","Magnitude","Target","Any",true,"Stack",2,null,null,1,null,null,null,null,null,null,["arch","source>","Class_Controller","eq","kTerrorized","target>","0",">","kImmobilized","target>","0",">","||","kHeld","target>","0",">","||","kStunned","target>","0",">","||","Sleep","target.EventTimeSince>","3","<=","||","&&"],true,null,null,null,null,null,null,null,null,"Containment"],
      ["RechargeTime",null,-0.1,1,10,"Melee_Ones","Str","Expression","Target","Any",true,"Replace",2,null,67,1,null,true,null,null,null,null,["Pool.Fighting.Boxing","source.ownPower?","Pool.Fighting.Kick","source.ownPower?","||"],true,null,null,null,null,["@StdResult","Pool.Fighting.Kick","source.ownPowerNum?","Pool.Fighting.Boxing","source.ownPowerNum?","+","*"],null,0.1,true,null,null,null,null,null,null,null,null,null,null,null,"boxing"],
      ["ToHit",null,-0.1,1,10,"Melee_Ones","Cur","Expression","Target","Any",true,"Replace",2,null,67,1,null,true,null,null,null,null,["Pool.Fighting.Boxing","source.ownPower?","Pool.Fighting.Kick","source.ownPower?","||"],true,null,null,null,null,["@StdResult","Pool.Fighting.Kick","source.ownPowerNum?","Pool.Fighting.Boxing","source.ownPowerNum?","+","*"],null,0.1,true,null,null,null,null,null,null,null,null,null,null,null,"boxing"],
      ["Regeneration",null,-0.1,1,10,"Melee_Ones","Cur","Magnitude","Target","Any",true,"Replace",2,null,67,1,null,true,null,null,null,null,["Pool.Fighting.Cross_Punch","source.ownPower?"],true,null,null,null,null,null,null,0.1,true,null,null,null,null,null,null,null,null,null,null,null,"cross_punch"],
      ["Recovery",null,-0.1,1,10,"Melee_Ones","Cur","Magnitude","Target","Any",true,"Replace",2,null,67,1,null,true,null,null,null,null,["Pool.Fighting.Cross_Punch","source.ownPower?"],true,null,null,null,null,null,null,0.1,true,null,null,null,null,null,null,null,null,null,null,null,"cross_punch"],
      ["Mez","Knockback",3,0,0,"Melee_Ones","Cur","Magnitude","Target","Any",true,"Stack",2,null,null,1,null,true,null,null,null,null,["isPVPMap?","!","FourOne","source.owned?","&&","enttype","target>","player","eq","&&"],true],
      ["Mez","Knockback",3,0,0,"Melee_Ones","Cur","Magnitude","Target","Any",true,"Stack",2,null,null,1,null,true,null,null,null,null,["isPVPMap?","!","FourOne","source.owned?","&&","enttype","target>","critter","eq","&&"],true],
      ["Mez","Knockback",0,0,0,"Melee_Ones","Cur","Magnitude","Target","Any",true,"Stack",2,null,null,0.33000001311302185,null,null,null,null,null,null,["isPVPMap?","!","FourOne","source.owned?","&&"],true],
      ["Mez","Knockback",0,0,0,"Melee_Ones","Cur","Magnitude","Target","Any",true,"Stack",2,null,null,0.33000001311302185,null,null,null,null,null,null,["isPVPMap?","!","FourOne","source.owned?","&&"],true],
      ["Mez","Knockback",0,0,0,"Melee_Ones","Cur","Magnitude","Target","Any",true,"Stack",2,null,null,0.4000000059604645,null,null,null,null,null,null,["isPVPMap?","!","FourOne","source.owned?","&&"],true],
      ["Mez","Knockback",0,0,0,"Melee_Ones","Cur","Magnitude","Target","Any",true,"Stack",2,null,null,0.25,null,null,null,null,null,null,["isPVPMap?","!","FourOne","source.owned?","&&"],true],
      ["Mez","Knockback",0,0,0,"Melee_Ones","Cur","Magnitude","Target","Any",true,"Stack",2,null,null,0.25,null,null,null,null,null,null,["isPVPMap?","!","FourOne","source.owned?","&&"],true],
      ["Mez","Knockback",0,0,0,"Melee_Ones","Cur","Magnitude","Target","Any",true,"Stack",2,null,null,0.25,null,null,null,null,null,null,["isPVPMap?","!","FourOne","source.owned?","&&"],true],
      ["Mez","Knockback",0,0,0,"Melee_Ones","Cur","Magnitude","Target","Any",true,"Stack",2,null,null,0.25,null,null,null,null,null,null,["isPVPMap?","!","FourOne","source.owned?","&&"],true],
      ["Mez","Knockback",0,0,0,"Melee_Ones","Cur","Magnitude","Target","Any",true,"Stack",2,null,null,0.25,null,null,null,null,null,null,["isPVPMap?","!","FourOne","source.owned?","&&"],true],
      ["Meta",null,0,1,1,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,0.5,null,true,null,null,null,null,["isPVPMap?","!","FourOne","source.owned?","&&"],true,null,null,null,null,null,null,null,null,null,null,"null"],
      ["Meta",null,0,1,1,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,["isPVPMap?","!","FourOne","source.owned?","&&"],true,null,null,null,null,null,null,null,null,null,null,"null"],
      ["ExecutePower",null,0,0,0,"Melee_Ones","Str","Magnitude","Target","Any",true,"Stack",2,null,null,0.1111999973654747,null,null,null,null,null,null,["isPVPMap?","!","FourOne","source.owned?","&&"],true],
      ["Meta",null,0,1,1,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,0.1111999973654747,null,true,null,null,null,null,["isPVPMap?","!","FourOne","source.owned?","&&"],true,null,null,null,null,null,null,null,null,null,null,"null",null,1],
      ["Mez","Knockback",100,0,0,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Stack",2,null,null,0.11550000309944153,null,true,null,null,null,null,["isPVPMap?","!","FourOne","source.owned?","&&"],true,null,null,null,null,null,null,null,null,null,null,null,null,0.01],
      ["Meta",null,0,1,1,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,0.11550000309944153,null,true,null,null,null,null,["isPVPMap?","!","FourOne","source.owned?","&&"],true,null,null,null,null,null,null,null,null,null,null,"null",null,0.75],
      ["ExecutePower",null,0,0,0,"Melee_Ones","Str","Magnitude","Target","Any",true,"Stack",2,null,null,0.18780000507831573,null,null,null,null,null,null,["isPVPMap?","!","FourOne","source.owned?","&&"],true],
      ["Meta",null,0,1,1,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,0.18780000507831573,null,true,null,null,null,null,["isPVPMap?","!","FourOne","source.owned?","&&"],true,null,null,null,null,null,null,null,null,null,null,"null",null,0.7],
      ["Mez","Repel",3,0,0,"Melee_Ones","Cur","Magnitude","Target","Any",true,"Stack",2,null,null,1,null,null,null,null,null,null,["isPVPMap?","!","FourOne","source.owned?","&&"],true],
      ["Meta",null,0,0,1,"Melee_Ones","Cur","Magnitude","Target","Any",true,"Stack",2,null,null,1,null,null,null,null,null,null,["isPVPMap?","!","FourOne","source.owned?","&&"],true,null,null,null,null,null,null,null,null,null,null,"null"],
      ["Meta",null,0,0,0.4,"Melee_Ones","Cur","Magnitude","Target","Any",true,"Stack",2,null,null,1,null,null,null,null,null,null,["isPVPMap?","!","FourOne","source.owned?","&&"],true,null,null,null,null,null,null,null,null,null,null,"null",null,0.9],
      ["Meta",null,0,0,0,"Melee_Ones","Cur","Magnitude","Self","Any",true,"Stack",2,null,null,1,null,null,null,null,null,null,["isPVPMap?","!","FourOne","source.owned?","&&"],true,null,null,null,null,null,null,null,null,null,null,"null",null,1],
      ["Meta",null,0,1,1,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,0.5,null,true,null,null,null,null,["isPVPMap?","!","FourOne","source.owned?","&&"],true,null,null,null,null,null,null,null,null,null,null,"null"],
      ["Meta",null,0,1,1,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,["isPVPMap?","!","FourOne","source.owned?","&&"],true,null,null,null,null,null,null,null,null,null,null,"null"],
      ["ExecutePower",null,0,0,0,"Melee_Ones","Str","Magnitude","Target","Any",true,"Stack",2,null,null,0.20000000298023224,null,null,null,null,null,null,["isPVPMap?","!","FourOne","source.owned?","&&"],true],
      ["Meta",null,0,1,1,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,0.20000000298023224,null,true,null,null,null,null,["isPVPMap?","!","FourOne","source.owned?","&&"],true,null,null,null,null,null,null,null,null,null,null,"null",null,0.45],
      ["Mez","Knockback",3,0,0,"Melee_Ones","Cur","Magnitude","Target","Any",true,"Stack",2,null,null,1,null,true,null,null,null,null,["isPVPMap?","!","FourOne","source.owned?","&&","enttype","target>","player","eq","&&"],true,null,null,null,null,null,null,null,null,null,null,null,null,1],
      ["Mez","Knockback",3,0,0,"Melee_Ones","Cur","Magnitude","Target","Any",true,"Stack",2,null,null,1,null,true,null,null,null,null,["isPVPMap?","!","FourOne","source.owned?","&&","enttype","target>","critter","eq","&&"],true,null,null,null,null,null,null,null,null,null,null,null,null,1]
    ],
    "conditionalEffects": [
      {
        "id": "boxing",
        "label": "Boxing",
        "scope": "global",
        "defaultActive": false,
        "ownedPower": {
          "path": "Pool.Fighting.Boxing",
          "count": 1
        },
        "effects": {
          "buffDuration": 10,
          "durations": {
            "rechargeDebuff": 10,
            "tohitDebuff": 10
          },
          "rechargeDebuff": {
            "ignoreStrength": true,
            "scale": 0.1,
            "table": "Melee_Ones"
          },
          "tohitDebuff": {
            "ignoreStrength": true,
            "scale": 0.1,
            "table": "Melee_Ones"
          }
        }
      },
      {
        "id": "cross_punch",
        "label": "Cross Punch",
        "scope": "global",
        "defaultActive": false,
        "ownedPower": {
          "path": "Pool.Fighting.Cross_Punch",
          "count": 1
        },
        "effects": {
          "buffDuration": 10,
          "durations": {
            "recoveryDebuff": 10,
            "regenDebuff": 10
          },
          "recoveryDebuff": {
            "ignoreStrength": true,
            "scale": 0.1,
            "table": "Melee_Ones"
          },
          "regenDebuff": {
            "ignoreStrength": true,
            "scale": 0.1,
            "table": "Melee_Ones"
          }
        }
      }
    ],
    "targetsAffected": [
      "Foe"
    ]
  },
  {
    "name": "Sprint",
    "internalName": "Sprint",
    "fullName": "Inherent.Inherent.Sprint",
    "available": -1,
    "grantedAtLevel": 1,
    "autoIssue": true,
    "free": true,
    "isLocked": true,
    "category": "basic",
    "description": "Sprint allows you to travel, or run away, slightly faster than normal, while slightly draining your Endurance.\n\nSprint's movement buff stacks with other travel powers.",
    "shortHelp": "Boost Run SPD",
    "icon": "inherent_sprint.png",
    "powerType": "Toggle",
    "modesSuspended": [
      "Peacebringer_Blaster_Mode",
      "Peacebringer_Tanker_Mode",
      "Suppress_RunToggles",
      "Warshade_Blaster_Mode",
      "Warshade_Tanker_Mode"
    ],
    "modesDisallowed": [
      "Disable_RunToggles"
    ],
    "targetType": "Self",
    "requires": [],
    "maxSlots": 6,
    "allowedEnhancements": [
      "EnduranceReduction",
      "Jump",
      "Run Speed"
    ],
    "allowedSetCategories": [
      "Leaping & Sprints",
      "Running & Sprints"
    ],
    "stats": {
      "accuracy": 1,
      "endurance": 0.1463,
      "activatePeriod": 0.5
    },
    "effectArea": "SingleTarget",
    "atoms": [
      ["Movement","Run",0.5,1,0.8,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1],
      ["Movement","Run",0.5,1,0.75,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
      ["Movement","JumpHeight",0.1,1,0.75,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1]
    ],
    "targetsAffected": [
      "Self"
    ]
  },
  {
    "name": "Rest",
    "internalName": "Rest",
    "fullName": "Inherent.Inherent.Rest",
    "available": -1,
    "grantedAtLevel": 2,
    "autoIssue": true,
    "free": true,
    "isLocked": true,
    "category": "basic",
    "description": "Activate Rest to heal Hit Points and recover Endurance. While Resting you cannot attack, and you are extremely vulnerable to attack and damage.\n\nActivation of Rest can be interrupted, and the power must be active for a few seconds before you start to recuperate.\n\nNotes: This power can be used while flying, but will make you fall to the ground.",
    "shortHelp": "Self Heal Recover, -DEF",
    "icon": "inherent_rest.png",
    "powerType": "Toggle",
    "modesDisallowed": [
      "Peacebringer_Blaster_Mode",
      "Peacebringer_Tanker_Mode",
      "Warshade_Blaster_Mode",
      "Warshade_Tanker_Mode"
    ],
    "targetType": "Self",
    "requires": [],
    "maxSlots": 6,
    "allowedEnhancements": [
      "EnduranceModification",
      "Healing",
      "Interrupt"
    ],
    "stats": {
      "accuracy": 1,
      "castTime": 6,
      "interruptTime": 6,
      "activatePeriod": 0.2
    },
    "effectArea": "SingleTarget",
    "atoms": [
      ["Regeneration",null,19,0,0.55,"Melee_Ones","Cur","Magnitude","Target","Any",false,"Replace",2,null,null,1,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"RestBuffs"],
      ["Recovery",null,4.25,0,0.55,"Melee_Ones","Cur","Magnitude","Target","Any",false,"Replace",2,null,null,1,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"RestBuffs"],
      ["Mez","OnlyAffectsSelf",100,0,0.55,"Melee_Ones","Cur","Magnitude","Target","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"RestPenalties"],
      ["Mez","Untouchable",-100,0,0.55,"Melee_Ones","Cur","Magnitude","Target","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"RestPenalties"],
      ["Resistance","Smashing",-10,1,0.55,"Melee_Ones","Res","Magnitude","Target","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"RestPenalties"],
      ["Resistance","Lethal",-10,1,0.55,"Melee_Ones","Res","Magnitude","Target","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"RestPenalties"],
      ["Resistance","Fire",-10,1,0.55,"Melee_Ones","Res","Magnitude","Target","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"RestPenalties"],
      ["Resistance","Cold",-10,1,0.55,"Melee_Ones","Res","Magnitude","Target","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"RestPenalties"],
      ["Resistance","Energy",-10,1,0.55,"Melee_Ones","Res","Magnitude","Target","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"RestPenalties"],
      ["Resistance","Negative",-10,1,0.55,"Melee_Ones","Res","Magnitude","Target","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"RestPenalties"],
      ["Resistance","Psionic",-10,1,0.55,"Melee_Ones","Res","Magnitude","Target","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"RestPenalties"],
      ["Resistance","Toxic",-10,1,0.55,"Melee_Ones","Res","Magnitude","Target","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"RestPenalties"],
      ["Resistance","Special",-10,1,0.55,"Melee_Ones","Res","Magnitude","Target","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"RestPenalties"],
      ["Defense","All",-1000,0,0.55,"Melee_Ones","Cur","Magnitude","Target","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"RestPenalties"],
      ["Movement","FlyMode",-1000,0,0.55,"Melee_Ones","Cur","Magnitude","Target","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"RestPenalties"],
      ["Mez","Teleport",-1000,0,0.55,"Melee_Ones","Cur","Magnitude","Target","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"RestPenalties"],
      ["Movement","Run",-1000,0,0.55,"Melee_Ones","Cur","Magnitude","Target","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"RestPenalties"],
      ["Movement","Fly",-1000,0,0.55,"Melee_Ones","Cur","Magnitude","Target","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"RestPenalties"],
      ["Movement","Jump",-1000,0,0.55,"Melee_Ones","Cur","Magnitude","Target","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"RestPenalties"],
      ["Movement","JumpHeight",-1000,1,0.55,"Melee_Ones","Cur","Magnitude","Target","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"RestPenalties"],
      ["Movement","Run",-1000,1,0.55,"Melee_Ones","Max","Magnitude","Target","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"RestPenalties"]
    ],
    "targetsAffected": [
      "Self"
    ]
  },
  {
    "name": "Ninja Run",
    "internalName": "Prestige_Ninja_Run",
    "fullName": "Prestige.Prestige_Travel.Prestige_Ninja_Run",
    "grantedBy": "Inherent.Inherent.Prestige_Ninja_Run",
    "available": -1,
    "grantedAtLevel": 4,
    "autoIssue": true,
    "free": true,
    "isLocked": true,
    "category": "basic",
    "description": "A combination of quickness and acrobatic grace, Ninja Run allows you to move at high speed, whether on the ground or leaping from rooftop to rooftop. This power is not as fast as Super Speed, nor will it allow you to jump as well as Super Leap, however it is considerably better than the Fitness powers Swift and Hurdle.\n\nNinja Run can be active at the same time as other running and jumping toggles, but only the strongest run speed buff and strongest jumping buff will apply.\n\nNote that Ninja Run is unaffected by Endurance Discount changes",
    "shortHelp": "Toggle: Self +Run Speed, +Jump",
    "icon": "inherent_ninjarun.png",
    "powerType": "Toggle",
    "setsModes": [
      "Disable_Stance"
    ],
    "modesSuspended": [
      "Peacebringer_Blaster_Mode",
      "Peacebringer_Tanker_Mode",
      "Suppress_JumpToggles",
      "Suppress_RunToggles",
      "Suppress_TravelToggles",
      "Warshade_Blaster_Mode",
      "Warshade_Tanker_Mode"
    ],
    "modesDisallowed": [
      "Disable_JumpToggles",
      "Disable_RunToggles",
      "Disable_Travel"
    ],
    "targetType": "Self",
    "requires": [],
    "maxSlots": 0,
    "allowedEnhancements": [],
    "stats": {
      "accuracy": 1,
      "endurance": 0.2844,
      "activatePeriod": 0.5
    },
    "effectArea": "SingleTarget",
    "atoms": [
      ["Movement","Control",10,1,0.75,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Suppress",2,null,null,1,null,null,null,null,null,null,null,null,null,true,null,"TravelBuff",null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,true],
      ["Movement","Jump",0.55,1,0.75,"Melee_SpeedJumping","Cur","Magnitude","Self","Any",false,"Suppress",2,null,null,1,null,null,null,null,null,null,null,null,null,true,null,"TravelBuff",null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,true],
      ["Movement","JumpHeight",0.25,1,0.75,"Melee_Leap","Cur","Magnitude","Self","Any",false,"Suppress",2,null,null,1,null,null,null,null,null,null,null,null,null,true,null,"TravelBuff",null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,true],
      ["Movement","Run",0.4,1,0.75,"Melee_SpeedRunning","Cur","Magnitude","Self","Any",false,"Suppress",2,null,null,1,null,null,null,null,null,null,null,null,null,true,null,"TravelBuff",null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,true],
      ["Movement","Friction",2,1,0.75,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Suppress",2,null,null,1,null,null,null,null,null,null,null,null,null,true,null,"TravelBuff",null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,true],
      ["Meta",null,1,1,0.75,"Melee_Ones","Cur","Constant","Self","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Stance",null,"null",null,null,null,null,null,["Stunned","Held","Sleep"]],
      ["Meta",null,1,32,0.75,"Melee_Ones","Cur","Constant","Self","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Stance",null,"set_mode"]
    ],
    "targetsAffected": [
      "Self"
    ]
  },
  {
    "name": "Beast Run",
    "internalName": "Prestige_Beast_Run",
    "fullName": "Prestige.Prestige_Travel.Prestige_Beast_Run",
    "grantedBy": "Inherent.Inherent.Prestige_Beast_Run",
    "available": -1,
    "grantedAtLevel": 4,
    "autoIssue": true,
    "free": true,
    "isLocked": true,
    "category": "basic",
    "description": "Show off your bestial nature by using this power. This power is not as fast as Super Speed, nor will it allow you to jump as well as Super Leap, however it is considerably better than the Fitness powers Swift and Hurdle.\n\nBeast Run can be active at the same time as other running and jumping toggles, but only the strongest run speed buff and strongest jumping buff will apply.\n\nNote that Beast Run is unaffected by Endurance Discount changes",
    "shortHelp": "Toggle: Self +Run Speed, +Jump",
    "icon": "inherent_beastrun.png",
    "powerType": "Toggle",
    "setsModes": [
      "Disable_Stance"
    ],
    "modesSuspended": [
      "Peacebringer_Blaster_Mode",
      "Peacebringer_Tanker_Mode",
      "Suppress_JumpToggles",
      "Suppress_RunToggles",
      "Suppress_TravelToggles",
      "Warshade_Blaster_Mode",
      "Warshade_Tanker_Mode"
    ],
    "modesDisallowed": [
      "Disable_JumpToggles",
      "Disable_RunToggles",
      "Disable_Travel"
    ],
    "targetType": "Self",
    "requires": [],
    "maxSlots": 0,
    "allowedEnhancements": [],
    "stats": {
      "accuracy": 1,
      "endurance": 0.2844,
      "activatePeriod": 0.5
    },
    "effectArea": "SingleTarget",
    "atoms": [
      ["Movement","Control",10,1,0.75,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Suppress",2,null,null,1,null,null,null,null,null,null,null,null,null,true,null,"TravelBuff",null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,true],
      ["Movement","Jump",0.55,1,0.75,"Melee_SpeedJumping","Cur","Magnitude","Self","Any",false,"Suppress",2,null,null,1,null,null,null,null,null,null,null,null,null,true,null,"TravelBuff",null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,true],
      ["Movement","JumpHeight",0.25,1,0.75,"Melee_Leap","Cur","Magnitude","Self","Any",false,"Suppress",2,null,null,1,null,null,null,null,null,null,null,null,null,true,null,"TravelBuff",null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,true],
      ["Movement","Run",0.4,1,0.75,"Melee_SpeedRunning","Cur","Magnitude","Self","Any",false,"Suppress",2,null,null,1,null,null,null,null,null,null,null,null,null,true,null,"TravelBuff",null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,true],
      ["Movement","Friction",2,1,0.75,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Suppress",2,null,null,1,null,null,null,null,null,null,null,null,null,true,null,"TravelBuff",null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,true],
      ["Meta",null,1,1,0.75,"Melee_Ones","Cur","Constant","Self","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Stance",null,"null",null,null,null,null,null,["Stunned","Held","Sleep"]],
      ["Meta",null,1,32,0.75,"Melee_Ones","Cur","Constant","Self","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Stance",null,"set_mode"]
    ],
    "targetsAffected": [
      "Self"
    ]
  },
  {
    "name": "Athletic Run",
    "internalName": "Prestige_Athletic_Run",
    "fullName": "Prestige.Prestige_Travel.Prestige_Athletic_Run",
    "available": -1,
    "grantedAtLevel": 1,
    "autoIssue": false,
    "free": true,
    "isLocked": true,
    "category": "basic",
    "description": "Show off your extensive athletic training by using this power. This power is not as fast as Super Speed, nor will it allow you to jump as well as Super Leap, however it is considerably better than the Fitness powers Swift and Hurdle.\n\nAthletic Run can be active at the same time as other running and jumping toggles, but only the strongest run speed buff and strongest jumping buff will apply.\n\nNote that Athletic Run is unaffected by Endurance Discount changes",
    "shortHelp": "Toggle: Self +Run Speed, +Jump",
    "icon": "inherent_athleticrun.png",
    "powerType": "Toggle",
    "modesSuspended": [
      "Peacebringer_Blaster_Mode",
      "Peacebringer_Tanker_Mode",
      "Suppress_JumpToggles",
      "Suppress_RunToggles",
      "Suppress_TravelToggles",
      "Warshade_Blaster_Mode",
      "Warshade_Tanker_Mode"
    ],
    "modesDisallowed": [
      "Disable_JumpToggles",
      "Disable_RunToggles",
      "Disable_Travel"
    ],
    "targetType": "Self",
    "requires": [],
    "maxSlots": 0,
    "allowedEnhancements": [],
    "stats": {
      "accuracy": 1,
      "endurance": 0.2844,
      "activatePeriod": 0.5
    },
    "effectArea": "SingleTarget",
    "atoms": [
      ["Movement","Control",10,1,0.75,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Suppress",2,null,null,1,null,null,null,null,null,null,null,null,null,true,null,"TravelBuff",null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,true],
      ["Movement","Jump",0.55,1,0.75,"Melee_SpeedJumping","Cur","Magnitude","Self","Any",false,"Suppress",2,null,null,1,null,null,null,null,null,null,null,null,null,true,null,"TravelBuff",null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,true],
      ["Movement","JumpHeight",0.25,1,0.75,"Melee_Leap","Cur","Magnitude","Self","Any",false,"Suppress",2,null,null,1,null,null,null,null,null,null,null,null,null,true,null,"TravelBuff",null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,true],
      ["Movement","Run",0.4,1,0.75,"Melee_SpeedRunning","Cur","Magnitude","Self","Any",false,"Suppress",2,null,null,1,null,null,null,null,null,null,null,null,null,true,null,"TravelBuff",null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,true],
      ["Movement","Friction",2,1,0.75,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Suppress",2,null,null,1,null,null,null,null,null,null,null,null,null,true,null,"TravelBuff",null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,true],
      ["GlobalChanceMod",null,1,1,0.75,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true]
    ],
    "targetsAffected": [
      "Self"
    ]
  },
  {
    "name": "Prestige Power Slide",
    "internalName": "prestige_DVD_Glidep",
    "fullName": "Inherent.Inherent.prestige_DVD_Glidep",
    "available": -1,
    "grantedAtLevel": 1,
    "autoIssue": true,
    "free": true,
    "isLocked": true,
    "category": "prestige",
    "description": "Forms a frictionless field of energy beneath the user that allows rapid transit while remaining close to the ground.",
    "shortHelp": "Boost Run SPD",
    "icon": "inherent_sprint.png",
    "powerType": "Toggle",
    "setsModes": [
      "Disable_Stance"
    ],
    "modesSuspended": [
      "Peacebringer_Blaster_Mode",
      "Peacebringer_Tanker_Mode",
      "Suppress_RunToggles",
      "Warshade_Blaster_Mode",
      "Warshade_Tanker_Mode"
    ],
    "modesDisallowed": [
      "Disable_RunToggles"
    ],
    "targetType": "Self",
    "requires": [
      "DVDSpecialEdition",
      "auth>",
      "cucpccp1",
      "productOwned?",
      "||"
    ],
    "maxSlots": 6,
    "allowedEnhancements": [
      "EnduranceReduction",
      "Jump",
      "Run Speed"
    ],
    "allowedSetCategories": [
      "Leaping & Sprints",
      "Running & Sprints"
    ],
    "stats": {
      "accuracy": 1,
      "endurance": 0.1463,
      "activatePeriod": 0.5
    },
    "effectArea": "SingleTarget",
    "atoms": [
      ["Movement","Run",0.5,1,0.75,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1],
      ["Movement","Run",0.5,1,0.75,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
      ["Movement","JumpHeight",0.1,1,0.75,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1],
      ["Meta",null,1,1,0.75,"Melee_Ones","Cur","Constant","Self","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Stance",null,"null",null,null,null,null,null,["Stunned","Held","Sleep"]],
      ["Meta",null,1,32,0.75,"Melee_Ones","Cur","Constant","Self","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Stance",null,"set_mode"]
    ],
    "targetsAffected": [
      "Self"
    ]
  },
  {
    "name": "Prestige Power Rush",
    "internalName": "prestige_Gamestop_Sprintp",
    "fullName": "Inherent.Inherent.prestige_Gamestop_Sprintp",
    "available": -1,
    "grantedAtLevel": 1,
    "autoIssue": true,
    "free": true,
    "isLocked": true,
    "category": "prestige",
    "description": "Sprint allows you to travel, or run away, slightly faster than normal, while slightly draining your Endurance.\n\nSprint's movement buff stacks with other travel powers.",
    "shortHelp": "Boost Run SPD",
    "icon": "inherent_sprint.png",
    "powerType": "Toggle",
    "modesSuspended": [
      "Peacebringer_Blaster_Mode",
      "Peacebringer_Tanker_Mode",
      "Suppress_RunToggles",
      "Warshade_Blaster_Mode",
      "Warshade_Tanker_Mode"
    ],
    "modesDisallowed": [
      "Disable_RunToggles"
    ],
    "targetType": "Self",
    "requires": [
      "Preorder:GameStop",
      "auth>",
      "VetSprints",
      "Owned?",
      "||"
    ],
    "maxSlots": 6,
    "allowedEnhancements": [
      "EnduranceReduction",
      "Jump",
      "Run Speed"
    ],
    "allowedSetCategories": [
      "Leaping & Sprints",
      "Running & Sprints"
    ],
    "stats": {
      "accuracy": 1,
      "endurance": 0.1463,
      "activatePeriod": 0.5
    },
    "effectArea": "SingleTarget",
    "atoms": [
      ["Movement","Run",0.5,1,0.8,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1],
      ["Movement","Run",0.5,1,0.75,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
      ["Movement","JumpHeight",0.1,1,0.75,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1]
    ],
    "targetsAffected": [
      "Self"
    ]
  },
  {
    "name": "Prestige Power Surge",
    "internalName": "prestige_generic_Sprintp",
    "fullName": "Inherent.Inherent.prestige_generic_Sprintp",
    "available": -1,
    "grantedAtLevel": 1,
    "autoIssue": true,
    "free": true,
    "isLocked": true,
    "category": "prestige",
    "description": "Sprint allows you to travel, or run away, slightly faster than normal, while slightly draining your Endurance.\n\nSprint's movement buff stacks with other travel powers.",
    "shortHelp": "Boost Run SPD",
    "icon": "inherent_sprint.png",
    "powerType": "Toggle",
    "modesSuspended": [
      "Peacebringer_Blaster_Mode",
      "Peacebringer_Tanker_Mode",
      "Suppress_RunToggles",
      "Warshade_Blaster_Mode",
      "Warshade_Tanker_Mode"
    ],
    "modesDisallowed": [
      "Disable_RunToggles"
    ],
    "targetType": "Self",
    "requires": [
      "Preorder:Generic",
      "auth>",
      "VetSprints",
      "Owned?",
      "||"
    ],
    "maxSlots": 6,
    "allowedEnhancements": [
      "EnduranceReduction",
      "Jump",
      "Run Speed"
    ],
    "allowedSetCategories": [
      "Leaping & Sprints",
      "Running & Sprints"
    ],
    "stats": {
      "accuracy": 1,
      "endurance": 0.1463,
      "activatePeriod": 0.5
    },
    "effectArea": "SingleTarget",
    "atoms": [
      ["Movement","Run",0.5,1,0.8,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1],
      ["Movement","Run",0.5,1,0.75,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
      ["Movement","JumpHeight",0.1,1,0.75,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1]
    ],
    "targetsAffected": [
      "Self"
    ]
  },
  {
    "name": "Prestige Power Dash",
    "internalName": "prestige_BestBuy_Sprintp",
    "fullName": "Inherent.Inherent.prestige_BestBuy_Sprintp",
    "available": -1,
    "grantedAtLevel": 1,
    "autoIssue": true,
    "free": true,
    "isLocked": true,
    "category": "prestige",
    "description": "Sprint allows you to travel, or run away, slightly faster than normal, while slightly draining your Endurance.\n\nSprint's movement buff stacks with other travel powers.",
    "shortHelp": "Boost Run SPD",
    "icon": "inherent_sprint.png",
    "powerType": "Toggle",
    "modesSuspended": [
      "Peacebringer_Blaster_Mode",
      "Peacebringer_Tanker_Mode",
      "Suppress_RunToggles",
      "Warshade_Blaster_Mode",
      "Warshade_Tanker_Mode"
    ],
    "modesDisallowed": [
      "Disable_RunToggles"
    ],
    "targetType": "Self",
    "requires": [
      "Preorder:BestBuy",
      "auth>",
      "VetSprints",
      "Owned?",
      "||"
    ],
    "maxSlots": 6,
    "allowedEnhancements": [
      "EnduranceReduction",
      "Jump",
      "Run Speed"
    ],
    "allowedSetCategories": [
      "Leaping & Sprints",
      "Running & Sprints"
    ],
    "stats": {
      "accuracy": 1,
      "endurance": 0.1463,
      "activatePeriod": 0.5
    },
    "effectArea": "SingleTarget",
    "atoms": [
      ["Movement","Run",0.5,1,0.8,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1],
      ["Movement","Run",0.5,1,0.75,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
      ["Movement","JumpHeight",0.1,1,0.75,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1]
    ],
    "targetsAffected": [
      "Self"
    ]
  },
  {
    "name": "Prestige Power Quick",
    "internalName": "prestige_EB_Sprintp",
    "fullName": "Inherent.Inherent.prestige_EB_Sprintp",
    "available": -1,
    "grantedAtLevel": 1,
    "autoIssue": true,
    "free": true,
    "isLocked": true,
    "category": "prestige",
    "description": "Sprint allows you to travel, or run away, slightly faster than normal, while slightly draining your Endurance.\n\nSprint's movement buff stacks with other travel powers.",
    "shortHelp": "Boost Run SPD",
    "icon": "inherent_sprint.png",
    "powerType": "Toggle",
    "modesSuspended": [
      "Peacebringer_Blaster_Mode",
      "Peacebringer_Tanker_Mode",
      "Suppress_RunToggles",
      "Warshade_Blaster_Mode",
      "Warshade_Tanker_Mode"
    ],
    "modesDisallowed": [
      "Disable_RunToggles"
    ],
    "targetType": "Self",
    "requires": [
      "Preorder:EB",
      "auth>",
      "VetSprints",
      "Owned?",
      "||"
    ],
    "maxSlots": 6,
    "allowedEnhancements": [
      "EnduranceReduction",
      "Jump",
      "Run Speed"
    ],
    "allowedSetCategories": [
      "Leaping & Sprints",
      "Running & Sprints"
    ],
    "stats": {
      "accuracy": 1,
      "endurance": 0.1463,
      "activatePeriod": 0.5
    },
    "effectArea": "SingleTarget",
    "atoms": [
      ["Movement","Run",0.5,1,0.8,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1],
      ["Movement","Run",0.5,1,0.75,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
      ["Movement","JumpHeight",0.1,1,0.75,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1]
    ],
    "targetsAffected": [
      "Self"
    ]
  }
];
