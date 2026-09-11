#!/usr/bin/env node
/**
 * verify-sync.cjs — the two repos share a pipeline; this is what notices when they stop.
 *
 * `scripts/` is copied between coh-sidekick-1.0 (canonical) and CoH-Sidekick (beta) by hand,
 * and the working belief was that the copies stayed byte-identical. On 2026-08-20 that belief
 * was measured: 17 of the 49 shared scripts differed, and `convert-powerset.cjs` was 714 lines
 * apart with content on BOTH sides that the other repo's history never held. Not one side
 * lagging. Two forks. Canonical had the register's gate-classification and caster-meter work;
 * beta had a `dual_pistols` proper noun in a converter conditional, which is the Rule 0 breach
 * drift let hide. See FORK-1 in docs/gaps/pipeline-provenance.md.
 *
 * So this guard does not try to keep the copies equal. It makes an unequal pair impossible to
 * be UNAWARE of, which is the failure that actually happened. Every shared path is adjudicated
 * in sync-manifest.json as `identical`, `forked` (should converge, hasn't yet — owes an exit) or
 * `per-repo` (two files on purpose — owes an argument and must NOT carry an exit). Each of the
 * two unequal statuses must name a reason and a gap id, and either repo editing a shared file
 * without re-adjudicating turns this red.
 *
 * Paths only one repo runs are declared instead, in `canonicalOnly` or `betaOnly`, and checked
 * from the other direction: that repo must neither hold the file nor name it anywhere tracked.
 * A retired script is checked the same way — `scripts/attic/X` in one repo and a live
 * `scripts/X` in the other is one file at two paths, so it pairs with nothing.
 *
 * Two modes, because only one repo can see the other. Beta is public and canonical's CI already
 * checks it out (see the beta-engine-staleness job), but canonical is private and beta's CI
 * cannot check it out at all:
 *
 *   paired  both trees present, so the manifest is verified against reality on both sides, and
 *           a shared file present in both repos but absent from the manifest is an error.
 *   solo    one tree, so this repo's files are verified against the hashes the manifest records
 *           for THIS repo's role. Weaker, but not weak: the manifest is itself a shared entry,
 *           so moving a hash means editing the manifest in both repos, and a hash edited on one
 *           side reds the other side's solo run.
 *
 *   node scripts/verify-sync.cjs                          # solo, report
 *   node scripts/verify-sync.cjs --gate                   # exit 1 on any error
 *   node scripts/verify-sync.cjs --sibling ../CoH-Sidekick --gate    # paired
 *   node scripts/verify-sync.cjs --sibling ../CoH-Sidekick --write   # re-adjudicate
 *
 * `--write` needs both trees. It records what's actually there and carries every existing
 * annotation forward; a path that newly differs is written as `forked` with a null reason, and
 * a null reason is an error. That's deliberate, and it's the same door CLAUDE.md puts in front
 * of a Rule 0 deviation: a fork becomes legitimate by being named and given an exit, never by
 * being noticed and left alone.
 *
 * WHAT MAKES THE PAIR AGREE IS NOT THIS FILE. This guard measures; the beta's `sync-shared.cjs`
 * (`npm run sync:shared`, beta-only, declared) is what copies. Keeping those apart is the point
 * — a guard that repaired what it measured could never report the one thing worth reporting, a
 * file the beta authored — but so is the fact that the copy exists at all. FORK-1's opening
 * census is the argument: `exported_powers/` was the only surface copied by machine and the only
 * one with zero drift, across 58,445 files, while the 49 hand-copied scripts beside it had
 * forked 17 ways. This guard shipped without its other half, and for three months every shared
 * edit was still a hand copy plus a re-adjudication in two repos. The mirror runs `--write` for
 * you, from canonical, and carries the result back; see FORK-3.
 */

'use strict';

const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const { execFileSync } = require('child_process');

const ROOT = path.join(__dirname, '..');
const MANIFEST = path.join(__dirname, 'sync-manifest.json');
// The manifest cannot record its own hash, so it is not one of its own entries and is checked
// separately below. That leaves one hole worth naming: a solo run cannot tell that its manifest
// has been loosened, only that its files match whatever the local manifest says. Closing it is
// what the paired run in canonical's CI is for.
const SELF_MANIFEST = 'scripts/sync-manifest.json';

