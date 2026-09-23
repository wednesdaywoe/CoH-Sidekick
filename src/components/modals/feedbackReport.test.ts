/**
 * F88: every field a feedback report carries is named on the form, and every line on the form
 * still names a field that travels.
 *
 * The defect was never that the form lied. It disclosed the one field with a control beside it --
 * the build snapshot -- and `userId`, `userName`, `userAgent` and every diagnostic went with the
 * report unmentioned. What makes that a standing defect rather than a wording fix is that nothing
 * would have said so the next time the payload grew: a stale disclosure looks exactly like an
 * accurate one from the outside.
 */

import { describe, expect, it } from 'vitest';
import { DISCLOSED, feedbackReport } from './feedbackReport';
import type { DiagnosticsSnapshot } from '@/utils/diagnostics';

/**
 * Dotted paths to every value the payload actually serializes.
 *
 * An array contributes its ELEMENT's paths under a `[]` component rather than an index, so a list
 * of objects is walked rather than hidden behind its own name -- `pools` is strings today and the
 * shape it would grow into is the one a disclosure would miss. One element is the whole of it:
 * the rest repeat the path. `undefined` is not a leaf, because `JSON.stringify` drops it and a
 * field that does not travel is not a field the form owes the reader a sentence about.
 */
function leaves(value: unknown, at: string, into: string[]): void {
  if (value === undefined) return;
  if (Array.isArray(value)) {
    if (value.length > 0) leaves(value[0], `${at}[]`, into);
    else into.push(at);
    return;
  }
  if (value !== null && typeof value === 'object') {
    const fields = Object.entries(value as Record<string, unknown>);
    if (fields.length > 0) {
      for (const [key, field] of fields) {
        leaves(field, at === '' ? key : `${at}.${key}`, into);
      }
      return;
    }
  }
  into.push(at);
}

/**
 * Whole-component matching: `userAgent` names the top-level field and NOT
 * `diagnostics.env.userAgent`, which a bare `startsWith` would have swallowed -- and those two
 * are different facts reaching the worker by different routes.
 */
function covers(path: string, leaf: string): boolean {
  return leaf === path || leaf.startsWith(`${path}.`) || leaf.startsWith(`${path}[`);
}

/**
 * What a disclosure table gets wrong about a payload: a field that travels named on no line or on
 * more than one, and a line naming something that does not travel.
 *
 * Split out of the test so the RULES can be run against tables written to break them. Only one
 * table is ever in play against the real payload, and it exercises neither the whole-component
 * match nor the one-line-each count -- no field is a bare prefix of another and no two lines
 * overlap -- so both rules would survive a mutation pass while looking checked. That is what the
 * rebuild's own mutation pass found, one repo over.
 */
function complaints(
  table: ReadonlyArray<{ paths: readonly string[]; line: string }>,
  carried: readonly string[],
): string[] {
  const said: string[] = [];
  for (const leaf of carried) {
    const lines = table.filter(({ paths }) => paths.some((path) => covers(path, leaf))).length;
    if (lines !== 1) said.push(`${leaf} travels and is named on ${lines} lines rather than one`);
  }
  for (const { paths } of table) {
    for (const path of paths) {
      if (!carried.some((leaf) => covers(path, leaf))) {
        said.push(`the form names ${path}, and nothing by that name travels`);
      }
    }
  }
  return said;
}

const diagnostics: DiagnosticsSnapshot = {
  app: { version: '1.4.2', buildTime: '2026-09-23T00:00:00.000Z' },
  env: {
    userAgent: 'Mozilla/5.0 (X11; Linux x86_64)',
    viewport: { width: 1920, height: 1080 },
    datasetId: 'homecoming',
    url: 'https://coh-sidekick.com/build/abc',
  },
  ui: { levelUpMode: false, combatMode: true, globalIOLevel: 50 },
};

/** Everything present: the optionals, the nested groups and a non-empty array. */
function maximal() {
  return feedbackReport({
    type: 'bug',
    description: '  S/L Res totals show 0%  ',
    globalName: ' @wednesdaywoe ',
    user: { id: 'f2b1-0000', displayName: 'wednesdaywoe' },
    buildContext: {
      archetype: 'Scrapper',
      level: 50,
      primary: 'Claws',
      secondary: 'Dark Armor',
      pools: ['Speed', 'Fighting'],
      epicPool: 'Body Mastery',
      powerCount: 24,
      slotCount: 67,
    },
    buildSnapshot: '{"name":"x"}',
    diagnostics,
    userAgent: 'Mozilla/5.0 (X11; Linux x86_64)',
    timestamp: '2026-09-23T12:00:00.000Z',
  });
}

