/**
 * The combat scope an effect group's `Requires` confines it to.
 *
 * Every consumer used to answer this by substring-testing the RPN for
 * `enttype target> player eq`. That is right for the bare case and backwards
 * for a negated one, and both ship (MAPGATE-1):
 *
 *   `arch target> Class_Minion_Grunt eq … enttype target> player eq || !`
 *       "not a minion and not a player" — the Scrapper/Stalker critical-hit
 *       branch against lieutenants and bosses, i.e. the PRIMARY PvE crit rate.
 *       The substring test dropped it from PvE entirely.
 *
 *   `Raid target.HasTag? enttype target> player eq || kRage source> 70 < &&`
 *       one branch of an `||`, so the token neither implies nor forbids PvP.
 *
 * The verdict is now read structurally by the parser
 * (`bin_crawler/parser/_requires.py`) and exported per group as `requires_pv`,
 * so there is exactly one implementation and the converters cannot drift from
 * it. Read the field; never re-derive it from the expression here.
 */

const SCOPES = new Set(['EITHER', 'PVE_ONLY', 'PVP_ONLY', 'NEVER', 'UNPARSED']);

/**
 * `requires_pv` off an effect group, validated.
 *
 * Throws rather than defaulting: a missing field means the export predates the
 * field and is stale, and a soft default here is precisely the shape that let
 * STACK-3 hide — an impossible value quietly becoming a plausible one.
 */
function requiresScope(group) {
  const v = group && group.requires_pv;
  if (v === undefined) {
    throw new Error(
      'effect group has no `requires_pv` — exported_powers/ predates the field; '
      + 're-run bin_crawler.export_powers');
  }
  if (!SCOPES.has(v)) {
    throw new Error(`unrecognized requires_pv ${JSON.stringify(v)}`);
  }
  return v;
}

/**
 * Should a PvE planner drop this group outright?
 *
 * True only when the group can NEVER apply against a non-player target.
 * `UNPARSED` (the parser met a requires vocabulary it has no arity for) is
 * deliberately not dropped — an unreadable gate is not evidence of PvP.
 */
function isPvpOnlyGroup(group) {
  return requiresScope(group) === 'PVP_ONLY';
}

module.exports = { requiresScope, isPvpOnlyGroup, SCOPES };
