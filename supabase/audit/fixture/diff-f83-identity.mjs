#!/usr/bin/env node
/**
 * SECURITY_AUDIT.md F83 — grade the generated SQL against the TypeScript it
 * was generated from, on strings neither of them was derived per-character on.
 *
 *   cd supabase/audit/fixture && ./build.sh
 *   node --experimental-strip-types diff-f83-identity.mjs
 *
 * **Why multi-character strings and not a code-point sweep.** The generator
 * classifies one code point at a time, so a per-code-point comparison would be
 * asking the derivation to confirm itself. What it cannot confirm is ORDER:
 * whether a separator leaves a gap before an invisible is taken, whether a
 * collapse that runs too early leaves two spaces, whether a prefix strip that
 * runs before the collapse sees a character the collapse would have removed.
 * `author-name.ts` calls that ordering load-bearing and records getting it
 * wrong. Every string below is drawn to put the four steps in each other's way.
 *
 * Characters are written as code-point NUMBERS throughout. Escapes in a source
 * file that reaches here through several layers of quoting are decoded by some
 * of them, and a corpus that arrives already mangled compares two mangled
 * inputs and agrees.
 */
import { execFileSync } from 'node:child_process';

const { normalizeIdentityName } = await import('../../functions/_shared/author-name.ts');

const U = (...cps) => cps.map((cp) => String.fromCodePoint(cp)).join('');

/** Weighted towards the characters the rule has something to say about. */
const ALPHABET = [
  ...'abcSavant019@@  ',
  U(0x09), U(0x0a), U(0x0b), U(0x0d), U(0x2028), U(0x2029),          // separators
  U(0x200b), U(0x200c), U(0x200d), U(0x2060), U(0xfeff), U(0x00ad),  // zero-width
  U(0x202a), U(0x202b), U(0x202c), U(0x202d), U(0x202e), U(0x2066), U(0x2069), // bidi
  U(0x3164), U(0x115f), U(0x1160), U(0xffa0), U(0x2800),             // blank letters
  U(0x00a0), U(0x2007), U(0x3000),                                   // space impostors
  U(0x0301), U(0x0300), U(0x0340),                                   // combining, incl. an NFC singleton
  U(0x00e9), U(0x65, 0x0301),                                        // composed and decomposed
  U(0x1f600), U(0x1d400), U(0x4e00), U(0x1f),                        // astral, CJK, a control
  // NOT U+0000. Postgres text cannot hold a NUL at all -- `E'\\u0000'` is
  // refused by the parser, not by the rule -- so it can never reach the seed
  // path, and putting it in the corpus would grade the database's type system
  // rather than the two implementations of the rule.

];

function draw(rng, n) {
  let out = '';
  for (let i = 0; i < n; i += 1) out += ALPHABET[Math.floor(rng() * ALPHABET.length)];
  return out;
}

/** Deterministic, so a failure is reproducible from the seed alone. */
function mulberry32(seed) {
  return () => {
    seed = (seed + 0x6d2b79f5) | 0;
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** The cases with names: both historical bypasses, and the welding case. */
const CASES = [
  '@savant', '@ @savant', U(0x3164) + '@admin', '@' + U(0x3164) + '@admin',
  'Savant' + U(0x0a, 0x0a) + 'Administrator', '  spaced ' + U(0x00a0) + ' out  ',
  U(0x2800) + '@admin', '', '   ', '@@@', U(0x200b), U(0x65, 0x0301) + 'clair',
];
const rng = mulberry32(20260923);
for (let i = 0; i < 20_000; i += 1) CASES.push(draw(rng, 1 + Math.floor(rng() * 12)));

/**
 * Every code point escaped, in both directions. The corpus is full of
 * characters a shell, psql and a terminal each mangle differently; the input
 * goes in as escapes and the answer comes back as hex, so nothing in the
 * middle can quietly alter what is being compared.
 */
function literal(s) {
  const body = Array.from(s).map((c) => {
    const cp = c.codePointAt(0);
    const hex = cp.toString(16);
    return cp <= 0xffff
      ? String.raw`\u` + hex.padStart(4, '0')
      : String.raw`\U` + hex.padStart(8, '0');
  }).join('');
  return `E'${body}'`;
}

const expected = new Map(CASES.map((s, i) => [i, normalizeIdentityName(s)]));
const rows = CASES.map((s, i) => `(${i}, ${literal(s)})`).join(',\n');
const sql = [
  String.raw`\pset tuples_only on`,
  String.raw`\pset format unaligned`,
  `WITH corpus(i, raw) AS (VALUES\n${rows}\n)`,
  `SELECT i || chr(9) || encode(convert_to(identity_name_normalized(raw), 'UTF8'), 'hex') FROM corpus ORDER BY i;`,
].join('\n');

const out = execFileSync('docker',
  ['exec', '-i', 'sk-sqlcheck', 'psql', '-v', 'ON_ERROR_STOP=1', '-U', 'postgres', '-d', 'skcheck', '-f', '-'],
  { input: sql, encoding: 'utf8', maxBuffer: 1 << 28 });

let checked = 0;
const bad = [];
for (const line of out.split('\n')) {
  const [i, hex] = line.split('\t');
  if (hex === undefined) continue;
  const got = Buffer.from(hex, 'hex').toString('utf8');
  checked += 1;
  if (got !== expected.get(Number(i))) bad.push({ i: Number(i), want: expected.get(Number(i)), got });
}

console.log(`compared ${checked} strings through both implementations`);
// A partial answer that reads as a pass is the failure mode this guards.
if (checked !== CASES.length) {
  console.error(`only ${checked} of ${CASES.length} came back; this comparison proved less than it looks`);
  process.exit(2);
}
if (bad.length === 0) { console.log('no disagreement'); process.exit(0); }

const show = (s) => Array.from(s).map((c) => {
  const cp = c.codePointAt(0);
  return cp > 0x20 && cp < 0x7f ? c : `<U+${cp.toString(16).toUpperCase().padStart(4, '0')}>`;
}).join('');
console.error(`${bad.length} disagreements, first 10:`);
for (const b of bad.slice(0, 10)) {
  console.error(`  in  ${show(CASES[b.i])}\n    ts  ${show(b.want)}\n    sql ${show(b.got)}`);
}
process.exit(1);
