#!/usr/bin/env node
/**
 * audit-dataset-roster.cjs — find the hand-copied dataset rosters a new dataset falls out of.
 *
 * Adding a dataset is two jobs, and only the first one fails loudly. The enum gains a variant
 * and `cargo` stops at every exhaustive `match`, which is Rule 1 working. The second job has no
 * such compiler: a roster written as a LITERAL — `['homecoming', 'rebirth', 'thunderspy']` in a
 * converter, `[DatasetId; 3]` in a corpus gate — keeps its three names, sweeps three datasets,
 * and reports PASS. Nothing is wrong with the output; the fourth dataset simply was not asked
 * about. That is the shape the register calls a silent partition blackout, and it is worse than
 * a red gate because a green one is read as coverage.
 *
 * Measured when `brainstorm` was added on 2026-08-21: 20 script rosters and 22 Rust corpus
 * gates carried a hand-copied three. All 22 Rust files listed the SAME three in the SAME order,
 * which is what a copy looks like — none of them was a deliberate subset. The 69 gates that
 * already read `DatasetId::ALL` swept the new dataset the moment the variant existed.
 *
 *   node scripts/audit-dataset-roster.cjs           # report
 *   node scripts/audit-dataset-roster.cjs --gate    # exit 1 on any finding
 *
 * What it grades: a COLLECTION literal (array, Set) that names all but ONE dataset. That
 * asymmetry is the whole heuristic, and it is worth stating plainly: a literal naming two of
 * four is nearly always a deliberate pair — `[Rebirth, Thunderspy]` is "the Parse6 forks", an
 * answer rather than a roster — while a literal naming three of four is what a roster looks
 * like the day after a fourth dataset arrives. So the gate flags the near-complete list and
 * stays quiet on the small partition. A stale roster that falls TWO datasets behind slips
 * through; that is the accepted cost of not drowning the real finding in fork pairs.
 *
 * What it deliberately does NOT grade, because these are legitimate and common:
 *   - a `match` arm partitioning datasets (`Rebirth | Thunderspy => 10`) — that is a per-fork
 *     ANSWER, and the compiler already forces it to be total;
 *   - a table KEYED by dataset name (`{ homecoming: 405, rebirth: 281 }`). Those need a value
 *     per dataset, but a missing key is a different defect with a different fix, and folding
 *     the two together would bury both. They are listed separately as advisories.
 *
 * Deliberate subsets belong in ALLOWLIST below, each with the reason it is not the whole roster.
 * An entry with no reason is the thing this gate exists to catch, so the reason is required.
 */

'use strict';

const fs = require('fs');
const path = require('path');
const { ALL_DATASETS, REPO_ROOT } = require('./_dataset-paths.cjs');

const GATE = process.argv.includes('--gate');

/** Deliberate subsets: file → why it is not the full roster. */
const ALLOWLIST = {
  // The four fixture emitters USED to sit here, on the reasoning that they dump from the beta's
  // frozen TypeScript calc and that calc has no brainstorm dataset. That was never true of this
  // repo: the emitters import `../src/data/dataset`, which is the vendored calc HERE, and the
  // brainstorm tree was scaffolded into it when BRAIN-1 landed. The allowance outlived its
  // premise and held the door open — three of the four had already been widened by hand while
  // their allowlist rows still said they could not be, and the audit reported the subset it was
  // told to expect rather than the one on disk. An allowance that names a blocker is only worth
  // what re-checking the blocker costs; this one was never re-checked, and the four fixture
  // corpora stayed three-dataset until `movement_gate`'s per-dataset floor said so out loud.
  // Three beta-runtime rows used to sit here, excused because `build:engine` had not yet
  // produced `public/engine/contract/brainstorm.json.gz` and widening them would have pointed
  // the beta at a missing bundle. The bundle landed with the dataset and nobody re-checked the
  // allowance, so the server picker stayed three-dataset and Brainstorm shipped unselectable.
  // All three read DATASET_IDS now. This is the paragraph above, happening again: an allowance
  // is only worth what re-checking its blocker costs.
  'src/utils/per-server-builds.test.ts': 'grades the beta runtime list above, so it moves with it',
  // `tools/mids-oracle/diff_enh_oracle.py` sat here until 2026-09-25, excused on the grounds that
  // the Mids oracle was retired as an authority (register: "Mids retired as an authority",
  // 2026-07-19) and pointing its three-dataset CLI at a fourth would imply a Brainstorm
  // cross-check nobody intends to run. The file is now deleted, so the entry went with it rather
  // than being left behind: this table is only consulted for files the walk FINDS, so an entry
  // whose subject is gone is never read and never reported — a stale allowance is how the next
  // real one gets waved through.
};

