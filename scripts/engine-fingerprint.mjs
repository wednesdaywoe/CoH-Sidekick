/**
 * Fingerprint the rebuild sources the committed engine artifacts are built FROM.
 *
 * The beta ships `src/engine/wasm/`, `src/engine/wasm-node/` and
 * `public/engine/contract/*.json.gz` as committed build output, because a GitHub-hosted
 * runner has no Rust and the rebuild repo is private (see .gitignore). That makes deploy
 * and CI self-sufficient — and it creates exactly one hazard: change `coh_math`, forget
 * `npm run build:engine`, push. CI is green, the deploy is green, and users get a stale
 * engine. Silent, and the same shape as the parser/export staleness that has already bitten
 * this project twice.
 *
 * Nothing in the beta can detect that: it cannot see the rebuild. But the reverse works —
 * the beta is PUBLIC, so the rebuild's CI can check it out with no secret, and it already
 * has the pinned Rust toolchain. So the fingerprint is WRITTEN here (by build-engine.mjs,
 * which has both trees in hand) and VERIFIED there.
 *
 * This module is the single implementation of the hash, imported by the writer and executed
 * by the verifier, so the two cannot compute it differently.
 *
 * What is hashed: the three crates whose source determines the .wasm, their manifests, the
 * workspace lock, and the toolchain pin — plus each contract bundle, which is a separate
 * output of the rebuild's `npm run regen` and goes stale independently.
 *
 * And every file those crates pull in at COMPILE time. `coh_math` `include_str!`s
 * `contract/effect-registry.json` and `contract/set-bonus-stat-vocab.json`, `coh_wasm` does the
 * same with `contract/schema-version.json`: their BYTES are compiled into the .wasm, so editing
 * one changes the shipped engine while touching no crate source. Until 2026-09-12 none of the
 * three was hashed, so a commit that moved only a contract JSON left both halves of this
 * fingerprint identical and this check certified a stale engine as fresh — the exact failure the
 * job exists to catch, one input further up than it was looking. It never fired, because the two
 * commits that did it (`d42784cdd7`, `6d5793d50b`) happened to touch crate source as well; that
 * is luck, and luck is what this file is here to replace.
 *
 * The set is DERIVED by scanning for the macros rather than listed, so a fourth `include_str!`
 * is covered the day it is written instead of reopening the hole silently. `#[cfg(test)]` items
 * are skipped: a release build never compiles them, so hashing `coh_wasm`'s test-only
 * `include_bytes!` of the app crate's vendored bundle would report staleness for a file the
 * shipped artifact does not contain — an unequal for the wrong reason, which this module already
 * treats as its own failure mode.
 *
 * A regex over Rust is a heuristic, and the authoritative answer exists only while a build is in
 * hand: cargo writes every file it actually read to `target/<triple>/release/coh_wasm.d`. So
 * `build-engine.mjs` calls {@link assertIncludesMatchDepInfo} straight after its cargo build, and
 * a scan that has drifted from what the compiler read fails there rather than silently hashing
 * the wrong set.
 *
 * Usage as a script (this is what the rebuild's CI runs):
 *   node scripts/engine-fingerprint.mjs --rebuild-dir <path> [--compare <manifest.json>]
 *
 * `--compare` also grades the shipped artifacts against the `artifacts` key the manifest now
 * carries, reading them from the manifest's own directory (override with `--engine-dir`). With
 * no `--compare` this prints the INPUT halves only — it is a debugging aid, not the writer.
 *
 * {@link gradeRebuild} is here for the same reason as the hash: it is the verdict
 * `build-engine.mjs --verify` reaches when it has rebuilt the engine and wants to know whether
 * the bytes match. That script cannot run without cargo, so a rule written inside it could never
 * be graded by the beta's own suite; written here it is a pure function the vitest file mutates.
 */

import { createHash } from 'node:crypto';
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';

/** Crates whose source compiles into the shipped .wasm. `app` (Dioxus UI) is not one. */
const ENGINE_CRATES = ['coh_data', 'coh_math', 'coh_wasm'];

/** Repo-root files that change the compiled output without changing any crate source. */
const ROOT_INPUTS = ['Cargo.lock', 'Cargo.toml', 'rust-toolchain.toml'];

/** `include_str!("…")` / `include_bytes!("…")` with a plain string literal. */
const INCLUDE_RE = /\binclude_(?:str|bytes)!\s*\(\s*"((?:[^"\\]|\\.)*)"/g;

