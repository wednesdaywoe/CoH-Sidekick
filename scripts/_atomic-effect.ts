/**
 * DSH4 — Closed atomic effect schema + canonical identity key.
 *
 * This encodes the Mids /
 * game / bin-parser atomic model — a power is a **flat array of atomic,
 * single-attrib effect records**, a compound effect = N sibling records
 * distinguished by damageType / mezType / pvMode / resistible. It is the schema the
 * converter's bag-of-~90-named-slots `PowerEffects` will be *projected from* (DSH6),
 * and the shape the oracle harness keys on (DSH5). This file defines the record, the
 * attrib→type bridge, and the identity keys — it does NOT yet rewrite the converter
 * (that is DSH6) nor read the Mids oracle (DSH5).
 *
 * Field provenance (each `AtomicEffect` field ← Mids `Effect` / bin-export template):
 *   effectType/subType ← export `attribs[]` string via `bridgeAttrib` (Mids splits
 *                        these into `EffectType` + `DamageType`/`MezType` natively).
 *   pvMode             ← export effect-group `is_pvp` (Mids `PvMode`/ePvX).
 *   resistible         ← NOT template `flags[].IgnoreResistance` (Mids `Resistible`).
 *   toWho              ← export template `target` (Mids `ToWho`/eToWho); replaces the
 *                        ad-hoc `selfPenalty` bag flag.
 *   attribType/aspect  ← export template `type`/`aspect` (Mids `AttribType`/`Aspect`).
 *   modifierTable      ← export template `table` (Mids `ModifierTable`).
 *   scale              ← export template `scale`, SIGN PRESERVED (the converter's
 *                        `makeEffect` does `Math.abs` at ingest — we stop that here).
 *   magnitude/duration/ticks/stacking/... ← the corresponding template fields.
 *   specialCase/conditionals ← group `requires_expression` gate (Mids
 *                        `SpecialCase`+`ActiveConditionals`); replaces the `domination`
 *                        bag bolt-on.
 */

/*
 * WHY THIS LIVES IN `scripts/` (moved 2026-09-24 from `src/data/core/atomic-effect.ts`).
 *
 * This is the converters' shared logic, not the oracle's. Twelve pipeline scripts call
 * into it — `convert-powerset.cjs` for `ingestTemplate`, `convert-pet-entities.cjs` for
 * `mapAttribType`, the collapse detectors for `ingestExportPower`, the sweeps for
 * `decodeAtoms`/`encodeAtom` — and it reads nothing back from them. It sat under `src/`
 * only because the TS planner was written first and the converters borrowed from it
 * where it stood.
 *
 * That address was the problem. `src/` is the REFERENCE IMPLEMENTATION the Rust engine
 * is graded against; a grader that the thing it grades depends on is not a grader. While
 * this file lived there, the path the shipped data travels ran export → converters →
 * oracle → contract → app, so "the oracle is frozen" and "the pipeline still works" were
 * two claims about one file, and no reader could tell which one a change was about.
 *
 * It has NO imports — it never did — so the move cost nothing and settled the direction:
 * the oracle may read the pipeline's core, the pipeline may not read the oracle. The
 * guard was `contract/` coming out byte-identical across all 1,446 files, and the
 * generated tree across all 26,627.
 *
 * Still TypeScript, and still loaded through `tsx`. Dropping that is a separate change
 * with its own risk (machine-stripping the types costs 788 of this file's 909 comment
 * lines, which is most of what it knows), and it is not an oracle question.
 */

// ============================================================================
// Enums (Mids-aligned; version- & handoff-stable — the structural TRUST set)
// ============================================================================

/** ePvX — which combat context this effect applies in. */
export type PvMode = 'Any' | 'PvE' | 'PvP';

/**
 * eAspect — which face of the attribute is modified.
 *
 * `Unspecified` means the source template STATED no aspect — it is not a synonym for
 * `Cur`. Homecoming and Rebirth always state one (0 empty aspects between them);
 * Thunderspy states one on 538 of 30,519 templates and leaves the rest blank, because
 * its exports carry no aspect field for the parser to read (only a prior fix's
 * synthesized `Resistance`/`Strength` are populated — see [[tspy-player-vocab-gap]]).
 *
 * This used to default to `Cur`, which fabricated "Current" for a template that said
 * nothing. That is the collapse Plan B exists to prevent, and it bit: the bag routes a
 * foe-targeted movement effect to `effects.movement` only on a literal
 * `aspect === 'current'` test, so a blank-aspect Thunderspy template is dropped by the
 * bag but was indistinguishable, on the wire, from a genuine HC `Current` one. Keeping
 * the two apart costs nothing — no consumer tests `=== 'Cur'` to mean "any", and every
 * `=== 'Res'/'Str'/'Max'` test excludes a blank aspect either way.
 */
export type Aspect = 'Res' | 'Max' | 'Abs' | 'Str' | 'Cur' | 'Unspecified';

/** eAttribType — how the scale is interpreted, one member per the parser's four
 *  `ATTRIB_MOD_TYPE` values (`_enums.py`, verified against the Ghidra keyword table).
 *  'Constant' used to be folded onto 'Magnitude' (ATTRTYPE-1), which laundered a parse
 *  fact into plausible data — the STACK-3 shape — even though no consumer distinguished
 *  the two; the fold is retired here so the type is what the game stores. */
export type AttribType = 'Magnitude' | 'Duration' | 'Constant' | 'Expression';

/**
 * eToWho — who the effect lands on. Replaces the ad-hoc `selfPenalty` flag: a foe-debuff
 * is `toWho:'Target'`, a genuine self-penalty is `toWho:'Self'`.
 *
 * One member per `ModTarget` (`Common/entity/attribmod.h:69`), because the game's seven
 * values are not four. `'All'` used to stand where two of them are now, and it was the
 * collapse this file exists to prevent (TARGETS-2): `SelfAndPets` anchors on the CASTER
 * and `AnyAffectedAndPets` anchors on whoever the power hit, and folding both onto one
 * member left nothing to tell a self-buff from a foe-facing one. Half the appliers then
 * read `'All'` as "lands on the caster" and half did not, so whether a Thunderspy Fly
 * atom reached your totals depended on which applier happened to ask.
 *
 * Names are the export's spellings, except `'Target'` — the export writes that one
 * `AnyAffected`, and the member predates this split by 25k atoms.
 *
 * | member              | export               | engine (`attribmod.h`)          |
 * |---------------------|----------------------|---------------------------------|
 * | `Self`              | `Self`               | `kModTarget_Caster`             |
 * | `SelfAndPets`       | `SelfAndPets`        | `kCastersOwnerAndAllPets`       |
 * | `TargetOnly`        | `TargetOnly`         | `kModTarget_Focus`              |
 * | `TargetOnlyAndPets` | `TargetOnlyAndPets`  | `kFocusOwnerAndAllPets`         |
 * | `Target`            | `AnyAffected`        | `kModTarget_Affected`           |
 * | `TargetAndPets`     | `AnyAffectedAndPets` | `kAffectedsOwnerAndAllPets`     |
 * | `Marker`            | `Marker`             | `kModTarget_Marker`             |
 *
 * The `…AndPets` members are not a recipient — they are an ANCHOR plus a pet copy. The
 * engine resolves the anchor to its top-level owner, attaches there, and then recurses
 * over that owner's pet list (`character_combat.c:749`). Use {@link landsOnCaster} rather
 * than testing members directly; it is the one place that reading is written down.
 */
export type ToWho =
  | 'Unspecified' | 'Self' | 'SelfAndPets'
  | 'Target' | 'TargetOnly' | 'TargetOnlyAndPets' | 'TargetAndPets' | 'Marker';

/** How repeated applications combine. Superset of Mids eStacking (No|Yes) plus the
 *  eleven members of the game's own `StackTypeEnum` (`Common/entity/attribmod.h`). */
export type Stacking =
  | 'No' | 'Yes' | 'Stack' | 'Replace' | 'Extend' | 'Refresh'
  | 'RefreshToCount' | 'Overlap' | 'Maximize' | 'Ignore' | 'Suppress'
  | 'StackThenIgnore' | 'Continuous';

/**
 * The primary effect classification. Mids stores `EffectType` (eEffectType) and a
 * separate `DamageType`/`MezType`; we keep that split — `effectType` here is the
 * gameplay category and `subType` carries the damage/mez/positional dimension. The
 * by-type protection split (Defense vs Resistance vs Elusivity) is NOT in the
 * bin-export attrib string — it is derived from `aspect`+`table` (see `bridgeAttrib`).
 */
export type EffectType =
  // offense / healing
  | 'Damage' | 'DamageBuff' | 'Heal' | 'HealResistance' | 'Absorb'
  // mitigation (by-type)
  | 'Defense' | 'Resistance' | 'Elusivity'
  // to-hit / accuracy
  | 'ToHit' | 'Accuracy'
  // control
  | 'Mez' | 'MezResist'
  // secondary-attribute strength buff (Power Boost family; Mids eEffectType.Enhancement)
  | 'Enhancement'
  // resource / survivability
  | 'Endurance' | 'EnduranceDiscount' | 'Recovery' | 'Regeneration'
  | 'MaxHP' | 'MaxEndurance'
  // utility stats
  | 'RechargeTime' | 'InterruptTime' | 'Range' | 'ThreatLevel' | 'Perception' | 'Stealth'
  // movement (subType: Run|Fly|FlyMode|Jump|JumpHeight|Control|Friction).
  // `Fly` is the FlyingSpeed buff; `FlyMode` is the kFly flight-mode grant
  // (magnitude = "can fly"), a different attrib entirely — see MOVEMENT_AXIS.
  | 'Movement'
  // meta / engine (grant/execute/summon/mode/etc. — not a numeric player stat)
  | 'GrantPower' | 'ExecutePower' | 'RechargePower' | 'GlobalChanceMod'
  | 'EntCreate' | 'Meta'
  // escape hatch — an attrib the bridge cannot confidently classify. Tracked as a
  // coverage gap, never silently mapped to a wrong slot.
  | 'Unmapped';

// ============================================================================
// The atomic effect record
// ============================================================================

