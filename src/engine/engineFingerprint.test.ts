import { describe, it, expect } from 'vitest';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import {
  compileTimeIncludes,
  diffFingerprints,
  fingerprintArtifacts,
  fingerprintRebuild,
  gradeRebuild,
} from '../../scripts/engine-fingerprint.mjs';

/**
 * The staleness gate's own blind spot — and the mutations that prove it closed.
 *
 * `engine-fingerprint.mjs` is what stops the beta shipping a .wasm built from Rust that has
 * since moved: canonical's CI hashes its own tree and compares against the manifest committed
 * here. Until 2026-09-12 that hash covered `crates/*​/src`, the manifests, the lock, the
 * toolchain pin and each contract bundle — but NOT the three contract JSONs the crates
 * `include_str!`. Those files' bytes are compiled into the .wasm, so a commit touching only
 * `contract/effect-registry.json` changed the shipped engine and moved neither half of the
 * fingerprint, and the job certified a stale binary as fresh. That is the failure it exists to
 * prevent, one input above where it was looking.
 *
 * It never actually fired: `d42784cdd7` and `6d5793d50b` both happened to touch crate source as
 * well, so the hash moved for an unrelated reason. A gate that passes by luck is untested, which
 * is what this file is for — every check below is a MUTATION, a specific edit that must make the
 * fingerprint move. The skill's rule: a green gate is a statement about the observer until you
 * have said what it cannot see.
 *
 * A synthetic tree rather than the real rebuild, because the beta is public and its CI has no
 * canonical checkout — these must run everywhere, not skip where it matters. The scan is also
 * cross-checked against cargo's own dep-info, but only where a build is in hand; that check
 * lives in `build-engine.mjs`.
 */

/** A minimal rebuild-shaped tree: three engine crates, root inputs, one bundle, one include. */
function makeRebuild(): string {
  const root = mkdtempSync(join(tmpdir(), 'fingerprint-'));
  const write = (rel: string, body: string) => {
    mkdirSync(dirname(join(root, rel)), { recursive: true });
    writeFileSync(join(root, rel), body);
  };
  for (const crate of ['coh_data', 'coh_math', 'coh_wasm']) {
    write(`crates/${crate}/Cargo.toml`, `[package]\nname = "${crate}"\n`);
    write(`crates/${crate}/src/lib.rs`, `pub fn ${crate}() {}\n`);
  }
  // The shape that was invisible: bytes compiled in from outside any crate's src.
  write(
    'crates/coh_math/src/lib.rs',
    `const REGISTRY: &str = include_str!("../../../contract/effect-registry.json");\n`,
  );
  write('contract/effect-registry.json', '{"effects":{}}\n');
  write('contract/homecoming/bundle.json.gz', 'not-really-gzip-but-bytes-are-bytes');
  for (const name of ['Cargo.lock', 'Cargo.toml', 'rust-toolchain.toml']) write(name, `# ${name}\n`);
  return root;
}

