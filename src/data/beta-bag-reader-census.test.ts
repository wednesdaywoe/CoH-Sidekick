/**
 * BPORT4 — the guard on `scripts/beta-bag-reader-census.cjs`, and on the adjudication it carries.
 *
 * BPORT1 pinned supply. This pins the other half: which seams the strip turns into dead code
 * and which into a wrong number, plus the four finder behaviours the whole answer rests on. A
 * finder that quietly narrows is the failure mode here — every one of the three corrections
 * BPORT4 made to its own population was a finder seeing less than it looked like it saw, and
 * a narrowed finder reports FEWER casualties, which reads as progress.
 *
 * Split the way BPORT1's guard is: the full census runs as a child process (it walks all four
 * datasets to answer the coverage question), while the source-only finders are exercised
 * in-process, because those are the assertions that have to be cheap enough to keep.
 */
import { describe, it, expect, beforeAll } from 'vitest';
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { createRequire } from 'node:module';
import path from 'node:path';

const REPO = path.resolve(__dirname, '../..');
const SCRIPT = path.join(REPO, 'scripts/beta-bag-reader-census.cjs');
const SIBLING = path.resolve(REPO, '../coh-sidekick-1.0');
const ORACLE = 'src/utils/calculations/legacy-totals.oracle.ts';
/** Where canonical kept the same file. PROD7 renamed it on this side only. */
const ORACLE_TWIN = 'src/utils/calculations/character-totals.ts';
const read = (rel: string) => readFileSync(path.join(REPO, rel), 'utf8');
const require_ = createRequire(import.meta.url);
const {
  bagSeams, stripComments, guardExpr, siblingSeams, counterpartOf, assertCounterpartsLive,
} = require_(SCRIPT) as {
  bagSeams: (rel: string) => Seam[];
  stripComments: (src: string) => string;
  guardExpr: (before: string) => string | null;
  siblingSeams: (rel: string, siblingRoot: string) => Seam[];
  counterpartOf: (rel: string) => string;
  assertCounterpartsLive: (siblingRoot: string) => void;
};

interface Seam {
  file: string;
  line: number | null;
  slot: string | null;
  binding: string;
  owner: string | null;
  guardedBy: string | null;
  guardCovers: boolean;
  verdict: string;
  display: string;
  supply: { own: number; cond: number; pet: number; disp: number; pspet: number } | null;
  /** Present only when the census ran with `--sibling`: what the other repo did with this seam. */
  sibling?: 'reads-too' | 'migrated-there' | 'absent';
  /** Atom-query symbols the sibling's copy of this file imports and ours does not. */
  atomArmGap?: string[];
}

interface Census {
  seams: Seam[];
  readerFiles: string[];
  sweep: string[];
  buckets: {
    bothRead: string[]; identical: string[]; betaOnly: string[]; migrated: string[]; renamed: string[];
  } | null;
  builders: string[];
  feeds: Record<string, string[]>;
  coverage: { powers: number; tally: Record<string, { stats: number; bag: number; bagOnly: number }> };
}

/**
 * The seams BPORT7 turns into dead code rather than into a zero.
 *
 * Each is a bag slot whose number ALSO rides `power.stats` or the power's own top level, on
 * every carrier — `bagOnly: 0` over 14,391 powers. That is the whole materiality line BPORT1
 * deferred: seven execution stats and `damage` survive the strip because a second arm already
 * answers, and no other dying slot has one.
 *
 * A pair leaving this set is the ENDSTAT-1 failure repeating — a fallback the code documents
 * and the data does not honour — so it reds here before BPORT7 ships it.
 */
const COVERED_ARMS: string[] = [
  // BPORT7's regen emptied the bag; the 6 surviving pairs are guards whose seams still
  // declare them (bag=0 bagOnly=0 atom=undefined — no atom side computed, no bag to compare).
  'damage/damage',
  'stats.arc/arc',
  'stats.castTime/castTime',
  'stats.endurance/enduranceCost',
  'stats.radius/radius',
  'stats.recharge/recharge',
];

