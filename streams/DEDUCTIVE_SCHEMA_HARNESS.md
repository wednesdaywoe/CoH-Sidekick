---
project: coh-sidekick
kind: plan
title: Deductive Effect Schema + Differential Harness
id-prefix: DSH
relates:
  - HOMECOMING_PARSER.md
  - THUNDERSPY_PARSER.md
  - REBIRTH_PARSER.md
  - GAME-DATA-PRINCIPLES.md
---

# Deductive Effect Schema + Differential Harness

> **Canonical (`coh-sidekick-1.0`) is authoritative for this log**, at
> `docs/streams/DEDUCTIVE_SCHEMA_HARNESS.md`. CoH-Sidekick (beta) carries a synced copy at
> `streams/DEDUCTIVE_SCHEMA_HARNESS.md`, cited from 11 files there (`tools/mids-oracle/read_i12.py`,
> `diff_harness.py`, `dsh8-incarnate-collapse.test.ts`, the mids-oracle README). One-way flow: edit
> it in canonical, then run beta's `scripts/sync-bin-crawler.sh`. Mirrored byte-for-byte, which is
> why this header names the repos instead of saying "this one".
>
> Paths below are written **relative to the repo root**, so they read as paths rather than
> resolving as markdown links from the directory this file sits in. Leave them that way — the two
> copies sit at different depths, so any `../` prefix correct in one repo is wrong in the other.
> 59 of the 68 link targets exist in both repos from the root.
>
> Canonical's copy is the newer of the two. Beta's had been deleted from disk outright and was
> recovered from its own history at 663 lines, 28 short — still carrying the pre-2026-07-08 wording
> that called the `PowerEffects`-becomes-a-list rewrite deferred, after the Phase 1 flip had landed.
> That is the divergence this authority rule exists to prevent.

Source of truth for the effort to stop rediscovering effect-schema bugs
bin-by-bin. The recurring "collapse" bug family (PvP-clobber, resistable/
unresistable pair-collapse, multi-damage-type merge, duration-variant loss,
`_dam`/`_dmg` table miss) is not a list of independent bugs — it is one
architectural mismatch. This doc plans (1) a **deductive closed effect schema**
derived once from an authoritative source (Mids Reborn) instead of induced
bin-by-bin, and (2) a **differential harness** that diffs every power's effect
*structure* against a Mids-derived oracle so the whole class is caught by a
one-time sweep + CI, not by noticing a wrong number.

Scope is the powerset conversion pipeline
([`convert-powerset.cjs`](scripts/convert-powerset.cjs) → generated TS) and a new
validation/oracle layer beside it. It does **not** cover the Python bin parser's
correctness for this class (see "Where the data is lost" — the parser is already
proven correct here). **DSH8/DSH9 (backlog) extend the same diagnosis to the two
sibling converters — [`convert-incarnate-effects.cjs`](scripts/convert-incarnate-effects.cjs)
and the enhancement extractors — which independently reinvented the same
bag-of-slots model** (audit 2026-07-05, user-confirmed the expansion).

> Grounded in a 2026-07-05 investigation (6-agent audit: Mids effect model, Mids
> DB feasibility, converter collapse map, parser information-loss proof, design,
> oracle red-team). Full findings archived in the session scratchpad; the durable
> conclusions are captured below and in agent memory
> (`converter-bag-vs-array-rootcause`, `mids-oracle-stale-structural`).

## Root cause (the reframe)

The game runtime, our Python parser's export JSON, **and** Mids all model a power
as a **flat array of atomic, single-attrib effect records** — a compound effect =
N sibling records distinguished by `damageType` / `PvMode` / `Resistible` /
`mezType`. [`convert-powerset.cjs`](scripts/convert-powerset.cjs) `extractEffects()`
(~L3369) does the opposite: it models a power as `PowerEffects`, a **bag of ~90
named single-value slots**, routing each game effect into one named key with
**last-write-wins**. The audit enumerated **10 collapse sites (A–J)** — scalar
overwrite, by-type map overwrite, mez same-type higher-mag tiebreaker (~L3807),
`addOrAccumulate` sum-collapse (~L3980), `recordDuration` single-duration-per-key,
EntCreate summon merge, `resolveSummonRedirects` one-per-type pseudo-pet dedup —
and they are all the same shape. Every shipped fix (`unresistable`,
`durationVariants`, `domination`, `selfPenalty`) is a narrowly-gated patch onto
single-slot keys, which is exactly why new collapse sites keep surfacing in the
same shape. **The fix direction that dissolves the family:** give the converter an
internal *list of effect instances* keyed by a canonical identity, and make
`PowerEffects` a **projection computed at the end**, not the working model.

## Where the data is lost (converter-only)

The Python parser already preserves all three compound dimensions in the export
JSON — verified with line cites:
[`is_pvp`](tools/bin-crawler/bin_crawler/parser/_powers.py) per group
(EITHER/PVE_ONLY/PVP_ONLY), template `flags`/`flags_raw` carrying
`IgnoreResistance` (the resistible bit), and the full multi-`attribs` list.
`export_powers.py` dumps every group and every template 1:1 with no merge/select.
**Therefore this is a converter-only fix; the parser is not touched to recover
this data.** (The CLAUDE.md note that the template tail "is NOT yet parsed" is
**stale** — the tail is fully decoded now. That correction should land in
[HOMECOMING_PARSER.md](HOMECOMING_PARSER.md)/CLAUDE.md separately.)

## Ground truth & trust boundary (Mids = structural oracle only)

Mids ships a committed HC DB (`MidsReborn-master/MidsReborn/Databases/Homecoming/I12.mhd`,
build `2026.1.1242`, **Issue 28, dated 2026-05-28** — ~5 weeks old, not multi-year
stale). It stores effects at **template granularity** (scale + `ModifierTable`
name + attribType/aspect) — the *same abstraction level as our bin parser* — so it
is a clean parser-to-parser structural diff, no Mids calc engine required.

Trust boundary (decision 2026-07-05, user-confirmed): **Mids for topology, bins
for names and numbers.** Mids is old and has changed hands, so its numbers drift
(~5-week rebalance) and its identifier strings carry typos. So:
- **TRUST (structural, version- & handoff-stable):** sub-effect count/identity,
  `PvMode` presence, `Resistible` flag, `ModifierTable` *selection*, `EffectType`/
  `Aspect`/`AttribType` classification, conditional-gate identity.
- **DISTRUST:** exact `Scale`/`nMagnitude`/`nDuration` (skew), any computed
  `Mag`/`MagPercent` (Mids display convention), `Compare.json` (cosmetic), Mids
  zeros (may be silent table-miss). When Sidekick and Mids disagree, the raw
  `.pigg` bin is the tiebreaker, not Mids.
  - **Two distinct numeric-distrust modes — do not conflate (2026-07-06):**
    **(a) rebalance staleness** — a value genuinely changed in a patch Mids hasn't
    absorbed (~1 mid-cycle behind: Mids build 2026-05-28 vs the live HC patch); a
    re-export closes it. **(b) format quantization** — a *permanent* precision floor:
    Mids stores set-bonus `Scale` to **3 decimals**, so a true `0.02525` is saved as
    `0.025` and the same stored scale backs both a repo `2.5` and a repo `2.525` (the
    `.025` damage residual). No re-export ever fixes (b); it is structural to the
    `.mhd`. Both are advisory, but only (a) is a candidate for a bin re-pull — (b) means
    "trust the bin value and stop looking." This is why a *full numeric sweep* against
    Mids is noise (see DSH7-descoped), not signal.

