/**
 * Manually-maintained changelog for the WelcomeModal "What's New" list.
  * This is separate from the auto-generated changelog (changelog.ts) which is based on git history.
 */

export interface ManualChangelogGroup {
  date: string; // YYYY-MM-DD
  items: ManualChangelogItem[];
}

export interface ManualChangelogItem {
  /**
   * Stable, permanent identifier — the dedup key for the Discord publisher
   * (scripts/push-changelog-discord.ts). Must be unique across the whole file.
   *
   * Assign one when you add the entry and NEVER change it afterwards: the id is
   * what lets you reword or fix typos in `message` without the publisher treating
   * the entry as new and reposting it. Changing an id reposts the entry.
   */
  id: string;
  message: string;
  type: 'feat' | 'fix' | 'update' | 'known-issue';
}

/** Flat entry used by changelog.ts */
export interface ManualEntry {
  id: string;
  date: string;
  message: string;
  type: 'feat' | 'fix' | 'update' | 'known-issue';
}
/* template
{
    date: '2026-00-00',

    items: [
      { id: 'x-x-x', message: 'y', type: 'fix' },
    ]
  },
*/



/*When ready, npm run changelog:push -- --dry-run
*/
export const MANUAL_CHANGELOG_GROUPS: ManualChangelogGroup[] = [
  // ───────────────────────────────────────────────────────────────────────
  {
    date: '2026-09-26',

    items: [
      { id: 'boost-zero-state', message: 'Guarded Spin shows its melee and lethal defense, along with 13 sibling powers that were reading zero: Parry, Defensive Sweep, Follow Up, Devour Psyche, Shadow Maul and the rest', type: 'fix' },
      { id: 'hide-defense-toggle', message: 'Hide moves defense numbers, and no longer flips Beta Decay on and off', type: 'fix' },
      { id: 'shell-power-recharge', message: 'Suppressive Fire shows each ammunition setting\'s 8-second recharge instead of the shell power\'s 20s', type: 'fix' }, 
    ]
  },
  {
    date: '2026-09-21',
    
    items: [
      { id: 'proc-chance-excludes-global-recharge', message: 'Fix to stop global recharge from touching proc calcs', type: 'fix' },
      { id: 'incarnate-proc-aoe-single-target', message: 'Incarnate proc damage on AoE powers was calculated as if against a single target', type: 'fix' },
      { id: 'proc-description-damage-types', message: 'Fix to remove hand-written proc dmg type labels that put the wrong type on some procs', type: 'fix' },
      { id: 'popmenu-enhancement-names', message: 'The in-game popmenu export wrote the wrong internal name for a few enhancements. The exporter now reads the game\'s own names instead of assembling them.', type: 'fix' },
      { id: 'intangible-io-missing-from-picker', message: 'The Intangible common IO was missing from the enhancement picker. This one only shows on Rebirth and Thunderspy', type: 'fix' },
      { id: 'stale-shell-self-recovers', message: 'If you had a tab open across a deploy, coming back to it could leave the app stuck before it finished loading, and the only way out was a hard refresh (this triggers the "Taking a while to load?" hint). A tab running a version that has been replaced should now throw it away and reload itself.', type: 'fix' },
      { id: 'crash-report-consent', message: 'Crash reporting: there is a switch in Settings and a note saying what a report contains: the error message, where in the code it happened, and the app version. It never contains:  builds, account name, or anything you typed', type: 'feat' },
      { id: 'brainstorm-rc3', message: 'HC Brainstorm is updated to RC3 (September 18), up from the August 20 build. That includes in Light Affinity\'s Radiance rework, Sonic Aura, the Bane Spider and Mu changes, Force Field\'s Repulsion Field revert, and the ATO PPM text.', type: 'update' },
    ]
  },
  {
    date: '2026-09-03',

    items: [
      { id: 'unlisted-build-visibility', message: 'Shared builds have a new "Unlisted" visibility option, between Private and Public. An unlisted build is open to anyone with the link, but it won\'t show up in Shared Builds browse or search.', type: 'feat' },
      { id: 'quick-share-was-secretly-private', message: 'The one-click share link should now create a real unlisted build, so the link works for whoever you send it to.', type: 'fix' },
    ]
  },
  {
    date: '2026-08-26',

    items: [
      { id: 'hidden-mechanic-picks-restored', message: 'Bio Armor\'s Adaptation, Staff Fighting\'s Staff Mastery and Fortunata Teamwork\'s Fate Sealed were missing from the power picker. A recent change meant to stop powers like Seismic Shockwaves from appearing as a power selection accidentally caught some other powers. Existing builds are unaffected; the powers are simply selectable again.', type: 'fix' },
      { id: 'slot-levels-placed-in-reverse', message: 'Fix a bug causing placing enhancement slots one at a time to be labeled in reverse. Saved builds affected by this should heal on load. The order you placed slots in was never lost, so the levels can be reconstructed... however interleaved slotting between powers picked at different levels is the one thing that cannot be rebuilt exactly. The same levels come back, but not necessarily on the same power 😔.', type: 'fix' },
      { id: 'mids-export-fix', message: 'Fix for Mids export utility that should make the build reconstruction more reliable', type: 'fix' },
    ]
  },
  {
    date: '2026-08-22',

    items: [
      { id: 'brainstorm-not-in-server-picker', message: 'HC Brainstorm shipped but was missing from the Server dropdown, so there was no way to select it', type: 'fix' },
      { id: 'hc-brainstorm-dataset', message: 'HC Brainstorm is now a server option. I will try to keep it current but Open Beta moves fast, so please don\'t expect every new beta update to be reflected.', type: 'feat' },
      { id: 'incarnate-level-shift-ceiling', message: 'New Level Shift control in the header, available one your incarnate loadout has earned a shift. This allows you to adjust your level shift without unslotted Incarnate powers', type: 'feat' },
      { id: 'veat-recased-power-keys', message: 'Arachnos Soldiers and Widows: 22 powers were tripping the "not in the Homecoming data" warning. Sidekick had been re-casing those powers\' internal names, so the key your build saved never matched the one the engine looks up and resetting the build or clearing the cache would not fix it', type: 'fix' },
      { id: 'power-description-line-breaks', message: 'Power descriptions were running sentences together where the game puts a line break.', type: 'fix' },
      { id: 'accolade-description-markup', message: 'Accolade descriptions were showing the game\'s own markup as literal text (raw <br> and <color #fcfc95> tags mid-sentence)', type: 'fix' },
      { id: 'brawl-fighting-pool-synergy', message: 'Homecoming and Rebirth: Brawl\'s Fighting-pool synergy was in the game data but nothing could switch it on. Its -recharge/-tohit with Boxing or Kick, and its -regen/-recovery with Cross Punch, now appear as toggles under the power\'s Mechanic Adjusters', type: 'fix' },
      { id: 'geode-spurious-taunt-resistance', message: 'Geode was granting taunt resistance. Two opposite effects share one name in the game data and Sidekick was reading the wrong one', type: 'fix' },
      { id: 'defense-softcap-real-total', message: 'The defense tiles stopped counting at 45.00%. At the softcap the dashboard showed 45.00% no matter how much defense you actually had. It now shows your real total, and the tooltip says how far over you are', type: 'fix' },
      { id: 'info-panel-scrollbar-shift', message: 'The whole app jumped a little to the left and back as you moved the mouse across powers. Longer info panels made the page tall enough to need a scrollbar and short ones did not, so the browser added and removed it under the pointer', type: 'fix' },
    ]
  },
  {
    date: '2026-08-20',

    items: [
      { id: 'synapses-agility-end-drain-global', message: 'Rebirth: Synapse\'s Agility\'s 6th enhancement showed as "Empty" and did nothing when slotted. It now has its real name (Endurance Drain Resistance) and grants its 20% resistance to endurance drain', type: 'fix' },
      { id: 'libertys-belt-blank-category', message: 'Rebirth: Liberty\'s Belt\'s resistance pieces weren\'t enhancing resistance (in Tough or anywhere else). Same underlying bug as the Gladiator\'s Armor fix below', type: 'fix' },
      { id: 'epic-pick-no-free-level-relevel', message: 'Re-picking an epic power with no free pick level at or above its unlock could leave it selected and slotted but invisible in the by-level view. Build now relevels instead, and earlier picks slide into the free low levels so the new power lands and saved builds carrying the broken state should heal on load', type: 'fix' },
      { id: 'bulk-placement-eligibility', message: 'Dragging a range of set pieces (or multi-selecting) could place a piece the picker itself showed as disabled, like slotting a unique ATO piece twice. Both bulk paths now check each piece', type: 'fix' },
      { id: 'winters-gift-icon-pair', message: 'Rebirth: Superior Winter\'s Gift had no icon, and on every server Winter\'s Gift was using the Superior icon', type: 'fix' },
      { id: 'poisoned-allow-list-heal', message: 'Some saved builds rejected every piece of a set the picker was offering The power\'s stored slotting rules had been saved in a broken state and kept overriding the real ones on every load', type: 'fix' },
    ]
  },
  {
    date: '2026-08-19',

    items: [
      { id: 'boost-spinner-apply-to-slotted', message: 'There\'s now an "Apply to slotted" button next to the enhancement boost setting that restamps every eligible slot at once (it respects attuned pieces and catalyzed sets, and undo-able)', type: 'feat' },
      { id: 'blank-category-set-aspects', message: 'Gladiator\'s Armor, and every PvP, purple, event, and ATO set was labelled from a field the game leaves blank, so its pieces exported as damage enhancement and the set\'s real bonuses never reached your totals', type: 'fix' },
      { id: 'form-redirect-dead-branch', message: 'Powers that change form under a mode could show the wrong form when their redirect table also carries a condition the planner can\'t answer (target distance). This froze the whole table. Energy Manipulation\'s Stun is the known case — with Power Boost active it should now show its AoE form (90s recharge) instead of the single-target one', type: 'fix' },
    ]
  },
  {
    date: '2026-08-15',

    items: [
      { id: 'thunderspy-stalker-reused-slots-hidden', message: 'Thunderspy Stalkers: every primary and secondary set was showing 8 powers instead of 9. Restoring the Hide and Placate inherents made the picker hide any set power sharing their internal names', type: 'fix' },
    ]
  },
  {
    date: '2026-08-12',

    items: [
      { id: 'grant-cover-team-only', message: 'Shield Defense: Grant Cover was adding its defense to your own totals on every server', type: 'fix' },
    ]
  },
  {
    date: '2026-08-11',

    items: [
      { id: 'heal-filter-bug', message: 'Fixed a bug that was discarding the calculated heal row when it arrived at the infopanel', type: 'fix' },
      { id: 'perma-tracking-adj', message: 'Fix for perma-tracking logic', type: 'fix' },
      { id: 'enh-piece-display', message: 'Hovering a slotted enhancement now shows more info, like which set piece you have slotted', type: 'feat' },
      { id: 'stacking-flag-indicator', message: 'Buffs should now also show flags that indicate whether it stacks with itself, or overwrites', type: 'fix' },
    ]
  },
  {
    date: '2026-08-10',

    items: [
      { id: 'blaster-defiance-not-permanent', message: 'Blasters: Defiance was being counted as a permanent damage buff. The few-seconds ramp every Blaster attack grants were added to totals as if it were always on', type: 'fix' },
      { id: 'proc-roll-in-executed-child', message: 'Walking back part of a proc change that affected Fault, Whitecap, Hypnotizing Lights and Spring Attack. The powers don\'t trigger procs, but they do through a child power that can', type: 'fix' },
      { id: 'pseudopet-resolves-on-pet-tables', message: 'Rain and patch debuffs were reading off archetype\'s tables instead of the summon\'s', type: 'fix' },
      { id: 'slow-movement-axes-shown', message: 'Slows now say what they slow.', type: 'fix' },
      { id: 'unenhanceable-values-were-enhanced', message: 'Values the game flags un-enhanceable were being enhanced anyway wherever a power\'s effect collapsed into one row', type: 'fix' },
      { id: 'pet-multi-table-cancellation', message: 'A pet effect that resolves against more than one of the game\'s tables was cancelling itself out instead of adding up', type: 'fix' },
      { id: 'mode-gated-rows-read-as-zero-chance', message: 'Pet and pseudo-pet effects gated by mode were being read as "0% chance" and dropped off the cards entirely', type: 'fix' },
      { id: 'hunter-mode-damage-always-on', message: 'Thunderspy: Pack Master\'s damage buff only applies in Hunter Mode, and was being counted as always-on.', type: 'fix' },
      { id: 'level-one-is-two-picks', message: 'Fixed a regression: choosing a secondary set force-added its first power at level 1 and then refused to let you remove or move it', type: 'fix' },
      { id: 'setbonus-knockback-endurance-stats', message: 'Knockback Strength and Endurance Drain Resistance set bonuses now resolve to real tracked stats instead of nothing, so they should highlight and total like every other bonus. -Speed Cap is a recognised effect now too', type: 'fix' },
    ]
  },
  {
    date: '2026-08-09',

    items: [
      { id: 'scrapper-crit-own-rows', message: 'The "w/ Crit" column now shows what a critical hit actually deals: the power\'s own crit damage added to the final, and the Critical Hits card lists the real chance and damage for both the vs-minion and vs-lieutenant-and-up branches', type: 'fix' },
      { id: 'pool-powers-wrong-pool-on-reload', message: 'Fix for a nasty issue causing builds losing set bonuses after a reload while still looking completely intact. Pool powers could be filed into the wrong pool container, which left every power, slot and enhancement on screen exactly as you left them but quietly dropped their contribution from totals. If a build looks weaker than you remember, try reloading it now and check.', type: 'fix' },
      { id: 'calc-error-banner', message: 'As a response to the error above: When part of a build cannot be calculated, the planner now shows a warning instead of only writing it to the browser console.', type: 'feat' },
      { id: 'specialized-pool-exclusion-five', message: 'The game allows only one Specialized power pool per build, and the planner was only enforcing that across three of the five. All five are mutually exclusive now.', type: 'fix' },
      { id: 'incompatible-powerset-pairs', message: 'Some primary/secondary combinations are impossible in game. Illegal combinations are now greyed out in the Primary and Secondary dropdowns. Users who made illegal builds have been reported to the PPD 🚨', type: 'fix' },
      { id: 'renamed-set-exclusions', message: 'Related: exclusions naming a powerset the game renamed were being missed entirely, so Shield Defense did not know it clashes with Scrapper Spines or Stalker Ninja Blade. Those now register.', type: 'fix' },
      { id: 'veat-branch-switch-strips', message: 'An Arachnos Soldier or Widow: Changing branch now removes the powers that branch owned, tells you which went, and offers an Undo.', type: 'fix' },
      { id: 'enh-compare-modal', message: 'Fix for the slotting comparisonal tool failing to allow multi-selection', type: 'fix'},
    ]
  },
  {
    date: '2026-08-05',

    items: [
      { id: 'proc-potential-badges', message: 'New "proc potential" badges indicate powers that are unusually good for slotting procs. This feature is OFF by default, but you can turn it on through Settings > Proc potential badges, or from the menu on mobile', type: 'feat' },
      { id: 'chain-form-switch', message: 'The attack chain builder can now support change form mid-rotation. Please note that Kheldian forms on HC have no activation time, which allows players to cancel the animation; if you want to simulate the intended animation time, "Play shift animations in full" used the full 2.244s', type: 'feat' },
      { id: 'procs-allowed-ppm', message: 'The game marks 165 Homecoming powers as never rolling a PPM proc, and that should now be reflected in the planner. Pet summons (Mastermind henchmen, Fire Imps, Phantasm, Singularity, Gang War, Voltaic Sentinel, Auto Turret) were the biggest group. A proc slotted there still reaches the pet and fires off the pet\'s attacks, it just has nothing to do with the summon\'s recharge. The rest fire nothing at all: Fault, Spring Attack, Whitecap, Paralyzing Blast, Shocking Grasp and Shockwaves.', type: 'fix' },
      { id: 'rain-proc-patch-rolls', message: 'Rains and patches roll their procs on the patch itself, once every 10 seconds, and are not helped by the parent power\'s recharge.', type: 'fix' },
      { id: 'quick-snipe-damage', message: 'Quick snipe damage was overstated: Proton Volley read 4x its real damage and Energy Sniper Blast 2x.', type: 'fix' },
      { id: 'chain-palette-form-values', message: 'The attack chain palette showed each power\'s numbers for the form it will not actually fire in, for example a fast snipe showed the slow charged cast, and Assassin\'s Strike undervalued its from-Hide damage by 3.17x.', type: 'fix' },
      { id: 'chain-pool-epic-form-gates', message: 'Pool and epic powers reached the chain builder with no form restrictions at all, so Boxing and Hasten could be placed in a Nova form rotation.', type: 'fix' },
      { id: 'chain-saved-in-form-empty', message: 'A chain saved while you were in a form reopened empty, then overwrote itself the next time you saved.', type: 'fix' },
      { id: 'set-bonus-tracked-highlight', message: 'Set bonus highlighting for your tracked stats only ever matched a fraction of the stats you can track. Max HP matched none of the 107 sets that grant it. The full tracked-stat vocabulary is mapped now.', type: 'fix' },
      { id: 'build-visibility-remount', message: 'Switching a build between public and private no longer remounts and refetches your whole My Builds grid mid-write, and a visibility change that fails now tells you why instead of silently reverting.', type: 'fix' },
    ]
  },
  {
    date: '2026-08-04',

    items: [
      { id: 'kheldian-inherent-slots-import', message: 'Fix for Kheldian powers losing slots and enhancements whenever a build was loaded from a share link, a JSON/.skif import or the cloud', type: 'fix' },
      { id: 'kheldian-form-subpower-wipe', message: 'Fix for an edge case where Kheldian and Primalist form attacks could vanish entirely when an older saved build was loaded', type: 'fix' },
      { id: 'unslottable-power-phantom-slot', message: 'Powers that take no enhancements (Swap Ammo, Adaptation, Staff Mastery, Reach for the Limit) no longer come back from an import with an empty slot stuck to them', type: 'fix' },
    ]
  },
  {
    date: '2026-08-03',

    items: [
      { id: 'rule-of-5-proc-pool-ring', message: 'Rule of 5: Fixed a capped LotG +Recharge global highlighting powers whose 7.5% Recharge came from a set bonus instead', type: 'fix' },
      
      { id: 'dashboard-power-contrib', message: 'Fxed power contributors not appearing in Dashboard totals', type: 'fix' },
      { id: 'importer-enhancement-vocab', message: 'Importing a build silently dropped Defense Debuff, ToHit Debuff, Intangible and Snare/Slow enhancements, and read ToHit Debuff as a ToHit buff. All 26 enhancement types the game actually authors are recognised now, for both in-game and Mids imports.', type: 'fix' },
    ]
  },
  {
    date: '2026-08-01',

    items: [
      { id: 'buff-debuff-sim', message: 'Added a new modal to simulate buff/debuffs on your character stats. This is wired to the chain builder modal as well.', type:'feat' },
      { id: 'character-identity-bookmarks', message: 'When saving a bookmark for a build, it will now use your character identity for the title instead of just the website string', type: 'feat' },
      { id: 'kheldian-form-attack-chain', message: 'The attack chain builder can now see Kheldian form attacks, gated by form. The Form control shows up only when your build can actually change form', type: 'feat' },
      { id: 'attack-chain-empty-character', message: 'Fix for the attack chain builder doing its math on an empty character', type: 'fix' },
      { id: 'tohit-buff-self-enhancing', message: 'Fix for +ToHit buffs enhancing themselves.', type: 'fix' },
      { id: 'generic-io-value-display', message: 'Generic IOs were advertising wrong values, level moved to the header and each stat\'s real value now lives in its own tooltip.', type: 'fix' },
      { id: 'unmapped-enhancement-aspects', message: 'Immobilize, Terrorize, Threat and Interrupt enhancements were contributing nothing', type: 'fix' },
      { id: 'tspy-pets-summoned-twice', message: 'Thunderspy: summons were exporting their pets twice', type: 'fix' },
      { id: 'tspy-range-disallowed', message: 'Thunderspy: 720 powers now say which enhancement strengths they refuse. Mostly this is +Range on melee attacks', type: 'fix' },
      { id: 'tspy-mechanic-names', message: 'Thunderspy: power mechanics should now show their real names instead of a placeholder. Contaminated Strike says "Contaminated" rather than "state", and about 170 powers too.', type: 'fix' },
      { id: 'rebirth-stealth-suppression', message: 'Rebirth: stealth and suppression data was being dropped before it reached the planner.', type: 'fix' },
      { id: 'modes-disallowed-recovered', message: 'Powers shoould now know which forms they cannot be used in. Roughly 1,500 powers across all three servers were missing that flag entirely. The attack chain builder no longer mixes forms you cannot be in at once.', type: 'fix' },
      { id: 'speed-of-sound-suppression', message: 'Speed of Sound now suppresses the pool, run and travel toggles the game suppresses, and Jaunt correctly requires it.', type: 'fix' },
      { id: 'tspy-warshade-umbral-blast', message: 'Thunderspy: Warshade Umbral Blast should read correctly now. Gravitic Emanation was at 40% damage and a 45s recharge, Unchain Essence was targeting only defeated foes on a 240s recharge, Dark Extraction was single-target, and Essence Drain and Gravity Well were both showing 7ft range instead of 25ft.', type: 'fix' },
    ]
  },
  {
    date: '2026-07-30',
    
    items: [
      { id: 'relative-level-enh-curve', message: 'Relative level is now curve-driven for origin/special', type: 'fix' },
      { id: 'caster-buff-toggle', message: 'Fix for state toggles appearing on anything with caster-buff key, regardless if it makes sense', type: 'fix' },
      { id: 'winters-gift-enh', message: 'Winters Gift show now be a normal craftable, not auto-attuned', type: 'fix'},
      { id: 'power-effect-conditionals', message:'Power conditionals like Impact should now be surfaced and flow into the attach chain builder', type: 'fix'},
      { id: 'compare-slotting-persistance', message: 'Extended persistance for the Compare Slotting tool', type: 'fix'},
      { id: 'slot-aggregation-level', message: 'Fix for an active Alpha causing enhancement craft level to get overidden by character level', type: 'fix'},
      { id: 'mm-upgrades-from-build', message: 'Mastermind pet upgrades now come from your build. Taking Equip Mercenary equips your henchmen, so the pet panel shows equipped henchmen: the boxes are pre-set from the powers you\'ve taken and named after them, instead of "Upgrade 1 / Upgrade 2". You can still click them to compare upgraded against un-upgraded, and there\'s a "follow build" link back.', type: 'feat' },
      { id: 'mm-upgrade-dps-double-count', message: 'Upgraded pet DPS was overstated. An upgrade replaces a henchman\'s powerset rather than adding to it, but the calc was stacking the tiers', type: 'fix' },
      { id: 'mm-upgrade-stats-update', message: 'Pet stats now actually change when you upgrade them.', type: 'fix' },
      { id: 'pet-stat-rows-out-of-ability-list', message: 'The upgrade powers no longer take up rows in a pet\'s ability list.', type: 'fix' },
      { id: 'tspy-upgrade-manual', message: 'Known issue: on Thunderspy, pet upgrades can\'t be read off your build. The game data doesn\'t record which power grants which upgrade, so those checkboxes stay manual and start unticked. The double-counted damage is fixed there regardless.', type: 'known-issue' },
      { id: 'pet-self-buffs-recovered', message: 'A pet\'s own resistance, defence, mez protection and mez resistance now surface in the info', type: 'fix' },
      { id: 'pet-class-stats-cross-server', message: 'Pet class list is now derived from the summons themselves instead of hand-maintained, and carries each pet\'s own hit points and caps.', type: 'fix' },
    ]
  },
  {
    date: '2026-07-29',

    items: [
      { id: 'proc-main-target-only', message: 'Procs in powers the game flags as main-target-only now roll against that one target instead of the power\'s knockdown splash', type: 'fix' },
      { id: 'proc-main-target-per-archetype', message: 'That flag is also read per archetype now instead of by power name', type: 'fix' },
      { id: 'perma-window-caster-clock', message: 'Fix for perma tracking: it now times the buff on you rather than whichever effect on the power lasted longest, and measures against your archetype\'s recharge cap ', type: 'fix' },
    ]
  },
  {
    date: '2026-07-27',

    items: [
      { id: 'end-cost-net-end', message: 'Fix for END COST and NET END not reading from the calculation path', type: 'fix'},
      { id: 'cache-bust-stamp', message: 'Added a build-engine stamp to help with calculator load issues', type: 'fix' },
      { id: 'missing-absorb-boost', message: 'Fix for Cardiac and Resilient Alpha missing absorb boost', type: 'fix' },
      { id: 'fly-speed-1.5-scale', message: 'Fly speed, once more: a fly buff is worth 21.48 mph per 100%, not 14.32. Flying has its own 1.5x base and its buffs scale off that', type: 'fix' },
      { id: 'sj-cj-jump-suppression', message: 'Known issue: with both Super Jump and Combat Jumping, in-combat jump speed reads about 0.14 mph high. In game, Super Jump\'s jump buff is suppressed rather than switched off in combat, but Sidekick drops it and lets Combat Jumping through instead.', type: 'known-issue' },
    ]
  },
  {
    date: '2026-07-26',

    items: [
      { id: 'rust-calc-engine', message: 'New calculation engine: Replaced the TypeScript math Sidekick has used until now. Despite being (hopefully) invisible to users, this is kind of a big deal, for reasons below...', type: 'update' },
      { id: 'parser-data-resync', message: 'This is the bigger part: Every power was re-read with a substantially corrected parser and re-converted: of the ~19,200 data modules, 3,956 were wrong in some way and 24 remain. Recovered along the way were seven power-header fields, Rebirth\'s special-attrib band, accolade and inherent data, and a one-field misalignment that mislabelled when an effect applies. So unlike the engine swap, this part you will (hopefully) see: powers whose effects were dropped or collapsed together, should now read correctly. I\'ve done everything I can on my end to get this working right, but because of how many powers and attributes there are, I can\'t personally verify every number.', type: 'fix' },
      { id: 'engine-offline-cache', message: 'The calculation engine and its game data are now cached, so going offline won\'t kill the calculations.', type: 'feat' },
      { id: 'engine-failure-visible', message: 'If the calculation engine fails to load, you get a red banner telling you so.', type: 'fix' },
      { id: 'jump-speed-base-mph', message: 'Jump speed was reading about 1.47x too fast due to a units mismatch.', type: 'fix' },
      { id: 'adaptation-stance-parent-desync', message: 'Bio Armor: fixed the Adaptation stance desyncing between the power-info picker and the header. On Scrapper/Brute/Tanker two powers share the internal name the stance binds to, and the picker was choosing between them by pick order...soooo taking Evolving Armor before Adaptation bound it to the wrong one. Look, I didn\'t name these.', type: 'fix' },
      { id: 'trip-mine-multi-detonation', message: 'Trip Mine on Blaster and Defender was reading roughly 13x too high 😅. Time Bomb, Seeker Drones, High Explosives and Photon Seekers had same issue.', type: 'fix' },
      { id: 'trip-mine-chance-weighting', message: 'Summoned-pet damage now weights partial-chance hits. Trip Mine\'s third Fire hit only lands half the time and was being counted in full.', type: 'fix' },
      { id: 'dominator-trip-mine-no-damage', message: 'Dominator Trip Mine showed no damage at all. Its built differently from the other ATs... the damage is only reachable through an "Info" power, which the converter discarded as tooltip-only.', type: 'fix' },
      { id: 'adaptation-stance-tooltips', message: 'The Adaptation and Staff Form stance pickers in a power\'s info now have tooltips, saying what the stance does and what that particular power contributes to it (or doesn\'t).', type: 'fix' },
      { id: 'fly-speed-base-mph', message: 'Fly speed had the same units mistake, plus one of its own.', type: 'fix' },
      { id: 'accolade-values-corrected', message: 'Fixed the new calc ignoring accolades. Also removed hardcoded values for Accolades, Sidekick now reads all of them from the game data. This also fixes the hero/villain pairing.', type: 'fix' },
      { id: 'thrust-run-speed-global', message: 'Thrust\'s +Run Speed was showing 10% and ignoring the enhancement in its own power. It reads its real speed table now, so it\'s 35% base. And it\'s enhanceable', type: 'fix' },
      { id: 'launch-jump-height-global', message: 'Launch\'s +Jump Height global was doing nothing. It was being read as +Run Speed, so it had to be switched off entirely; jump and fly globals now have their own stats. Its separate +Max Jump Height is a cap raise, not a buff, and is deliberately left out of the total.', type: 'fix' },
    ]
  },
  {
    date: '2026-07-19',

    items: [
      { id: 'set-damage-bonus', message: 'Fixed a damage set bonus multiplier that was hardcoded for HC and inflating values for Rebirth', type: 'fix'},
      { id: 'belt-of-liberty', message: 'Rebirth: Added Belt of Liberty enhancement set', type: 'fix'},
      { id: 'ruleof5-per-bonus-override', message: 'You can now hide pesky Rule of 5 warnings at a per-set-bonus level, just open the Set Totals menu', type: 'feat' },
    ]
  },
  {
    date: '2026-07-17',

    items: [
      { id: 'dataset-lazy-load-slimdown', message: 'Sidekick went on a weight-loss program 🍩, and lost 26mb of entry chunk and 35mb precache.', type: 'update' },
      { id: 'bio-absorb-overstated', message: 'Fixed overstated absorb on Bio Armor caused by a stale override and a converter that summed a placeholder value. They now grant the 30% (Ablative) and 10% of Max HP per foe hit (Parasitic).', type: 'fix' },
      { id: 'parasitic-absorb-per-target', message: 'Parasitic Aura\'s absorb now grows with the number of foes hit (up to 10), instead of ignoring the targets-hit slider.', type: 'fix' },
      { id: 'targets-hit-slider-off', message: 'Fixed the targets-hit slider computing its 1-target value while displaying "Off". Evolving Armor, Parasitic Aura, DNA Siphon and every other power with a targets-hit slider now read 0 at "Off".', type: 'fix' },
      { id: 'offensive-adaptation-res-resisted', message: 'Bio Armor: Offensive Adaptation\'s -7.5% Resistance self-penalty is now reduced by your own resistance to each damage type.', type: 'fix' },
      { id: 'adaptation-stance-desync', message: 'Fixed the Adaptation stance occasionally desyncing from the sub-power stance display, including a case where importing a Stalker/Sentinel build dropped the stance entirely.', type: 'fix' },
    ]
  },
  {
    date: '2026-07-16',

    items: [
      { id: 'movement-speed-display-rates', message: 'Movement speeds in the info panel should now be displayed with rates instead of modifier values.', type: 'fix' },
      { id: 'attribmod-coverage-expansion', message: 'A broad swath of powers should now have attribMods to surface their effects correctly instead of being smooshed', type: 'fix' },
      { id: 'internal-name-collisions', message: 'Fix for internal name collisions that could result in powers accepting the wrong enhancements', type: 'fix' },
    ]
  },
  {
    date: '2026-07-14',

    items: [
      { id: 'calc-perf-per-instance', message: 'Fixed a slowdown on completed builds where toggling powers, adjusting sliders, or swapping enhancements had a ~0.5–1s delay before updating. The character totals were being recalculated once per power/slot instead of a single shared time. Most noticeable in Chrome.', type: 'fix'},
      { id: 'adaptations-by-level-subpowers', message: 'Adaptations now show as sub-powers of Adaptation in the By-Level layout, matching By-Powerset', type: 'fix'},
      { id: 'bio-adaptation-stance-leak', message: 'Fixed Bio Armor adaptations leaking into base powers — Evolving Armor\'s defense (Defensive) and regen/recovery (Efficient) now apply only in their stance, at full value', type: 'fix'},
      { id: 'stance-picker-double-click', message: 'Fixed the stance picker in the power info window needing a double-click to fill in the selected adaptation', type: 'fix'},
      { id: 'offensive-adaptation-self-penalty', message: 'Bio Armor: Offensive Adaptation now applies its -7.5% Resistance(all) self-penalty to your totals', type: 'fix'},
      { id: 'maxhp-two-halves', message: 'Fixed +MaxHP powers that grant their bonus in two halves — one enhanceable, one un-enhanceable (Inexhaustible, High Pain Tolerance, Dull Pain, Hoarfrost, Earth\'s Embrace, Revive, Serum, and more)', type: 'fix'},
      { id: 'rage-crash-defense-self', message: 'Rage\'s post-crash -20% Defense(All) is now correctly counted as a penalty to yourself rather than a debuff to enemies', type: 'fix'},
    ]
  },
  {
    date: '2026-07-13',

    items: [
      { id: 'movement-speed-oddities', message: 'Fixed some movement speed oddities', type: 'fix'},
      { id: 'travel-combat-suppression', message: 'Fixed combat suppression for travel powers', type: 'fix'},
      { id: 'removed-vertical-cell-adjustment', message: 'Removed vertical cell adjustment for now, it has some unintended consequences that are not great. Rearrange and expand/collapse remain', type: 'fix'},
      { id: 'dot-damage-overstated', message: 'Fix for DoT damage being overstated; this was widespread', type: 'fix'},
      { id: 'rebirth-martial-prowess-order', message: 'Fix for Rebirth\'s Tanker Martial Prowess epic pool, Art of War was listed as the first selectable power instead of Throwing Dagger and Battle Hardened', type: 'fix'},
      { id: 'buff-pet-aura-toggle', message: 'Added buff-pet aura toggle and calculations for character totals', type: 'feat'},
    ]
  },
  {
    date: '2026-07-10',

    items: [
      { id: 'planner-layout-rearrange', message: 'The Sidekick main planner surface can now be reorganized with drag-n-drop, adjustable borders, and show/hide (through the new Layout menu)', type: 'feat'},
      { id: 'import-illegal-build-guard', message: 'Added an import guard that warns you when the import has created an illegal build, and reports you to the PPD. This typically happens when importing from a Mids file and is caused by a name mismatch', type: 'feat'},
      { id: 'per-server-build-persistence', message: 'Switching servers now keeps a separate build for each one. Your Homecoming, Rebirth and Thunderspy builds are saved independently, so moving between datasets no longer clears your work. No account needed.', type: 'feat' },
      { id: 'shared-build-server-switch', message: 'Importing a shared build (or opening a .skif) made on a different server now correctly switches Sidekick to that server\'s dataset.', type: 'fix' },
      { id: 'build-load-dataset-order', message: 'Loaded builds should now load the appropriate server dataset BEFORE loading the rest of the build', type: 'fix'},
      { id: 'account-synced-favorites', message: 'Added account-synced favorites with local cache reconciliation', type: 'fix'},
      { id: 'system-atlas', message: 'Added a neat system atlas for the nerds 🤓: Menu > System Atlas', type: 'feat' },
      { id: 'absorb-stat', message: 'New "Absorb" stat — a trackable total for your absorb shield (the temporary HP layer that soaks damage before your health). Enable it under Settings → Stats → Survival & Mobility, or read it in the Detailed Totals sheet. It sums absorb from your active powers plus set-bonus/proc absorb.', type:'feat'},
      { id: 'absorb-maxhp-scaling', message: 'Absorb shields that scale off Max HP now compute correctly. Wild Bastion, Ablative Carapace, Parasitic Aura and Force Barrier deliver their absorb via a Max-HP formula the old converter dropped (keeping only the duration), so their shield amount never showed. These now read as a % of your current Max HP — growing with +HP accolades and with +Absorb strength from Power Boost / Clarion Radial — while flat shields like Psychokinetic Barrier stay fixed regardless of Max HP. A few conditional cases (e.g. Master Brawler\'s missing-HP formula) remain duration-only for now.', type:'fix'},
      { id: 'tspy-empty-powers-restored', message: 'Thunderspy: ~1,000 powers that were showing no effects should now display correctly. Resistance armors (Mind Over Body, Willpower, High Pain Tolerance, Absorption) and the +To-Hit portion of buffs (Aim, Build Up, Link Minds) stored their real data in a format the parser wasn\'t reading, so they came through empty', type:'fix'},
      { id: 'tspy-multi-type-defense', message: 'Thunderspy: multi-type defense powers now show all their defense types. Mind Link / Link Minds, Fade, Farsight, Invincibility, Energy Cloak and ~85 other powers were rendering as a single defense type (Mind Link showed Def(Melee) instead of Def(All)); the full set of types is now recovered.', type:'fix'},
      { id: 'tspy-resistance-mislabeled', message: 'Thunderspy: some damage-type resistances were mislabeled as damage. Glacial Shield\'s +Res(Cold) and the -Res(all) from Corrosive Sap / Enervating Field were being read as damage effects; they now correctly show as resistance.', type:'fix'},
      { id: 'stat-label-shortening', message: 'Shortened a few stat labels (healing and jump) so the totals panel reads more cleanly.', type:'update'},
    ]
  },
  {
    date: '2026-07-08',

    items: [
      { id: 'slotted-proc-controls', message: 'New "Slotted Procs" controls in the power info panel: each slotted proc now has its own on/off switch, plus a slider for variable procs. A stack slider for Might of the Tanker (0–3 stacks of +Res(All)) and an HP% slider for Reactive Defenses\' scaling +Res(All) (3%→12.9%). Settings save and share with the build', type: 'feat'},
      { id: 'might-of-tanker-proc-totals', message: 'Might of the Tanker\'s +Res(All) proc now contributes to your resistance totals (resolved at the correct 5%/stack via the slotted-power modifier, rather than the raw unscaled value). Defaults to 1 stack; slide to set your actual stack count. Procs in a toggled-off power correctly contribute nothing. This will move resistance numbers on builds slotting it.', type: 'update'},
      { id: 'tspy-gadgetry-utility-belt', message: 'Thunderspy: the Gadgetry power and Utility Belt pools are now available. It was present in the game data but wasn\'t being surfaced in the planner', type: 'feat'},
      { id: 'stance-mode-suppression', message: 'Stance and mode toggles should now correctly suppress the powers they disable', type: 'fix'},
      { id: 'tooltip-conditional-healing', message: 'Power hover-tooltips now show conditional, stance-gated healing (such as DNA Siphon) that previously only appeared in the full info panel.', type: 'fix'},
      { id: 'obscure-sustenance-recharge', message: 'Corrected Obscure Sustenance\'s recharge', type: 'update'},
      { id: 'converter-rewrite', message: '🚨 A very large and comprehensive data converter rewrite was implemented to address a large family of bugs related to the old converter flattening data and dropping important attributes before it reached the planner (ie: a power does energy/smashing damage, but in the planner you only see the smashing damage portion). If the change is successful, you won\'t notice anything has changed other than more information surfacing in the planner that was previously missing 🚨', type: 'fix'},
      { id: 'judgement-attack-chain', message: 'Added support for Incarnate Judgement power in attack chain calculations', type: 'feat'},
    ]
  },

];

/** Flatten groups into individual entries for changelog.ts consumption */
export const MANUAL_CHANGELOG: ManualEntry[] = MANUAL_CHANGELOG_GROUPS.flatMap(group =>
  group.items.map(item => ({ id: item.id, date: group.date, message: item.message, type: item.type }))
);
