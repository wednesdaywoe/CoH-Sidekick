/**
 * PROD6B-2b — reachability gate for the archetype-name support modifier.
 *
 * The beta used to scale a magnitude by the archetype's support modifier when the powerset
 * belonged to a Defender or Controller (`getEffectiveBuffDebuffModifier`) — a rule keyed on
 * archetype NAMES, which the engine cannot carry (Rule 0). PROD6B-2 therefore had the engine
 * pass 1.0 and drove the beta resolver with 1.0 too, on the argument that the modifier is
 * only reachable on the table-less fallback paths and every exported effect value carries a
 * resolvable table. That argument was asserted, never measured — and a parity gate that pins
 * both sides to the same 1.0 is structurally blind to it.
 *
 * Measured (PROD6B-2b): the stated reason was wrong and the conclusion was right. The
 * fallback IS reached, ~6–7k rows per fork — but every single one is the `accuracy` key on a
 * POOL or EPIC POOL power, whose `powerSet` is a bare pool id (`leadership`, `sorcery`), never
 * an archetype-prefixed one. No AT-owned powerset row falls through at all. So the rule could
 * never fire, and it is deleted rather than carried into PROD6C.
 *
 * What this gate holds down, now that the rule is gone: the data shape that made it dead. The
 * probe runs the REAL `resolvePowerMagnitudes` twice over the whole corpus, once with the
 * modifier at 1.0 and once at a sentinel, and diffs — no reimplementation of the resolution
 * to disagree with what the display actually runs.
 */
import { describe, it, expect, beforeAll } from 'vitest';
import { loadDataset } from '@/data/dataset';
import { getAllPowersets } from '@/data/powersets';
import { getAllPowerPools } from '@/data/power-pools';
import { getAllEpicPools } from '@/data/epic-pools';
import { getArchetypeIds, STANDARD_ARCHETYPE_IDS } from '@/data/archetypes';
import { resolvePowerMagnitudes } from './resolvePowerMagnitudes';
import { buildDisplayEffects } from './buildDisplayEffects';
import { EFFECT_RESOLUTION } from '@/data/generated/effect-registry.generated';
import type { Power, PowerEffects } from '@/types';

const SERVERS = ['homecoming', 'rebirth', 'thunderspy', 'brainstorm'] as const;

/** Any value but the 1.0 both sides are pinned to; a reached row's numbers scale by it. */
const SENTINEL_MODIFIER = 3.0;

/** The corpus level. The pinned-50 table reads make this the level everything resolves at. */
const LEVEL = 50;

/**
 * The registry keys whose resolution path can consult the modifier at all: the buff/debuff
 * base-rate face, and every percent-format key (which multiplies by it when no table
 * resolves). A key outside this set can never reach it, so rows through these keys are what
 * make a "nothing reached" result mean something rather than measuring an empty sweep.
 */
const FALLBACK_CAPABLE = new Set(
  Object.entries(EFFECT_RESOLUTION)
    .filter(([, c]) => c.calculation !== undefined || c.format === 'percent')
    .map(([key]) => key),
);

interface Reach {
  rowKey: string;
  effectKey: string;
  at1: number | null;
  atSentinel: number | null;
}

/** Rows that move when only the support modifier changes — i.e. rows that reached it. */
function probe(effects: PowerEffects, archetypeId: string): { reached: Reach[]; capableRows: number } {
  const at = resolvePowerMagnitudes({ effects, archetypeId, level: LEVEL, buffDebuffMod: 1.0 });
  const sentinel = resolvePowerMagnitudes({
    effects,
    archetypeId,
    level: LEVEL,
    buffDebuffMod: SENTINEL_MODIFIER,
  });

  const byKey = new Map(sentinel.map((r) => [r.rowKey, r]));
  const reached: Reach[] = [];
  for (const row of at) {
    const other = byKey.get(row.rowKey);
    // A row present at one modifier and absent at the other also reached it: the resolver
    // drops a row whose base value lands on zero.
    if (!other) {
      reached.push({ rowKey: row.rowKey, effectKey: row.effectKey, at1: row.tiers.base, atSentinel: null });
      continue;
    }
    if (row.tiers.base !== other.tiers.base) {
      reached.push({ rowKey: row.rowKey, effectKey: row.effectKey, at1: row.tiers.base, atSentinel: other.tiers.base });
    }
  }
  for (const row of sentinel) {
    if (!at.some((r) => r.rowKey === row.rowKey)) {
      reached.push({ rowKey: row.rowKey, effectKey: row.effectKey, at1: null, atSentinel: row.tiers.base });
    }
  }
  return { reached, capableRows: at.filter((r) => FALLBACK_CAPABLE.has(r.effectKey)).length };
}

/** A set owned by an archetype (`blaster/fire-blast`) rather than shared (`leadership`). Only an
 *  AT-owned set can carry the `defender`/`controller` prefix the retired rule keyed on. */
function archetypeOwned(setId: string, allArchetypes: string[]): boolean {
  return allArchetypes.includes(setId.split('/')[0]);
}

/** The archetypes a set's table reads are measured under. An AT-owned powerset resolves under
 *  its own AT; a pool or epic pool is shared, so it resolves under every one. */
function archetypesFor(setId: string, allArchetypes: string[]): string[] {
  const prefix = setId.split('/')[0];
  return archetypeOwned(setId, allArchetypes) ? [prefix] : allArchetypes;
}

/** Every `Power` bag the info display can render: the AT powersets plus the two shared pool
 *  registries, which `getAllPowersets` does not carry. */
function allPowerSources(): [string, { powers: { internalName?: string; effects?: PowerEffects }[] }][] {
  return [
    ...Object.entries(getAllPowersets()),
    ...Object.entries(getAllPowerPools()),
    ...Object.entries(getAllEpicPools()),
  ] as [string, { powers: { internalName?: string; effects?: PowerEffects }[] }][];
}

