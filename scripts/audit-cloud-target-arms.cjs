/**
 * Cloud layer target arms — does the client carry a per-platform branch it should not?
 *
 * The defect class is RB4a's: a cloud client written behind `cfg(target_arch = "wasm32")` and
 * ported to desktop later, with a `None` twin standing in for the platform that got nothing. It
 * is the slot-drag defect with a bigger blast radius, and it is discovered at the point desktop
 * tries to sign in — which is after the web work has been declared done. `crates/app/src/cloud/`
 * names no target, and its module doc makes a rule of it.
 *
 * This replaces the shell one-liner that rule shipped with:
 *
 *     test -d crates/app/src/cloud/ && grep -rn 'target_arch' crates/app/src/cloud/ |
 *       grep -vE '^[^:]+:[0-9]+:\s*[/][/]' | wc -l      # -> 0
 *
 * (`[/][/]` there is the original's bare `//`, spelled so it cannot close this comment.)
 *
 * Both of that command's filters were bought by a failure and both are kept here: the `test -d`,
 * because an unguarded grep over a missing path prints 0 and reads exactly like a pass; and the
 * comment strip, because the bare grep printed 3 against a clean module whose own doc explains
 * the rule, and a gate whose cheapest green is deleting that paragraph is worse than no gate.
 *
 * What it could not do is tell a client from a test. It went red on 2026-09-18 for
 * `session.rs`'s `a_sign_out_dropped_mid_request_leaves_the_session_standing`, which is
 * `#[cfg(not(target_arch = "wasm32"))]` because it binds a real `TcpListener` and the observable
 * it grades IS a socket. Nothing about the shape of the layer had changed. A gate that is red for
 * something nobody can act on is the failure the register's frontier leads with — a key rotting
 * in the direction nobody looks — so the choice is between a gate that can read Rust scope and no
 * gate at all.
 *
 * So: occurrences inside a `#[cfg(test)]` module are a different population from occurrences in
 * the client, counted and printed separately, and only the client's fail. A test arm still has to
 * say why it is one — the reason has to be readable where the arm is, which is ROSTER-3's lesson
 * and the audit's allowlist paragraphs twice over. A reason living in a stream file is a reason
 * nobody re-checks.
 *
 * It prints its population every run, including the file count, because the house rule this
 * replaces a one-liner under is that a check which can fail silently is not a check: a scan that
 * matches nothing must be distinguishable from a scan that found nothing wrong.
 *
 * Usage:
 *   node scripts/audit-cloud-target-arms.cjs [--gate] [--verbose]
 *
 * Exits non-zero under --gate when the directory is missing, no files are scanned, a client-scope
 * arm exists, or a test-scope arm carries no reason.
 */

const fs = require('node:fs');
const path = require('node:path');

const ROOT = path.resolve(__dirname, '..');
const DIR = path.join(ROOT, 'crates/app/src/cloud');

const args = process.argv.slice(2);
const GATE = args.includes('--gate');
const VERBOSE = args.includes('--verbose');

/** The needle. Matches `target_arch = "wasm32"` and its `not(...)` form alike. */
const NEEDLE = 'target_arch';

/**
 * How far above an arm a reason may sit and still count as attached to it. Four lines covers an
 * attribute stack (`#[cfg]` + `#[test]`) under a one-line comment without reaching the previous
 * item's trailing prose.
 */
const REASON_WINDOW = 4;

/** A reason has to mention what makes the arm native, not merely be a comment. */
const REASON_SHAPE = /native|wasm|socket|listener|tcp|webview|browser-only|no runtime/i;

/**
 * Classify every byte of a Rust source file as code or not-code, and track brace depth over the
 * code bytes only.
 *
 * Brace counting is the whole mechanism here, so it cannot be fooled by a `{` inside a string or
 * a comment. Rust needs four states for that — line comment, block comment (which nests), string
 * (with raw and escaped forms), char — and getting any of them wrong moves a scope boundary,
 * which silently reclassifies an arm rather than erroring.
 *
 * @param {string} src
 * @returns {{ depth: number[], code: boolean[] }} per-character brace depth and code-ness
 */
