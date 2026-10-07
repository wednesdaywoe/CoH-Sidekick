/**
 * Whether an effect group's `Requires` holds in the default situation — a
 * character in ordinary PvE content with nothing switched on. A group whose
 * gate is SATISFIED there belongs to the power's base; anything else is a
 * branch the planner surfaces as a conditional.
 *
 * The converter used to answer this by testing whether the `&&`-flattened
 * expression ends in `!` — the last token read as if it were the top-level
 * operator. That holds for a conjunction of negations and is backwards when the
 * `!` negates one conjunct: Rebirth Guardian's Dispersion Bubble carries
 * `isPVPMap? entref target> entref source> eq ! &&`, whose root operator is
 * `&&`, and its whole PvP-only defence set landed in base. 3,230 groups across
 * 223 expressions were exposed; two facets carried hand patches
 * (DATA-GAP-REGISTER COND-1).
 *
 * The verdict is now read structurally by the parser
 * (`bin_crawler/parser/_gate_default.py`), which evaluates the parsed tree
 * against a documented default assignment, and exported per group as
 * `requires_default` — plus `requires_archetypes` where the caster's archetype
 * is the only thing standing between the group and base. Read the fields; never
 * re-derive them from the expression here — one implementation, so the five
 * converters cannot drift from it.
 */

const VERDICTS = new Set([
  'SATISFIED',
  'UNSATISFIED',
  'INDETERMINATE',
  'INDETERMINATE_ARCHETYPE',
  'UNPARSED',
  'UNCLASSIFIED',
]);

/**
 * `requires_default` off an effect group, validated.
 *
 * Throws rather than defaulting: a missing field means the export predates it
 * and is stale, and a soft default here is precisely the shape that let STACK-3
 * hide — an impossible value quietly becoming a plausible one.
 */
function requiresDefault(group) {
  const v = group && group.requires_default;
  if (v === undefined) {
    throw new Error(
      'effect group has no `requires_default` — exported_powers/ predates the field; '
      + 're-run bin_crawler.export_powers');
  }
  if (!VERDICTS.has(v)) {
    throw new Error(`unrecognized requires_default ${JSON.stringify(v)}`);
  }
  return v;
}

/**
 * The archetypes an archetype-forked group is the base case FOR, in the export's
 * own `Class_*` spelling. Empty for every other group.
 *
 * `arch source> Class_Scrapper eq` on a pool power is base for a Scrapper and a
 * branch for everyone else; one boolean cannot say that, so the parser evaluates
 * the gate once per archetype the dataset defines and ships the side that came
 * out satisfied (DATA-GAP-REGISTER AT-FORK-1). A verdict of
 * INDETERMINATE_ARCHETYPE with an EMPTY list is a real state and means the
 * opposite of a fork worth keeping: the archetypes disagree but none of them
 * makes the gate hold on its own, so the group is base for nobody.
 */
function casterArchetypes(group) {
  if (requiresDefault(group) !== 'INDETERMINATE_ARCHETYPE') return [];
  const named = group.requires_archetypes;
  if (named === undefined) return [];
  if (!Array.isArray(named) || named.some((a) => typeof a !== 'string')) {
    throw new Error(`malformed requires_archetypes ${JSON.stringify(named)}`);
  }
  return named;
}

/**
 * Is this group's gate the power's base case?
 *
 * `SATISFIED`, or an archetype fork that resolved to at least one archetype —
 * for a build of one of those the gate genuinely IS open, and the atoms carry
 * the list so the engine can drop them for every other build. Routing them
 * through the conditional path instead would strip their slots: that path
 * re-admits gated atoms as slot-less synthetics with empty strength, which is
 * right for the `ignoreStrength` mode atoms it was built for and wrong for
 * Rebirth Tough's enhanceable resistance.
 *
 * `UNPARSED` and `UNCLASSIFIED` are deliberately NOT base: an unreadable gate is
 * not evidence that the gate is open, and both sit at zero corpus-wide with a
 * gate holding them there.
 */
function isBaseCase(group) {
  return requiresDefault(group) === 'SATISFIED' || casterArchetypes(group).length > 0;
}

module.exports = { requiresDefault, isBaseCase, casterArchetypes, VERDICTS };
