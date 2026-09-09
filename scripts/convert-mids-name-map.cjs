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
 * So the pairing is derived in three passes. The exact `group.set` key first; then the
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
 * Then the third pass, for the sets whose two spellings share nothing at all (MBDEXPORT-9).
 * Mids puts the archetype qualifier on the far end of an epic pool — `Epic.Dark_Mastery_Blaster`
 * for our `Epic.Blaster_Dark_Mastery` — contracts it where archetypes share the set
 * (`Epic.Ice_Mastery_DefCorr`, `Epic.Psionic_Mastery_ScrapStalk`) and abbreviates the theme
 * as readily as the archetype (`Epic.Sentinel_Elec_Mastery`). And a set HC renamed outright
 * is worse than any of those: Shock Therapy is Mids' Electrical Affinity, Time Manipulation
 * its Temporal Manipulation, and no rule over the two strings will ever say so.
 *
 * So the name carries no signal here and the ROSTER carries the whole claim: the two sets
 * hold the same number of powers, and every one of Mids' is one of ours. That is a strong
 * condition — full coverage alone lets a one-power critter set match anything that happens
 * to carry its name — and it is still not an identity, so two more follow:
 *
 *   the group ours ASSERTS   where several Mids sets hold our roster, keep the ones in a
 *                            group the pairs already made map ours to. Three archetypes'
 *                            Electrical Affinity carry the same nine powers, and only the
 *                            group says which is the Controller's.
 *   the abbreviation         then `abbreviationScore` below, and only where it is decisive.
 *                            `Corr_Flame_Mastery` against `Def_Flame_Mastery` for our
 *                            Corruptor's Fire Mastery is the whole population of that.
 *   refused on a merge       one Mids set holding the exact roster of TWO of ours is a
 *                            merge, and a merge read backwards has no answer — the same
 *                            withdrawal the name rows take, one level up. Both are refused
 *                            and named rather than one being awarded the pair.
 *
 * Every pair this pass mints goes to stderr in full, because none of them can be checked
 * by reading the two names. `scripts/keys/mbdexport9-powerset-pairing-census.cjs` is the
 * other half: it attributes every set the three passes leave, and reds on one that has an
 * unpaired Mids counterpart holding its exact roster.
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
 *   node scripts/convert-mids-name-map.cjs --dataset homecoming --pairs   # the powerset pairing
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
 * Mids' powerset keys, folded for the join and kept literal for the writer (MBDEXPORT-6).
 *
 * The pairing below compares spellings across two namespaces, so it has to fold. The .mbd
 * WRITER composes a `PowerName` out of these two segments and Mids resolves one with an
 * ordinal `==`, so it needs the case back — `Epic.VEAT_Mace_Mastery` and
 * `Pool.Force_of_Will` are not what title-casing the folded key produces.
 *
 * A dump emitted before MBDEXPORT-6 carries folded keys and no marker. Its case is not
 * recoverable from the JSON, and guessing it would write a path that binds to nothing
 * while looking resolved, so the path table is withheld for that fork and said out loud.
 */
const KEYS_ARE_LITERAL = midsNames.powersetKeys === 'literal';
const midsSets = new Map();
for (const [key, rows] of Object.entries(midsNames.powersets || {})) {
  midsSets.set(key.toLowerCase(), { path: KEYS_ARE_LITERAL ? key : null, rows });
}

/**
 * Every non-alphanumeric gone — the import matcher's own ladder (MBDEXPORT-8).
 *
 * `normalizeDisplay` above stops at collapsing separator RUNS, so "Moon Beam" and
 * "Moonbeam" are a miss there, and deliberately: a forward row for a pair the matcher
 * already resolves on this ladder is a row that is not a rotation, and a row that is not a
 * rotation is a chance to bind the wrong power. The WRITER has no ladder, so for it a miss
 * is not a fallback but a wrong name. This width serves the writer and only the writer.
 */
