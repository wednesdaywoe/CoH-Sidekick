#!/usr/bin/env node
/**
 * The atom tuple's field order → `pipeline/_shared/atom-tuple-fields.json`.
 *
 * `ATOM_TUPLE_FIELDS` in `scripts/_atomic-effect.ts` is the ONE source of truth for which
 * position in an encoded atom tuple means what. `emit-contract.cjs` ships it verbatim as
 * `contract/schema-version.json`'s `atomTupleFields`, and `coh_data`'s `atom_wire.rs` carries
 * a hand-mirrored copy that the loader compares against that section — so a drift between the
 * two is a load error rather than a field silently read one position over.
 *
 * WHY THIS IS ITS OWN SCRIPT. `emit-pipeline-json.cjs` wrote this file until 2026-09-25, as the
 * last thing it did that was not a straight copy. That script existed to `require()` the
 * TypeScript oracle out of `src/data`, and when the last oracle read went there was no reason to
 * keep a general-purpose dumper alive for one constant. This is a converter like any other: one
 * input, one `pipeline/` output, named in `regen-all.cjs`.
 *
 * It is SHARED, not per-dataset — the tuple schema is one fact about the wire format, the same
 * for every fork — so it takes no `--dataset` and writes under `pipeline/_shared/`.
 *
 * The one `require` here is `scripts/_atomic-effect.ts`, which is NOT an oracle read: that module
 * lives in `scripts/`, beside this one. It is pipeline code that happens to be TypeScript.
 *
 * Usage: node scripts/convert-atom-tuple-fields.cjs
 */

require('tsx/cjs');
const fs = require('fs');
const path = require('path');
const { REPO_ROOT } = require('./_dataset-paths.cjs');
const { ATOM_TUPLE_FIELDS } = require('./_atomic-effect.ts');

const OUT_PATH = path.join(REPO_ROOT, 'pipeline', '_shared', 'atom-tuple-fields.json');

function main() {
  if (!Array.isArray(ATOM_TUPLE_FIELDS) || ATOM_TUPLE_FIELDS.length === 0) {
    throw new Error('convert-atom-tuple-fields: _atomic-effect.ts exported no ATOM_TUPLE_FIELDS');
  }
  // The key is the export name the emitter destructures, and no whitespace, both the
  // `pipeline/` convention. See `pipelinePath` in `_dataset-paths.cjs`.
  fs.mkdirSync(path.dirname(OUT_PATH), { recursive: true });
  fs.writeFileSync(OUT_PATH, JSON.stringify({ ATOM_TUPLE_FIELDS }));
  console.log(`Wrote ${OUT_PATH} (${ATOM_TUPLE_FIELDS.length} fields)`);
}

if (require.main === module) main();
