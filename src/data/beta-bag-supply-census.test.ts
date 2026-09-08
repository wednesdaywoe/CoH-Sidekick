/**
 * BPORT1 — the guard on `scripts/beta-bag-supply-census.cjs`.
 *
 * The census is the measurement the STRIP-1 beta port stands on: it decides, per bag slot,
 * whether a reader is spending something or standing over an empty shelf, and BPORT3/BPORT4
 * adjudicate their seams against its verdicts. A measurement nothing grades drifts, and this
 * one drifts in a particularly quiet way — a converter change moves supply, and the census
 * keeps reporting whatever it now finds without anyone noticing the answer changed.
 *
 * So this pins the SETS, not the counts. `own: 1065` moving to `own: 1071` is a re-export and
 * says nothing; a slot crossing from LIVE to DEAD, or from converter-supplied to minted-only,
 * changes what the port is allowed to delete. Counts stay out of the assertions for the same
 * reason `emit-totals-fixtures` freezes shapes rather than magnitudes.
 *
 * **BPORT13 re-took the measurement.** This file pins SETS precisely so that a change in supply
 * has to be read and signed for rather than absorbed, and BPORT7 is the largest change in
 * supply the beta will ever see. Four things moved, and one of them falsifies a claim the port
 * was relying on:
 *
 *  1. **Six of the nine `MINT_ONLY_SLOTS` did not survive the strip.** The set's own comment
 *     said "BPORT7's regen empties the authored bag and cannot touch these". It could:
 *     `defense`, `fly`, `runSpeed`, `runSpeedUnenhanced`, `jumpHeight` and `jumpSpeed` all
 *     report `displayMint: 0` now and are DEAD. The distinction the set needed was not
 *     minted-vs-emitted, it was where the mint READS FROM. `castTime`, `enduranceCost`,
 *     `accuracy`, `range`, `recharge`, `radius`, `arc` and `maxTargets` mint out of
 *     `power.stats` and are untouched; the six that died were `buildDisplayEffects` folding the
 *     bag's own `movement` container and the pet-aura fold reading a pet power's bag — mints of
 *     the bag, not mints beside it. A mint is strip-proof only if its source is.
 *  2. **The DEAD set grew from 6 to 27**, which is the strip working: a read slot whose
 *     converter supply is gone and whose mint does not reach it is a reader over an empty
 *     shelf, and naming all 27 is what lets BPORT3/BPORT4 tell a dead read from a live one.
 *  3. **`rechargeBuff` lost its converter supply entirely** — `own` was 317 and is 0. It stays
 *     LIVE on 20 conditional carriers, which is a different argument for the same verdict and
 *     has to be written down as such.
 *  4. **Both undeclared-key sets emptied.** `activationTime`/`endurance`/`interruptTime` were
 *     un-renamed execution stats in the emitted bag and went with it; the two movement mints
 *     went with the six above.
 *
 * And one thing this census does NOT see, recorded here because BPORT3 and BPORT4 adjudicate
 * deletions against its `own` column: **the overrides layer.** `generatedModules()` walks
 * `src/data/datasets/<ds>/generated` only, so a hand-written override carrying an `effects` key
 * is invisible to every `own` count in this file. 36 homecoming override files still carry one
 * (see the guard at the bottom). A slot this census calls `own: 0` may still be supplied on
 * Homecoming and on no other fork, which is exactly the shape of hole TEAMBUFF-1 was.
 *
 * Run as a child process rather than required in-process: the census loads every generated
 * module on all four datasets (~12.5k modules, ~1.3 GB peak), and a vitest worker holding
 * that alongside the rest of the suite is how a runner gets OOM-killed. The child also
 * exercises the CLI the stream doc names as the artifact.
 */
import { describe, it, expect, beforeAll } from 'vitest';
import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync, existsSync } from 'node:fs';
import path from 'node:path';

const REPO = path.resolve(__dirname, '../..');

/** Every `.ts` file under a directory, recursively; empty if the directory does not exist. */
function listTs(dir: string): string[] {
  if (!existsSync(dir)) return [];
  return readdirSync(dir, { withFileTypes: true }).flatMap((e) => {
    const full = path.join(dir, e.name);
    if (e.isDirectory()) return listTs(full);
    return e.isFile() && full.endsWith('.ts') ? [full] : [];
  });
}
const SCRIPT = path.join(REPO, 'scripts/beta-bag-supply-census.cjs');