function stripSeparators(s) {
  return String(s || '').replace(/[^a-z0-9]/gi, '').toLowerCase();
}

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
 * How much of Mids' set segment reads as an ABBREVIATION of ours (DATA-GAP MBDEXPORT-9).
 *
 * Mids shortens by truncation, and does it everywhere: `Sentinel_Elec_Mastery`,
 * `Sentinel_Psi_Mastery`, `Sentinel_Lev_Mastery`, `Corr_Flame_Mastery`, `Def_Flame_Mastery`.
 * So a token of Mids' that no token of ours matches outright still counts when one is a
 * prefix of the other. Tokens the two spellings share are set aside first — what is left
 * is the part that has to be explained.
 *
 * Only ever a tie-break, and only between Mids sets that carry OUR EXACT ROSTER. Both arms
 * of `Corr_Flame_Mastery` / `Def_Flame_Mastery` hold the same five power names, so the
 * names written are the same either way and it is the path that differs.
 */
function abbreviationScore(ourKey, midsKey) {
  const tokens = (key) => normalizeSegment(key.split('.').slice(1).join('.')).split('_').filter(Boolean);
  const ours = tokens(ourKey);
  const theirs = tokens(midsKey);
  const oursLeft = ours.filter((t) => !theirs.includes(t));
  return theirs.filter((t) => !ours.includes(t))
    .filter((t) => oursLeft.some((o) => o.startsWith(t) || t.startsWith(o)))
    .length;
}

/**
 * Mids' powersets paired to ours: the exact key, then the corroborated residual join.
 * See the header for why the second pass takes three conditions and not one.
 */
function pairPowersets() {
  const paired = [];
  const unmatched = [];
  const unpairedOurs = [];
  const midsKeys = [...midsSets.keys()];

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
    const shared = corroboration(midsSets.get(midsKey).rows, exportSets.get(ourKey));
    if (shared === 0) {
      unmatched.push({ midsKey, why: `uncorroborated — shares no power with ${ourKey}` });
      continue;
    }
    paired.push({ midsKey, ourKey, shared });
    claimedOurs.add(ourKey);
  }

  // The third pass, where BOTH segments are respelled (DATA-GAP MBDEXPORT-9). See the
  // header: the name carries no signal at all here, so the roster carries the whole claim.
  const pairedMids = new Set(paired.map((p) => p.midsKey));
  const ourRosterOf = (key) => exportSets.get(key);
  const midsRosterOf = (key) => midsSets.get(key).rows;

  // The group correspondence the pairs already made ASSERT, rather than one asserted here.
  // Thirteen Guardian pairs are what says `guardian_comp` is Mids' `Guardian_Composition`,
  // and that is the evidence that separates the Controller's Shock Therapy from the
  // Corruptor's when both carry the same nine powers.
  const groupsAsserted = new Map();
  for (const { midsKey, ourKey } of paired) {
    const group = ourKey.split('.')[0];
    if (!groupsAsserted.has(group)) groupsAsserted.set(group, new Set());
    groupsAsserted.get(group).add(midsKey.split('.')[0]);
  }

  const whole = new Map();
  for (const ourKey of exportSets.keys()) {
    if (claimedOurs.has(ourKey)) continue;
    const ours = ourRosterOf(ourKey);
    const hits = [];
    for (const midsKey of midsKeys) {
      if (pairedMids.has(midsKey)) continue;
      const rows = midsRosterOf(midsKey);
      if (rows.length !== ours.length) continue;
      if (corroboration(rows, ours) !== ours.length) continue;
      hits.push(midsKey);
    }
    if (hits.length > 0) whole.set(ourKey, hits);
  }

  const narrowed = new Map();
  for (const [ourKey, hits] of whole) {
    let candidates = hits;
    if (candidates.length > 1) {
      const groups = groupsAsserted.get(ourKey.split('.')[0]);
      const sameGroup = candidates.filter((k) => groups && groups.has(k.split('.')[0]));
      if (sameGroup.length > 0) candidates = sameGroup;
    }
    if (candidates.length > 1) {
      const scored = candidates.map((k) => [abbreviationScore(ourKey, k), k]);
      const best = Math.max(...scored.map(([n]) => n));
      const top = scored.filter(([n]) => n === best).map(([, k]) => k);
      if (best > 0 && top.length === 1) candidates = top;
    }
    if (candidates.length > 1) {
      unpairedOurs.push({
        ourKey,
        why: `${candidates.length} Mids sets carry exactly our roster: ${candidates.join(', ')}`,
      });
      continue;
    }
    narrowed.set(ourKey, candidates[0]);
  }

  // One Mids set claimed by two of ours is a MERGE, and a merge has no answer read
  // backwards — the same withdrawal the name rows take, one level up. Refused for both,
  // and named, rather than resolved to whichever the tie-breaks happen to favour.
  const claimants = new Map();
  for (const [ourKey, midsKey] of narrowed) {
    if (!claimants.has(midsKey)) claimants.set(midsKey, []);
    claimants.get(midsKey).push(ourKey);
  }
  for (const [midsKey, ourKeys] of claimants) {
    if (ourKeys.length > 1) {
      for (const ourKey of ourKeys) {
        unpairedOurs.push({ ourKey, why: `Mids ${midsKey} is the whole roster of ${ourKeys.length} of ours: ${ourKeys.join(', ')}` });
      }
      continue;
    }
    paired.push({ midsKey, ourKey: ourKeys[0], shared: midsRosterOf(midsKey).length, rekeyed: true });
  }

  // A Mids leftover pass three DID reach loses the reason pass two gave it; the rest keep
  // theirs. `unpairedOurs` is the other side of the same coin and stays its own list — a
  // count that mixed the two would answer neither question.
  const reached = new Set(paired.map((p) => p.midsKey));
  return { paired, unmatched: unmatched.filter((u) => !reached.has(u.midsKey)), unpairedOurs };
}