/**
 * Rust source with `#[cfg(test)]`-guarded items removed.
 *
 * Brace-matched from the guarded item's opening `{`, and a `;` reached first means the item had
 * no block (`#[cfg(test)] use foo;`) so only that statement is dropped. Braces inside string
 * literals or comments would fool this; `assertIncludesMatchDepInfo` is what notices.
 */
function withoutTestItems(source) {
  const marker = '#[cfg(test)]';
  let out = '';
  let i = 0;
  for (;;) {
    const at = source.indexOf(marker, i);
    if (at === -1) return out + source.slice(i);
    out += source.slice(i, at);
    const brace = source.indexOf('{', at);
    const semi = source.indexOf(';', at);
    if (brace === -1 && semi === -1) return out;
    if (semi !== -1 && (brace === -1 || semi < brace)) {
      i = semi + 1;
      continue;
    }
    let depth = 0;
    let j = brace;
    for (; j < source.length; j += 1) {
      if (source[j] === '{') depth += 1;
      else if (source[j] === '}') {
        depth -= 1;
        if (depth === 0) { j += 1; break; }
      }
    }
    i = j;
  }
}

/**
 * Every file the engine crates compile in, as repo-relative POSIX paths, sorted and deduped.
 *
 * Throws on a referenced file that is missing or that resolves outside the rebuild — a
 * fingerprint over an input we cannot read is worse than no fingerprint, since it would compare
 * unequal for a reason nobody can act on (Rule 1: fail loud).
 */
export function compileTimeIncludes(rebuildDir) {
  const found = new Set();
  for (const crate of ENGINE_CRATES) {
    const srcDir = join(rebuildDir, 'crates', crate, 'src');
    if (!existsSync(srcDir)) throw new Error(`engine-fingerprint: no ${crate}/src under ${rebuildDir}`);
    for (const rel of filesUnder(srcDir)) {
      if (!rel.endsWith('.rs')) continue;
      const file = join(srcDir, rel);
      const source = withoutTestItems(readFileSync(file, 'utf8'));
      for (const match of source.matchAll(INCLUDE_RE)) {
        const resolved = resolve(dirname(file), match[1]);
        const label = relative(rebuildDir, resolved).split(sep).join('/');
        if (label.startsWith('..')) {
          throw new Error(`engine-fingerprint: ${crate}/src/${rel} includes ${match[1]}, outside ${rebuildDir}`);
        }
        if (!existsSync(resolved)) {
          throw new Error(`engine-fingerprint: ${crate}/src/${rel} includes ${match[1]}, which does not exist`);
        }
        found.add(label);
      }
    }
  }
  return [...found].sort();
}

/**
 * The same question answered by the compiler instead of by a regex: cargo's dep-info lists every
 * file the build read. Called by `build-engine.mjs` while the build is still in hand, because
 * that is the only moment the authoritative answer exists — CI verifies with node alone and
 * cannot build.
 *
 * Only the non-crate-source includes are compared. Dep-info also lists every .rs file, which
 * {@link fingerprintRebuild} already hashes by walking the tree.
 */
export function assertIncludesMatchDepInfo(rebuildDir, depInfoPath) {
  if (!existsSync(depInfoPath)) {
    throw new Error(`engine-fingerprint: no dep-info at ${depInfoPath} — expected cargo to have just written it.`);
  }
  // `<target>: <space-separated inputs>`, with `\ ` escaping a space inside a path.
  const body = readFileSync(depInfoPath, 'utf8')
    .split('\n')
    .filter((line) => line.includes(':') && !line.startsWith(' '))
    .map((line) => line.slice(line.indexOf(':') + 1))
    .join(' ');
  const read = new Set();
  for (const raw of body.split(/(?<!\\) /)) {
    const token = raw.replace(/\\ /g, ' ').trim();
    if (!token) continue;
    const label = relative(rebuildDir, resolve(rebuildDir, token)).split(sep).join('/');
    if (label.startsWith('..') || label.endsWith('.rs') || !label.includes('/')) continue;
    if (label.startsWith('crates/') && label.includes('/src/')) continue;
    read.add(label);
  }
  const scanned = new Set(compileTimeIncludes(rebuildDir));
  const missed = [...read].filter((f) => !scanned.has(f)).sort();
  const phantom = [...scanned].filter((f) => !read.has(f)).sort();
  if (missed.length === 0 && phantom.length === 0) return scanned.size;
  throw new Error(
    `engine-fingerprint: the compile-time include scan disagrees with what cargo read.\n` +
      (missed.length ? `  cargo read but the scan missed: ${missed.join(', ')}\n` : '') +
      (phantom.length ? `  the scan found but cargo never read: ${phantom.join(', ')}\n` : '') +
      `  A missed file is a hole in the staleness gate; fix the scan in engine-fingerprint.mjs.`,
  );
}