interface Row {
  slot: string;
  own: number;
  cond: number;
  petReachable: number;
  petRoster: number;
  displayMint: number;
  pseudoPetMint: number;
  dataSupply: number;
  mintSupply: number;
  supply: number;
  verdict: 'LIVE' | 'DEAD';
  survivesStrip: boolean;
  diesWithStrip: boolean;
  readCount: number;
  readFiles: string[];
  readFilesOutsideDisplay: string[];
}

interface Census {
  datasets: string[];
  slots: string[];
  rows: Row[];
  counts: Record<string, {
    powers: number;
    conditionals: number;
    bagCarriers: number;
    bagWithoutAtoms: number;
    narrowMissed: number;
    displayFailures: number;
  }>;
  undeclaredInData: Record<string, number>;
  dynamicKeysUndeclared: Record<string, string[]>;
  undeclaredMints: Record<string, number>;
  buffPetMintedSlots: Record<string, string>;
  displayBagBuilders: string[];
}

/**
 * Slots a non-test reader spends that NO supplier fills, on any of the four datasets.
 *
 * These are the only branches the census proves dead outright, and each one is a deletion
 * BPORT3/BPORT4 may make without further evidence. A slot leaving this set has gained a
 * supplier; a slot joining it has lost its last one, which for a converter-supplied slot is
 * exactly the BPORT7 regression this file exists to catch early.
 *
 * `speedBuff` and `enduranceCrash` joined at BPORT3, and not because supply moved: both are
 * registered in `EFFECT_REGISTRY`, which is the domain of a reader the census could not see
 * until BPORT3 gave it an entry in `DYNAMIC_READ_SITES`. They were dead all along and
 * counted as unread. A registered display row that no supplier ever fills is dead in the
 * same way the other four are.
 */
const ZERO_SUPPLY_SLOTS = [
  // The six BPORT1 found: no supplier on any fork, before or after the strip.
  'dot', 'elusivity', 'protection', 'flySpeed', 'speedBuff', 'enduranceCrash',
  // Converter-supplied until BPORT7 emptied the bag, and no mint reaches them.
  'accuracyBuff', 'accuracyDebuff', 'damage', 'defenseBuffSuppressible', 'movementCapBump',
  'perceptionDebuff', 'placate', 'repel', 'specialDebuff', 'teleport', 'threatBuff',
  'threatDebuff',
  // The stacking metadata, which lived in the bag rather than beside it (see
  // `stacking-flaw-fix.verify.test.ts`, restated onto `atom.stackCap`).
  'maxStacks', 'stackCaps', 'stacksLinear',
  // The six that were declared mint-only and were not: their mint read the bag.
  'defense', 'fly', 'runSpeed', 'runSpeedUnenhanced', 'jumpHeight', 'jumpSpeed',
];

/**
 * Slots the converters emit that genuinely nothing spends.
 *
 * BPORT1 reported fourteen. Eleven of those were read the whole time by
 * `resolvePowerMagnitudes`, which names no slot — it walks the bag and keeps whatever the
 * registry registers — so a census keyed on `effects.<slot>` saw no reader for
 * `defenseDebuff` (1,623 carriers), `enduranceDrain` (964) and nine more the info panel
 * renders on every power that has them. Three survive the correction, and they are the only
 * emitted keys a deletion may take on the "nothing reads it" argument alone.
 */
const UNREAD_BUT_SUPPLIED: string[] = [];  // all three were converter-emitted; BPORT7 took them

/**
 * Names a dynamic reader's roster claims that `PowerEffects` does not declare.
 *
 * A derived roster is the reading code's own statement about what the bag can hold, so a
 * name the type contradicts is an inert arm of that reader rather than a typo. All four are
 * `characterStateAdapter`'s: `adjusterAffectsSelfTotals` tests a conditional's keys against
 * a 32-name set, and these four can never match anything.
 */
const DYNAMIC_KEYS_UNDECLARED = {
  SELF_TOTAL_EFFECT_KEYS: ['regeneration', 'recovery', 'maxEndurance', 'maxHealth'],
};