export interface AtomicEffect {
  // --- identity-bearing (structural) ---
  effectType: EffectType;
  /** damage type (Smashing…), mez type (Held…), positional (Melee/Ranged/AoE),
   *  or movement axis (Run/Fly/…). One record per subType — the multi-type
   *  explosion. `undefined` for scalar effects (RechargeTime, ToHit, …). */
  subType?: string;
  pvMode: PvMode;
  /** first-class: absence of `IgnoreResistance` ⇒ resistible (never left agnostic). */
  resistible: boolean;
  toWho: ToWho;
  attribType: AttribType;
  aspect: Aspect;
  /** validated AT/pet table name; '' when the effect carries no table (rare). */
  modifierTable: string;
  /** SIGNED scale — sign preserved at ingest (a debuff is negative). */
  scale: number;

  // --- value / context (non-identity) ---
  magnitude: number;
  /**
   * Raw `magnitude_expression` — the CoH stack-machine program that computes this
   * atom's magnitude at runtime (evaluated by the expr VM), present when the value is
   * a formula rather than a fixed `scale`/`magnitude`. The meter-driven inherents live
   * here: Brute Fury's `Rage_Buff` carries `kRage source> .02 *` (2% damage-Strength
   * per Rage point) on each damage-type atom, so the calc reads the coefficient from
   * the data instead of a hardcoded constant. Absent for ordinary numeric-magnitude
   * atoms. Distinct from {@link requiresExpression}, which GATES rather than values.
   *
   * Only Homecoming's export currently carries it; the Rebirth/Thunderspy parsers drop
   * the expression string (their `Rage_Buff` damage atoms arrive value-less), so Fury is
   * not derivable there — a recorded data gap (DATA-GAP INHERENT-2), not a fallback.
   */
  magnitudeExpression?: string;
  duration: number;
  /**
   * Seconds after the cast this mod BEGINS — the AttribMod's own `Delay` plus every
   * enclosing effect group's, composed the way {@link requiresExpression} composes a
   * nested gate. Absent (and encoded absent) when the whole chain is zero, which is
   * the overwhelming majority.
   *
   * It is what separates a power's own effect from its CRASH. Rage states its +damage
   * and +ToHit at delay 0 for 120 seconds and its −20% defense at `Delay 120` for 10;
   * all three are `toWho: Self` rows of one power, and nothing else on them tells the
   * buff phase from the crash. Read it before counting a self-directed penalty as sustained state — the
   * whole caster-reaching −ToHit population of all three forks is a rez after-effect
   * at delay 60–90 (DATA-GAP DEFDEBUFF-1).
   *
   * NOT a duration offset for the ordinary case: half the delayed population is sub-second
   * animation timing (7.5k of the export's 14.7k delayed templates — a heal landing 0.25s
   * into its own cast), so "delayed" alone does not mean "later phase"; the magnitude is.
   */
  delay?: number;
  ticks?: number;
  applicationPeriod?: number;
  stacking: Stacking;
  stackCap?: number;
  /**
   * Raw `stack_key` — the binary's mutual-suppression group (`TravelBuff` on
   * Combat Jumping / Super Jump / Super Speed / Fly, `TravelMaxBuff` on their cap
   * raises, `Stealth` on the stealth family). Meaningful only alongside
   * `stacking: 'Suppress'`: powers sharing a key suppress each other per stat, so
   * only the strongest applies. A parser field (see `parse_stack_key_table`), NOT
   * a converter verdict — it carries here as data and the rule is applied by the
   * consumer.
   */
  stackKey?: string;
  /**
   * Raw `RequiredEvents` gate from the AttribMod tail, comma-joined in authored
   * order (`'Held,Sleep'`). The mod applies only when one of these events is
   * live on the target/caster — the mez-state bonus family (Sonic Thrust's
   * +Energy vs Held/Slept foes, Telekinesis's repel-while-Immobilized, Aura of
   * Insanity's per-mez debuffs, sleep-breaks-on-damage duration mods). A parser
   * field like {@link stackKey}, NOT a converter verdict — but the base
   * collectors treat any carrier as conditional, so every carrier atom is also
   * {@link gated}. The per-event post-event window (seconds) stays in the
   * export; this projection carries the event names only.
   */
  requiredEvents?: string;
  baseProbability: number;
  procsPerMinute?: number;
  /**
   * Raw template-level `tick_chance` — the roll this ONE template makes, distinct
   * from {@link baseProbability}, which is the enclosing effect group's. Carried
   * only when it is not 1, so the overwhelming majority of atoms encode unchanged.
   *
   * On a periodic template ({@link applicationPeriod} > 0) it is the per-tick apply
   * chance, and {@link cancelOnMiss} says how the misses compound.
   *
   * Whether it is a SECOND roll depends on the fork's schema, which the value itself
   * says: Parse6 has no effect group, so its one `Chance` reaches both this field and
   * {@link baseProbability} — a carrier repeating its group's own chance is one field
   * seen twice, not two rolls to compound. Homecoming and Thunderspy have both fields
   * and every carrier of theirs differs from its group (88 of Homecoming's are the
   * 0.998 to-hit roll the old bag comment named, the rest genuine per-template rolls
   * like Sound Cannon's 0.33 Knockback). It carries here as data so a consumer can
   * decide in the open; the damage calc folds the duplicate case and reports the rest.
   */
  tickChance?: number;
  /**
   * The template's `CancelOnMiss` flag — a periodic effect whose chain STOPS at the
   * first missed tick, so tick k needs k consecutive hits and the expected tick count
   * is the geometric sum Σ chance^k rather than n·chance. Carried only alongside a
   * {@link tickChance}, which is the only thing that makes it readable: with every
   * tick certain there is no miss to cancel on.
   */
  cancelOnMiss?: boolean;
  /**
   * The enclosing effect group's authored `Tag` list, comma-joined in source order
   * (`'CritLarge,ScrapperCrit_ST'`). This is where the game NAMES a mechanic: the
   * archetype hit-time forks tag themselves (`ScrapperCrit_ST` / `ScrapperCrit_AoE`,
   * `SentCrit`, `StealthCrit`, `ASTeamCrit`, `Containment`, `PvPCrit`), and
   * `FieryEmbrace` names the buff that wakes a dormant component — the thing an atom
   * with `baseProbability: 0` otherwise leaves unsaid.
   *
   * A parser field, NOT a converter verdict. HOMECOMING ONLY, and that is a schema
   * fact rather than a parse gap: Parse6 (Rebirth, Thunderspy) stores AttribMods flat
   * with no EffectGroup wrapper to hang a Tag on, so both forks carry none at all
   * (0 of 135k groups). A consumer that cannot name a mechanic on those forks must
   * say so rather than infer one from an archetype.
   */
  tags?: string;

  // --- enhancement / calc flags ---
  buffable?: boolean;
  ignoreED?: boolean;
  ignoreScaling?: boolean;
  ignoreStrength?: boolean;

  // --- conditional gate (replaces the `domination`/`selfPenalty` bolt-ons) ---
  specialCase?: string;
  /** raw gate expression (CoH stack-machine string) or Mids (key,value) pairs. */
  requiresExpression?: string[];
  /**
   * True when this atom is NOT part of the power's unconditional base — it
   * applies only under a gate (mode/stance, PvP, hidden-state, Containment,
   * dead-state, `rand()`, a chance-0 proc trigger). Absent ⇒ always-on base.
   *
   * `requiresExpression` says WHAT the gate is; this is the converter's verdict
   * on whether it fires by default. The two are not redundant: base atoms can
   * carry a requires (the PvE `enttype` filter, a negated "target NOT drowning"
   * clause) and still be unconditional, and the classification depends on
   * collection PROVENANCE as well as the expression — a template reached via a
   * redirect chain or `activation_effects` passes through filters (Self-only,
   * IgnoreStrength-dupe removal) that no gate expression records. Only the
   * converter has that context, so it decides here rather than leaving the
   * runtime to re-derive a lossy heuristic.
   *
   * Read via `baseAtoms()` / `gatedAtoms()` in `atom-query.ts`. Exact by
   * construction and verified corpus-wide: `baseAtoms(power.atoms)` reproduces
   * the converter's own base template set (`scripts/planb-shadow-bag.cjs`).
   */
  gated?: boolean;

  /**
   * Per-target increment this atom contributes — an AoE self-buff that grows
   * with the number of foes hit (Soul Drain's +ToHit/+Dmg, Invincibility's
   * +Def/+ToHit, Consume Psyche's +Regen). Present ONLY on the increment atoms
   * of a per-target group; absent on flat base atoms and non-AoE powers.
   *
   * STAMPED BY THE CONVERTER, not re-derivable at runtime — the bag's
   * `{ scale, perTarget }` comes from `computeAoePerTargetPatches`, which reads
   * AoE geometry (`max_targets_hit`, `targets_affected`), redirect-chain
   * `number_allowed`, Defiance tags, and the raw `Continuous` stack flavor. The
   * geometry does not survive to the runtime — `targets_affected` is not emitted
   * and redirect/Defiance provenance is not on the atom — so, exactly like
   * {@link gated}, the converter decides and stamps the verdict. (`Continuous`
   * itself now reaches the atom; it stopped folding to `No` with STACK-3. It is
   * still not sufficient on its own — see `computeAoePerTargetPatches`, where the
   * stack flavor is one of several terms.)
   *
   * Reconstruct the bag value with `perTargetValueOf` in `atom-query.ts`:
   * `perTarget = Σ atom.perTarget`, `scale (at N=1) = Σ |atom.scale|` over the
   * slot's atoms. Verified bag-equal corpus-wide by `scripts/planb-shadow-pertarget.cjs`.
   */
  perTarget?: number;

  /**
   * This atom's share of a redirect chain's BASE contribution to its slot — the other arm of
   * the branch {@link perTarget} covers, and PERFOE-2.
   *
   * `detectStackingEffects` walks an `Execute_Power` chain into another power's file and adds
   * what it finds to the patch: as `perTarget` when the outer row targets `AnyAffected` or the
   * redirect declares `number_allowed > 1`, and as `scale` otherwise. Fulcrum Shift and Kinetic
   * Transfer take their base 4 from `Redirects.Kinetics.KineticTransferBuffSelf` that way,
   * Siphon Power its 2 from `Redirects.Kinetics.SiphonPower`.
   *
   * STAMPED BY THE CONVERTER for exactly {@link perTarget}'s reason and by the same replay: the
   * value is computed by walking a chain into a file this power does not own, the chain's own
   * templates are parsed separately and never become this power's atoms, and the redirect's
   * `number_allowed` is not on the wire — so nothing here can re-derive it. The replay matches
   * by `(|scale|, table)`, and the two arms never claim the same template: a per-foe increment
   * and a base one-shot differ in scale.
   *
   * Present ONLY on the atoms of such a chain's base arm; absent everywhere else. A consumer
   * rebuilding the patched slot sums the DISTINCT stamps, the way it sums {@link perTarget}.
   */
  redirectBase?: number;