/** Every file under `dir`, recursively, as paths relative to `dir`, sorted. */
function filesUnder(dir) {
  const out = [];
  const walk = (current) => {
    for (const entry of readdirSync(current, { withFileTypes: true })) {
      const full = join(current, entry.name);
      if (entry.isDirectory()) walk(full);
      else if (entry.isFile()) out.push(relative(dir, full).split(sep).join('/'));
    }
  };
  walk(dir);
  return out.sort();
}

/** Hash a list of (label, bytes) pairs, with the label mixed in so a rename is a change. */
function hashEntries(entries) {
  const h = createHash('sha256');
  for (const [label, bytes] of entries) {
    h.update(label);
    h.update('\0');
    h.update(bytes);
    h.update('\0');
  }
  return h.digest('hex');
}

/**
 * Fingerprint a rebuild checkout. Throws (rather than returning a partial hash) if an
 * expected input is missing — a fingerprint over an incomplete tree would compare unequal
 * for the wrong reason and send someone hunting a staleness that isn't there.
 */
export function fingerprintRebuild(rebuildDir) {
  const entries = [];

  for (const crate of ENGINE_CRATES) {
    const crateDir = join(rebuildDir, 'crates', crate);
    const srcDir = join(crateDir, 'src');
    if (!existsSync(srcDir)) throw new Error(`engine-fingerprint: no ${crate}/src under ${rebuildDir}`);
    for (const rel of filesUnder(srcDir)) {
      entries.push([`crates/${crate}/src/${rel}`, readFileSync(join(srcDir, rel))]);
    }
    const manifest = join(crateDir, 'Cargo.toml');
    if (!existsSync(manifest)) throw new Error(`engine-fingerprint: no ${crate}/Cargo.toml`);
    entries.push([`crates/${crate}/Cargo.toml`, readFileSync(manifest)]);
  }

  for (const name of ROOT_INPUTS) {
    const path = join(rebuildDir, name);
    if (!existsSync(path)) throw new Error(`engine-fingerprint: no ${name} under ${rebuildDir}`);
    entries.push([name, readFileSync(path)]);
  }

  // Files the crates compile in by path — their bytes are in the .wasm, so they belong to the
  // SOURCE hash, not the bundle map. Deduped against what the crate walk already pushed, so an
  // include that points back into a crate's own src is not counted twice.
  const alreadyHashed = new Set(entries.map(([label]) => label));
  for (const label of compileTimeIncludes(rebuildDir)) {
    if (alreadyHashed.has(label)) continue;
    entries.push([label, readFileSync(join(rebuildDir, label))]);
  }

  // Contract bundles, hashed individually so a mismatch names the dataset that drifted.
  const contractDir = join(rebuildDir, 'contract');
  if (!existsSync(contractDir)) throw new Error(`engine-fingerprint: no contract/ under ${rebuildDir}`);
  /** @type {Record<string, string>} dataset name -> sha256 of its bundle */
  const bundles = {};
  for (const dataset of readdirSync(contractDir).sort()) {
    const bundle = join(contractDir, dataset, 'bundle.json.gz');
    if (!statSync(join(contractDir, dataset)).isDirectory() || !existsSync(bundle)) continue;
    bundles[dataset] = createHash('sha256').update(readFileSync(bundle)).digest('hex');
  }
  if (Object.keys(bundles).length === 0) throw new Error(`engine-fingerprint: no bundles under ${contractDir}`);

  return { source: hashEntries(entries), bundles };
}

/**
 * Hash the SHIPPED artifacts — the other half of the claim `_engine_manifest.json` makes.
 *
 * {@link fingerprintRebuild} grades the INPUTS: it walks canonical's tree and says the Rust the
 * engine was built from has not moved. Nothing in it opens the two `.wasm` it certifies, so until
 * 2026-09-19 the manifest was a statement about two binaries with none of their bytes in it, and
 * `--compare` graded the premises while never reading the conclusion (STALE-2).
 *
 * What this catches, and what it cannot, because the difference is the whole point. At `--write`
 * time the hash is taken of the artifact `build-engine.mjs` has just produced, so it agrees by
 * construction — it can never be the evidence that the build REPRODUCES. What it catches is
 * DESYNC between the manifest and the files beside it: a half-copied refresh, a stale `wasm-node`
 * next to a fresh `wasm`, a hand-edited binary, a bad merge. Those are exactly the failures an
 * input hash is structurally blind to, because every input still matches.
 *
 * The reproducibility half — rebuild in the gate and compare bytes — is a separate capability and
 * is deliberately NOT this function. STALE-2 measured what it rests on: the build is not
 * bit-identical across hosts as it stands, because panic-location strings in the data section
 * carry the rustup toolchain path and `$CARGO_HOME`.
 *
 * The directories are DERIVED, like the include scan: every `wasm*` subdirectory of the engine
 * dir, every file under it. `build-engine.mjs` emits `wasm/` and `wasm-node/` today; a third
 * target is covered the day it is added rather than reopening the hole silently.
 */
