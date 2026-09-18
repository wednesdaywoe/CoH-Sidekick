/**
 * The F73 projection, held in step across the four places that spell it.
 *
 * SECURITY_AUDIT.md F73 was `SELECT b.*` — one wildcard that published every
 * column of `shared_builds`, including `owner_token_hash`, to anyone holding
 * the public anon key. Replacing a wildcard with a list closes that, and buys a
 * new way to be wrong: four lists that must agree, in two languages and three
 * files, with nothing but care holding them together.
 *
 *   1. `CREATE TABLE shared_builds` — what exists.
 *   2. `GRANT SELECT (...) ON public.shared_builds` — what anon may read.
 *   3. `CREATE VIEW shared_builds_with_author` — what the view publishes.
 *   4. `BUILD_COLUMNS` in `get-build/index.ts` — what a detail read asks for,
 *      under the service role, which no grant constrains.
 *
 * The case this exists for is not a typo. It is the NEXT column somebody adds
 * to shared_builds: a secret one must reach none of these lists, and an
 * ordinary one must reach all four, and neither outcome should depend on
 * whoever writes the ALTER TABLE remembering that three other lists exist.
 * Adding a column now fails this test until it is classified, which is the
 * loud failure the schema comment promises.
 *
 * Parsed from the files rather than restated here — a copy of the list in the
 * assertion would agree with itself forever.
 */

import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const HERE = dirname(fileURLToPath(import.meta.url));
const SCHEMA = readFileSync(join(HERE, 'schema.sql'), 'utf8');
const GET_BUILD = readFileSync(join(HERE, 'functions/get-build/index.ts'), 'utf8');

/** The credential hash. Every list below is asserted not to carry it. */
const SECRET = 'owner_token_hash';

/** Columns of `CREATE TABLE shared_builds (...)`, in declaration order. */
function tableColumns(): string[] {
  const body = /CREATE TABLE shared_builds \(([\s\S]*?)\n\);/.exec(SCHEMA);
  if (!body) throw new Error('CREATE TABLE shared_builds not found in schema.sql');
  return body[1]
    .split('\n')
    .map((line) => /^ {2}([a-z_]+) +[A-Z]/.exec(line))
    .filter((m): m is RegExpExecArray => m !== null)
    .map((m) => m[1])
    .filter((name) => name !== 'CONSTRAINT');
}

/**
 * The LAST `GRANT SELECT (...) ON public.shared_builds` in the file. Last
 * rather than first because schema.sql is cumulative and read top to bottom —
 * the final statement is the one whose effect survives.
 */
function grantedColumns(): string[] {
  const all = [...SCHEMA.matchAll(/GRANT SELECT \(([\s\S]*?)\) ON public\.shared_builds/g)];
  if (all.length === 0) throw new Error('GRANT SELECT (...) ON public.shared_builds not found');
  return all[all.length - 1][1]
    .split(',')
    .map((s) => s.replace(/--.*$/gm, '').trim())
    .filter(Boolean);
}

/**
 * The LAST `CREATE VIEW shared_builds_with_author`, split into the columns it
 * takes from `shared_builds` and the aliases it joins from `profiles`. Same
 * last-wins reasoning as the grant: the two historical `b.*` definitions above
 * it are the SQL that once ran, and are superseded in the same file.
 */
function viewProjection(): { base: string[]; joined: string[] } {
  const all = [...SCHEMA.matchAll(/CREATE VIEW shared_builds_with_author[\s\S]*?;/g)];
  if (all.length === 0) throw new Error('CREATE VIEW shared_builds_with_author not found');
  const last = all[all.length - 1][0];
  expect(last, 'the live view definition must not be a wildcard').not.toMatch(/\bb\.\*/);
  // Only the SELECT list. Past `FROM` lies `LEFT JOIN profiles p ON p.user_id
  // = b.user_id`, whose `b.user_id` is a join predicate and not a published
  // column — reading the whole statement counted it twice.
  const selectList = last.slice(0, last.indexOf('FROM shared_builds b'));
  return {
    base: [...selectList.matchAll(/\bb\.([a-z_]+)/g)].map((m) => m[1]),
    joined: [...selectList.matchAll(/AS (author_[a-z_]+)/g)].map((m) => m[1]),
  };
}

/** The `BUILD_COLUMNS` array literal in get-build. */
function getBuildColumns(): string[] {
  const block = /const BUILD_COLUMNS = \[([\s\S]*?)\]\.join/.exec(GET_BUILD);
  if (!block) throw new Error('BUILD_COLUMNS not found in get-build/index.ts');
  return [...block[1].matchAll(/'([a-z_]+)'/g)].map((m) => m[1]);
}

describe('F73 — the ownership credential is on no published list', () => {
  it('the table still has the column, so this suite is testing something', () => {
    // If the column is ever moved to its own table (the alternative weighed in
    // schema.sql's F73 block), this fails and the whole file should go with it
    // rather than sit here passing vacuously.
    expect(tableColumns()).toContain(SECRET);
  });

  it('is not granted to anon or authenticated', () => {
    expect(grantedColumns()).not.toContain(SECRET);
  });

  it('is not projected by the view', () => {
    const { base, joined } = viewProjection();
    expect(base).not.toContain(SECRET);
    expect(joined).not.toContain(SECRET);
  });

  it('is not selected by get-build, which runs past every grant', () => {
    // The service role key bypasses RLS and column grants alike, so this list
    // is the only thing standing between an anonymous caller with an id and
    // the hash — for unlisted rows as much as public ones.
    expect(getBuildColumns()).not.toContain(SECRET);
    expect(GET_BUILD).not.toMatch(/\.select\('\*'\)/);
  });
});

describe('F73 — the four lists agree', () => {
  it('every table column is either granted or deliberately withheld', () => {
    // The guard that fires when someone adds a column. It cannot decide
    // whether a new column is public; it only refuses to let the question go
    // unasked. Answer it by adding the name to the GRANT list, or to the
    // withheld list here.
    const withheld = [SECRET];
    expect(new Set(tableColumns())).toEqual(new Set([...grantedColumns(), ...withheld]));
  });

  it('the view projects exactly what anon is granted', () => {
    // security_invoker = on means reading the view checks the INVOKER's
    // privileges on these base columns. Project one that is not granted and
    // every anonymous browse fails with "permission denied for column".
    expect(viewProjection().base).toEqual(grantedColumns());
  });

  it('get-build asks for exactly what the view publishes', () => {
    const { base, joined } = viewProjection();
    expect(getBuildColumns()).toEqual([...base, ...joined]);
  });

  it('get-build carries the build document and the visibility gate', () => {
    // Three columns this function cannot lose without breaking silently:
    // build_json is the only reason a detail read exists, and visibility and
    // user_id are what its own 404-or-not decision reads.
    expect(getBuildColumns()).toEqual(expect.arrayContaining(['build_json', 'visibility', 'user_id']));
  });
});
