'use strict';

/**
 * The canonical `stats` block, minted from one export record.
 *
 * Execution stats have two homes in the shipped data. `convert-powerset.cjs` writes them to
 * `power.stats`; the pool, epic, accolade and inherent converters wrote them into the `effects`
 * bag under the export's own field names (`activationTime` for cast, raw `endurance`), which is
 * the legacy shape both runtimes then adapt on the way in (the beta's `transformPoolPower`,
 * `coh_data`'s `normalize_legacy_power`, PROD6C-3h). One mint, so the partitions publish the
 * same object as an archetype power and the bag stops being the only source (atom-migration,
 * display item job 2).
 *
 * A toggle's `endurance` is per TICK. Divide by `activatePeriod` for the per-second drain; both
 * engines do that at their own display seam and nowhere else.
 */
function powerStats(rawJson) {
  const stats = {
    accuracy: rawJson.accuracy,
    range: rawJson.range,
    radius: rawJson.radius,
    arc: rawJson.arc,
    recharge: rawJson.recharge_time,
    endurance: rawJson.endurance_cost,
    castTime: rawJson.activation_time,
    // Interruptible channel (Trip Mine, Rest, rez powers, and the single-form snipes on servers
    // that bake it onto the base power). 0 for the vast majority; dropped below when 0.
    interruptTime: rawJson.interrupt_time,
    activatePeriod: rawJson.activate_period,
    maxTargets: rawJson.max_targets_hit,
  };

  // Animation root/lock time, surfaced only when it diverges from castTime. Root == full
  // activation is the common case and says nothing new.
  if (rawJson.time_to_root != null && rawJson.time_to_root !== rawJson.activation_time) {
    stats.timeToRoot = rawJson.time_to_root;
  }

  for (const key of Object.keys(stats)) {
    if (!stats[key]) delete stats[key];
  }

  // Homecoming's over-cap block: past `overCapTrigger` targets, each further target's per-target
  // effects are scaled by `overCapMultiplier` (raised to the excess when `overCapExponential`).
  // It is what makes Radiation Therapy add 9.46s for its first foe and 2.84s for each after.
  // Absent on every record that leaves the trigger at 0, which is every Parse6 record.
  if (rawJson.over_cap_trigger > 0) {
    stats.overCapTrigger = rawJson.over_cap_trigger;
    stats.overCapMultiplier = rawJson.over_cap_multiplier;
    if (rawJson.over_cap_exponential) stats.overCapExponential = true;
  }
  return stats;
}

module.exports = { powerStats };