const argv = process.argv.slice(2);
const gate = argv.includes('--gate');
const write = argv.includes('--write');
const siblingArg = ((i) => (i < 0 ? null : argv[i + 1]))(argv.indexOf('--sibling'));

/** sha256 of a file's bytes, or null if it isn't there. */
function sha(file) {
  try {
    return crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
  } catch {
    return null;
  }
}

/**
 * Which repo is this? Keyed on package.json `name` rather than the directory, because the
 * directory is whatever a checkout called it — canonical's own CI checks the two out as
 * `rebuild/` and `beta/`, so a path-shaped answer would be wrong in the one place it matters.
 */
const ROLES = { 'coh-sidekick-pipeline': 'canonical', 'coh-sidekick': 'beta' };
function roleOf(root) {
  const name = JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf8')).name;
  const role = ROLES[name];
  if (!role) throw new Error(`${root}: package.json name "${name}" is neither repo`);
  return role;
}

const selfRole = roleOf(ROOT);
const siblingRoot = siblingArg ? path.resolve(siblingArg) : null;
const siblingRole = siblingRoot ? roleOf(siblingRoot) : null;
if (siblingRoot && siblingRole === selfRole)
  throw new Error(`--sibling ${siblingRoot} is another ${selfRole} checkout, not the other repo`);
const paired = !!siblingRoot;
const rootFor = (role) => (role === selfRole ? ROOT : siblingRoot);

if (write && !paired) throw new Error('--write needs --sibling: it records both repos');

/**
 * The tracked trees both repos are expected to hold in common.
 *
 * `tools/bin-crawler` joined `scripts/` on 2026-08-21 at zero drift — 69 files, same roster, every
 * hash equal. That is the whole argument for adding it. It is the parser, so a fork there is the
 * one this register ranks above everything else, and the manifest wrongly believed it already had
 * a fork under FORK-1's opening census. Measuring it while it agrees costs nothing and means the
 * first disagreement is the thing that reports, rather than a later census discovering an old one.
 */
const TRACKED_ROOTS = [
  'scripts',
  'tools/bin-crawler',
  // Joined 2026-08-30 at zero drift, same argument as bin-crawler: 21 files, every hash equal.
  // It reads Mids' database, and MBDIMPORT-2 made it a build input — `mids-power-names.*.json`
  // is the committed half of a name map the planner resolves every .mbd import through.
  'tools/mids-oracle',
  // Joined 2026-08-30. Not reachable the way the rest of `src/` here is: `pipelineSources`
  // follows `require('../src/…')` out of the .cjs converters, and nothing a converter executes
  // imports the importer. It is seeded as a ROOT rather than by widening that walk, because the
  // walk would arrive through `scripts/bulk-import-mids.ts` and drag the whole `@/data` closure
  // with it — 66 more paths and 27 engine files mid-bag-strip, which is a different adjudication
  // than this one. Listing the directory guards its own files and follows none of its imports.
  'src/utils/mids-import',
  // Joined 2026-09-09, and these are two FILES rather than a tree, which `git ls-files` takes as
  // readily as a directory. `src/utils` is not addable: 132 paths shared with the beta and 61 of
  // them differing, so listing the tree would surface 61 unadjudicated forks in one commit. These
  // two are the ones with a live consumer. `mids-export.ts` is the source the Rust .mbd writer
  // will be ported from, and it forked from the beta's copy under SLOT-3 — three hunks, all slot
  // levels. `slot-levels.ts` is where that fork actually lives (690 lines to the beta's 879).
  // Their test twins were already adjudicated in `shared-test-surface.json` under the same gap;
  // the sources were watched by nothing, and the only reason `mids-import/importer.ts` next door
  // IS watched is that someone seeded its directory as a root above.
  'src/utils/mids-export.ts',
  'src/utils/slot-levels.ts',
  // Joined 2026-09-10 under FORK-7, reconciled the same day. It holds
  // `isInherentlyAttuned`, a rule about what the game data MEANS that both
  // engines' slotting paths obey, and the two copies had disagreed since
  // 2026-07-30: the beta took Winter's Gift off the attuned-only list after a
  // user report and canonical's copy kept it, which is MBDEXPORT-14. Nothing
  // could have seen that — `pipelineSources` reaches `src/` by require edges out
  // of the .cjs converters and none of them loads this file, so it was
  // undiscoverable rather than overlooked, FORK-6's shape one directory over. A
  // file path and not `src/data`: that tree shares 142 paths with the beta and
  // 34 of them differ, which is FORK-7's own population, not this entry's.
  'src/data/enhancement-registry.ts',
  // Joined 2026-09-09 under MBDIMPORT-7, at zero drift — five files, every hash equal. The
  // `.mbd` name map: the accessor, and the four generated tables it reads. Nothing discovered
  // them. `pipelineSources` follows `require('../src/…')` out of the converters and
  // `convert-mids-name-map.cjs` requires none — it only WRITES TypeScript — while
  // `src/utils/mids-import` is seeded as a directory and follows none of its imports, so the
  // module `mappers.ts` resolves every .mbd power name through sat outside both. A glob rather
  // than five paths so a fifth fork's table arrives guarded instead of unwatched, which is the
  // shape this entry is fixing.
  'src/data/mids-name-map.ts',
  'src/data/datasets/*/generated/mids-name-map.ts',
  'docs',
];

