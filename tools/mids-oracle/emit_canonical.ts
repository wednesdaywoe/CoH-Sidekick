/**
 * DSH5 — export-side canonicalizer. Walks the committed HC parser export
 * (`exported_powers/`, minus rebirth/thunderspy/tables), runs the **tested DSH4
 * bridge** (`ingestExportPower` from src/data/core/atomic-effect.ts), and emits one
 * canonical JSONL line per power. This is deliberately the ONLY place the export
 * side is turned into `(effectType, subType, …)` — the Python harness never re-ports
 * `bridgeAttrib`, so the app's schema and the oracle diff can never drift.
 *
 * Redirect shells (`effects: []` + `redirect[]`, e.g. Arachnos Burst) are resolved
 * by inlining the unconditional branch's effects (fallback: first target) and
 * tagged `redirect: <target>` so the harness scores them in a separate advisory
 * bucket — branch selection is ambiguous vs Mids' single inline, so they never gate.
 *
 * "Unconditional" is `isAlwaysCondition` from `scripts/_gate-tokens.cjs`, the same
 * helper the converters use, rather than a local test. This file read the branch as a
 * STRING equal to 'Always' until PROV-3 (2026-09-13). COND-8 had since made every gate
 * field a token array on the wire and the sentinel string went with it — all 1,097
 * redirect rows in the export are arrays now, so `.trim()` threw and the emitter could
 * not run at all. Both spellings of unconditional (`[]`, `['1']`) live in that helper;
 * a second copy here is how the first one rotted.
 *
 * Run:  npx tsx tools/mids-oracle/emit_canonical.ts <out.jsonl>
 * (Invoked automatically by diff_harness.py; run standalone to refresh the cache.)
 */
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
import { ingestExportPower } from '../../src/data/core/atomic-effect';

const { isAlwaysCondition } = createRequire(import.meta.url)('../../scripts/_gate-tokens.cjs');

const REPO = fileURLToPath(new URL('../..', import.meta.url));
const EXPORT_ROOT = path.join(REPO, 'exported_powers');
// The non-Homecoming trees. `brainstorm` was missing until PROV-3 (2026-09-13) and it is
// a FORK, not a Homecoming subtree — 1,461 of the 14,852 names it shares with Homecoming
// carry different effects. Because `byName` kept the last writer and `readdirSync` hands
// back filesystem order, whichever tree walked later won the name: 3,232 records came from
// brainstorm, 101 of them differing from their Homecoming twin AND joined to the Mids
// Homecoming oracle. Those 101 were graded against the wrong fork's data.
const SKIP_TOP = new Set(['rebirth', 'thunderspy', 'brainstorm', 'tables']);

interface RawPower {
  full_name?: string;
  effects?: unknown[];
  // A gate expression is a token array (COND-8); `gateTokens` throws on a string
  // rather than re-splitting one, so an un-ported producer surfaces here loudly.
  redirect?: { name?: string; condition_expression?: string[] | null }[];
}

function hcFiles(): { path: string; category: string }[] {
  const out: { path: string; category: string }[] = [];
  const walk = (dir: string, top: string | null) => {
    for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
      if (top === null && SKIP_TOP.has(e.name)) continue;
      const p = path.join(dir, e.name);
      if (e.isDirectory()) walk(p, top ?? e.name);
      else if (e.name.endsWith('.json') && e.name !== 'index.json') out.push({ path: p, category: top ?? e.name });
    }
  };
  walk(EXPORT_ROOT, null);
  return out;
}

// --- pass 1: index every HC power by full_name (lower) so redirects can inline ---
const byName = new Map<string, RawPower>();
const priorPath = new Map<string, string>();
const files = hcFiles();
for (const { path: p } of files) {
  let power: RawPower;
  try {
    power = JSON.parse(fs.readFileSync(p, 'utf-8')) as RawPower;
  } catch {
    // unreadable/partial JSON — skipped, surfaces later as an export-side coverage gap.
    // Only the READ is guarded; the collision check below must not be swallowed by it.
    continue;
  }
  const fn = (power.full_name || '').toLowerCase();
  if (!fn) continue;
  // Last-writer-wins is what let a fork masquerade as Homecoming above, so the map
  // refuses a second claimant instead of picking one. Homecoming alone has no such pair
  // (measured: 0), so this is a tripwire on SKIP_TOP going stale again — the next fork
  // added under `exported_powers/` announces itself here rather than quietly re-deciding
  // 3,000 rows.
  if (byName.has(fn)) {
    throw new Error(`two files claim ${power.full_name}: ${priorPath.get(fn)} and ${p} `
      + '— a non-Homecoming tree is missing from SKIP_TOP (PROV-3)');
  }
  priorPath.set(fn, p);
  byName.set(fn, power);
}

/** Resolve a redirect shell to its inlined target (Always branch, else first). */
function resolveRedirect(power: RawPower): { target: RawPower; targetName: string } | null {
  const redirects = power.redirect ?? [];
  if (!redirects.length) return null;
  const always = redirects.find((r) => isAlwaysCondition(r.condition_expression));
  const chosen = always ?? redirects[0];
  const tgt = byName.get((chosen.name || '').toLowerCase());
  return tgt ? { target: tgt, targetName: chosen.name || '' } : null;
}

// --- pass 2: bridge + emit ------------------------------------------------------
const outPath = process.argv[2];
if (!outPath) {
  console.error('usage: emit_canonical.ts <out.jsonl>');
  process.exit(2);
}

const catByName = new Map<string, string>();
for (const { path: p, category } of files) {
  try {
    const fn = (JSON.parse(fs.readFileSync(p, 'utf-8')).full_name || '').toLowerCase();
    if (fn) catByName.set(fn, category);
  } catch {
    /* already counted above */
  }
}

const lines: string[] = [];
let emitted = 0;
let redirectResolved = 0;
let redirectUnresolved = 0;
for (const [fn, power] of byName) {
  const hasEffects = Array.isArray(power.effects) && power.effects.length > 0;
  let redirectOf: string | null = null;
  let source = power;
  if (!hasEffects && (power.redirect?.length ?? 0) > 0) {
    const r = resolveRedirect(power);
    if (r) {
      source = r.target;
      redirectOf = r.targetName;
      redirectResolved++;
    } else {
      redirectUnresolved++;
      // emit the shell (0 effects) — the harness records it as a redirect coverage gap
    }
  }
  const atoms = ingestExportPower(source as { effects?: never[] });
  lines.push(
    JSON.stringify({
      full_name: power.full_name,
      category: catByName.get(fn) ?? '',
      redirect: redirectOf, // null unless this power's effects were inlined from a target
      effects: atoms.map((a) => ({
        effectType: a.effectType,
        subType: a.subType ?? null,
        pvMode: a.pvMode,
        resistible: a.resistible,
        modifierTable: a.modifierTable,
        aspect: a.aspect,
        attribType: a.attribType,
        scale: a.scale,
        sourceAttrib: a.sourceAttrib ?? null,
      })),
    }),
  );
  emitted++;
}

fs.writeFileSync(outPath, lines.join('\n') + '\n');
console.error(
  `[emit_canonical] ${emitted} HC powers -> ${path.relative(REPO, outPath)} ` +
    `(${redirectResolved} redirect shells inlined, ${redirectUnresolved} unresolved)`,
);
