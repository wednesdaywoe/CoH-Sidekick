/**
 * Plan B, Phase 2 Slice 1 — shadow compare for the ToHit applier.
 *
 * Before migrating `character-totals.ts`'s ToHit blocks off the bag, measure
 * what changes. For every power in every dataset this computes the +ToHit
 * contributions TWICE and diffs them:
 *
 *   BAG   — `effects.tohitBuff` (+ `effects.tohitBuffUnenhanced`), i.e. what the
 *           calc reads today. At most ONE value each, because the projection
 *           assigns those slots (`effects.tohitBuff = makeEffect()`) rather than
 *           accumulating: last write wins.
 *   ATOMS — the power's BASE (ungated) `effectType:'ToHit'` atoms, minus the ones
 *           the bag routes elsewhere, split by `ignoreStrength`. N values.
 *
 * Comparison is the SUMMED scale per (table, enhanceable), at a single target.
 *
 * Not the multiset of individual values, and not a resolved number:
 *   - Multiset is wrong because the bag legitimately RESHAPES several templates
 *     into one value. Soul Drain's two ToHit templates (1.0 flat + 0.2 per foe)
 *     become `{scale: 1.2, perTarget: 0.2}` — one slot, same effect at N=1.
 *     Comparing raw values flags that as a collapse when nothing was lost.
 *   - A resolved number is wrong because the AT modifier table is applied
 *     identically by both sides at runtime; folding it in adds archetype noise
 *     to a question that is purely about ROUTING — which effects reach the total.
 *
 * Summing at N=1 is exactly the invariant that survives the bag's reshaping:
 * `scale` at one target must equal the sum of the atoms behind it. A shortfall
 * means the bag genuinely dropped an effect.
 *
 * READ THE DIVERGENCES AS FINDINGS, NOT FAILURES. Where a power has two ToHit
 * buff atoms, the bag necessarily dropped one — that is the collapse Plan B
 * exists to remove, so the atom side having MORE is the fix, not a bug. Each
 * such power needs verifying against Mids/in-game before the applier migrates
 * (plan §Risks: "that divergence is the fix landing … not auto-accepted").
 * Exit code is 0 regardless; this is a triage report.
 *
 * Usage:
 *   node scripts/planb-shadow-tohit.cjs                  # all datasets
 *   node scripts/planb-shadow-tohit.cjs --dataset homecoming
 *   node scripts/planb-shadow-tohit.cjs --power "Tactics"
 */

require('tsx/cjs');
const fs = require('fs');
const path = require('path');
const { sweepDataset } = require('./planb-shadow-sweep.cjs');
const { baseAtoms } = require('../src/data/core/atom-query.ts');

const REPO = path.resolve(__dirname, '..');
const argv = process.argv.slice(2);
const argVal = (f) => { const i = argv.indexOf(f); return i >= 0 ? argv[i + 1] : undefined; };
const POWER_FILTER = argVal('--power');
const DATASETS = (() => {
  const picked = argv.flatMap((a, i) => (a === '--dataset' && argv[i + 1] ? [argv[i + 1]] : []));
  return picked.length ? picked : ['homecoming', 'rebirth', 'thunderspy'];
})();

const bucketOf = (c) => `${(c.table || '').toLowerCase()}|${c.enhanceable ? 'E' : 'U'}`;

/** Sum scale per (table, enhanceable) bucket → a comparable, order-free shape. */
function summarize(contribs) {
  const m = new Map();
  for (const c of contribs) m.set(bucketOf(c), (m.get(bucketOf(c)) || 0) + c.scale);
  return [...m].sort().map(([k, v]) => `${k}=${v.toFixed(4)}`);
}

/**
 * What the bag now represents for +ToHit. `tohitBuff`/`tohitBuffUnenhanced` are
 * single-valued, but a burst+tail power (Inner Light) carries its shorter-lived
 * instances as `durationVariants[]` on the primary — so summing the primary plus
 * its variants is the bag's complete contribution set, matching the atoms. Before
 * Slice 1's converter fix the variants did not exist and the burst was dropped
 * (the four Inner Light / Inner Umbra divergences this report was built to find).
 */
function bagContributions(effects) {
  const out = [];
  const push = (v, enhanceable) => {
    if (v === undefined) return;
    const scale = typeof v === 'number' ? v : v.scale;
    const table = typeof v === 'number' ? '' : v.table;
    out.push({ scale: Math.abs(scale), table, enhanceable });
    if (v && typeof v === 'object' && Array.isArray(v.durationVariants)) {
      for (const dv of v.durationVariants) out.push({ scale: Math.abs(dv.scale), table, enhanceable });
    }
  };
  push(effects.tohitBuff, true);
  push(effects.tohitBuffUnenhanced, false);
  return out;
}

