/**
 * Plan B — the shared corpus sweep every `planb-shadow-*` gate walks.
 *
 * This exists because the sweep was copy-pasted into seven shadows, and every copy had
 * the same hole: it walked `generated/powersets` only. Pool and epic-pool powers —
 * Health, Stamina, Tough, Weave, Maneuvers, Assault, and the entire epic/patron tier,
 * ~1,358 powers across the three datasets — live in `power-pools.ts` / `epic-pools.ts`,
 * built by two separate converters. So every gate's "corpus-wide, 0 divergences" claim
 * was structurally silent about ~15% of the corpus (found 2026-07-15).
 *
 * Two independent bugs stacked, which is why nothing caught it: those powers had no
 * atoms at all (the pool converters never called `encodeAtomsForEmit`), so each helper
 * returned `undefined` and the appliers fell back to the bag — behavior-preserving, and
 * therefore invisible. Mutation-testing a gate cannot find this: every mutant still
 * passes on a corpus that excludes the affected powers.
 *
 * Hence ONE sweep, shared. A gate cannot forget to look somewhere the sweep already
 * goes, and a future generated tree is covered by construction rather than by each
 * author remembering. Sweeping the dataset's output wholesale (not a named
 * subdirectory) is the point — do not narrow it back.
 *
 * The sweep walked `src/data/datasets/<id>/generated/**.ts` until 2026-09-25, and by then
 * four of the modules it read there — `power-pools.ts`, `epic-pools.ts`, `inherents.ts`,
 * `incarnate-effects.ts` — were written by nothing: their converters had moved to
 * `pipeline/<id>/*.json` and the TypeScript was a committed leftover. Every gate on this
 * sweep was therefore grading a snapshot for that ~15% of the corpus, the exact silence
 * the paragraph above was written about, arrived at a second way. So the sweep reads
 * `pipeline/<id>/` now, which is what the converters actually write. Measured at the
 * switch: 1,263 Homecoming powers either way, no name gained or lost.
 */

require('tsx/cjs'); // `forkResolvedViews` reads the atom codec straight from its TS home
const fs = require('fs');
const path = require('path');

const REPO = path.resolve(__dirname, '..');

/**
 * Every Power-shaped object reachable from a generated module's exports.
 *
 * `generated/powersets/**` exports one power per file at the top level, but the pool
 * trees nest theirs inside pool objects (`{ pools: { fitness: { powers: [...] } } }`),
 * so this recurses. `seen` guards the cycles those structures contain.
 *
 * An `atoms` array is the whole rule; the bag was the other arm until 2026-09-03, dropped in
 * step with `emit-contract.cjs`, `collect-composed-powers.cjs` and the engine repo's contract
 * loader (`coh_data`'s `collect_into`), where a node carrying a bag and no atoms is now an
 * error rather than a skip. Measured 0 such powers in both repos' generated trees before the
 * arm came off. The recursion also reaches `resolvedPseudoPets` ability rows (Lightning_Rod,
 * Shield_Charge_AoE, Geode_Scaling), which carry a `name` and an `effects` ARRAY of effect
 * rows; they are not Powers, have no atom list, and matching them made the gate report
 * nonsense slot names ("projected slots [0, 1, 2]" — array indices). Requiring atoms excludes
 * them by construction, which is what the old non-array check was buying.
 */
function isPower(node) {
  if (typeof node.name !== 'string') return false;
  return Array.isArray(node.atoms);
}

function collectPowers(node, out = [], seen = new Set()) {
  if (!node || typeof node !== 'object' || seen.has(node)) return out;
  seen.add(node);
  if (Array.isArray(node)) {
    for (const v of node) collectPowers(v, out, seen);
    return out;
  }
  if (isPower(node)) out.push(node);
  for (const v of Object.values(node)) collectPowers(v, out, seen);
  return out;
}

/**
 * The only files under `pipeline/<id>/` the sweep skips, and each for a stated reason.
 *
 * The three pet files hold PET abilities, which are not player powers and had no
 * counterpart in the generated TypeScript tree this sweep used to walk; admitting them
 * would hand every gate 551 extra Homecoming entries no gate was written against.
 * `powersets.json` is the flat aggregate of the `powersets/` tree beside it, so sweeping
 * both visits every powerset power twice. Top level only — a name is skipped because of
 * what that one file is, never because of what it is called.
 */
const SWEEP_EXCLUDE = new Set([
  'pet-entities.json',
  'pet-entity-powers.json',
  'pet-lifespans.json',
  'powersets.json',
]);

/**
 * Walk one dataset's whole generated tree, calling `onPower(power, relPath)` for each
 * player power found.
 *
 * `opts.onLoadError(relPath, err)` is called for a module that fails to require. A gate
 * that treats a load failure as a skip is claiming a pass for a power it never checked,
 * so callers should surface it as a divergence; the default rethrows rather than let a
 * caller silently inherit the lenient behavior.
 */
