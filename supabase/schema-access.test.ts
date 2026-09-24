/**
 * SECURITY_AUDIT.md F33, F53 — two paths that answered about rows the caller
 * cannot read.
 *
 * Both are SECURITY DEFINER in effect: `increment_views` is declared that way,
 * and a foreign key is checked by a system trigger running as the constraint's
 * owner. Both therefore saw past RLS, and both told a caller something about a
 * private build.
 *
 * The behaviour is graded by execution, in
 * `supabase/audit/fixture/check-f33-f53-access.sql`, against the throwaway
 * Postgres that `fixture/build.sh` derives from this same schema. That needs
 * docker; this is the half that runs in CI and goes red on a revert.
 */
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';

const schema = readFileSync(new URL('./schema.sql', import.meta.url), 'utf8');

/**
 * Last definition wins in a file that migrates by CREATE OR REPLACE — and it
 * ENDS at its own terminator, which this did not do before 2026-09-23.
 *
 * It was `schema.slice(lastIndexOf(...))`, i.e. everything to the end of the
 * file, which is harmless for a `toContain` and silently wrong for a
 * `not.toContain`: the first negative assertion written against it failed on
 * `auth.uid()` appearing in a DIFFERENT function three migrations further
 * down. A span that only works for positive claims is a span that will pass
 * the next negative one for the wrong reason.
 */
function lastDefinitionOf(name: string): string {
  const start = schema.lastIndexOf(`CREATE OR REPLACE FUNCTION ${name}`);
  if (start < 0) throw new Error(`no definition of ${name} in schema.sql`);
  const end = schema.indexOf('$$ LANGUAGE', start);
  if (end < 0) throw new Error(`definition of ${name} has no terminator`);
  return schema.slice(start, end);
}

const lastIncrementViews = lastDefinitionOf('increment_views');

describe('increment_views counts only what answers by id (F33)', () => {
  it('is bounded to the readable visibilities', () => {
    expect(lastIncrementViews).toContain("AND visibility IN ('public', 'unlisted')");
  });

  it('is still the id lookup it was, not a broader update', () => {
    // A fix that widened the UPDATE while narrowing the visibility would be
    // worse than the finding. `target` is the parameter under another name --
    // `build_id` became ambiguous once build_views gained a column of that
    // name -- so this reads the same claim against the current spelling.
    expect(lastIncrementViews).toContain('WHERE id = target');
    expect(lastIncrementViews).toContain('SET views = views + 1');
    // One row at a time, still. An UPDATE with no id predicate would count
    // every build in the table on one call.
    expect(lastIncrementViews).not.toMatch(/UPDATE shared_builds\s+SET views = views \+ 1;/);
  });

  it('draws the same line preview-visibility.ts and get-build draw', () => {
    // public + unlisted answer by id; private does not. Three files, one line.
    const preview = readFileSync(
      new URL('./functions/_shared/preview-visibility.ts', import.meta.url), 'utf8');
    expect(preview).toContain("visibility === 'public' || visibility === 'unlisted'");
  });
});

/**
 * F33's second clause — the one the 2026-09-22 pass left open, and the reason
 * the row stayed PARTIAL: "anyone who can view a public build can call the RPC
 * as often as they like."
 *
 * These are source guards. The BEHAVIOUR is graded by execution, in
 * `supabase/audit/fixture/check-f33-f53-access.sql`, which is where the count
 * is actually watched not moving — and where the first draft of this fix was
 * caught raising on every call, because `ON CONFLICT (build_id, viewer)`
 * resolves its column list against plpgsql variables and the parameter was
 * called `build_id`. That is a run-time error in a function that CREATEs
 * cleanly, so reading the source could not have found it and this file cannot
 * either. What these hold is the shape, against a revert.
 */
