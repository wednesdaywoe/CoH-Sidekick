/**
 * Shared dim_returns derivation (SOURCE-1 SW3/item 9): schedule identity by
 * tier-start triple, and the boost-type -> schedule map an
 * `enhancement_curves.json` carries. Used by convert-enhancement-curves.cjs
 * (full curve module) and convert-special-enhancements.cjs (schedule lookup
 * for boosts_allowed tokens).
 */

// Schedule identity is the dim_returns tier-start triple; the letters are the
// project-wide names for those triples (SW1 §1). A triple not listed here is a
// new data variant and must break the build, not default.
const SCHEDULE_BY_TRIPLE = new Map([
  ['0.7,0.9,1', 'A'],
  ['0.4,0.5,0.6', 'B'],
  ['0.8,1,1.2', 'C'],
  ['1.2,1.5,1.8', 'D'],
]);

// Strip float32 storage noise from an authored decimal (0.4000000059604645 -> 0.4).
function authored(x) {
  return Math.round(x * 1e4) / 1e4;
}

function tripleKey(tiers, field) {
  return tiers.map((t) => String(authored(t[field]))).join(',');
}

/**
 * Derive schedules, thresholds, tier effectiveness, and the boost-type map
 * from a parsed `enhancement_curves.json`. Throws on any unrecognized or
 * non-uniform decode.
 */
function deriveSchedules(curves) {
  const edThresholds = {};
  const boostTypeSchedules = {};
  let defaultSchedule = null;
  let tierEffectiveness = null;
  const attribFamily = new Map(); // attrib name -> schedule letter (non-default entries)

  for (const entry of curves.dim_returns) {
    if (entry.returns.length === 0) {
      throw new Error(`dim_returns entry ${JSON.stringify(entry.boost_types)} has no returns`);
    }
    const startsKey = tripleKey(entry.returns[0].tiers, 'start');
    const handicapKey = tripleKey(entry.returns[0].tiers, 'handicap');
    for (const ret of entry.returns) {
      if (tripleKey(ret.tiers, 'start') !== startsKey || tripleKey(ret.tiers, 'handicap') !== handicapKey) {
        throw new Error(`dim_returns entry ${JSON.stringify(entry.boost_types)} has non-uniform tiers across returns`);
      }
    }
    const letter = SCHEDULE_BY_TRIPLE.get(startsKey);
    if (!letter) {
      throw new Error(`Unknown ED tier triple [${startsKey}] on ${entry.is_default ? 'default' : JSON.stringify(entry.boost_types)} — new schedule variant, extend SCHEDULE_BY_TRIPLE deliberately`);
    }
    if (tierEffectiveness === null) {
      tierEffectiveness = entry.returns[0].tiers.map((t) => authored(t.handicap));
    } else if (handicapKey !== tierEffectiveness.map(String).join(',')) {
      throw new Error(`Non-uniform tier effectiveness: ${handicapKey} vs ${tierEffectiveness}`);
    }
    const thresholds = entry.returns[0].tiers.map((t) => authored(t.start));
    if (edThresholds[letter] && edThresholds[letter].join(',') !== thresholds.join(',')) {
      throw new Error(`Schedule ${letter} appears with two different threshold triples`);
    }
    edThresholds[letter] = thresholds;

    if (entry.is_default) {
      if (defaultSchedule !== null) throw new Error('Multiple default dim_returns entries');
      defaultSchedule = letter;
    } else {
      for (const bt of entry.boost_types) {
        if (boostTypeSchedules[bt]) throw new Error(`Boost type ${bt} in two dim_returns entries`);
        boostTypeSchedules[bt] = letter;
      }
      for (const ret of entry.returns) {
        for (const a of ret.attribs) {
          if (!attribFamily.has(a)) attribFamily.set(a, letter);
        }
      }
    }
  }
  if (defaultSchedule === null) throw new Error('No default dim_returns entry');
  return { edThresholds, boostTypeSchedules, defaultSchedule, tierEffectiveness, attribFamily };
}

module.exports = { SCHEDULE_BY_TRIPLE, authored, deriveSchedules };