/**
 * Seams where canonical has ALREADY grown the atom arm and the beta has not.
 *
 * The stream doc scoped BPORT4 expecting "mostly leave, matching canonical". Per file that
 * reads right — both repos read the bag. Per SLOT it is the opposite: canonical calls
 * `movementCapBumpValue(p)` before it looks at `effects.movementCapBump`, and its
 * `character-totals` had no stack read left at all. So the answer for these is CARRY, not
 * leave, and the list is BPORT6's work order rather than a curiosity.
 *
 * A seam joining this map means canonical migrated something else and the beta is now one
 * more behind; a seam leaving it means the beta caught up, or — the case worth catching —
 * canonical grew a bag read back.
 *
 * **BPORT6 corrected the list twice before working it, both times downward.** The 33 seams
 * BPORT4 counted were 21:
 *
 *  - **Eight were a defect in the instrument.** A roster-bound seam (`effects[key]` over a
 *    `DOMINATION_MEZ_KEYS` loop) is minted from a hand-kept register, not found by the
 *    finder, and the sibling comparison ran the FINDER over canonical — which can never
 *    return a seam it did not produce. So every roster seam reported `migrated-there`
 *    unconditionally. Six of the eight were wrong: canonical's `getPowerDominationSummary`
 *    is byte-identical to this repo's and reads the same bag, so "carry canonical's arm"
 *    named an arm that does not exist. `siblingRosterVerdict` decides these on the symbol
 *    now. The other two were right — `ROUTED_SUBTYPES` really was deleted — and the register
 *    kept minting them anyway, which `assertSitesLive` now refuses.
 *  - **Four more are already adjudicated elsewhere.** `perma.ts`'s pair is the
 *    `authoredCycle` arm BPORT2 kept deliberately and pinned by census, and `inherents.ts`'s
 *    is `getDominationInfo` — canonical reads it off the inherent power through
 *    `@/data/archetype-inherent`, a module this fork does not have. Both wait on the same
 *    thing: PARTSTAT-2, the join from an archetype's declared inherent to the
 *    `Inherent.Inherent` power that holds it, which is canonical-only.
 *
 * What was left, and what BPORT6 carried: `power-row-utils` (the whole predicate, atom-native
 * — see `toggle-roster-atom-native.verify.test.ts`) and `character-totals`
 * (`collectStrengthBuffs` onto `specialBuffValue` + `stackCapOf`). The `summon` cluster was
 * BPORT7's by BPORT3's verdict — reader and writer crossed together once
 * `convert-powerset.cjs` lifted the slot — and left this map with the crossing: five files
 * dropped out, and what remains is BPORT6's pair, still waiting on PARTSTAT-2.
 */
const CANONICAL_MIGRATED_FIRST: Record<string, string[]> = {
  // BPORT7's regen emptied the bag; the beta has no bag-only seams, so nothing carries.
};

/**
 * Files whose canonical copy calls atom-native readers this repo's copy does not.
 *
 * The complement of the map above, and the reason that map is a LOWER bound. `StatsDashboard`
 * still names `effects.movementCapBump` in both repos, so a per-slot comparison calls it
 * "reads-too" and stops — but canonical called `movementCapBumpValue(p)` first and only fell
 * back, while this repo called nothing. Canonical's own comment on that seam records what the
 * difference costs: at its strip, "Quantum Acceleration on all four forks and Energy Flight on
 * Homecoming/Brainstorm answered no cap raise at all, with nothing red to say so".
 *
 * BPORT6 emptied three of the four. `StatsDashboard` and `character-totals` took their arms;
 * `power-row-utils` went atom-native outright and imports more than canonical's copy does now
 * (`absorbMaxHPFractionValue`, which canonical has landed and not yet wired into its own
 * toggle predicate). `inherents.ts` is the one left, and its two symbols are the
 * `getDominationInfo` read that waits on PARTSTAT-2 — an entry here, not an omission.
 */
const ATOM_ARM_BEHIND = [
  'src/utils/calculations/inherents.ts',
  // BPORT5's, and the largest gap in the repo: 27 atom queries canonical's copy of this file
  // calls and the oracle does not. It reads `absent` — no arm gap at all — until the rename
  // register resolves the counterpart, which is how the gap stayed invisible.
  ORACLE,
];