  /**
   * True when this atom is combat-SUPPRESSED — the game turns it off while the
   * caster is in combat (Hide's +Def, Stealth/Cloaking Device's +Def, and every
   * travel-power speed/cap buff), and the planner's In-Combat toggle removes it
   * from totals. The bag models this by routing the effect to a parallel slot
   * (`defenseBuffSuppressible`) or bolting a `suppressible: true` flag onto the
   * movement `ScaledEffect` — one more discriminator the single-valued slot
   * forces it to re-materialize per effect type.
   *
   * STAMPED BY THE CONVERTER, not re-derivable at runtime — exactly like
   * {@link gated} and {@link perTarget}. The suppression comes from two sources
   * the converter folds into one verdict (`isCombatSuppressed`):
   *   - a `Suppress ActivateAttackClick` (et al.) event on the template's
   *     `suppress_events` tail — Hide's AoE-defense suppression, travel speed —
   *     which is NOT part of the wire atom, so the runtime cannot see it;
   *   - an `OutOfCombat` `requires` gate (also surfaced as `specialCase`).
   * Because the event half never reaches the runtime, the converter decides and
   * stamps here (see `encodeAtomsForEmit`). Read as a plain boolean: an applier
   * splits `baseAtomsOfType(power,'Defense')` on `!!a.suppressible` to reproduce
   * the bag's `defenseBuff` (always-on) vs `defenseBuffSuppressible` (drops in
   * combat) pair. Verified bag-equal corpus-wide by `scripts/planb-shadow-defense.cjs`.
   */
  suppressible?: boolean;

  /**
   * True when this atom's effect does NOT land on the CASTER, despite routing to
   * a slot the caster's totals read. Thunderspy's `_Ones`-table resource buffs are
   * the whole population: a Mastermind pet-equip power (Equip Thugs, Train Ninjas)
   * or a foe attack (Disrupting Torrent) carries a +Recovery/+Regeneration template
   * that buffs the PET or the FOE, not the player — a target-trap the bag resolves
   * by DELETING the slot (`guardThunderspyOnesBuffs`).
   *
   * STAMPED BY THE CONVERTER, not re-derivable at runtime — like {@link gated},
   * {@link perTarget} and {@link suppressible}. The verdict is a heuristic over the
   * power's `shortHelp` TEXT ("Self +Recovery" ⇒ the buff is genuinely the
   * caster's, so `targets_affected` merely under-reports) plus `targets_affected`
   * itself; neither is on the wire, and the trapped atoms are `toWho:
   * Unspecified/Target` — byte-identical to the legitimate HC Target-recovery buffs
   * the bag KEEPS, so no atom field can distinguish them.
   *
   * Stamped rather than dropped from `power.atoms` because the atom is still real:
   * it describes what the pet/foe receives and feeds pet display. Only the caster's
   * totals must ignore it, which `regenBuffValue`/`recoveryBuffValue` do by
   * filtering it out. NB it is deliberately NOT {@link gated} — the hard base-set
   * invariant at the emit site asserts the unstamped atoms are exactly
   * `templatesToAtoms(allTemplates)`, and a trapped template IS in `allTemplates`.
   */
  notOnCaster?: boolean;

  /**
   * The `EntsAffected` of the power this atom LIVES on, when that is not the power
   * carrying it. Absent means the carrier is also the owner, which is the ordinary case.
   *
   * A `toWho: 'Target'` atom names no recipient: `AnyAffected` means "whoever this power
   * affects", so only `targetsAffected` says whether the caster is one of them. That is a
   * power-level field, and a collector that follows a redirect or an `Execute_Power`
   * attaches the child's AttribMods to the SHELL, whose list answers about the shell. The
   * pool's Spring Attack is `['Self']` because the parent teleports you, while the foe
   * knockback it pulls in belongs to a `['Foe']` power; Trick Arrow's EMP Arrow is a
   * `['Self']` shell over a `['Friend']` field, so the field's buffs are the team's and not
   * the caster's. Reading the shell's list credits the caster with both (TARGETS-3).
   *
   * STAMPED BY THE CONVERTER, not re-derivable at runtime — like {@link gated},
   * {@link perTarget}, {@link suppressible} and {@link notOnCaster}. The redirect chain is
   * only walkable at convert time, and nothing on the wire records that a walk happened.
   * `reachesCaster` reads this in place of the power's own list; every other consumer of
   * `targetsAffected` is asking about the power, not about one atom, and leaves it alone.
   */
  ownerTargets?: readonly string[];

  /**
   * The archetypes this atom is base FOR, comma-joined in the export's own
   * `Class_*` spelling. Absent means every archetype — the ordinary case.
   *
   * An effect group gated `arch source> Class_Scrapper eq` is genuinely part of
   * a Scrapper's base and genuinely absent from everyone else's, and one
   * base/conditional boolean cannot say that. Rebirth spells the fork out as a
   * pair: Tough carries a Kheldian arm and a thirteen-archetype arm, and while
   * both sat in base every Rebirth build read 3.0 S/L resistance where
   * Homecoming and Thunderspy read 1.5 (DATA-GAP-REGISTER AT-FORK-1).
   *
   * The parser resolves the fork by evaluating the gate once per archetype the
   * dataset defines and exporting the satisfied side (`requires_archetypes`);
   * the converter carries it here, and the gather drops the atom when the build
   * is not one of them. Kept as BASE rather than {@link gated} on purpose: the
   * conditional path re-admits gated atoms as slot-less synthetics with empty
   * strength, which is right for the `ignoreStrength` mode atoms it exists for
   * and would silently un-enhance Tough's resistance.
   *
   * Not re-derivable at runtime from {@link requiresExpression}: deciding it
   * needs the dataset's archetype roster and a three-valued walk that keeps a
   * definite `false` under an indeterminate sibling, which the engine's eager
   * evaluator does not do.
   */
  casterArchetypes?: string;

  /**
   * The export attrib this atom came from, lowercased — carried ONLY where
   * {@link effectType} + {@link subType} cannot name it, which today is exactly
   * `Meta`.
   *
   * Every other effect type names its own attrib: `Defense`/`Ranged` IS
   * `Ranged`, `Damage`/`Fire` IS `Fire_Dmg`. `Meta` is the one bucket — the
   * ~44 non-stat engine markers in {@link META_EFFECT} (`meter`, `rage`,
   * `set_mode`, `set_token`, `designer_status`, the travel stances…) all
   * collapse onto `Meta` with NO subType, so which marker an atom is was
   * unrecoverable at runtime. That is a many-to-one map, not a coverage gap:
   * these are markers rather than numeric stats, so no applier wants a subType
   * for them — but a GATE can name one, and then the collapse bites.
   *
   * CHAIN-1 is where it bit. `kMeter` is a single character attribute that ten
   * different mechanics publish (Hide, Placate, Domination, Defiance,
   * Opportunity, Fury/Rage, Primal Energy, Battle Euphoria, Pack Mentality) —
   * the game reuses one slot because a character has exactly one meter
   * mechanic, its archetype's. So "is my meter the HIDE meter" is a question
   * about which marker the build's own powers publish, and without this field
   * the shape that comes closest (Self-targeted + combat-suppressed `Meta`)
   * also matches `rage` on Rage_Dampen, `designer_status` on the Teleport
   * family, and `set_mode` on **Engagement** — which is auto-issued to every
   * character, so every build in the game would have matched.
   *
   * Scoped to `Meta` on the trailing-null economics every field past
   * `requiresExpression` is placed by: ~1.6k of Homecoming's atoms are `Meta`,
   * so every other atom's encoding stays byte-identical. `Unmapped` is the
   * other many-to-one collapse and deliberately does NOT carry this — it is a
   * tracked coverage gap (ATOMIC-STATE-AUDIT), its members feed no gate, and it
   * is a far larger population. If one ever needs naming, widen here.
   *
   * The value is the key that SELECTED `Meta` — `META_EFFECT[metaAttrib] ===
   * 'Meta'` holds for every carrier, which is what lets a gate assert coverage
   * rather than trust the spelling.
   */
  metaAttrib?: string;

  /**
   * Event names that suppress this effect while they're recent, verbatim from the
   * template's `suppress_events` tail, with {@link suppressSeconds} carrying the
   * window. {@link suppressible} folds this same tail into one combat-suppression
   * verdict; these fields are the tail itself, for the consumer that needs the
   * clock rather than the verdict. RB5-d's per-cast walk is that consumer: Hide's
   * meter suppresses on Attacked/Damaged at 8.0s, so an attack drops the meter for
   * 8 seconds and a gapped rotation re-hides.
   *
   * Emitted ONLY on `Meta` atoms, like {@link metaAttrib} and on the same
   * economics: 6,485 templates corpus-wide carry a suppress tail (mostly the
   * generic mez/travel suppression the verdict already answers for) and the Meta
   * population is ~256 templates across the three forks. A non-Meta consumer that
   * ever needs the tail widens the scope at the `ingestTemplate` stamp site.
   */
  suppressEvents?: readonly string[];

  /**
   * The suppress window in seconds, one value per template. The export states a
   * per-event duration; every emitted (Meta) template's events agree, and the
   * converter THROWS on disagreement rather than collapsing it (the 7 known
   * non-uniform templates, Illusion Invisibility's, are all non-Meta). Present
   * exactly when {@link suppressEvents} is.
   */
  suppressSeconds?: number;

  /**
   * The suppress tail's `always` flag, uniform per emitted template and guarded
   * like {@link suppressSeconds}. False on exactly one carrier today, inherent
   * Engagement's `Set_Mode`. Present exactly when {@link suppressEvents} is.
   */
  suppressAlways?: boolean;

  /**
   * Event names that cancel this effect outright, verbatim from the template's
   * `cancel_events` tail. Meta-scoped like {@link suppressEvents}. Placate's
   * meter cancels on Attacked/Damaged/MissionObjectClick: the 10s re-hide dies
   * the moment you act, which is the from-Hide position rule RB5-d schedules by.
   */
  cancelEvents?: readonly string[];