/**
 * The same effects, read from atoms. Mirrors the projection's ToHit routing
 * (convert-powerset.cjs, COMBAT_MODIFIERS modType==='toHit') so a divergence
 * means a genuine bag collapse rather than a routing disagreement:
 *   aspect Res  -> debuffResistance.tohit  (not a +ToHit buff)
 *   aspect Str  -> specialBuff.tohit       (ToHit STRENGTH, not ToHit itself)
 *   isDebuff    -> tohitDebuff             (a foe debuff; sign/table decides)
 *   ignoreStrength -> tohitBuffUnenhanced  (else tohitBuff)
 */
function atomContributions(power) {
  return baseAtoms(power)
    .filter((a) => a.effectType === 'ToHit')
    .filter((a) => a.aspect !== 'Res' && a.aspect !== 'Str')
    .filter((a) => !(a.scale < 0 || (a.modifierTable || '').toLowerCase().includes('debuff')))
    .map((a) => ({ scale: Math.abs(a.scale), table: a.modifierTable, enhanceable: !a.ignoreStrength }));
}

const stats = { powers: 0, withToHit: 0, agree: 0 };
const findings = [];

function checkPower(dataset, power, genPath) {
  const name = power.name || genPath;
  if (POWER_FILTER && !name.toLowerCase().includes(POWER_FILTER.toLowerCase())) return;
  stats.powers++;
  const bag = bagContributions(power.effects || {});
  const atoms = atomContributions(power);
  if (!bag.length && !atoms.length) return;
  stats.withToHit++;

  const bagKeys = summarize(bag);
  const atomKeys = summarize(atoms);
  if (JSON.stringify(bagKeys) === JSON.stringify(atomKeys)) { stats.agree++; return; }

  const sum = (xs) => xs.reduce((t, c) => t + c.scale, 0);
  findings.push({
    dataset, name,
    kind: sum(atoms) > sum(bag) + 1e-4 ? 'BAG-DROPPED'
        : sum(bag) > sum(atoms) + 1e-4 ? 'ATOMS-MISSING'
        : 'SHAPE-DIFF',
    bag: bagKeys, atoms: atomKeys,
  });
  if (POWER_FILTER) {
    console.log(`\n=== ${name} [${dataset}] ===`);
    console.log('  bag  :', bagKeys);
    console.log('  atoms:', atomKeys);
    for (const a of baseAtoms(power).filter((x) => x.effectType === 'ToHit')) {
      console.log(`    ToHit scale=${a.scale} ${a.aspect}/${a.toWho}/${a.pvMode}` +
        `${a.ignoreStrength ? ' IGN-STR' : ''}${a.resistible ? '' : ' UNRES'} [${a.modifierTable}]`);
    }
  }
}

// Sweeps the dataset's WHOLE generated tree (see planb-shadow-sweep.cjs) — including
// power-pools.ts / epic-pools.ts, which every shadow's hand-rolled sweep used to miss.
function sweep(dataset) {
  sweepDataset(dataset, (power, rel) => checkPower(dataset, power, rel));
}

for (const ds of DATASETS) sweep(ds);

console.log(`\nPlan B Slice 1 — ToHit bag-vs-atoms`);
console.log(`  powers swept:        ${stats.powers}`);
console.log(`  powers with +ToHit:  ${stats.withToHit}`);
console.log(`  agree:               ${stats.agree}`);
console.log(`  diverge:             ${findings.length}`);

const byKind = new Map();
for (const f of findings) byKind.set(f.kind, (byKind.get(f.kind) || 0) + 1);
if (byKind.size) console.log('  by kind: ' + [...byKind].map(([k, n]) => `${k}=${n}`).join('  '));

for (const f of findings.slice(0, 30)) {
  console.log(`\n  [${f.kind}] ${f.name} (${f.dataset})`);
  console.log(`      bag  : ${f.bag.join(', ') || '(none)'}`);
  console.log(`      atoms: ${f.atoms.join(', ') || '(none)'}`);
}
if (findings.length > 30) console.log(`\n  ... and ${findings.length - 30} more`);
console.log('\n(triage report — divergence where atoms carry MORE is the bag collapse Plan B removes;');
console.log(' verify each against Mids/in-game before migrating the applier)');
