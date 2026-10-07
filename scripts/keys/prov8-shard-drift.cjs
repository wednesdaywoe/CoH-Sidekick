#!/usr/bin/env node
/**
 * prov8-shard-drift.cjs — has the game moved since we exported it?
 *
 * PROV-8's key. Every gate in this repo consumes `exported_powers/`, so all of
 * them are downstream of the one question none of them asks: is that tree still
 * a copy of the shard it was read from. `export-staleness` compares the export
 * to the EXPORTER, `export-provenance` compares a dataset's surfaces to EACH
 * OTHER, `export-contents` compares the bytes to their own digest. Three checks,
 * one blind spot, and on 2026-09-20 a user found it for us: Homecoming had
 * pushed i28p4 RC3 to Brainstorm, Seeker Drones had gained Ranged AoE Damage on
 * three archetypes, and the first thing that noticed was a bug report.
 *
 * The check itself is cheap, because `_export_manifest.json` already records the
 * `sha256` of every `.pigg` it read and the `assets_dir` it read them from. So
 * this hashes what is on disk now and compares. It cannot MEASURE in CI — the
 * client is installed on a person's machine, not the runner — but that is an
 * argument for skipping there, not for running nowhere, and for two days it was
 * read as the latter: this file lived in `scripts/keys/`, was named by nothing
 * but a comment in this file's own header, and ran only when somebody remembered it.
 *
 * PROV-8 IS NOW CLOSED. `npm run audit:shard-drift` names it, and
 * `scripts/regen-all.cjs` runs it in the preflight beside
 * `tools/export-integrity.py`, which is always executed on a machine that has
 * the client. A dataset whose install is absent still reports SKIPPED and still
 * exits 0, so CI is unaffected (closed 2026-09-26).
 *
 *   node scripts/keys/prov8-shard-drift.cjs           # report every dataset
 *   node scripts/keys/prov8-shard-drift.cjs --gate    # exit 1 if any moved
 *   npm run audit:shard-drift                         # the same, named
 *
 * BREAKS THE CLAIM: any dataset reporting MOVED. That says the committed export
 * describes a game build nobody is playing any more, and the planner is costing
 * numbers the shard has already changed. Re-export that dataset and mirror it.
 *
 * A dataset whose `assets_dir` is not present reports SKIPPED, not OK. The
 * absence of the client is the absence of the measurement, and calling that a
 * pass is how a check comes to mean nothing on the machine that runs it most.
 */

'use strict';

const fs = require('fs');
const path = require('path');
const crypto = require('crypto');

const ROOT = path.join(__dirname, '..', '..');
const gate = process.argv.includes('--gate');

/** Every dataset's manifest: the flat Homecoming tree plus the namespaced forks. */
function manifests() {
  const base = path.join(ROOT, 'exported_powers');
  const found = [];
  const flat = path.join(base, '_export_manifest.json');
  if (fs.existsSync(flat)) found.push({ dataset: 'homecoming', file: flat });
  for (const entry of fs.readdirSync(base, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const file = path.join(base, entry.name, '_export_manifest.json');
    if (fs.existsSync(file)) found.push({ dataset: entry.name, file });
  }
  return found;
}

let moved = 0;
let skipped = 0;
let ok = 0;

for (const { dataset, file } of manifests()) {
  const source = JSON.parse(fs.readFileSync(file, 'utf-8')).source ?? {};
  const dir = source.assets_dir;
  if (!dir || !fs.existsSync(dir)) {
    console.log(`SKIPPED  ${dataset} — ${dir ? `no ${dir}` : 'manifest names no assets_dir'}`);
    skipped += 1;
    continue;
  }

  const drifted = [];
  for (const pigg of source.sources ?? []) {
    const p = path.join(dir, pigg.name);
    if (!fs.existsSync(p)) {
      drifted.push(`${pigg.name}: gone`);
      continue;
    }
    const now = crypto.createHash('sha256').update(fs.readFileSync(p)).digest('hex');
    if (now !== pigg.sha256) {
      drifted.push(`${pigg.name}: ${pigg.sha256.slice(0, 12)} -> ${now.slice(0, 12)}`);
    }
  }

  if (drifted.length) {
    console.log(`MOVED    ${dataset} (${source.shard ?? '?'}) — ${drifted.length} archive(s) differ`);
    for (const d of drifted) console.log(`           ${d}`);
    moved += 1;
  } else {
    console.log(`ok       ${dataset} (${source.shard ?? '?'}) — ${(source.sources ?? []).length} archives match`);
    ok += 1;
  }
}

console.log(`\n${ok} current, ${moved} moved, ${skipped} unmeasured`);
if (gate && moved > 0) process.exit(1);