/**
 * `docs` joined on 2026-08-21, and it is not symmetric the way the other two are: canonical holds
 * 58 files there and the beta 9, so only the intersection is ever adjudicated. That intersection
 * is the point. `docs/DATA-GAP-REGISTER.md` is hand-mirrored between the repos — both sides commit
 * to it, and the beta's converters cite its ids by name — and it had lagged canonical by two
 * commits, showing FORK-2 open in one repo and closed in the other. That is the FORK-1 shape
 * exactly, in the file that tells you what is open.
 */

/** Tracked files under the shared trees, relative to the repo root. */
function trackedScripts(root) {
  return execFileSync('git', ['ls-files', ...TRACKED_ROOTS], { cwd: root, encoding: 'utf8' })
    .split('\n')
    .filter(Boolean)
    .filter((f) => f !== SELF_MANIFEST);
}

/**
 * The `src/` modules the pipeline loads at RUNTIME, plus everything they import.
 *
 * `scripts/` alone is not the shared surface, and believing it was cost a day: the three
 * converters were byte-identical in both repos and still produced different files, because
 * `convert-powerset.cjs` pulls its atom encoder from `src/data/core/atomic-effect.ts` through
 * `tsx` and that file had forked 309 lines. A guard that watches only the scripts reports green
 * while the thing the scripts EXECUTE disagrees.
 *
 * Discovered rather than listed: the `require('../src/…')` edges are read out of the tracked
 * scripts and the TypeScript imports are followed from there, so a script that starts loading a
 * new module widens the guarded surface by itself. A hardcoded list would need remembering,
 * which is the same failure one level up.
 *
 * Files under `generated/` or `datasets/` are dropped. They are data, they are enormous, and the
 * regen-diff guard already owns them; what belongs here is the code that PRODUCES them.
 */
function pipelineSources(root) {
  const tracked = new Set(
    execFileSync('git', ['ls-files', 'src'], { cwd: root, encoding: 'utf8', maxBuffer: 256 * 1024 * 1024 }).split('\n').filter(Boolean)
  );
  const resolve = (spec, from) => {
    let base;
    if (spec.startsWith('@/')) base = `src/${spec.slice(2)}`;
    else if (spec.startsWith('.')) base = path.posix.normalize(path.posix.join(path.posix.dirname(from), spec));
    else return null;
    for (const c of [base, `${base}.ts`, `${base}.tsx`, `${base}/index.ts`]) if (tracked.has(c)) return c;
    return null;
  };
  const queue = [];
  for (const f of trackedScripts(root)) {
    if (!f.endsWith('.cjs')) continue;
    // Tracked and not on disk = deleted without `git rm`. Both the manifest loop and the
    // canonical-only check name that file explicitly; reading it here first turned their
    // report into an ENOENT stack trace, which is loud but says nothing.
    if (!fs.existsSync(path.join(root, f))) continue;
    const src = fs.readFileSync(path.join(root, f), 'utf8');
    for (const m of src.matchAll(/require\('\.\.\/(src\/[^']+)'\)/g)) queue.push(m[1]);
  }
  const seen = new Set();
  while (queue.length) {
    const f = queue.pop();
    if (seen.has(f) || !tracked.has(f)) continue;
    seen.add(f);
    const src = fs.readFileSync(path.join(root, f), 'utf8');
    for (const m of src.matchAll(/from\s+'([^']+)'/g)) {
      const r = resolve(m[1], f);
      if (r && !seen.has(r)) queue.push(r);
    }
  }
  return [...seen].filter((f) => !f.includes('/generated/') && !f.includes('/datasets/'));
}