  /**
   * When the game applies this row, verbatim from the template's
   * `application_type`. Six values corpus-wide: `OnTick` (180,345), `OnActivate`
   * (8,793), `OnEnable`/`OnDisable` (1,806 each), `OnDeactivate` (294) and a lone
   * `OnExpire`.
   *
   * Carried because it is a discriminator, not bookkeeping. `OnDeactivate` marks a
   * toggle's shutdown burst — the rows that fire when the power turns OFF — and the
   * bag has skipped them at its routing pass since it was written
   * (`convert-powerset.cjs:5759`). The atom stream did not, so those rows rode the
   * wire ungated and indistinguishable from a standing effect, and the movement
   * reader had to reconstruct the verdict by matching `+X` against `−X` on the same
   * axis (MOVEMAP-6). Emitting the field replaces that inference with the fact.
   */
  applicationType?: string;

  /**
   * The caster-side window this row opens, in seconds, when the row IS the power's
   * summon: the pet's lifespan, stamped by the converter's own summon resolution
   * (`extractSummon` / `rebuildTierConditionalSummon`, both through
   * `resolvePetLifespan`).
   *
   * Distinct from {@link duration}, and the distinction is the whole point.
   * `duration` says how long THIS row's entity lives; it cannot say which row
   * constitutes the power's window, and the corpus holds three shapes that no rule
   * over `duration` separates: Homecoming Soul Extraction (three tier-gated ghosts,
   * exactly one materializes — an ungated-only read says the power summons nothing),
   * Victory Rush (six rank-gated 2-second `PL_StaticObject`s carrying a buff, no pet
   * kept — a gated-inclusive read invents a 2-second window), and Rebirth Soul
   * Extraction (the raw template states no duration while the pet lives 300).
   * Separating them at runtime means testing `Class_*_Henchman` in an `if`, which is
   * Rule 0's ban; the converter already knows which template it read, so it says so
   * here (DATA-GAP-REGISTER ENT-14).
   *
   * Present on a few hundred atoms corpus-wide and absent everywhere else, so it is
   * the claim itself — a summon row without one is an entity the power creates but
   * does not keep.
   */
  summonWindow?: number;

  /**
   * The `conditionalEffects` entry this atom belongs to, by that entry's `id` — the join
   * between a per-power adjuster and the atoms it turns on. Absent means the atom belongs
   * to no surviving entry, which is the ordinary case for a base atom AND for a
   * {@link gated} one whose gate the conditional extractor does not surface (a PvP
   * `enttype` pair, a chance-0 proc, an out-of-combat gate).
   *
   * STAMPED BY THE CONVERTER, like {@link gated} and its siblings, and for the same kind of
   * reason: the id is not a property of the atom's gate alone. `_classifyGateExpression`
   * folds the POWERSET key and the gate's referenced power name to mint it, then
   * `extractConditionalEffects` discards the groups that project to no payload — so the
   * set of ids that exist is a whole-power verdict, and an atom's membership in one cannot
   * be recomputed from `requiresExpression` without re-implementing that classifier and
   * its survivability filter. A second implementation of a gate classifier is exactly the
   * drift the chain-window migration measured at a third of the corpus, and here it would
   * fail SILENTLY — an entry that joins no atoms reports an empty key set rather than an
   * error (Rule 1).
   *
   * Stamped on the group's own templates, so an atom carries it whether or not the entry
   * ends up surfaced for this build's archetype: {@link casterArchetypes} answers that
   * question separately, on the entry.
   */
  conditionalId?: string;

  /**
   * The template's `StackByAttribAndKey` flag, verbatim. The game keys the buff by
   * (attrib, {@link stackKey}) instead of by casting power, so a re-application
   * REFRESHES the existing mod rather than adding a second one — which is what lets
   * Icy Bastion's toggle re-execute sixty times without stacking to +24,000% regen.
   *
   * A parser field like {@link stackKey}, NOT a converter verdict: it carries here as
   * data and the rule is applied by the consumer. Absent means the flag was not set.
   *
   * Carried because the flag ALONE means refresh semantics, while the flag beside
   * `stacking: 'Stack'`/`'Continuous'` means something else entirely — a per-target
   * increment that `computeAoePerTargetPatches` folds separately, so the resource
   * router must skip it or double-count it. The converter has read that pair off the
   * raw template since the beginning; nothing on the wire could ask, so the atom mirror
   * substituted "the row is Stack/Continuous AND its stackKey is non-empty" on the
   * reasoning that a keyed row was the flag's surviving spelling. Reactive
   * Regeneration falsified it: five flagged `Stack` templates with no stack key at all,
   * so the proxy declined to skip and the projection wrote a `regenBuffUnenhanced` the
   * bag never had (DATA-GAP-REGISTER STACK-5). 505 Homecoming templates carry the flag
   * and 10 Thunderspy ones; Rebirth authors none (its neighbouring bit 19,
   * `StackExactPower`, is on 3,108 templates, so the word is being read).
   */
  stackByAttribAndKey?: boolean;

  // --- provenance (debugging + DSH6 migration) ---
  sourceAttrib?: string;
}

// ============================================================================
// Positional tuple codec — the runtime wire format (Plan B, Phase 0)
// ============================================================================
//
// The generated `Power` carries its atom list as `atoms: EncodedAtom[]` — a
// positional array per atom rather than a keyed object, chosen to keep the
// committed generated tree (and the runtime chunk) small: field NAMES dominate
// the object encoding, so dropping them cuts ~5× (measured: 619 → 124 B/atom on
// the HC corpus). The `effects` bag stays human-readable during migration; only
// the new `atoms` field is tuple-encoded. Consumers decode with `decodeAtoms`.
//
// `sourceAttrib` (debugging provenance) and every converter-local `_`-prefixed
// field are deliberately NOT part of the wire format — only the canonical
// AtomicEffect schema ships. `ATOM_TUPLE_FIELDS` is the ONE source of truth for
// field order; both the converter's encoder and the runtime decoder read it, so
// adding/reordering a field is a single edit here.

/** Canonical AtomicEffect fields, in wire order. Identity/value fields first so
 *  the rarely-set flags fall at the tail and trim away (encoder drops trailing
 *  nulls). `sourceAttrib` and `_`-prefixed provenance are intentionally absent. */
export const ATOM_TUPLE_FIELDS = [
  'effectType', 'subType', 'scale', 'magnitude', 'duration', 'modifierTable',
  'aspect', 'attribType', 'toWho', 'pvMode', 'resistible', 'stacking',
  'stackCap', 'ticks', 'applicationPeriod', 'baseProbability', 'procsPerMinute',
  'ignoreStrength', 'buffable', 'ignoreED', 'ignoreScaling',
  'specialCase', 'requiresExpression',
  // `gated`, `perTarget` and `suppressible` sit LAST on purpose: ~64% of atoms
  // are base (gated absent) and only a few hundred carry a per-target increment
  // or a combat-suppression flag, and the encoder trims trailing nulls, so the
  // common case costs zero extra bytes. `perTarget` follows `gated` so a base
  // per-target increment (Soul Drain: gated absent, perTarget present) costs just
  // the one interior null; `suppressible` follows both — the suppressed set
  // (Hide +Def, travel buffs) is neither gated nor per-target, so it only pays
  // for the interior nulls between `baseProbability` and itself, and appending it
  // (rather than reordering) leaves every non-suppressed atom's encoding untouched.
  // `notOnCaster` (the Thunderspy resource target-trap) is rarest of all — a few
  // dozen atoms — so it appends last for the same reason. `stackKey` follows it:
  // only 0.8% of templates carry one (and only ~320 pair it with `Suppress`, the
  // one flavor that means anything), so appending keeps every other atom's
  // encoding byte-identical and the keyed few pay a handful of interior nulls.
  // `magnitudeExpression` appends after all of them: only ~757 Homecoming atoms carry
  // one (the meter/expression-valued powers — Fury's Rage_Buff among them) and none on
  // Rebirth/Thunderspy, so every other atom's encoding stays byte-identical (a trailing
  // null trims away) and the expression-valued few pay the interior nulls.
  // `requiredEvents` appends after `magnitudeExpression` for the same
  // trailing-null economics: only ~48 Homecoming templates (and their
  // Rebirth twins) carry an event gate, so every other atom's encoding
  // stays byte-identical.
  'gated',
  'perTarget',
  'suppressible',
  'notOnCaster',
  'stackKey',
  'magnitudeExpression',
  'requiredEvents',
  // `tickChance` and `cancelOnMiss` append last on the same economics: only a few
  // hundred templates per fork carry a non-1 tick chance, so
  // every other atom's encoding stays byte-identical and the periodic few pay the
  // interior nulls. `cancelOnMiss` follows `tickChance` because it is only ever
  // emitted beside one.
  'tickChance',
  'cancelOnMiss',
  // `tags` appends last on the same trailing-null economics: only ~a third of
  // Homecoming groups carry one and neither Parse6 fork carries any, so every
  // untagged atom's encoding stays byte-identical.
  'tags',
  // `casterArchetypes` appends after `tags` for the same reason: a few hundred
  // atoms per fork are archetype-forked and every other atom's encoding stays
  // byte-identical (a trailing null trims away).
  'casterArchetypes',
  // `metaAttrib` appends last on the same economics: only `Meta` atoms carry one
  // (~1.6k of Homecoming's), so every other atom's encoding stays byte-identical.
  'metaAttrib',
  // `ownerTargets` appends after it for the same reason: only an atom a collector pulled
  // out of another power's file carries one, so every atom that lives on its own power
  // stays byte-identical.
  'ownerTargets',
  // `delay` appends last on the same trailing-null economics: about 6% of atoms carry a
  // non-zero one, so every undelayed atom's encoding stays byte-identical. Appended
  // rather than placed beside `duration`, where it belongs by meaning, because moving a
  // field re-encodes the whole corpus for no gain.
  'delay',
  // The suppress/cancel event tails append after `delay`, Meta-scoped like
  // `metaAttrib` and on the same economics: ~256 Meta templates across the three
  // forks carry one, so every other atom's encoding stays byte-identical.
  // `suppressible` folds the suppress tail to a verdict; RB5-d's per-cast walk
  // needs the clock itself (Hide's 8s window, Placate's cancel list).
  'suppressEvents',
  'suppressSeconds',
  'suppressAlways',
  'cancelEvents',
  // `applicationType` appends last on the same trailing-null economics, and it is the
  // one field whose absence carries a MEANING rather than a silence: the encoder omits
  // `OnTick`, so 94% of atoms keep a byte-identical encoding and only the ~13k
  // event-applied rows pay the interior nulls.
  //
  // That is a compression convention and not a defaulted axis, but only because the
  // export is never silent here — every template on all three forks states an
  // `application_type` (measured: zero absent of 192,251). The converter ASSERTS that
  // rather than trusting it, so the day a template arrives without one the build stops
  // instead of quietly calling it standing. Absence on the wire therefore means "the
  // export said OnTick", which is a fact; it never means "the export said nothing".
  'applicationType',
  // `summonWindow` appends last on the same trailing-null economics: only the rows the
  // converter's summon resolution CLAIMS carry one (a few hundred corpus-wide against
  // ~1k EntCreate atoms), so every other atom's encoding stays byte-identical.
  'summonWindow',
  // `conditionalId` appends last on the same trailing-null economics: only the atoms a
  // surviving `conditionalEffects` entry claims carry one (~2.5k corpus-wide against
  // ~190k), so every other atom's encoding stays byte-identical.
  'conditionalId',
  // `stackByAttribAndKey` appends last for the same reason: 515 templates across the
  // three forks carry the flag (505 Homecoming, 10 Thunderspy, 0 Rebirth), so every
  // unflagged atom's encoding stays byte-identical and the flagged few pay the interior
  // nulls.
  'stackByAttribAndKey',
  // `redirectBase` appends last on the same trailing-null economics: only the atoms the
  // Execute_Power redirect branch's BASE arm claims carry one (Homecoming's Kinetics family,
  // 8 slot values corpus-wide), so every other atom's encoding stays byte-identical.
  'redirectBase',
] as const satisfies ReadonlyArray<keyof AtomicEffect>;

