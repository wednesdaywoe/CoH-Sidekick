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
 * The rebuild repo is located via COH_REBUILD_DIR (or --rebuild-dir), defaulting to the
 * sibling checkout. Every input is verified before use; a missing rebuild, a version skew,
 * or a missing bundle aborts with a specific message rather than emitting a half-built engine.
 *
 * `--verify` runs steps 1 and 2 into a scratch directory and compares what came out against
 * the committed artifacts instead of replacing them (STALE-3). It writes nothing into the
 * beta tree. That mode exists so a machine which is NOT the one that wrote the manifest can
 * rebuild and grade the bytes: every hash in `_engine_manifest.json` is taken of the file the
 * writer had just produced, so it agrees by construction and can never be the evidence that
 * the build reproduces. Only a second build somewhere else is.
 *
 * It lives HERE rather than in a gate script of its own because the two must build alike. The
 * remap flags below are derived per host, and STALE-3's own arm F is the lesson: it logged the
 * bytes it produced and not the flags that produced them, so its number was unreproducible
 * three days later. A verifier that spells the flags a second time is arm F with a green tick.
 */

import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, mkdtempSync, copyFileSync, readdirSync, readFileSync, renameSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { homedir, tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { assertIncludesMatchDepInfo, fingerprintArtifacts, fingerprintRebuild, gradeRebuild } from './engine-fingerprint.mjs';
import { fileURLToPath } from 'node:url';

const betaRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');

const flagValue = (flag) => {
  const i = process.argv.indexOf(flag);
  return i === -1 ? null : process.argv[i + 1];
};
/** Rebuild-and-compare instead of rebuild-and-replace. Writes nothing into the beta tree. */
const verifyOnly = process.argv.includes('--verify');
const rebuildDir = resolve(flagValue('--rebuild-dir') ?? process.env.COH_REBUILD_DIR ?? join(betaRoot, '..', 'coh-sidekick-1.0'));
const engineDir = resolve(flagValue('--engine-dir') ?? join(betaRoot, 'src', 'engine'));

function die(message) {
  console.error(`\n[build-engine] ${message}\n`);
  process.exit(1);
}

function run(command, args, cwd, env) {
  console.log(`[build-engine] ${command} ${args.join(' ')}`);
  execFileSync(command, args, { cwd, stdio: 'inherit', env: env ? { ...process.env, ...env } : process.env });
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
      `  cargo install -f wasm-bindgen-cli --version ${requiredCliVersion} --locked`,
  );
}

if (installedCliVersion !== requiredCliVersion) {
  die(
    `wasm-bindgen CLI ${installedCliVersion} != crate-resolved ${requiredCliVersion}.\n` +
      `The CLI and the linked lib must match. Install the pinned version:\n` +
      `  cargo install -f wasm-bindgen-cli --version ${requiredCliVersion} --locked`,
  );
}
console.log(`[build-engine] wasm-bindgen ${installedCliVersion} matches rebuild lockfile`);
// `--locked` in both hints above, and it is not tidiness. The version equality checked here is
// the CLI's OWN version, and two CLIs at 0.2.126 installed on different days resolve different
// versions of walrus, which wasm-bindgen writes into the .wasm's `producers` section. That is a
// one-byte difference in a shipped artifact, invisible to every check that existed, and it is
// what the rebuild gate caught on its first foreign run (2026-09-19): jpc `walrus 0.26.4`,
// espresso `0.26.5`, everything else byte-identical. Installing from the crate's own lockfile is
// what makes two hosts agree; this script cannot verify it, the gate is what notices.

// --- 2. build the wasm + emit the browser glue ---

// STALE-3: make the build path-independent, so the gate can rebuild and compare instead of
// taking this manifest's word for what came out.
//
// `core::panic::Location` puts the source path of every panic site in the .wasm's data section.
// For workspace members cargo passes those relative, so they are already portable — measured, not
// assumed: the shipped artifact carries zero strings under the checkout dir. What it does carry is
// 22 under `$CARGO_HOME/registry` and 16 under the rustup toolchain dir, and those two move with
// the machine. That is the whole of the difference: six builds across three environments gave
// three binaries, each internally reproducible, differing only here.
//
// Both prefixes are host-specific, so neither can be committed as a literal in a cargo config —
// they have to be read off the machine at build time, which is why this lives here and not in
// `.cargo/config.toml`. (`-Zremap-path-scope`/`trim-paths` would be the supported answer; it is
// still unstable on the 1.96.1 pin.)
const cargoRegistrySrc = join(process.env.CARGO_HOME ?? join(homedir(), '.cargo'), 'registry', 'src');
// The index hash is part of the path and is protocol-derived (`index.crates.io-…` for sparse),
// so remap through it rather than up to it: a runner on a different protocol would otherwise
// still differ, and the gate would red for the one reason it is meant to rule out.
const registryIndexes = existsSync(cargoRegistrySrc)
  ? readdirSync(cargoRegistrySrc).filter((entry) => entry.startsWith('index.crates.io-'))
  : [];
