#!/usr/bin/env node
/**
 * Key for DATA-GAP MBDEXPORT-17: every level-less piece we write is stamped `IoLevel: 0`.
 *
 * Run from the repo root:  node scripts/keys/mbdexport17-iolevel-spelling-census.cjs
 *
 * The row's door-closing claim is that this divergence is INVISIBLE to the round trip. That is
 * why it needs a key: `mbd_writer_roundtrip` is green on all 253 of these slots, because our
 * reader normalises all three kinds back — an attuned piece drops its level, an origin and a
 * special never had one — so the two spellings import to the same build and nothing reds. A
 * count taken from the round trip would read zero and the row would look closed.
 *
 * So the census is taken from the FILES, pairing each slot by (power, slot index) and comparing
 * `IoLevel` for the same `Uid`. It prints the population split by what the piece is, because the
 * three kinds may not have one answer: Mids writes an attuned piece at the level the author
 * placed it (49 on a level-50 build, 9 on a level-10 one) and an origin or special at 1.
 *
 * What BREAKS the claim: a non-zero count in the `ours` column for anything other than 0. That
 * would mean our writer has started stating a level for these and the row is about something
 * else. A total that is not 253 means the population moved and the row's measurement is stale.
 */
const fs = require('node:fs');
const path = require('node:path');

const ROOT = path.join(__dirname, '..', '..', 'fixtures', 'mids');
const FORKS = ['homecoming', 'rebirth'];

/** Every filled slot in a `.mbd`, keyed by the power and the slot's position in it. */
function slots(doc) {
  const out = new Map();
  for (const power of doc.PowerEntries ?? []) {
    (power.SlotEntries ?? []).forEach((slot, index) => {
      const enh = slot.Enhancement;
      if (enh && enh.Uid) out.set(`${power.PowerName}#${index}`, enh);
    });
  }
  return out;
}

/** Which of the three level-less kinds a Mids UID names, or `io-set` for a leveled one. */
function kindOf(uid) {
  if (/^(Synthetic_Hamidon|Hamidon|Titan|Hydra|DSync|Dsync|Generic)_/.test(uid)) return 'special';
  if (/^(Magic|Mutation|Natural|Science|Technology)_/.test(uid)) return 'origin';
  return 'io-set';
}

let total = 0;
const byKind = new Map();
const perFile = [];

for (const file of fs.readdirSync(path.join(ROOT, 'ours')).sort()) {
  const fork = FORKS.find((f) => fs.existsSync(path.join(ROOT, f, file)));
  if (!fork) throw new Error(`${file}: no corpus twin — the fixtures are out of step`);
  const theirs = slots(JSON.parse(fs.readFileSync(path.join(ROOT, fork, file), 'utf8')));
  const ours = slots(JSON.parse(fs.readFileSync(path.join(ROOT, 'ours', file), 'utf8')));

  let n = 0;
  for (const [key, mine] of ours) {
    const twin = theirs.get(key);
    if (!twin || twin.Uid !== mine.Uid || twin.IoLevel === mine.IoLevel) continue;
    n += 1;
    const tag = `${kindOf(mine.Uid)}  ours ${mine.IoLevel} / Mids ${twin.IoLevel}`;
    byKind.set(tag, (byKind.get(tag) ?? 0) + 1);
  }
  perFile.push([`${fork}/${file}`, n, ours.size]);
  total += n;
}

console.log(`MBDEXPORT-17: ${total} slots where our IoLevel differs from Mids' for the same piece\n`);
for (const [file, n, of] of perFile) console.log(`  ${String(n).padStart(3)} / ${String(of).padEnd(3)}  ${file}`);
console.log('');
for (const [tag, n] of [...byKind].sort()) console.log(`  ${String(n).padStart(3)}  ${tag}`);

const stated = [...byKind.keys()].filter((tag) => !tag.includes('ours 0 /'));
if (stated.length) {
  console.log(`\nBROKEN: our writer states a level for ${stated.length} of these kinds — ${stated.join(', ')}`);
  console.log('The row says every level-less piece goes out as 0. It no longer does.');
}
