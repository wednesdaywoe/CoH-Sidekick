---
project: coh-sidekick
kind: plan
title: Data Gap Register
relates:
  - GAME-DATA-PRINCIPLES.md
  - REBUILD-PROGRESS.md
  - CLAUDE.md
---

# Data Gap Register

The audited state of the game-data pipeline. **Every known gap is either closed with a guard,
or recorded here with its severity and what it is waiting on.** The bar is "no unrecorded
unknowns," not "no gaps" — a recorded, understood gap is an acceptable state; an unknown one is
not. Full narrative for every entry lives in [docs/gaps/](gaps/); the audit tooling and what
each leg can see is in [gaps/audit-legs.md](gaps/audit-legs.md), the method notes in
[gaps/method-notes.md](gaps/method-notes.md).

## Current frontier

**0 open, of 215 entries.** When an entry is open it is listed here with what it's waiting on.
Closed entries keep their narrative in [docs/gaps/](gaps/); this section stays a pointer list and
doesn't accumulate closure prose.

Nothing open. The next finding gets a row below and a pointer here.

Closures are the `[x]` rows in the sections below; each entry's narrative (severity, census,
guards and their mutations) lives in its [gaps/](gaps/) file, not here.

---

## Sets, boosts, incarnates, inherents

[Full detail](gaps/sets-boosts-incarnates.md) — 26 of 27 closed

- [x] **HYBRID-2** — Homecoming and its Brainstorm beta dropped the Melee Hybrid's status-protection
  rows at Total Radial Graft and both T4 Embodiments while the tooltip still promises them, where
  Rebirth and Thunderspy keep the rows and read true; adjudicated as an upstream removal, not a
  parse gap, on a 408-power census of the same packed-mez shape plus a same-family control, and
  pinned by a tripwire that fires whichever side moves
- [x] **HYBRID-PT-1** — the Melee Hybrid's per-foe ceiling was scraped out of its tooltip prose
  rather than read off the power, and the per-enemy layer that ceiling caps reached no total on
  either calc; the cap now derives from `max_targets_hit` minus the caster's own slot (byte-identical
  output on all four forks), and the layer stacks against a foe-count input the beta exposes as a
  slider
- [x] **INHERENT-9** — the basic-inherent converter dropped the same four mode arrays ACCOLADE-2
  closed, so Sprint, Rest, Brawl and the prestige travel toggles published none of the Kheldian
  form gating the game gives them on every fork; the call landed with the script's FORK-1
  reconciliation, and a two-legged guard now derives the emitter roster and grades all 10,502
  emitted powers against their own export records
- [x] **ACCOLADE-2** — the accolade converter was the one tree that never called `assignModes`, so
  the Labyrinth pair's `modes_required` zone gate was dropped and a buff you only have in one zone
  presented as permanent; the picker now warns from the field, in both UIs
- [x] **ICON-1** — one mis-keyed override gave the base Winter's Gift the SUPERIOR artwork on all
  three forks and left Rebirth's `superior_winters_gift` on a filename the extractor fabricated,
  `ssuperior_winters_gift.png`, 404ing from the day the set was added while the base set's own
  asset sat unreferenced; the fabricating fallback is retired for a loud stop, and a guard now
  resolves all 673 icons against the asset library
- [x] **BOOST-5** — the two repos' `io-sets-raw` registries drifted wholesale on all three forks and each side was ahead of the other; the damage tiers are export-true, shared sets carry their own fork's values, and the beta now regenerates byte-identical registries from the same extractor
- [x] **BOOST-4** — Synapse's Agility's 6th piece read "Empty" and the 20% end-drain-resist global it grants was modeled nowhere; the piece-gated-bonus census found it the one absentee among 39
- [x] **BOOST-3** — a resist set's pieces were labelled from the record's `Category`, which the game leaves blank on every PvP, purple, event and ATO set; Gladiator's Armor exported as damage enhancement on all three forks and its resistance never reached the totals
- [x] **INHERENT-6** — the free travel toggles were never exported; the `Prestige` category is now in the export
- [x] **INHERENT-4** — one hand table gave Ninja/Beast/Athletic Run to all three forks; membership is now each fork's own export, and Thunderspy's Sprint turned out to BE its travel toggle
- [x] **INHERENT-5** — Rest's hand numbers disagreed with the export on every axis and dropped the vulnerability half; sourcing it also gave SETCAT-2's orphaned heading its host power
- [x] **INHERENT-8** — Brawl was hand-authored too, wrong on recharge/cast/endurance/damage, and dropped both its Containment and Fighting-pool halves
- [x] **INHERENT-7** — export silence on the slot ceiling IS a stated six (the parse table stamps it, the exporter suppresses it); the hand table's four had no source and is retired
- [x] **BOOSTNAME-1** — every IO-set piece name was assembled from a hand-written label table instead of read off the boost power; 41% disagreed with the game, 202 were a bare "Chance"
- [x] **BONUS-REQ-2** — Rebirth's 20 challenge-mode tiers gate on a task-force state nothing models; decision recorded not to model it, skip pinned effect-for-effect
- [x] **SETCAT-2** — Rebirth's Inexhaustibility slots only into Rest; decision recorded not to model Rest, reach guard pins the one exception
- [x] **SETCAT-1** — the pool/epic inference fallback offered real sets in powers the game lists in no set; deleted, raw-backed guard added
- [x] **BOOST-2** — the slot category was read off the enhancement converter's field; four Thunderspy sets reached no power
- [x] **BONUS-REQ-1** — gated set-bonus tiers: the PvP arm was hand-curated over a fixed binary read, and credited knockback where the game grants repel
- [x] **BOOST-1** — no contract section carried the name the game prints for an enhancement; two families were modeled nowhere
- [x] **SALV-1** — salvage.bin's rarity for InfiniteTessellation contradicted the community registry; the binary was right
- [x] **ACCOLADE-1** — accolades are a real powerset the beta had re-encoded as a hand-built silo
- [x] **INCARNATE-1** — Alpha's enhancement of other powers (the ED-bypass split), plus genesis's dormant data
- [x] **SLOT-INDEX-1** — slot pickers served hand-vendored Homecoming indices
- [x] **GENESIS-1** — the Genesis slot was served on Thunderspy though the picker hid it
- [x] **HYBRID-1** — Thunderspy's Hybrid passives (Support, Assault, Control) were missing
- [x] **INHERENT-1** — the beta's fitness silo dropped Health's Res(Sleep); both engines now atomize fitness

