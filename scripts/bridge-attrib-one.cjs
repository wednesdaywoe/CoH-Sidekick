#!/usr/bin/env node
// Resolve one (attrib, aspect, table) tuple via DSH4 bridgeAttrib.
// Usage:
//   node scripts/bridge-attrib-one.cjs '<json>'
// where json is: {"attrib":"Smashing_Dmg","aspect":"Resistance","table":"Ranged_Res_Dmg"}

require('tsx/cjs');
const { bridgeAttrib } = require('./_atomic-effect.ts');

function main() {
  const raw = process.argv[2];
  if (!raw) {
    console.error('missing JSON arg');
    process.exit(2);
  }
  let req;
  try {
    req = JSON.parse(raw);
  } catch (e) {
    console.error(`invalid JSON: ${e.message}`);
    process.exit(2);
  }
  const attrib = req.attrib || '';
  const aspect = req.aspect || '';
  const table = req.table || '';
  const result = bridgeAttrib(attrib, aspect, table);
  process.stdout.write(JSON.stringify(result));
}

main();