if (registryIndexes.length !== 1) {
  die(
    `expected exactly one crates.io registry index under ${cargoRegistrySrc}, found ${registryIndexes.length}` +
      `${registryIndexes.length ? ` (${registryIndexes.join(', ')})` : ''}.\n` +
      `The remap has to name one directory; pick it with CARGO_HOME rather than guess.`,
  );
}
const registryPrefix = join(cargoRegistrySrc, registryIndexes[0]);

// `rustc --print sysroot` answers for the toolchain rustup resolves IN THAT DIRECTORY, and the
// two trees do not resolve alike: the rebuild pins 1.96.1 in rust-toolchain.toml, the beta pins
// nothing and gets `stable`. Asked from the wrong cwd this returns a real path that is simply not
// the one the build embeds, and the flag becomes a silent no-op — it would build, pass every gate
// present, and leave all 16 toolchain strings in place. Hence cwd, and hence the assertion below.
const sysrootPrefix = execFileSync('rustc', ['--print', 'sysroot'], { cwd: rebuildDir, encoding: 'utf8' }).trim();
if (!sysrootPrefix) die('rustc --print sysroot returned nothing; cannot remap the toolchain path.');

// The third family, and it is not a path the builder chose — it is an OPTIONAL COMPONENT.
//
// rustc ships std already virtualised: a panic site in `alloc` is recorded as
// `/rustc/<commit-hash>/library/alloc/src/...`, which is the same on every machine. But when the
// `rust-src` component is installed, rustc translates that virtual prefix BACK to the local copy
// under `<sysroot>/lib/rustlib/src/rust`, so the real sysroot path is what reaches the .wasm —
// and the remap above then rewrites it to `/rustc/lib/rustlib/src/rust/library/...`. Two hosts on
// the same toolchain, same triple, same flags, differing only in whether someone once ran
// `rustup component add rust-src`, produce 192 bytes of different string data and, once every
// later address has shifted, a different code section too. Measured, 2026-09-19: jpc (rust-src
// installed) against espresso (not), both x86_64 Linux on the 1.96.1 pin.
//
// So map the local rust-src tree onto the spelling a machine without it emits anyway. This rule
// is listed FIRST and rustc takes the longest match, so the sysroot rule below cannot shadow it.
const rustcCommitHash = execFileSync('rustc', ['-vV'], { cwd: rebuildDir, encoding: 'utf8' })
  .split('\n')
  .find((line) => line.startsWith('commit-hash: '))
  ?.slice('commit-hash: '.length)
  .trim();
if (!rustcCommitHash || !/^[0-9a-f]{40}$/.test(rustcCommitHash)) {
  die(
    `rustc -vV gave no usable commit-hash (got ${JSON.stringify(rustcCommitHash ?? null)}).\n` +
      `That hash IS the virtual prefix std's paths carry, so without it a host with the rust-src\n` +
      `component installed cannot be made to agree with one without it.`,
  );
}
const rustSrcPrefix = join(sysrootPrefix, 'lib', 'rustlib', 'src', 'rust');

// RUSTFLAGS is split on spaces, and both prefixes are paths that may contain them. The encoded
// form is unit-separator-delimited and has no such ambiguity.
const UNIT_SEPARATOR = String.fromCharCode(0x1f);
const remapFlags = [
  `--remap-path-prefix=${rustSrcPrefix}=/rustc/${rustcCommitHash}`,
  `--remap-path-prefix=${registryPrefix}=/cargo`,
  `--remap-path-prefix=${sysrootPrefix}=/rustc`,
];
console.log(`[build-engine] remapping ${rustSrcPrefix} -> /rustc/${rustcCommitHash}`);
console.log(`[build-engine] remapping ${registryPrefix} -> /cargo`);
console.log(`[build-engine] remapping ${sysrootPrefix} -> /rustc`);

run(
  'cargo',
  ['build', '--release', '--target', 'wasm32-unknown-unknown', '-p', 'coh_wasm'],
  rebuildDir,
  { CARGO_ENCODED_RUSTFLAGS: remapFlags.join(UNIT_SEPARATOR) },
);

