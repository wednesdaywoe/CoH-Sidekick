/**
 * A4 — parallel-converter twin diff (data-integrity audit, GAME-DATA-PRINCIPLES §5).
 *
 * The same underlying game power often ships through TWO converter paths: a powerset
 * copy via convert-powerset (Blaster Dark Blast's Moonbeam) and a pool/epic copy via
 * convert-pool-powers / convert-epic-pools (epic Moonbeam). Each path is the other's
 * free oracle: "same data shape, two code paths, one answer each." Epic Soul Drain
 * shipping FLAT (perTarget lost) and six epic snipes shipping with NO damage were both
 * this class — invisible from inside either converter, obvious the moment you diff them.
 *
 * Comparison is SHAPE-level, not value-level: the set of (effectType|subType) families
 * present in each side's decoded atoms, plus whether a family carries perTarget. Scales
 * and tables legitimately differ across paths (AT modifier tables), so values are noise
 * here; a whole missing family (epic side has no Damage; pool side lost perTarget on
 * ToHit) is the dead-snipe / flat-Soul-Drain signal.
 *
 * READ DIVERGENCES AS FINDINGS, NOT FAILURES — same-name powers can be genuinely
 * different powers. Each hit needs a human eyeball; the report ranks the alarming
 * classes first (missing Damage > missing perTarget > other family gaps).
 *
 * Usage:
 *   node scripts/audit-converter-twins.cjs [--dataset homecoming] [--json out.json]
 *   node scripts/audit-converter-twins.cjs --gate             # regen/CI ratchet vs baseline
 *   node scripts/audit-converter-twins.cjs --write-baseline   # refresh after adjudicating
 */

require('tsx/cjs');
const path = require('path');
const fs = require('fs');
const { sweepDataset } = require('./planb-shadow-sweep.cjs');
const { decodeAtoms } = require('./_atomic-effect.ts');

const argv = process.argv.slice(2);
const argVal = (f) => { const i = argv.indexOf(f); return i >= 0 ? argv[i + 1] : undefined; };
const JSON_OUT = argVal('--json');
// --gate: ratchet mode for regen/CI. Divergences are findings needing a human eyeball
// (see header), so the gate cannot fail on their existence — it fails only on a finding
// NOT in the committed baseline (scripts/audit-twins-baseline.json), i.e. NEW converter
// drift. After adjudicating a new finding (real bug fixed, or genuinely-different power
// accepted), refresh the baseline with --write-baseline.
const GATE = argv.includes('--gate');
const WRITE_BASELINE = argv.includes('--write-baseline');
const BASELINE_PATH = path.join(__dirname, 'audit-twins-baseline.json');
const DATASETS = (() => {
  const picked = argv.flatMap((a, i) => (a === '--dataset' && argv[i + 1] ? [argv[i + 1]] : []));
  return picked.length ? picked : require('./_dataset-paths.cjs').ALL_DATASETS;
})();

/** Which converter produced this module, from its generated-tree location. */
function sourceOf(rel) {
  if (rel.includes(`${path.sep}powersets${path.sep}`)) return 'powerset';
  if (rel.endsWith('power-pools-raw.json')) return 'pool';
  if (rel.endsWith('epic-pools-raw.json')) return 'epic';
  if (rel.endsWith('incarnate-effects.json')) return 'incarnate';
  return 'other';
}

/**
 * Shape signature, split by the `gated` verdict. Only BASE families are compared for
 * findings: gated conditionals (the Fiery Embrace rider block, stance variants, PvP
 * copies) legitimately differ between a powerset copy and its pool/epic twin, because
 * the gate's precondition (e.g. owning Fiery Aura) differs by context. Gated diffs are
 * counted informationally, never as findings.
 *
 * `baseProbability === 0` counts as gated too: HC's chance-0 gating carries riders
 * (the FE fire tick on every FA-adjacent melee attack) as present-but-inert templates
 * whose chance a mechanic flips at runtime — they are conditionals wearing a
 * probability field instead of a requires expression.
 */
function signature(power) {
  const atoms = decodeAtoms(power.atoms || []);
  const base = new Set();
  const gated = new Set();
  for (const a of atoms) {
    // §3 strength meta-template: aspect=Str + scale=0 rows are the engine's
    // strength-DEFINITION bookkeeping, not effects — never part of a power's shape.
    if (a.aspect === 'Str' && a.scale === 0) continue;
    const fam = `${a.effectType}|${a.subType || ''}`;
    const target = a.gated || a.baseProbability === 0 ? gated : base;
    target.add(fam);
    if (a.perTarget != null) target.add(`${fam}|PT`);
  }
  return { base, gated };
}

const lastSeg = (s) => (s || '').split('.').pop().toLowerCase().replace(/[^a-z0-9]+/g, '_');