/** Files handed a display bag as a prop — the hop BPORT1 could not see by import edge. */
const PROP_FED = [
  'src/components/info/DamageBlock.tsx',
  'src/components/info/PowerInfoBlocks.tsx',
  'src/components/info/SharedPowerComponents.tsx',
];

let census: Census;

beforeAll(() => {
  const args = [SCRIPT, '--json', ...(existsSync(SIBLING) ? ['--sibling', SIBLING] : [])];
  census = JSON.parse(execFileSync('node', args, {
    cwd: REPO, encoding: 'utf8', maxBuffer: 256 * 1024 * 1024,
  })) as Census;
}, 300_000);

describe('BPORT4 census — what the strip costs, per seam', () => {
  it('finds exactly these slots with a second arm that already answers', () => {
    const covered = Object.entries(census.coverage.tally)
      .filter(([, v]) => v.bagOnly === 0)
      .map(([k]) => k)
      .sort();
    expect(covered).toEqual([...COVERED_ARMS].sort());
  });

  it('proves the second arm on every carrier, not on a sample', () => {
    // The claim is `bagOnly === 0`, and it is only worth anything against the whole
    // population: a covered pair measured on one dataset would say nothing about the three
    // forks whose converters wrote the bag differently.
    expect(census.coverage.powers).toBeGreaterThan(14_000);
    for (const pair of COVERED_ARMS) {
      expect(census.coverage.tally[pair].bagOnly, pair).toBe(0);
    }
  });

  it('leaves no dying slot silently credited with an arm it does not have', () => {
    // The complement, stated so a NEW covered pair has to be adjudicated rather than
    // inherited: every other dying slot must report every carrier as a casualty.
    for (const [pair, v] of Object.entries(census.coverage.tally)) {
      if (COVERED_ARMS.includes(pair)) continue;
      expect(v.bagOnly, pair).toBe(v.bag);
    }
  });

  it('resolves the prop hop BPORT1 left open, and finds only builders behind it', () => {
    expect(Object.keys(census.feeds).sort()).toEqual([...PROP_FED].sort());
    for (const [fed, from] of Object.entries(census.feeds)) {
      expect(from.every((f) => census.builders.includes(f)), fed).toBe(true);
    }
  });

  it('reproduces the population the stream doc scoped BPORT4 against', () => {
    if (!census.buckets) {
      // Stated rather than silent: without the sibling checkout there is nothing to
      // partition against, and a skipped assertion that says so is not a passing one.
      expect(existsSync(SIBLING)).toBe(false);
      return;
    }
    const b = census.buckets;
    expect(b.bothRead.length + b.identical.length + b.betaOnly.length + b.migrated.length +
      b.renamed.length).toBe(census.sweep.length);
    // 27 until BPORT6, when `power-row-utils` stopped reading the bag entirely and left the
    // bucket; 27 again at BPORT11, and the new member reads nothing. BPORT7's crossing took
    // `CompareSlottingModal` out — its only bag read was the summon that moved to
    // `power.summon`. `character-totals.ts` declares {@link syntheticEffects}, whose body is
    // `power.effects` — the sweep's grep sees a bag read, the corrected finder sees no slot,
    // and the asymmetry is BPORT4's whole point restated. Pinned as a pair so the bucket
    // cannot grow a REAL reader unnoticed.
    expect(b.bothRead).toHaveLength(26);
    expect(b.bothRead).toContain('src/utils/calculations/character-totals.ts');
    expect(bagSeams('src/utils/calculations/character-totals.ts')).toHaveLength(0);
    // 6 until BPORT5. The sixth was the oracle, and it was never beta-only — canonical kept
    // the same file under the name PROD7 renamed away from on this side. BPORT7's crossing
    // dropped `proc-potential` — its two summon reads were its only bag seams.
    // 5 since PROD6B-BETA-PARITY's closure added `powerProjectionFillPin`, whose only `.effects`
    // is in the prose explaining which bag the beta's collision test asks — the same
    // sweep-sees-a-read / finder-sees-no-slot asymmetry `character-totals.ts` carries above, and
    // pinned the same way, as a pair. A REAL reader entering this bucket has seams and still
    // trips the count.
    expect(b.betaOnly).toHaveLength(5);
    expect(b.betaOnly).toContain('src/engine/powerProjectionFillPin.ts');
    expect(bagSeams('src/engine/powerProjectionFillPin.ts')).toHaveLength(0);
    expect(b.renamed).toEqual([ORACLE]);
  });

  it('names the seams canonical migrated first, which the beta owes BPORT6', () => {
    if (!census.buckets) return; // no sibling: the disposition is unmeasurable, not empty
    const carried: Record<string, string[]> = {};
    for (const s of census.seams) {
      // The oracle's carry is BPORT5's whole work order and is pinned on its own below —
      // folding 150 seams into BPORT6's map would bury the seven files BPORT6 was scoped on.
      if (s.file === ORACLE) continue;
      if (s.sibling !== 'migrated-there') continue;
      if (s.verdict !== 'dies' && s.verdict !== 'caller-decides') continue;
      (carried[s.file] ??= []).push(s.slot!);
    }
    for (const k of Object.keys(carried)) carried[k] = [...new Set(carried[k])].sort();
    expect(carried).toEqual(CANONICAL_MIGRATED_FIRST);
  });

  it('names the files whose atom arm canonical has and this repo does not', () => {
    if (!census.buckets) return; // no sibling: the disposition is unmeasurable, not empty
    const behind = [...new Set(census.seams
      .filter((s) => (s.atomArmGap ?? []).length > 0)
      .map((s) => s.file))].sort();
    expect(behind).toEqual([...ATOM_ARM_BEHIND].sort());
    // The one that made the point, now from the other side: `StatsDashboard` still names
    // `effects.movementCapBump` in both repos — so the per-slot comparison still calls it
    // "reads-too" — and the arm gap it reported is closed. A gap re-appearing here is
    // canonical growing a reader this fork has not taken.
    const dash = census.seams.find((s) => s.file === 'src/components/layout/StatsDashboard.tsx')!;
    expect(dash.sibling).toBe('reads-too');
    expect(dash.atomArmGap).toEqual([]);
  });

  it('decides a roster-bound seam on the sibling s own constant, not on the finder', () => {
    // The instrument defect BPORT6 found. `rosterSeams` mints these; the finder cannot
    // return them; the sibling comparison asked the finder. Every roster seam therefore read
    // `migrated-there`, which is the one answer that turns into work — "carry canonical's
    // arm" — for a file where canonical has no arm. Stated on the six that were wrong.
    const roster = census.seams.filter((s) => s.binding.startsWith('roster:'));
    expect(roster.length, 'roster-bound seams found').toBeGreaterThan(0);
    const domination = roster.filter((s) => s.binding === 'roster:DOMINATION_MEZ_KEYS');
    expect(domination.map((s) => s.slot).sort())
      .toEqual(['confuse', 'fear', 'hold', 'immobilize', 'sleep', 'stun']);
    for (const seam of domination) expect(seam.sibling, seam.slot ?? undefined).toBe('reads-too');
  });

  it('finds the two bag readers the file-level sweep cannot see', () => {
    // Both take the bag as a PARAMETER and never write `.effects` themselves, so the
    // instrument that built the 27 has no way to reach them. `types/power.ts` is one of the
    // 175 paths verify-sync calls identical, which is why this matters: the same blind spot
    // is in the canonical repo's copy of the same census.
    const missed = census.readerFiles.filter((f) => !census.sweep.includes(f));
    expect(missed.sort()).toEqual([
      'src/components/info/SharedPowerComponents.tsx',
      'src/types/power.ts',
    ]);
  });
});