/** One atom, positionally encoded. A `null` at position `i` means the field
 *  `ATOM_TUPLE_FIELDS[i]` is absent; trailing nulls are trimmed, so a short
 *  array leaves every field past its end absent. */
export type EncodedAtom = ReadonlyArray<string | number | boolean | readonly string[] | null>;

/** Encode one AtomicEffect to its positional tuple (trailing nulls trimmed). */
export function encodeAtom(a: AtomicEffect): EncodedAtom {
  type Slot = string | number | boolean | readonly string[] | null;
  const t: Slot[] = ATOM_TUPLE_FIELDS.map((f) => {
    const v = a[f];
    // A gate expression is a token list, and a list with no tokens states nothing —
    // the same fact as carrying no gate. Encoding it as `[]` would put a value where
    // the field is absent, and every reader that asks "is there a gate here?" would
    // start answering yes (COND-8).
    if (Array.isArray(v) && v.length === 0) return null;
    return v === undefined ? null : (v as Slot);
  });
  while (t.length > 0 && t[t.length - 1] === null) t.pop();
  return t;
}

/** Decode a wire atom list back to AtomicEffect records. `null` / past-end
 *  positions restore to `undefined`; every stored non-null value round-trips. */
export function decodeAtoms(encoded: readonly EncodedAtom[] | undefined): AtomicEffect[] {
  if (!encoded) return [];
  return encoded.map((tuple) => {
    const a = {} as Record<string, unknown>;
    for (let i = 0; i < ATOM_TUPLE_FIELDS.length; i++) {
      const v = tuple[i];
      if (v !== undefined && v !== null) a[ATOM_TUPLE_FIELDS[i]] = v;
    }
    return a as unknown as AtomicEffect;
  });
}

// ============================================================================
// Canonical identity keys
// ============================================================================

const KEY_SEP = '|';

/**
 * Full canonical identity key (plan §DSH4). Two records with the same key are the
 * SAME application and may merge; a different key is a genuinely distinct sibling.
 * Includes `round(scale,4)` so the resistible/unresistable twin (identical structure,
 * half scale) and duration-variant stacks stay distinct. Used by the converter's
 * dedup/merge (DSH6) and the collapse detector (DSH3/DSH6).
 */
export function identityKey(e: AtomicEffect): string {
  return [
    e.effectType,
    e.subType ?? '',
    e.pvMode,
    e.resistible ? 'R' : 'U',
    e.toWho,
    e.attribType,
    e.aspect,
    e.modifierTable.toLowerCase(),
    round4(e.scale),
  ].join(KEY_SEP);
}

/**
 * Reduced STRUCTURAL key (plan §guardrail 1) — `(effectType, subType, pvMode,
 * resistible, modifierTable)`, no scale/aspect/attribType/toWho. Used by the oracle
 * differential harness (DSH5) where exact scale is skew-distrusted; canonicalize
 * BOTH sides to a multiset of these before diffing.
 */
export function structuralKey(e: AtomicEffect): string {
  return [
    e.effectType,
    e.subType ?? '',
    e.pvMode,
    e.resistible ? 'R' : 'U',
    e.modifierTable.toLowerCase(),
  ].join(KEY_SEP);
}

function round4(n: number): string {
  // stable, locale-independent 4-dp string; avoids -0 and float noise in the key.
  const r = Math.round((n + Number.EPSILON) * 1e4) / 1e4;
  return (Object.is(r, -0) ? 0 : r).toString();
}

// ============================================================================
// attrib → (effectType, subType) bridge
// ============================================================================

export interface BridgeResult {
  effectType: EffectType;
  subType?: string;
  /** set when effectType === 'Unmapped' — why the bridge declined (coverage gap). */
  reason?: string;
}

/** damage-type dimension normalization → canonical subType (Mids eDamage names). */
const DAMAGE_SUBTYPE: Record<string, string> = {
  smashing: 'Smashing', lethal: 'Lethal', fire: 'Fire', cold: 'Cold',
  energy: 'Energy', negative_energy: 'Negative', negative: 'Negative',
  toxic: 'Toxic', psionic: 'Psionic', special: 'Special',
  melee: 'Melee', ranged: 'Ranged', area: 'AoE', aoe: 'AoE',
  // exotic damage-only types seen in the HC export (Kheldian / signature)
  radiation: 'Radiation', electrical: 'Electrical', quantum: 'Quantum',
  sonic: 'Sonic', unique1: 'Unique1', unique2: 'Unique2', unique3: 'Unique3',
};

/** mez-name attribs → canonical mez subType (Mids eMez names; Knockback/Knockup/
 *  Repel are eMez in the canonical model even though the UI treats KB specially). */
const MEZ_SUBTYPE: Record<string, string> = {
  stunned: 'Stunned', held: 'Held', immobilized: 'Immobilized', sleep: 'Sleep',
  confused: 'Confused', terrorized: 'Terrorized', afraid: 'Afraid',
  placate: 'Placate', taunt: 'Taunt', teleport: 'Teleport', intangible: 'Intangible',
  untouchable: 'Untouchable', onlyaffectsself: 'OnlyAffectsSelf',
  combat_phase: 'CombatPhase', knockback: 'Knockback', knockup: 'Knockup',
  repel: 'Repel', evade: 'Evade',
  // Thunderspy respells two applied-mez attribs where HC/Rebirth use the canonical
  // participle (`Stun`→`Stunned`, `Immobilize`→`Immobilized`), on the matching
  // `*_Stun`/`*_Immobilize` tables. Same present-but-respelled class as the movement
  // vocab below and the converter's own MEZ_TYPES (which already reads both spellings, so
  // the effects bag already credits these) — mapping here converges the atom to the bag,
  // recovering the tspy `specialBuff.stun` mez-STRENGTH the Rust reader was reading as 0
  // (Conserve Power / Energize). TSPY-3 step 2. [[tspy-player-vocab-gap]]
  stun: 'Stunned', immobilize: 'Immobilized',
};

/** scalar-stat attribs → effectType (no subType). */
const SCALAR_EFFECT: Record<string, EffectType> = {
  // InterruptTime surfaced 2026-07-20 when ATTRIB_NAME id 91 stopped
  // misdecoding (incarnate alpha_silent interrupt boosts are its only HC
  // carriers) — scalar activation-interrupt window, RechargeTime's sibling.
  rechargetime: 'RechargeTime', interrupttime: 'InterruptTime', endurance: 'Endurance',
  endurancediscount: 'EnduranceDiscount', recovery: 'Recovery',
  regeneration: 'Regeneration', hitpoints: 'MaxHP', accuracy: 'Accuracy',
  tohit: 'ToHit', range: 'Range', threatlevel: 'ThreatLevel',
  perceptionradius: 'Perception', absorb: 'Absorb',
};

/**
 * [BRIDGE-2] stealth-radius attribs → Stealth/<axis>.
 *
 * The bin export carries the PvE and PvP stealth radii as DISTINCT attribs
 * (`StealthRadius_PvE` / `StealthRadius_PvP`) and `Translucency` as the visual-alpha
 * component — three faces of the Stealth family. Without a subType all three collapse to
 * one indistinguishable `Stealth` atom (within a power the two radii differ ONLY in raw
 * scale), which is exactly why the stealth applier could not read atoms PvE-vs-PvP. The
 * subType names the axis so they stay separable — the bag already keeps them apart in
 * `stealthPvE`/`stealthPvP`. Named `Radius*` (not bare `PvE`/`PvP`) to avoid colliding with
 * the separate `pvMode` field.
 */
const STEALTH_AXIS: Record<string, string> = {
  stealthradius_pve: 'RadiusPvE', stealthradius_pvp: 'RadiusPvP', translucency: 'Translucency',
};

/**
 * movement attribs → Movement/<axis>.
 *
 * `fly` (kFly) and `flyingspeed` are DIFFERENT attribs and must not share an axis.
 * kFly is the flight-MODE grant — its scale is a mode magnitude ("can fly": Hover
 * 4.0, Fly 2.0), not a speed percentage — while FlyingSpeed is the actual speed
 * buff. Mapping both to `Fly` made the pair unrecoverable from the wire on the 32
 * powers carrying both (Hover is the worst: kFly 2.0 and FlyingSpeed 0 share a
 * `Melee_Ones` table, so scale and table cannot separate them either), and reading
 * the grant as a speed buff double-counts Fly by +200% — the bug the bag's own
 * `movement.fly` / `movement.flySpeed` split exists to avoid.
 */
