/**
 * Grades `mayWriteBuild` and `mayClaimBuild` — SECURITY_AUDIT.md F30, F31, F80.
 *
 * The cases that matter are the two directions of the same rule: a token must
 * still work where it is the only credential, and must stop working where an
 * account exists. A test suite that only checked the second would pass a
 * function that returns `false` always, and that function locks every anonymous
 * sharer out of their own build.
 */

import { describe, it, expect } from 'vitest';
import { mayWriteBuild, mayClaimBuild } from './build-ownership';

const ALICE = '11111111-1111-1111-1111-111111111111';
const MALLORY = '22222222-2222-2222-2222-222222222222';

describe('mayWriteBuild', () => {
  it('lets the token stand alone on an unclaimed build', () => {
    // The anonymous share. There is no account to fall back on, so removing
    // this branch would strand every token-only owner.
    expect(mayWriteBuild({ buildUserId: null, tokenMatches: true, authUserId: null })).toBe(true);
  });

  it('refuses a token that does not match, claimed or not', () => {
    expect(mayWriteBuild({ buildUserId: null, tokenMatches: false, authUserId: null })).toBe(false);
    expect(mayWriteBuild({ buildUserId: ALICE, tokenMatches: false, authUserId: null })).toBe(false);
  });

  it('refuses a stranger holding the token of a claimed build', () => {
    // F80's actual shape: the token was minted at share time and the build was
    // claimed afterwards, so both credentials exist and they disagree.
    expect(mayWriteBuild({ buildUserId: ALICE, tokenMatches: true, authUserId: null })).toBe(false);
    expect(mayWriteBuild({ buildUserId: ALICE, tokenMatches: true, authUserId: MALLORY })).toBe(false);
  });

  it('lets the owning account through with no token at all', () => {
    expect(mayWriteBuild({ buildUserId: ALICE, tokenMatches: false, authUserId: ALICE })).toBe(true);
  });

  it('lets a logged-in caller use the token of a build they never claimed', () => {
    // Shared anonymously, logged in later, has not pressed Claim yet.
    expect(mayWriteBuild({ buildUserId: null, tokenMatches: true, authUserId: MALLORY })).toBe(true);
  });
});

describe('mayClaimBuild', () => {
  it('claims an unclaimed build for a token holder who is logged in', () => {
    expect(mayClaimBuild({ buildUserId: null, tokenMatches: true, authUserId: ALICE })).toBe(true);
  });

  it('is idempotent for the account that already owns it', () => {
    // claim-builds re-sends the whole localStorage map on every login, so a
    // re-claim has to report success rather than failure.
    expect(mayClaimBuild({ buildUserId: ALICE, tokenMatches: true, authUserId: ALICE })).toBe(true);
  });

  it('refuses to transfer a build away from the account holding it', () => {
    // F31. The old code ran the UPDATE anyway and moved the row to Mallory.
    expect(mayClaimBuild({ buildUserId: ALICE, tokenMatches: true, authUserId: MALLORY })).toBe(false);
  });

  it('refuses an anonymous caller, who has no account to claim into', () => {
    expect(mayClaimBuild({ buildUserId: null, tokenMatches: true, authUserId: null })).toBe(false);
  });

  it('refuses a non-matching token', () => {
    expect(mayClaimBuild({ buildUserId: null, tokenMatches: false, authUserId: ALICE })).toBe(false);
  });
});