/**
 * Keyed TABLES whose reason cannot be written inline, file → why.
 *
 * Separate from ALLOWLIST above, and deliberately so. That one excuses a file's roster
 * literals; folding it onto the keyed tables as well would excuse a file for one shape
 * because it was adjudicated for another — `per-server-builds.test.ts` is allowlisted for
 * the beta runtime list it grades, and blanketing that over its round-trip scenario would
 * drop that table's stale-marker tripwire without saying so.
 *
 * Membership here is not "this table is fine". It is "the reason physically cannot live
 * next to the table", which so far means exactly one thing: the file's BYTES are load-bearing.
 */
const ADVISORY_ALLOWLIST = {
  // `_STRENGTH_ATTRIB_RESOLVER` is keyed on the detected binary FLAVOR, not on a dataset id,
  // and its four keys are exactly what `detect_dataset_flavor` can return — complete by
  // construction, and a `brainstorm` key would be a value the lookup can never ask for.
  //
  // The reason lives here rather than as a `dataset-absent` marker in the file, where every
  // other one of these sits, because `_export_fingerprint` hashes every parser source plus
  // the exporter entry module and each dataset's `_export_manifest.json` records the result.
  // A comment added to that file makes all four committed export trees read STALE and demands
  // a re-export from `.pigg` archives CI does not have. The behaviour is real and still worth
  // avoiding; the note that used to sit here said the marker "reddened `export-staleness.test.ts`
  // on every dataset", which cannot have happened — that file has never existed in this
  // repository. The staleness check is real and lives in
  // `tools/export-integrity.py`, so from here on the consequence is one this repo can observe.
  'tools/bin-crawler/bin_crawler/export_powers.py':
    'flavor-keyed, not dataset-keyed; and its bytes are in the export fingerprint',
};

const SCAN_DIRS = ['scripts', 'src', 'crates', 'tools'];
const SKIP_DIRS = new Set(['node_modules', 'target', 'generated', 'datasets', '.git', 'attic']);
const EXTS = new Set(['.js', '.cjs', '.mjs', '.ts', '.tsx', '.rs', '.py']);

/** `DatasetId::Homecoming` → `homecoming`, for comparing Rust variants against the roster. */
const variantOf = (id) => id.charAt(0).toUpperCase() + id.slice(1);
const VARIANTS = new Map(ALL_DATASETS.map((d) => [variantOf(d), d]));

function walk(dir, out = []) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (entry.name.startsWith('.') || SKIP_DIRS.has(entry.name)) continue;
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) walk(full, out);
    else if (EXTS.has(path.extname(entry.name))) out.push(full);
  }
  return out;
}

/**
 * Collection literals naming datasets. Matched on the bracketed span rather than line by line,
 * because every Rust roster and half the JS ones are written across four lines.
 */