const MOVEMENT_AXIS: Record<string, string> = {
  runningspeed: 'Run', flyingspeed: 'Fly', jumpingspeed: 'Jump',
  jumpheight: 'JumpHeight', fly: 'FlyMode', movementcontrol: 'Control',
  movementfriction: 'Friction',
  // Thunderspy names the movement attrib differently — `SpeedRunning`/`SpeedJumping`/
  // `SpeedFlying` (and the odd `RunSpeed`/`FlySpeed`) where HC uses `RunningSpeed` etc.
  // These are the SAME axis (verified: same `Melee_Speed*` tables, self-targeted travel
  // powers). Mapping them here — not renaming at the parser — because the committed tspy
  // export already carries this spelling and its current binary is incomplete, so a
  // re-export would regress the data (see [[tspy-player-vocab-gap]]). `speedflying` is the
  // FlyingSpeed buff (axis `Fly`), NOT the kFly mode grant.
  speedrunning: 'Run', speedjumping: 'Jump', speedflying: 'Fly',
  runspeed: 'Run', flyspeed: 'Fly',
  // Thunderspy also drops the `Movement` prefix on the friction/control axes
  // (`Friction`→MovementFriction, `Control`→MovementControl) on the same
  // `*_Friction`/`*_Control` tables HC uses. Verified: tspy `Friction` sits on
  // `Melee_Friction` (== HC MovementFriction's table) and co-occurs with MovementControl
  // on 169/170 files, mirroring HC's always-paired travel-power rule — so the old TSPY-2
  // note that "tspy MovementFriction is absent" was just this respelling (the census
  // looked for `MovementFriction`, missed `Friction`). The parser already relabels most
  // front-`Control`→`MovementControl`; `control` here catches the 1 residual it misses.
  // Both axes are excluded from movement totals (movement.rs), so this only de-Unmaps them.
  friction: 'Friction', control: 'Control',
};

/** engine / meta attribs → their effectType (or 'Meta' for non-stat markers). */
const META_EFFECT: Record<string, EffectType> = {
  grant_power: 'GrantPower', create_entity: 'EntCreate',
  execute_power: 'ExecutePower', recharge_power: 'RechargePower',
  global_chance_mod: 'GlobalChanceMod',
  set_mode: 'Meta', set_token: 'Meta', add_behavior: 'Meta',
  cancel_effects: 'Meta', designer_status: 'Meta', meter: 'Meta',
  rage: 'Meta', null: 'Meta', 'jump pack': 'Meta', stealth: 'Stealth',
  // Engine / script markers surfaced by the 2026-07-07 export refresh (mode-system
  // parsing + the attrib-118 byte-granular sub-index fix, which stopped collapsing
  // several of these onto Set_Mode). None is a numeric player stat — all are
  // non-stat engine/reward/script/costume markers → 'Meta'. (`translucency` is the
  // stealth-visual component → Stealth/Translucency; see STEALTH_AXIS [BRIDGE-2].)
  revoke_power: 'Meta', cancel_mods: 'Meta', set_costume: 'Meta',
  silent_kill: 'Meta', xpdebtprotection: 'Meta', token_add: 'Meta',
  token_set: 'Meta', clear_damagers: 'Meta', view_attributes: 'Meta',
  vision_phase: 'Meta', combat_mod_shift: 'Meta', avoid: 'Meta',
  grant_boosted_power: 'Meta', set_script_value: 'Meta', reward: 'Meta',
  ninja_run: 'Meta', script_notify: 'Meta',
  // Rebirth's raw-keyed special-attrib decode (SPECIAL_ATTRIB_BY_RAW_REBIRTH,
  // WS7) surfaces the full i24 special block under its real names — all
  // non-stat engine markers. `power_redirect` in particular is the fork's
  // redirect mechanism; its mechanical meaning (target + condition) is
  // exported on the power-level `redirect` list, so the atom is a marker.
  power_redirect: 'Meta', unset_mode: 'Meta', drop_toggles: 'Meta',
  power_chance_mod: 'Meta', reward_source: 'Meta', reward_source_team: 'Meta',
  clear_fog: 'Meta', xpdebt: 'Meta', exclusive_vision_phase: 'Meta',
  token_clear: 'Meta', lua_exec: 'Meta', force_move: 'Meta',
  // travel-stance markers
  glide: 'Meta', walk: 'Meta', beast_run: 'Meta', steam_jump: 'Meta',
  hover_board: 'Meta', magic_carpet: 'Meta', parkour_run: 'Meta',
};

/**
 * Map a bin-export `attribs[]` string (with its template `aspect`/`table` for the
 * cases where those disambiguate) to a canonical `(effectType, subType)`. Returns
 * `effectType:'Unmapped'` with a reason for anything it cannot confidently classify
 * — the caller tracks that as a coverage gap rather than silently mis-slotting it.
 *
 * The one context-dependent family: bare by-type attribs (`Smashing`, `Melee`, …)
 * carry no effectType in the name — Defense vs Resistance vs Elusivity lives in
 * `aspect`+`table` (verified against the committed HC export, 2026-07-05).
 */
export function bridgeAttrib(attrib: string, aspect?: string, table?: string): BridgeResult {
  const a = (attrib || '').toLowerCase();
  const tbl = (table || '').toLowerCase();
  const asp = (aspect || '').toLowerCase();
  if (!a) return { effectType: 'Unmapped', reason: 'empty attrib' };

  // aspect is the deep discriminator (verified against the HC oracle 2026-07-05):
  //   Str ⇒ a STRENGTH buff (DamageBuff for damage, Enhancement for a secondary
  //          attribute), NOT dealing/applying the attribute.
  //   Res ⇒ resistance to the attribute (Resistance / MezResist).
  //   Abs/Cur/Max ⇒ deal / apply / cap.
  const isStr = asp === 'strength';
  const isRes = asp === 'resistance';
  const resTable = tbl.includes('res') && !tbl.includes('restore');
  const defTable = tbl.includes('def');

  // Heal_Dmg is special: aspect=Resistance ⇒ healing *received* (not -res); the
  // bare `endsWith('_Dmg')` test historically flattened it into resistanceAll.
  if (a === 'heal_dmg') {
    return isRes ? { effectType: 'HealResistance' } : { effectType: 'Heal' };
  }

  // [ATOM-TSPY step 2] Thunderspy respells HC's `HitPoints`@Maximum (+MaxHP) as bare
  // `Heal`/`HealSelf` on the same `*_HealSelf`/`*_Heal` tables (HC splits the two faces:
  // HitPoints@Maximum is the cap, Heal_Dmg is the applied heal). Only the Maximum face is a
  // consumed total, so only it bridges — to MaxHP, exactly as `hitpoints`@maximum does. The
  // converter's RESOURCE_TYPES mirrors this (heal→hitPoints ⇒ maxHPBuff), keeping atom==bag.
  // Applied-heal / heal-strength faces (Absolute/Current/Strength) stay Unmapped on BOTH
  // sides — no calc total reads them. tspy-only: HC/Rebirth carry 0 bare Heal/HealSelf.
  if ((a === 'heal' || a === 'healself') && asp === 'maximum') return { effectType: 'MaxHP' };

  // Damage / DamageBuff: `<type>_Dmg`. aspect=Str ⇒ a buff to the Damage attribute.
  if (a.endsWith('_dmg')) {
    const sub = DAMAGE_SUBTYPE[a.slice(0, -4)];
    if (!sub) return { effectType: 'Unmapped', reason: `unknown damage type: ${attrib}` };
    if (isStr) return { effectType: 'DamageBuff', subType: sub };
    if (isRes) return { effectType: 'Resistance', subType: sub };
    return { effectType: 'Damage', subType: sub };
  }

  // Elusivity: `<type>_Elusivity` / `ElusivityBase`.
  if (a === 'elusivitybase') return { effectType: 'Elusivity', subType: 'All' };
  if (a.endsWith('_elusivity')) {
    const sub = DAMAGE_SUBTYPE[a.slice(0, -'_elusivity'.length)];
    return { effectType: 'Elusivity', subType: sub ?? 'All' };
  }

  // Mez family. Res ⇒ mez RESISTANCE (duration reduction); Str ⇒ a buff to the
  // mez's strength (Power Boost); else ⇒ applying the mez.
  if (a in MEZ_SUBTYPE) {
    const sub = MEZ_SUBTYPE[a];
    if (isRes) return { effectType: 'MezResist', subType: sub };
    if (isStr) return { effectType: 'Enhancement', subType: sub };
    return { effectType: 'Mez', subType: sub };
  }

  // Stealth radius / translucency — the axis lives in the subType [BRIDGE-2]. Must precede
  // SCALAR/META (which used to catch these attrib-typed with no subType).
  if (a in STEALTH_AXIS) return { effectType: 'Stealth', subType: STEALTH_AXIS[a] };

  // Scalar stats keep their type at any aspect (Mids keeps Recovery/Regen/ToHit at
  // Str); only Endurance gains a Max variant.
  if (a in SCALAR_EFFECT) {
    if (a === 'endurance' && asp === 'maximum') return { effectType: 'MaxEndurance' };
    return { effectType: SCALAR_EFFECT[a] };
  }

  // Movement.
  if (a in MOVEMENT_AXIS) return { effectType: 'Movement', subType: MOVEMENT_AXIS[a] };

  // Meta / engine.
  if (a in META_EFFECT) return { effectType: META_EFFECT[a] };

  // Base_Defense / bare by-type dimension (Smashing/…/Melee/Ranged/Area) — effectType
  // is NOT in the name (the damage/resistance face is always written `<type>_Dmg`).
  // Res/res-table ⇒ Resistance; def-table ⇒ Defense; Cur ⇒ Defense (the bare attrib IS
  // the defense characteristic, on any table); Str ⇒ Enhancement (defense strength buff);
  // else (Abs/Max on a non-def/res table) ⇒ deferred to DSH6's holistic routing.
  const sub = a === 'base_defense' ? 'All' : DAMAGE_SUBTYPE[a];
  if (sub) {
    // [BRIDGE-1] base_defense IS the defense characteristic; its @Resistance (or
    // res-table) face is defense-DEBUFF-RESISTANCE — route to Defense, NOT all-damage
    // Resistance. ingestTemplate preserves aspect=Res from the template ⇒ the atom
    // becomes Defense/All aspect=Res (an encoding coh_data already models). 'All' is
    // excluded from both the defense-buff and resistance totals, so this reclassifies
    // typing only — no total moves. A bare positional <type> attrib's Res face genuinely
    // IS damage resistance (the NPC "Resistance" powers whose real resistance rides
    // <type>_Dmg@Res), so it stays on the Resistance rule below.
    if (a === 'base_defense' && (isRes || (resTable && !defTable)))
      return { effectType: 'Defense', subType: sub };
    if (isRes || (resTable && !defTable)) return { effectType: 'Resistance', subType: sub };
    if (defTable) return { effectType: 'Defense', subType: sub };
    // A bare position/type attrib with no `_Dmg` suffix IS the *defense* characteristic
    // (the damage/resistance face is always written `<type>_Dmg`); aspect=Current ⇒ a
    // defense buff/debuff — EVEN on a generic scaling table (`Melee_Ones`/`Ranged_Ones`,
    // the incarnate flat-buff tables), not just a `*Def*` table. Verified across the full
    // HC export (2026-07-05): every bare-by-type @ Current template is defense — Barrier /
    // Support Core incarnates and the positional NPC "Resistance" powers (whose real
    // resistance rides `_Dmg`@Res) — with zero mez/notify co-listing at aspect=Current.
    // Previously these fell to Unmapped, blinding DSH5/DSH6 to defense on non-`*Def*` tables.
    if (asp === 'current') return { effectType: 'Defense', subType: sub };
    if (isStr) return { effectType: 'Enhancement', subType: sub };
    return { effectType: 'Unmapped', reason: `by-type '${attrib}' at aspect '${aspect ?? '?'}' on non-def/res table '${table}' (deferred — DSH6 routing)` };
  }

  // kSpecial: the game's per-power special-behavior attribute (the exporter
  // prints it with its index, e.g. `Special(485)` on HC). Entered the corpus
  // with the Boosts.*/Set_Bonus.* piece templates — Cupid's Crush "chance
  // to..." segments carry it. Not a quantity the calc models; recognized here
  // so the no-bridge-rule tripwire stays sharp for genuinely new attrib names.
  if (a.startsWith('special(')) return { effectType: 'Unmapped', reason: `kSpecial special-behavior attrib (not a modeled quantity): ${attrib}` };
  if (a.startsWith('unknown(')) return { effectType: 'Unmapped', reason: `parser-unmapped attrib index: ${attrib}` };
  return { effectType: 'Unmapped', reason: `no bridge rule: ${attrib}` };
}