/**
 * Slots with no converter supply at all — spent only where a mint reaches.
 *
 * The distinction the port turns on: BPORT7's regen empties the authored bag and cannot touch
 * these, because they come from `power.stats`, a pseudo-pet, or a pet entity's auras. Deleting
 * one of these reads because "the bag is gone" would break a live surface.
 */
const MINT_ONLY_SLOTS = [
  // Minted out of `power.stats`, which the strip never touched. These are the reads a
  // "the bag is gone, delete it" argument must NOT take.
  'enduranceCost', 'castTime', 'accuracy', 'range', 'recharge', 'radius', 'arc', 'maxTargets',
  // Minted by the display edge and the pseudo-pet fold from sources outside the bag.
  'healing', 'taunt',
];

/**
 * Keys the converters emit into the bag that `PowerEffects` does not declare.
 *
 * `endurance` / `activationTime` are the un-renamed execution stats — the same pair
 * `transformEpicPower` destructures away into `enduranceCost` / `castTime` — so the bag and the
 * type disagree about their spelling on every primary and secondary power. A fourth name
 * appearing here means a converter started emitting something no reader is typed for.
 */
const UNDECLARED_IN_DATA: string[] = [];  // emptied by BPORT7 — see the header

/** Keys `buildDisplayEffects` mints that `PowerEffects` does not declare. */
const UNDECLARED_MINTS: string[] = [];  // both were movement mints of the bag; see the header

let census: Census;

beforeAll(() => {
  const raw = execFileSync('node', [SCRIPT, '--json'], {
    cwd: REPO,
    encoding: 'utf8',
    maxBuffer: 256 * 1024 * 1024,
  });
  census = JSON.parse(raw) as Census;
}, 300_000);

const row = (slot: string): Row => {
  const r = census.rows.find((x) => x.slot === slot);
  if (!r) throw new Error(`census has no row for slot "${slot}"`);
  return r;
};

describe('BPORT1 census — the population it walks', () => {
  it('covers all four datasets', () => {
    expect([...census.datasets].sort()).toEqual(['brainstorm', 'homecoming', 'rebirth', 'thunderspy']);
  });

  it('finds no power carrying a bag without atoms', () => {
    // The shared sweep's `isPower` requires an `atoms` array, which is right for a gate
    // comparing the two and wrong for a supply census — a bagged power with no atoms is the
    // most interesting row there is. The census walks a wider predicate and reconciles; this
    // asserts the two populations still agree, so the narrowing costs nothing today.
    for (const ds of census.datasets) {
      expect(census.counts[ds].bagWithoutAtoms, ds).toBe(0);
      expect(census.counts[ds].narrowMissed, ds).toBe(0);
    }
  });

  it('builds a display bag for every power', () => {
    // A power the display path throws on is a slot count the census never took.
    for (const ds of census.datasets) {
      expect(census.counts[ds].displayFailures, ds).toBe(0);
    }
  });

  it('walks a non-trivial corpus on each dataset', () => {
    for (const ds of census.datasets) {
      expect(census.counts[ds].powers, ds).toBeGreaterThan(2000);
      expect(census.counts[ds].conditionals, ds).toBeGreaterThan(100);
    }
  });
});

describe('BPORT1 census — the verdicts BPORT3 and BPORT4 adjudicate against', () => {
  it('proves exactly these read slots dead in all five suppliers', () => {
    const dead = census.rows.filter((r) => r.readCount > 0 && r.verdict === 'DEAD').map((r) => r.slot);
    expect(dead.sort()).toEqual([...ZERO_SUPPLY_SLOTS].sort());
  });

  it('finds exactly these read slots minted-only — the strip cannot empty them', () => {
    const mintOnly = census.rows.filter((r) => r.survivesStrip).map((r) => r.slot);
    expect(mintOnly.sort()).toEqual([...MINT_ONLY_SLOTS].sort());
  });

  it('keeps every mint-only slot free of converter supply', () => {
    for (const slot of MINT_ONLY_SLOTS) {
      const r = row(slot);
      expect(r.own, slot).toBe(0);
      expect(r.cond, slot).toBe(0);
      expect(r.mintSupply, slot).toBeGreaterThan(0);
    }
  });

  it('has no totals reader left for the movement mints, which is what BPORT11 settled', () => {
    // The four flattened movement axes are minted by `buildDisplayEffects` out of the nested
    // `movement` container, and the totals oracle used to read them while building no display
    // bag — so for THAT reader the slot had no supply at all, which is what this test pinned.
    // BPORT11 retired the four scalar blocks (0 carriers on any fork) and the oracle left the
    // list. Kept as the inverse assertion, because a reader coming BACK is the regression:
    // it would be reading a slot the strip does not fill and the mint does not reach.
    for (const slot of ['runSpeed', 'runSpeedUnenhanced', 'jumpHeight', 'jumpSpeed']) {
      expect(row(slot).readFilesOutsideDisplay, slot)
        .not.toContain('src/utils/calculations/legacy-totals.oracle.ts');
      // What remains is BPORT3's verdict, not this row's: the registry walker names no slot,
      // and the adapter reads supplier 2 only. Named so "nobody reads it" is not assumed.
      expect(row(slot).readFilesOutsideDisplay, slot).toEqual([
        'src/components/info/resolvePowerMagnitudes.ts',
        'src/engine/characterStateAdapter.ts',
      ]);
    }
    expect(census.displayBagBuilders).not.toContain('src/utils/calculations/legacy-totals.oracle.ts');
  });
});

