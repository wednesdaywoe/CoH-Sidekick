/**
 * Mids internal name → this dataset's internal name, per powerset (DATA-GAP MBDIMPORT-2).
 *
 * A `.mbd` names a power by its INTERNAL name and nothing else, and that namespace has
 * drifted: HC has rotated internal names underneath stable display names. Tactical Arrow's
 * `Gymnastics` is Oil Slick Arrow in the game data now, and the power the game displays as
 * "Gymnastics" is internally `Quickness`. Stalker Shield Defense is a clean three-cycle.
 * An importer that trusts an exact internal-name match therefore binds the wrong power
 * SILENTLY — the name it was handed does exist, it just means something else.
 *
 * The join is the DISPLAY name AND the unlock level together, and it takes both. Display
 * is the half that stayed put through every one of these reworks — Mids' `Lightning_Field`
 * carries Regeneration/Endurance/Damage and displays "Dynamo", and ours displaying "Dynamo"
 * is internally `Lightning_Clap`, the toggle DoT aura. Only the display name says so.
 *
 * But HC reuses display names too, so display alone mints false rows. Ninjitsu's old
 * Blinding Powder is called "Smoke Flash" now, which pairs it with Mids' unrelated Smoke
 * Flash — a coincidence that would have moved a real power's slots.
 *
 * The unlock level breaks that tie, and only that tie. A row is withdrawn when the power
 * it targets is already accounted for: Mids has a power of that exact internal name AND
 * the two unlock at the same level, which is an identity no display coincidence outranks.
 * Ninjitsu's `Blinding_Powder` is level 28 on both sides, so ours is spoken for and the
 * row goes. Willpower's `Reconstruction` is level 4 in Mids and 28 here — same spelling,
 * different power — so nothing is accounted for and the row stands.
 *
 * Deliberately a tie-breaker rather than a second gate. Requiring level agreement on every
 * row costs six real ones on Homecoming alone, because Mids' level for a granted power is
 * 0 where the export says 1, and because a fork's Mids database lags HC's own level moves.
 * Evidence AGAINST a pairing is what should withdraw it; absence of confirmation is not.
 *
 * Reading Mids here is not a Rule 0 breach. The question is not a game fact — it is what
 * Mids calls a power, and Mids is the sole authority on its own namespace. The export
 * cannot answer it, which is why the hand table this replaces (`MIDS_NAME_TYPOS`) existed
 * at all. Deriving it means the next HC rework flows through a regeneration instead of
 * waiting for a user to notice their slots landed on the wrong power.
 *
 * A row is emitted only where the display join lands on a DIFFERENT internal name than
 * the one Mids used. Names that already agree need no row, and names whose display has no
 * counterpart here (a power HC removed) get none either — those fall through to the
 * matcher's own ladder and, failing that, to a warning, which is the honest outcome.
 *
 * All of that is the join INSIDE a powerset. Pairing the powersets themselves is its own
 * problem, and MBDIMPORT-7 is what happens when it is assumed away: the group segment
 * drifts too. Rebirth's Guardian secondaries are `Guardian_Comp` here and
 * `Guardian_Composition` in Mids, so a `group.set` string comparison matched none of the
 * 13 and skipped them without a word. The import path never had this bug because it never
 * reads the group — `resolvePowerset` resolves on the SECOND segment and the build's own
 * archetype — so a Guardian build imported fine while the map that should have carried its
 * rotations was empty.
 *
 * So the pairing is derived in two passes. The exact `group.set` key first; then the
 * leftovers pair on the set segment alone, and take three conditions, because the set
 * segment alone is not an identity — six of our powersets are called `savage_melee`:
 *
 *   unique on both sides   only one leftover Mids set and one leftover of ours carry the
 *                          segment. Thunderspy's `scrapper_melee.ice_melee` and
 *                          `stalker_melee.ice_melee` both want `mission_maker_attacks.ice_melee`,
 *                          and a pairing that has to choose is not a decode.
 *   corroborated           the two sets share at least one power, by internal name or by
 *                          display. Two unrelated sets that happen to share a name mint
 *                          nothing but false rows; `redirects.staff_fighting` shares no
 *                          power with ours and is refused on exactly that.
 *   reported when refused  a leftover is named with the condition it failed. The bare
 *                          `continue` this replaces is what let 13 powersets vanish.
 *
 * The map is keyed by OUR `group.set`, because that is what its main reader has in hand:
 * `findPowerByMidsName` builds the key from the candidate powers' own paths. The importer's
 * `midsNameIsRetired` holds the .mbd's Mids-spelled path instead, so the pairs whose
 * spellings differ are emitted as `MIDS_POWERSET_ALIAS` and both readers resolve through
 * it. One table, addressable from either namespace — two keyings of one table is the trap
 * METHOD-7 records.
 *
 * Usage:
 *   node scripts/convert-mids-name-map.cjs --dataset homecoming
 *   node scripts/convert-mids-name-map.cjs --dataset homecoming --dry-run
 */

