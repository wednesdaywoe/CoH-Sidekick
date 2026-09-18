import { describe, it, expect } from 'vitest';
import { authorIdentity } from './author-identity';

describe('authorIdentity', () => {
  it('pairs a proved handle with the name the share carried', () => {
    expect(authorIdentity('Tiger Eyes', 'tigereyes')).toEqual({
      kind: 'verified',
      display: 'Tiger Eyes',
      handle: 'tigereyes',
    });
  });

  it('is unverified when nothing proves the name', () => {
    expect(authorIdentity('Tiger Eyes', null)).toEqual({
      kind: 'unverified',
      display: 'Tiger Eyes',
    });
  });

  it('is anonymous only when both columns are empty', () => {
    expect(authorIdentity('', null)).toEqual({ kind: 'anonymous' });
    expect(authorIdentity('   ', undefined)).toEqual({ kind: 'anonymous' });
    expect(authorIdentity(null, null)).toEqual({ kind: 'anonymous' });
  });

  it('gives an owner who named no author their handle rather than Anonymous', () => {
    // 101 of the 1,328 public rows with a verified author carry no author_name,
    // and every one of them drew "Anonymous" before this.
    expect(authorIdentity('', 'tigereyes')).toEqual({
      kind: 'verified',
      display: '',
      handle: 'tigereyes',
    });
  });

  // ---- the finding ----

  it('never lets a free-text name render as a handle', () => {
    // 64 stored rows open with `@`, and one names a handle that exists on an
    // account other than the (absent) sharer's. Whatever was typed and whoever
    // typed it, the line may not read as somebody's handle — this client
    // cannot tell the readings apart, so the rule is unconditional.
    for (const claim of ['@tigereyes', ' @tigereyes', '@@tigereyes', '@ @tigereyes', '  @  @  tigereyes']) {
      expect(authorIdentity(claim, null)).toEqual({
        kind: 'unverified',
        display: 'tigereyes',
      });
    }
  });

  it('puts an @ on the line only when the handle put it there', () => {
    const names = ['@tigereyes', '@@savant', 'Savant', '', '  @  ', '@ @savant'];
    const handles = [null, undefined, '', '   ', '@', 'tigereyes', '@TigerEyes'];
    for (const name of names) {
      for (const handle of handles) {
        const identity = authorIdentity(name, handle);
        // What the markup produces: the sigil is written in front of the
        // handle and nowhere else.
        const drawn =
          identity.kind === 'anonymous'
            ? ''
            : identity.kind === 'unverified'
              ? identity.display
              : `${identity.display} @${identity.handle}`;
        const expected = identity.kind === 'verified' ? 1 : 0;
        expect(
          (drawn.match(/@/g) ?? []).length,
          `name ${JSON.stringify(name)} + handle ${JSON.stringify(handle)} drew ${JSON.stringify(drawn)}`,
        ).toBe(expected);
      }
    }
  });

  it('leaves an @ that is not the sigil alone', () => {
    // A name is not a claim because it contains an @; it is one because it
    // opens with one.
    expect(authorIdentity('tiger@eyes', null)).toEqual({
      kind: 'unverified',
      display: 'tiger@eyes',
    });
  });

  it('keeps a display name case and lowercases a handle', () => {
    // The handle is the key `/author/$handle` routes on; the name is a label.
    expect(authorIdentity('Savant', '@TigerEyes')).toEqual({
      kind: 'verified',
      display: 'Savant',
      handle: 'tigereyes',
    });
  });

  it('treats an empty handle column as no handle at all', () => {
    // An empty handle would otherwise link to /author/ and draw a bare @.
    expect(authorIdentity('Savant', '')).toEqual({ kind: 'unverified', display: 'Savant' });
    expect(authorIdentity('Savant', '@')).toEqual({ kind: 'unverified', display: 'Savant' });
  });
});
