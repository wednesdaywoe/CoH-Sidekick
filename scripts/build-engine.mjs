/**
 * PROD1 — regenerate the engine artifacts the beta ships on top of.
 *
 * Replaces the manual spike commands with one repeatable step:
 *   1. Verify the installed `wasm-bindgen` CLI matches the version the rebuild's
 *      Cargo.lock resolved (the CLI and the linked lib MUST be identical or the glue
 *      it emits is incompatible with the .wasm) — fail loud otherwise.
 *   2. Build `coh_wasm` for wasm32 and run `wasm-bindgen` twice: `--target web` into
 *      src/engine/wasm/ (the browser glue engine.ts imports) and `--target nodejs` into
 *      src/engine/wasm-node/ (the artifact the src/engine/ parity gates load). Both, because
 *      a run that refreshed only the browser one left every gate grading a stale engine —
 *      a Rust change could then read as "the engine does nothing" with no failure anywhere.
 *   3. Copy the rebuild's per-dataset contract bundles into public/engine/contract/
 *      as <server>.json.gz (what engine.ts fetches at boot).
 *
 * The rebuild repo is located via COH_REBUILD_DIR, defaulting to the sibling checkout.
 * Every input is verified before use; a missing rebuild, a version skew, or a missing
 * bundle aborts with a specific message rather than emitting a half-built engine.
 */

import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, copyFileSync, readdirSync, readFileSync, renameSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { assertIncludesMatchDepInfo, fingerprintRebuild } from './engine-fingerprint.mjs';
import { fileURLToPath } from 'node:url';

const betaRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const rebuildDir = resolve(process.env.COH_REBUILD_DIR ?? join(betaRoot, '..', 'coh-sidekick-1.0'));

function die(message) {
  console.error(`\n[build-engine] ${message}\n`);
  process.exit(1);
}

function run(command, args, cwd) {
  console.log(`[build-engine] ${command} ${args.join(' ')}`);
  execFileSync(command, args, { cwd, stdio: 'inherit' });
}

// --- 1. locate the rebuild + verify the wasm-bindgen CLI matches its lockfile ---

if (!existsSync(join(rebuildDir, 'crates', 'coh_wasm', 'Cargo.toml'))) {
  die(
    `rebuild repo not found at ${rebuildDir}.\n` +
      `Set COH_REBUILD_DIR to your coh-sidekick-1.0 checkout.`,
  );
}

const lockText = readFileSync(join(rebuildDir, 'Cargo.lock'), 'utf8');
// The resolved version is the block whose name is exactly "wasm-bindgen" (not -macro etc.).
const lockMatch = lockText.match(/name = "wasm-bindgen"\nversion = "([^"]+)"/);
if (!lockMatch) die('could not read the resolved wasm-bindgen version from the rebuild Cargo.lock.');
const requiredCliVersion = lockMatch[1];

let installedCliVersion;
try {
  installedCliVersion = execFileSync('wasm-bindgen', ['--version'], { encoding: 'utf8' }).trim().replace(/^wasm-bindgen\s+/, '');
} catch {
  die(
    `wasm-bindgen CLI not found. Install the pinned version:\n` +
      `  cargo install -f wasm-bindgen-cli --version ${requiredCliVersion}`,
  );
}

if (installedCliVersion !== requiredCliVersion) {
  die(
    `wasm-bindgen CLI ${installedCliVersion} != crate-resolved ${requiredCliVersion}.\n` +
      `The CLI and the linked lib must match. Install the pinned version:\n` +
      `  cargo install -f wasm-bindgen-cli --version ${requiredCliVersion}`,
  );
}
console.log(`[build-engine] wasm-bindgen ${installedCliVersion} matches rebuild lockfile`);

// --- 2. build the wasm + emit the browser glue ---

run('cargo', ['build', '--release', '--target', 'wasm32-unknown-unknown', '-p', 'coh_wasm'], rebuildDir);

const wasmArtifact = join(rebuildDir, 'target', 'wasm32-unknown-unknown', 'release', 'coh_wasm.wasm');
if (!existsSync(wasmArtifact)) die(`cargo did not produce ${wasmArtifact}.`);