const fs = require('fs');
const path = require('path');
const { parseDatasetArg, datasetPath } = require('./_dataset-paths.cjs');

const datasetId = parseDatasetArg();
const dryRun = process.argv.includes('--dry-run');

const REPO_ROOT = path.resolve(__dirname, '..');
/**
 * Which fork's Mids namespace a build for this dataset actually carries.
 *
 * Brainstorm is Homecoming's open beta and Mids ships no build for it, so a Brainstorm
 * planner's Mids file was authored in Mids' HOMECOMING database — Homecoming's names
 * joined against Brainstorm's own export is the true pairing, and an empty map would be
 * a silent skip dressed as "no data".
 */
const NAMES_DATASET = { brainstorm: 'homecoming' };
const namesDataset = NAMES_DATASET[datasetId] || datasetId;
const NAMES_PATH = path.join(REPO_ROOT, 'tools', 'mids-oracle', `mids-power-names.${namesDataset}.json`);
const EXPORT_BASE = path.join(REPO_ROOT, 'exported_powers');
const RAW_ROOT = (datasetId === 'homecoming' && !fs.existsSync(path.join(EXPORT_BASE, datasetId)))
  ? EXPORT_BASE
  : path.join(EXPORT_BASE, datasetId);
const OUTPUT_PATH = datasetPath(datasetId, 'generated', 'mids-name-map.ts');

/**
 * Display names compared with separators and case folded away, and nothing else.
 *
 * Deliberately not the matcher's `stripSep`, which deletes every non-alphanumeric: that
 * would make "Quick Sand" and "Quicksand" equal here and mint a row for a pair the
 * matcher already resolves on its own. A remap row is for a name that means a DIFFERENT
 * power, so the join has to be tight enough that a spelling drift does not qualify.
 */
function normalizeDisplay(s) {
  return String(s || '').replace(/[\s_-]+/g, ' ').trim().toLowerCase();
}

/** Every powerset in this dataset's export, as `group.set` → its powers in game order. */
function readExportPowersets() {
  const out = new Map();
  for (const group of fs.readdirSync(RAW_ROOT, { withFileTypes: true })) {
    if (!group.isDirectory()) continue;
    const groupDir = path.join(RAW_ROOT, group.name);
    for (const set of fs.readdirSync(groupDir, { withFileTypes: true })) {
      if (!set.isDirectory()) continue;
      const indexPath = path.join(groupDir, set.name, 'index.json');
      if (!fs.existsSync(indexPath)) continue;
      const index = JSON.parse(fs.readFileSync(indexPath, 'utf-8'));
      const powers = index.powers || [];
      if (powers.length === 0) continue;
      const displays = index.power_display_names || [];
      // The key comes from the powers' own fullName rather than the directory names:
      // a converter that keys on the path silently stops matching when a fork renames
      // a folder, and the fullName is what the .mbd path is compared against anyway.
      const segments = String(powers[0]).split('.');
      if (segments.length < 3) continue;
      const key = `${segments[0]}.${segments[1]}`.toLowerCase();
      const levels = index.available_level || [];
      out.set(key, powers.map((full, i) => ({
        internalName: String(full).split('.').pop(),
        displayName: displays[i] || '',
        // `available_level` is 0-based here and Mids' is 1-based; the +1 is the whole
        // difference, and getting it backwards would silently reject every row.
        level: typeof levels[i] === 'number' ? levels[i] + 1 : null,
      })));
    }
  }
  return out;
}

const midsNames = JSON.parse(fs.readFileSync(NAMES_PATH, 'utf-8'));
const exportSets = readExportPowersets();

/**
 * A powerset's own segment, separators and case folded away.
 *
 * Wider than `normalizeDisplay` on purpose: this compares INTERNAL names, where Mids'
 * `stone composition` and our `Stone_Composition` are the same set spelled two ways, and
 * a Mids key can carry trailing whitespace inside a segment (`dark_composition `).
 */
