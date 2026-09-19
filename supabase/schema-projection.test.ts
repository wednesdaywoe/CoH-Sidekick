/**
 * The published column lists, held in step with the things that read them.
 *
 * Two findings, one invariant. F73 is `shared_builds`; F34 is `profiles`. Both
 * were fixed the same way -- REVOKE the table grant, GRANT the columns by name
 * -- and both bought the same new way to be wrong: a list that has to agree
 * with every reader of the table, in two languages and several files, with
 * nothing but care holding them together.
 *
 * -- F73 --------------------------------------------------------------------
 *
 * SECURITY_AUDIT.md F73 was `SELECT b.*` — one wildcard that published every
 * column of `shared_builds`, including `owner_token_hash`, to anyone holding
 * the public anon key. Replacing a wildcard with a list closes that. Four lists
 * then have to agree:
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
 * -- F34 --------------------------------------------------------------------
 *
 * `profiles` published discord_id and discord_username -- 452 Discord
 * identities -- to anyone holding the public anon key. The five columns granted
 * back are the union of what three invoker-rights readers need, so here the
 * lists must agree in the other direction too: grant too FEW and the browse
 * breaks, because `shared_builds_with_author` is security_invoker and
 * `search_authors` / `resolve_author` are both `prosecdef = false`. A reader
 * that touches an ungranted column is a 42501 for every anonymous visitor.
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

/** Columns of `CREATE TABLE <table> (...)`, in declaration order. */
function tableColumns(table = 'shared_builds'): string[] {
  const body = new RegExp(`CREATE TABLE ${table} \\(([\\s\\S]*?)\\n\\);`).exec(SCHEMA);
  if (!body) throw new Error(`CREATE TABLE ${table} not found in schema.sql`);
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
function grantedColumns(table = 'shared_builds'): string[] {
  // `[^)]` rather than a lazy `[\s\S]*?`: with two granted tables in the file,
  // a lazy span starting at the FIRST `GRANT SELECT (` runs on until it meets
  // `) ON public.<table>`, swallowing the other table's list and every comment
  // between. A column list cannot contain a paren, so this cannot over-reach.
  const all = [...SCHEMA.matchAll(
    new RegExp(`GRANT SELECT \\(([^)]*)\\) ON public\\.${table}`, 'g'),
  )];
  if (all.length === 0) throw new Error(`GRANT SELECT (...) ON public.${table} not found`);
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

// ---------------------------------------------------------------------------
// F34 — profiles
// ---------------------------------------------------------------------------

/** The five columns F34's migration deliberately withholds from anon. */
const WITHHELD_FROM_ANON = [
  'discord_id',
  'discord_username',
  'handle_changed_at',
  'created_at',
  'updated_at',
];

/**
 * The LAST body of a `CREATE [OR REPLACE] FUNCTION <name>`, dollar-quoted.
 * Last-wins for the same reason the grant and the view use it: schema.sql is
 * cumulative, and the final definition is the one left standing.
 */
function functionBody(name: string): string {
  const all = [...SCHEMA.matchAll(
    new RegExp(`CREATE (?:OR REPLACE )?FUNCTION ${name}\\(([\\s\\S]*?)\\$\\$;`, 'g'),
  )];
  if (all.length === 0) throw new Error(`FUNCTION ${name} not found in schema.sql`);
  return all[all.length - 1][0];
}

/**
 * Columns of `profiles` that a body reads through the alias `p`. Covers the
 * view and search_authors, which both alias it; resolve_author does not alias
 * and is read separately below.
 */
function profileColumnsVia(alias: string, body: string): string[] {
  const seen = [...body.matchAll(new RegExp(`\\b${alias}\\.([a-z_]+)`, 'g'))].map((m) => m[1]);
  return [...new Set(seen)].sort();
}

/** `resolve_author` is `SELECT <list> FROM profiles` with no alias. */
function resolveAuthorColumns(): string[] {
  const body = functionBody('resolve_author');
  const select = /AS \$\$\s*SELECT ([\s\S]*?)\s*FROM profiles/.exec(body);
  if (!select) throw new Error('resolve_author SELECT list not found');
  return select[1].split(',').map((c) => c.trim()).sort();
}

describe('F34 — the Discord identities are on no anon list', () => {
  it('the table still has the columns, so this suite is testing something', () => {
    // If discord_id ever moves or is dropped, this fails and the withheld list
    // should be revisited rather than left here passing vacuously.
    expect(tableColumns('profiles')).toEqual(expect.arrayContaining(WITHHELD_FROM_ANON));
  });

  it('none of the withheld five is granted to anon', () => {
    const granted = grantedColumns('profiles');
    for (const column of WITHHELD_FROM_ANON) {
      expect(granted, `${column} must not be granted to anon`).not.toContain(column);
    }
  });

  it('every profiles column is either granted or deliberately withheld', () => {
    // The guard that fires when someone adds a column to profiles. It cannot
    // decide whether the new column is public; it refuses to let the question
    // go unasked.
    expect(new Set(tableColumns('profiles')))
      .toEqual(new Set([...grantedColumns('profiles'), ...WITHHELD_FROM_ANON]));
  });

  it('the grant goes to anon only — authenticated is still open, by decision', () => {
    // Not an aspiration: if someone adds `authenticated` to F34's GRANT without
    // first narrowing the two clients' `select('*')`, every signed-in user's
    // own-profile read starts 42501-ing. That is the schema block's exit
    // condition, and this is the tripwire on it.
    const grant = /REVOKE SELECT ON public\.profiles FROM ([a-z, ]+);/.exec(SCHEMA);
    expect(grant, 'F34 REVOKE not found in schema.sql').not.toBeNull();
    expect(grant![1].trim()).toBe('anon');
  });
});

describe('F34 — every anon-reachable reader stays inside the grant', () => {
  // The direction F73 did not have to worry about. All three readers below are
  // invoker-rights, so a column they touch but anon cannot read is not a quiet
  // omission — it is a failed read for every anonymous visitor.

  it('the view reads only granted columns of profiles, join predicate included', () => {
    const all = [...SCHEMA.matchAll(/CREATE VIEW shared_builds_with_author[\s\S]*?;/g)];
    const view = all[all.length - 1][0];
    // Deliberately the WHOLE statement, not just the SELECT list: `LEFT JOIN
    // profiles p ON p.user_id = b.user_id` needs SELECT on p.user_id as much
    // as the projection does.
    expect(profileColumnsVia('p', view)).toEqual(
      expect.arrayContaining(['handle', 'display_name', 'avatar_url', 'user_id']),
    );
    for (const column of profileColumnsVia('p', view)) {
      expect(grantedColumns('profiles'), `view reads p.${column}`).toContain(column);
    }
  });

  it('search_authors reads only granted columns of profiles', () => {
    for (const column of profileColumnsVia('p', functionBody('search_authors'))) {
      expect(grantedColumns('profiles'), `search_authors reads p.${column}`).toContain(column);
    }
  });

  it('resolve_author returns only granted columns of profiles', () => {
    for (const column of resolveAuthorColumns()) {
      expect(grantedColumns('profiles'), `resolve_author returns ${column}`).toContain(column);
    }
  });

  it('the grant is exactly the union of the three, and carries nothing spare', () => {
    // This is the sentence the schema block makes: the list is derived, not
    // chosen. If it ever grows a column no reader needs, that column was
    // published by decision and the decision should be written down first.
    const all = [...SCHEMA.matchAll(/CREATE VIEW shared_builds_with_author[\s\S]*?;/g)];
    const union = new Set([
      ...profileColumnsVia('p', all[all.length - 1][0]),
      ...profileColumnsVia('p', functionBody('search_authors')),
      ...resolveAuthorColumns(),
    ]);
    expect(new Set(grantedColumns('profiles'))).toEqual(union);
  });
});

describe('F34 — search_authors cannot go back to answering an empty query', () => {
  const body = () => functionBody('search_authors');

  it('rejects a query under two characters, whitespace not counted', () => {
    // `q = ''` degenerated to ILIKE '%%' and returned all 530 rows in one call.
    // btrim so that ' ' and '  ' are rejected too; COALESCE so that NULL is.
    expect(body()).toMatch(/char_length\(btrim\(COALESCE\(q, ''\)\)\) >= 2/);
  });

  it('matches on the raw q, so trimming did not change what a real query finds', () => {
    // The guard trims; the predicates must not. Trimming the match too would
    // be a behaviour change smuggled in beside a security fix.
    expect(body()).toMatch(/p\.display_name ILIKE '%' \|\| q \|\| '%'/);
  });

  it('clamps the caller-supplied limit', () => {
    // q04 passed lim = 1000000 and was served. Both clients ask for 8.
    expect(body()).toMatch(/LIMIT LEAST\(GREATEST\(COALESCE\(lim, 10\), 1\), 25\)/);
  });

  it('is still invoker-rights, which is what makes the grant bind on it', () => {
    // A SECURITY DEFINER here would run as the owner and read every column of
    // profiles regardless of what anon is granted — it would reopen F34
    // through the RPC while the grant above still looked correct.
    expect(body()).not.toMatch(/SECURITY DEFINER/);
    expect(functionBody('resolve_author')).not.toMatch(/SECURITY DEFINER/);
  });
});