// The fingerprint's compile-time include set is a regex over Rust source, because CI verifies
// with node alone and cannot build. Cargo has just written down every file it ACTUALLY read, so
// this is the one moment the authoritative answer is in hand — compare the two here rather than
// let a drifted scan quietly hash the wrong set and certify a stale engine as fresh.
const depInfo = join(rebuildDir, 'target', 'wasm32-unknown-unknown', 'release', 'coh_wasm.d');
try {
  const included = assertIncludesMatchDepInfo(rebuildDir, depInfo);
  console.log(`[build-engine] ${included} compile-time include(s) match cargo's dep-info`);
} catch (error) {
  die(error.message);
}

const wasmOutDir = join(betaRoot, 'src', 'engine', 'wasm');
mkdirSync(wasmOutDir, { recursive: true });
run('wasm-bindgen', ['--target', 'web', '--out-dir', wasmOutDir, '--out-name', 'coh_wasm', wasmArtifact], betaRoot);

const nodeOutDir = join(betaRoot, 'src', 'engine', 'wasm-node');
mkdirSync(nodeOutDir, { recursive: true });
run('wasm-bindgen', ['--target', 'nodejs', '--out-dir', nodeOutDir, '--out-name', 'coh_wasm', wasmArtifact], betaRoot);
// The emitted glue is CommonJS but lands as `.js`, which this ESM package would parse as a
// module — the gates `require` it as `.cjs`. Rename rather than leave both, or the stale one
// keeps being the file that loads.
renameSync(join(nodeOutDir, 'coh_wasm.js'), join(nodeOutDir, 'coh_wasm.cjs'));

// --- 3. copy the per-dataset contract bundles ---

const contractSrc = join(rebuildDir, 'contract');
if (!existsSync(contractSrc)) die(`rebuild contract dir not found at ${contractSrc}.`);

// The datasets are whatever the rebuild exported — derived, never hardcoded.
const datasets = readdirSync(contractSrc).filter(
  (name) => statSync(join(contractSrc, name)).isDirectory() && existsSync(join(contractSrc, name, 'bundle.json.gz')),
);
if (datasets.length === 0) die(`no dataset bundles found under ${contractSrc} (expected <server>/bundle.json.gz).`);

const contractOut = join(betaRoot, 'public', 'engine', 'contract');
mkdirSync(contractOut, { recursive: true });
const bundleVersions = {};
for (const dataset of datasets) {
  const src = join(contractSrc, dataset, 'bundle.json.gz');
  copyFileSync(src, join(contractOut, `${dataset}.json.gz`));
  // The bundle filename is NOT content-hashed (`homecoming.json.gz` keeps its name across
  // builds), so nothing stops an HTTP cache from handing a stale bundle to a newer .wasm —
  // and a bundle a schema change has moved past fails the engine's load outright ("missing
  // field `absorbCap`", reported 2026-07-28). The service worker precaches both halves with
  // revisions so they move together, but that only covers SW-CONTROLLED loads. This stamp is
  // what covers the rest: engine.ts appends it as `?v=`, so a changed bundle is a changed URL.
  bundleVersions[dataset] = createHash('sha256').update(readFileSync(src)).digest('hex').slice(0, 16);
  console.log(`[build-engine] bundle ${dataset} -> public/engine/contract/${dataset}.json.gz (v${bundleVersions[dataset]})`);
}
const versionsModule =
  `// AUTO-GENERATED by scripts/build-engine.mjs — do not hand-edit. Content hash of each\n` +
  `// contract bundle, appended to its fetch URL by engine.ts so an HTTP-cached bundle can\n` +
  `// never be served to a .wasm built against a different schema. Workbox is told to ignore\n` +
  `// the \`v\` parameter (vite.config.ts \`ignoreURLParametersMatching\`), so the precached\n` +
  `// entry still matches and offline loads are unaffected.\n\n` +
  `export const ENGINE_BUNDLE_VERSIONS: Record<string, string> =\n` +
  `  ${JSON.stringify(bundleVersions, null, 2).replace(/\n/g, '\n  ')};\n`;
writeFileSync(join(betaRoot, 'src', 'engine', 'bundleVersions.generated.ts'), versionsModule);
console.log(`[build-engine] bundle versions -> src/engine/bundleVersions.generated.ts`);