// A twin pair is keyed on the record name AND the display name, because neither is an
// identity on its own. The record name is not stable across a revamp — HC reuses
// `Pool.Flight.Combat_Flight` for Hover and `Epic.Defender_Ice_Mastery.Ice_Slick` for
// Build Up, so an ident-only key joins unrelated powers. The display name is not unique —
// two powerset records both answer to `moonbeam`. Requiring both agree is what makes the
// twin an oracle rather than a name collision.
//
// This is not free: it also refuses a genuine twin the game LABELS differently in its
// epic pool (Earth's Embrace shipping as "Embrace of the Earth"). A refused pair produces
// no finding, which is coverage lost silently — the failure mode this tree distrusts most.
// So every ident key that has copies in two sources but no partition spanning them is
// counted and printed as UNPAIRED. It is informational, never a finding: the number is
// the audit stating what it declined to compare, so a real twin cannot hide in the gap.
const twinKeyOf = (power) => `${lastSeg(power.internalName)}::${lastSeg(power.name)}`;

let report = {};
for (const dataset of DATASETS) {
  // twinKey ("ident::display") -> source -> { copies, count, atomless, examples }
  const index = new Map();
  // ident -> Set(source), to spot pairs the display split refused (see UNPAIRED above).
  const srcsByIdent = new Map();
  let total = 0;
  sweepDataset(dataset, (power, rel) => {
    const src = sourceOf(rel);
    if (src === 'other') return;
    total += 1;
    const ident = lastSeg(power.internalName);
    const key = twinKeyOf(power);
    if (!ident) return;
    if (!srcsByIdent.has(ident)) srcsByIdent.set(ident, new Set());
    srcsByIdent.get(ident).add(src);
    if (!index.has(key)) index.set(key, new Map());
    const perSrc = index.get(key);
    if (!perSrc.has(src)) perSrc.set(src, { copies: [], count: 0, atomless: 0, examples: [] });
    const slot = perSrc.get(src);
    slot.count += 1;
    if (!Array.isArray(power.atoms) || power.atoms.length === 0) slot.atomless += 1;
    slot.copies.push(signature(power));
    if (slot.examples.length < 3) slot.examples.push(power.name);
  });

  // Reference "must-have" base families for a source = the INTERSECTION of its copies'
  // base sets (a family EVERY powerset copy carries). Union-referencing inflates
  // "missing" whenever a last-segment key collides across genuinely different powers
  // (two different Power Sinks), because the union carries the superset's families.
  const mustHave = (slot) =>
    slot.copies.length === 0
      ? new Set()
      : slot.copies.map((s) => s.base).reduce((acc, s) => new Set([...acc].filter((f) => s.has(f))));
  const anyHave = (slot) => new Set(slot.copies.flatMap((s) => [...s.base]));
  const gatedAny = (slot) => new Set(slot.copies.flatMap((s) => [...s.gated]));

  const findings = [];
  let twinKeys = 0;
  const pairedIdents = new Set();
  for (const [key, perSrc] of index) {
    const srcs = [...perSrc.keys()].filter((s) => s !== 'incarnate'); // incarnates emit no atoms yet (known)
    if (srcs.length < 2) continue;
    twinKeys += 1;
    pairedIdents.add(key.split('::')[0]);
    const [ident, disp] = key.split('::');
    // powerset side is the reference where present; otherwise compare pairwise.
    const refName = perSrc.has('powerset') ? 'powerset' : srcs[0];
    const ref = perSrc.get(refName);
    const refMust = mustHave(ref);
    const refAny = anyHave(ref);
    for (const src of srcs) {
      if (src === refName) continue;
      const cand = perSrc.get(src);
      if (cand.atomless > 0 && refAny.size > 0) {
        findings.push({ key: ident, disp, cls: 'ATOMLESS', src, detail: `${cand.atomless}/${cand.count} copies have no atoms`, examples: cand.examples });
        continue;
      }
      const candAll = anyHave(cand);
      // missing: a family EVERY reference copy has, that NO candidate copy has.
      const missing = [...refMust].filter((f) => !candAll.has(f));
      // extra: a family the candidate has that no reference copy has at all.
      const candMust = mustHave(cand);
      const extra = [...candMust].filter((f) => !refAny.has(f));
      const gatedDelta = [...gatedAny(ref)].filter((f) => !gatedAny(cand).has(f)).length;
      if (missing.length || extra.length) {
        const sev = missing.some((f) => f.startsWith('Damage|')) ? 'MISSING_DAMAGE'
          : missing.some((f) => f.endsWith('|PT')) ? 'MISSING_PERTARGET'
          : missing.length ? 'FAMILY_GAP' : 'FAMILY_EXTRA';
        findings.push({ key: ident, disp, cls: sev, src, missing, extra, gatedDelta, examples: cand.examples, refExamples: ref.examples });
      }
    }
  }

  const order = { ATOMLESS: 0, MISSING_DAMAGE: 1, MISSING_PERTARGET: 2, FAMILY_GAP: 3, FAMILY_EXTRA: 4 };
  findings.sort((a, b) => order[a.cls] - order[b.cls] || a.key.localeCompare(b.key));

  // Idents with copies in 2+ sources that no single display partition spans: the pairs
  // the display split declined. Printed, never a finding — see twinKeyOf.
  const unpaired = [...srcsByIdent]
    .filter(([ident, srcs]) => srcs.size >= 2 && !pairedIdents.has(ident))
    .map(([ident]) => ident)
    .sort();

  console.log(`\n=== ${dataset}: ${total} powers swept, ${twinKeys} twin keys compared, ${findings.length} shape divergences ===`);
  console.log(`   ${unpaired.length} ident(s) unpaired by the display split (not findings): ${unpaired.slice(0, 12).join(', ')}${unpaired.length > 12 ? ', …' : ''}`);
  const byCls = {};
  for (const f of findings) byCls[f.cls] = (byCls[f.cls] || 0) + 1;
  console.log('  ', JSON.stringify(byCls));
  for (const f of findings.slice(0, 40)) {
    const detail = f.detail || `missing [${(f.missing || []).join(', ')}]${f.extra && f.extra.length ? ` extra [${f.extra.join(', ')}]` : ''}`;
    const label = f.disp && f.disp !== f.key ? `${f.key} (${f.disp})` : f.key;
    console.log(`  ${f.cls.padEnd(18)} ${label.padEnd(28)} on ${f.src}: ${detail}`);
    const pair = f.refExamples ? ` vs ${f.refExamples[0]}` : '';
    console.log(`    ${' '.repeat(18)} ${f.examples[0]}${pair}`);
  }
  if (findings.length > 40) console.log(`  ... ${findings.length - 40} more (use --json)`);
  report[dataset] = { total, twinKeys, byClass: byCls, unpaired, findings };
}