const { paired, unmatched, unpairedOurs } = pairPowersets();

// `--pairs` prints the powerset pairing and stops. It exists because the pairing is an
// input to measurements this generator does not itself make (MBDEXPORT-8's separator
// census), and a measurement that re-derives it is a second copy of the three conditions
// above — free to drift, and wrong in exactly the way the number is supposed to settle.
if (process.argv.includes('--pairs')) {
  // `writeSync` and not `process.stdout.write`: a piped stdout is asynchronous, and an
  // `exit` under it drops whatever has not flushed. That truncates a 3,596-line pairing
  // at whatever the pipe happened to take — silently, and differently each run, so a
  // census reading it reports powersets as unpaired that are paired.
  fs.writeSync(1, paired.map(({ midsKey, ourKey }) => `${ourKey}\t${midsKey}\n`).join(''));
  process.exit(0);
}

const map = {};
const reverse = {};
const looseReverse = {};
const alias = {};
const stats = {
  shared: paired.length, rows: 0, reverseRows: 0,
  ambiguous: [], merges: [], levelRejected: [], reverseWithdrawn: [],
  loose: [], looseAmbiguous: [],
};

for (const { midsKey, ourKey } of paired) {
  const midsPowers = midsSets.get(midsKey).rows;
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
  // Reverse rows the tight join cannot reach, for the writer alone (DATA-GAP MBDEXPORT-8).
  // Kept in their own table rather than merged above, so that "every reverse row inverts a
  // forward row" stays an invariant a gate can hold, and so the looser provenance of these
  // is impossible to read past.
  const looseReverseRows = {};
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

  // The writer's second pass. Only the Mids names the tight join reached NOTHING with are
  // eligible — anything it did reach is already answered, rightly or by a withdrawal, and
  // reopening it here would overrule a decision made with more evidence.
  const byStripped = new Map();
  for (const power of ours) {
    const k = stripSeparators(power.displayName);
    if (!k) continue;
    if (!byStripped.has(k)) byStripped.set(k, []);
    byStripped.get(k).push(power);
  }
  for (const [midsInternal, midsDisplay] of midsPowers) {
    if ((byDisplay.get(normalizeDisplay(midsDisplay)) || []).length > 0) continue;
    const candidates = byStripped.get(stripSeparators(midsDisplay)) || [];
    if (candidates.length !== 1) {
      if (candidates.length > 1) {
        stats.looseAmbiguous.push(`${ourKey}: "${midsDisplay}" reaches ${candidates.length} powers here`);
      }
      continue;
    }
    const ourInternal = candidates[0].internalName;
    if (ourInternal.toLowerCase() === String(midsInternal).toLowerCase()) continue;
    // Ours is already answered by the tight pass, or claimed by it: leave it alone.
    if (reverseRows[ourInternal.toLowerCase()] || claimed.has(ourInternal)) continue;
    // The same withdrawal the tight pass takes, and for the same reason — a display
    // coincidence does not outrank Mids carrying that exact name at that exact level.
    const incumbent = midsByName.get(ourInternal.toLowerCase());
    if (incumbent && incumbent[2] !== null && candidates[0].level === incumbent[2]) {
      stats.levelRejected.push(
        `${ourKey}: "${midsDisplay}" (loose) — ${ourInternal} is Mids' own ${incumbent[0]} `
        + `(both level ${incumbent[2]}), not ${midsInternal}`,
      );
      continue;
    }
    looseReverseRows[ourInternal.toLowerCase()] = String(midsInternal);
    stats.loose.push(`${ourKey}: ${ourInternal} → ${midsInternal} ("${candidates[0].displayName}" / "${midsDisplay}")`);
  }
  if (Object.keys(looseReverseRows).length > 0) {
    looseReverse[ourKey] = Object.fromEntries(
      Object.entries(looseReverseRows).sort(([a], [b]) => a.localeCompare(b)));
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

/**
 * Ours → Mids' literal `group.set`, for every paired set (MBDEXPORT-6).
 *
 * A separate table from `MIDS_NAME_MAP` because it covers a different population: the map
 * carries only sets that ROTATED a power name, and the writer needs a path for every set
 * a build can hold. Rebirth's Guardian secondaries are the shape of the defect — nine
 * power names already correct inside a `group.set` Mids has never heard of.
 *
 * Only from a dump that kept Mids' case. Where it did not, the table is empty and the
 * header below says which fork and why, because a path assembled from a folded key looks
 * resolved and binds to nothing.
 */
const pathTable = {};
for (const { midsKey, ourKey } of paired) {
  const literal = midsSets.get(midsKey).path;
  if (literal) pathTable[ourKey] = literal;
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
const sortedPaths = Object.fromEntries(Object.keys(pathTable).sort().map((k) => [k, pathTable[k]]));
const sortedLoose = Object.fromEntries(Object.keys(looseReverse).sort().map((k) => [k, looseReverse[k]]));

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
 * Powersets paired with the export: ${stats.shared} of ${midsSets.size}. Remapped names: ${stats.rows}.
 * Reverse rows for the writer: ${stats.reverseRows}${stats.reverseWithdrawn.length ? `, with ${stats.reverseWithdrawn.length} withdrawn as ambiguous` : ''}, plus ${stats.loose.length} the display join could only reach with its separators stripped.
 * Powerset paths for the writer: ${Object.keys(sortedPaths).length}${KEYS_ARE_LITERAL ? '' : ` — NONE. The ${namesDataset} names dump predates MBDEXPORT-6 and carries folded powerset keys, so Mids' own spelling is not in it. Re-run emit_mids_names.py against that fork's I12.mhd to fill this in.`}
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

/**
 * OUR \`group.powerset\` (lower-cased) → Mids' own spelling of it, for the .mbd writer
 * (DATA-GAP MBDEXPORT-6).
 *
 * The first two segments of a \`PowerName\`, read out of Mids' database rather than
 * composed. The writer used to build them from an archetype table and the powerset's ICON
 * filename, and neither is a read of what Mids calls the set: a Rebirth Guardian went out
 * as \`Guardian_Comp.Electric_Armor\` where Mids holds
 * \`Guardian_Composition.Atmospheric_Composition\`, with all nine power names already
 * right inside it.
 *
 * Case is load-bearing here for the same reason it is in \`MIDS_NAME_REVERSE\`, and it is
 * not reconstructible: \`Epic.VEAT_Mace_Mastery\`, \`Pool.Force_of_Will\` and
 * \`Epic.Dark_Mastery_TankBrute\` are none of them what title-casing produces.
 *
 * A set absent from this table is one this pairing could not reach. The writer reports it
 * rather than composing a path, because Mids answers a \`group.set\` it cannot resolve
 * with a blank row that still holds the power's slots.
 */
export const MIDS_POWERSET_PATH: Readonly<Record<string, string>> = ${JSON.stringify(sortedPaths, null, 2)};

/**
 * Reverse rows the display join could only reach with every separator stripped — for the
 * .mbd writer, and for it alone (DATA-GAP MBDEXPORT-8).
 *
 * Rebirth spells a power \`Moonbeam\` and Mids spells it \`Moon_Beam\`. The join above
 * folds separator RUNS to one space and stops, so that pair is a miss, and that tightness
 * is right where it is: the IMPORT matcher resolves such a pair on its own
 * all-separators-stripped ladder, and a forward row for a pair it already handles is a row
 * that is not a rotation — a chance to bind the wrong power for no gain.
 *
 * The writer has no ladder. One lookup, and ours goes out on a miss, under a name Mids
 * answers with a blank row that keeps the slots. So the width the reader needs and the
 * width the writer needs are different, and this is the writer's.
 *
 * A separate table rather than extra rows in \`MIDS_NAME_REVERSE\` for two reasons: that
 * one is exactly the inverse of \`MIDS_NAME_MAP\` and a gate holds it to that, and these
 * rows come from a looser join, which is a fact about them a reader should not have to
 * infer. Ours-already-answered is never overruled — a key here is one the tight pass left
 * empty.
 */
export const MIDS_NAME_REVERSE_LOOSE: Readonly<Record<string, Readonly<Record<string, string>>>> = ${JSON.stringify(sortedLoose, null, 2)};
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
  `${Object.keys(sortedAlias).length} by alias), ${stats.reverseRows} reverse, ` +
  `${Object.keys(sortedPaths).length} powerset paths, ${stats.loose.length} loose reverse` +
  (dryRun ? ' [dry run]' : ` -> ${path.relative(REPO_ROOT, OUTPUT_PATH)}`),
);
if (!KEYS_ARE_LITERAL) {
  console.error(
    `  no powerset paths: the ${namesDataset} names dump carries folded powerset keys and no`
    + ` "powersetKeys": "literal" marker, so Mids' own spelling of a group.set is not in it.`
    + ` The .mbd writer will report every set on this fork rather than guess (MBDEXPORT-6).`,
  );
}
for (const line of stats.loose) console.error(`  loose reverse: ${line}`);
for (const line of stats.looseAmbiguous) console.error(`  loose-ambiguous: ${line}`);
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

// Every pair the roster pass minted, in full (DATA-GAP MBDEXPORT-9). These are the ones no
// part of the NAME argues for — the claim is that the two sets hold the same powers and
// nothing else holds them — so the list is the reviewable artifact, not the count.
const rekeyed = paired.filter((p) => p.rekeyed);
console.error(`  rekeyed: ${rekeyed.length} pairs whose two spellings share nothing, matched on the roster`);
for (const { ourKey, midsKey, shared } of rekeyed) {
  console.error(`    ${ourKey} — ${midsSets.get(midsKey).path || midsKey} (${shared} powers, all of both)`);
}
// And ours the pass declined to pair, with the condition each failed.
console.error(`  ours-unpaired: ${unpairedOurs.length} of our powersets a Mids set carries whole but ambiguously`);
for (const { ourKey, why } of unpairedOurs) console.error(`    ${ourKey} — ${why}`);