/**
 * BPORT5 — the oracle's carry, and the lookup that hid it.
 *
 * The stream doc scoped BPORT5 as a choice between two ways to FREEZE `legacy-totals.oracle.ts`:
 * a frozen pre-strip dataset as its input, or its output frozen into fixtures. Both were priced
 * against "this file has no migrated twin to carry" — and it has one. PROD7 quarantined the
 * legacy calc under a new name on this side only; canonical never split its copy and migrated it
 * in place through its own STRIP-1. Every sibling question here is keyed by path, so the twin
 * answered `absent` 165 times and the census reported the strongest verdict available without
 * ever performing the lookup.
 *
 * What the resolved lookup says: canonical's copy is the same file (31 of the oracle's 34
 * declarations), it is the oracle behind `emit-totals-fixtures.ts` -> `totals_gate.rs`, and post
 * strip it reads the bag at six seams. So the answer is CARRY, on the BPORT6 pattern, and the
 * two freezes are the fallback nobody needs.
 *
 * These assertions are written as identities rather than as literals wherever the carry will
 * move them: the residual is "the slots canonical still reads", derived from canonical's own
 * source, so a family landing removes its rows here without an edit and canonical growing a bag
 * read back reds instead of passing.
 */
describe('BPORT5 — what grades the engine once the bag is gone', () => {
  it('resolves the oracle onto the twin canonical kept under the old name', () => {
    expect(counterpartOf(ORACLE)).toBe(ORACLE_TWIN);
    // Unmapped paths pass straight through — the register is an exception list, not a routing
    // table, and a file whose name both repos share must not go near it.
    expect(counterpartOf('src/utils/calculations/perma.ts')).toBe('src/utils/calculations/perma.ts');
    if (!census.buckets) return; // no sibling: the disposition is unmeasurable, not empty
    expect(census.buckets.renamed).toEqual([ORACLE]);
    expect(census.buckets.betaOnly).not.toContain(ORACLE);
  });

  it('refuses a rename that has stopped naming the same file', () => {
    if (!existsSync(SIBLING)) return; // nothing to compare the register against
    expect(() => assertCounterpartsLive(SIBLING)).not.toThrow();
    // The register is a claim about LINEAGE, and lineage rots without deleting anything: the
    // day canonical splits its copy the way PROD7 split this one, the mapping starts grading
    // two unrelated modules and reports the difference as work. Both failure directions are
    // exercised, because a check that only catches deletion catches the easy half.
    const tmp = mkdtempSync(path.join(tmpdir(), 'bport5-'));
    try {
      expect(() => assertCounterpartsLive(tmp)).toThrow(/absent/);
      mkdirSync(path.dirname(path.join(tmp, ORACLE_TWIN)), { recursive: true });
      writeFileSync(path.join(tmp, ORACLE_TWIN), 'export function somethingElse() { return 1; }\n');
      expect(() => assertCounterpartsLive(tmp)).toThrow(/stopped being the same file/);
    } finally {
      rmSync(tmp, { recursive: true, force: true });
    }
  });

  it('splits the oracle s seams into carry and residual on canonical s own reads', () => {
    if (!census.buckets) return; // no sibling: the disposition is unmeasurable, not empty
    const mine = census.seams.filter((s) => s.file === ORACLE);
    // The population, pinned: a finder that narrows here reports LESS work, which reads as
    // progress, and BPORT11 is the row that legitimately shrinks it — so the number moves
    // with each carry and the pin is what makes each move deliberate. 165 at BPORT5 (157 in
    // source plus the 8 `mezProtTypes` roster keys); 141 after BPORT11's first cluster took
    // accuracy, recharge, maxEnd, endurance-discount, perception, range and elusivity, with
    // their three stacking slots each; 131 once the mez cluster took the six MEZ, KB/KU,
    // mezResistance, taunt, placate and the empty `effects.protection` object; 129 when repel
    // joined that fold; 127 when the `protTeleport` stat was retired with its read; 107 once
    // defense, defenseBuffSuppressible, resistance, resistanceDebuff and debuffResistance
    // crossed, taking the pet-aura fold's `effects.defense` key with them; 67 once the movement
    // cluster went — the five empty scalars, the axis map, and the self slow / cap-debuff pair;
    // and 17 when the last cluster landed. BPORT5's finish line is the six residual slots
    // canonical also reads plus the eight `mezProtTypes` roster keys — 14 — and the three over
    // it are ABSORB, the one family BPORT11 declined to carry. BPORT7 A2 carried it: the oracle
    // resolves the MaxHP-fraction probe first, the flat fold second, the pet-aura mint last —
    // the shape canonical's own strip settled on — and the per-foe increment neither reader can
    // express is the named ABSORB-4 residual, which `serverParity` excludes by key and logs
    // unverified rather than letting it red the walk.
    //
    // The carry does NOT count the arms it keeps: `syntheticEffects(power)?.rechargeBuff`
    // names no `effects.` prefix, so the finder cannot see it and does not. That is right —
    // it is the totals pass reading back its own output — but it means this number measures
    // the DATA seams only, which is the population the strip empties.
    expect(mine).toHaveLength(14);
    expect(mine.every((s) => s.sibling !== 'absent')).toBe(true);

    // The residual is derived, not listed — canonical's own copy answers it.
    const residual = new Set(siblingSeams(ORACLE, SIBLING).map((s) => s.slot));
    for (const s of mine) {
      if (s.binding.startsWith('roster:')) continue; // decided on the symbol, not the finder
      expect(s.sibling, `${s.slot} @${s.line}`)
        .toBe(residual.has(s.slot!) ? 'reads-too' : 'migrated-there');
    }

    // What canonical kept: four guarded proc-PPM reads (`stats.x ?? effects.x`) and the two
    // slots its conditional expansion mints onto a synthesized bag. Nothing else survived its
    // strip, which is the shape this carry lands on.
    expect([...residual].sort()).toEqual(
      ['arc', 'castTime', 'defenseBuff', 'radius', 'recharge', 'resistance'],
    );
  });

  it('leaves the mez roster with canonical, which still reads it', () => {
    if (!census.buckets) return; // no sibling: the disposition is unmeasurable, not empty
    // The BPORT6 correction applied to this file: a roster seam names no slot at the read site,
    // so the finder can never return it and the sibling comparison must ask the symbol instead.
    // Answered `reads-too` — canonical's `mezProtTypes` loop is still there — so these eight are
    // NOT part of the carry, and a finder-based verdict would have made them work.
    const roster = census.seams.filter((s) => s.file === ORACLE && s.binding === 'roster:mezProtTypes');
    expect(roster.map((s) => s.slot).sort()).toEqual(
      ['confuse', 'fear', 'hold', 'immobilize', 'knockback', 'knockup', 'sleep', 'stun'],
    );
    for (const s of roster) expect(s.sibling, s.slot ?? undefined).toBe('reads-too');
  });

  it('measures the carry as an arm gap, not only as a bag read', () => {
    if (!census.buckets) return; // no sibling: the disposition is unmeasurable, not empty
    const gap = census.seams.find((s) => s.file === ORACLE)!.atomArmGap ?? [];
    // The per-slot comparison is a lower bound (the `StatsDashboard` lesson): a slot canonical
    // still names reads `reads-too` even where it calls an atom query first. The import gap is
    // the other half, and on this file it is the largest in the repo — the oracle imported 12 of
    // `atom-query`'s helpers at BPORT5 and imports 35 now; canonical's copy of it imports 39.
    // BPORT11 shrinks this by construction: 27 at BPORT5, 18 once the first cluster's seven
    // helpers were called here too, and 4 once BPORT7 A2 called the two absorb readers.
    // Asserted as a shrinking bound plus the named residue, so
    // a carry that lands removes rows without an edit while a helper going UNCALLED again
    // reds.
    // What is left is not a gap in the totals path at all. `specialBuffValue` and
    // `SPECIAL_BUFF_STACK` are `collectStrengthBuffs`' arms, which live in `character-totals.ts`
    // on this side and so read as "the oracle does not call them"; `atomsOf` and `isDebuffAtom`
    // are canonical's `carries_combat_debuff`, which is in that same file here. The gap is an
    // artifact of PROD7's split, not work owed.
    // `specialBuffValue` / `SPECIAL_BUFF_STACK` are `collectStrengthBuffs`' arms and `atomsOf` /
    // `isDebuffAtom` are `carries_combat_debuff`'s; all four live in `character-totals.ts` on
    // this side, so they read as "the oracle does not call them" and are an artifact of PROD7's
    // split rather than work owed. The two absorb readers that stood in this gap until now are
    // carried by BPORT7 A2 — the per-foe channel they cannot express is the ABSORB-4 residual.
    expect(gap.sort()).toEqual(['SPECIAL_BUFF_STACK', 'atomsOf', 'isDebuffAtom', 'specialBuffValue']);
    // And what has landed, which is the half that must not silently come back.
    for (const carried of ['accuracyBuffValue', 'rechargeBuffValue', 'rangeBuffValue',
      'perceptionBuffValue', 'enduranceDiscountValue', 'maxEndBuffValue', 'elusivityValue',
      'mezSlotValue', 'mezResistanceValue', 'tauntPlacateValue', 'debuffResistanceValue',
      'selfSlowValue', 'selfMovementCapDebuffValue', 'movementAxisSubType', 'stealthValue',
      'selfDamageDebuffValue', 'selfRechargeDebuffValue', 'damageBuffIsDefianceOnly',
      'absorbValue', 'absorbMaxHPFractionValue']) {
      expect(gap, carried).not.toContain(carried);
    }
  });
});