function normalizeSegment(s) {
  return String(s || '').trim().replace(/[\s_-]+/g, '_').toLowerCase();
}

/** The `set` half of a `group.set` key — everything after the first dot. */
function setSegmentOf(key) {
  return normalizeSegment(key.split('.').slice(1).join('.'));
}

/** How many of `midsPowers` this powerset also carries, by internal name or by display. */
function corroboration(midsPowers, ours) {
  const internal = new Set(ours.map((p) => p.internalName.toLowerCase()));
  const display = new Set(ours.map((p) => normalizeDisplay(p.displayName)).filter(Boolean));
  let shared = 0;
  for (const [midsInternal, midsDisplay] of midsPowers) {
    if (internal.has(String(midsInternal).trim().toLowerCase())) shared++;
    else if (display.has(normalizeDisplay(midsDisplay))) shared++;
  }
  return shared;
}

/**
 * Mids' powersets paired to ours: the exact key, then the corroborated residual join.
 * See the header for why the second pass takes three conditions and not one.
 */
function pairPowersets() {
  const paired = [];
  const unmatched = [];
  const midsKeys = Object.keys(midsNames.powersets || {});

  const claimedOurs = new Set();
  const residual = [];
  for (const midsKey of midsKeys) {
    if (exportSets.has(midsKey)) {
      paired.push({ midsKey, ourKey: midsKey });
      claimedOurs.add(midsKey);
    } else {
      residual.push(midsKey);
    }
  }

  const index = (keys) => {
    const out = new Map();
    for (const key of keys) {
      const segment = setSegmentOf(key);
      if (!out.has(segment)) out.set(segment, []);
      out.get(segment).push(key);
    }
    return out;
  };
  const midsBySegment = index(residual);
  const oursBySegment = index([...exportSets.keys()].filter((k) => !claimedOurs.has(k)));

  for (const midsKey of residual) {
    const segment = setSegmentOf(midsKey);
    const theirs = midsBySegment.get(segment) || [];
    const candidates = oursBySegment.get(segment) || [];
    if (candidates.length === 0) {
      unmatched.push({ midsKey, why: 'no powerset of that name here' });
      continue;
    }
    if (candidates.length > 1 || theirs.length > 1) {
      unmatched.push({
        midsKey,
        why: `ambiguous — ${theirs.length} Mids sets and ${candidates.length} of ours share "${segment}"`,
      });
      continue;
    }
    const ourKey = candidates[0];
    const shared = corroboration(midsNames.powersets[midsKey], exportSets.get(ourKey));
    if (shared === 0) {
      unmatched.push({ midsKey, why: `uncorroborated — shares no power with ${ourKey}` });
      continue;
    }
    paired.push({ midsKey, ourKey, shared });
  }
  return { paired, unmatched };
}

const { paired, unmatched } = pairPowersets();

const map = {};
const reverse = {};
const alias = {};
const stats = {
  shared: paired.length, rows: 0, reverseRows: 0,
  ambiguous: [], merges: [], levelRejected: [], reverseWithdrawn: [],
};

