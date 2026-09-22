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

/** Last definition wins in a file that migrates by CREATE OR REPLACE. */
const lastIncrementViews = schema.slice(schema.lastIndexOf('CREATE OR REPLACE FUNCTION increment_views'));

describe('increment_views counts only what answers by id (F33)', () => {
  it('is bounded to the readable visibilities', () => {
    expect(lastIncrementViews).toContain("AND visibility IN ('public', 'unlisted')");
  });

  it('is still the id lookup it was, not a broader update', () => {
    // A fix that widened the UPDATE while narrowing the visibility would be
    // worse than the finding.
    expect(lastIncrementViews).toContain('WHERE id = build_id');
    expect(lastIncrementViews).toContain('SET views = views + 1');
  });

  it('draws the same line preview-visibility.ts and get-build draw', () => {
    // public + unlisted answer by id; private does not. Three files, one line.
    const preview = readFileSync(
      new URL('./functions/_shared/preview-visibility.ts', import.meta.url), 'utf8');
    expect(preview).toContain("visibility === 'public' || visibility === 'unlisted'");
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
