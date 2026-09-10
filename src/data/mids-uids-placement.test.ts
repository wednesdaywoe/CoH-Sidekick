/**
 * Where each piece UID SITS in its set, across all four forks.
 *
 * The emitter keys a piece on its trailing letter, because both directions do.
 * A member Mids gives no letter — `..._Rez_Effects` — has to be placed some
 * other way, and for as long as this table existed that way was "the last free
 * slot". That is right for a set with one hole and wrong for a set with two:
 * Rebirth's Return From the Grave lists its sixth record as a byte-copy of its
 * fifth, so the letter key drops it, slots 1 and 6 both come free, and the rez
 * proc — Mids' FIRST member — landed on our piece 6, Recharge. Silently, in
 * both directions, because the import path reads this same table (MBDEXPORT-10).
 *
 * Nothing downstream could see it. The export sweep in `mids-export.test.ts`
 * asks whether the UID we emit is one Mids HAS, and the mis-bound one is; the
 * round trip asks whether the table is an injection, and a swap of two slots
 * still is. Both stayed green. What tells the two apart is the placement, so
 * that is what this pins — per fork, by name, so a second mislettered set
 * arrives red rather than mis-bound.
 *
 * Regenerate with: python3 tools/mids-oracle/emit_mids_uids.py --dataset all
 */
import { describe, expect, it } from 'vitest';
import type { MidsUidTable } from './mids-uids';
import { MIDS_UIDS as HOMECOMING } from './datasets/homecoming/generated/mids-uids';
import { MIDS_UIDS as BRAINSTORM } from './datasets/brainstorm/generated/mids-uids';
import { MIDS_UIDS as REBIRTH } from './datasets/rebirth/generated/mids-uids';
import { MIDS_UIDS as THUNDERSPY } from './datasets/thunderspy/generated/mids-uids';

const TABLES: [string, MidsUidTable][] = [
  ['homecoming', HOMECOMING],
  ['brainstorm', BRAINSTORM],
  ['rebirth', REBIRTH],
  ['thunderspy', THUNDERSPY],
];

/** `setId piece N` for every slot whose UID is the empty string. */
function holes(table: MidsUidTable): string[] {
  return Object.entries(table.ioSetPieces)
    .flatMap(([setId, pieces]) => pieces.map((uid, i) => (uid ? null : `${setId} piece ${i + 1}`)))
    .filter((s): s is string => s !== null)
    .sort();
}

/**
 * `setId piece N → UID` for every slot whose UID does not end in the letter its
 * position names.
 *
 * A lettered UID lands on its own letter by construction, so this census is
 * exactly the set of members placed by something OTHER than their letter —
 * which is the population the last-free-slot rule could get wrong.
 *
 * Case-folded, because the emitter's own key is (`piece_index` uppercases) and
 * this has to isolate the same population it does. Mids spells Exploit
 * Weakness's third piece `_c`; that is a fossil of its own, not a placement.
 */
function offLetter(table: MidsUidTable): string[] {
  return Object.entries(table.ioSetPieces)
    .flatMap(([setId, pieces]) =>
      pieces.map((uid, i) =>
        !uid || uid.slice(-2).toUpperCase() === `_${String.fromCharCode(65 + i)}`
          ? null
          : `${setId} piece ${i + 1} → ${uid}`,
      ),
    )
    .filter((s): s is string => s !== null)
    .sort();
}

describe('mids UID tables place every piece where Mids does', () => {
  /**
   * Rebirth's pair, and nothing else in four databases. Both are Return From
   * the Grave; both put the unlettered rez proc at piece 1, which is where it
   * sits in Mids' own member list and what our piece 1 is called. The superior
   * twin never needed the fix — its letters run B..F, one hole, and last-free
   * and by-position give the same answer — so it is here as the control: the
   * two sets agree, and the day they stop, one of them moved.
   */
  const OFF_LETTER: Record<string, string[]> = {
    homecoming: [],
    brainstorm: [],
    rebirth: [
      'return_from_the_grave piece 1 → Return_From_the_Grave_Rez_Effects',
      'superior_return_from_the_grave piece 1 → Superior_Return_From_the_Grave_Rez_Effects',
    ],
    thunderspy: [],
  };

  /**
   * A slot Mids' own database cannot name. One, and it is the other half of the
   * same record: Rebirth's Return From the Grave has `_E` twice where the
   * superior twin has `_F`, so its Recharge piece has no UID to go out under.
   * The export reports that slot rather than emitting one — `UNNAMEABLE` in
   * `mids-export.test.ts` is the same fact from the writer's side, and the two
   * lists have to move together.
   */
  const HOLES: Record<string, string[]> = {
    homecoming: [],
    brainstorm: [],
    rebirth: ['return_from_the_grave piece 6'],
    thunderspy: [],
  };

  for (const [dataset, table] of TABLES) {
    describe(dataset, () => {
      it('places a member Mids gave no letter by its position in Mids’ member list', () => {
        expect(offLetter(table)).toEqual(OFF_LETTER[dataset]);
      });

      it('leaves a slot Mids cannot name empty rather than filling it from a neighbour', () => {
        expect(holes(table)).toEqual(HOLES[dataset]);
      });

      it('never spells two slots with one UID', () => {
        // The round trip in `mids-export.test.ts` asserts this through the two
        // converters, on Homecoming alone. Here it is on the table itself, on
        // every fork: a duplicate is how a placement rule that guesses returns
        // a build with one piece twice and another missing.
        const seen = new Map<string, string>();
        const collisions: string[] = [];
        for (const [setId, pieces] of Object.entries(table.ioSetPieces)) {
          pieces.forEach((uid, i) => {
            if (!uid) return;
            const here = `${setId} piece ${i + 1}`;
            const first = seen.get(uid);
            if (first) collisions.push(`${uid}: ${first} and ${here}`);
            else seen.set(uid, here);
          });
        }
        expect(collisions).toEqual([]);
      });
    });
  }
});