describe('increment_views counts a viewer once a day, not once a click (F33)', () => {
  it('meters the caller before it counts them', () => {
    expect(lastIncrementViews).toContain('INSERT INTO build_views');
    expect(lastIncrementViews).toContain('ON CONFLICT ON CONSTRAINT build_views_pkey DO NOTHING');
    // The mechanism: a swallowed insert means this viewer is already counted,
    // and the function must return rather than fall through to the UPDATE.
    expect(lastIncrementViews).toMatch(/IF NOT FOUND THEN\s+RETURN;\s+END IF;\s+UPDATE shared_builds/);
  });

  it('identifies the caller from a header they cannot choose', () => {
    // cf-connecting-ip is set by the edge. x-forwarded-for is the fallback and
    // is a client claim, which costs nothing: forging it splits your own views
    // across buckets you invented.
    expect(lastIncrementViews).toContain("'cf-connecting-ip'");
    expect(lastIncrementViews).toContain("'x-forwarded-for'");
    // NOT the account id on the request. That is the sender's own claim, and
    // keying on it would let a caller pick a fresh bucket per click -- the
    // same defect the feedback worker's limiter is keyed away from.
    expect(lastIncrementViews).not.toContain('auth.uid()');
  });

  it('counts nothing for a caller it cannot identify', () => {
    // Failing closed. The alternative is one shared bucket that the first call
    // of the day fills for everybody, which would be worse than not counting.
    expect(lastIncrementViews).toMatch(/IF address IS NULL OR address = ''\s+THEN\s+RETURN;/);
  });

  it('stores a digest and never an address', () => {
    expect(lastIncrementViews).toContain('encode(sha256(');
    // The day is IN the digest, which is what makes the window a window: at
    // midnight UTC every viewer is new, with no rotation step to get wrong.
    expect(lastIncrementViews).toMatch(/sha256\(convert_to\(today::text \|\| '\|' \|\| address/);
    // The COLUMNS, with the comments stripped: the DDL's own comment explains
    // at length that no address is stored, and matching raw text there scores
    // the explanation rather than the table.
    const ddl = schema.slice(schema.indexOf('CREATE TABLE IF NOT EXISTS build_views'));
    const columns = ddl.slice(0, ddl.indexOf(');'))
      .split('\n').filter(line => !line.trim().startsWith('--')).join('\n');
    expect(columns).not.toMatch(/\bip\b|address/);
    expect(columns).toContain('viewer   TEXT NOT NULL');
  });

  it('keeps the metering table off anon, both ways', () => {
    // RLS with no policies is what actually refuses; the REVOKE is against a
    // Supabase project's default privileges, which GRANT ALL on new public
    // tables to anon and authenticated.
    expect(schema).toContain('ALTER TABLE build_views ENABLE ROW LEVEL SECURITY');
    expect(schema).toContain('REVOKE ALL ON public.build_views FROM anon, authenticated');
    expect(schema).not.toMatch(/CREATE POLICY[^;]*ON build_views/);
  });

  it('ages the table out, so the window does not become a retention policy', () => {
    expect(schema).toContain("'purge-build-views'");
    expect(schema).toContain("DELETE FROM public.build_views WHERE day < (now() AT TIME ZONE 'utc')::date");
  });
});

describe('favoriting refuses identically for "not yours" and "not there" (F53)', () => {
  const fn = schema.slice(schema.indexOf('CREATE OR REPLACE FUNCTION favorites_build_must_be_reachable'));

  it('gates the insert before the foreign key can answer', () => {
    // BEFORE INSERT fires ahead of the FK's AFTER ROW check, so 23503 never
    // reaches the caller.
    expect(schema).toContain('BEFORE INSERT ON favorites');
    expect(schema).toContain('EXECUTE FUNCTION favorites_build_must_be_reachable()');
  });

  it('raises one message for both cases, which is the whole fix', () => {
    const raises = fn.slice(0, fn.indexOf('$$ LANGUAGE')).match(/RAISE EXCEPTION[^;]*/g) ?? [];
    expect(raises).toHaveLength(1);
    expect(raises[0]).toContain('Build not found');
    // A message naming which case it was would undo it.
    expect(fn).not.toMatch(/RAISE EXCEPTION 'Build is private/);
    expect(fn).not.toMatch(/RAISE EXCEPTION 'No such build/);
  });

  it('asks get-build\'s question, not the RLS policy\'s', () => {
    // RLS grants public + own. Unlisted is deliberately absent from it, so a
    // trigger that asked RLS would refuse to favourite an unlisted build -
    // which is a thing people can do, and the point of unlisted.
    expect(fn).toContain("b.visibility IN ('public', 'unlisted') OR b.user_id = auth.uid()");
    expect(fn).toContain('SECURITY DEFINER');
  });

  it('keeps the foreign key, which is what makes the cascade work', () => {
    expect(schema).toContain('build_id   TEXT NOT NULL REFERENCES shared_builds(id) ON DELETE CASCADE');
  });
});
