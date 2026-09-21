import { describe, it, expect } from 'vitest';
import { PROC_DATABASE } from '@/data/proc-data';
import { PROC_DAMAGE_EFFECTS } from '@/data/generated/proc-damage.generated';

/**
 * A damage proc's row states the same fact twice: `effects[]`, generated from
 * the boost template by scripts/extract-proc-data.py, and the hand-written
 * `mechanics` prose the tooltip prints above it. Both reach the contract, so
 * when they disagree the planner shows a user two answers in one line.
 *
 * PPM-6 is what that costs. Bombardment's proc is `Fire_Dmg` in the export and
 * its prose read "Damage(Energy 7 - 72)", so the tooltip printed
 * "Chance for Fire Damage (Damage Energy 7-72) Dmg: Fire 71.75" for seven
 * weeks. This guard existed and was green throughout: it read the NUMBERS
 * beside the type and never the type, and the numbers were right.
 *
 * So both axes are graded here, and the allowlist is gone. It held seven rows —
 * Ice Mistral's min, entered as 10 against a 0.67 scale, and six ATO procs
 * carrying a flat level-50 value where the effect scales — all of which were
 * corrections the prose should have taken rather than exceptions the guard
 * should carry. A row that needs an exception needs a fix or a reason; an
 * allowlist entry is neither. See PROC-DATA-BINARY-SOURCING.md and
 * docs/gaps/procs-ppm.md.
 */
const HAND = /Damage\s*\(\s*([\w ]+?)\s+(\d+)(?:\s*-\s*(\d+))?\s*\)/;

describe('damage proc structured-vs-mechanics parity', () => {
  const unexpected: string[] = [];

  for (const [key, effects] of Object.entries(PROC_DAMAGE_EFFECTS)) {
    const entry = PROC_DATABASE[key];
    if (!entry) { unexpected.push(`missing entry: ${key}`); continue; }
    const m = entry.mechanics.match(HAND);
    if (!m) { unexpected.push(`hand mechanics not Damage(...): ${key} -> ${entry.mechanics}`); continue; }

    const e = effects[0];

    // The damage TYPE, which is the axis PPM-6 was wrong on. Compared exactly:
    // "Negative" for Negative Energy is the near-miss that makes a fuzzy match
    // useless here, since every wrong answer is also a real damage type.
    const handType = m[1].trim();
    if (handType !== e.effectType) {
      unexpected.push(`${key}\n  hand type: ${handType}  gen: ${e.effectType}`);
    }

    // Generated values carry 2-decimal precision (e.g. 6.7 / 71.75); the hand
    // mechanics strings are integer "N - M". Compare on the rounded integers.
    const handN = parseInt(m[2], 10);
    const handM = m[3] ? parseInt(m[3], 10) : handN;
    const genN = Math.round(e.value ?? NaN);
    const genM = Math.round(e.valueMax ?? NaN);
    if (genN !== handN || genM !== handM) {
      unexpected.push(`${key}\n  hand: ${handN}-${handM}  gen: ${e.value}-${e.valueMax}`);
    }
  }

  it('damage type and N-M both match the generated effect', () => {
    expect(unexpected, `Unexpected damage parity diffs:\n${unexpected.join('\n')}`).toEqual([]);
  });

  // Without this, deleting PROC_DAMAGE_EFFECTS' contents leaves the loop above
  // grading nothing and passing. The count is the corpus as of PPM-6.
  it('grades every damage proc in the generated table', () => {
    expect(Object.keys(PROC_DAMAGE_EFFECTS).length).toBeGreaterThanOrEqual(36);
  });
});