const wasmArtifact = join(rebuildDir, 'target', 'wasm32-unknown-unknown', 'release', 'coh_wasm.wasm');
if (!existsSync(wasmArtifact)) die(`cargo did not produce ${wasmArtifact}.`);

// A remap that names the wrong prefix still builds and still passes everything downstream; the
// only place it shows is the bytes. So read them back. Checking the two prefixes by name cannot
// false-positive, and it is the direct proof that the flags above bit rather than resolved to
// some real-but-unused path.
const wasmBytes = readFileSync(wasmArtifact);
for (const [label, prefix] of [['registry', registryPrefix], ['toolchain', sysrootPrefix]]) {
  if (wasmBytes.includes(prefix)) {
    die(
      `the ${label} remap did not take: ${prefix} is still embedded in the built .wasm.\n` +
        `The prefix must match what the compiler recorded, exactly and from its first character.`,
    );
  }
}
// Every `/rustc/…` left in the bytes must be the virtualised form std already ships with, which
// is the one spelling a machine cannot influence. This is the check that would have caught the
// rust-src collision above by itself: under the old flags jpc emitted `/rustc/lib/rustlib/src/…`
// and no assertion looked, so the difference only surfaced as a hash mismatch on another box,
// three sections away from its cause. Reading the shape rather than the absence of one prefix
// generalises: any future rule that maps something else under `/rustc` reds here.
const VIRTUAL_STD_PREFIX = /\/rustc\/(?![0-9a-f]{40}\/)[\x20-\x7e]{0,60}/g;
const strayStd = [...new Set(String(wasmBytes).match(VIRTUAL_STD_PREFIX) ?? [])];
if (strayStd.length > 0) {
  die(
    `the .wasm carries \`/rustc/…\` paths that are not std's virtualised \`/rustc/<commit-hash>/\`:\n` +
      strayStd.slice(0, 10).map((line) => `  ${line}`).join('\n') +
      `${strayStd.length > 10 ? `\n  … and ${strayStd.length - 10} more` : ''}\n` +
      `The usual cause is the rust-src component: rustc rewrites std's virtual paths to the local\n` +
      `copy when it is installed, so this host embeds something a host without it does not.`,
  );
}

// And a wider sweep, because the two prefixes are what was MEASURED, not a proof that nothing
// else carries the machine. A third family appearing later would otherwise re-host-lock the
// artifact in silence, which is the failure this whole entry is about.
const home = homedir();
if (home.length > 1 && wasmBytes.includes(home)) {
  const stray = [...new Set(String(wasmBytes).split(/[^\x20-\x7e]+/).filter((run) => run.includes(home)))];
  die(
    `the built .wasm still embeds paths under ${home}, beyond the two this build remaps:\n` +
      stray.slice(0, 10).map((line) => `  ${line}`).join('\n') +
      `${stray.length > 10 ? `\n  … and ${stray.length - 10} more` : ''}\n` +
      `Each is a path that moves with the machine; remap it or the rebuild gate reds off this host.`,
  );
}
console.log('[build-engine] no host-absolute paths remain in the built .wasm');

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

// Under `--verify` the glue is emitted into a scratch tree and the committed one is left alone:
// a gate that overwrites what it is grading has graded nothing, and on a CI runner it would also
// leave the checkout dirty for whatever runs next.
const outEngineDir = verifyOnly ? mkdtempSync(join(tmpdir(), 'coh-engine-verify-')) : engineDir;
if (verifyOnly) console.log(`[build-engine] --verify: emitting into ${outEngineDir}, comparing against ${engineDir}`);

const wasmOutDir = join(outEngineDir, 'wasm');
mkdirSync(wasmOutDir, { recursive: true });
run('wasm-bindgen', ['--target', 'web', '--out-dir', wasmOutDir, '--out-name', 'coh_wasm', wasmArtifact], betaRoot);

const nodeOutDir = join(outEngineDir, 'wasm-node');
mkdirSync(nodeOutDir, { recursive: true });
run('wasm-bindgen', ['--target', 'nodejs', '--out-dir', nodeOutDir, '--out-name', 'coh_wasm', wasmArtifact], betaRoot);
// The emitted glue is CommonJS but lands as `.js`, which this ESM package would parse as a
// module — the gates `require` it as `.cjs`. Rename rather than leave both, or the stale one
// keeps being the file that loads.
renameSync(join(nodeOutDir, 'coh_wasm.js'), join(nodeOutDir, 'coh_wasm.cjs'));