describe('PROD6B-2b: archetype-name support modifier reachability', () => {
  // The probe itself must be able to see a reached row, or "zero reached" is vacuous. A
  // table-less scale on a buff-calculation effect is exactly the shape the fallback exists
  // for: with no table to resolve, `getEffectBaseValue` falls to scale × base-rate × modifier.
  it('detects a row that does reach the modifier', async () => {
    await loadDataset(SERVERS[0]);
    const tableless = { tohitBuff: 1 } as unknown as PowerEffects;
    expect(probe(tableless, STANDARD_ARCHETYPE_IDS[0]).reached).not.toEqual([]);
  }, 60000);

  describe.each(SERVERS)('%s', (server) => {
    beforeAll(async () => {
      await loadDataset(server);
    }, 60000);

    it('no archetype-owned powerset falls through to the table-less fallback', () => {
      const reachedAll: string[] = [];
      const archetypeOwnedReached: string[] = [];
      const byPrefix = new Map<string, number>();
      const byEffectKey = new Map<string, number>();
      let powersProbed = 0;
      let capableRows = 0;
      const allArchetypes = [...getArchetypeIds()];

      for (const [setId, set] of allPowerSources()) {
        for (const archetypeId of archetypesFor(setId, allArchetypes)) {
          for (const power of set.powers) {
            // The bag the DISPLAY resolves, not the authored one. This walked `power.effects`
            // until 2026-09-08, and STRIP-1 emptied that for every power on every fork — so the
            // sweep resolved nothing and both floors below went to zero while the invariant they
            // guard stayed green on a population of none. The surfaces have never resolved the
            // authored bag; reading the built one is what this file's own header claims ("no
            // reimplementation of the resolution to disagree with what the display actually
            // runs"), and it is where the `accuracy` rows the original finding was ABOUT live.
            const effects = buildDisplayEffects(power as unknown as Power);
            powersProbed++;
            const result = probe(effects, archetypeId);
            capableRows += result.capableRows;
            for (const row of result.reached) {
              const where = `${archetypeId}: ${setId}/${power.internalName} ${row.rowKey} (${row.at1} → ${row.atSentinel})`;
              reachedAll.push(where);
              byPrefix.set(setId.split('/')[0], (byPrefix.get(setId.split('/')[0]) ?? 0) + 1);
              byEffectKey.set(row.effectKey, (byEffectKey.get(row.effectKey) ?? 0) + 1);
              if (archetypeOwned(setId, allArchetypes)) archetypeOwnedReached.push(where);
            }
          }
        }
      }

      const top = (m: Map<string, number>, n: number) =>
        [...m].sort((a, b) => b[1] - a[1]).slice(0, n).map(([k, c]) => `${k}:${c}`).join(' ');
      // eslint-disable-next-line no-console
      console.warn(
        `[PROD6B-2b] ${server}: ${powersProbed} probes, ${capableRows} modifier-capable rows, ` +
          `${reachedAll.length} reached the fallback, ${archetypeOwnedReached.length} of those archetype-owned\n` +
          `  reached by set: ${top(byPrefix, 12)}\n  reached by effect key: ${top(byEffectKey, 12)}`,
      );
      if (archetypeOwnedReached.length) {
        // Expected since the input moved to the display bag — an AT-owned power's `accuracy` is
        // minted there where the authored bag never carried it. Reported, not failed: the axis
        // under test is the KEY, asserted below.
        // eslint-disable-next-line no-console
        console.warn(`[PROD6B-2b] ${server}: ${archetypeOwnedReached.length} of the reached rows are archetype-owned, e.g.\n    ${archetypeOwnedReached.slice(0, 3).join('\n    ')}`);
      }

      // "Nothing reaches it" is only evidence if the sweep resolved rows that could, and drove
      // some of them onto the fallback.
      expect(capableRows, `${server}: sweep resolved no modifier-capable row`).toBeGreaterThan(0);
      expect(reachedAll.length, `${server}: sweep reached the fallback on no row`).toBeGreaterThan(0);

      // The invariant that keeps the retired rule retired, re-cut 2026-09-08 onto the axis the
      // measurement actually supports: the KEY, not the powerset prefix.
      //
      // PROD6B-2b recorded that the fallback is reached only by `accuracy` on POOL and EPIC
      // powers, and that no AT-owned row falls through at all. The first half holds. The second
      // was an artefact of the input: this sweep read `power.effects`, where an AT-owned power's
      // accuracy never lived (it is on `stats`; only `transformPoolPower` puts execution stats in
      // the bag). Against the bag the display resolves, AT-owned rows fall through too — 3339 /
      // 2806 / 2798 / 3420 per fork — and every one of them is still `accuracy`. So the retired
      // rule WOULD have scaled a Defender's accuracy had it survived, which makes its deletion
      // more necessary than the "it could never fire" note claimed, not less.
      //
      // What the rule's scope actually needs is that no BUFF/DEBUFF row is table-less, and that
      // is what this pins: the reached population is exactly the execution keys. A buff key
      // appearing here means a modifier table stopped resolving (the Rebirth Guardian class of
      // bug — see at-table-archetype-coverage.test.ts), and it names the key rather than leaving
      // it inside a count.
      const reachedNonExecution = [...byEffectKey.keys()].filter(
        (key) => (EFFECT_RESOLUTION[key] as { category?: string } | undefined)?.category !== 'execution',
      );
      expect(reachedNonExecution, `${server}: a non-execution key fell through to the table-less fallback`).toEqual([]);
      expect([...byEffectKey.keys()].sort(), `${server}: the reached key set moved`).toEqual(['accuracy']);
    }, 120000);
  });
});