export function fingerprintArtifacts(engineDir) {
  if (!existsSync(engineDir)) throw new Error(`engine-fingerprint: no engine dir at ${engineDir}`);
  /** @type {Record<string, string>} `<dir>/<file>` -> sha256 of its bytes */
  const artifacts = {};
  for (const name of readdirSync(engineDir).sort()) {
    if (!name.startsWith('wasm') || !statSync(join(engineDir, name)).isDirectory()) continue;
    for (const rel of filesUnder(join(engineDir, name))) {
      artifacts[`${name}/${rel}`] = createHash('sha256')
        .update(readFileSync(join(engineDir, name, rel)))
        .digest('hex');
    }
  }
  if (Object.keys(artifacts).length === 0) {
    throw new Error(
      `engine-fingerprint: no wasm*/ artifacts under ${engineDir} — expected the committed build ` +
        `output \`npm run build:engine\` writes. A manifest with nothing to certify is worse than none.`,
    );
  }
  return artifacts;
}

/**
 * Grade a REBUILD against the committed engine — the half no hash taken at write time can do.
 *
 * {@link fingerprintArtifacts} at `--write` time hashes the file the writer has just produced, so
 * it agrees by construction; it catches the manifest and the tree drifting apart afterwards and
 * cannot be evidence that the build reproduces. The only evidence for that is building a second
 * time somewhere else and comparing, which is what `build-engine.mjs --verify` does and what this
 * function judges (STALE-3).
 *
 * Three verdicts, not two, and the third is the point. If the rebuild's sources are not the ones
 * the committed artifacts were built from, the bytes SHOULD differ and this run has measured
 * staleness, not reproducibility — reporting that as "does not reproduce" would be a true failure
 * with a false diagnosis, which is the shape STALE-1 already cost. It is still not a pass: a gate
 * that reports green when it could not grade is the family of defect this row belongs to.
 *
 * @param {{manifest: {source?: string, bundles?: Record<string,string>, artifacts?: Record<string,string>}, inputs: {source: string, bundles?: Record<string,string>}, committed: Record<string,string>, rebuilt: Record<string,string>}} args
 * @returns {{verdict: 'reproduces'|'inconclusive'|'differs', problems: string[]}}
 */
export function gradeRebuild({ manifest, inputs, committed, rebuilt }) {
  if (inputs.source !== manifest.source) {
    return {
      verdict: 'inconclusive',
      problems: [
        `the rebuild's engine sources are not the ones the committed artifacts were built from ` +
          `(manifest ${String(manifest.source).slice(0, 12)}…, rebuild ${inputs.source.slice(0, 12)}…), ` +
          `so a byte difference below would not be a reproducibility failure. That mismatch is what ` +
          `\`engine-fingerprint.mjs --compare\` grades; fix the staleness, then re-run this.`,
      ],
    };
  }

  const problems = [];
  for (const name of [...new Set([...Object.keys(committed), ...Object.keys(rebuilt)])].sort()) {
    const a = committed[name];
    const b = rebuilt[name];
    if (a === b) continue;
    if (!a) problems.push(`${name}: this build produced it, the beta does not ship it.`);
    else if (!b) problems.push(`${name}: the beta ships it, this build did not produce it.`);
    else problems.push(`${name}: committed ${a.slice(0, 12)}…, rebuilt ${b.slice(0, 12)}… — DOES NOT REPRODUCE.`);
  }

  // And the manifest against the files beside it. `--compare` grades that too; it is repeated
  // here because this is the only caller holding all three, so it can name which of the three
  // moved rather than leaving two red jobs pointing at each other.
  if (!manifest.artifacts) {
    problems.push(`the committed manifest has no "artifacts" key — rebuild it with \`npm run build:engine\`.`);
  } else {
    for (const name of [...new Set([...Object.keys(manifest.artifacts), ...Object.keys(committed)])].sort()) {
      if (manifest.artifacts[name] === committed[name]) continue;
      problems.push(`${name}: the committed file and the manifest disagree — manifest desync, not a build difference.`);
    }
  }

  return { verdict: problems.length === 0 ? 'reproduces' : 'differs', problems };
}