/** Every path the two repos are expected to share: the scripts, and what the scripts execute. */
function sharedSurface(root) {
  return [...new Set([...trackedScripts(root), ...pipelineSources(root)])];
}

const errors = [];
const manifest = JSON.parse(fs.readFileSync(MANIFEST, 'utf8'));

// -- --write: re-adjudicate against both trees -------------------------------------
if (write) {
  const prior = new Map(manifest.entries.map((e) => [e.path, e]));
  const shared = sharedSurface(rootFor('canonical')).filter((f) =>
    fs.existsSync(path.join(rootFor('beta'), f))
  );
  manifest.entries = shared.sort().map((p) => {
    const canonical = sha(path.join(rootFor('canonical'), p));
    const beta = sha(path.join(rootFor('beta'), p));
    const was = prior.get(p);
    if (canonical === beta) return { path: p, status: 'identical', sha256: canonical };
    // A `per-repo` declaration survives --write; anything else differing is a fork, including a
    // path that was per-repo and has since converged (it comes back as `identical` above, and
    // re-declaring it is then a deliberate act rather than a carried-forward one).
    return {
      path: p,
      status: was?.status === 'per-repo' ? 'per-repo' : 'forked',
      sha256: { canonical, beta },
      reason: was?.reason ?? null,
      exit: was?.exit ?? null,
      gap: was?.gap ?? null,
    };
  });
  fs.writeFileSync(MANIFEST, JSON.stringify(manifest, null, 2) + '\n');
  const forked = manifest.entries.filter((e) => e.status === 'forked');
  console.log(
    `wrote ${manifest.entries.length} shared paths (${forked.length} forked) to scripts/sync-manifest.json`
  );
}

const STATUSES = ['identical', 'forked', 'per-repo'];

// -- verify -------------------------------------------------------------------------
for (const e of manifest.entries) {
  if (!STATUSES.includes(e.status)) errors.push(`${e.path}: unknown status "${e.status}"`);
  if (e.status === 'identical' && typeof e.sha256 !== 'string')
    errors.push(`${e.path}: status "identical" but no single sha256`);
  if (e.status === 'forked') {
    if (!e.reason)
      errors.push(
        `${e.path}: forked with no reason — a fork is legitimate by being NAMED, ` +
          `not by being noticed (CLAUDE.md, the deviation door)`
      );
    if (!e.gap)
      errors.push(`${e.path}: forked with no gap id — an undeclared fork is debt, not precedent`);
  }
  // `forked` and `per-repo` are the same shape and opposite claims. A fork says the two copies
  // OUGHT to be one file and are not yet, so it owes an exit condition. A per-repo entry says
  // they are two files on purpose and no exit is coming, so it owes the argument instead — which
  // is the part worth being unable to skip, because "we decided not to converge this" and "we
  // never got round to converging this" are indistinguishable once the reason is missing.
  if (e.status === 'per-repo') {
    if (!e.reason)
      errors.push(
        `${e.path}: per-repo with no reason — the same door as a fork, and the reason IS the ` +
          `whole declaration: nothing else here will ever ask about this path again`
      );
    if (!e.gap) errors.push(`${e.path}: per-repo with no gap id`);
    if (e.exit)
      errors.push(
        `${e.path}: per-repo with an exit condition — a path with an exit is a fork that hasn't ` +
          `been closed, not a declaration`
      );
  }

  const roles = paired ? ['canonical', 'beta'] : [selfRole];
  for (const role of roles) {
    const want = e.status === 'identical' ? e.sha256 : e.sha256[role];
    const got = sha(path.join(rootFor(role), e.path));
    if (got === null) {
      errors.push(`${e.path}: listed in the manifest, missing from ${role}`);
    } else if (got !== want) {
      errors.push(
        `${e.path}: ${role}'s copy is not the hash the manifest records — a shared file ` +
          `changed without re-adjudicating (run --sibling <path> --write, then say why)`
      );
    }
  }
}