describe('BPORT1 census — supplier 3, the buff-pet mint', () => {
  it('mints seven slots, one per ally-aura PetEffect type', () => {
    expect(Object.keys(census.buffPetMintedSlots)).toHaveLength(7);
  });

  it('still agrees with the switch inside the frozen oracle', () => {
    // The census restates the oracle's mapping rather than importing it, because the oracle
    // exports nothing and its header forbids editing it into agreement with anything. A
    // restatement needs a tripwire or it is just a second, quieter source of truth.
    const src = readFileSync(path.join(REPO, 'src/utils/calculations/legacy-totals.oracle.ts'), 'utf8');
    const start = src.indexOf('function buffPetAuraEffects(');
    expect(start, 'buffPetAuraEffects not found in the oracle').toBeGreaterThan(-1);
    const body = src.slice(start, src.indexOf('\n}', start));
    for (const [type, slot] of Object.entries(census.buffPetMintedSlots)) {
      const caseAt = body.indexOf(`case '${type}':`);
      expect(caseAt, `oracle has no case for PetEffect type ${type}`).toBeGreaterThan(-1);
      const nextCase = body.indexOf("case '", caseAt + 6);
      const arm = body.slice(caseAt, nextCase === -1 ? undefined : nextCase);
      expect(arm, `${type} no longer writes effects.${slot}`).toContain(`effects.${slot}`);
    }
  });

  it('has no source for the seventh slot on any dataset', () => {
    // `RechargeBuff` is in the oracle's switch and in the aura-type set, and no pet entity in
    // any of the four datasets carries one — reachable AND roster are zero. The branch is
    // unreachable today.
    //
    // What kept the slot LIVE has changed underneath that, and the assertion has to change with
    // it or it stops meaning anything: `own` was 317 converter carriers and is now 0. The slot
    // survives on 20 `conditionalEffects` carriers, one of the two suppliers STRIP-1 left
    // standing. Same verdict, different reason, so both halves are pinned.
    const r = row('rechargeBuff');
    expect(r.petReachable).toBe(0);
    expect(r.petRoster).toBe(0);
    expect(r.own, 'the converter bag came back').toBe(0);
    expect(r.cond, 'the conditional supply that keeps it LIVE').toBeGreaterThan(0);
    expect(r.verdict).toBe('LIVE');
  });
});

describe('BPORT1 census — where the bag and its type disagree', () => {
  it('finds exactly the known undeclared keys in the emitted data', () => {
    expect(Object.keys(census.undeclaredInData).sort()).toEqual([...UNDECLARED_IN_DATA].sort());
  });

  it('finds exactly the known undeclared keys minted at the display edge', () => {
    expect(Object.keys(census.undeclaredMints).sort()).toEqual([...UNDECLARED_MINTS].sort());
  });

  it('finds exactly the known inert names in the dynamic readers\' rosters', () => {
    expect(census.dynamicKeysUndeclared).toEqual(DYNAMIC_KEYS_UNDECLARED);
  });
});