---

## Parser + binary fidelity

[Full detail](gaps/parser-fidelity.md) — 38 of 38 closed

- [x] **PARSE6-3** — WITHDRAWN, no defect: the Parse6 `tags` census counted a label, and the mechanic is a gate. Containment is a mez-gated damage twin and is comparably populated on all four forks (377/325/326/377); Domination reaches the UI on all four. The Parse6 field is `AttribModTemplate.pchName`, read correctly. Gated by `mechanic_gates_survive_every_fork.rs`
- [x] **STACK-5** — the encoder dropped `StackByAttribAndKey`, so the converter's per-target regen skip had no wire spelling and the mirror's keyed-row proxy misfired on Reactive Regeneration's keyless `Stack` rows; the flag now rides slot 44 and the mirror asks it
- [x] **LIFETIME-1** — the power's usage-limit/lifetime block (parse-table fields 56–65) was decoded and discarded on both layouts, so a granted charge's authored decay (`Combo_Level_1: LifeTime 6`, Energy Focus 15s) read as immortal; now exported sparsely with a def-oracle gate
- [x] **MAXBOOST-1** — a stated `max_boosts: 0` is literal and absence is a stamped 6; the binary has no third state, and all three converters now read it that way
- [x] **DELAY-1** — the AttribMod's `Delay` was decoded and exported and reached no atom, so the only field separating a power's own effect from its crash was invisible to every consumer
- [x] **TARGETS-3** — `AnyAffected` names no recipient; the join is power-level, and for a redirect-collected atom it is the LEAF's power that answers
- [x] **TARGETS-2** — the target enum's two "includes me" cases were folded onto one member, and half the appliers read it as the caster
- [x] **SETGATE-1** — the powerset record's own two gates were read into nothing (pool exclusion, VEAT branch)
- [x] **STACK-3** — the AttribMod's `DelayedRequires` was unread on Thunderspy and discarded on Homecoming
- [x] **WRAP-1** — Thunderspy exported no `params` on any power-referencing attrib but `Create_Entity`
- [x] **WRAP-1a** — the summon byte-scan shipped every Thunderspy pet twice, and named one by its message key
- [x] **WRAP-2** — Rebirth's wrapper attrib is `Power_Redirect`; a guard above the tail block starved the decode
- [x] **WRAP-3** — the combat-suppress gate matched event NAMES, and Thunderspy's events are deliberately unnamed
- [x] **TSPY-7** — the collapsed 4-aligned attrib view outranked the band and named its neighbour
- [x] **TSPY-6** — a new attrib slid Thunderspy's special-attrib band, renaming it and deleting 2,523 templates
- [x] **TSPY-5** — Thunderspy read as publishing zero `StrengthsDisallowed` across its whole corpus
- [x] **TSPY-4** — every Thunderspy `Set_Mode` template resolved as `Ones`, so no mode was bindable
- [x] **TSPY-3** — Thunderspy atoms were largely `Unmapped`; typing recovered, then the vocabulary swept
- [x] **TSPY-1** — the on-disk Thunderspy binary re-parses byte-identical to the committed export
- [x] **TSPY-2** — Combat Jumping's `jumpHeight` on Thunderspy
- [x] **WALK-1** — five JSON-reading guards walked `child_groups`, a key the export does not emit
- [x] **TAIL-1** — the Parse7 AttribMod tail fully ordered; Messages/FX decoded, Params is a tagged union
- [x] **FLAGS-1** — a false `DeepSleep` on every duration mod; the second word is a per-attrib union
- [x] **FLAGS-2** — a RequiredEvents misparse corrupted every event-gated template
- [x] **PROJ-1** — the decoded tail fields reach the wire (alpha ED-bypass split, `required_events`)
- [x] **PARSE6-1** — Rebirth's "ModesSuspended" slot is `FreeBoostSlotsOnPower`; the bonus-slot schedule came back
- [x] **PARSE6-2** — Rebirth's special-attrib band was mislabeled wholesale
- [x] **CLASSES-1** — `_classes.py`'s header was anchor-scanned; now fully sequential
- [x] **CLASSES-2** — Homecoming NPC-class origins and restrictions were misaligned reads
- [x] **CLASSES-3** — the AttribMaxMax Absorb row was never read, so absorb shipped with no ceiling
- [x] **TARGETS-1** — the Homecoming target-type enum is +2-shifted past `Any`
- [x] **CURVES-1** — the ED curve layer is binary-parsed, exported, and read by both engines
- [x] **STACK-1** — `Scaled` flattening loses the number-vs-object bit the stack multiply guards on
- [x] **STACK-2** — the beta's perTarget short-circuit, checked against the corpus that would expose it
- [x] **STACK-4** — `StackFamily` dropped `sub_type`, so one cap covered a family whose axes disagree: Time Wall's Run atom stacks while its Fly and Jump atoms `Replace`, and all three multiplied together; the finer key narrows each row to its own sub type's depth — 6 mover rows on Thunderspy (Time Wall / Time Stop / Be Gone × Fly/Jump), 6 on Rebirth (Burnout's six `Replace` defense positions), 0 on Homecoming, every mover a narrowing, with the census A/B as the standing record
- [x] **HC-1** — the count-parity sweep against CoD2: clean, residual classes explained
- [x] **HC-2** — `.powers` audit residuals: the drift class, and the field-capture backlog
- [x] **CENSUS-1** — the coverage census: clean, two benign structural zeros

---

## Pets + summoned entities

[Full detail](gaps/pets-entities.md) — 18 of 18 closed

- [x] **ENT-1** — Rebirth pet commandability was guessed from the class name
- [x] **ENT-2** — the villaindef level element was read at Homecoming's width on both forks
- [x] **ENT-3** — a pet whose whole kit falls outside the converter's vocabulary was dropped entirely
- [x] **ENT-4** — the pet converter read no AttribMod flags, so an unenhanceable debuff looked enhanceable
- [x] **ENT-5** — a movement CAP debuff overwrote the speed debuff in the same `slow` slot
- [x] **ENT-6** — the `*Unenhanced` half of a buff reached the totals with no display row
- [x] **ENT-7** — every foe-facing Regeneration debuff a pet carries was dropped
- [x] **ENT-8** — the converter kept one effect per type per power, and the merge below it summed across tables
- [x] **ENT-9** — six pet effect types were emitted by the converter and read by no consumer
- [x] **ENT-10** — a pseudo-pet's merged effects resolved against the summoner's tables, not the pet's
- [x] **ENT-12** — a pet's own protection and debuff resistance reached no total
- [x] **ENT-13** — a `chance: 0` effect group is a mode-gate sentinel, published as a probability
- [x] **ENT-14** — the wire `EntCreate` atom carries neither the pet lifespan nor an entity key; `summonWindow` now says which row IS the power's window
- [x] **ENT-16** — a summon whose `EntCreate` states redirect powers rather than an `entity_def` was walked by nothing; the converter now resolves it to the entity that declares exactly those powers
- [x] **ENT-15** — the `activation_effects` buff-dedup filter dropped a summon's create-entity rows before they could become atoms
- [x] **ENT-18** — the aggregate `base_defense` attrib names no vector, so it was missing from the
  converter's positional defence map and every branch reading that map declined it: the ally path
  published a pet's +Def to its owner as a DEBUFF, the self path dropped it whole; it now expands
  to the eleven vectors `defense_key` resolves
- [x] **ENT-19** — a `Heal`-attrib heal has two display homes: the powerset converter routes it into
  `damage` for `display_effects`'s `healing_from_damage` transform and writes no bag key, while the
  pet converter emits a `Heal` row the merge publishes as `healing`; adjudicated on the ground that
  `damage` is an `EXECUTION_STATS` def field no atom router writes, scoped to the bag→atom direction
  because the key's `MaxHp` half is atom-projected and disjoint, and pinned by a staleness assert
- [x] **ENT-17** — the inline pseudo-pet route classified an ally +Defense, +Absorb or scalar
  +Regen/+Recovery/+ToHit/+Recharge aura as nothing at all, where its entity-route twin has read all
  four families since ENT-9, so a buff a summon delivers reached no total and left no trace to
  audit; the three families the corpus actually holds now emit through the same vocabulary the fold
  already spends, and a converter-side tripwire fails the regen on the next ally-buff-shaped row
  that classifies to nothing
- [x] **SHELL-1** — opaque-shell pseudo-pet summons were unresolved

---

## Procs + PPM

[Full detail](gaps/procs-ppm.md) — 9 of 9 closed

- [x] **PROCCAT-1** — `proc-data.ts` gave each of 184 procs a hand-authored `setCategory` in front
  of the `type` each fork's registry owns, wrong in 52 and read by nothing; the field is deleted in
  both repos and out of all four contracts, the extractor's five order-anchored readers now fail
  loud on a shape change, and a cross-fork guard grades the `setName` join a derive-at-use depends
  on plus the 17 sets whose type the forks spell differently

- [x] **HC-4** — procs in `ExecutePower` wrappers roll on the parent; settled by live measurement
- [x] **PPM-1** — the auto/toggle proc period is the piece's field, not the host power's
- [x] **PPM-2** — the click proc window ignored recharge slotted in the power
- [x] **PPM-3** — `procsOnlyOnMainTarget` reached the contract and no consumer read it
- [x] **PPM-4** — `ProcAllowed kNone` was parsed, and the contract dropped it
- [x] **HC-3** — `ProcMainTargetOnly` was never parsed, so those procs scored the wrong PPM area factor
- [x] **PROC-PATCH-1** — a summoned patch owns its procs' clock and footprint; the engine read the parent's
- [x] **PROC-PERCEPTION-1** — `+Perception` globals landed in the converter's junk-drawer category
- [x] **PROCPET-1** — the pet-carried proc stamp keyed on the record's `Category`, blank on every purple set, so Soulbound Allegiance's pet Build Up leaked into the player's totals; moved onto `GroupName` (BOOST-3's field, on a site BOOST-3 named and didn't move)

---

## Conditionals + gates

[Full detail](gaps/conditionals-gates.md) — 22 of 22 closed

- [x] **EXPR-1** — the evaluator wasn't Kleene: an unvaluable term aborted the whole gate, so a redirect branch already definitively false (its mode off) still froze the form walk on its `distance` clause and Stun projected its base form while Power Boost was live; unknowns now ride the stack as a value and `&&`/`||` absorb them exactly where two-valued logic already answers — `false && x` is false for every x
- [x] **COND-13** — the forks' Domination bonuses are sunk by the blanket `arch source> Class_` skip, and named from the powerset path
- [x] **COND-8** — the export joins a `Requires` token array with spaces; a token containing a space cannot be recovered
- [x] **COND-9** — the Swap Ammo debuff atoms carry no ammo-mode gate; the bag strips them by powerset name
- [x] **COND-10** — the Parse6 forks ship Swap Ammo without the `Set_Mode` that names it, so their ammo variants cannot be gated
- [x] **COND-11** — a tag swap with no mode publisher (Staff forms, the high/low heal tiers, the Kheldian shapeshifts) reaches base ungated
- [x] **COND-12** — a flight-mode chance-mod tag family spanning several selector and carrier powers with no shared parent; `_variant-modes.cjs`'s per-powerset architecture cannot read it
- [x] **CHAIN-1** — every data bullet closed 2026-08-06/07; the per-cast position rule was never a data gap and is re-filed as feature work, REBUILD-PROGRESS RB5-d
- [x] **COND-1** — `_isConditionalGate` used an expression's last token as a proxy for its root operator
- [x] **COND-2** — the pool and epic converters built no `conditionalEffects` at all
- [x] **COND-3** — a conditional id collapsed two different states when the gate is spelled `>=`
- [x] **COND-4** — an archetype-forked conditional group was dropped rather than scoped
- [x] **COND-5** — the adjuster sweep walked one partition of three
- [x] **COND-6** — COND-5's five sibling gates, worked one at a time; two were not the predicted defect
- [x] **COND-7** — a `mode: "replace"` conditional's damage concatenated instead of replacing
- [x] **MAPGATE-1** — the PvE/PvP verdict was a substring test that read the crit branch backwards
- [x] **AT-FORK-1** — an archetype-forked group is base for the archetypes it names, conditional for the rest
- [x] **AT-FORK-2** — a fork whose arms agree IS expressible; four Rebirth pool powers stated nothing at all
- [x] **AUTOISSUE-1** — the game's own grant marker was re-encoded as a curated table
- [x] **AUTOISSUE-2** — a grant gated on the ARCHETYPE alone matched neither closure rule, on the converter and in the engine
- [x] **AUTOISSUE-3** — the beta picker re-keyed its shadow filter on the internal name AUTOISSUE-2's closure warned about, hiding one power from all 28 Thunderspy Stalker sets
- [x] **OVERRIDE-1** — 20 Kheldian form sub-powers shipped display-name `requires` over the binary RPN

---

## Damage + power variants

[Full detail](gaps/damage-variants.md) — 9 of 9 closed

- [x] **ASFORM-1** — the meter-selected redirect pair (Assassin's Strike Quick/Stealth) was every form detector's orphan, so a hidden opener's Assassination damage reached no atom and projected zero; the pair now ships as `formVariants` with union atoms
- [x] **SNIPE-1** — a damage template's `magnitude_expression` values it, and `extractDamage` read only `scale`
- [x] **SNIPE-2** — the fast-snipe detector knew only Homecoming's gate, so two forks had no quick snipe
- [x] **SNIPE-3** — the fast form swapped the cast but not the damage, because it carried no atoms
- [x] **MODEVAR-1** — a mode variant swapped the display fields but not the atoms
- [x] **MAGEXPR-1** — the damage resolver read an expression-valued atom's `scale` as if it were the result
- [x] **HEAL-1** — the displayed heal collapsed a heal-over-time's ticks into one tick
- [x] **DOT-TABLE-1** — the incarnate converter's `Ranged_Tempdamage` table guess fired on real data
- [x] **INHERENT-2** — Vigilance and Fury derived from the data on all three datasets

---

## Stat routing + caps

[Full detail](gaps/stat-routing.md) — 52 of 52 closed

- [x] **DEBUFFRES-1** — Brainstorm's Light Affinity states an accuracy debuff resistance and
  Regeneration's Revive a range one, and neither reached a total: the type carried both keys while
  no global, stat definition or `ROUTED_SUBTYPES` entry did; `mod_Process` resists by the attrib's
  own offset, so the game applies both, and the pair now has fields, routes, guards and dashboard
  rows on every fork
- [x] **MOVE-4** — `planb-shadow-movement`'s three pin tables key on `<dataset>|<power>|<axis>` and never gained Brainstorm rows, so 30 movement and 7 slow slots the TARGETS-3 buckets already answer for on Homecoming fell through to `diverge` and held `npm run regen` shut; the five powers are byte-identical across the two datasets so the pins mirror, and `audit-dataset-roster` now bracket-matches composite keys its bare `homecoming:` anchor could not see
- [x] **STACK-6** — `stacksLinear` was keyed by a second, hand-maintained copy of the routing rules, so it named `specialBuff` for sentinel Aim's `Range|Str` row the router puts on `rangeBuff` — a key with no value beside an omitted key that has one; the key comes from `projectAtomsToEffects` now and admission stays the classifier's, 166 swaps and 17 collector-superset orphans gone, and the beta's stacking adjudication retires with it
- [x] **MOVEMAP-7** — the display `slow` slot carried the kFly mode row folded in as a slow magnitude on all three forks (487 rows); the extractor now skips the mode axis in debuff slots, matching the atom readers, with the census and both guards in the narrative
- [x] **FLYPOOL-1** — the export walk settled for the engine: shown fly = enhanceable Cur row × the AT flying table (HC's in-game +160.91% calibrates); the legacy 83/80 was the IgnoreStrength Ones row clobbering the enhanceable one in the twin's one-slot axis — the converter now pairs `<axis>Unenhanced`, and the beta rows went green
- [x] **MOVEMAP-5** — the per-target stacking pass writes a movement axis to a TOP-LEVEL bag key while the routing pass writes the same atom to `effects.movement[axis]`; the classifier now returns a subKey, `MovementValue` carries `per_target`, and the scalar-slot block (ATOM-BAG-4(a)) is dropped
- [x] **MOVEMAP-6** — the self `aspect=Maximum` run-cap "raise" is a toggle's `OnDeactivate` mirror the bag's ROUTING pass drops and the atom could not see; `applicationType` now reaches the wire, `is_cancelled_pair` is retired and ATOM-BAG-4(b) landed with it
- [x] **ATOM-BAG-7** — a `StealthRadius` row on the `Str` face is the Assassin's Strike reveal, not a radius; `|−1|` shipped as +1 ft of stealth on both axes for 34 powers in three forks, 33 of them invisible to the bag-removal census because the atom path reproduced the error, and the one it named was a hand override; the `?? bag.stealth()` arm is retired with it and the census is now zero everywhere
- [x] **DISPLAYONLY-1** — an effect group tagged `DisplayOnly` is a tooltip twin quoting a number the
  power causes elsewhere; nothing in the pipeline read the tag, so Brainstorm Disruption Strike's
  −2.5 res debuff — stated at `AnyAffected` on a `["Self"]` toggle, the exact shape of Rest's real
  crash — landed as a phantom self penalty on all 8 types in the live applier. `isDisplayOnly` /
  `is_display_only` now drop the row from both totals entry points on both routes, and only there
- [x] **ATOM-BAG-9** — the converter folded an `Enhancement`-face control row into the `effects.taunt`/`effects.placate` slot the engine reads as RESISTANCE, and wrote the caster's `effects.rangeBuff` from an ally-only `aspect=Str` Range row; both now key on face and recipient, in the converter AND its `window_slots.rs` display twin, which kept the old rules until the presence gate said so; 4 slots over 3 powers left the bag, both residual lists empty by census
- [x] **ATOM-BAG-8** — ATOM-BAG-5's converter half, which left `planb-shadow-resources` red and `npm run regen` blocked ahead of `emit-contract`: the bag summed a power's caster rows and its `target ≠ source` rows into one regen/recovery slot, stating a number nobody receives (Valiance 1.4, Pack Master 5.4); the projection now carries the bag-side `reachesCaster` and splits a MIXED queue to its caster half, while a wholly non-caster slot is left as authored because the engine already reads no bag here and the power card renders this one
- [x] **APPTYPE-1** — the shutdown-burst rule was movement-only, and whether the mode/grant readers spent an `OnDeactivate` row was unmeasured; the census found the mode reader consumes no atom (static `setsModes` + `Source.Mode?`), the grant reader's one spend is the adaptation stances' same-cast net-zero revoke, and `GrantPower`/`set_mode` on `OnDeactivate` is inert in every bundle — both carriers correctly inert, pinned by `deactivation_burst_census`

- [x] **CAPS-1** — four attribute ceilings the class binary authors and the export discarded
- [x] **ABSORB-2** — two mutually exclusive gate arms of one absorb were both satisfied, doubling it
- [x] **MEZRES-1** — "Mez Resistance (All)" accumulated in a field the calc never spends
- [x] **MEZRES-2** — taunt/placate resistance's second encoding reached no total
- [x] **MEZRES-3** — repel and teleport resistance had no modelled global
- [x] **SETSTAT-1** — two set-bonus stats the calc modelled nowhere
- [x] **SETSTAT-2** — a JumpSpeed set bonus routed nowhere, though the target field exists
- [x] **MOVE-1** — `movementControl` and `movementFriction` had no modelled global
- [x] **MOVE-2** — 24 Homecoming overrides hand-wrote a foe slow into the caster's `effects.movement`
- [x] **MOVE-3** — Group Energy Flight's `movement` override replaces the converter's, losing the stack key; deleted on an empty consumer census, the guard now pins the shipped key
- [x] **MOVEMIN-1** — the game floors a current attribute at its `AttribMin` row and the export discarded it, so a grounding power's kill switch projected as a negative speed; the floor is 0.1 run/fly, not zero
- [x] **ATTRMIN-1** — the same discarded row floors ToHit, every typed defense, Stealth and the stealth radii; defense measured reachable and exported, ToHit measured unreachable and closed unmodelled, the stealth family measured last — the radii's negative population is all on the strength face and bare Stealth is saturating but unprojected, both closed unmodelled with the premise pinned
- [x] **DEFDEBUFF-1** — the defense and ToHit appliers were buff-only, so a power's self-directed defense debuff reached no total; defense now routes through the sibling applier onto ATTRMIN-1's floor, and the ToHit half is closed unmodelled — every caster-reaching −ToHit in all three forks is a rez after-effect the new `delay` field identifies
- [x] **ABSORB-3** — a mode-gated absorb reaches no total: the apply pass's absorb block is entered only when the bag carries the slot, and the synthetic conditional power carries no bag; fixed by un-gating the fraction path (the bag had no slot to narrow), Organic Armor's 1% shield now lands
- [x] **DEFALL-1** — `Base_Defense` is the eleven typed slots at once, and the defense BUFF applier dropped it as a non-standard subType: Personal Force Field granted no defense at all on Rebirth and Thunderspy; the `All`-at-`Cur` arm now expands to the eleven keys in all three files, and both forks read Homecoming's 75.0%
- [x] **MOVEMAP-1** — the movement map held one value per axis; both maps now split, and the fly axis was reporting −51% where the game gives −1%
- [x] **MOVEMAP-3** — the kFly MODE kill was spent as a flight-speed percentage: −1000% on Granite Armor, −1,000,000% on Hibernate
- [x] **MOVEMAP-4** — the movement gate's fixtures were built without atoms, so it graded Rust-from-atoms against TS-from-bag
- [x] **MOVEMAP-2** — the premise was false: Parse6 reads the aspect, and both forks author movement caps
- [x] **BRIDGE-1** — the atom bridge folded defense-debuff-resistance into `Resistance/All`
- [x] **BRIDGE-2** — the atom bridge folded three stealth axes into one untyped `Stealth`
- [x] **STRENGTH-1** — Pass 1 over-credited `strengthMovement` for a self-slow it failed to abs
- [x] **STRENGTH-2** — the beta's bag over-credited `strengthMez` from Foresight's mez resistance
- [x] **PASS2B-1** — knockback protection counted a single-target attack's knockback as self protection
- [x] **PASS2B-2** — the regen/recovery HP-scaling expression (`kHitPoints%`), atom-native
- [x] **PASS2B-3** — the recharge family
- [x] **PASS2B-4** — the maxEndurance family
- [x] **PASS2B-5** — the accuracy + toHit bag fallback
- [x] **PASS2B-6** — the perception family
- [x] **PASS2B-7** — the endurance-discount family
- [x] **PASS2B-8** — the range family
- [x] **PASS2B-9** — the absorb family: the flat-HP half and the MaxHP fraction
- [x] **PASS2B-10** — the maxHP bag fallback: corpus-vacuous
- [x] **PASS2B-11** — the debuffResistance family, migrated atom-native
- [x] **PASS2B-12** — elusivity was a hand-override double-count of defense-debuff-resistance
- [x] **PASS2B-13** — the mezResistance family, per type
- [x] **PASS2B-14** — taunt/placate resistance (Res_Boolean)
- [x] **PASS2B-15** — stacking magnitudes, settled from the binary and the server source
- [x] **PASS2B-16** — stealth radius, gather-then-resolve
- [x] **TEAMBUFF-1** — Grant Cover's team-only defense landed in the caster's own totals on all three forks

---

## Pipeline + provenance

[Full detail](gaps/pipeline-provenance.md) — 35 of 36 closed

- [x] **PARTSTAT-1** — four converters wrote a power's execution stats into the `effects` bag
  under the export's own field names, and the loader's rename reached only two of them: every
  accolade and archetype inherent projected with no cast time and no endurance cost (82 and 30
  across the four forks), and no partition power had the top-level `effectArea` its card's tag row
  reads; one shared mint now serves all six converters, graded against the export record
- [x] **FORKSTAMP-1** — `hydrateBuild` named its fork roster inline and never grew a Brainstorm
  arm, so a Brainstorm save re-stamped itself Homecoming on open: the engine keys its calculation
  on that field while the badge reads the loaded dataset, giving live numbers under the beta's
  label; the roster now has one home and a round-trip over `DATASET_IDS` reds on a fork any
  reader forgets
- [x] **FORKPORT-1** — a file could only be read on the fork it was saved on: both planners
  reloaded onto the file's own dataset, so "what does my live build look like on Brainstorm" was
  unaskable; each now parks the file and offers both opens, and a port re-stamps the build with the
  fork it was read against rather than rebuilding FORKSTAMP-1 by hand
- [x] **ROSTER-1** — 23 powersets across Homecoming and its Brainstorm beta converted cleanly,
  shipped in the contract, passed every corpus gate and could be picked by nobody, because the
  archetype rosters that decide what the Build Identity menu offers are hand-maintained lists no
  regen step refreshes: Sonic Melee, Wind Control and the Stalker's Stone Armor on live, plus
  Light Affinity and Sonic Aura on the beta; the rosters now name every converted set and a
  two-directional join grades them on all four forks
- [x] **BRAIN-11** — the bin-crawler guards and ten engine corpus rosters iterated a three-fork
  roster, so a shipped dataset's decode and its numbers were graded by none of them; rosters derive
  from `_forks`/`DatasetId::ALL` now and every per-fork expectation was measured, not copied
- [x] **BRAIN-13** — the advisory bucket gates now, and its own reader was miscounting it: a
  600-char window cut tables off before the rows that sit behind a provenance comment; real
  absences carry a `dataset-absent` marker, and the audit itself now runs in `regen-all`
- [x] **BRAIN-12** — Brainstorm shipped unselectable: the server picker's `SERVER_OPTIONS`, the
  `?serverId=` parser and the per-server build store each restated the roster and each stayed
  three-dataset, and all three sat on the roster audit's allow-list, excused by a blocker
  (`build:engine` had not yet emitted `brainstorm.json.gz`) that the dataset itself had cleared
  and nobody re-checked; `DatasetId` is now derived from a `DATASET_IDS` const array all three
  read, and the allow-list rows are gone
- [x] **BRAIN-10** — a sixth roster shape, in Python: 14 bin-crawler parser guards pruned
  `("rebirth", "thunderspy")` from the Homecoming walk, so Brainstorm's tree was swept as
  Homecoming's; only 2 went red and 12 counted it silently, one reporting 96,371 Homecoming
  templates for 47,905. The roster now derives from `assets_sources.json` plus the tree layout
  (`tests/_forks.py`), `test_export_roster.py` fails on an unrostered export tree or a
  hand-written fork list, and CI stopped aborting the loop on the first failure
- [x] **BRAIN-9** — the fourth dataset landed in canonical and stopped there: 19 shared-surface
  files drifted unadjudicated pending the beta-repo port, and the port found the rosters an audit
  keyed on collection literals cannot see — a REGEX in `vite.config.ts` named the per-dataset
  chunks, so brainstorm's was named after its facade module, missed `globIgnores`, and failed the
  beta build trying to precache 20 MB; chunk names now capture the directory, and
  `ServerId`/`Build.serverId` read `DatasetId` rather than restate it
- [x] **BRAIN-1** — Homecoming's open beta was a ring only readable, never exportable, so the
  Brainstorm server's players had nowhere to plan i28p4; the ring is now the `brainstorm` dataset,
  removed from `homecoming` because two datasets claiming one tree misroute the recipe export, and
  the export reads clean at 11,307 player powers with every powerset change the notes name present
  and gated
- [x] **BRAIN-5** — adding a dataset failed loudly in Rust and silently everywhere else: 20 script
  rosters, 22 Rust corpus gates and 29 vendored TS tests carried a hand-copied three and reported
  PASS while sweeping three of four, and three gates asked `dataset == Homecoming` to mean "the
  Homecoming game" and so answered no for its other ring; rosters now read one source,
  `is_homecoming()` says what those sites meant, and `audit-dataset-roster.cjs` flags a literal
  naming all but one
- [x] **BRAIN-3** — the conditional-coverage gate ran nowhere, so Brawl's Fighting-pool synergy
  shipped with no toggle able to reach it on three forks (the fifth capability
  `convert-powerset.cjs` never handed a sibling), while the gate bare-parsed each export where
  every converter reads through `_readPowerFile` — hiding Thunderspy's Quantum Acceleration on
  both sides at once; three converters ported, every drifted pin adjudicated rather than
  re-pinned on sight, and pool/epic `predicted == shipped` now asserted
- [x] **BRAIN-2** — censusing all 250 keyed override slots against both forks' own generated
  output found them agreeing on 249; the one that drifted, Mastermind Traps Caltrops, restates
  Homecoming's `allowedSetCategories` verbatim and on Brainstorm dropped the Knockback the newer
  export grants, taking Knockback sets off the power in the picker, and the regen's set-category
  audit had been printing the row as informational since the dataset was added
- [x] **OVERRIDE-5** — censusing the rest of the layer found no key the export does not already
  own: 130 `description`/`shortHelp` slots from a second text source, 89 `allowedSetCategories`
  that restated or reordered the derived list, six VEAT/Kheldian slots contradicting the binary,
  a lone `maxSlots` 6 among 57 derived zeroes, and a Slice `excludes` the export states as a
  `requires` gate on all eight members; the layer is empty and guarded
- [x] **HELPTEXT-1** — the converters handled the client's `<br>`/`<color>` markup three ways:
  `convert-powerset` deleted each tag with nothing in its place, gluing the sentences either side
  of a break on 3,135 Homecoming powers, while the pool, epic, inherent, basic-inherent and
  accolade converters passed it through and shipped 440 raw `<br>` and 130 `<color` into the
  contract; all seven emitters now share one `helpText`, and the view layer's paragraph splitter
  reads the newlines it leaves
- [x] **BRAIN-4** — the three powersets the notes did not name were not three powerset changes:
  Energy Blast did not change, Electrical Melee's 26 files are description text, and both sets'
  remaining deltas are Taunt/Confront, changed on 65 of 65 copies game-wide as the announced PvP
  pass; the one mechanical change, Scrapper Lightning Clap gaining `ScrapperCrit_AoE`, is
  announced on LIVE twice (2026-03-11, 2026-06-23) and absent from a live build dated after both
  — the beta is where the fix lands
- [x] **BRAIN-8** — all four TS-oracle fixture corpora stayed three-dataset, so nothing
  cross-checked the Rust engine on Brainstorm; three emitters were widened but never re-run, and
  their roster-audit allowlist rows still named a blocker false of this repo. Only
  `movement_gate`'s per-dataset floor could say so, and cargo's fail-fast truncated the run before two
  sibling gates with the same floor ran. Emitters re-run, allowlist rows deleted, and
  two hand-rolled `&str`-to-`DatasetId` inverses replaced by a roster-derived `from_wire`
- [x] **BRAIN-7** — fourteen per-dataset expectation tables across nine gate files were typed
  `[(DatasetId, …); 3]` and iterated over THEMSELVES, so Brainstorm was graded by none of them and
  all stayed green vacuously; the roster audit matched only the bare `[DatasetId; N]` arity form
  and the tuple form is the majority shape. Arity now covers both as a gate finding, the fourteen
  are typed `DatasetId::ALL.len()`, and every new row's delta from Homecoming is read and named
- [x] **BRAIN-6** — the display slot-presence gate's hand adjudication table listed three datasets
  and never went stale-checked, so Brainstorm's two ENT-14 `summon` rows were missing while a
  `Thunderous_Blast` row whose divergence had already been fixed was still held open; the gate now
  asserts every adjudicated row still matches something, and `audit-dataset-roster.cjs` reached
  none of this because its bracket-span scan capped at 400 chars and the table is longer
- [x] **FORK-1** — the two repos hand-copy `scripts/` and nobody had measured the copies: 17 of 49
  had forked, `convert-powerset.cjs` by 714 lines in BOTH directions, and the beta still shipped a
  `dual_pistols` proper noun in a converter conditional plus the hand table MAXBOOST-1 retired; a
  hash manifest now adjudicates 142 shared paths including `tools/bin-crawler`, every script is
  identical or declared, and `regen-all.cjs` is per-repo because the driver was already identical
  and only the stage and gate rosters differ
- [x] **FORK-2** — `convert-powerset.cjs` loads its atom encoder from `src/` at runtime, so three
  byte-identical converters still emitted different files; twelve `src/` modules the pipeline
  executes had forked in BOTH directions, unwatched, because the manifest read its surface from
  `git ls-files scripts` alone; the hand-typed leveling schedule gated Thunderspy's pools at 4
  where `schedules.bin` says 1, and the last fork's form model was two proper-noun enums served
  by a community mapping the parser had already made retirable
- [x] **FIXTURE-1** — the manually-emitted gate fixtures (procs, movement, set-bonus) drifted on
  sampling identity and labels only, no values; all three re-emitted, gates green, and the
  emitters now run in `npm run regen` so the drift class is closed
- [x] **PERFOE-1** — `mergeStackingPatches` rebuilds a patched slot as `{scale, table, perTarget}`
  after `projectAtomsToEffects` wrote it, dropping every mark the projection put there; the base
  half's 1,702 values grade off the converter's `per_target` stamp, and the conditional pre-pass
  now keeps that stamp too, so the entry mirror reads the other 142
- [x] **PERFOE-2** — the Execute_Power redirect branch stamped its per-foe half onto the emitted
  atoms by signature and its BASE half onto nothing, so 8 Homecoming Kinetics slot values stated a
  contribution the wire did not carry; the base arm now rides the same replay as `redirectBase`
- [x] **OVERRIDE-3** — 30 hand-written override files restated their generated `effects` slot: 21 keys verbatim and 27 minus a mark the converter now writes, plus Entropy Shield's taunt, a hand pick between two real colliding rows; all retired and the gate's exclusion list with them
- [x] **OVERRIDE-4** — the override layer stated identity and placement fields the export owns:
  92 `targetType` restatements, 22 re-cased `internalName` entries, and the Widow pair's swapped
  `name`/`icon`/`available`; verdict was delete all (none knew what the export does not), the
  Widow oracle was the authored `raw defs`, and a guard now pins both identity fields
- [x] **TAUNT-1** — the single control slot kept whichever row wrote last, always the
  redirect-collected inherent; stated rule now: the power's own row wins, keyed on the
  `ownerTargets` provenance stamp, mirrored in the router and applier and mutation-scored
- [x] **TWIN-3** — the subset projection ran the Thunderspy target-trap guards over a conditional
  entry's bag, where the converter runs them on the finished base and never walks
  `conditionalEffects`
- [x] **OVERRIDE-2** — two hand-written override entries put a value in the `effects` bag that no export template produces; both retired, and both numbers were already stated correctly elsewhere: Foresight's 0.75 by `buffDuration`, Omega Maneuver's stun by the summoned bomb's own ability
- [x] **TWIN-2** — four of five converters called one Thunderspy target-trap guard and the fifth called neither; 15 recovered foe-control keys on 4 self-only powers
- [x] **REGEN-1** — the powerset converter deleted a set before rebuilding it, and reported success if the rebuild died
- [x] **VENDOR-1** — the beta's vendored export sat a parser commit behind, and three converter defaults stated the gap as a rule
- [x] **PROV-1** — the export manifest named the parser, never the assets tree the bytes came from
- [x] **PROV-2** — stale in-repo asset twins shared a shard name with the live installs
- [x] **INHERENT-3** — the committed `exported_powers/` was not reproducible by a plain export run
- [x] **SOURCE-1** — second-source constants the export did not surface (the engine's wholesale Rule-0 holes)
- [x] **TWIN-1** — converter-twin shape divergences, burned down entry by entry
- [x] **Advisory checks** — adjudicated binary-first; Mids retired as an authority

---

## Method notes

[Full detail](gaps/method-notes.md) — chance-0 templates are conditionals wearing a probability
field (METHOD-1); the audit's false-positive filters, in the order they must be applied
(METHOD-2); why a parity gate can't see a defect both engines inherit from one input, with the
roll call of the five that hid there (METHOD-3); and the five times a filed claim didn't survive
being measured (METHOD-4).

*Update this register whenever an audit leg is rerun or a recorded gap changes state. A gap
leaving this file must leave as fixed-with-a-guard, not silently deleted.*

*When an entry closes, tick its row and put the narrative in `gaps/`. Current frontier lists open
entries only. If a closure's story is sitting in this file, it's in the wrong file.*