/**
 * Tracked files in `root` that WIRE `needle` — name it somewhere it could be run from.
 *
 * Scoped rather than corpus-wide, for two reasons that pull the same way. The question this
 * answers is whether the repo runs the script, and the places a script gets run from are the
 * scripts themselves, the modules they load, the npm scripts, and CI. The scoping earned itself
 * on a committed `.ua/.trash-*` dump in the beta that named half of `scripts/` in tool scratch
 * JSON: an unscoped grep was permanently red on it, which is how a gate stops being read. That
 * dump is deleted and gitignored now and the scope stays, because scratch will do it again.
 *
 * The two files that exist to DESCRIBE the declaration are excluded, or it would report itself.
 * Other prose that merely names a script is handled per declaration, in `mentions`.
 */
const WIRING_PATHS = ['scripts', 'src', 'package.json', '.github'];
const SELF_GUARD = 'scripts/verify-sync.cjs';
function referencesTo(root, needle) {
  try {
    return execFileSync('git', ['grep', '-l', '--fixed-strings', needle, '--', ...WIRING_PATHS], {
      cwd: root,
      encoding: 'utf8',
    })
      .split('\n')
      .filter(Boolean)
      .filter((f) => f !== SELF_MANIFEST && f !== SELF_GUARD);
  } catch {
    return []; // git grep exits 1 on no match
  }
}

/**
 * Scripts only ONE repo runs, declared rather than shared.
 *
 * Most of canonical's audits were never on the shared surface at all — they exist here, the beta
 * has no copy, and nothing noticed because the manifest discovers its entries from the paths the
 * two repos both hold. `audit-converter-twins.cjs` and `audit-coverage-census.cjs` were the
 * exception only by accident: the beta held a copy, so they were adjudicated as forks and given
 * exit conditions, when the truth was that neither had ever been wired into anything there and
 * neither could run — both read a `*-baseline.json` the beta does not have, so `--gate` was an
 * ENOENT away from the first line it mattered on.
 *
 * So the rule the other audits already follow gets written down: a script only one repo runs is
 * not shared surface. And written down is not enough on its own, because the reason those two
 * became drift is that a convention nobody measures decays into one. This is the measurement. A
 * declaration goes stale in two directions and both are errors: the script disappearing from the
 * repo that owns it, and the other repo growing the file back or naming it anywhere tracked. The
 * second is the live one — the day the other repo wires one of these in, it is shared surface
 * again, and nothing else here would say so.
 *
 * It runs in both directions because the beta owns scripts too, and assuming otherwise cost the
 * pair a silent fork: `push-changelog-discord.ts` and `delete-user-shared-builds.ts` are wired
 * into the beta's package.json and the beta runs them, while canonical kept archived copies in
 * `scripts/attic/`. The changelog pusher then gained id-keyed dedup and unknown-flag rejection in
 * the beta, canonical's copy stayed 100 lines behind, and nothing said so — the attic move had
 * already taken the file off the path this manifest keys on.
 */
const OWNED = [
  ['canonicalOnly', 'canonical', 'beta'],
  ['betaOnly', 'beta', 'canonical'],
];
for (const [key, owner, other] of OWNED) {
  for (const d of manifest[key] ?? []) {
    if (!d.reason)
      errors.push(`${d.path}: declared ${owner}-only with no reason — same door as a fork`);

    if (paired || selfRole === owner) {
      if (!fs.existsSync(path.join(rootFor(owner), d.path)))
        errors.push(
          `${d.path}: declared ${owner}-only and absent from ${owner} — a stale declaration`
        );
    }

    if (paired || selfRole === other) {
      const otherRoot = rootFor(other);
      if (fs.existsSync(path.join(otherRoot, d.path)))
        errors.push(
          `${d.path}: declared ${owner}-only but present in ${other} — either that copy is dead ` +
            `and should go, or the declaration is wrong and this is shared surface`
        );
      // Not "names it nowhere" but "names it exactly where the declaration says". A path that
      // only DESCRIBES the script — a doc comment, an archive README — is a mention, not wiring,
      // and there is no way to tell those apart by reading the grep hit. So they are enumerated
      // instead, and the check is set equality: a mention the declaration did not predict is the
      // error, and so is a declared mention that has since gone. A bare exclusion list would go
      // quiet on the first NEW reference, which is the only one that matters.
      const declared = new Set(d.mentions ?? []);
      const refs = referencesTo(otherRoot, path.basename(d.path));
      const undeclared = refs.filter((f) => !declared.has(f));
      if (undeclared.length)
        errors.push(
          `${d.path}: declared ${owner}-only, and ${other} names it in ${undeclared.join(', ')} ` +
            `— if ${other} RUNS it there, this is shared surface and owes an entry; if that is ` +
            `prose about the declaration, add the path to "mentions"`
        );
      const gone = [...declared].filter((f) => !refs.includes(f));
      if (gone.length)
        errors.push(
          `${d.path}: declared ${owner}-only with mentions in ${gone.join(', ')}, and ${other} ` +
            `no longer names them there — a stale allowance is how the next real one gets waved ` +
            `through`
        );
    }
  }
}