function sweepDataset(dataset, onPower, opts = {}) {
  const root = path.join(REPO, 'pipeline', dataset);
  if (!fs.existsSync(root)) {
    if (opts.onMissingDataset) opts.onMissingDataset(dataset);
    return;
  }
  const stack = [root];
  while (stack.length) {
    const d = stack.pop();
    for (const e of fs.readdirSync(d, { withFileTypes: true })) {
      const p = path.join(d, e.name);
      if (e.isDirectory()) { stack.push(p); continue; }
      if (!e.name.endsWith('.json')) continue;
      if (d === root && SWEEP_EXCLUDE.has(e.name)) continue;
      const rel = path.relative(REPO, p);
      let mod;
      try {
        mod = JSON.parse(fs.readFileSync(p, 'utf-8'));
      } catch (err) {
        if (opts.onLoadError) { opts.onLoadError(rel, err); continue; }
        throw err;
      }
      for (const power of collectPowers(mod)) onPower(power, rel);
    }
  }
}

/**
 * ARCHETYPE-FORK RESOLUTION — the one thing a shadow gate cannot do by reading `power`.
 *
 * `atom-query`'s build-agnostic readers drop an archetype-forked atom, because they have
 * no build to resolve it against. The bag used to drop it too, so a forked slot was
 * `undefined` on both sides and every gate scored it as agreement — silence matching
 * silence, which is not evidence of anything. The converter now states a forked slot in
 * the bag when the fork turned out to make no difference — every archetype in the
 * roster gets the same value (`_addUnanimousForkedSlots`) — and that claim is exactly
 * what a gate should be checking rather than stepping around.
 *
 * So: hand the reader the power as ONE archetype sees it. If the converter's unanimity
 * claim holds, every archetype's view reproduces the bag; if it does not, some view
 * disagrees and the gate says so. Strictly more than the old mutual `undefined`.
 */
const { decodeAtoms, encodeAtom, ATOM_TUPLE_FIELDS } = require('./_atomic-effect.ts');
const { derivePlayerClassTokens } = require('./_player-classes.cjs');

const FORK_FIELD = ATOM_TUPLE_FIELDS.indexOf('casterArchetypes');
const rosterCache = new Map();
const viewCache = new WeakMap();

function rosterOf(dataset) {
  if (!rosterCache.has(dataset)) {
    const base = path.join(REPO, 'exported_powers');
    const dir = fs.existsSync(path.join(base, dataset, 'tables'))
      ? path.join(base, dataset, 'tables')
      : path.join(base, 'tables'); // HC's checked-in tree is unprefixed
    rosterCache.set(dataset, derivePlayerClassTokens(dir));
  }
  return rosterCache.get(dataset);
}

/**
 * One `AtomSource` per player archetype, each carrying the unforked atoms plus that
 * archetype's own arms with the stamp cleared — so `atom-query`'s base readers, which
 * skip anything stamped, see the arms that genuinely are this archetype's base.
 *
 * `[]` for a power with no forked atom, which is the overwhelming majority: the caller
 * then has nothing to reconcile and the old comparison stands.
 */
function forkResolvedViews(dataset, power) {
  const cached = viewCache.get(power);
  if (cached) return cached;
  const raw = power.atoms || [];
  const views = raw.some((t) => t[FORK_FIELD])
    ? rosterOf(dataset).map((archetype) => ({
      archetype,
      // `targetsAffected` rides along because an atom reader needs it to resolve a
      // `toWho: 'Target'` recipient (TARGETS-3), and a view that dropped it would answer
      // "reaches nobody" for every such atom — a fork-resolved view silently reading a
      // different code path than the power it stands for.
      source: {
        targetsAffected: power.targetsAffected,
        atoms: decodeAtoms(raw)
          .filter((a) => !a.casterArchetypes || a.casterArchetypes.split(',').includes(archetype))
          .map((a) => encodeAtom({ ...a, casterArchetypes: undefined })),
      },
    }))
    : [];
  viewCache.set(power, views);
  return views;
}

/**
 * Does every archetype's fork-resolved view read back the value the bag states?
 *
 * `read(source)` is the gate's own atom reader for the slot under test, already
 * normalized; `eq` its own comparison. False for a power with no fork — there is then
 * no resolution to appeal to and the plain divergence stands.
 */
function forkResolvedAgrees(dataset, power, bagValue, read, eq) {
  const views = forkResolvedViews(dataset, power);
  return views.length > 0 && views.every((v) => eq(read(v.source), bagValue));
}

module.exports = { REPO, collectPowers, sweepDataset, forkResolvedViews, forkResolvedAgrees };
