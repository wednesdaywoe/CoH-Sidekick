/**
 * Re-convert all powersets that contain powers with redirect arrays.
 * Run after updating convert-powerset.cjs with redirect support.
 */
const fs = require('fs');
const path = require('path');
const { execSync } = require('child_process');
const { parseDatasetArg } = require('./_dataset-paths.cjs');
const { CATEGORY_MAP, RAW_DATA_PATH } = require('./convert-powerset.cjs');

const datasetId = parseDatasetArg();
const datasetFlag = `--dataset ${datasetId}`;

// Find all unique category/powerset pairs that have redirect powers
const affectedPowersets = new Set();

for (const [category, info] of Object.entries(CATEGORY_MAP)) {
  const catPath = path.join(RAW_DATA_PATH, 'powers', category);
  if (!fs.existsSync(catPath)) continue;

  const powersets = fs.readdirSync(catPath).filter(d =>
    fs.statSync(path.join(catPath, d)).isDirectory()
  );

  for (const ps of powersets) {
    const psPath = path.join(catPath, ps);
    const files = fs.readdirSync(psPath).filter(f => f.endsWith('.json') && f !== 'index.json');

    for (const file of files) {
      const json = JSON.parse(fs.readFileSync(path.join(psPath, file), 'utf-8'));
      if (json.redirect && json.redirect.length > 0 && (!json.effects || json.effects.length === 0)) {
        affectedPowersets.add(`${category} ${ps}`);
        break;
      }
    }
  }
}

console.log(`Found ${affectedPowersets.size} powersets with redirect powers. Converting...\n`);

let success = 0;
let failed = 0;

for (const pair of affectedPowersets) {
  const [category, powerset] = pair.split(' ');
  try {
    const output = execSync(
      `node scripts/convert-powerset.cjs ${category} ${powerset} ${datasetFlag}`,
      { cwd: path.join(__dirname, '..'), encoding: 'utf-8', timeout: 30000 }
    );
    const lines = output.split('\n');
    const redirectLines = lines.filter(l => l.includes('[redirect]'));
    const summaryLine = lines.find(l => l.includes('Wrote'));
    if (redirectLines.length > 0) {
      console.log(`${category}/${powerset}:`);
      redirectLines.forEach(l => console.log(l));
      if (summaryLine) console.log(summaryLine);
      console.log('');
    }
    success++;
  } catch (err) {
    console.error(`FAILED: ${category}/${powerset}: ${err.message}`);
    failed++;
  }
}

console.log(`\nDone. ${success} succeeded, ${failed} failed.`);
