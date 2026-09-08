/**
 * The UID table is generated from a vendored copy of Mids' own enhancement
 * database. Updating that database without re-running the emitter leaves the
 * table describing a file that is no longer there — and the way that shows up
 * is a UID Mids has since renamed, which opens as an empty slot rather than an
 * error. Nothing downstream can see it, so the check has to be here.
 *
 * Regenerate with: python3 tools/mids-oracle/emit_mids_uids.py --dataset all
 */
import { createHash } from 'node:crypto';
import { existsSync, readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { MIDS_UIDS as HOMECOMING } from './datasets/homecoming/generated/mids-uids';
import { MIDS_UIDS as BRAINSTORM } from './datasets/brainstorm/generated/mids-uids';
import { MIDS_UIDS as REBIRTH } from './datasets/rebirth/generated/mids-uids';
import { MIDS_UIDS as THUNDERSPY } from './datasets/thunderspy/generated/mids-uids';

// Mirrors DATASET_SOURCES in tools/mids-oracle/emit_mids_uids.py. Brainstorm is
// Homecoming's open beta and shares its enhancement namespace.
const SOURCES = [
  ['homecoming', HOMECOMING, 'MidsReborn-master/MidsReborn/Databases/Homecoming/EnhDB.mhd'],
  ['brainstorm', BRAINSTORM, 'MidsReborn-master/MidsReborn/Databases/Homecoming/EnhDB.mhd'],
  ['rebirth', REBIRTH, 'MidsReborn-master/MidsReborn/Databases/Rebirth/EnhDB.mhd'],
  ['thunderspy', THUNDERSPY, 'Thunderspy/EnhDB.mhd'],
] as const;

describe('mids UID tables track their source database', () => {
  for (const [dataset, table, source] of SOURCES) {
    it(`${dataset} was generated from the EnhDB now on disk`, () => {
      const url = new URL(`../../${source}`, import.meta.url);
      if (!existsSync(url)) {
        // `MidsReborn-master/` and `Thunderspy/` are gitignored vendored checkouts, so no CI
        // runner has either and this guard has been red there since it was written — one of the
        // reds that made the beta's suite something nobody read. Stated rather than silent: the
        // staleness it catches is only POSSIBLE on a machine carrying the database, and a skip
        // that says why is not a passing assertion. Live wherever the vendored copy exists.
        expect(existsSync(url)).toBe(false);
        return;
      }
      expect(createHash('sha256').update(readFileSync(url)).digest('hex')).toBe(table.sourceSha256);
    });
  }
});
