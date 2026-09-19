/**
 * Grades `descriptionRuns` — SECURITY_AUDIT.md F77.
 *
 * The first block is the bypass the finding names. The rest is the ordinary
 * rendering, which is here because a sanitiser that eats real descriptions gets
 * reverted and the hole comes back with it.
 */

import { describe, it, expect } from 'vitest';
import { descriptionRuns } from './powerDescription';

describe('descriptionRuns', () => {
  it('does not let an unterminated tag reach the renderer as markup', () => {
    // The old chain stripped `/<[^>]+>/g`, so this survived whole and then
    // borrowed the `>` from the NOTE span appended after it.
    const runs = descriptionRuns('Deals fire damage. <img src=x onerror=alert(1) NOTE: stacks to 3.');
    const rendered = runs.map(r => r.text).join('');
    expect(rendered).toContain('<img src=x onerror=alert(1)');
    // It is TEXT now, not a tag: nothing downstream parses these strings, and
    // React escapes them. The assertion that matters is that the runs carry no
    // markup of their own for it to complete.
    expect(runs.some(r => r.text.includes('<span'))).toBe(false);
    expect(runs.find(r => r.note)?.text).toBe('NOTE: stacks to 3.');
  });

  it('flattens the markup the dataset really contains', () => {
    expect(descriptionRuns('First line.<br>Second line.')).toEqual([
      { note: false, text: 'First line. Second line.' },
    ]);
    expect(descriptionRuns('a<BR />b')).toEqual([{ note: false, text: 'a b' }]);
    expect(descriptionRuns('bold <b>text</b> here')).toEqual([
      { note: false, text: 'bold text here' },
    ]);
  });

  it('splits a NOTE out of the surrounding prose', () => {
    expect(descriptionRuns('Ranged blast. NOTE: unaffected by recharge. Fires fast.')).toEqual([
      { note: false, text: 'Ranged blast. ' },
      { note: true, text: 'NOTE: unaffected by recharge.' },
      { note: false, text: ' Fires fast.' },
    ]);
  });

  it('terminates a NOTE that runs to the end of the description', () => {
    expect(descriptionRuns('Melee. NOTE: not enhanceable')).toEqual([
      { note: false, text: 'Melee. ' },
      { note: true, text: 'NOTE: not enhanceable.' },
    ]);
  });

  it('carries more than one NOTE', () => {
    const runs = descriptionRuns('NOTE: first. Middle. NOTE: second.');
    expect(runs.filter(r => r.note).map(r => r.text)).toEqual([
      'NOTE: first.',
      'NOTE: second.',
    ]);
  });

  it('returns nothing for an empty description rather than an empty run', () => {
    expect(descriptionRuns('')).toEqual([]);
  });
});
