/**
 * Universal inherent powers — AUTO-GENERATED, DO NOT EDIT.
 *
 * Sprint, Rest, the free travel toggles and the prestige sprints, read from
 * THIS fork's own export. A power missing here is one rebirth does not
 * have; see scripts/convert-basic-inherents.cjs for how each is addressed.
 * Regenerate: node scripts/convert-basic-inherents.cjs --dataset rebirth
 *
 * Powers: 10, atoms: 71
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
    "description": "When all else fails, you have only your two fists to depend on, and will cause smashing damage to your target. Brawl also features a synergy with the Fighting pool. If you have trained Boxing or Kick, Brawl will also reduce the target's attack speed and chance to hit. The strength of this effect increases if both Boxing and Kick are owned. If you have trained Cross Punch, Brawl will also reduce the target's regeneration and recovery.",
    "shortHelp": "Melee, Minor DMG (Smashing), Fighting Synergy",
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
      ["Damage","Smashing",0.36,1,0,"Melee_Damage","Abs","Magnitude","Target","Any",true,"Stack",2,null,null,1,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Damage"],
      ["Meta",null,1,1,0,"Melee_Ones","Abs","Magnitude","Target","PvP",false,"Stack",2,null,null,0,null,true,null,null,null,null,["enttype","target>","player","eq"],true,null,null,null,null,null,null,0,null,"Ones",null,"drop_toggles"],
      ["Damage","Smashing",0.36,1,0,"Melee_Damage","Abs","Magnitude","Target","Any",true,"Stack",2,null,null,1,null,null,null,null,null,null,["arch","source>","Class_Controller","eq","kImmobilized","target>","0",">","kHeld","target>","0",">","||","kSleep","target>","0",">","||","kStunned","target>","0",">","||","&&"],true,null,null,null,null,null,null,null,null,"Damage"],
      ["RechargeTime",null,-0.1,1,10,"Melee_Ones","Str","Expression","Target","Any",true,"Replace",2,null,67,0.10000000149011612,null,true,null,null,null,null,["Pool.Fighting.Boxing","source.ownPower?","Pool.Fighting.Kick","source.ownPower?","||"],true,null,null,null,null,["@StdResult","Pool.Fighting.Kick","source.ownPowerNum?","Pool.Fighting.Boxing","source.ownPowerNum?","+","*"],null,0.1,true,"Ones",null,null,null,null,null,null,null,null,null,null,"boxing"],
      ["ToHit",null,-0.1,1,10,"Melee_Ones","Cur","Expression","Target","Any",true,"Replace",2,null,67,0.10000000149011612,null,true,null,null,null,null,["Pool.Fighting.Boxing","source.ownPower?","Pool.Fighting.Kick","source.ownPower?","||"],true,null,null,null,null,["@StdResult","Pool.Fighting.Kick","source.ownPowerNum?","Pool.Fighting.Boxing","source.ownPowerNum?","+","*"],null,0.1,true,"Ones",null,null,null,null,null,null,null,null,null,null,"boxing"],
      ["Regeneration",null,-0.1,1,10,"Melee_Ones","Cur","Magnitude","Target","Any",true,"Replace",2,null,67,0.10000000149011612,null,true,null,null,null,null,["Pool.Fighting.Cross_Punch","source.ownPower?"],true,null,null,null,null,null,null,0.1,true,"Ones",null,null,null,null,null,null,null,null,null,null,"cross_punch"],
      ["Recovery",null,-0.1,1,10,"Melee_Ones","Cur","Magnitude","Target","Any",true,"Replace",2,null,67,0.10000000149011612,null,true,null,null,null,null,["Pool.Fighting.Cross_Punch","source.ownPower?"],true,null,null,null,null,null,null,0.1,true,"Ones",null,null,null,null,null,null,null,null,null,null,"cross_punch"]
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
    "description": "Sprint allows you to travel, or run away, slightly faster than normal, while slightly draining your Endurance.",
    "shortHelp": "Boost Run SPD",
    "icon": "inherent_sprint.png",
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
      "EnduranceReduction",
      "Jump",
      "Run Speed"
    ],
    "allowedSetCategories": [
      "Leaping",
      "Running"
    ],
    "stats": {
      "accuracy": 1,
      "endurance": 0.1462,
      "activatePeriod": 0.5
    },
    "effectArea": "SingleTarget",
    "atoms": [
      ["Movement","Run",0.5,1,0.8,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones"],
      ["Movement","Run",0.5,1,0.75,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones"],
      ["Movement","JumpHeight",0.1,1,0.75,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones"]
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
    "description": "Activate Rest to heal Hit Points and recover Endurance. While Resting you cannot attack, and you are extremely vulnerable to attack and damage. Activation of Rest can be interrupted, and the power must be active for a few seconds before you start to recuperate.",
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
      "Interrupt",
      "Recharge"
    ],
    "allowedSetCategories": [
      "Rest Buff"
    ],
    "stats": {
      "accuracy": 1,
      "recharge": 180,
      "castTime": 6,
      "interruptTime": 6,
      "activatePeriod": 0.2
    },
    "effectArea": "SingleTarget",
    "atoms": [
      ["Regeneration",null,19,1,0.55,"Melee_Ones","Cur","Magnitude","Target","Any",false,"Replace",2,null,null,1,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones"],
      ["Recovery",null,4.25,1,0,"Melee_Ones","Cur","Magnitude","Target","Any",false,"Replace",2,null,1,1,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones"],
      ["Mez","Immobilized",0.3,100,0,"Melee_Ones","Cur","Duration","Target","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones"],
      ["Mez","OnlyAffectsSelf",0.55,100,0,"Melee_Ones","Cur","Duration","Target","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones"],
      ["Mez","Untouchable",0.55,-100,0,"Melee_Ones","Cur","Duration","Target","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones"],
      ["Resistance","Smashing",-10,1,0.55,"Melee_Ones","Res","Magnitude","Target","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones"],
      ["Resistance","Lethal",-10,1,0.55,"Melee_Ones","Res","Magnitude","Target","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones"],
      ["Resistance","Fire",-10,1,0.55,"Melee_Ones","Res","Magnitude","Target","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones"],
      ["Resistance","Cold",-10,1,0.55,"Melee_Ones","Res","Magnitude","Target","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones"],
      ["Resistance","Energy",-10,1,0.55,"Melee_Ones","Res","Magnitude","Target","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones"],
      ["Resistance","Negative",-10,1,0.55,"Melee_Ones","Res","Magnitude","Target","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones"],
      ["Resistance","Toxic",-10,1,0.55,"Melee_Ones","Res","Magnitude","Target","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones"],
      ["Resistance","Psionic",-10,1,0.55,"Melee_Ones","Res","Magnitude","Target","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones"],
      ["Resistance","Special",-10,1,0.55,"Melee_Ones","Res","Magnitude","Target","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones"],
      ["Defense","All",-1000,1,0.55,"Melee_Ones","Cur","Magnitude","Target","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones"],
      ["Mez","Teleport",-100,1,0.55,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones"],
      ["Mez","Stunned",0.55,1,0,"Melee_Ones","Cur","Duration","Target","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones"],
      ["MezResist","Stunned",-1,1,0.55,"Melee_Ones","Res","Magnitude","Target","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones"],
      ["Mez","Sleep",0.55,1,0,"Melee_Ones","Cur","Duration","Target","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones"],
      ["MezResist","Sleep",-1,1,0.55,"Melee_Ones","Res","Magnitude","Target","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones"],
      ["Movement","FlyMode",0.55,-100,0,"Melee_Ones","Cur","Duration","Target","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones"]
    ],
    "targetsAffected": [
      "Self"
    ]
  },
  {
    "name": "Ninja Run",
    "internalName": "Prestige_Ninja_Run",
    "fullName": "Inherent.Inherent.Prestige_Ninja_Run",
    "available": -1,
    "grantedAtLevel": 4,
    "autoIssue": true,
    "free": true,
    "isLocked": true,
    "category": "basic",
    "description": "A combination of quickness and acrobatic grace, Ninja Run allows you to move at high speed, whether on the ground or leaping from rooftop to rooftop.  This power is not as fast as Super Speed, nor will it allow you to jump as well as Super Leap, however it is considerably better than the Fitness powers Swift and Hurdle.",
    "shortHelp": "Toggle: Self +Run Speed, +Jump",
    "icon": "inherent_ninjarun.png",
    "powerType": "Toggle",
    "setsModes": [
      "TravelJumpOn"
    ],
    "modesDisallowed": [
      "Disable_Travel",
      "Peacebringer_Blaster_Mode",
      "Peacebringer_Tanker_Mode",
      "Warshade_Blaster_Mode",
      "Warshade_Tanker_Mode"
    ],
    "targetType": "Self",
    "requires": [
      "MartialArtsPack",
      "auth>",
      "cucpmanr",
      "productOwned?",
      "||",
      "Inherent.Inherent.Prestige_Ninja_Run",
      "||"
    ],
    "maxSlots": 0,
    "allowedEnhancements": [],
    "stats": {
      "accuracy": 1,
      "endurance": 0.2275,
      "activatePeriod": 0.5
    },
    "effectArea": "SingleTarget",
    "atoms": [
      ["Meta",null,1,122,999999,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones",null,"set_mode",null,null,null,null,null,null,"OnActivate"],
      ["Movement","JumpHeight",0.25,1,0.75,"Melee_Leap","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,null,null,null,null,null,["isPVPMap?","!"],null,null,true,null,null,null,null,null,null,"Leap"],
      ["Movement","Jump",0.55,1,0.75,"Melee_SpeedJumping","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,null,null,null,null,null,["isPVPMap?","!"],null,null,true,null,null,null,null,null,null,"SpeedJumping"],
      ["Movement","Control",10,1,0.75,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,null,null,null,null,null,["isPVPMap?","!"],null,null,true,null,null,null,null,null,null,"Ones"],
      ["Movement","Friction",2,1,0.75,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,null,null,null,null,null,["isPVPMap?","!"],null,null,true,null,null,null,null,null,null,"Ones"],
      ["Movement","Run",0.4,1,0.75,"Melee_SpeedRunning","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,null,null,null,null,null,["isPVPMap?","!"],null,null,true,null,null,null,null,null,null,"SpeedRunning"],
      ["Meta",null,1,1,0.75,"Melee_Ones","Cur","Constant","Self","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones",null,"ninja_run",null,null,null,null,null,["Held","Stunned","Sleep"]],
      ["Movement","JumpHeight",0.25,1,0.75,"Melee_Leap","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,null,null,null,null,null,["isPVPMap?"],true,null,null,null,null,null,null,null,null,"Leap"],
      ["Movement","Jump",0.55,1,0.75,"Melee_SpeedJumping","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,null,null,null,null,null,["isPVPMap?"],true,null,null,null,null,null,null,null,null,"SpeedJumping"],
      ["Movement","Control",10,1,0.75,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,null,null,null,null,null,["isPVPMap?"],true,null,null,null,null,null,null,null,null,"Ones"],
      ["Movement","Friction",2,1,0.75,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,null,null,null,null,null,["isPVPMap?"],true,null,null,null,null,null,null,null,null,"Ones"],
      ["Movement","Run",0.4,1,0.75,"Melee_SpeedRunning","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,null,null,null,null,null,["isPVPMap?"],true,null,true,null,null,null,null,null,null,"SpeedRunning"]
    ],
    "targetsAffected": [
      "Self"
    ]
  },
  {
    "name": "Beast Run",
    "internalName": "Prestige_Beast_Run",
    "fullName": "Inherent.Inherent.Prestige_Beast_Run",
    "available": -1,
    "grantedAtLevel": 4,
    "autoIssue": true,
    "free": true,
    "isLocked": true,
    "category": "basic",
    "description": "Show off your bestial nature by using this power.  This power is not as fast as Super Speed, nor will it allow you to jump as well as Super Leap, however it is considerably better than the Fitness powers Swift and Hurdle.",
    "shortHelp": "Toggle: Self +Run Speed, +Jump",
    "icon": "inherent_beastrun.png",
    "powerType": "Toggle",
    "setsModes": [
      "TravelJumpOn"
    ],
    "modesDisallowed": [
      "Disable_Travel",
      "Peacebringer_Blaster_Mode",
      "Peacebringer_Tanker_Mode",
      "Warshade_Blaster_Mode",
      "Warshade_Tanker_Mode"
    ],
    "targetType": "Self",
    "requires": [
      "AnimalPack",
      "auth>",
      "cucpapbr",
      "productOwned?",
      "||"
    ],
    "maxSlots": 0,
    "allowedEnhancements": [],
    "stats": {
      "accuracy": 1,
      "endurance": 0.2275,
      "activatePeriod": 0.5
    },
    "effectArea": "SingleTarget",
    "atoms": [
      ["Meta",null,1,122,999999,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones",null,"set_mode",null,null,null,null,null,null,"OnActivate"],
      ["Movement","JumpHeight",0.25,1,0.75,"Melee_Leap","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,null,null,null,null,null,["isPVPMap?","!"],null,null,true,null,null,null,null,null,null,"Leap"],
      ["Movement","Jump",0.55,1,0.75,"Melee_SpeedJumping","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,null,null,null,null,null,["isPVPMap?","!"],null,null,true,null,null,null,null,null,null,"SpeedJumping"],
      ["Movement","Control",10,1,0.75,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,null,null,null,null,null,["isPVPMap?","!"],null,null,true,null,null,null,null,null,null,"Ones"],
      ["Movement","Friction",2,1,0.75,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,null,null,null,null,null,["isPVPMap?","!"],null,null,true,null,null,null,null,null,null,"Ones"],
      ["Movement","Run",0.4,1,0.75,"Melee_SpeedRunning","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,null,null,null,null,null,["isPVPMap?","!"],null,null,true,null,null,null,null,null,null,"SpeedRunning"],
      ["Meta",null,1,1,0.75,"Melee_Ones","Cur","Constant","Self","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones",null,"beast_run",null,null,null,null,null,["Held","Stunned","Sleep"]],
      ["Movement","JumpHeight",0.25,1,0.75,"Melee_Leap","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,null,null,null,null,null,["isPVPMap?"],true,null,null,null,null,null,null,null,null,"Leap"],
      ["Movement","Jump",0.55,1,0.75,"Melee_SpeedJumping","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,null,null,null,null,null,["isPVPMap?"],true,null,null,null,null,null,null,null,null,"SpeedJumping"],
      ["Movement","Control",10,1,0.75,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,null,null,null,null,null,["isPVPMap?"],true,null,null,null,null,null,null,null,null,"Ones"],
      ["Movement","Friction",2,1,0.75,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,null,null,null,null,null,["isPVPMap?"],true,null,null,null,null,null,null,null,null,"Ones"],
      ["Movement","Run",0.4,1,0.75,"Melee_SpeedRunning","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,null,null,null,null,null,["isPVPMap?"],true,null,true,null,null,null,null,null,null,"SpeedRunning"]
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
    "modesDisallowed": [
      "Peacebringer_Blaster_Mode",
      "Peacebringer_Tanker_Mode",
      "Warshade_Blaster_Mode",
      "Warshade_Tanker_Mode"
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
      "Leaping",
      "Running"
    ],
    "stats": {
      "accuracy": 1,
      "endurance": 0.1462,
      "activatePeriod": 0.5
    },
    "effectArea": "SingleTarget",
    "atoms": [
      ["Movement","Run",0.5,1,0.8,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones"],
      ["Movement","Run",0.5,1,0.75,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones"],
      ["Movement","JumpHeight",0.1,1,0.75,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones"],
      ["Meta",null,1,1,0.8,"Melee_Ones","Cur","Constant","Self","Any",false,"Replace",2,null,1,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones",null,"glide",null,null,null,null,null,["Held","Stunned","Sleep"]]
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
    "description": "Sprint allows you to travel, or run away, slightly faster than normal, while slightly draining your Endurance.",
    "shortHelp": "Boost Run SPD",
    "icon": "inherent_sprint.png",
    "powerType": "Toggle",
    "modesDisallowed": [
      "Peacebringer_Blaster_Mode",
      "Peacebringer_Tanker_Mode",
      "Warshade_Blaster_Mode",
      "Warshade_Tanker_Mode"
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
      "Leaping",
      "Running"
    ],
    "stats": {
      "accuracy": 1,
      "endurance": 0.1462,
      "activatePeriod": 0.5
    },
    "effectArea": "SingleTarget",
    "atoms": [
      ["Movement","Run",0.5,1,0.8,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones"],
      ["Movement","Run",0.5,1,0.75,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones"],
      ["Movement","JumpHeight",0.1,1,0.75,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones"]
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
    "description": "Sprint allows you to travel, or run away, slightly faster than normal, while slightly draining your Endurance.",
    "shortHelp": "Boost Run SPD",
    "icon": "inherent_sprint.png",
    "powerType": "Toggle",
    "modesDisallowed": [
      "Peacebringer_Blaster_Mode",
      "Peacebringer_Tanker_Mode",
      "Warshade_Blaster_Mode",
      "Warshade_Tanker_Mode"
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
      "Leaping",
      "Running"
    ],
    "stats": {
      "accuracy": 1,
      "endurance": 0.1462,
      "activatePeriod": 0.5
    },
    "effectArea": "SingleTarget",
    "atoms": [
      ["Movement","Run",0.5,1,0.8,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones"],
      ["Movement","Run",0.5,1,0.75,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones"],
      ["Movement","JumpHeight",0.1,1,0.75,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones"]
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
    "description": "Sprint allows you to travel, or run away, slightly faster than normal, while slightly draining your Endurance.",
    "shortHelp": "Boost Run SPD",
    "icon": "inherent_sprint.png",
    "powerType": "Toggle",
    "modesDisallowed": [
      "Peacebringer_Blaster_Mode",
      "Peacebringer_Tanker_Mode",
      "Warshade_Blaster_Mode",
      "Warshade_Tanker_Mode"
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
      "Leaping",
      "Running"
    ],
    "stats": {
      "accuracy": 1,
      "endurance": 0.1462,
      "activatePeriod": 0.5
    },
    "effectArea": "SingleTarget",
    "atoms": [
      ["Movement","Run",0.5,1,0.8,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones"],
      ["Movement","Run",0.5,1,0.75,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones"],
      ["Movement","JumpHeight",0.1,1,0.75,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones"]
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
    "description": "Sprint allows you to travel, or run away, slightly faster than normal, while slightly draining your Endurance.",
    "shortHelp": "Boost Run SPD",
    "icon": "inherent_sprint.png",
    "powerType": "Toggle",
    "modesDisallowed": [
      "Peacebringer_Blaster_Mode",
      "Peacebringer_Tanker_Mode",
      "Warshade_Blaster_Mode",
      "Warshade_Tanker_Mode"
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
      "Leaping",
      "Running"
    ],
    "stats": {
      "accuracy": 1,
      "endurance": 0.1462,
      "activatePeriod": 0.5
    },
    "effectArea": "SingleTarget",
    "atoms": [
      ["Movement","Run",0.5,1,0.8,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones"],
      ["Movement","Run",0.5,1,0.75,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones"],
      ["Movement","JumpHeight",0.1,1,0.75,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"Ones"]
    ],
    "targetsAffected": [
      "Self"
    ]
  }
];