for (const { midsKey, ourKey } of paired) {
  const midsPowers = midsNames.powersets[midsKey];
  const ours = exportSets.get(ourKey);

  // Display → our powers. A list, not a single entry: a set with two powers under one
  // display name (the Nature Affinity pet's "Rebirth" heal and rez) cannot be joined on
  // display, and picking either arm would be a coin flip dressed as a decode.
  const byDisplay = new Map();
  for (const power of ours) {
    const k = normalizeDisplay(power.displayName);
    if (!k) continue;
    if (!byDisplay.has(k)) byDisplay.set(k, []);
    byDisplay.get(k).push(power);
  }

  const midsByName = new Map(midsPowers.map((row) => [String(row[0]).toLowerCase(), row]));
  const rows = {};
  // The same join read the other way, for the EXPORT path (DATA-GAP MBDEXPORT-3). It has
  // to be minted here rather than inverted from `rows` in TypeScript, because `rows` keys
  // on a folded spelling and the writer needs Mids' literal one: Mids' loader resolves a
  // `PowerName` with `PiDFromUidPower`, an ordinal `==` against the database's own string.
  // Rebirth's Martial Mastery carries `"Shukuchi "` with a trailing space, and the folded
  // key `"shukuchi"` is a name Mids has no record of.
  const reverseRows = {};
  const claimed = new Map();
  for (const [midsInternal, midsDisplay] of midsPowers) {
    const candidates = byDisplay.get(normalizeDisplay(midsDisplay)) || [];
    if (candidates.length === 0) continue;
    if (candidates.length > 1) {
      stats.ambiguous.push(`${ourKey}: "${midsDisplay}" names ${candidates.length} powers here`);
      continue;
    }
    const ourInternal = candidates[0].internalName;
    if (ourInternal.toLowerCase() === String(midsInternal).toLowerCase()) continue;

    // Withdraw the row if our target is already spoken for by an identity: Mids carries a
    // power of that exact name, unlocking at the same level. See the header.
    const incumbent = midsByName.get(ourInternal.toLowerCase());
    if (incumbent && incumbent[2] !== null && candidates[0].level === incumbent[2]) {
      stats.levelRejected.push(
        `${ourKey}: "${midsDisplay}" — ${ourInternal} is Mids' own ${incumbent[0]} `
        + `(both level ${incumbent[2]}), not ${midsInternal}`,
      );
      continue;
    }
    rows[String(midsInternal).trim().toLowerCase()] = ourInternal;
    reverseRows[ourInternal.toLowerCase()] = String(midsInternal);
    // Two Mids names resolving onto one of ours is a MERGE, not a rotation, and a remap
    // row would silently drop whichever entry the build listed second. Recorded so the
    // gate can see it; the row still stands, because the alternative is the mis-bind.
    //
    // Read backwards the merge has no answer at all — one of our powers, two Mids names,
    // and nothing in the data says which the user meant. The reverse row is WITHDRAWN
    // rather than resolved to whichever came last, so the writer falls back to our own
    // spelling and reports it. That is today's behaviour for the name, which is the one
    // outcome here that is not a guess.
    if (claimed.has(ourInternal)) {
      stats.merges.push(`${ourKey}: ${claimed.get(ourInternal)} and ${midsInternal} both → ${ourInternal}`);
      stats.reverseWithdrawn.push(`${ourKey}: ${ourInternal} is claimed by ${claimed.get(ourInternal)} and ${midsInternal}`);
      delete reverseRows[ourInternal.toLowerCase()];
    }
    claimed.set(ourInternal, midsInternal);
  }

  if (Object.keys(rows).length > 0) {
    map[ourKey] = Object.fromEntries(Object.entries(rows).sort(([a], [b]) => a.localeCompare(b)));
    stats.rows += Object.keys(rows).length;
    if (Object.keys(reverseRows).length > 0) {
      reverse[ourKey] = Object.fromEntries(Object.entries(reverseRows).sort(([a], [b]) => a.localeCompare(b)));
      stats.reverseRows += Object.keys(reverseRows).length;
    }
    // Only for a pair that actually carries rows: an alias to an absent key is a lookup
    // that resolves to nothing, which reads exactly like the miss it is meant to fix.
    if (normalizeSegment(midsKey) !== normalizeSegment(ourKey)) alias[normalizeSegment(midsKey)] = ourKey;
  }
}

// An alias that is also a key of the map would silently steer one powerset's lookup into
// another's rows. It cannot happen — a pair only reaches the residual pass when its Mids
// key matched no export set — and the assert is here because "cannot happen" is how the
// bug above this one got written.
for (const midsKey of Object.keys(alias)) {
  if (map[midsKey]) {
    throw new Error(`alias ${midsKey} -> ${alias[midsKey]} collides with a map key of the same name`);
  }
}

const sorted = Object.fromEntries(Object.keys(map).sort().map((k) => [k, map[k]]));
const sortedReverse = Object.fromEntries(Object.keys(reverse).sort().map((k) => [k, reverse[k]]));
const sortedAlias = Object.fromEntries(Object.keys(alias).sort().map((k) => [k, alias[k]]));

const source = `Mids Reborn ${namesDataset} database ${midsNames.version} `
  + `(sha256 ${String(midsNames.sha256).slice(0, 12)}…)`
  + (namesDataset === datasetId ? '' : ` — Mids ships no ${datasetId} build, so a ${datasetId} .mbd carries ${namesDataset}'s namespace`);