describe('engine fingerprint', () => {
  it('moves when an include_str!-ed contract file changes, touching no crate source', () => {
    // THE regression. Before the fix both fingerprints were identical here, and a stale engine
    // shipped with beta-engine-staleness green.
    const root = makeRebuild();
    try {
      const before = fingerprintRebuild(root);
      writeFileSync(join(root, 'contract/effect-registry.json'), '{"effects":{"heal":{}}}\n');
      const after = fingerprintRebuild(root);
      expect(after.source).not.toBe(before.source);
      // and it is the SOURCE half that moved — the bundle map must not absorb it
      expect(after.bundles).toEqual(before.bundles);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it('finds the included file by scanning, so a newly added include_str! is covered', () => {
    const root = makeRebuild();
    try {
      expect(compileTimeIncludes(root)).toEqual(['contract/effect-registry.json']);
      const before = fingerprintRebuild(root);
      // A fourth include added later must be picked up without editing any list.
      writeFileSync(join(root, 'contract/schema-version.json'), '{"v":1}\n');
      writeFileSync(
        join(root, 'crates/coh_wasm/src/lib.rs'),
        `const S: &str = include_str!("../../../contract/schema-version.json");\n`,
      );
      expect(compileTimeIncludes(root)).toEqual([
        'contract/effect-registry.json',
        'contract/schema-version.json',
      ]);
      // and now that it is hashed, editing IT moves the fingerprint too
      const added = fingerprintRebuild(root);
      expect(added.source).not.toBe(before.source);
      writeFileSync(join(root, 'contract/schema-version.json'), '{"v":2}\n');
      expect(fingerprintRebuild(root).source).not.toBe(added.source);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it('ignores a #[cfg(test)] include, which a release build never compiles', () => {
    // Hashing this would report staleness for bytes the shipped artifact does not contain —
    // an unequal for the wrong reason, which sends someone hunting a staleness that isn't there.
    const root = makeRebuild();
    try {
      mkdirSync(join(root, 'crates/app/assets'), { recursive: true });
      writeFileSync(join(root, 'crates/app/assets/bundle.json.gz'), 'test-only bytes');
      writeFileSync(
        join(root, 'crates/coh_wasm/src/lib.rs'),
        `pub fn real() {}\n\n#[cfg(test)]\nmod tests {\n` +
          `    fn fixture() -> Vec<u8> { include_bytes!("../../app/assets/bundle.json.gz").to_vec() }\n` +
          `}\n`,
      );
      expect(compileTimeIncludes(root)).toEqual(['contract/effect-registry.json']);
      const before = fingerprintRebuild(root);
      writeFileSync(join(root, 'crates/app/assets/bundle.json.gz'), 'different test-only bytes');
      expect(fingerprintRebuild(root).source).toBe(before.source);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it('still moves on the things it already covered, so the new inputs did not displace them', () => {
    const root = makeRebuild();
    try {
      const base = fingerprintRebuild(root);
      writeFileSync(join(root, 'crates/coh_data/src/lib.rs'), 'pub fn changed() {}\n');
      const srcMoved = fingerprintRebuild(root);
      expect(srcMoved.source).not.toBe(base.source);

      writeFileSync(join(root, 'Cargo.lock'), '# bumped a dependency\n');
      expect(fingerprintRebuild(root).source).not.toBe(srcMoved.source);

      writeFileSync(join(root, 'contract/homecoming/bundle.json.gz'), 'regenerated bytes');
      expect(fingerprintRebuild(root).bundles.homecoming).not.toBe(base.bundles.homecoming);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it('fails loud on an include it cannot read rather than hashing a partial tree', () => {
    const root = makeRebuild();
    try {
      writeFileSync(
        join(root, 'crates/coh_math/src/lib.rs'),
        `const X: &str = include_str!("../../../contract/does-not-exist.json");\n`,
      );
      expect(() => compileTimeIncludes(root)).toThrow(/does not exist/);

      writeFileSync(
        join(root, 'crates/coh_math/src/lib.rs'),
        `const X: &str = include_str!("../../../../escaped.json");\n`,
      );
      expect(() => compileTimeIncludes(root)).toThrow(/outside/);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
});

/**
 * The other half of the manifest's claim — and why the cheap half is still worth having.
 *
 * `fingerprintRebuild` grades the INPUTS. Until 2026-09-19 that was the whole manifest, so
 * `_engine_manifest.json` was a statement about two `.wasm` with none of their bytes in it, and
 * `--compare` graded the premises without ever opening the conclusion (STALE-2).
 *
 * `fingerprintArtifacts` is deliberately NOT the fix for what STALE-2 measured. At `--write` time
 * it hashes the artifact `build-engine.mjs` has just produced, so it agrees by construction and
 * can never be evidence that the build reproduces. It catches the OTHER failure: the manifest and
 * the committed output dirs drifting apart — a half-copied refresh, a stale `wasm-node` next to a
 * fresh `wasm`, a hand-edited binary, a bad merge. An input hash is structurally blind to every
 * one of those, because each leaves all its inputs matching. Same mutation discipline as above:
 * each check is an edit that must make the gate red.
 */

/** An engine dir shaped like the beta's `src/engine/`: the two wasm-bindgen output dirs. */
function makeEngineDir(): string {
  const root = mkdtempSync(join(tmpdir(), 'artifacts-'));
  const write = (rel: string, body: string) => {
    mkdirSync(dirname(join(root, rel)), { recursive: true });
    writeFileSync(join(root, rel), body);
  };
  write('wasm/coh_wasm_bg.wasm', 'browser wasm bytes');
  write('wasm/coh_wasm.js', 'export function init() {}');
  write('wasm-node/coh_wasm_bg.wasm', 'browser wasm bytes');
  write('wasm-node/coh_wasm.cjs', 'module.exports = {};');
  // Not a wasm-bindgen output dir, and not build output — must stay out of the map.
  write('_engine_manifest.json', '{}');
  return root;
}

describe('engine artifact fingerprint', () => {
  it('hashes every shipped artifact and nothing else in the engine dir', () => {
    const root = makeEngineDir();
    try {
      expect(Object.keys(fingerprintArtifacts(root)).sort()).toEqual([
        'wasm-node/coh_wasm.cjs',
        'wasm-node/coh_wasm_bg.wasm',
        'wasm/coh_wasm.js',
        'wasm/coh_wasm_bg.wasm',
      ]);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it('derives the output dirs, so a third wasm-bindgen target is covered the day it is added', () => {
    const root = makeEngineDir();
    try {
      mkdirSync(join(root, 'wasm-bundler'));
      writeFileSync(join(root, 'wasm-bundler/coh_wasm_bg.wasm'), 'a third target');
      expect(fingerprintArtifacts(root)['wasm-bundler/coh_wasm_bg.wasm']).toBeDefined();
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it('reds on a half-copied refresh: one output dir moved, the other left stale', () => {
    // The shape no input hash can see — the rebuild sources are untouched, so `source` and
    // `bundles` both still match and the old manifest called this fresh.
    const root = makeEngineDir();
    try {
      const committed = { source: 's', bundles: {}, artifacts: fingerprintArtifacts(root) };
      writeFileSync(join(root, 'wasm/coh_wasm_bg.wasm'), 'rebuilt browser wasm bytes');
      const actual = { source: 's', bundles: {}, artifacts: fingerprintArtifacts(root) };
      const problems = diffFingerprints(committed, actual);
      expect(problems).toHaveLength(1);
      expect(problems[0]).toMatch(/wasm\/coh_wasm_bg\.wasm/);
      expect(problems[0]).not.toMatch(/wasm-node/);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it('reds on an artifact that is in the manifest but gone from disk, and the reverse', () => {
    const root = makeEngineDir();
    try {
      const committed = { source: 's', bundles: {}, artifacts: fingerprintArtifacts(root) };
      rmSync(join(root, 'wasm-node/coh_wasm.cjs'));
      expect(diffFingerprints(committed, { source: 's', bundles: {}, artifacts: fingerprintArtifacts(root) })).toEqual([
        expect.stringMatching(/wasm-node\/coh_wasm\.cjs.*not on disk/),
      ]);

      const thinner = { source: 's', bundles: {}, artifacts: fingerprintArtifacts(root) };
      writeFileSync(join(root, 'wasm-node/coh_wasm.cjs'), 'module.exports = {};');
      expect(diffFingerprints(thinner, { source: 's', bundles: {}, artifacts: fingerprintArtifacts(root) })).toEqual([
        expect.stringMatching(/wasm-node\/coh_wasm\.cjs.*not in the committed manifest/),
      ]);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it('reds on a manifest that predates the artifacts key rather than passing it', () => {
    // Skipping quietly would restore the exact blind spot the key closes: a manifest with no
    // artifacts in it is the STALE-2 shape, and a gate that passes it is the STALE-2 gate.
    const root = makeEngineDir();
    try {
      const problems = diffFingerprints({ source: 's', bundles: {} }, {
        source: 's',
        bundles: {},
        artifacts: fingerprintArtifacts(root),
      });
      expect(problems).toHaveLength(1);
      expect(problems[0]).toMatch(/no "artifacts" key/);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it('fails loud on an engine dir with no artifacts rather than certifying nothing', () => {
    const root = mkdtempSync(join(tmpdir(), 'artifacts-empty-'));
    try {
      expect(() => fingerprintArtifacts(root)).toThrow(/no wasm\*\/ artifacts/);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
    expect(() => fingerprintArtifacts(join(tmpdir(), 'definitely-not-here-4f2a'))).toThrow(/no engine dir/);
  });

  it('keeps the two halves separate: an artifact edit must not move the input hashes', () => {
    const rebuild = makeRebuild();
    const root = makeEngineDir();
    try {
      const before = fingerprintRebuild(rebuild);
      writeFileSync(join(root, 'wasm/coh_wasm_bg.wasm'), 'different bytes entirely');
      const after = fingerprintRebuild(rebuild);
      expect(after.source).toBe(before.source);
      expect(after.bundles).toEqual(before.bundles);
    } finally {
      rmSync(rebuild, { recursive: true, force: true });
      rmSync(root, { recursive: true, force: true });
    }
  });
});

/**
 * The rebuild verdict — the half the hashes above structurally cannot reach.
 *
 * `fingerprintArtifacts` is taken of the file `build-engine.mjs` has just written, so the manifest
 * agrees with the tree by construction. That catches the two drifting apart afterwards and is
 * silent on whether the build REPRODUCES, which is the question STALE-3 asks: build a second time
 * on a machine that is not the writer, and compare. `gradeRebuild` is the judgement in that
 * comparison, and it lives in the shared module rather than in `build-engine.mjs` because the
 * script needs a real cargo build to run and the beta's CI has neither Rust nor a canonical
 * checkout — a rule written there could never be graded here.
 *
 * The real-build arm runs in canonical's CI on a foreign runner. These are its mutations.
 */
describe('rebuild verdict', () => {
  const artifacts = { 'wasm/coh_wasm_bg.wasm': 'aaa', 'wasm-node/coh_wasm_bg.wasm': 'bbb' };
  const manifest = { source: 'src-hash', bundles: {}, artifacts };
  const inputs = { source: 'src-hash', bundles: {} };

  it('passes only when all three agree: committed, rebuilt, and what the manifest records', () => {
    expect(gradeRebuild({ manifest, inputs, committed: artifacts, rebuilt: { ...artifacts } })).toEqual({
      verdict: 'reproduces',
      problems: [],
    });
  });

  it('reds when the rebuilt bytes differ from the shipped ones — the property itself', () => {
    const { verdict, problems } = gradeRebuild({
      manifest,
      inputs,
      committed: artifacts,
      rebuilt: { ...artifacts, 'wasm/coh_wasm_bg.wasm': 'ccc' },
    });
    expect(verdict).toBe('differs');
    expect(problems).toEqual([expect.stringMatching(/wasm\/coh_wasm_bg\.wasm.*DOES NOT REPRODUCE/)]);
  });

  it('calls a stale rebuild INCONCLUSIVE rather than a reproducibility failure', () => {
    // The bytes WILL differ here and saying "does not reproduce" would be true and misdiagnosed,
    // sending someone hunting host-specific strings in a .wasm built from other sources. STALE-1
    // is the row that cost: its staleness message was true and its "the tree moved" was wrong.
    const { verdict, problems } = gradeRebuild({
      manifest,
      inputs: { source: 'moved-on', bundles: {} },
      committed: artifacts,
      rebuilt: { ...artifacts, 'wasm/coh_wasm_bg.wasm': 'ccc' },
    });
    expect(verdict).toBe('inconclusive');
    expect(problems[0]).toMatch(/not the ones the committed artifacts were built from/);
    expect(problems[0]).not.toMatch(/DOES NOT REPRODUCE/);
  });

  it('does not let inconclusive read as a pass', () => {
    // A gate that cannot grade must not report green. This is the assertion that stops someone
    // "fixing" the noisy stale case by returning reproduces with an empty problem list.
    expect(
      gradeRebuild({ manifest, inputs: { source: 'moved-on', bundles: {} }, committed: artifacts, rebuilt: artifacts })
        .verdict,
    ).toBe('inconclusive');
  });

  it('names an artifact only one side has, in both directions', () => {
    const extra = gradeRebuild({
      manifest,
      inputs,
      committed: artifacts,
      rebuilt: { ...artifacts, 'wasm-bundler/coh_wasm_bg.wasm': 'ddd' },
    });
    expect(extra.problems).toEqual([expect.stringMatching(/wasm-bundler.*the beta does not ship it/)]);

    const missing = gradeRebuild({
      manifest,
      inputs,
      committed: artifacts,
      rebuilt: { 'wasm/coh_wasm_bg.wasm': 'aaa' },
    });
    expect(missing.problems).toEqual([expect.stringMatching(/wasm-node.*this build did not produce it/)]);
  });

  it('separates a manifest desync from a build difference, because the fixes differ', () => {
    // The committed file and the rebuild agree; the manifest is the odd one out. Reporting that
    // as "does not reproduce" would send someone to the compiler for a bookkeeping error.
    const { verdict, problems } = gradeRebuild({
      manifest: { ...manifest, artifacts: { ...artifacts, 'wasm/coh_wasm_bg.wasm': 'stale-record' } },
      inputs,
      committed: artifacts,
      rebuilt: { ...artifacts },
    });
    expect(verdict).toBe('differs');
    expect(problems).toEqual([expect.stringMatching(/manifest desync, not a build difference/)]);
  });

  it('reds on a manifest with no artifacts key instead of grading the pair it can see', () => {
    const { verdict, problems } = gradeRebuild({
      manifest: { source: 'src-hash', bundles: {} },
      inputs,
      committed: artifacts,
      rebuilt: { ...artifacts },
    });
    expect(verdict).toBe('differs');
    expect(problems).toEqual([expect.stringMatching(/no "artifacts" key/)]);
  });
});