// ============================================================================
// Reference ingest: bin-export effect template → AtomicEffect[]
// ============================================================================

/** minimal structural shape of a committed `exported_powers/**.json` effect group. */
export interface ExportGroup {
  is_pvp?: string;
  chance?: number;
  ppm?: number;
  /** EffectGroup `Delay` in seconds — the ancestor half of {@link AtomicEffect.delay}. */
  delay?: number;
  requires_expression?: string[];
  tags?: string[];
  templates?: ExportTemplate[];
}
export interface ExportTemplate {
  attribs?: string[];
  type?: string;
  aspect?: string;
  target?: string;
  table?: string;
  scale?: number;
  magnitude?: number;
  magnitude_expression?: string;
  duration?: string | number;
  /** AttribMod `Delay` in seconds — see {@link AtomicEffect.delay}. */
  delay?: number;
  application_period?: number;
  /** When the game applies the row — see {@link AtomicEffect.applicationType}. */
  application_type?: string;
  tick_chance?: number;
  stack?: string;
  stack_key?: string;
  stack_limit?: number;
  flags?: string[];
  /** AttribMod-tail event gate records; only the event names reach the atom. */
  required_events?: Array<{ event: string; event_id?: number; duration?: number; always?: number }>;
  /** AttribMod-tail suppression records: the event, its window in seconds, and the
   *  `always` flag. Folded to the `suppressible` verdict for every atom; carried
   *  whole (events + window) on Meta atoms only. */
  suppress_events?: Array<{ event: string; event_id?: number; duration?: number; always?: number }>;
  /** AttribMod-tail cancel-event names. Carried on Meta atoms only. */
  cancel_events?: string[];
}

const PV_MAP: Record<string, PvMode> = { EITHER: 'Any', PVE_ONLY: 'PvE', PVP_ONLY: 'PvP' };
const ASPECT_MAP: Record<string, Aspect> = {
  Absolute: 'Abs', Current: 'Cur', Resistance: 'Res', Strength: 'Str', Maximum: 'Max',
};

export function mapAttribType(t?: string): AttribType {
  const known: Record<string, AttribType> = {
    Duration: 'Duration',
    Magnitude: 'Magnitude',
    Constant: 'Constant',
    Expression: 'Expression',
  };
  // Absence is the parse table's own default, not a guess: the AttribMod table declares
  // the field `TOK_INT(AttribModTemplate, eType, kModType_Magnitude)`
  // (`Common/entity/attribmod.h:678`), so a def with no `Type` line compiles to
  // Magnitude. ATTRTYPE-1 could only measure this (Detention Field's mez row reads
  // Magnitude); the parse table states it.
  if (!t) return 'Magnitude';
  const mapped = known[t];
  if (!mapped) throw new Error(`unrecognized attrib type ${JSON.stringify(t)}`);
  return mapped;
}
/**
 * Template `target` → {@link AtomicEffect.toWho}, one member of the game's `ModTarget`
 * per `Common/entity/attribmod.h`.
 *
 * An unrecognized value THROWS, on `mapStacking`'s reasoning and for the same reason it
 * was needed there. This used to be four substring tests falling through to
 * `'Unspecified'`, and both halves of that did damage. The `includes('Pets')` arm caught
 * `SelfAndPets` and `AnyAffectedAndPets` alike and answered `'All'` for both, which is
 * the fold TARGETS-2 measured. And the fall-through turned the one `TargetOnly` template
 * in the corpus into "the source stated nothing" — the exact absent-vs-defaulted
 * confusion {@link Aspect} documents, run backwards.
 *
 * Absent stays `'Unspecified'`: a template that states no target is not a template that
 * states `Self`. (The parser defaults the field to `kModTarget_Affected` when the
 * authored def omits it — `powers_load.c:2377` — so absence here means the exporter
 * emitted no field at all, which is rarer still: one atom corpus-wide.)
 */
function mapToWho(target?: string): ToWho {
  const known: Record<string, ToWho> = {
    Self: 'Self',
    SelfAndPets: 'SelfAndPets',
    TargetOnly: 'TargetOnly',
    TargetOnlyAndPets: 'TargetOnlyAndPets',
    AnyAffected: 'Target',
    AnyAffectedAndPets: 'TargetAndPets',
    Marker: 'Marker',
  };
  if (!target) return 'Unspecified';
  const mapped = known[target];
  if (!mapped) throw new Error(`unrecognized template target ${JSON.stringify(target)}`);
  return mapped;
}

/**
 * Does this atom land on the CASTER — the character whose totals we are computing?
 *
 * The one place that question is answered, because it used to be answered fourteen
 * times: some appliers tested `toWho === 'Self'`, others `'Self' || 'All'`, and the two
 * readings disagreed on every `SelfAndPets` atom in the corpus (TARGETS-2).
 *
 * - `Self` and `SelfAndPets` anchor on the caster by construction — the engine starts
 *   the walk at `pSrc` and never consults the power's targets, so neither needs context.
 * - `TargetAndPets` anchors on whoever the power hit, then walks UP to that entity's
 *   top-level owner and fans out over the owner's pets. The caster is in that set
 *   whenever the caster and the hit entity share an owner, which is every player power
 *   in the corpus that carries it: Serum and Smoke Flash hit `MyPet`, Force Shield hits
 *   `MyOwner`, and the Incarnate sockets hit `Self`. `scripts/planb-shadow-towho.cjs`
 *   pinned that premise and was deleted on 2026-09-25 with the `src/` tree it read, so
 *   nothing measures it now: a foe-facing one arriving in a future export would pass
 *   quietly, and the premise rests on this comment rather than on a check.
 * - `Target` and `TargetOnly` are the open half. They land on the caster exactly when
 *   the power's own `targetsAffected` names `Self` (Maneuvers buffs you; Wormhole's
 *   teleport resistance is the victim's), and no atom carries that — it is a power-level
 *   field. The bag route already reads it as `selfIsCountedTarget`; the atom route does
 *   not, and every family whose atoms are `Target` still answers from the bag because of
 *   it. Deliberately unchanged here: 2,082 / 1,948 / 2,208 base atoms would move at once.
 *   See DATA-GAP-REGISTER TARGETS-3.
 */
export function landsOnCaster(a: Pick<AtomicEffect, 'toWho'>): boolean {
  switch (a.toWho) {
    case 'Self':
    case 'SelfAndPets':
    case 'TargetAndPets':
      return true;
    case 'Target':
    case 'TargetOnly':
    case 'TargetOnlyAndPets':
    case 'Marker':
    case 'Unspecified':
      return false;
  }
}
/**
 * `stack_key` unresolved-registry sentinel (0xFFFFFFFF). The exporter emits it
 * verbatim where a template carries no key; it is "absent", not a group name.
 * It never co-occurs with `stack: 'Suppress'` in the corpus, so no consumer has
 * been misled by it — but it must not reach the wire as a groupable key.
 */
const STACK_KEY_NONE = '4294967295';

function mapStackKey(k?: string): string | undefined {
  return !k || k === STACK_KEY_NONE ? undefined : k;
}

function mapRequiredEvents(evs?: ExportTemplate['required_events']): string | undefined {
  if (!evs || evs.length === 0) return undefined;
  return evs.map((e) => e.event).join(',');
}

/** Every `application_type` the three exports state. Closed on purpose — an
 *  unlisted value is a parse surprise, and the STACK-3 rule says a converter that
 *  folds one into a plausible default ships the surprise as data. */
const APPLICATION_TYPES = new Set([
  'OnTick',
  'OnActivate',
  'OnEnable',
  'OnDisable',
  'OnDeactivate',
  'OnExpire',
]);

/**
 * Template `application_type` → the atom's field, omitting the standing case.
 *
 * `OnTick` encodes as absent (94% of atoms, and the wire comment on
 * `ATOM_TUPLE_FIELDS` owns the economics). That is only sound while the export
 * states the field on every template, so a missing one THROWS instead of being
 * read as standing — the whole point of omitting a value is that its absence has
 * one cause, and a silent export would give it two.
 */
function mapApplicationType(a?: string): string | undefined {
  if (!a) throw new Error('[atoms] template states no application_type');
  if (!APPLICATION_TYPES.has(a)) {
    throw new Error(`[atoms] unrecognized application_type '${a}'`);
  }
  return a === 'OnTick' ? undefined : a;
}

/**
 * Template `suppress_events` → the atom's whole-tail fields (Meta atoms only; the
 * caller owns the scope). The export states duration and `always` per event and the
 * atom carries one of each per template, so a template whose events disagree, or an
 * event missing either field, THROWS rather than collapsing or defaulting (the
 * STACK-3 rule: a converter default turns a data surprise into plausible data).
 * Every emitted template today is uniform; the known non-uniform ones are non-Meta.
 */