if (JSON_OUT) {
  fs.writeFileSync(JSON_OUT, JSON.stringify(report, null, 1));
  console.log(`\nfull report -> ${JSON_OUT}`);
}

// A finding's ratchet identity: precise enough that a divergence CHANGING SHAPE
// (a FAMILY_GAP gaining a family, a class flip to MISSING_DAMAGE) counts as new.
const findingIdentity = (dataset, f) => [
  dataset, f.cls, `${f.key}::${f.disp}`, f.src,
  (f.missing || []).join('+'), (f.extra || []).join('+'),
].join('|');

if (WRITE_BASELINE) {
  const identities = Object.entries(report)
    .flatMap(([ds, r]) => r.findings.map((f) => findingIdentity(ds, f)))
    .sort();
  fs.writeFileSync(BASELINE_PATH, JSON.stringify({
    _note: 'Frozen twin-divergence baseline for audit-converter-twins.cjs --gate. Each entry is a standing shape divergence (findings, not failures — same-name powers can be genuinely different). The gate fails only on findings absent from this list. Refresh with --write-baseline after adjudicating.',
    findings: identities,
  }, null, 1) + '\n');
  console.log(`baseline (${identities.length} findings) -> ${path.relative(process.cwd(), BASELINE_PATH)}`);
}

if (GATE && !WRITE_BASELINE) {
  const baseline = new Set(JSON.parse(fs.readFileSync(BASELINE_PATH, 'utf8')).findings);
  const fresh = [];
  const seen = new Set();
  for (const [ds, r] of Object.entries(report)) {
    for (const f of r.findings) {
      const id = findingIdentity(ds, f);
      seen.add(id);
      if (!baseline.has(id)) fresh.push(id);
    }
  }
  // Stale = baselined divergence no longer observed (an improvement). Reported, not
  // failed — but prune with --write-baseline so the baseline tracks reality.
  const sweptPrefix = new RegExp(`^(${DATASETS.join('|')})\\|`);
  const stale = [...baseline].filter((id) => sweptPrefix.test(id) && !seen.has(id));
  for (const s of stale) console.log(`  stale baseline entry (divergence resolved): ${s}`);
  if (fresh.length) {
    console.error(`GATE FAIL — ${fresh.length} NEW twin divergence(s) not in the baseline:`);
    for (const id of fresh) console.error(`  ${id}`);
    console.error('Adjudicate (bug vs genuinely-different power), then refresh with --write-baseline.');
    process.exit(1);
  }
  console.log(`GATE PASS — twin divergences ⊆ baseline (${stale.length} stale baseline entr${stale.length === 1 ? 'y' : 'ies'}).`);
}