Two non-negotiable guardrails for the harness (else it is noise): **(1)** never
raw-shape diff — canonicalize *both* sides to a multiset of
`(effectType, subType, pvMode, resistible, modifierTable)` tuples first; **(2)**
tier findings — structural divergence = defect (fail CI), numeric divergence on a
structurally-identical effect = skew advisory (never fails). Filter the oracle to
**HC + PvE/Any**; for Thunderspy/Rebirth/Veracity/incarnate-pet content Mids has
no answer, so **SKIP with explicit coverage tracking**, never flag.

## Active

Open self-check guarantees. The shipped gates + oracle PoC (**DSH1–DSH3**) are
archived; what remains here are the SC-N routing/accounting invariants not yet built.

- [x] **DSH1** (oracle-reader PoC) · **DSH2** (AT-table integrity gate) · **DSH3**
  (resistible-twin collapse gate) — **SHIPPED 2026-07-05**, full narrative in
  [the archive](DEDUCTIVE_SCHEMA_HARNESS_ARCHIVE.md).
- [x] **SC-3/SC-4/SC-5 self-check invariants — SHIPPED 2026-07-06.** Implemented
  in [`validate-converter-output.cjs`](scripts/validate-converter-output.cjs)
  (CI-gated through `npm run validate:converter`) and pinned by
  [`sc345-converter-self-check.test.ts`](src/utils/calculations/sc345-converter-self-check.test.ts):
  SC-3 checks explicit `resistible` disposition on all emitted damage/debuff atoms,
  SC-4 enforces PvP-sibling accounting (dropped-by-design + logged, no unaccounted
  sibling), and SC-5 guards sign/target routing so foe debuffs cannot surface as
  caster self-penalties. Current gate state: **0/0/0** on HC + Rebirth + tspy.

## Backlog

- [x] **DSH4** (closed atomic-effect schema + attrib→type bridge) · **DSH5**
  (oracle-backed differential harness — UNCLASSIFIED **6,107 at ship**, gate-on-new, see
  the DSH7-descoped note for why the pile is not a burn-down worklist; the live count moves
  with the register, not with this line) —
  **SHIPPED 2026-07-05**, full narrative in
  [the archive](DEDUCTIVE_SCHEMA_HARNESS_ARCHIVE.md).