// --- 2b. --verify: grade what this build produced against what is committed (STALE-3) ---
//
// This is the half `_engine_manifest.json` structurally cannot do. Its `artifacts` hashes are
// taken at write time of the files step 2 has just written, so they agree by construction; they
// catch the manifest and the committed tree DRIFTING APART afterwards (STALE-2) and say nothing
// about whether the build reproduces. The only evidence for that is a second build, and a second
// build on the writer's own machine grades the writer's machine. So the verdict below is worth
// exactly as much as the distance between the runner and `jpc` — see the job in canonical's CI.
//
// Three-way, because with the rebuilt bytes in hand all three disagreements are distinguishable
// and they have different fixes. Saying which one it is matters more than saying it is red: a
// stale beta reds this job for a reason that is not reproducibility, and reporting THAT as a
// reproducibility failure is STALE-1's lesson (a true message with the wrong diagnosis) repeated.
if (verifyOnly) {
  const manifestPath = join(engineDir, '_engine_manifest.json');
  if (!existsSync(manifestPath)) {
    die(`no manifest at ${manifestPath} — nothing to compare this build against.`);
  }
  const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
  const rebuilt = fingerprintArtifacts(outEngineDir);
  const committed = fingerprintArtifacts(engineDir);
  const inputs = fingerprintRebuild(rebuildDir);

  // The verdict itself is in engine-fingerprint.mjs, for the same reason the hash is: this file
  // needs a real cargo build to run, so nothing in the beta's own CI could ever grade a rule
  // written here. Over there it is a pure function the vitest suite mutates.
  const { verdict, problems } = gradeRebuild({ manifest, inputs, committed, rebuilt });

  if (verdict === 'inconclusive') {
    die(`INCONCLUSIVE — ${problems[0]}\nScratch build left at ${outEngineDir}.`);
  }
  if (verdict === 'differs') {
    console.error(
      `\n[build-engine] --verify: this host's build is not the engine the beta ships:\n\n` +
        problems.map((p) => `  - ${p}`).join('\n') +
        `\n\nScratch build left at ${outEngineDir} for diffing.\n` +
        `A "DOES NOT REPRODUCE" line means the .wasm still carries something that moves with the\n` +
        `machine. The two remapped prefixes are already asserted absent from the bytes above, so a\n` +
        `third family is the thing to look for: \`strings\` both .wasm and diff them.\n`,
    );
    process.exit(1);
  }

  rmSync(outEngineDir, { recursive: true, force: true });
  console.log(
    `[build-engine] --verify: ${Object.keys(rebuilt).length} artifact(s) rebuilt on this host are byte-identical\n` +
      `                        to the committed ones, and the manifest records those same bytes.\n` +
      `                        (Contract bundles and the generated TS modules are the input half — ` +
      `\`engine-fingerprint.mjs --compare\` grades those.)`,
  );
  process.exit(0);
}

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

// --- 6. stamp the fingerprint of the rebuild sources these artifacts were built FROM, and of
// the artifacts themselves ---
// The beta cannot detect its own engine going stale — it cannot see the rebuild. The rebuild's
// CI can: the beta is public, so it checks the beta out with no secret and re-runs
// engine-fingerprint.mjs against its own tree. This file is the thing it compares to, and it is
// written HERE because this is the one moment both trees are in hand. See engine-fingerprint.mjs.
//
// The `artifacts` half is hashed here too, and it is worth being precise about what that buys:
// taken at this moment it agrees by construction, because these are the bytes step 2 just wrote.
// It is not evidence the build reproduces. It is what lets `--compare` notice the manifest and the
// committed `wasm*/` having drifted apart afterwards — a half-copied refresh, a stale `wasm-node`
// beside a fresh `wasm`, a bad merge — which no input hash can see, since every input still
// matches. STALE-2 is the row that separates the two.
const manifestOut = join(engineDir, '_engine_manifest.json');
const manifest = { ...fingerprintRebuild(rebuildDir), artifacts: fingerprintArtifacts(engineDir) };
writeFileSync(manifestOut, `${JSON.stringify(manifest, null, 2)}\n`);
console.log(
  `[build-engine] fingerprint -> src/engine/_engine_manifest.json ` +
    `(${Object.keys(manifest.artifacts).length} artifacts hashed)`,
);

console.log(`\n[build-engine] done — ${datasets.length} dataset(s): ${datasets.join(', ')}`);