function rostersIn(text, rust) {
  const found = [];
  tupleTables.length = 0;
  // The span cap is deliberately generous. It was 400 and that is how the display slot-presence
  // gate's adjudication table — a `(dataset, power, key, reason)` tuple array longer than 400
  // chars — fell out of the scan entirely and reported nothing, while Brainstorm was in fact
  // missing two of its rows. A cap that silently drops the biggest tables drops exactly the ones
  // worth reading.
  const brackets = text.matchAll(/\[[^[\]]{0,6000}?\]/gs);
  for (const m of brackets) {
    const span = m[0];
    // A ROSTER is a list of datasets to sweep. A TABLE pairs each dataset with an expectation
    // — `[(DatasetId::Homecoming, 612), …]` — and a missing row there is the advisory class
    // below, not this one. They are told apart by the tuple, because the fix differs: a roster
    // is repointed at the one source, while a table needs a MEASURED value per dataset and
    // cannot be filled in mechanically.
    if (/\(\s*DatasetId::[A-Za-z]+\s*,/.test(span)) {
      const line = text.slice(0, m.index).split('\n').length;
      const named = [...span.matchAll(/DatasetId::([A-Za-z]+)/g)]
        .map((v) => VARIANTS.get(v[1]))
        .filter(Boolean);
      const gap = ALL_DATASETS.filter((d) => !named.includes(d));
      const scope = preambleAbove(text, m.index) + span;
      if (named.length >= 2 && (gap.length || HAS_MARKER.test(scope))) {
        tupleTables.push({ line, missing: gap, scope });
      }
      continue;
    }
    const names = new Set();
    if (rust) {
      for (const v of span.matchAll(/DatasetId::([A-Za-z]+)/g)) {
        if (VARIANTS.has(v[1])) names.add(VARIANTS.get(v[1]));
      }
    } else {
      for (const q of span.matchAll(/['"]([a-z]+)['"]/g)) {
        if (ALL_DATASETS.includes(q[1])) names.add(q[1]);
      }
    }
    // A span that is really a table KEYED by dataset belongs to the advisory class below, and
    // saying so here keeps one shape from being reported twice under two different fixes. The
    // widened cap made this reachable: `SURFACES` names three datasets as quoted path segments
    // inside `join(...)` while `homecoming:` is an unquoted key whose path is the tree root, so
    // the quoted-name count reads as a roster missing Homecoming. It is not a roster at all.
    const keyed = ALL_DATASETS.filter((d) => new RegExp(`['"]?${d}['"]?\\s*:`).test(span));
    if (keyed.length >= 2) continue;
    if (names.size === ALL_DATASETS.length - 1) {
      const line = text.slice(0, m.index).split('\n').length;
      found.push({ line, missing: ALL_DATASETS.filter((d) => !names.has(d)) });
    }
  }
  // A Rust roster can also declare its size in the TYPE, which is the form that silently
  // stays at three: `const DATASETS: [DatasetId; 3]`.
  //
  // Both the bare roster and the TUPLE-TABLE form are matched, because only the first was, and
  // the gap cost a runtime panic. A movement-cap parity gate declared
  // `[(DatasetId, usize, usize); 3]` and looped over `DatasetId::ALL`, so Brainstorm found no
  // row and the `.expect` fired. The audit did name both lines — as two entries in an advisory
  // list 74 long, which is a list nobody reads. A literal arity that disagrees with the roster
  // is mechanical and needs no measurement to spot, so it is a GATE finding even though a
  // MISSING ROW in the same table is only advisory: the arity is the part that can be checked
  // without knowing what the right number is. Writing the length as `DatasetId::ALL.len()`
  // demotes the whole class to a compile error, which is where it belongs.
  if (rust) {
    const arity = /\[\s*(?:DatasetId|\(\s*DatasetId\s*,[^;\]]{0,120}?\))\s*;\s*(\d+)\s*\]/g;
    for (const m of text.matchAll(arity)) {
      if (Number(m[1]) !== ALL_DATASETS.length) {
        found.push({
          line: text.slice(0, m.index).split('\n').length,
          missing: [`array typed \`; ${m[1]}\`, roster has ${ALL_DATASETS.length}`],
        });
      }
    }
  }
  return found;
}

/**
 * `match dataset { DatasetId::X => …, _ => … }` — a wildcard arm partitioning datasets.
 *
 * This is the one shape that defeats Rule 1's exhaustive match, and it is worse than a stale
 * roster because it does not merely skip the new dataset: it silently ASSIGNS it another
 * dataset's answer. Both times it bit here the wrong answer was a fork's — Brainstorm was
 * handed Thunderspy's snipe floor of `(0, 8)`, which a dataset trading damage on all 48 of its
 * snipes clears while grading none of them. A vacuous pass, reported as coverage.
 *
 * `_ => panic!(…)` / `unreachable!(…)` is exempt: refusing an unexpected dataset is the
 * opposite failure mode, and it is loud.
 */
function wildcardArmsIn(text) {
  const out = [];
  for (const m of text.matchAll(/^[ \t]*_ =>(.*)$/gm)) {
    if (/panic!|unreachable!|todo!/.test(m[1])) continue;
    const before = text.slice(Math.max(0, m.index - 1200), m.index);
    const lastMatch = before.lastIndexOf('match ');
    if (lastMatch === -1) continue;
    if (!/DatasetId::[A-Za-z]+\s*(=>|\|)/.test(before.slice(lastMatch))) continue;
    out.push({ line: text.slice(0, m.index).split('\n').length });
  }
  return out;
}

/**
 * The span a table occupies, bracket-matched from its opening `{` / `[`.
 *
 * This used to be a flat 600-char slice, and the cut is what made a third of the advisory
 * bucket false. A table whose last dataset row sits behind a comment paragraph — which is
 * exactly what a MEASURED row looks like, since the measurement has to be written down
 * somewhere — falls outside the window and reports as missing. `audit-conditional-coverage`'s
 * `EXPECTED` carried a brainstorm row measured off the i28p4 beta and was reported missing it
 * for eight lines of provenance. `rostersIn` learned this and got a generous span; this reader
 * kept the cut, and every advisory it emitted had to be hand-checked against the file.
 *
 * Returns null when no close bracket is found inside CAP, and the caller reports THAT rather
 * than dropping the table. A runaway span reads every dataset as present and goes quiet, which
 * is the direction that hides findings.
 */
const SPAN_CAP = 20000;
function balancedSpan(text, start) {
  const open = text[start], close = open === '{' ? '}' : ']';
  let depth = 0;
  const limit = Math.min(text.length, start + SPAN_CAP);
  for (let i = start; i < limit; i++) {
    if (text[i] === open) depth++;
    else if (text[i] === close && --depth === 0) return text.slice(start, i);
  }
  return null;
}

/**
 * A table's own written reason for not naming a dataset, in the form the audit can read:
 *
 *   // dataset-absent: thunderspy — tspy rebalanced Speed Boost to ['Friend','Self']
 *
 * The bucket these live in used to be advisory, which is a silent skip with a label on it, and
 * the register carries that lesson three times over. Making it gate needs a way to tell an
 * absence somebody MEASURED from one nobody has looked at, and prose alone cannot do that: half
 * these tables already carried a perfectly good reason in the comment above them, and the audit
 * reported them exactly like the ones that carried nothing.
 *
 * The marker is deliberately not an allowlist keyed by file and line. A line number drifts on
 * the next edit, and a reason kept in another file is a reason the next editor of the table does
 * not see — which is how `per-server-builds.test.ts` stayed three-dataset behind an allowance
 * written twelve lines above the rows it excused (BRAIN-12).
 *
 * Read from the table's span AND from the comment block immediately above it, because that is
 * where a reason is naturally written. The preamble walk stops at the first line that is not a
 * comment or blank, so a marker never leaks from one table to the next.
 */
const MARKER = /dataset-absent:\s*([a-z, ]+?)\s*(?:—|--|-)\s*(\S)/g;
/**
 * Non-global twin, used to decide whether a table is worth REPORTING even when it is
 * complete. Without this the stale check is unreachable: a table that gains its last
 * dataset stops being emitted at all, so the marker excusing the row it just gained is
 * never looked at again. Measured — the tripwire was written, and did not fire.
 */
const HAS_MARKER = /dataset-absent:/;

/** The contiguous comment block directly above `index`, or ''. */
function preambleAbove(text, index) {
  const lines = text.slice(0, index).split('\n');
  lines.pop();
  const kept = [];
  for (let i = lines.length - 1; i >= 0; i--) {
    const t = lines[i].trim();
    if (t === '' || t.startsWith('//') || t.startsWith('*') || t.startsWith('/*') || t.startsWith('#')) {
      kept.unshift(lines[i]);
    } else break;
  }
  return kept.join('\n');
}

/** Datasets a span (plus its preamble) declares deliberately absent. */
function adjudicatedIn(scope) {
  const named = new Set();
  for (const m of scope.matchAll(MARKER)) {
    for (const d of m[1].split(',').map((x) => x.trim())) {
      if (ALL_DATASETS.includes(d)) named.add(d);
    }
  }
  return named;
}

/** Tables keyed by dataset name — advisory: a missing key is a different defect. */
function keyedTablesIn(text) {
  const out = [];
  for (const m of text.matchAll(/[{[]\s*(?:\/\/[^\n]*\n\s*)*['"]?homecoming['"]?\s*:/g)) {
    const line = text.slice(0, m.index).split('\n').length;
    const span = balancedSpan(text, m.index);
    if (span === null) {
      out.push({ line, missing: [`table span unclosed within ${SPAN_CAP} chars — unread`] });
      continue;
    }
    const present = ALL_DATASETS.filter((d) => new RegExp(`['"]?${d}['"]?\\s*:`).test(span));
    const missing = ALL_DATASETS.filter((d) => !present.includes(d));
    const scope = preambleAbove(text, m.index) + span;
    if (present.length >= 2 && (missing.length || HAS_MARKER.test(scope))) {
      out.push({ line, missing, scope });
    }
  }
  return out;
}

/**
 * The same defect one level down: a pin table whose key is `<dataset>|<power>|<axis>`
 * rather than a bare dataset name. `keyedTablesIn` can't see these, because the
 * character after the dataset name is a delimiter and not a colon, so
 * planb-shadow-movement's three TARGETS-3 tables sat a dataset behind while its two
 * sibling gates were flagged and widened (MOVE-4).
 *
 * Scoped to a bracket-matched `const NAME = { ... }` / `[ ... ]` so the whole table is one
 * span (the sibling gates pin theirs as arrays), which is the same `balancedSpan` the keyed
 * reader above now uses.
 */
function compositeKeyedTablesIn(text) {
  const out = [];
  for (const m of text.matchAll(/const\s+\w+\s*=\s*[{[]/g)) {
    const start = m.index + m[0].length - 1;
    const span = balancedSpan(text, start);
    if (span === null) continue;
    const present = ALL_DATASETS.filter((d) => new RegExp(`['"\`]${d}\\|`).test(span));
    const missing = ALL_DATASETS.filter((d) => !present.includes(d));
    const scope = preambleAbove(text, m.index) + span;
    if (present.length >= 2 && (missing.length || HAS_MARKER.test(scope))) {
      out.push({ line: text.slice(0, m.index).split('\n').length, missing, scope });
    }
  }
  return out;
}

const findings = [];
const advisories = [];
/** Filled by `rostersIn`: `(DatasetId, expectation)` tables missing a dataset. */
const tupleTables = [];

for (const dir of SCAN_DIRS) {
  const root = path.join(REPO_ROOT, dir);
  if (!fs.existsSync(root)) continue;
  for (const file of walk(root)) {
    const rel = path.relative(REPO_ROOT, file);
    if (rel === path.join('scripts', 'audit-dataset-roster.cjs')) continue;
    const text = fs.readFileSync(file, 'utf-8');
    if (!ALL_DATASETS.some((d) => text.includes(d) || text.includes(variantOf(d)))) continue;
    const rust = file.endsWith('.rs');
    for (const hit of rostersIn(text, rust)) {
      if (ALLOWLIST[rel]) continue;
      findings.push({ rel, ...hit });
    }
    if (!ADVISORY_ALLOWLIST[rel]) for (const hit of tupleTables) advisories.push({ rel, ...hit });
    if (rust) {
      for (const hit of wildcardArmsIn(text)) {
        if (!ALLOWLIST[rel]) findings.push({ rel, ...hit, missing: ['`_ =>` arm over datasets'] });
      }
    }
    if (!ADVISORY_ALLOWLIST[rel]) {
      for (const hit of keyedTablesIn(text)) advisories.push({ rel, ...hit });
      for (const hit of compositeKeyedTablesIn(text)) advisories.push({ rel, ...hit });
    }
  }
}

/**
 * Sort the keyed tables by what their own text says about the gap. Three buckets, and only the
 * first two are silent: a declared absence, a table that has since GAINED the row its marker
 * excuses, and a gap nobody has written anything about.
 *
 * The stale bucket is the tripwire, and it is the reason a marker beats an allowlist. A reason
 * that outlives its cause is the failure this file's header describes twice — the fixture
 * emitters and the beta runtime rows both sat behind an allowance whose blocker had been gone
 * for weeks. A marker naming a dataset the table now carries is that state, detectable.
 */
const declared = [];
const stale = [];
const undeclared = [];
for (const a of advisories) {
  const named = adjudicatedIn(a.scope || '');
  const unexplained = a.missing.filter((d) => !named.has(d));
  const present = ALL_DATASETS.filter((d) => !a.missing.includes(d));
  const outlived = [...named].filter((d) => present.includes(d));
  if (outlived.length) stale.push({ ...a, outlived });
  if (unexplained.length) undeclared.push({ ...a, unexplained });
  else if (!outlived.length && a.missing.length) declared.push({ ...a, named: [...named] });
}

console.log(`roster: ${ALL_DATASETS.join(', ')}`);
console.log(`allowlisted files: ${Object.keys(ALLOWLIST).length} roster, `
  + `${Object.keys(ADVISORY_ALLOWLIST).length} keyed-table`);
for (const [file, why] of Object.entries(ALLOWLIST)) console.log(`  roster      ${file} — ${why}`);
for (const [file, why] of Object.entries(ADVISORY_ALLOWLIST)) {
  console.log(`  keyed table ${file} — ${why}`);
}

console.log(`\nkeyed tables with a declared absence (${declared.length}):`);
for (const a of declared) console.log(`  ${a.rel}:${a.line}  ${a.named.join(', ')} — declared absent`);
if (!declared.length) console.log('  none');

console.log(`\nkeyed tables missing a dataset, undeclared (${undeclared.length}):`);
for (const a of undeclared) console.log(`  ${a.rel}:${a.line}  missing ${a.unexplained.join(', ')}`);
if (!undeclared.length) console.log('  none — every keyed table names the whole set or says why not.');

console.log(`\nstale \`dataset-absent\` markers (${stale.length}):`);
for (const a of stale) {
  console.log(`  ${a.rel}:${a.line}  declares ${a.outlived.join(', ')} absent, but the table has it`);
}
if (!stale.length) console.log('  none');

console.log(`\npartial rosters (${findings.length}):`);
for (const f of findings) console.log(`  ${f.rel}:${f.line}  missing ${f.missing.join(', ')}`);
if (!findings.length) console.log('  none — every roster literal names the whole set.');

const failures = findings.length + undeclared.length + stale.length;
if (GATE && failures) {
  if (findings.length) {
    console.error(
      `\nGATE FAIL — ${findings.length} literal roster(s) name a subset of the datasets. ` +
        `Point them at ALL_DATASETS / DatasetId::ALL, or allowlist each with its reason.`,
    );
  }
  if (undeclared.length) {
    console.error(
      `GATE FAIL — ${undeclared.length} keyed table(s) miss a dataset with nothing written ` +
        `about why. Measure the row, or mark it \`dataset-absent: <id> — <reason>\`.`,
    );
  }
  if (stale.length) {
    console.error(
      `GATE FAIL — ${stale.length} \`dataset-absent\` marker(s) name a dataset the table now ` +
        `carries. The reason outlived its cause; delete the marker.`,
    );
  }
  process.exit(1);
}
console.log(GATE ? '\nGATE PASS' : '');