- [~] **DSH6** — Converter repair: rework `extractEffects()` to build the DSH4
  internal effect list first (one record per template × attrib × pvMode ×
  resistibility, sign preserved), then project to `PowerEffects` at the end. The
  merge rules operate on matching identity keys only, dissolving collapse sites
  A–J at the source. Retire the bolt-ons (`unresistable`, `durationVariants`,
  `domination`, `selfPenalty`) **one harness-gated site at a time** — each site's
  diffs must go to zero before the next. Fold DSH2 table validation into the
  converter so it fails loudly at emit time.
  needs: DEDUCTIVE_SCHEMA_HARNESS#DSH4, DEDUCTIVE_SCHEMA_HARNESS#DSH5
  - [x] **DSH6a** — general per-slot collapse detector (found clean by-type collapse
    essentially does not occur in HC → helped disprove the DSH6 rewrite). **SHIPPED
    2026-07-05**, full narrative in [the archive](DEDUCTIVE_SCHEMA_HARNESS_ARCHIVE.md).
  - [~] **DSH6b** — Scalar-identity gate + converter fixes. **Ally/foe-targeted buff
    *display* is IN SCOPE** (decision 2026-07-05, user-chosen) — so the 69 class-absent
    + target-directed by-type flags (Speed Boost / Inertial Reduction +movement,
    Enforced Morale ally mez-resist) are real gaps to fix, not by-design drops.
    - [x] **Scalar-identity gate** — the site-A *table*-variant collapse the by-type
      gate folds out, added to the same detector: two same-`(effectType,sign)` SCALAR
      templates on DIFFERENT tables where last-write-wins keeps one. Two FP-fixes
      (verify-don't-assume): fold `resistible` out (buffs have no twin; DSH3 gates
      debuff twins) cut 815→86, then mirror the converter's PvP `enttype target>
      player eq` drop cut 86→4. **The EndDrain "collapse" (Lightning Field) was a FP**
      — Mids confirmed `*_EndDrain` is the PvP (`player eq`) variant the converter
      *correctly* drops; my `addOrAccumulate` hypothesis was wrong. **Gate now at 0.**
      verify: file:scripts/dsh6-collapse-detector.cjs, tests:src>=801
    - [x] **Conditional-pipeline PvP-variant leak — FIXED 2026-07-05.** The real bug
      the scalar gate surfaced: `collectConditionalsGrouped` (and the redirect/special
      collectors) only dropped `is_pvp==='PVP_ONLY'`, NOT the `enttype target> player
      eq` PvP twin the base `collectTemplatesDeep` drops — so a conditional PvE/PvP
      pair kept the PvP value (Beam Rifle Disintegrate showed PvP -3/Ranged_Res_Boolean
      regen over PvE -0.75/Ranged_Ones; Mids-confirmed). Fix = single shared
      `isPvpEnttypeVariant()` predicate applied at all collectors
      ([convert-powerset.cjs](scripts/convert-powerset.cjs), the two inline copies
      de-duped). Regen: 65 HC files (94 PvP damage/mez entries dropped, 9 PvP→PvE
      debuff-table flips, no PvE content lost); rebirth/thunderspy unaffected. Lint +
      DSH3 gate + 801 tests + scalar gate all green.
      verify: fn:isPvpEnttypeVariant, tests:src>=801
    - [x] Emit ally/team-targeted **movement** buffs for Info-panel display (the
      in-scope decision). Converter movement branch now routes non-Self,
      aspect=Current, positive (non-slow) movement mods to `effects.movement`
      (previously dropped): Speed Boost / Accelerate Metabolism +run/fly, Inertial
      Reduction +jump, Group Fly / Group Energy Flight team fly, Toroidal Bubble
      +jump. The calc's `ALLY_ONLY_TARGET_TYPES` gate ([character-totals.ts:956](src/utils/calculations/character-totals.ts#L956))
      keeps ally-only powers off the caster's totals — same as this power's existing
      `rechargeBuff`/`recoveryBuff` — while Self-target team powers correctly buff the
      caster (Inertial Reduction now grants Kinetics its +jump). Regen: 37 HC+Rebirth
      files; leak audit clean (no active foe-toggle gains +speed); tspy unaffected.
      **Enforced Morale mez-resist was NOT a real gap** — its PvE mez PROTECTION
      (all 6) + sleep-resist already render; the confuse/fear/hold/immob/stun
      *resistance* is `isPVPMap?` PvP-map-only (correctly dropped). DSH6 detector
      by-type 33→2 (isPVPMap? filter added, mirroring the `player eq` PvP drop);
      residual 2 = NPC display-name aggregation, not player collapse.
      verify: file:src/data/datasets/homecoming/generated/powersets/defender/primary/kinetics/speed-boost.ts, tests:src>=801
    - [x] **CONDTAG** — Surface target-tag conditional effects as `vs <type>`
      Mechanic Adjusters. `_classifyConditionalGate` now matches an allowlisted
      `<Tag> target.HasTag?` (on the stripped, anchored expression) → a per-power
      (target-side) conditional; the base collector still drops them (they're gated)
      but `extractConditionalEffects` re-surfaces them with their damage + effects.
      **ESD Arrow** (`EMP_Arrow`) now shows its "vs Machines/Robots" bonus — +1.64
      Energy damage + Mag-2 Hold — matching the in-game description; base Stun +
      End-drain unchanged. Allowlist is a CLOSED semantic set (`SURFACEABLE_TARGET_TAGS`
      = `{Electronic: 'Machines/Robots'}`): the dominant `.HasTag?` gates — `Raid`
      (4,185×) / `IncarnateBoss` (367×) — are internal engine mechanics (chained with
      `@ToHitRoll`/`kRage`), never a player bonus, and stay untoggleable. Blast radius
      is exactly the EMP/electric family (ESD Arrow, EM Pulse, Short Circuit, EMP
      Arrow, EM Wave — HC+Rebirth+tspy); DSH6 by-type 2→1 (ESD Arrow Mez|hold flag
      cleared). Undead/Demon/Ghost/Human/Generator are candidate allowlist additions
      pending per-power verification. Lint + DSH3 + 801 tests green.
      verify: fn:_classifyConditionalGate, tests:src>=801
    - [x] **DOMINATION bolt-on retired** — the clearest *dual-representation*: HC's
      `Tag "Domination"` mez bonus was captured into a special-case `MezEffect.domination`
      sub-field (81 HC powers), while Rebirth/tspy encode the *same* Dominator-inherent
      bonus via the general `domination` conditionalEffect (their `kStealth source>` gate;
      134 powers). Empty intersection ⇒ two encodings of one mechanic. Converged HC onto
      the general pipeline: `collectConditionalsGrouped` now recognizes `Tag "Domination"`
      groups → routes them to the shared `{id:'domination', label:'Domination Active',
      side:'source'}` gate (base collector skips them like Containment; `extractEffects`
      no longer diverts to the sub-field). HC Char/Dominate now emit byte-identical shape
      to Rebirth (base `hold` + `domination` conditional). **UI kept rich for all 3 servers**
      (decision 2026-07-05, user chose "Converge, keep rich UI" over minimal-retire / reverse /
      defer): the `dominationActive` Header toggle already drives the `domination` conditional
      (via `AT_INHERENT_CONDITIONAL_IDS` + `selectActiveConditionals` `atInherentState`); the
      badge (`getPowerDominationSummary`) re-sourced from the conditional so it renders for
      Rebirth/tspy too (was HC-only); the inline "+mag, longer duration" mez boost re-sourced
      from the `domination` `extraInstances` collision (tagged with `conditionalId`) and its
      duplicate "+…(from…)" row suppressed on mez rows only (non-mez collisions e.g. Shadow
      Field's summon keep their row). `MezEffect.domination` type deleted; converter
      `_tagDomination`/`pendingDomination`/`_domination` all removed. Regen: 83 HC files
      (Rebirth/tspy 0 — their `kStealth` path untouched); DSH6 detector-neutral (by-type 1
      = Focused Fighting residual, scalar 0). Lint + DSH3 (3 ds) + 801 tests green.
      verify: fn:getPowerDominationSummary, file:src/data/domination-per-effect.test.ts, tests:src>=801
    - [x] **Retire `selfPenalty` → per-effect `toWho`.** **Shipped 2026-07-05.** The
      bag-level `selfPenalty` boolean is gone; each self-directed debuff value now carries
      the DSH4 `eToWho` projection `toWho:'Self'` ([`ScaledEffect.toWho`](src/types/power.ts)),
      read via `isSelfDirectedEffect` / `hasSelfDirectedPenalty`. **Verified an observable
      bug, not just a rename (verify-don't-assume):** a scan of all 36,129 export files
      (`route()` mimicking the converter's 6 selfPenalty branches) found 8 powers that set the
      old bag flag AND routed a NON-Self (`AnyAffected`) template into a calc-consumed slot —
      the calc's bag-wide `selfPenalty && effects.slow` gate then dragged the foe entry onto
      the caster, **in direct violation of the converter's own `L3949` "foe slows don't slow
      the player" comment.** Canonical case: Rebirth Granite/Rooted's `AnyAffected -JumpHeight`
      landed in `slow[jumpHeight]=500` and was self-applied as a nonsense ~−50000% caster jump;
      HC Reaction Time similarly self-applied a foe −70% jumpHeight. **HC is a VERIFIED no-op**
      (every HC self-penalty template is `target:'Self'` — Granite's whole slow map keeps the
      marker); only genuinely foe-classified entries stop leaking. Converter tags per-value
      (6 branches, `if (isSelfTargeting) x.toWho='Self'`); calc self-applies PER ENTRY
      ([character-totals.ts](src/utils/calculations/character-totals.ts) 3 sites, per-entry
      slow loop); 3 predicate sites (perma / attack-chain / power-row) use
      `hasSelfDirectedPenalty`. `rangeDebuff` self-branch confirmed dead (0 powers). Regen
      (49 files carry `toWho:'Self'`, 0 retain `selfPenalty`); lint clean, 805 tests
      (+4 guard [`self-penalty-towho.test.ts`](src/utils/calculations/self-penalty-towho.test.ts)),
      DSH6 detector-neutral, all 3 converter gates green.
      verify: file:src/utils/calculations/self-penalty-towho.test.ts, fn:hasSelfDirectedPenalty, tests:src>=805
    - [~] Retire the remaining bolt-ons. **Finding (verify-don't-assume):** `unresistable`
      and `durationVariants` are NOT independently retireable — each *is* the projection of
      multiple atomic records into one single-value `PowerEffects` slot (an unresistable-twin
      flag / a `[{scale,duration}]` mini-list), with no second representation to converge onto;
      "retiring" them requires the full `PowerEffects`-becomes-a-list rewrite. That rewrite
      landed (Phase 1 flip, 2026-07-08: `extractEffects` = `templatesToAtoms` →
      `projectAtomsToEffects`), unblocking both:
      - [x] **`durationVariants` retired as a bolt-on (Phase 2R, 2026-07-08).** The RESOURCES
        accumulate slots (healing, regenDebuff, absorb, …) are no longer built by an
        `addOrAccumulate` closure mutating the live bag per atom; the loop only classifies
        atoms into per-slot queues and `foldResourceSlot` computes each slot — variants
        included — as a pure fold over its queued atom list (durations, last-table-wins,
        duration-distinct-debuff grouping all in one documented function). The emitted
        `PowerEffects.durationVariants` field is unchanged (it IS the projection's output
        shape); byte-identical regen + armed shadow compare were the gate.
        verify: fn:foldResourceSlot, tests:src>=901
      - [x] **`unresistable` + twin pre-scan retired (Phase 3, 2026-07-08).** The pre-scan
        no longer drops the IgnoreResistance sibling nor mutates the atom list with a `_twin`
        stamp. The atom list stays IMMUTABLE — the `resistible:false` sibling is kept — and
        `unresistable` is derived at projection time from the atom's own `resistible`: a
        `twinRole` map marks each twin template's atoms `res` (route it, stamp `unresistable`)
        or `unres` (skip at routing — its resistable sibling already represents it).
        Skip-at-routing ≡ the old drop (a twin's only difference is the flag, so it routes to
        the same slot). Byte-identical regen + 0 shadow-compare divergences were the gate.
        verify: fn:projectAtomsToEffects, tests:src>=901
      `selfPenalty` is DONE (above). With Phase 2R + Phase 3 done, `projectAtomsToEffects`
      has no remaining mutate-in-place bolt-ons.
      - [x] **Legacy deletion (2026-07-09).** `extractEffectsLegacy` (~1,030 lines), the
        `DSH6_SHADOW_COMPARE` hook, the dead `__DSH6_ATOM_SINK__` hook, and the obsolete
        comparator `scripts/dsh6-shadow-project.cjs` are all removed now that the projection
        is the sole routing path and the shadow compare has no audience. `extractEffects` is
        now just `templatesToAtoms → projectAtomsToEffects → extractSummon`. The atom-ingest
        cross-check `scripts/attic/dsh6-shadow-atoms.cjs` survives (it validates `templatesToAtoms`
        against the reference encoder — independent of the deleted legacy). Regen
        byte-identical; 901 tests; gates ×3 green.
    needs: DEDUCTIVE_SCHEMA_HARNESS#DSH6
- [x] **DSH7 — DESCOPED (decision 2026-07-06, review follow-up).** The two halves
  split cleanly and only one survives:
  - **CI wiring of the *structural* gates — KEEP, and it is mostly already done.**
    DSH2/DSH3/DSH6a/DSH8 all ride `npm test`/`ci.yml` (committed inputs). This was the
    load-bearing part of DSH7 and it landed piecemeal with each detector. Remaining: nothing
    oracle-dependent (the `.mhd` is gitignored, so the oracle sweep cannot be a CI gate).
  - **Full-corpus numeric resolution + numeric CI — DROP.** Numbers are advisory-only by
    the trust boundary, and the 2026-07-06 `.025` finding is the proof: **Mids quantizes
    scale to 3 decimals**, so a full numeric sweep would emit a permanent sub-1% haze on
    every damage buff — noise by construction, never a gate. Even the point-value it was
    meant to add is thin: the `.025` question was settled by comparing Mids scale to the
    repo value directly, no `AttribMod.json` resolution needed.
  - **What remains is on-demand only:** resolve a *specific* disputed number
    (`scale × modifierTable` vs `AttribMod.json`) when a concrete case needs adjudication —
    e.g. the finite "Mids has a damage type we lack" candidate list from DSH5. Not a sweep,
    not CI, not a standing worklist.
  needs: DEDUCTIVE_SCHEMA_HARNESS#DSH5
- [x] **DSH8** — Extend the harness to the **Incarnate** pipeline. All sub-items
  shipped (bridge convergence, collapse detector + CI gate, Hybrid/Destiny
  pvMode+resistible awareness, Support Core/tspy parity, Alpha+Genesis calc-feeding
  sweep). Residual is coverage-only (non-gating class-absent slots, Deferred). The audit
  (2026-07-05, 2-agent) found [`convert-incarnate-effects.cjs`](scripts/convert-incarnate-effects.cjs)
  is a wholly separate 1,614-line converter that **independently reinvented** the
  bag-of-named-slots / last-write-wins model (it never imports `extractEffects()`),
  and is a *worse* blind spot than regular powers: it **never reads `pvMode` or
  `resistible`** (0 `is_pvp`/`IgnoreResistance` matches → PvE/PvP + resistible twins
  collapse silently), has silent allowlist drops (Destiny `:415`/`:420-423`, Hybrid
  `:588`/`:591`), keep-one-largest (Interface `:774`/`:782`, Judgement `:868`), and
  Alpha first-attrib-only + sum (`:269-278`). The bug family has **already shipped
  here repeatedly** — [`incarnate-effects-completeness.test.ts`](src/utils/calculations/incarnate-effects-completeness.test.ts)
  pins empty-`{}` Rebirth core, dropped Clarion mez, 300% debuffResistance, 8×
  Hybrid inflation.
  **Correction after verifying the audit's "cheap oracle win" (2026-07-05, verify-don't-assume):**
  the DSH5 oracle diff is the WRONG tool for the converter drops — both its sides
  (Mids oracle ↔ parser export) sit **upstream** of `convert-incarnate-effects.cjs`,
  so it structurally cannot see a converter drop. Confirmed empirically: incarnates
  are **already in the DSH5 sweep** (975 export incarnate powers matched by
  `full_name`), and their 533 UNCLASSIFIED residuals across 159 powers are
  **dominated by the known DSH4 `aspect=Str`→Enhancement relabel** (e.g. Alpha
  `accuracy_common`: export `Accuracy` vs oracle `Enhancement`) — parser-faithful,
  not drops. **The real drop-catcher is a DSH6a-style detector comparing the export
  atomic list to the converter output** — exactly the tool that is blind to incarnates.
  - [x] Diagnostic: confirm parser fidelity for incarnates via the existing DSH5
    sweep — done; the incarnate residuals are the `aspect=Str` relabel + scope, not
    converter drops, so DSH5 is NOT extended for DSH8 and the export JSON is a
    trustworthy "truth" side for the detector below.
  - [x] **Bridge convergence (DSH4) — defense on generic `_Ones` tables now classified;
    prereq for a faithful incarnate detector. SHIPPED 2026-07-05 (HC + Rebirth).** The
    detector's input side is `ingestExportPower` → [`bridgeAttrib`](src/data/core/atomic-effect.ts),
    but the bridge dropped every bare position/type attrib (`Melee`/`Ranged`/`Area`/`Smashing`/…)
    at aspect=Current to **Unmapped** unless the table name contained `def` — so the incarnate
    flat-buff tables (`Melee_Ones`/`Ranged_Ones`) left the detector **blind to Defense**. Root
    cause: the bare attrib *is* the defense characteristic (the damage/resistance face is always
    written `<type>_Dmg`); aspect=Current ⇒ Defense on **any** table. **Verified across the full
    HC export (13,733 files):** every bare-by-type @ Current template is defense — Barrier /
    Support Core incarnates + the positional NPC "Resistance" powers (whose real resistance rides
    `_Dmg`@Res) — with **zero** mez/notify co-listing at aspect=Current, disproving the bridge's
    old "co-listed noise" guard. **Fix:** one rule in `bridgeAttrib` (`asp === 'current'` ⇒
    `Defense|<sub>`). Incarnate coverage now HC/Rebirth **99.94%** (only the parser-side
    `Unknown(91)` remains — [[special-attrib-subindex-fix]], a parser gap, not the bridge). Oracle
    **corroborates**: DSH5 resolved 149 oracle-only Defense residuals (UNCLASSIFIED 6245→6107) and
    Mids independently classifies these `Defense|Melee/Ranged/AoE`. The 10 new EXPORT_ONLY
    signatures are a Toxic-set-completeness fold nuance (the live HC bin has Toxic positional
    defense; the ~5-week-stale Mids does not) — re-baselined into `oracle_divergence_rules.json`,
    not a defect. Converter `ATTRIB_MAP` left in place (its output already agrees with the fixed
    taxonomy: `defMelee`↔`Defense|Melee`, `resSmashing`↔`Resistance|Smashing`); a wholesale
    converter→bridge rewrite is unnecessary for detector fidelity — the detector's output-side
    SLOT_TABLE bridges the two representations. tsc + DSH3 (3 ds) + DSH5 cohort/regression + 810
    tests green.
    verify: fn:bridgeAttrib, file:src/data/atomic-effect.test.ts, tests:src>=810
  - [x] **Incarnate collapse-detector — SHIPPED, CI-gated 2026-07-06.**
    [`scripts/dsh8-incarnate-collapse-detector.cjs`](scripts/dsh8-incarnate-collapse-detector.cjs) —
    the DSH6-shaped detector for the incarnate converter. INPUT = DSH4 bridge atoms
    (`ingestExportPower`, single-source) from `exported_powers/<ds>/incarnate/<slot>/`;
    OUTPUT = the generated `incarnate-effects.ts` record (keyed by filename base, which IS
    the generated slug — `HYBRID_ID_ALIASES` maps only the friendly runtime IDs onto it).
    GATE = class-present, sibling-missing over the two multi-type BUFF slots that feed caster
    totals: **Hybrid** (passive/frontLoaded/perTarget) + **Destiny** (flat map). By-design
    drops DIFFER from DSH6 (aspect=Str DamageBuff/Accuracy ARE surfaced by incarnates; the
    drops are Enhancement/Heal/pets/engine; `player eq` is the kept leaguemate buff, NOT a PvP
    variant). Two FP classes traced and folded (the DSH6 discipline): (1) Incandescence
    Radial's `runSpeed` represents run+fly+jumpHeight — the Destiny calc fans one key to all
    three axes ([character-totals.ts:3147](src/utils/calculations/character-totals.ts#L3147));
    (2) aspect=Res on a buff effectType is debuff-resistance (→`debuffResistance`), not a buff.
    **Gate GREEN (0) across all three datasets** — HC + Rebirth confirm the Support Core fix
    holds; remaining tspy class-absent findings are non-support slots (Destiny/Hybrid residuals),
    tracked as coverage and non-gating. Wired: `npm run validate:incarnate-collapse` (all 3 ds, `--gate`) + vitest
    guard. Alpha/Genesis (single-aspect enhancement) + Interface/Judgement/Lore (proc/nuke/pet)
    are structurally collapse-free here — tracked as coverage, not swept.
    verify: file:scripts/dsh8-incarnate-collapse-detector.cjs, file:src/utils/calculations/dsh8-incarnate-collapse.test.ts, tests:src>=814
  - [x] **Detector by-design residual fold (ArchVillain_Res) — SHIPPED 2026-07-06.**
    The DSH8 detector now classifies Ageless-style `*_ArchVillain_Res` atoms as
    by-design projection to `debuffResistance` (not `resistanceAll`) instead of
    non-gating `Resistance|*` class-absent noise. Gate behavior unchanged
    (still 0 high-confidence collapses all datasets), but residual accounting is
    sharper: HC class-absent **48→0**; Rebirth/tspy residuals unchanged.
    verify: file:scripts/dsh8-incarnate-collapse-detector.cjs, tests:src>=814
  - [x] **Targeted `pvMode` + `resistible` awareness in Hybrid extraction — SHIPPED 2026-07-06.**
    [`convert-incarnate-effects.cjs`](scripts/convert-incarnate-effects.cjs)
    now applies a narrow PvP filter in `extractHybrid`: explicit/synthesized PvP
    groups are dropped by default, with the existing beneficial `enttype target>
    player eq` leaguemate-buff exception preserved (the Parse6-support-core fix).
    The per-template dedup key is now `resistible`-aware (`R/U`) so true
    resistible/unresistable twins are not silently coalesced during frontLoaded /
    perTarget accumulation. DSH8 gates + incarnate completeness regression stayed
    green on HC/Rebirth/tspy.
    verify: fn:extractHybrid, tests:src>=814
  - [x] **Targeted `pvMode` + `resistible` awareness in Destiny extraction — SHIPPED 2026-07-06.**
    `extractDestiny` now mirrors the same narrow PvP handling used by Hybrid:
    explicit/synthetic PvP rows are dropped by default with the beneficial
    `enttype target> player eq` leaguemate exception preserved (Parse6 safety).
    Timeline collapse is now resistible-aware (`duration × R/U`) so true
    resistible/unresistable twins are not coalesced into one duration bucket.
    DSH8 detector + incarnate completeness suite stayed green across HC/Rebirth/tspy.
    verify: fn:extractDestiny, tests:src>=814
  - [x] **Clarion PvE status-resistance leak — the Destiny pvMode fix's missed regen,
    caught + closed 2026-07-06.** Follow-up to the item above surfaced a **regen-hygiene
    gap, not a data bug**: commit `58915ecd0b` shipped the correct `extractDestiny` PvP
    drop but regenerated **only thunderspy** — HC + Rebirth `incarnate-effects.ts` were
    left stale, so the fix never went live for them. Consequence: the shipped HC/Rebirth
    planner still showed Clarion's `statusResistance` **2.1 (= 210%) in PvE**, even though
    the export marks those Confused/`aspect=Resistance` templates `is_pvp=PVP_ONLY`
    (scales 2.1/0.3/0.6). **Verified fresh bins were NOT implicated:** old converter
    (`09ea747a4e`) + current export = byte-identical to the committed `.ts`, so the drift
    is purely the un-run converter change. **Fix:** regenerated HC + Rebirth (9 Clarion
    entries each drop only `statusResistance`; mez/KB protection + the genuine PvE
    `debuffResistance` 0.7 Repel value all retained), and corrected the stale guard
    [`incarnate-effects-completeness.test.ts`](src/utils/calculations/incarnate-effects-completeness.test.ts)
    L98 (was pinning the leaked 2.1 as expected; now asserts `statusResistance` is
    **absent** in PvE). DSH8 gate + 23 incarnate tests + tsc green. **Process follow-up
    (Deferred):** a CI regen-diff guard on the incarnate converter (mirroring the powerset
    regen guard) would make a converter change un-mergeable without matching generated
    output — this class of "fix shipped, output stale" only exists because no such gate
    covers `convert-incarnate-effects.cjs`.
    verify: file:src/utils/calculations/incarnate-effects-completeness.test.ts, tests:src>=814
  - [x] **Support Core Hybrid leaguemate-buff drop — FIXED 2026-07-05 (HC + Rebirth).**
    The 4 *Core* Support Hybrids (Support Core Genome / Partial-Core / Total-Core Graft /
    Core Embodiment) rendered an EMPTY caster buff. They gate it with `enttype target>
    player eq` (the leaguemate value the caster receives) + `enttype target> critter eq`
    (pets, "doubled"), but `extractHybrid` recognized only empty-req / self-RPN /
    per-target-RPN, so the whole buff routed nowhere; the Radial line (empty req) worked,
    hiding it. **Verified 3 ways:** empty output; Mids carries a PvE DamageBuff+Defense
    for all 4; in-game help ("+Damage, +Accuracy, +Defense(All) to all leaguemates …
    doubled for pets") names the buff.
  - [x] **CORRECTION — Rebirth was NOT a no-op; the fix's first guard was wrong.** The
    initial fix keyed on `is_pvp != PVP_ONLY`, which left Rebirth/Thunderspy empty and
    was mis-recorded as a "verified no-op." Re-investigation (poking the reachable Rebirth
    bins — `z_rebirth_bin.pigg` under the Sweet Tea launcher, *contra* the stale
    "rebirth-bins-absent" note) found **two Parse6-only traps**, both systemic:
    (1) **CASE** — Rebirth writes `Enttype target> player eq` (capital E); the match was
    lowercase-only. (2) **SYNTHESIZED is_pvp** — Parse6 has no explicit is_pvp field, so
    the bin parser *derives* it from the requires target-type
    ([_powers.py `_parse_effects_parse6`](tools/bin-crawler/bin_crawler/parser/_powers.py#L1859)),
    marking **every** `player eq` group `PVP_ONLY` and every `critter eq` `PVE_ONLY`. That
    synthesis is *harmless for FOE effects* (the `critter eq` PVE sibling carries the
    effect to the critter foe in PvE) but **wrong for ally-BUFFS**, whose caster is a
    *player* and needs the `player eq` copy in PvE. **Proof it's a synthesis artifact, not
    real data:** a coherence tally shows HC keeps `player eq`/`critter eq` groups
    independent of is_pvp (121/124 all `EITHER`), while Rebirth AND Thunderspy — two
    independently-forked Parse6 servers — both lock `player eq`→PVP_ONLY 100% (176/176,
    110/110) and `critter eq`→PVE_ONLY 100% (172/172, 112/112); a real is_pvp field is not
    100%-determined by target-type. An HC↔Rebirth divergence cross-check confirmed the 790
    foe-effect divergences are harmless and the discriminator is **polarity, not
    magnitude** (ratio 2.0 is dominated by PvP-halved foe mez twins — Force Bolt/Eagles
    Claw Stun, Ki Push Repel — sitting next to the 0.06/0.12 ally buff). **Fix:** key the
    leaguemate route on POLARITY (`scale > 0` — extractHybrid only maps beneficial stats)
    + a case-insensitive match, dropping the is_pvp dependency; HC's explicit
    is_pvp=EITHER is the ground truth. Also added a `(bucket,statKey,scale)` dedup so the
    Parse6 *per-attrib split* (8 `*_Dmg` groups → one `damage` stat) counts once, not 8×
    (+48%→+6%). HC + Rebirth regen; HC unchanged; Rebirth Core now populates (damage 0.06,
    Defense(All), Accuracy). tsc + DSH3 (3 ds) + 810 tests (+2 Rebirth guards).
    verify: fn:extractHybrid, file:src/utils/calculations/incarnate-effects-completeness.test.ts, tests:src>=810
  - [x] **Thunderspy Support Hybrid parity — FIXED 2026-07-06.** Tspy's Parse6 export
    encodes Support front-loaded buffs as generic-category attribs
    `[Damage]`/`[Defense]`/`[Accuracy]` with empty aspect, and omits the Grant_Power linkage
    for Support passives (the main power carries only an `Ones` marker with no `power_names`).
    Converter fix in [`convert-incarnate-effects.cjs`](scripts/convert-incarnate-effects.cjs):
    (1) map generic front-categories to planner stats (`Damage`→`damage`,
    `Defense`→`defenseAll`, `Accuracy`→`accuracy`), and (2) infer
    `support_boost_{common,uncommon,rare,very_rare}` by tier when Parse6 omits linkage,
    then map Support silent `Ones`@empty-aspect to passive `enduranceDiscount`.
    Regen verified on tspy generated output (`support_genome_*` now emits
    `passive.enduranceDiscount` 0.025/0.05/0.075/0.1 and non-empty support frontLoaded),
    with `incarnate-effects-completeness.test.ts` guards and DSH8 gate green.
  - [x] **Calc-feeding sweep (Alpha + Genesis) — SHIPPED 2026-07-06.** The
    export-vs-generated sweep over every alpha/destiny/genesis silent file across
    all three datasets surfaced one **real, systemic drop**: **all 72 thunderspy
    Alpha entries rendered EMPTY** (0/72 populated vs HC/Rebirth 72/72) — a slotted
    Agility/Cardiac/Musculature/… gave the tspy planner **zero** enhancement. Root
    cause is the **same Parse6 Grant_Power-linkage omission** as the Support Hybrid
    fix: tspy alpha mains carry only a bare `Ones` marker with no `power_names`, so
    [`extractGrantedPowers`](scripts/convert-incarnate-effects.cjs) returned `[]`.
    **Fix (tspy-isolated, verified no-op for HC/Rebirth):** (1) when linkage is
    empty, recover it from the parallel HC alpha power of the same id
    ([`inferAlphaSilentFromReference`](scripts/convert-incarnate-effects.cjs)) —
    every silent file HC references (93) exists in the tspy export (0 missing), and
    values resolve against tspy's OWN silent scales; (2) fold tspy's split `Ones`
    ED-bypass template into the per-aspect sum (regular + Ones; e.g. accuracy 0.11 +
    0.22 = 0.33, matching HC) — HC/Rebirth have **zero** `Ones` alpha templates so
    this is a pure no-op there; (3) a defensive `PVP_ONLY`-group skip (alpha silent
    files are all `EITHER`, so no `pvMode`/`resistible` drop exists to fix — checked).
    **Genesis is clean** (37/37 populated all datasets); the destiny/genesis
    `DISTINCT-A0-SCALES` sweep hits are by-design multi-stat grants handled by their
    own per-attrib extractors, not the Alpha first-attrib collapse. Regen: tspy only
    (single clean hunk = the `GENERATED_ALPHA_EFFECTS` block); DSH8 gate + completeness
    suite green all 3 ds. The `ATTRIB_MAP`→DSH4-bridge convergence was **not needed**
    for this drop (linkage, not attrib-classification) and stays optional cleanup.
    verify: fn:inferAlphaSilentFromReference, file:src/utils/calculations/incarnate-effects-completeness.test.ts, tests:src>=814
  needs: DEDUCTIVE_SCHEMA_HARNESS#DSH4
- [x] **DSH9** — Extend the harness to the **Enhancement** pipeline (IO sets / set
  bonuses / procs / raw magnitudes). The audit found this splits into three
  sub-pipelines with very different exposure — and, unlike incarnates, the *data* is
  already in good shape; the gaps are the **extractors** and a **total absence of a
  Mids oracle**. Set-bonus data is already an atomic list `{stat,value,desc}`
  ([`io-sets.ts`](src/data/io-sets.ts)) and the Rule-of-5 calc has no collapse; proc
  data is already `ProcEffect[]` ([`proc-data.ts`](src/data/proc-data.ts)). But
  [`read_i12.py`](tools/mids-oracle/read_i12.py) reads **only** the I12 Powers section
  (`BEGIN:POWERS`→`BEGIN:SUMMONS`) and never opens `EnhDB.mhd`, so set-bonus and proc
  *values* are diffed against **nothing** from Mids —
  [`io-sets-bonus-keys.test.ts`](src/data/io-sets-bonus-keys.test.ts) only proves the
  two allowlists agree with *each other*, not with the game.
  - [x] **`EnhDB.mhd` reader — SHIPPED 2026-07-06** as
    [`read_enhdb.py`](tools/mids-oracle/read_enhdb.py) + smoke test
    [`test_read_enhdb.py`](tools/mids-oracle/test_read_enhdb.py). Ports
    `DatabaseAPI.LoadEnhancementDb` + `Enhancement(BinaryReader)` +
    `EnhancementSet(BinaryReader)` verbatim (reuses `read_i12`'s `Effect` reader for FX
    payloads), with the same EOF-alignment self-check as `read_i12` (parses local HC
    `EnhDB.mhd`: 1,345 enhancements / 227 sets / 128 procs, lands exactly on EOF).
    verify: file:tools/mids-oracle/read_enhdb.py, file:tools/mids-oracle/test_read_enhdb.py
  - [x] **Coverage + value-diff bootstrap — SHIPPED 2026-07-06** as
    [`diff_enh_oracle.py`](tools/mids-oracle/diff_enh_oracle.py) +
    [`test_diff_enh_oracle.py`](tools/mids-oracle/test_diff_enh_oracle.py). Diffs oracle
    set names + proc tuples vs `io-sets-raw.ts`/`proc-data.ts` (identity), and with
    `--value-diff` projects EnhDB→I12 set-bonus links to planner `(stat,value)` for a
    value-level compare. Residual-signature baseline + `--strict` regression gate +
    `--triage-json`. **Root-cause fix (review follow-up, see [20240706.md](20240706.md)):**
    set-bonus links were resolved by the EnhDB's cached power `index` (a constant −22
    offset vs the I12 array, verified across all 1,138 links) → every bonus read the
    WRONG power; the stale `DamageBuff/Str ×250` multiplier had been calibrated on that
    garbage. Fixed by resolving links by `full_name` + default ×100 → value residuals
    1014/1422/54 → **26/43/20**. The 20 remaining are all the **Mids 3-decimal scale
    quantization** skew (same scale `0.025` → repo `2.5` AND `2.525`) — advisory per the
    trust boundary, NOT staleness, NOT a mapping bug. Proc identity residuals remain
    baseline-frozen (`missing=2`, `extra=56`) and are gate-on-new under `--strict`.
    verify: file:tools/mids-oracle/diff_enh_oracle.py, file:tools/mids-oracle/test_diff_enh_oracle.py
  - [x] **Freeze-and-gate the proc residual NOW (2026-07-06) — DONE 2026-07-06.**
    Applied the DSH5 discipline while the pile is small: baseline + `--strict` is the
    gate, and only NEW residual signatures fail. Triage was refined to split
    oracle-staleness from extractor gaps, including two key corrections:
    `Convert Knockback to Knockdown` is a special conversion mechanic (not a chance-proc
    mapping miss), and Stupefy's extra `Chance for Stun` is bucketed as likely
    per-set oracle staleness (set exists, no oracle counterpart) rather than a mapping
    gap. Current 56-extra split: `likely_mapping_gap` **0** /
    `likely_non_proc_global_or_passive` 37 / `likely_oracle_set_staleness` 16 /
    `likely_oracle_proc_staleness` 1 / `unknown` 2. Actionable mapping-gap worklist is
    now empty (`enh_oracle_mapping_gap_worklist.{json,md}`: **P1=0 / P2=0 / P3=0**).
    The 20 value residuals remain frozen as advisory quantization skew (gate-on-new).
    verify: file:tools/mids-oracle/enh_oracle_mapping_gap_worklist.md
  - [x] Converge the divergent parallel bridges onto the DSH4 `bridgeAttrib`:
    `ATTRIB_TO_BONUS_STAT` ([extract-rebirth-io-sets-v2.py](scripts/extract-rebirth-io-sets-v2.py) `:724`)
    + `ATTRIB_ASPECT_TO_EFFECT` ([extract-proc-data.py](scripts/extract-proc-data.py) `:51`)
    are duplicates of it. **SHIPPED 2026-07-06** via shared adapter
    [`bridge-attrib-one.cjs`](scripts/bridge-attrib-one.cjs) and bridge-first routing in
    both extractors, with narrow legacy fallbacks only for known non-bridge edge cases.
    Validation: `test_read_enhdb.py` PASS, `test_diff_enh_oracle.py` PASS,
    `io-sets-bonus-keys.test.ts` + `set-bonus-groups.test.ts` PASS, lint PASS.
  - [x] Replace the value-keyed family collapse (`_resolve_bonus_effects` `:875-894` —
    a float-rounding split can leak or mis-collapse a per-type family) with
    identity-keyed grouping; fix the single-type mez-resist drop (`:715-718`) and the
    double-allowlist lockstep risk vs
    [`STAT_NAME_MAP`](src/utils/calculations/set-bonuses.ts). **SHIPPED 2026-07-06.**
    `_resolve_bonus_effects` now groups by identity first (not value), family-collapse
    is identity-based, single-type mez-resistance maps to aggregate
    `mez_resistance_(all)`, and paired-stat de-dupe is preserved post-aggregation.
    Validation: `io-sets-bonus-keys.test.ts` + `set-bonus-groups.test.ts` +
    `mez-duration-bonus.test.ts` PASS; `py_compile` + lint PASS.
  - [x] Close the proc allowlist gaps: `applySingleProcEffect` `default:` drop
    ([character-totals.ts](src/utils/calculations/character-totals.ts) `:2345`) +
    typed-`Defense`-only-when-`'all'` drop (`:2194`); the `proc:false` silent-drop
    class (the ATO passive-global 6th piece). **SHIPPED 2026-07-06.** Runtime proc
    application now handles `Absorb`, keeps typed defense/global handling aligned, and
    includes a guarded legacy fallback for `isProc:false` passive-global slots.
    Validation: `proc-runtime-allowlist.test.ts` + proc coverage/resolution tests PASS;
    lint PASS.
  - **Raw enhancement magnitudes** (schedules/ED/exemplar in
    [`enhancement-values.ts`](src/utils/calculations/enhancement-values.ts)) are **not**
    an atomic-effect target — they're table lookups. Any validation there needs a
    value/table oracle, tracked under Deferred.
  needs: DEDUCTIVE_SCHEMA_HARNESS#DSH4
- [x] Replace the AT-table extractor's hand-maintained 45-name allowlist
  ([`extract-at-tables.cjs`](scripts/extract-at-tables.cjs)) with a principled
  filter (extract every player-referenced table, or all 110 binary tables with a
  documented skip-list). The allowlist is the **same inductive-schema anti-pattern**
  as the converter's bag-of-slots — it silently omits real tables until a power
  references one on a fatal slot. DSH2 found `Melee_Debuff_Dam` + `*_EndDrain` this
  way; there are ~62 more unextracted tables that only escape notice because no
  fatal-slot power references them yet. **SHIPPED 2026-07-06.** Replaced fixed
  `RELEVANT_TABLES` with a source-driven filter over usable `named_tables` referenced by
  player AT + pet class exports. Validation: `converter-table-integrity.test.ts` PASS,
  `validate:converter` PASS on homecoming/rebirth/thunderspy, lint PASS.

## Deferred

- Surfacing PvP values in the planner UI — `pvMode` enters the schema/harness so
  PvE can't be clobbered by PvP; exposing a PvP view is a separate product
  decision, not part of this correctness effort.
- Full numeric parity with Mids' calc engine — structural correctness is the
  mission; numeric equality is inherently skew-limited against a stale oracle
  (DSH7 does resolved-number diffing only as a low-severity advisory tier).
- Running Mids headless to export a golden JSON DB (`SaveJsonDatabase`) — reserved
  for if a Windows/dotnet box appears; not on the critical path (the `.mhd` reader
  in DSH1 is fully self-contained on Linux). Rejected as primary path: WinForms/
  dotnet unavailable here.
- Parser rewrite to recover compound data — **non-goal**; the parser already
  preserves it. Only candidate parser change: make the two silent parse-failure
  swallow paths ([`_powers.py`](tools/bin-crawler/bin_crawler/parser/_powers.py)
  ~L764, ~L878) loud under a strict flag so an unparseable PvP twin can't vanish
  before JSON. Diagnostics only.
- Enhancement **raw-magnitude** value oracle — validating the hand-transcribed
  schedule / ED / exemplar tables
  ([`enhancement-values.ts`](src/utils/calculations/enhancement-values.ts)) against a
  Mids value/table source (`Maths.mhd`/`NLevels.mhd`). Low collapse exposure (table
  lookups, not an effect model), so it is not part of DSH9's atomic treatment;
  surfaced here so the scope boundary isn't mistaken for forgotten work.

- [x] **Same collapse in other slots — RESOLVED at the parser layer 2026-07-07** (the
  "tspy Ageless End / Melee-hybrid Regen / Rebirth Destiny Regeneration class-absent"
  deferred note). Investigation found the converter-map framing was wrong twice over:
  **(1) the committed tspy incarnate exports were STALE** — the 2026-07-06 parser
  hybrid-relabel (commit `950c929d50`) shipped with no re-export, so its own fix was
  inert, and **(2) the tspy bin's post-`requires` index array carries the REAL affected
  attribs for Destiny too** (verified by instrumented raw-bin probes: zero swallowed
  templates; Barrier `Ones`→10 bare defense types, Clarion `Ones`→6 mez / `Knockback`,
  `Res_Boolean`→6 mez + `isPVPMap?`, `Tempdamage`→`Heal_Dmg`, `ArchVillain_Res`→the
  20-attrib debuff-res bundle). Fixes at the right layer
  ([_powers.py](tools/bin-crawler/bin_crawler/parser/_powers.py)): `incarnate_scope`
  replaces `hybrid_scope` — a new **Destiny relabel** (defense/mez-prot/KB-prot/status-res/
  heal/debuff-res, each idx-verified vs the HC parallel by (scale,duration) alignment) and a
  **melee-tree fix** to the hybrid relabel (`*_Dmg` on `Incarnate.Hybrid.Melee_*` =
  RESISTANCE per HC/Rebirth/help — the uniform `Strength` synthesis would have shipped
  melee res buffs as +damage). Re-export adopted (763 files: 48 substantive = exactly the
  relabels; 715 = benign `modes_required` metadata). Converter: HC-reference passive-boost
  inference generalized to ALL hybrid trees (`inferHybridPassiveFromReference`, the
  Alpha pattern) + tspy silent-row token maps (melee regen / assault damage / control
  status-res). **tspy planner gains:** Clarion mez+KB protection (was NOTHING), Barrier
  defenseAll, Ageless Radial debuffResistance 50%, Rebirth click heal (healScale 5–8),
  melee-hybrid res/def toggles + all melee/assault/control passives. Support
  `defenseAll`→`defMelee` (the bin's index names ONLY Melee, count=1 byte-verified — the
  category-token guess overstated it). **Detector residuals to 0/0/0:** tspy coverage
  20→254 atoms; the HC/Rebirth `MezResist ×9` (Clarion) classified = Repel/Taunt/Placate
  resist projected to scalar `debuffResistance` by design + the `isPVPMap?` PvP-map twin
  (both folded in [dsh8-incarnate-collapse-detector.cjs](scripts/dsh8-incarnate-collapse-detector.cjs)).
  HC/Rebirth converter output byte-identical (verified regen no-op).
  verify: file:tools/bin-crawler/bin_crawler/parser/_powers.py, fn:inferHybridPassiveFromReference, file:src/data/thunderspy-support-hybrid.test.ts, tests:src>=831
- [x] **Parser→export staleness guard — SHIPPED 2026-07-07.** Closes the class the DSH8
  work surfaced twice (a parser change shipping with no re-export → inert downstream fix;
  `exported_powers/` had NO gate, unlike the `generated/` regen-diff). The `generated/`
  trick is impossible here — CI has neither the gitignored `.pigg` archives nor Python, so
  the powers export can't be regenerate-and-diffed. Instead a **fingerprint manifest**:
  [`export_powers.py`](tools/bin-crawler/bin_crawler/export_powers.py) stamps each dataset's
  output dir with `_export_manifest.json` recording the sha256 of the powers-exporter source
  (every `parser/*.py` + `export_powers.py`, via
  [`_export_fingerprint.py`](tools/bin-crawler/bin_crawler/_export_fingerprint.py) — a
  directory glob, not a curated allowlist); a vitest guard
  ([`export-staleness.test.ts`](src/data/export-staleness.test.ts)) recomputes it from the
  committed sources and asserts every dataset matches. If the parser changed but a dataset
  wasn't re-exported, its fingerprint diverges → red; the only fix is to actually re-export
  (re-stamp). Per-dataset (HC root / rebirth / thunderspy), mirroring regen-diff's all-three
  stance, so a "re-exported tspy only" slip is caught on HC/rebirth. **The guard immediately
  did its job:** turning it on revealed HC + tspy exports were already stale by ~two parser
  generations (2026-07-04 vintage). Adopted the catch-up: HC 2024 files (**calc-relevant** —
  97 generated corrections: Shadow Step stealth+durations, `buffDuration` fixes), tspy 4068
  files (all `modes_required` metadata, **calc-inert** — 0 generated change), rebirth already
  current (manifest only). Three previously-stale-pinning tests flipped to the resolved state
  (all verify-don't-assume, not blind updates): (a) `set-mode-resolution` mag==1 attrib-118
  impostor → `XPDebtProtection` (self-predicted by the test's own comment); (b)
  `granted-damage-procs` Bio Offensive Adaptation now correctly surfaces its Toxic DoT
  scoped to its own stance (mode-system parsing separated the stances — defensive/efficient
  still emit no damage proc, verified no leak); (c) `atomic-effect` bridge coverage — 18 new
  engine/script attribs (Set_Costume/Silent_Kill/Ninja_Run/Translucency/… — attrib-118 fix
  fallout) mapped to `Meta`/`Stealth` in `bridgeAttrib`'s `META_EFFECT`. Full suite green.
  verify: file:src/data/export-staleness.test.ts, file:tools/bin-crawler/bin_crawler/_export_fingerprint.py, fn:bridgeAttrib, tests:src>=832
- [x] **CI regen-diff guard for the incarnate converter — SHIPPED 2026-07-06.** The Clarion
  PvE status-resistance leak (DSH8, above) shipped only because a correct
  `convert-incarnate-effects.cjs` change regenerated one dataset and not the others, with no
  gate catching the drift. **Root cause of the miss:** the existing regen-diff guard
  ([regen-diff.yml](.github/workflows/regen-diff.yml)) DID rebuild `generated/` (incl.
  `incarnate-effects.ts`) — but only for **HC + Rebirth**; thunderspy was silently excluded
  from `regen-all.cjs`'s default dataset list, so a tspy-only (or HC/Rebirth-only) regen could
  still drift undetected. Fix: made thunderspy a first-class regen target — its `generated/`
  tree reproduces byte-identical from committed `exported_powers/` (verified 0-diff), so
  `regen-all.cjs` default now includes it and the guard asserts all three datasets' `generated/`.
  The stale "FUTURE: regen-and-diff guard" sketch in `ci.yml` (the guard already exists) was
  corrected. This closes the incarnate-converter half of the class for all datasets.
  verify: file:scripts/regen-all.cjs, file:.github/workflows/regen-diff.yml
- [x] **at-tables.ts stale outputs adopted across all 3 datasets — SHIPPED 2026-07-06.**
  Regenerated [`at-tables.ts`](src/data/datasets/homecoming/at-tables.ts) for homecoming,
  rebirth, and thunderspy from the already-landed source-driven extractor
  ([`extract-at-tables.cjs`](scripts/extract-at-tables.cjs)); this activates the prior
  extractor fix that had been inert while outputs stayed on the old 45-name allowlist.
  As predicted by the defer note, adoption was not calc-neutral: `_inherentdamage`
  tables becoming directly resolvable caused pseudo-pet runtime double-count in
  [`calculateResolvedPseudoPetDamage`](src/utils/calculations/pet-damage.ts). Fixed by
  mirroring the main damage-path filter there (drop `*_PvP*` and
  `*InherentDamage*` siblings when a non-inherent sibling exists; fallback to
  inherent-only rows when needed). Validation after regen+fix: targeted AT-table risk
  suites PASS (`pseudopet-redirect`, `converter-table-integrity`, `strength-buffs`,
  `trick-arrow-fixes`, `converter-invariants`, `assassin-strike-damage`) and
  `npm run validate:converter` PASS on homecoming/rebirth/thunderspy.
  verify: file:src/utils/calculations/pet-damage.ts, file:src/data/datasets/homecoming/at-tables.ts