function scan(src) {
  const depth = new Array(src.length).fill(0);
  const code = new Array(src.length).fill(false);

  let i = 0;
  let d = 0;
  let block = 0; // block-comment nesting

  while (i < src.length) {
    const c = src[i];
    const next = src[i + 1];

    if (block > 0) {
      if (c === '/' && next === '*') { block += 1; i += 2; continue; }
      if (c === '*' && next === '/') { block -= 1; i += 2; continue; }
      i += 1;
      continue;
    }

    // Line comment — runs to the newline. Covers `//`, `///` and `//!` alike.
    if (c === '/' && next === '/') {
      while (i < src.length && src[i] !== '\n') i += 1;
      continue;
    }

    if (c === '/' && next === '*') { block = 1; i += 2; continue; }

    // Raw string: r"..." or r#"..."# with any number of hashes.
    if (c === 'r' && (next === '"' || next === '#')) {
      let j = i + 1;
      let hashes = 0;
      while (src[j] === '#') { hashes += 1; j += 1; }
      if (src[j] === '"') {
        const close = '"' + '#'.repeat(hashes);
        const end = src.indexOf(close, j + 1);
        i = end === -1 ? src.length : end + close.length;
        continue;
      }
    }

    if (c === '"') {
      i += 1;
      while (i < src.length) {
        if (src[i] === '\\') { i += 2; continue; }
        if (src[i] === '"') { i += 1; break; }
        i += 1;
      }
      continue;
    }

    // Char literal or a lifetime — `'a` is not a string, so only consume a real literal.
    if (c === "'") {
      const closes = src[i + 1] === '\\' ? src.indexOf("'", i + 3) : (src[i + 2] === "'" ? i + 2 : -1);
      if (closes !== -1) { i = closes + 1; continue; }
      i += 1;
      continue;
    }

    if (c === '{') d += 1;
    code[i] = true;
    depth[i] = d;
    if (c === '}') d -= 1;
    i += 1;
  }

  return { depth, code };
}

/**
 * Character offset of the start of each line, so an offset can be turned into a line number and
 * back without re-splitting the source.
 *
 * @param {string} src
 * @returns {number[]}
 */
function lineStarts(src) {
  const starts = [0];
  for (let i = 0; i < src.length; i += 1) if (src[i] === '\n') starts.push(i + 1);
  return starts;
}

/**
 * Spans of every `#[cfg(test)]` module in a file, as character offsets.
 *
 * Anchored on the attribute rather than on `mod tests`, because the name is a convention and the
 * attribute is the thing that decides whether the code compiles into a test binary.
 *
 * @param {string} src
 * @param {{ depth: number[], code: boolean[] }} scanned
 * @returns {{ start: number, end: number }[]}
 */
function testSpans(src, scanned) {
  const spans = [];
  const attr = /#\[\s*cfg\s*\(\s*test\s*\)\s*\]/g;
  let m;

  while ((m = attr.exec(src)) !== null) {
    if (!scanned.code[m.index]) continue; // the attribute itself was inside a comment

    // The brace that opens the item this attribute decorates.
    let open = -1;
    for (let i = m.index + m[0].length; i < src.length; i += 1) {
      if (!scanned.code[i]) continue;
      if (src[i] === ';') break;       // an attributed item with no block
      if (src[i] === '{') { open = i; break; }
    }
    if (open === -1) continue;

    // `scan` records an opening brace AFTER the increment and a closing brace BEFORE the
    // decrement, so a `{` and its partner carry the same depth. Comparing against one less than
    // the opener — the enclosing level — matches nothing here and runs the span to end-of-file,
    // which swallows the rest of the module and reads every client arm below it as a test arm.
    // Caught by the first mutation: a `#[cfg(target_arch)]` appended to a file whose test module
    // sits above it was reported as an unreasoned test arm rather than as the violation it is.
    const level = scanned.depth[open];
    let close = src.length;
    for (let i = open + 1; i < src.length; i += 1) {
      if (!scanned.code[i]) continue;
      if (src[i] === '}' && scanned.depth[i] === level) { close = i; break; }
    }
    spans.push({ start: m.index, end: close });
  }

  return spans;
}