describe('the feedback form says what the feedback payload carries', () => {
  it('names every field a report carries, on exactly one line each', () => {
    const carried: string[] = [];
    leaves(JSON.parse(JSON.stringify(maximal())), '', carried);

    // The fixture is asserted maximal rather than assumed: these are the optional and the nested
    // ones, and a fixture that quietly lost one would grade a subset and pass.
    for (const present of [
      'userId',
      'userName',
      'globalName',
      'buildSnapshot',
      'diagnostics.env.userAgent',
      'diagnostics.app.version',
      'buildContext.pools[]',
    ]) {
      expect(carried, `the fixture is not maximal -- ${present} is missing`).toContain(present);
    }

    expect(complaints(DISCLOSED, carried)).toEqual([]);
  });

  it('names the three fields F88 was filed on', () => {
    // Not a restatement of the test above: that one passes on any table covering the payload,
    // including one whose sentences are neutral. These three are the fields the row says were
    // undisclosed, and this asserts the WORDS reach the reader, not merely that a path is covered.
    const words = DISCLOSED.map(({ line }) => line).join(' ').toLowerCase();
    expect(words).toContain('account');
    expect(words).toContain('display name');
    expect(words).toContain('user-agent');
  });

  it('applies the rules it states, each against a table written to break it', () => {
    const carried = ['userId', 'userAgent', 'diagnostics.env.userAgent'];
    const allThree = ['userId', 'userAgent', 'diagnostics.env.userAgent'];

    expect(complaints([{ paths: allThree, line: 'all three' }], carried)).toEqual([]);

    // A field that travels with no line naming it. This is F88 itself, in miniature.
    const missed = complaints([{ paths: ['userId'], line: 'your account' }], carried);
    expect(missed).toHaveLength(2);
    expect(missed.every((said) => said.includes('on 0 lines'))).toBe(true);

    // Two lines over one field: the reader is told twice, and the table has a copy in it that
    // only one of the two will be kept in step with.
    const twice = complaints(
      [{ paths: allThree, line: 'all three' }, { paths: ['userId'], line: 'your account, again' }],
      carried,
    );
    expect(twice).toHaveLength(1);
    expect(twice[0]).toContain('on 2 lines');

    // A line that outlived the field it named.
    const stale = complaints(
      [{ paths: allThree, line: 'all three' }, { paths: ['userEmail'], line: 'your email' }],
      carried,
    );
    expect(stale).toHaveLength(1);
    expect(stale[0]).toContain('userEmail');

    // Whole components, not characters.
    expect(covers('userAgent', 'diagnostics.env.userAgent')).toBe(false);
    expect(covers('user', 'userId')).toBe(false);
    expect(covers('diagnostics.env', 'diagnostics.env.userAgent')).toBe(true);
    expect(covers('buildContext', 'buildContext.pools[]')).toBe(true);
  });

  it('leaves the account and the diagnostics out when they are absent, and the table still holds', () => {
    // The signed-out, box-unticked report: the form must not name fields that did not travel, or
    // it overstates in the other direction. `undefined` is dropped by JSON.stringify, so those
    // lines correctly cover nothing -- and the guard has to tolerate that rather than red.
    const minimal = feedbackReport({
      type: 'other',
      description: 'hello',
      globalName: '',
      user: null,
      buildContext: {
        archetype: 'None', level: 1, primary: 'None', secondary: 'None',
        pools: [], epicPool: null, powerCount: 0, slotCount: 0,
      },
      userAgent: 'UA',
      timestamp: '2026-09-23T12:00:00.000Z',
    });
    const wire = JSON.parse(JSON.stringify(minimal));
    expect(Object.keys(wire).sort()).toEqual(
      ['buildContext', 'description', 'timestamp', 'type', 'userAgent'].sort(),
    );

    const carried: string[] = [];
    leaves(wire, '', carried);
    // Every field that DID travel is still named on exactly one line.
    const uncovered = complaints(DISCLOSED, carried).filter((said) => said.includes('travels and is named'));
    expect(uncovered).toEqual([]);
  });
});