function mapSuppressWindow(
  evs?: ExportTemplate['suppress_events']
): { events: readonly string[]; seconds: number; always: boolean } | undefined {
  if (!evs || evs.length === 0) return undefined;
  const seconds = new Set<number>();
  const always = new Set<number>();
  for (const e of evs) {
    if (e.duration === undefined || e.always === undefined) {
      throw new Error(
        `[atoms] suppress_events entry '${e.event}' lacks duration or always; ` +
          `refusing to default a field the wire carries`
      );
    }
    seconds.add(e.duration);
    always.add(e.always);
  }
  if (seconds.size > 1 || always.size > 1) {
    throw new Error(
      `[atoms] suppress_events disagree within one template (durations [${[...seconds]}], ` +
        `always [${[...always]}]); the wire carries one window per atom ` +
        `(suppressSeconds/suppressAlways), so widen it to per-event pairs before emitting this`
    );
  }
  return {
    events: evs.map((e) => e.event),
    seconds: [...seconds][0],
    always: [...always][0] !== 0,
  };
}

/**
 * Template `stack` → {@link AtomicEffect.stacking}, one member of the game's
 * `StackTypeEnum` per `Common/entity/attribmod.h`.
 *
 * An unrecognized value THROWS. It used to fall back to `'No'`, and that default
 * did two kinds of damage at once (STACK-3). It mislabelled: `StackThenIgnore`
 * and `Continuous` were absent from the table below though the parser decodes
 * both, so 744 templates across the three forks landed on a sentinel meaning the
 * OPPOSITE of what they say. And it laundered: Thunderspy's stack field was being
 * read from a shifted offset on 63 sub-records, and the string offsets that came
 * out — `Unknown(784601)` and friends, values an eleven-member enum cannot
 * produce — arrived here and were quietly turned into a plausible answer, so no
 * census downstream of the converter could see the parse defect. Absent is still
 * `'No'`; only an unrecognized *present* value is a fault.
 */
function mapStacking(s?: string): Stacking {
  const known: Record<string, Stacking> = {
    Stack: 'Stack', Replace: 'Replace', Extend: 'Extend', Refresh: 'Refresh',
    RefreshToCount: 'RefreshToCount', Overlap: 'Overlap', Maximize: 'Maximize',
    Ignore: 'Ignore', Suppress: 'Suppress', Yes: 'Yes', No: 'No',
    StackThenIgnore: 'StackThenIgnore', Continuous: 'Continuous',
  };
  if (!s) return 'No';
  const mapped = known[s];
  if (!mapped) throw new Error(`unrecognized stack type ${JSON.stringify(s)}`);
  return mapped;
}
/**
 * Template `tick_chance` → the atom's {@link AtomicEffect.tickChance}. A chance of 1
 * (or an absent field) is every application landing, which is the default reading, so
 * it is left off the wire.
 *
 * Rounded to two decimals because the field is a float32 and reaches the export as
 * noise (`0.800000011920929` for an authored 0.8). The damage bag has always read it
 * this way; rounding here keeps the atom and the bag reporting the same DoT rather
 * than two numbers that differ in the eighth decimal.
 */
export function mapTickChance(chance?: number): number | undefined {
  if (typeof chance !== 'number' || chance >= 1) return undefined;
  return Math.round(chance * 100) / 100;
}

export function parseDuration(d?: string | number): number {
  if (typeof d === 'number') return d;
  if (!d) return 0;
  const m = /-?\d+(\.\d+)?/.exec(d);
  return m ? parseFloat(m[0]) : 0;
}

/**
 * Group-level context an `ExportTemplate` inherits from its enclosing effect
 * group (or, in the converter's flattened-template world, from the `_group*`
 * tags the collectors stamp onto each template). `mapPvMode` converts the raw
 * export `is_pvp` string.
 */
export interface IngestContext {
  pvMode: PvMode;
  baseProbability: number;
  procsPerMinute?: number;
  requiresExpression?: string[];
  /** e.g. 'OutOfCombat' for combat-gated (suppressible) templates. */
  specialCase?: string;
  /** The group's `Tag` list, comma-joined — see {@link AtomicEffect.tags}. */
  tags?: string;
  /**
   * Seconds of `Delay` the enclosing group chain states, summed outermost-in — the
   * ancestor half of {@link AtomicEffect.delay}, which adds the template's own. Only
   * ~100 groups corpus-wide carry one (Placate's 0.05, Levitate's nested 0.1s), and no
   * self-directed debuff does, so this half is carried for completeness rather than for
   * a case that turns on it.
   */
  delay?: number;
}

/** Effect-group `Tag` array → the atom's {@link AtomicEffect.tags}. */
export function mapGroupTags(tags?: readonly string[]): string | undefined {
  return tags && tags.length > 0 ? tags.join(',') : undefined;
}

/** Raw export `is_pvp` string → PvMode ('EITHER'/undefined ⇒ 'Any'). */
export function mapPvMode(isPvp?: string): PvMode {
  return PV_MAP[isPvp || 'EITHER'] ?? 'Any';
}

/**
 * Ingest ONE export template into AtomicEffect records — one per mapped attrib.
 * Every attrib produces a record; an unbridged attrib yields an
 * `effectType:'Unmapped'` record (never dropped) so the caller can measure
 * coverage. Shared by the reference encoder below AND the converter's
 * production ingest (DSH6 — `templatesToAtoms` in convert-powerset.cjs), so
 * both sides encode a template identically by construction.
 */
export function ingestTemplate(t: ExportTemplate, ctx: IngestContext): AtomicEffect[] {
  const resistible = !(t.flags ?? []).includes('IgnoreResistance');
  const ignoreStrength = (t.flags ?? []).includes('IgnoreStrength');
  const tickChance = mapTickChance(t.tick_chance);
  const cancelOnMiss =
    tickChance !== undefined && (t.flags ?? []).includes('CancelOnMiss') ? true : undefined;
  // NOT `?? 'Cur'` — an unstated aspect stays `Unspecified`. See the Aspect type.
  const aspect = ASPECT_MAP[t.aspect ?? ''] ?? 'Unspecified';
  const attribType = mapAttribType(t.type);
  const toWho = mapToWho(t.target);
  const stacking = mapStacking(t.stack);
  // Meta-scoped, so computed lazily: the guard inside THROWS on a non-uniform
  // window, and a template no Meta atom carries must not fail the convert.
  let suppressWindow: ReturnType<typeof mapSuppressWindow> | null = null;
  const metaSuppress = () => {
    suppressWindow ??= mapSuppressWindow(t.suppress_events);
    return suppressWindow;
  };
  const out: AtomicEffect[] = [];
  for (const attrib of t.attribs ?? []) {
    const bridged = bridgeAttrib(attrib, t.aspect, t.table);
    out.push({
      effectType: bridged.effectType,
      subType: bridged.subType,
      pvMode: ctx.pvMode,
      resistible,
      toWho,
      attribType,
      aspect,
      modifierTable: t.table ?? '',
      scale: t.scale ?? 0,
      magnitude: t.magnitude ?? 0,
      // A value expression (kRage-driven Fury etc.); empty string ⇒ absent, like the
      // other optional strings. Read by the calc's expr VM, not by the bag.
      magnitudeExpression: t.magnitude_expression?.length ? t.magnitude_expression : undefined,
      duration: parseDuration(t.duration),
      // The mod's own Delay plus the group chain's. Zero ⇒ absent: "starts with the
      // cast" is the unstated case, and encoding a 0 would spend a slot on every atom.
      delay: (t.delay ?? 0) + (ctx.delay ?? 0) || undefined,
      applicationPeriod: t.application_period || undefined,
      stacking,
      stackCap: t.stack_limit && t.stack_limit > 0 ? t.stack_limit : undefined,
      stackKey: mapStackKey(t.stack_key),
      requiredEvents: mapRequiredEvents(t.required_events),
      tickChance,
      cancelOnMiss,
      baseProbability: ctx.baseProbability,
      procsPerMinute: ctx.procsPerMinute,
      ignoreStrength: ignoreStrength || undefined,
      specialCase: ctx.specialCase,
      requiresExpression: ctx.requiresExpression,
      tags: ctx.tags,
      // Only `Meta` carries it — every other effectType names its own attrib, and
      // `Meta` is the ~44-marker bucket where the name is otherwise lost. Lowercased
      // so the value IS the `META_EFFECT` key that selected `Meta`.
      metaAttrib: bridged.effectType === 'Meta' ? attrib.toLowerCase() : undefined,
      // The event tails, whole, on the same Meta scope as `metaAttrib` (the
      // schema comments there and on the fields own the reasoning).
      suppressEvents: bridged.effectType === 'Meta' ? metaSuppress()?.events : undefined,
      suppressSeconds: bridged.effectType === 'Meta' ? metaSuppress()?.seconds : undefined,
      suppressAlways: bridged.effectType === 'Meta' ? metaSuppress()?.always : undefined,
      cancelEvents:
        bridged.effectType === 'Meta' && t.cancel_events?.length ? t.cancel_events : undefined,
      applicationType: mapApplicationType(t.application_type),
      // Refresh-by-(attrib, key) semantics, and beside `Stack`/`Continuous` the mark of a
      // per-target increment the resource router must not also route (STACK-5).
      stackByAttribAndKey: (t.flags ?? []).includes('StackByAttribAndKey') || undefined,
      sourceAttrib: attrib,
    });
  }
  return out;
}

/**
 * Ingest one export effect group's templates into AtomicEffect records — one per
 * (template × mapped attrib). This is the *reference* encoder that validates the
 * schema/key against real data; the converter's production ingest shares
 * `ingestTemplate` above.
 */
export function ingestExportGroup(group: ExportGroup): AtomicEffect[] {
  const ctx: IngestContext = {
    pvMode: mapPvMode(group.is_pvp),
    baseProbability: group.chance ?? 1,
    procsPerMinute: group.ppm && group.ppm > 0 ? group.ppm : undefined,
    requiresExpression: group.requires_expression?.length ? group.requires_expression : undefined,
    tags: mapGroupTags(group.tags),
    delay: group.delay || undefined,
  };
  return (group.templates ?? []).flatMap((t) => ingestTemplate(t, ctx));
}

/** Ingest a whole power JSON (`{ effects: ExportGroup[] }`). */
export function ingestExportPower(power: { effects?: ExportGroup[] }): AtomicEffect[] {
  return (power.effects ?? []).flatMap(ingestExportGroup);
}