describe('BPORT4 finders — the four ways a bag read hides from a regex', () => {
  it('does not count prose', () => {
    // `atom-query.ts` names the bag slot each atom query replaces, 49 times, in doc comments.
    // Counted as reads they made the file the second-largest bag reader in the repo — a file
    // whose entire purpose is to not read the bag.
    expect(bagSeams('src/data/core/atom-query.ts')).toHaveLength(0);
    expect(stripComments('a; // effects.damage\nb;')).not.toContain('effects.damage');
    expect(stripComments('const s = "// effects.damage";')).toContain('effects.damage');
  });

  it('follows an alias', () => {
    // BPORT7 crossed the summon read this fixture used to pin — the one alias seam in the
    // 24 — so the fixture now holds on a surviving control-slot read of the same alias.
    const seams = bagSeams('src/components/modals/PowersetCompareModal.tsx');
    expect(seams.length).toBeGreaterThan(0);
    expect(seams.every((s) => s.binding === 'alias:power')).toBe(true);
    expect(seams.map((s) => s.slot)).toContain('hold');
  });

  it('follows an alias bound through a chain, not just an identifier', () => {
    // `const domEffects = getArchetype('dominator')?.inherent?.effects` — two optional hops,
    // and an identifier-only pattern reported `getDominationInfo` as reading nothing at all.
    const seams = bagSeams('src/utils/calculations/inherents.ts')
      .filter((s) => s.binding.startsWith('alias:'));
    expect(seams.map((s) => s.slot).sort()).toEqual(['buffDuration', 'recharge']);
  });

  it('does not count an assignment as a read', () => {
    // The oracle MINTS the seven buff-pet aura slots onto a synthesized bag
    // (`effects.regenBuff = sc`). Those are supplier 3 writing, not a reader spending, and
    // counting them made the mint look like the mint being consumed — the exact inversion the
    // supply census exists to prevent. Stated on lines that would otherwise land: each one
    // matches the direct finder's pattern exactly and is excluded only by the assignment test.
    const seams = bagSeams('src/utils/calculations/legacy-totals.oracle.ts');
    const src = read('src/utils/calculations/legacy-totals.oracle.ts').split('\n');
    const mints = src
      .map((l, i) => ({ line: i + 1, l }))
      .filter(({ l }) => /^\s*effects\.\w+ = (?!effects\.)/.test(l));
    expect(mints.length).toBeGreaterThanOrEqual(5);
    for (const { line } of mints) {
      expect(seams.some((s) => s.line === line), `line ${line}`).toBe(false);
    }
  });

  it('tells a real guard from a chain of dying reads', () => {
    expect(guardExpr('const c = p.stats?.recharge ?? ')).toBe('p.stats?.recharge');
    expect(guardExpr('if (e?.hold || ')).toBe('e?.hold');
    expect(guardExpr('if (p.stats?.castTime == null && ')).toBe('p.stats?.castTime');
    expect(guardExpr('const hasDamage = !!p.damage || !!')).toBe('p.damage');
    expect(guardExpr('const x = ')).toBeNull();
    // `e?.hold || e?.stun` reads as guarded and is not: both arms are the same bag and die
    // together. The alias set is what makes that decidable, so this is asserted on a real
    // file rather than on a string.
    const cmp = bagSeams('src/components/modals/PowersetCompareModal.tsx');
    expect(cmp.filter((s) => s.guardedBy && s.guardCovers)).toHaveLength(0);
  });
});
