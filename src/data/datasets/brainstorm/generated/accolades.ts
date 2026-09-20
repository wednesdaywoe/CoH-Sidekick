/**
 * Accolades powerset — AUTO-GENERATED, DO NOT EDIT.
 *
 * The Temporary_Powers.Accolades members, extracted from
 * exported_powers/<ds>/temporary_powers/accolades/ as ordinary auto-on/gated Powers.
 * Regenerate: node scripts/convert-accolades.cjs --dataset brainstorm
 *
 * Powers: 28, atoms: 122
 */

export const ACCOLADES_POWERSET = {
  "id": "Accolades",
  "setPath": "Temporary_Powers.Accolades",
  "name": "Accolades",
  "archetype": "accolade",
  "category": "accolade",
  "powers": [
    {
      "name": "The Atlas Medallion",
      "internalName": "The_Atlas_Medallion",
      "fullName": "Temporary_Powers.Accolades.The_Atlas_Medallion",
      "available": 0,
      "autoIssue": false,
      "free": true,
      "description": "Earning The Atlas Medallion has granted you a permanent increase of +5 to your Max Endurance.",
      "shortHelp": "+Max END",
      "icon": "ba_atlas_medallions.png",
      "powerType": "Auto",
      "modesDisallowed": [
        "Arena"
      ],
      "targetType": "Self",
      "activateRequires": [
        "type",
        "char>",
        "hero",
        "eq"
      ],
      "allowedEnhancements": [],
      "stats": {
        "accuracy": 1,
        "activatePeriod": 10
      },
      "effectArea": "SingleTarget",
      "atoms": [
        ["MaxEndurance",null,5,1,10.75,"Melee_Ones","Max","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true]
      ],
      "targetsAffected": [
        "Self"
      ]
    },
    {
      "name": "Super Patriot",
      "internalName": "Super_Patriot",
      "fullName": "Temporary_Powers.Accolades.Super_Patriot",
      "available": 0,
      "autoIssue": false,
      "free": true,
      "description": "As a Super Patriot, you have been granted a permanent increase to your Max Endurance by 5%.",
      "shortHelp": "+Max END",
      "icon": "ba_super_patriot.png",
      "powerType": "Auto",
      "modesDisallowed": [
        "Arena"
      ],
      "targetType": "Self",
      "activateRequires": [
        "type",
        "char>",
        "hero",
        "eq"
      ],
      "allowedEnhancements": [],
      "stats": {
        "accuracy": 1,
        "activatePeriod": 10
      },
      "effectArea": "SingleTarget",
      "atoms": [
        ["MaxEndurance",null,5,1,10.75,"Melee_Ones","Max","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true]
      ],
      "targetsAffected": [
        "Self"
      ]
    },
    {
      "name": "Freedom Phalanx Reserve",
      "internalName": "Freedom_Phalanx_Reserve",
      "fullName": "Temporary_Powers.Accolades.Freedom_Phalanx_Reserve",
      "available": 0,
      "autoIssue": false,
      "free": true,
      "description": "Earning Membership into the Freedom Phalanx Reserve has granted you a permanent increase to your Max Hit Points by 10%.",
      "shortHelp": "+Max HP",
      "icon": "ba_phalanx_reserve.png",
      "powerType": "Auto",
      "modesDisallowed": [
        "Arena"
      ],
      "targetType": "Self",
      "activateRequires": [
        "type",
        "char>",
        "hero",
        "eq"
      ],
      "allowedEnhancements": [],
      "stats": {
        "accuracy": 1,
        "activatePeriod": 10
      },
      "effectArea": "SingleTarget",
      "atoms": [
        ["MaxHP",null,1,1,10.75,"Melee_HealSelf","Max","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true]
      ],
      "targetsAffected": [
        "Self"
      ]
    },
    {
      "name": "Task Force Commander",
      "internalName": "Task_Force_Commander",
      "fullName": "Temporary_Powers.Accolades.Task_Force_Commander",
      "available": 0,
      "autoIssue": false,
      "free": true,
      "description": "As a Task Force Commander, you have been granted a permanent increase to your Max Hit Points by 5%.",
      "shortHelp": "+Max HP",
      "icon": "ba_task_force_cmmndr.png",
      "powerType": "Auto",
      "modesDisallowed": [
        "Arena"
      ],
      "targetType": "Self",
      "activateRequires": [
        "type",
        "char>",
        "hero",
        "eq"
      ],
      "allowedEnhancements": [],
      "stats": {
        "accuracy": 1,
        "activatePeriod": 10
      },
      "effectArea": "SingleTarget",
      "atoms": [
        ["MaxHP",null,0.5,1,10.75,"Melee_HealSelf","Max","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true]
      ],
      "targetsAffected": [
        "Self"
      ]
    },
    {
      "name": "Portal Jockey",
      "internalName": "Portal_Jockey",
      "fullName": "Temporary_Powers.Accolades.Portal_Jockey",
      "available": 0,
      "autoIssue": false,
      "free": true,
      "description": "As a Portal Jockey, you have been granted a permanent increase to your Max Hit Points and Max Endurance by 5%.",
      "shortHelp": "+Max HP, +Max END",
      "icon": "ba_poortal_jockey.png",
      "powerType": "Auto",
      "modesDisallowed": [
        "Arena"
      ],
      "targetType": "Self",
      "activateRequires": [
        "type",
        "char>",
        "hero",
        "eq"
      ],
      "allowedEnhancements": [],
      "stats": {
        "accuracy": 1,
        "activatePeriod": 10
      },
      "effectArea": "SingleTarget",
      "atoms": [
        ["MaxHP",null,0.5,1,10.75,"Melee_HealSelf","Max","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["MaxEndurance",null,5,1,10.75,"Melee_Ones","Max","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true]
      ],
      "targetsAffected": [
        "Self"
      ]
    },
    {
      "name": "Crey CBX-9 Pistol",
      "internalName": "Crey_CBX-9_Pistol",
      "fullName": "Temporary_Powers.Accolades.Crey_CBX-9_Pistol",
      "available": 0,
      "autoIssue": false,
      "free": true,
      "description": "The Crey CBX-9 Pistol Immobilizes your target in an icy trap. Deals some damage over time and slightly Slows the target's attack and movement speed even if they break free from the Immobilization. It can also cause flying targets to be grounded. The Crey Cryo Pistol is extremely accurate, but it can only fire once every 10 minutes.\n\nNotes:\nCrey CBX-9 Pistol is unaffected by Recharge Time changes.\n\nDamage: Light.\nRecharge: Very Long.",
      "shortHelp": "Ranged, Moderate DoT(Cold), Foe Immobilize, -SPD, -Recharge, -Fly",
      "icon": "ba_crey_pistol.png",
      "powerType": "Click",
      "modesDisallowed": [
        "Arena"
      ],
      "targetType": "Foe",
      "allowedEnhancements": [],
      "stats": {
        "accuracy": 1.5,
        "range": 80,
        "recharge": 600,
        "castTime": 2.33
      },
      "effectArea": "SingleTarget",
      "damage": {
        "type": "Cold",
        "scale": 0.2,
        "table": "Ranged_Tempdamage",
        "duration": 9.2,
        "tickRate": 2
      },
      "atoms": [
        ["Damage","Cold",0.2,1,9.2,"Ranged_Tempdamage","Abs","Magnitude","Target","Any",true,"Stack",2,null,2,1],
        ["Movement","Run",0.3,1,18,"Ranged_Slow","Cur","Magnitude","Target","Any",true,"Stack",2,null,null,1],
        ["Movement","Fly",0.3,1,18,"Ranged_Slow","Cur","Magnitude","Target","Any",true,"Stack",2,null,null,1],
        ["RechargeTime",null,0.2,1,18,"Ranged_Slow","Str","Magnitude","Target","Any",true,"Stack",2,null,null,1,null,true],
        ["MezResist","Knockup",100,1,15,"Ranged_Ones","Res","Magnitude","Target","Any",false,"Stack",2,null,null,1],
        ["MezResist","Knockback",100,1,15,"Ranged_Ones","Res","Magnitude","Target","Any",false,"Stack",2,null,null,1],
        ["Movement","FlyMode",-1.6,1,15,"Ranged_Ones","Cur","Magnitude","Target","Any",false,"Stack",2,null,null,1],
        ["RechargePower",null,1,1,0,"Ranged_Ones","Abs","Magnitude","Self","Any",false,"Stack",2,null,null,1,null,true],
        ["Mez","Immobilized",15,3,0,"Ranged_Immobilize","Cur","Duration","Target","Any",true,"Stack",2,null,null,1,null,null,null,null,null,null,["enttype","target>","critter","eq"]],
        ["Mez","Immobilized",1,3,0,"Ranged_PvPMez","Cur","Duration","Target","Any",true,"Stack",2,null,null,1,null,null,null,null,null,null,["enttype","target>","player","eq"],true]
      ],
      "targetsAffected": [
        "Foe"
      ]
    },
    {
      "name": "Eye of the Magus",
      "internalName": "Eye_of_the_Magus",
      "fullName": "Temporary_Powers.Accolades.Eye_of_the_Magus",
      "available": 0,
      "autoIssue": false,
      "free": true,
      "description": "When you activate the Eye of the Magus, you are granted a very high bonus to Defense and Damage Resistance for one minute.\n\nNotes:\nEye of the Magus is unaffected by Recharge Time changes.\n\nRecharge: Very Long.",
      "shortHelp": "Self, +Res(All but Psionics), +DEF(All but Psionics)",
      "icon": "ba_eye_of_the_magus.png",
      "powerType": "Click",
      "modesDisallowed": [
        "Arena"
      ],
      "targetType": "Self",
      "allowedEnhancements": [],
      "stats": {
        "accuracy": 1,
        "recharge": 600,
        "castTime": 0.67
      },
      "effectArea": "SingleTarget",
      "atoms": [
        ["Resistance","Smashing",0.3,1,60,"Melee_Ones","Res","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["Resistance","Lethal",0.3,1,60,"Melee_Ones","Res","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["Resistance","Fire",0.3,1,60,"Melee_Ones","Res","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["Resistance","Cold",0.3,1,60,"Melee_Ones","Res","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["Resistance","Energy",0.3,1,60,"Melee_Ones","Res","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["Resistance","Negative",0.3,1,60,"Melee_Ones","Res","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["Resistance","Toxic",0.3,1,60,"Melee_Ones","Res","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["Defense","Smashing",0.5,1,60,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["Defense","Lethal",0.5,1,60,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["Defense","Fire",0.5,1,60,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["Defense","Cold",0.5,1,60,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["Defense","Energy",0.5,1,60,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["Defense","Negative",0.5,1,60,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["RechargePower",null,1,1,0,"Ranged_Ones","Abs","Magnitude","Self","Any",false,"Stack",2,null,null,1,null,true]
      ],
      "targetsAffected": [
        "Self"
      ]
    },
    {
      "name": "Vanguard Medal",
      "internalName": "Vanguard_Medal",
      "fullName": "Temporary_Powers.Accolades.Vanguard_Medal",
      "available": 0,
      "autoIssue": false,
      "free": true,
      "description": "When activated, your Vanguard Medal will increase the duration of all your Disorient, Hold, Immobilize, Fear, Confuse and Sleep powers for a short time. Knockback distance is also increased. The effect of this boost will last for 1 minute.\n\nNotes:\nVanguard Medal is unaffected by Recharge Time changes.\n\nRecharge: Very Long.",
      "shortHelp": "Self +Special",
      "icon": "ba_vangaurd_medal.png",
      "powerType": "Click",
      "modesDisallowed": [
        "Arena"
      ],
      "targetType": "Self",
      "allowedEnhancements": [],
      "stats": {
        "accuracy": 1,
        "recharge": 600,
        "castTime": 1.17
      },
      "effectArea": "SingleTarget",
      "atoms": [
        ["Enhancement","Confused",0.66,1,60,"Melee_Ones","Str","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["Enhancement","Terrorized",0.66,1,60,"Melee_Ones","Str","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["Enhancement","Held",0.66,1,60,"Melee_Ones","Str","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["Enhancement","Immobilized",0.66,1,60,"Melee_Ones","Str","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["Enhancement","Stunned",0.66,1,60,"Melee_Ones","Str","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["Enhancement","Sleep",0.66,1,60,"Melee_Ones","Str","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["RechargePower",null,1,1,0,"Ranged_Ones","Abs","Magnitude","Self","Any",false,"Stack",2,null,null,1,null,true]
      ],
      "targetsAffected": [
        "Self"
      ]
    },
    {
      "name": "Geas of the Kind Ones",
      "internalName": "Geas_of_the_Kind_Ones",
      "fullName": "Temporary_Powers.Accolades.Geas_of_the_Kind_Ones",
      "available": 0,
      "autoIssue": false,
      "free": true,
      "description": "You have uncovered the secrets of Croatoa and have been awarded the Geas of the Kind Ones. Like most supernatural gifts, the Geas is both a blessing and a curse. By using it you can greatly increase your recharge speed, Endurance recovery, and Accuracy for 1 minute. However, your Defense will be severely reduced.\n\nNotes:\nGeas of the Kind Ones is unaffected by Recharge Time changes.\n\nRecharge: Very Long.",
      "shortHelp": "Self +Recovery, +ACC, +Recharge, -DEF",
      "icon": "ba_geas_of_kind_ones.png",
      "powerType": "Click",
      "modesDisallowed": [
        "Arena"
      ],
      "targetType": "Self",
      "allowedEnhancements": [],
      "stats": {
        "accuracy": 1,
        "recharge": 600,
        "castTime": 0.73
      },
      "effectArea": "SingleTarget",
      "atoms": [
        ["Recovery",null,8,1,60,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1],
        ["ToHit",null,0.25,1,60,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1],
        ["RechargeTime",null,1,1,60,"Melee_Ones","Str","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["Defense","All",-0.1,1,60,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["RechargePower",null,1,1,0,"Ranged_Ones","Abs","Magnitude","Self","Any",false,"Stack",2,null,null,1,null,true]
      ],
      "targetsAffected": [
        "Self"
      ]
    },
    {
      "name": "Marshal",
      "internalName": "Marshall",
      "fullName": "Temporary_Powers.Accolades.Marshall",
      "available": 0,
      "autoIssue": false,
      "free": true,
      "description": "You've been awarded the title of Marshal for your service to Arachnos. This has given you a 5% increase to Endurance.",
      "shortHelp": "+Max END",
      "icon": "ba_atlas_medallions.png",
      "powerType": "Auto",
      "modesDisallowed": [
        "Arena"
      ],
      "targetType": "Self",
      "activateRequires": [
        "type",
        "char>",
        "villain",
        "eq"
      ],
      "allowedEnhancements": [],
      "stats": {
        "accuracy": 1,
        "activatePeriod": 10
      },
      "effectArea": "SingleTarget",
      "atoms": [
        ["MaxEndurance",null,5,1,10.75,"Melee_Ones","Max","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true]
      ],
      "targetsAffected": [
        "Self"
      ]
    },
    {
      "name": "High Pain Threshold",
      "internalName": "High_Pain_Threshold",
      "fullName": "Temporary_Powers.Accolades.High_Pain_Threshold",
      "available": 0,
      "autoIssue": false,
      "free": true,
      "description": "You've got an incredibly High Pain Threshold, an after effect of which is that your Hit Points are 10%",
      "shortHelp": "+Max HP",
      "icon": "ba_phalanx_reserve.png",
      "powerType": "Auto",
      "modesDisallowed": [
        "Arena"
      ],
      "targetType": "Self",
      "activateRequires": [
        "type",
        "char>",
        "villain",
        "eq"
      ],
      "allowedEnhancements": [],
      "stats": {
        "accuracy": 1,
        "activatePeriod": 10
      },
      "effectArea": "SingleTarget",
      "atoms": [
        ["MaxHP",null,1,1,10.75,"Melee_HealSelf","Max","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true]
      ],
      "targetsAffected": [
        "Self"
      ]
    },
    {
      "name": "Born In Battle",
      "internalName": "Born_In_Battle",
      "fullName": "Temporary_Powers.Accolades.Born_In_Battle",
      "available": 0,
      "autoIssue": false,
      "free": true,
      "description": "You've proved yourself as Born in Battle, adding 5% to both your Endurance and Hit Point totals.",
      "shortHelp": "+Max HP, +Max END",
      "icon": "ba_poortal_jockey.png",
      "powerType": "Auto",
      "modesDisallowed": [
        "Arena"
      ],
      "targetType": "Self",
      "activateRequires": [
        "type",
        "char>",
        "villain",
        "eq"
      ],
      "allowedEnhancements": [],
      "stats": {
        "accuracy": 1,
        "activatePeriod": 10
      },
      "effectArea": "SingleTarget",
      "atoms": [
        ["MaxHP",null,0.5,1,10.75,"Melee_HealSelf","Max","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["MaxEndurance",null,5,1,10.75,"Melee_Ones","Max","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true]
      ],
      "targetsAffected": [
        "Self"
      ]
    },
    {
      "name": "Stolen Immobilizer Ray",
      "internalName": "Stolen_Immobilizer_Ray",
      "fullName": "Temporary_Powers.Accolades.Stolen_Immobilizer_Ray",
      "available": 0,
      "autoIssue": false,
      "free": true,
      "description": "In your crime spree you have acquired an Immobilizer Ray. You're not sure where it came from, but it sure is useful for stopping foes in their tracks. It can even cause flying targets to be grounded!\n\nNotes:\nStolen Immobilizer Ray is unaffected by Recharge Time changes.\n\nDamage: Light.\nRecharge: Very Long.",
      "shortHelp": "Ranged, Moderate DoT(Energy), Foe Immobilize, -SPD, -Recharge, -Fly",
      "icon": "ba_crey_pistol.png",
      "powerType": "Click",
      "modesDisallowed": [
        "Arena"
      ],
      "targetType": "Foe",
      "allowedEnhancements": [],
      "stats": {
        "accuracy": 1.5,
        "range": 80,
        "recharge": 600,
        "castTime": 1.87
      },
      "effectArea": "SingleTarget",
      "damage": {
        "type": "Energy",
        "scale": 0.2,
        "table": "Ranged_Tempdamage",
        "duration": 9.2,
        "tickRate": 2
      },
      "atoms": [
        ["Damage","Energy",0.2,1,9.2,"Ranged_Tempdamage","Abs","Magnitude","Target","Any",true,"Stack",2,null,2,1],
        ["Movement","Run",0.3,1,18,"Ranged_Slow","Cur","Magnitude","Target","Any",true,"Stack",2,null,null,1],
        ["Movement","Fly",0.3,1,18,"Ranged_Slow","Cur","Magnitude","Target","Any",true,"Stack",2,null,null,1],
        ["RechargeTime",null,0.2,1,18,"Ranged_Slow","Str","Magnitude","Target","Any",true,"Stack",2,null,null,1,null,true],
        ["MezResist","Knockup",100,1,15,"Ranged_Ones","Res","Magnitude","Target","Any",false,"Stack",2,null,null,1],
        ["MezResist","Knockback",100,1,15,"Ranged_Ones","Res","Magnitude","Target","Any",false,"Stack",2,null,null,1],
        ["Movement","FlyMode",-1.6,1,15,"Ranged_Ones","Cur","Magnitude","Target","Any",false,"Stack",2,null,null,1],
        ["RechargePower",null,1,1,0,"Ranged_Ones","Abs","Magnitude","Self","Any",false,"Stack",2,null,null,1,null,true],
        ["Mez","Immobilized",15,3,0,"Ranged_Immobilize","Cur","Duration","Target","Any",true,"Stack",2,null,null,1,null,null,null,null,null,null,["enttype","target>","critter","eq"]],
        ["Mez","Immobilized",1,3,0,"Ranged_PvPMez","Cur","Duration","Target","Any",true,"Stack",2,null,null,1,null,null,null,null,null,null,["enttype","target>","player","eq"],true]
      ],
      "targetsAffected": [
        "Foe"
      ]
    },
    {
      "name": "Demonic Aura",
      "internalName": "Demonic_Aura",
      "fullName": "Temporary_Powers.Accolades.Demonic_Aura",
      "available": 0,
      "autoIssue": false,
      "free": true,
      "description": "The spirit of a demon resides within you. Bringing it to the surface of your mind can make you highly resistant to all types of damage for a short time. You mustn't let the demon inside you out for too long, however, or it just may cost you your mortal soul.\n\nNotes:\nDemonic Aura is unaffected by Recharge Time changes.\n\nRecharge: Very Long.",
      "shortHelp": "Self, +Res(All but Psionics), +DEF(All but Psionics)",
      "icon": "ba_eye_of_the_magus.png",
      "powerType": "Click",
      "modesDisallowed": [
        "Arena"
      ],
      "targetType": "Self",
      "allowedEnhancements": [],
      "stats": {
        "accuracy": 1,
        "recharge": 600,
        "castTime": 3
      },
      "effectArea": "SingleTarget",
      "atoms": [
        ["Resistance","Smashing",0.3,1,60,"Melee_Ones","Res","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["Resistance","Lethal",0.3,1,60,"Melee_Ones","Res","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["Resistance","Fire",0.3,1,60,"Melee_Ones","Res","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["Resistance","Cold",0.3,1,60,"Melee_Ones","Res","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["Resistance","Energy",0.3,1,60,"Melee_Ones","Res","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["Resistance","Negative",0.3,1,60,"Melee_Ones","Res","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["Resistance","Toxic",0.3,1,60,"Melee_Ones","Res","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["Defense","Smashing",0.5,1,60,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["Defense","Lethal",0.5,1,60,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["Defense","Fire",0.5,1,60,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["Defense","Cold",0.5,1,60,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["Defense","Energy",0.5,1,60,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["Defense","Negative",0.5,1,60,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["RechargePower",null,1,1,0,"Ranged_Ones","Abs","Magnitude","Self","Any",false,"Stack",2,null,null,1,null,true]
      ],
      "targetsAffected": [
        "Self"
      ]
    },
    {
      "name": "Megalomaniac",
      "internalName": "Megalomaniac",
      "fullName": "Temporary_Powers.Accolades.Megalomaniac",
      "available": 0,
      "autoIssue": false,
      "free": true,
      "description": "You've wanted more control and now you have it. Activating this power increases the power of all of your Sleeps, Holds, Immobilize, and Confuse for 60 seconds.\n\nNotes:\nMegalomaniac is unaffected by Recharge Time changes.\n\nRecharge: Very Long.",
      "shortHelp": "Self +Special",
      "icon": "ba_vangaurd_medal.png",
      "powerType": "Click",
      "modesDisallowed": [
        "Arena"
      ],
      "targetType": "Self",
      "allowedEnhancements": [],
      "stats": {
        "accuracy": 1,
        "recharge": 600,
        "castTime": 1.17
      },
      "effectArea": "SingleTarget",
      "atoms": [
        ["Enhancement","Confused",0.66,1,60,"Melee_Ones","Str","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["Enhancement","Terrorized",0.66,1,60,"Melee_Ones","Str","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["Enhancement","Held",0.66,1,60,"Melee_Ones","Str","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["Enhancement","Immobilized",0.66,1,60,"Melee_Ones","Str","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["Enhancement","Stunned",0.66,1,60,"Melee_Ones","Str","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["Enhancement","Sleep",0.66,1,60,"Melee_Ones","Str","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["RechargePower",null,1,1,0,"Ranged_Ones","Abs","Magnitude","Self","Any",false,"Stack",2,null,null,1,null,true]
      ],
      "targetsAffected": [
        "Self"
      ]
    },
    {
      "name": "Invader",
      "internalName": "Invader",
      "fullName": "Temporary_Powers.Accolades.Invader",
      "available": 0,
      "autoIssue": false,
      "free": true,
      "description": "As a Task Force Commander, you have been granted a permanent increase to your Max Hit Points by 5%.",
      "shortHelp": "+Max HP",
      "icon": "ba_task_force_cmmndr.png",
      "powerType": "Auto",
      "modesDisallowed": [
        "Arena"
      ],
      "targetType": "Self",
      "activateRequires": [
        "type",
        "char>",
        "villain",
        "eq"
      ],
      "allowedEnhancements": [],
      "stats": {
        "accuracy": 1,
        "activatePeriod": 10
      },
      "effectArea": "SingleTarget",
      "atoms": [
        ["MaxHP",null,0.5,1,10.75,"Melee_HealSelf","Max","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true]
      ],
      "targetsAffected": [
        "Self"
      ]
    },
    {
      "name": "Force of Nature",
      "internalName": "Force_of_Nature",
      "fullName": "Temporary_Powers.Accolades.Force_of_Nature",
      "available": 0,
      "autoIssue": false,
      "free": true,
      "description": "You have shown yourself as a true Force of Nature, and been bestowed with the fury of the elements. Like most supernatural gifts, the Force of Nature is both a blessing and a curse. By using it you can greatly increase your recharge speed, Endurance recovery, and Accuracy for 1 minute. However, your Defense will be severely reduced.\n\nNotes:\nForce of Nature is unaffected by Recharge Time changes.\n\nRecharge: Very Long.",
      "shortHelp": "Self +Recovery, +ACC, +Recharge, -DEF",
      "icon": "ba_geas_of_kind_ones.png",
      "powerType": "Click",
      "modesDisallowed": [
        "Arena"
      ],
      "targetType": "Self",
      "allowedEnhancements": [],
      "stats": {
        "accuracy": 1,
        "recharge": 600,
        "castTime": 0.73
      },
      "effectArea": "SingleTarget",
      "atoms": [
        ["Recovery",null,8,1,60,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1],
        ["ToHit",null,0.25,1,60,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1],
        ["RechargeTime",null,1,1,60,"Melee_Ones","Str","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["Defense","All",-0.1,1,60,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["RechargePower",null,1,1,0,"Ranged_Ones","Abs","Magnitude","Self","Any",false,"Stack",2,null,null,1,null,true]
      ],
      "targetsAffected": [
        "Self"
      ]
    },
    {
      "name": "Iron Man",
      "internalName": "Iron_Man",
      "fullName": "Temporary_Powers.Accolades.Iron_Man",
      "available": 0,
      "autoIssue": false,
      "free": true,
      "description": "You are a true Iron Man. You persistence and stamina have resulted in your Max Hit Points and Max Endurance increasing by 10%.",
      "shortHelp": "+Max HP, +Max END",
      "icon": "ba_poortal_jockey.png",
      "powerType": "Auto",
      "modesDisallowed": [
        "Arena"
      ],
      "targetType": "Self",
      "activateRequires": [
        "type",
        "char>",
        "villain",
        "eq"
      ],
      "allowedEnhancements": [],
      "stats": {
        "accuracy": 1,
        "activatePeriod": 10
      },
      "effectArea": "SingleTarget",
      "atoms": [
        ["MaxHP",null,1,1,10.75,"Melee_HealSelf","Max","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["MaxEndurance",null,10,1,10.75,"Melee_Ones","Max","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true]
      ],
      "targetsAffected": [
        "Self"
      ]
    },
    {
      "name": "Portable Workbench",
      "internalName": "Portable_Workbench",
      "fullName": "Temporary_Powers.Accolades.Portable_Workbench",
      "available": 0,
      "autoIssue": false,
      "free": true,
      "description": "You can use this power to summon a Portable Workbench, which will allow anyone nearby to craft recipes. The Portable Workbench lasts 5 minutes when summoned and can be summoned every 5 minutes. You may not summon the Portable Workbench in PvP enabled areas.",
      "shortHelp": "Summon Portable Workbench",
      "icon": "veteran_summonpet.png",
      "powerType": "Auto",
      "targetType": "Self",
      "requires": [
        "0"
      ],
      "allowedEnhancements": [],
      "stats": {
        "accuracy": 1,
        "activatePeriod": 10
      },
      "effectArea": "SingleTarget",
      "atoms": [
        ["Meta",null,1,1,0,"Melee_Ones","Abs","Magnitude","Self","Any",false,"Ignore",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"revoke_power",null,1],
        ["GrantPower",null,1,1,0,"Ranged_Ones","Abs","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true]
      ],
      "grantEdges": [
        {
          "op": "revoke",
          "path": "Temporary_Powers.Accolades.Portable_Workbench",
          "count": 1,
          "delaySeconds": 1
        },
        {
          "op": "grant",
          "path": "Prestige.Prestige_Utility.Portable_Workbench",
          "count": 1,
          "maxCount": 1
        }
      ],
      "targetsAffected": [
        "Self"
      ]
    },
    {
      "name": "Elusive Mind",
      "internalName": "RIWE_Accolade_Power",
      "fullName": "Temporary_Powers.Accolades.RIWE_Accolade_Power",
      "available": 0,
      "autoIssue": false,
      "free": true,
      "description": "You have gained the ability to deflect and protect yourself from mental invasions of all sorts. By focusing your mind using this technique, you gain strong defense against Psionic attacks and are moderately resistant to Psionic damage while this power is active. It lasts for 60 seconds per use.\n\nNotes:\nElusive Mind is unaffected by Recharge Time changes.\n\nRecharge: Very Long.",
      "shortHelp": "Moderate Psionic Resistance",
      "icon": "ba_elusivemind.png",
      "powerType": "Click",
      "modesDisallowed": [
        "Arena"
      ],
      "targetType": "Self",
      "allowedEnhancements": [],
      "stats": {
        "accuracy": 1,
        "recharge": 600,
        "castTime": 0.73
      },
      "effectArea": "SingleTarget",
      "atoms": [
        ["Resistance","Psionic",0.075,1,60,"Melee_Ones","Res","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["Defense","Psionic",0.25,1,60,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["RechargePower",null,1,1,0,"Ranged_Ones","Abs","Magnitude","Self","Any",false,"Stack",2,null,null,1,null,true]
      ],
      "targetsAffected": [
        "Self"
      ]
    },
    {
      "name": "Long Range Teleporter",
      "internalName": "Long_Range_Teleport",
      "fullName": "Temporary_Powers.Accolades.Long_Range_Teleport",
      "available": 0,
      "autoIssue": false,
      "free": true,
      "description": "The Long Range Teleporter allows you to teleport very long distances, even across city zones and to Supergroup bases. Collecting any Exploration badge in a zone will unlock it as a destination for your Long Range Teleporter.\n\nNotes: Long Range Teleporter is unaffected by Recharge Time changes.\n\nRecharge: Very Long.",
      "shortHelp": "Teleport to Zone or Base",
      "icon": "accolade_longrangeteleport.png",
      "powerType": "Click",
      "targetType": "Self",
      "activateRequires": [
        "isTutorialMap?",
        "!"
      ],
      "allowedEnhancements": [],
      "stats": {
        "accuracy": 1,
        "recharge": 600,
        "castTime": 2
      },
      "effectArea": "SingleTarget",
      "atoms": [
        ["Stealth","Translucency",0,1,5,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Stack",2,null,null,1,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,["Self"]],
        ["Mez","Teleport",1,1,0,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Stack",2,null,null,1,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,["Self"],1.17],
        ["GrantPower",null,1,1,0,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,["Self"],0.05],
        ["RechargePower",null,1,1,0,"Ranged_Ones","Abs","Magnitude","Self","Any",false,"Stack",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,["Self"],2.05],
        ["Meta",null,1,1,0,"Ranged_Ones","Abs","Constant","Target","Any",true,"Stack",2,null,null,1,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"token_add",["Self"]]
      ],
      "targetsAffected": [
        "Self"
      ]
    },
    {
      "name": "Mark and Recall",
      "internalName": "MarkRecall",
      "fullName": "Temporary_Powers.Accolades.MarkRecall",
      "available": 0,
      "autoIssue": false,
      "free": true,
      "description": "Activating this power will bind a Recall Marker to your current location, which will then be marked on your map with an icon.\n\nWhen this power is deactivated, you will be teleported and returned to the location of your Recall Marker.\n\nNotes:\nMark and Recall is unaffected by Recharge Time changes. Cannot be activated on Incarnate Trials or in certain mission maps.\n\nRecharge: Very Long.",
      "shortHelp": "Special: Set Teleport",
      "icon": "accolade_mark.png",
      "powerType": "Toggle",
      "setsModes": [
        "MarkPlaced"
      ],
      "modesDisallowed": [
        "Arena"
      ],
      "targetType": "Location",
      "activateRequires": [
        "NoRecall",
        "inVolume>",
        "!",
        "Accolades.NoRecall",
        "ScriptMessage>",
        "!",
        "&&",
        "Axp",
        "inVolume>",
        "!",
        "&&",
        "Bxp",
        "inVolume>",
        "!",
        "&&",
        "ABxp",
        "inVolume>",
        "!",
        "&&",
        "Cxp",
        "inVolume>",
        "!",
        "&&"
      ],
      "allowedEnhancements": [],
      "stats": {
        "accuracy": 1,
        "range": 10,
        "recharge": 600,
        "activatePeriod": 1
      },
      "effectArea": "Location",
      "summon": {
        "isPseudoPet": false,
        "entity": "Pets_Recall_Marker",
        "duration": 99999
      },
      "atoms": [
        ["EntCreate",null,-1,1,99999,"Ranged_Ones","Cur","Magnitude","Self","Any",false,"Ignore",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"OnActivate",99999],
        ["Meta",null,1,171,1.5,"Ranged_Ones","Cur","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"set_mode"],
        ["Meta",null,1,1,0,"Melee_Damage","Cur","Magnitude","Target","Any",true,"Ignore",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"script_notify"]
      ],
      "targetsAffected": [
        "Self"
      ]
    },
    {
      "name": "Sheer Willpower",
      "internalName": "SFC_Accolade_Power",
      "fullName": "Temporary_Powers.Accolades.SFC_Accolade_Power",
      "available": 0,
      "autoIssue": false,
      "free": true,
      "description": "Whether through spiteful rebellion or heroic perseverence, you push on despite the odds.\n\nFrees you from many Sleep, Hold, Immobilization, Disorient, Fear, Confuse and KnockBack effects and boosts your resistance to Repel, Taunt and Placate effects for 30 seconds. This power can be used even while under such effects.",
      "shortHelp": "Resist Effects",
      "icon": "accolade_sheerwillpower.png",
      "powerType": "Click",
      "modesDisallowed": [
        "Arena"
      ],
      "targetType": "Self",
      "allowedEnhancements": [],
      "stats": {
        "accuracy": 1,
        "recharge": 600
      },
      "effectArea": "SingleTarget",
      "atoms": [
        ["Mez","Knockup",-7.5,1,30,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Stack",2,null,null,1,null,true],
        ["Mez","Knockback",-7.5,1,30,"Melee_Ones","Cur","Magnitude","Self","Any",false,"Stack",2,null,null,1,null,true],
        ["MezResist","Repel",1,1,30,"Melee_Ones","Res","Magnitude","Self","Any",false,"Stack",2,null,null,1,null,true],
        ["MezResist","Taunt",0.5,1,30,"Melee_Ones","Res","Magnitude","Self","Any",false,"Stack",2,null,null,1,null,true],
        ["MezResist","Placate",0.5,1,30,"Melee_Ones","Res","Magnitude","Self","Any",false,"Stack",2,null,null,1,null,true],
        ["Mez","Confused",-15,1,30,"Melee_Ones","Cur","Magnitude","Self","PvE",false,"Stack",2,null,null,1,null,true],
        ["Mez","Terrorized",-15,1,30,"Melee_Ones","Cur","Magnitude","Self","PvE",false,"Stack",2,null,null,1,null,true],
        ["Mez","Held",-15,1,30,"Melee_Ones","Cur","Magnitude","Self","PvE",false,"Stack",2,null,null,1,null,true],
        ["Mez","Immobilized",-15,1,30,"Melee_Ones","Cur","Magnitude","Self","PvE",false,"Stack",2,null,null,1,null,true],
        ["Mez","Stunned",-15,1,30,"Melee_Ones","Cur","Magnitude","Self","PvE",false,"Stack",2,null,null,1,null,true],
        ["Mez","Sleep",-15,1,30,"Melee_Ones","Cur","Magnitude","Self","PvE",false,"Stack",2,null,null,1,null,true],
        ["MezResist","Confused",1.5,1,30,"Melee_Res_Boolean","Res","Magnitude","Self","PvP",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,true],
        ["MezResist","Terrorized",1.5,1,30,"Melee_Res_Boolean","Res","Magnitude","Self","PvP",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,true],
        ["MezResist","Held",1.5,1,30,"Melee_Res_Boolean","Res","Magnitude","Self","PvP",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,true],
        ["MezResist","Immobilized",1.5,1,30,"Melee_Res_Boolean","Res","Magnitude","Self","PvP",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,true],
        ["MezResist","Stunned",1.5,1,30,"Melee_Res_Boolean","Res","Magnitude","Self","PvP",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,true],
        ["MezResist","Sleep",1.5,1,30,"Melee_Res_Boolean","Res","Magnitude","Self","PvP",false,"Replace",2,null,null,1,null,true,null,null,null,null,null,true]
      ],
      "targetsAffected": [
        "Self"
      ]
    },
    {
      "name": "Conqueror of the Labyrinth",
      "internalName": "Labyrinth_Conqueror",
      "fullName": "Temporary_Powers.Accolades.Labyrinth_Conqueror",
      "available": 0,
      "autoIssue": false,
      "free": true,
      "description": "You've mastered the experience of fighting in the strange dream-like corridors of the maze, and as a result have gained increased Hitpoints and Endurance while adventuring within the fog!\n\nCan only be used when fueled by the presence of mythic fog.\n\nMoving through the Fog takes practice, such as learning to assert your mind's calm when the tendrils of malevolence begin to bore into one's thoughts.",
      "shortHelp": "+Max HP, +Max END",
      "icon": "accolade_labyrinthconqueror.png",
      "powerType": "Auto",
      "modesRequired": [
        "InLabyrinth"
      ],
      "targetType": "Self",
      "allowedEnhancements": [],
      "stats": {
        "accuracy": 1,
        "activatePeriod": 10
      },
      "effectArea": "SingleTarget",
      "atoms": [
        ["MaxHP",null,0.5,1,10.75,"Melee_HealSelf","Max","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true],
        ["MaxEndurance",null,5,1,10.75,"Melee_Ones","Max","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true]
      ],
      "targetsAffected": [
        "Self"
      ]
    },
    {
      "name": "Mazebreaker",
      "internalName": "Mazebreaker",
      "fullName": "Temporary_Powers.Accolades.Mazebreaker",
      "available": 0,
      "autoIssue": false,
      "free": true,
      "description": "You've been tested time and time again and emerged triumphant, and as a result have gained increased Endurance while adventuring within the fog!\n\nCan only be used when fueled by the presence of mythic fog.\n\nThe Four were mighty, but their strongest trait was their care for the armies that served under them. Individually instructing every warrior in their service, the Generals were loved and revered by all in the Goddess' service.",
      "shortHelp": "+Max END",
      "icon": "accolade_mazebreaker.png",
      "powerType": "Auto",
      "modesRequired": [
        "InLabyrinth"
      ],
      "targetType": "Self",
      "allowedEnhancements": [],
      "stats": {
        "accuracy": 1,
        "activatePeriod": 10
      },
      "effectArea": "SingleTarget",
      "atoms": [
        ["MaxEndurance",null,5,1,10.75,"Melee_Ones","Max","Magnitude","Self","Any",false,"Replace",2,null,null,1,null,true]
      ],
      "targetsAffected": [
        "Self"
      ]
    },
    {
      "name": "Legacy: Vanguard MDC",
      "internalName": "Challenge_VanguardDummy_Pet",
      "fullName": "Temporary_Powers.Accolades.Challenge_VanguardDummy_Pet",
      "available": 0,
      "autoIssue": false,
      "free": true,
      "description": "This power exists to unlock the Vanguard MDC under the Combat Dummy power category.",
      "shortHelp": "Unlock",
      "icon": "challenge_vanguarddummy_pet.png",
      "powerType": "Auto",
      "targetType": "Self",
      "allowedEnhancements": [],
      "stats": {
        "accuracy": 1
      },
      "effectArea": "SingleTarget",
      "atoms": [
        ["Meta",null,-1,1,1,"Ranged_Ones","Cur","Magnitude","Self","Any",false,"Ignore",2,null,null,1,null,true,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,"null",null,1.5]
      ],
      "targetsAffected": [
        "Self"
      ]
    },
    {
      "name": "Excalibonk",
      "internalName": "Toy_Bat",
      "fullName": "Temporary_Powers.Accolades.Toy_Bat",
      "available": 0,
      "autoIssue": false,
      "free": true,
      "description": "This Rikti Monkey 'chew toy' was one of the many prototypes that the Rikti attempted to fabricate so that their Honoree could wield it to lead them to victory against the Earth.\n\nDue to all of the prototypes being failures they were instead repurposed.\n\nNotes:\nCannot be used in Pocket D.\n\nDamage: Minor Minor.\nRecharge: Very Fast.",
      "shortHelp": "Minor Minor DMG(Special)",
      "icon": "accolade_excalibonk.png",
      "powerType": "Click",
      "modesDisallowed": [
        "Peacebringer_Blaster_Mode",
        "Peacebringer_Tanker_Mode",
        "Warshade_Blaster_Mode",
        "Warshade_Tanker_Mode"
      ],
      "targetType": "Any",
      "activateRequires": [
        "mapname>",
        "City_02_04",
        "eq",
        "!"
      ],
      "allowedEnhancements": [],
      "stats": {
        "accuracy": 1,
        "range": 7,
        "recharge": 3,
        "endurance": 3.25,
        "castTime": 1.83
      },
      "effectArea": "SingleTarget",
      "damage": {
        "type": "Smashing",
        "scale": 0.036,
        "table": "Melee_Tempdamage"
      },
      "atoms": [
        ["Damage","Smashing",0.036,1,0,"Melee_Tempdamage","Abs","Magnitude","Target","Any",true,"Stack",2,null,null,1,null,null,null,null,null,null,["enttype","target>","critter","eq","group","target>","MastermindPets","eq","!","&&","group","target>","Pets","eq","!","&&"]],
        ["Movement","Run",0,1,0,"Melee_Ones","Abs","Magnitude","Target","Any",false,"Stack",2,null,null,1,null,true,null,null,null,null,["enttype","target>","player","eq"],true]
      ],
      "targetsAffected": [
        "Any"
      ]
    },
    {
      "name": "Orb Weaver R-Scanalyzer",
      "internalName": "Scanalyzer",
      "fullName": "Temporary_Powers.Accolades.Scanalyzer",
      "available": 0,
      "autoIssue": false,
      "free": true,
      "description": "The Orb Weaver R-Modulator enables you to gain significant insight into the abilities, strengths and weakness of your foes. It will not function on PVP targets.\n\nArachnos surveillance aparatus are very high-tech, and can be used indefinitely without the need for regular maintenance.\n\nRecharge: Moderate.",
      "shortHelp": "Special",
      "icon": "accolade_weaver_scanalyzer.png",
      "powerType": "Click",
      "modesDisallowed": [
        "Arena"
      ],
      "targetType": "Foe",
      "allowedEnhancements": [],
      "stats": {
        "accuracy": 1,
        "range": 100,
        "recharge": 10,
        "endurance": 6.5,
        "castTime": 1.5
      },
      "effectArea": "SingleTarget",
      "atoms": [
        ["Meta",null,100,1,0,"Ranged_Ones","Cur","Magnitude","Target","Any",true,"Stack",2,null,null,1,null,null,null,null,null,null,["enttype","target>","critter","eq"],null,null,null,null,null,null,null,null,null,null,null,"view_attributes"]
      ],
      "targetsAffected": [
        "Foe"
      ]
    }
  ]
};