/**
 * A retired script in one repo may not have a live twin in the other.
 *
 * Every check above keys on the path, so moving a file into `scripts/attic/` unpairs it: the same
 * file then sits at two different paths, matches no entry, and is graded by neither side. Seven
 * dead scripts sat in that gap for a month and `push-changelog-discord.ts` drifted 100 lines
 * inside it. The attic is mirrored now — both repos hold the same seven — and this is what keeps
 * it that way. Retiring a script is a decision about the pair, so it happens in both repos, or
 * the file belongs to one of them and the declaration above is where that gets said.
 */
const ATTIC = 'scripts/attic/';
if (paired) {
  for (const role of ['canonical', 'beta']) {
    const otherRole = role === 'canonical' ? 'beta' : 'canonical';
    for (const f of trackedScripts(rootFor(role))) {
      if (!f.startsWith(ATTIC) || f.endsWith('README.md')) continue;
      const live = `scripts/${f.slice(ATTIC.length)}`;
      if (fs.existsSync(path.join(rootFor(otherRole), live)))
        errors.push(
          `${f}: retired in ${role}, still live at ${live} in ${otherRole} — one file at two ` +
            `paths pairs with nothing and is graded by neither; retire it in both repos, or ` +
            `declare it ${otherRole}Only and drop the archived copy`
        );
    }
  }
}


// A shared path nobody adjudicated is the original failure in miniature, so paired runs look
// for it. Solo runs cannot: the intersection is only knowable with both trees in hand.
if (paired) {
  const listed = new Set(manifest.entries.map((e) => e.path));
  const shared = sharedSurface(rootFor('canonical')).filter((f) =>
    fs.existsSync(path.join(rootFor('beta'), f))
  );
  for (const p of shared)
    if (!listed.has(p))
      errors.push(`${p}: present in both repos and in no manifest entry — unadjudicated shared surface`);
}

// The manifest is the thing every other check trusts, so a paired run verifies it directly.
if (paired) {
  const mine = sha(MANIFEST);
  const theirs = sha(path.join(siblingRoot, SELF_MANIFEST));
  if (theirs === null) errors.push(`${SELF_MANIFEST}: missing from ${siblingRole}`);
  else if (mine !== theirs)
    errors.push(
      `${SELF_MANIFEST}: the two repos hold different manifests — every other check here ` +
        `trusts this file, so an unequal pair means both sides were grading themselves`
    );
}

// -- report -------------------------------------------------------------------------
if (errors.length) {
  for (const e of errors) console.error(`ERROR: ${e}`);
  console.error(`\n${errors.length} error(s) — the shared pipeline drifted unadjudicated.`);
  if (gate) process.exit(1);
} else {
  const count = (s) => manifest.entries.filter((e) => e.status === s).length;
  const canonicalOnly = (manifest.canonicalOnly ?? []).length;
  const betaOnly = (manifest.betaOnly ?? []).length;
  console.log(
    `ok: ${manifest.entries.length} shared paths verified ${paired ? 'paired' : `solo (${selfRole})`}, ` +
      `${count('identical')} identical, ${count('forked')} forked and declared, ` +
      `${count('per-repo')} per-repo by declaration; ` +
      `${canonicalOnly} canonical-only, ${betaOnly} beta-only`
  );
}