const body = `/**
 * Mids internal name → this dataset's internal name — AUTO-GENERATED, DO NOT EDIT.
 *
 * Keyed by OUR \`group.powerset\` (lower-cased), then by the Mids internal name
 * (lower-cased). The value is this dataset's internal name for the SAME power, joined on
 * the display name — the identity that survived HC's internal-name rotations. See
 * DATA-GAP MBDIMPORT-2.
 *
 * The key is ours rather than Mids' because Mids' group segment drifts too
 * (\`Guardian_Composition\` for our \`Guardian_Comp\`, MBDIMPORT-7). \`MIDS_POWERSET_ALIAS\`
 * below carries those pairs so a reader holding the .mbd's own path can reach the same row.
 *
 * Source: ${source}
 * Powersets paired with the export: ${stats.shared} of ${Object.keys(midsNames.powersets || {}).length}. Remapped names: ${stats.rows}.
 * Reverse rows for the writer: ${stats.reverseRows}${stats.reverseWithdrawn.length ? `, with ${stats.reverseWithdrawn.length} withdrawn as ambiguous` : ''}.
 * Mids powersets with no counterpart here: ${unmatched.length} — listed by the generator on stderr.
 *
 * Regenerate: node scripts/convert-mids-name-map.cjs --dataset ${datasetId}
 */

export const MIDS_NAME_MAP: Readonly<Record<string, Readonly<Record<string, string>>>> = ${JSON.stringify(sorted, null, 2)};

/**
 * Mids' \`group.powerset\` → ours, for the pairs that spell the group differently.
 *
 * A reader that starts from the .mbd (the importer's retired-name check) resolves through
 * this; a reader that starts from our own powers (the matcher) already holds the map's key.
 */
export const MIDS_POWERSET_ALIAS: Readonly<Record<string, string>> = ${JSON.stringify(sortedAlias, null, 2)};

/**
 * The same join backwards — THIS dataset's internal name (lower-cased) → Mids' own, for
 * the .mbd writer (DATA-GAP MBDEXPORT-3).
 *
 * Not derivable from \`MIDS_NAME_MAP\` above, and that is the point of emitting it. The
 * forward map keys on a folded spelling because its reader is matching; the writer is
 * producing, and Mids resolves a \`PowerName\` by ordinal \`==\` against its own database
 * string. Case and inner whitespace are load-bearing on this side and discarded on that
 * one — Rebirth spells one power \`"Shukuchi "\`, trailing space and all.
 *
 * One row per forward row, minus any withdrawn: two Mids names landing on one power of
 * ours is answerable forwards and not backwards, so that name gets no row and the writer
 * reports it instead of picking.
 */
export const MIDS_NAME_REVERSE: Readonly<Record<string, Readonly<Record<string, string>>>> = ${JSON.stringify(sortedReverse, null, 2)};
`;

if (dryRun) {
  process.stdout.write(body);
} else {
  fs.mkdirSync(path.dirname(OUTPUT_PATH), { recursive: true });
  fs.writeFileSync(OUTPUT_PATH, body);
}

console.error(
  `[convert-mids-name-map] ${datasetId}: ${stats.rows} remapped names across ` +
  `${Object.keys(sorted).length} powersets (of ${stats.shared} paired, ` +
  `${Object.keys(sortedAlias).length} by alias), ${stats.reverseRows} reverse` +
  (dryRun ? ' [dry run]' : ` -> ${path.relative(REPO_ROOT, OUTPUT_PATH)}`),
);
for (const line of stats.merges) console.error(`  merge: ${line}`);
for (const line of stats.reverseWithdrawn) console.error(`  reverse-withdrawn: ${line}`);
for (const line of stats.levelRejected) console.error(`  level-rejected: ${line}`);
for (const line of stats.ambiguous.slice(0, 10)) console.error(`  ambiguous: ${line}`);
if (stats.ambiguous.length > 10) console.error(`  ambiguous: … +${stats.ambiguous.length - 10} more`);

// Every Mids powerset this run could not pair, and why. MBDIMPORT-7 was 13 of these
// answered with a bare `continue`: a skip nobody could see is indistinguishable from a
// powerset Mids does not carry.
const byReason = new Map();
for (const { why } of unmatched) {
  const reason = why.split(' —')[0];
  byReason.set(reason, (byReason.get(reason) || 0) + 1);
}
console.error(
  `  unpaired: ${unmatched.length} Mids powersets with no counterpart here (`
  + [...byReason].map(([why, n]) => `${why}: ${n}`).join('; ') + ')',
);
for (const { midsKey, why } of unmatched) console.error(`    ${midsKey} — ${why}`);