/** Human-readable diff of two fingerprints; empty array means they match. */
export function diffFingerprints(committed, actual) {
  const problems = [];
  if (committed.source !== actual.source) {
    problems.push(
      `engine SOURCE differs — the committed artifacts were built from different Rust sources.\n` +
        `  committed ${committed.source}\n  rebuild   ${actual.source}`,
    );
  }
  const datasets = [...new Set([...Object.keys(committed.bundles ?? {}), ...Object.keys(actual.bundles)])].sort();
  for (const ds of datasets) {
    const a = committed.bundles?.[ds];
    const b = actual.bundles[ds];
    if (a === b) continue;
    if (!a) problems.push(`contract bundle "${ds}" exists in the rebuild but not in the committed manifest.`);
    else if (!b) problems.push(`contract bundle "${ds}" is in the committed manifest but not in the rebuild.`);
    else problems.push(`contract bundle "${ds}" differs — committed ${a.slice(0, 12)}…, rebuild ${b.slice(0, 12)}…`);
  }

  // The shipped artifacts themselves. A manifest written before this key existed cannot be graded,
  // and skipping quietly would restore the exact blind spot the key was added to close — so it is
  // a failure, not a pass (Rule 1: fail loud).
  if (!committed.artifacts) {
    problems.push(
      `the committed manifest has no "artifacts" key: it predates artifact hashing and certifies\n` +
        `    two .wasm with none of their bytes in it. Rebuild it with \`npm run build:engine\`.`,
    );
  } else {
    const names = [...new Set([...Object.keys(committed.artifacts), ...Object.keys(actual.artifacts ?? {})])].sort();
    for (const name of names) {
      const a = committed.artifacts[name];
      const b = actual.artifacts?.[name];
      if (a === b) continue;
      if (!a) problems.push(`shipped artifact "${name}" is on disk but not in the committed manifest.`);
      else if (!b) problems.push(`shipped artifact "${name}" is in the committed manifest but not on disk.`);
      else problems.push(`shipped artifact "${name}" is not the one the manifest records — manifest ${a.slice(0, 12)}…, on disk ${b.slice(0, 12)}…`);
    }
  }
  return problems;
}

// --- CLI (the rebuild's CI entry point) ---

const invokedDirectly = process.argv[1] && import.meta.url.endsWith(process.argv[1].split(sep).join('/'));
if (invokedDirectly) {
  const arg = (flag) => {
    const i = process.argv.indexOf(flag);
    return i === -1 ? null : process.argv[i + 1];
  };
  const rebuildDir = arg('--rebuild-dir') ?? '.';
  const comparePath = arg('--compare');
  const actual = fingerprintRebuild(rebuildDir);

  if (!comparePath) {
    const engineDir = arg('--engine-dir');
    console.log(JSON.stringify(engineDir ? { ...actual, artifacts: fingerprintArtifacts(engineDir) } : actual, null, 2));
    process.exit(0);
  }

  if (!existsSync(comparePath)) {
    console.error(`\n[engine-fingerprint] no manifest at ${comparePath} — run \`npm run build:engine\` in the beta and commit it.\n`);
    process.exit(1);
  }
  // The shipped artifacts sit beside the manifest — `src/engine/_engine_manifest.json` next to
  // `src/engine/wasm*/` — so the CI invocation needs no second path and cannot pass a mismatched one.
  actual.artifacts = fingerprintArtifacts(arg('--engine-dir') ?? dirname(resolve(comparePath)));

  const problems = diffFingerprints(JSON.parse(readFileSync(comparePath, 'utf8')), actual);
  if (problems.length === 0) {
    console.log(
      `[engine-fingerprint] beta engine artifacts match this rebuild ` +
        `(${Object.keys(actual.bundles).length} bundles, ${Object.keys(actual.artifacts).length} artifacts).`,
    );
    process.exit(0);
  }
  console.error(
    `\n[engine-fingerprint] the beta's committed engine is not the one this rebuild describes:\n\n` +
      problems.map((p) => `  - ${p}`).join('\n') +
      `\n\nFix: in the beta checkout, run \`npm run build:engine\` against this rebuild and commit\n` +
      `the refreshed src/engine/wasm*/, public/engine/contract/ and src/engine/_engine_manifest.json.\n`,
  );
  process.exit(1);
}
