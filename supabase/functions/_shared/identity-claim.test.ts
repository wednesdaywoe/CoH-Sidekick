/**
 * SECURITY_AUDIT.md F83 — the identity-claim rule, and both columns asking it.
 *
 * F69 applied this to `shared_builds.author_name`. F83 is that it stopped
 * there: `profiles.display_name` is the same kind of string, rendered in the
 * same author surfaces, written through a service-role path, and it had a
 * `.trim()` and a length check. Measured 2026-09-18, `author_name` held 0
 * claims on somebody else's handle and `display_name` held 1.
 */
import { describe, it, expect, vi } from 'vitest';
import { readFileSync } from 'node:fs';
import { identityClaimRefusal, type HandleLookup } from './identity-claim';
import { handleCandidate, normalizeIdentityName, MAX_DISPLAY_NAME } from './author-name';

const read = (name: string) =>
  readFileSync(new URL(`../${name}/index.ts`, import.meta.url), 'utf8');

const lookupOf = (result: Partial<HandleLookup>) =>
  vi.fn(async (): Promise<HandleLookup> => ({ reserved: false, claimedBy: null, ...result }));

describe('identityClaimRefusal', () => {
  it('refuses one of the service\'s own names', async () => {
    const refusal = await identityClaimRefusal(
      'admin', null, handleCandidate, lookupOf({ reserved: true }), 'display name');
    expect(refusal).toContain('reserved name');
    expect(refusal).toContain('display name');
  });

  it('refuses a handle a different account has proved it holds', async () => {
    const refusal = await identityClaimRefusal(
      'savant', 'user-a', handleCandidate, lookupOf({ claimedBy: 'user-b' }), 'author name');
    expect(refusal).toContain('registered account');
    expect(refusal).toContain('author name');
  });

  it('lets an account use its own handle as its name', async () => {
    expect(await identityClaimRefusal(
      'savant', 'user-a', handleCandidate, lookupOf({ claimedBy: 'user-a' }), 'display name',
    )).toBeNull();
  });

  it('names the field it was asked about, because these are two forms', async () => {
    const asAuthor = await identityClaimRefusal(
      'admin', null, handleCandidate, lookupOf({ reserved: true }), 'author name');
    expect(asAuthor).toContain('author name');
    expect(asAuthor).not.toContain('display name');
  });

  it('spends no round trip on a name that cannot be a handle', async () => {
    // Two people called "Savant Administrator" are just two people; a name
    // with a space in it cannot collide with a handle however it is cased.
    const lookup = lookupOf({ reserved: true });
    expect(await identityClaimRefusal(
      'Savant Administrator', null, handleCandidate, lookup, 'display name',
    )).toBeNull();
    expect(lookup).not.toHaveBeenCalled();
  });
});

describe('normalizeIdentityName is F69\'s rule without the truncation (F83)', () => {
  it('strips a claimed @ sigil', () => {
    expect(normalizeIdentityName('@savant')).toBe('savant');
    expect(normalizeIdentityName(' @savant ')).toBe('savant');
  });

  it('removes invisibles but turns separators into spaces', () => {
    // Deleting the newlines would weld two words into a name nobody typed.
    expect(normalizeIdentityName('Savant\n\nAdministrator')).toBe('Savant Administrator');
    expect(normalizeIdentityName('Sav​ant')).toBe('Savant');
  });

  it('does not truncate, which is what separates it from sanitizeAuthorName', () => {
    const long = 'x'.repeat(200);
    expect(normalizeIdentityName(long)).toHaveLength(200);
  });

  it('answers empty for anything that is not a string', () => {
    expect(normalizeIdentityName(null)).toBe('');
    expect(normalizeIdentityName(42)).toBe('');
  });
});

describe('both identity columns ask the rule (F83)', () => {
  it('update-profile normalises display_name instead of trimming it', () => {
    const source = read('update-profile');
    expect(source).toContain('normalizeIdentityName(body.display_name)');
    expect(source).not.toMatch(/String\(body\.display_name\)\.trim\(\)/);
  });

  it('update-profile measures the cap in code points, not UTF-16 units', () => {
    // The defect F69 found in `.slice(0, 50)`, one column over.
    expect(read('update-profile')).toContain('Array.from(dn).length > MAX_DISPLAY_NAME');
  });

  it('update-profile runs the claim check before it writes', () => {
    const source = read('update-profile');
    const checked = source.indexOf("'display name',");
    const written = source.indexOf('updates.display_name = dn;');
    expect(checked).toBeGreaterThan(-1);
    expect(written).toBeGreaterThan(checked);
  });

  it('share-build keeps no second copy of the rule', () => {
    const source = read('share-build');
    expect(source).toContain('identityClaimRefusal(');
    expect(source).not.toContain('is a reserved name.');
    expect(source).not.toContain('is the handle of a registered account.');
  });

  it('the cap is one constant, not a literal 30 in the function', () => {
    expect(MAX_DISPLAY_NAME).toBe(30);
    expect(read('update-profile')).not.toMatch(/dn\.length > 30/);
  });
});