/** @returns {string[]} every .rs file under DIR, recursively */
function rustFiles(dir) {
  const out = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) out.push(...rustFiles(full));
    else if (entry.name.endsWith('.rs')) out.push(full);
  }
  return out;
}

function main() {
  if (!fs.existsSync(DIR) || !fs.statSync(DIR).isDirectory()) {
    console.error(`MISSING: ${path.relative(ROOT, DIR)} does not exist.`);
    console.error('An absent directory is not a clean one. If the cloud layer moved, this gate moves with it.');
    process.exit(GATE ? 1 : 0);
  }

  const files = rustFiles(DIR);
  const client = [];
  const tests = [];
  const commented = [];

  for (const file of files) {
    const src = fs.readFileSync(file, 'utf8');
    const scanned = scan(src);
    const starts = lineStarts(src);
    const spans = testSpans(src, scanned);
    const lines = src.split('\n');

    let from = 0;
    for (;;) {
      const at = src.indexOf(NEEDLE, from);
      if (at === -1) break;
      from = at + NEEDLE.length;

      const lineNo = starts.findIndex((s, n) => s <= at && (starts[n + 1] === undefined || starts[n + 1] > at)) + 1;
      const rel = path.relative(ROOT, file);
      const text = (lines[lineNo - 1] || '').trim();
      const hit = { rel, lineNo, text };

      if (!scanned.code[at]) { commented.push(hit); continue; }

      const inTest = spans.some((s) => at >= s.start && at <= s.end);
      if (!inTest) { client.push(hit); continue; }

      const above = lines.slice(Math.max(0, lineNo - 1 - REASON_WINDOW), lineNo - 1).join('\n');
      hit.reason = REASON_SHAPE.test(above);
      tests.push(hit);
    }
  }

  const unreasoned = tests.filter((t) => !t.reason);

  console.log(`Scanned ${files.length} file(s) under ${path.relative(ROOT, DIR)}/ for \`${NEEDLE}\`.`);
  console.log(`  client-scope arms : ${client.length}   (must be 0 — this is the rule)`);
  console.log(`  test-scope arms   : ${tests.length}   (allowed, each with a reason beside it)`);
  console.log(`  in comments       : ${commented.length}   (the module doc explaining the rule)`);

  if (VERBOSE) {
    for (const h of commented) console.log(`    comment  ${h.rel}:${h.lineNo}  ${h.text}`);
  }
  for (const h of tests) {
    console.log(`    test     ${h.rel}:${h.lineNo}  ${h.reason ? 'reason present' : 'NO REASON'}`);
  }
  for (const h of client) {
    console.log(`    CLIENT   ${h.rel}:${h.lineNo}  ${h.text}`);
  }

  if (files.length === 0) {
    console.error('\nERROR: no .rs files scanned. A scan that matches nothing is not a clean scan.');
    process.exit(GATE ? 1 : 0);
  }

  let bad = false;

  if (client.length > 0) {
    bad = true;
    console.error(`\nERROR: ${client.length} client-scope target arm(s) in the cloud layer.`);
    console.error('The layer names no target — see the "Why this file names no target" section of');
    console.error('crates/app/src/cloud/mod.rs. A per-platform branch here is the slot-drag defect.');
  }

  if (unreasoned.length > 0) {
    bad = true;
    console.error(`\nERROR: ${unreasoned.length} test-scope target arm(s) with no reason above them.`);
    for (const h of unreasoned) console.error(`  ${h.rel}:${h.lineNo}`);
    console.error(`Put one line within ${REASON_WINDOW} lines above the attribute saying what makes the`);
    console.error('test native — a socket, a listener, a runtime. A reason that lives in a stream file');
    console.error('is a reason nobody re-checks.');
  }

  if (bad) process.exit(GATE ? 1 : 0);

  console.log('\nOK — the cloud layer names no target.');
}

main();
