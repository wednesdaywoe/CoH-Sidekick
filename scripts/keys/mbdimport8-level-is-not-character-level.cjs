#!/usr/bin/env node
// MBDIMPORT-8's key: the corpus still contains a COMPLETE build whose `Level` field is not 50.
//
// The row's door-closing claim is "a .mbd's `Level` is not the character level, so do not read it
// plainly." What makes that checkable rather than merely asserted is the Warshade: its author
// confirmed it as a finished level-50 character, it holds a full 67 placed slots, and it reads
// `Level: "48"`. The slots-only twin is the control for that confirmation — the same character
// re-picked from scratch with every slot placed by hand, reaching the same 67 at the same levels
// — so the claim does not rest on one file being remembered correctly.
//
// If the corpus ever stops carrying such a file, the claim is ungradeable — not disproved, and
// not fixed — and the next session would be free to "simplify" the derivation back into the bug
// this row exists to prevent.
const fs = require('fs');
const path = require('path');

const REPO = path.join(__dirname, '../..');
const dir = path.join(REPO, 'fixtures/mids/homecoming');

let witnesses = 0;
for (const file of fs.readdirSync(dir).sort()) {
  if (!file.endsWith('.mbd')) continue;
  const mbd = JSON.parse(fs.readFileSync(path.join(dir, file), 'utf8').replace(/^\uFEFF/, ''));
  const placed = mbd.PowerEntries.reduce(
    (n, e) => n + Math.max(0, e.SlotEntries.length - 1),
    0,
  );
  const stated = Number(mbd.Level);
  // 67 placed slots is the full complement, which only a level-50 character has.
  const complete = placed === 67;
  if (complete && stated + 1 < 50) {
    witnesses += 1;
    console.log(`${file}: Level=${stated} (reads as ${stated + 1}) but holds ${placed} placed slots — a complete level-50 build`);
  } else {
    console.log(`${file}: Level=${stated}, ${placed} placed slots`);
  }
}

console.log(`\nfiles proving Level is not the character level: ${witnesses}`);
if (witnesses === 0) {
  console.error("BROKEN: no corpus file pairs a sub-50 `Level` with a complete slot complement.");
  process.exitCode = 1;
}