describe('BPORT1 census — the slots nothing spends', () => {
  it('finds exactly these emitted slots with no reader at all', () => {
    const unread = census.rows.filter((r) => r.readCount === 0 && r.supply > 0).map((r) => r.slot);
    expect(unread.sort()).toEqual([...UNREAD_BUT_SUPPLIED].sort());
  });

  it('credits the registry-driven reader for the eleven slots BPORT1 called unread', () => {
    // The correction itself, pinned. Each of these is named by no `effects.<slot>` read
    // anywhere and rendered by `RegistryEffectsDisplay` on every power carrying it, so a census
    // keyed on `effects.<slot>` reported no reader for all eleven. That half is unchanged by
    // the strip and is the half the correction was: the registry still reaches every one.
    const REGISTRY_READ = ['accuracy', 'threatBuff', 'defenseDebuff', 'regenDebuff',
      'recoveryDebuff', 'enduranceDrain', 'threatDebuff', 'perceptionDebuff', 'specialDebuff',
      'fly', 'untouchable'];
    for (const slot of REGISTRY_READ) {
      expect(row(slot).readFiles, slot).toContain('src/components/info/resolvePowerMagnitudes.ts');
    }
    // The other half was `supply > 0` on all eleven, and BPORT7 moved it: five keep a supplier
    // and six do not. Split rather than dropped, because the two halves fail for different
    // reasons and only one of them is a regression. A slot leaving the supplied list is the
    // strip working; a slot leaving the read list is the registry losing a row, which is what
    // would let a later deletion take a live surface with it.
    const supplied = REGISTRY_READ.filter((slot) => row(slot).supply > 0);
    expect(supplied.sort()).toEqual(
      ['accuracy', 'defenseDebuff', 'enduranceDrain', 'recoveryDebuff', 'regenDebuff', 'untouchable'],
    );
  });

  it('names the bag supply this census cannot see: 36 homecoming override files', () => {
    // `generatedModules()` walks `src/data/datasets/<ds>/generated` and nothing else, so every
    // `own` count above is blind to the hand-written overrides layer — which still carries an
    // `effects` key on 36 Homecoming powers and on no power of any other fork. That matters
    // here specifically, because BPORT3 and BPORT4 decide what may be deleted using this
    // file's verdicts, and a slot reported `own: 0` can still be supplied on one fork.
    //
    // Guarded rather than fixed: teaching the census to load the overrides would move `own` on
    // eight slots and re-open every adjudication built on those numbers, which is its own row.
    // What this owes is that the population cannot grow, or reach a second fork, unseen.
    const OVERRIDE_ROOT = 'src/data/datasets';
    const bySlot: Record<string, Record<string, number>> = {};
    for (const ds of ['homecoming', 'rebirth', 'thunderspy', 'brainstorm']) {
      const dir = path.join(REPO, OVERRIDE_ROOT, ds, 'overrides');
      for (const file of listTs(dir)) {
        const src = readFileSync(file, 'utf8');
        const m = /"?effects"?\s*:\s*\{/.exec(src);
        if (!m) continue;
        // The slot names are the keys one level in; a shallow scan is enough because these
        // files are generated-shaped JSON literals with one `effects` object each.
        const body = src.slice(m.index + m[0].length);
        for (const km of body.matchAll(/^\s{4}"([a-zA-Z]+)":/gm)) {
          (bySlot[km[1]] ??= { homecoming: 0, rebirth: 0, thunderspy: 0, brainstorm: 0 })[ds] += 1;
        }
      }
    }
    expect(bySlot).toEqual({
      rechargeDebuff: { homecoming: 23, rebirth: 0, thunderspy: 0, brainstorm: 0 },
      buffDuration: { homecoming: 21, rebirth: 0, thunderspy: 0, brainstorm: 0 },
      stealth: { homecoming: 6, rebirth: 0, thunderspy: 0, brainstorm: 0 },
      taunt: { homecoming: 2, rebirth: 0, thunderspy: 0, brainstorm: 0 },
      stun: { homecoming: 1, rebirth: 0, thunderspy: 0, brainstorm: 0 },
      movement: { homecoming: 1, rebirth: 0, thunderspy: 0, brainstorm: 0 },
      durations: { homecoming: 1, rebirth: 0, thunderspy: 0, brainstorm: 0 },
      effectDuration: { homecoming: 1, rebirth: 0, thunderspy: 0, brainstorm: 0 },
    });
    // And the census really is blind to them, which is the claim this guard exists to make.
    for (const slot of Object.keys(bySlot)) expect(row(slot).own, slot).toBe(0);
  });
});