// --- 4. emit the shared set-bonus stat vocabulary as a typed module (PROD6A) ---
// The single source both the engine (include_str! in coh_math set_bonuses.rs) and the beta
// (normalizeStatName / getPairedStat) read, so the two formerly hand-maintained stat tables
// can't drift. It is imported synchronously in render, not fetched, so it's a bundled TS
// module (resolveJsonModule is off) written to src/data/generated/ as committed source this
// step refreshes — like the other *.generated files.
const vocabSrc = join(contractSrc, 'set-bonus-stat-vocab.json');
if (!existsSync(vocabSrc)) die(`rebuild set-bonus stat vocab not found at ${vocabSrc}.`);
const vocab = JSON.parse(readFileSync(vocabSrc, 'utf8'));
const vocabModule =
  `// AUTO-GENERATED by scripts/build-engine.mjs from the rebuild's contract/set-bonus-stat-vocab.json\n` +
  `// — do not hand-edit. The single source of truth for the set-bonus stat vocabulary, shared\n` +
  `// with the engine (coh_math set_bonuses.rs include_str!s the same file), so normalizeStatName /\n` +
  `// getPairedStat can no longer drift from the engine's own map_stat_name / paired().\n\n` +
  `export const SET_BONUS_STAT_NAME_MAP: Record<string, string | null> =\n` +
  `  ${JSON.stringify(vocab.statNameMap, null, 2).replace(/\n/g, '\n  ')};\n\n` +
  `export const SET_BONUS_PAIRED_STATS: Record<string, string> =\n` +
  `  ${JSON.stringify(vocab.pairedStats, null, 2).replace(/\n/g, '\n  ')};\n`;
const vocabOut = join(betaRoot, 'src', 'data', 'generated', 'set-bonus-stat-vocab.generated.ts');
writeFileSync(vocabOut, vocabModule);
console.log(`[build-engine] vocab -> src/data/generated/set-bonus-stat-vocab.generated.ts`);

// --- 5. emit the shared power-effect resolution registry as a typed module (PROD6B-2) ---
// Same single-source pattern as the vocab above: the engine include_str!s
// contract/effect-registry.json (coh_math effect_registry.rs) to resolve per-power granted
// magnitudes, and this module is what the beta's own EFFECT_REGISTRY is gated against
// (effectRegistryDrift.test.ts), so the resolution rules can't drift between the two.
// Presentation-only fields (colorClass, renderAs) are deliberately not in the contract.
const registrySrc = join(contractSrc, 'effect-registry.json');
if (!existsSync(registrySrc)) die(`rebuild effect registry not found at ${registrySrc}.`);
const registry = JSON.parse(readFileSync(registrySrc, 'utf8'));
const registryModule =
  `// AUTO-GENERATED by scripts/build-engine.mjs from the rebuild's contract/effect-registry.json\n` +
  `// — do not hand-edit. The single source of truth for how an exported effect key is resolved\n` +
  `// for display, shared with the engine (coh_math effect_registry.rs include_str!s the same\n` +
  `// file). Presentation-only fields (colorClass, renderAs) stay in src/data/core/effect-registry.ts.\n\n` +
  `export const EFFECT_RESOLUTION: Record<string, Record<string, unknown>> =\n` +
  `  ${JSON.stringify(registry.effects, null, 2).replace(/\n/g, '\n  ')};\n\n` +
  `export const EFFECT_TYPE_LABELS: Record<string, string> =\n` +
  `  ${JSON.stringify(registry.typeLabels, null, 2).replace(/\n/g, '\n  ')};\n\n` +
  `export const EFFECT_MEZ_LABELS: Record<string, string> =\n` +
  `  ${JSON.stringify(registry.mezLabels, null, 2).replace(/\n/g, '\n  ')};\n`;
const registryOut = join(betaRoot, 'src', 'data', 'generated', 'effect-registry.generated.ts');
writeFileSync(registryOut, registryModule);
console.log(`[build-engine] effect registry -> src/data/generated/effect-registry.generated.ts`);

// --- 6. stamp the fingerprint of the rebuild sources these artifacts were built FROM ---
// The beta cannot detect its own engine going stale — it cannot see the rebuild. The rebuild's
// CI can: the beta is public, so it checks the beta out with no secret and re-runs
// engine-fingerprint.mjs against its own tree. This file is the thing it compares to, and it is
// written HERE because this is the one moment both trees are in hand. See engine-fingerprint.mjs.
const manifestOut = join(betaRoot, 'src', 'engine', '_engine_manifest.json');
writeFileSync(manifestOut, `${JSON.stringify(fingerprintRebuild(rebuildDir), null, 2)}\n`);
console.log(`[build-engine] fingerprint -> src/engine/_engine_manifest.json`);

console.log(`\n[build-engine] done — ${datasets.length} dataset(s): ${datasets.join(', ')}`);
