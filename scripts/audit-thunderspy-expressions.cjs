#!/usr/bin/env node
// Structural completeness guard for the Thunderspy magnitude/duration EXPRESSION recovery
// (DATA-GAP INHERENT-2). `_parse_effect_template_thunderspy` reads the two RPN token arrays that
// follow the post-table magnitude (Fury's `kRage source> .02 *`, Possession's HP-delta expression,
// the Wind Control / Kinetic Assault stack-scalers, …); a prior version SKIPPED them, blanking every
// Expression-typed effect. Thunderspy has no `.powers` def oracle, so this gate is the only structural
// backstop against a silent re-drop of that whole class.
//
// The check is STRUCTURAL — presence, never a scalar value — so it survives balance patches (a
// re-tuned coefficient changes the expression's numbers, not whether it is present; the planner reads
// those numbers live from the re-exported binary, Rule 0). It asserts:
//   1. No Expression-typed template ships with BOTH expression fields blank, except an allowlist of
//      powers whose blank is legitimate (verified against Homecoming or as a rider among non-blank
//      siblings). A systemic re-drop blanks hundreds of templates → the allowlist can't absorb it;
//      a single new drop on any other power trips too.
//   2. The allowlist is not stale: every listed power still exists and still carries exactly the
//      expected number of blank Expression templates (so a fixed blank must be de-listed, and a
//      regression that ADDS blanks to an allowlisted power is not masked).
//   3. The Fury anchor: `Inherent.Inherent.Rage_Buff` carries an Expression template with a non-empty
//      `magnitude_expression` (presence only — the `.02` value is the calc's to read and rebalance).
//
// Run: node scripts/audit-thunderspy-expressions.cjs  (exit 1 on any failure)

const fs = require('fs');
const path = require('path');
const { gateTokens } = require('./_gate-tokens.cjs');

const TSPY_ROOT = path.join(__dirname, '..', 'exported_powers', 'thunderspy');

// Powers whose Expression templates are legitimately blank, with the exact count expected. Keyed by
// full_name so it stays stable across re-exports (template order is positional and not an identity).
//   - Defiance: the Blaster inherent shell. Homecoming's Defiance is ALSO a blank Expression template
//     (the mechanic is driven by the attack powers, not this slot) — verified equal to HC 2026-07-19.
//   - Disrupting_Torrent (Defender/Dominator Kinetic_Assault): a tspy-exclusive set (absent on HC).
//     Two blank rider templates; the load-bearing siblings recovered their expressions. The pair is
//     the two AttribMods of ONE Expression-typed element (Smashing 1.037488 + Energy 0.558647); the
//     Energy half only became visible when the parser started walking every sub-record (TSPY-4). Its
//     RPN arrays are count=0 in its own bytes, and its scale sits at exactly the same Energy/Smashing
//     ratio (0.5385) as the power's plain-magnitude pair — a real damage component, not a drop.
const BLANK_ALLOWLIST = {
  'Inherent.Inherent.Defiance': 1,
  'Defender_Ranged.Kinetic_Assault.Disrupting_Torrent': 2,
  'Dominator_Assault.Kinetic_Assault.Disrupting_Torrent': 2,
};

const FURY_POWER = 'Inherent.Inherent.Rage_Buff';

function* walkTemplates(group) {
  for (const t of group.templates || []) yield t;
  for (const c of group.child_effects || []) yield* walkTemplates(c);
}

function* allTemplates(power) {
  for (const g of power.effects || []) yield* walkTemplates(g);
  for (const g of power.activation_effects || []) yield* walkTemplates(g);
}

function walkFiles(dir, out) {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const f = path.join(dir, e.name);
    if (e.isDirectory()) walkFiles(f, out);
    else if (e.name.endsWith('.json') && !e.name.startsWith('_') && e.name !== 'manifest.json') {
      out.push(f);
    }
  }
}

const failures = [];
const blankByPower = new Map(); // full_name -> blank Expression-template count
let expressionTemplates = 0;
let recoveredExpressions = 0;
let furyExpressionPresent = false;

const files = [];
walkFiles(TSPY_ROOT, files);
for (const file of files) {
  const power = JSON.parse(fs.readFileSync(file, 'utf8'));
  for (const t of allTemplates(power)) {
    if (t.type !== 'Expression') continue;
    expressionTemplates += 1;
    const hasExpr = gateTokens(t.magnitude_expression).length > 0
      || gateTokens(t.duration_expression).length > 0;
    if (hasExpr) recoveredExpressions += 1;
    else blankByPower.set(power.full_name, (blankByPower.get(power.full_name) || 0) + 1);
    if (power.full_name === FURY_POWER && gateTokens(t.magnitude_expression).length) {
      furyExpressionPresent = true;
    }
  }
}

// 1 + 2: blanks must exactly match the allowlist.
for (const [name, count] of blankByPower) {
  const allowed = BLANK_ALLOWLIST[name] || 0;
  if (count > allowed) {
    failures.push(
      `${name}: ${count} blank Expression template(s), allowlist permits ${allowed} — a dropped expression`,
    );
  }
}
for (const [name, expected] of Object.entries(BLANK_ALLOWLIST)) {
  const actual = blankByPower.get(name) || 0;
  if (actual !== expected) {
    failures.push(
      `stale allowlist: ${name} expected ${expected} blank Expression template(s) but found ${actual} — ` +
        `${actual === 0 ? 'the blank is gone; remove the entry' : 'the count changed; re-verify and update'}`,
    );
  }
}

// 3: Fury anchor.
if (!furyExpressionPresent) {
  failures.push(
    `Fury anchor: ${FURY_POWER} carries no Expression template with a magnitude_expression — the ` +
      `Thunderspy parser dropped Rage_Buff's Fury meter expression (DATA-GAP INHERENT-2 regressed)`,
  );
}

if (failures.length) {
  console.error(`FAIL — Thunderspy expression completeness (${failures.length}):`);
  for (const m of failures) console.error('  · ' + m);
  process.exit(1);
}
console.log(
  `OK — Thunderspy expression completeness: ${recoveredExpressions}/${expressionTemplates} ` +
    `Expression-typed templates carry a recovered RPN expression; ` +
    `${Object.keys(BLANK_ALLOWLIST).length} allowlisted blanks accounted for; Fury anchor present.`,
);
