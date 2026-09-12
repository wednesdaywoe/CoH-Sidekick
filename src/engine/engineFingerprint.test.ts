import { describe, it, expect } from 'vitest';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { compileTimeIncludes, fingerprintRebuild } from '../../scripts/engine-fingerprint.mjs';

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
