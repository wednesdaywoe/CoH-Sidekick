/**
 * SECURITY_AUDIT.md F82, F83 — the signup seed cannot refuse the signup.
 *
 * `profiles.display_name` carries `CHECK (char_length(display_name) <= 30)`,
 * the seed trigger wrote a provider-supplied name into it untruncated, and the
 * trigger fires in the signing-up transaction — so an over-long Discord global
 * name aborted the account, not just the profile.
 *
 * The behaviour itself is graded by executing it, against the throwaway
 * Postgres that `supabase/audit/fixture/build.sh` builds from this same file:
 * `check-f82-seed.sql` there shows the truncation, the stripped sigil and the
 * character-rather-than-byte cut, and shows the user surviving. That needs
 * docker, so this is the half that runs in CI — it pins the shape the executed
 * check depends on, and it is what goes red if the migration is reverted.
 */
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';

const schema = readFileSync(new URL('./schema.sql', import.meta.url), 'utf8');

/** The last definition wins in a file that migrates by CREATE OR REPLACE. */
const lastSeedBody = schema.slice(schema.lastIndexOf('CREATE OR REPLACE FUNCTION seed_profile_on_signup'));

describe('seeded_display_name (F82)', () => {
  it('exists, and is the one copy the seeding rule has', () => {
    expect(schema).toContain('CREATE OR REPLACE FUNCTION seeded_display_name');
    expect(schema).toContain("left(regexp_replace(btrim(COALESCE(raw, '')), '^@+\\s*', ''), 30)");
  });

  it('truncates to the number the column actually checks', () => {
    // The two numbers are compared rather than both pinned to 30: a cap here
    // that is larger than the CHECK re-opens F82 exactly, and pinning each
    // literal separately would let them be raised one at a time.
    const checked = schema.match(/CHECK \(char_length\(display_name\) <= (\d+)\)/);
    const truncated = schema.match(/^\s*SELECT left\(.*, (\d+)\);$/m);
    expect(checked?.[1]).toBeDefined();
    expect(truncated?.[1]).toBeDefined();
    expect(truncated![1]).toBe(checked![1]);
  });

  it('cuts by character, not by byte, so an astral name is not halved', () => {
    // `left()` is character-based; `substring(... for N bytes)` would not be.
    expect(schema).not.toMatch(/substr\w*\([^)]*display_name/i);
  });
});

describe('the seed trigger asks it (F82, F83)', () => {
  it('routes the provider name through the rule', () => {
    expect(lastSeedBody).toContain('seeded_display_name(COALESCE(');
  });

  it('is the definition that wins, after the original', () => {
    const first = schema.indexOf('CREATE OR REPLACE FUNCTION seed_profile_on_signup');
    const last = schema.lastIndexOf('CREATE OR REPLACE FUNCTION seed_profile_on_signup');
    expect(last).toBeGreaterThan(first);
    expect(lastSeedBody).toContain('seeded_display_name');
  });

  it('still seeds the columns it always did', () => {
    // A fix that quietly stopped seeding an avatar would pass every check
    // above and break the author card.
    for (const column of ['discord_id', 'discord_username', 'avatar_url']) {
      expect(lastSeedBody).toContain(column);
    }
    expect(lastSeedBody).toContain('storable_avatar_url(');
    expect(lastSeedBody).toContain('ON CONFLICT (user_id) DO NOTHING');
  });
});
