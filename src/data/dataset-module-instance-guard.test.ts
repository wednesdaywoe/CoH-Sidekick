import { it, expect } from 'vitest';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative, resolve } from 'node:path';

/**
 * Under vitest a dataset is served from its flattened bundle, not its module graph
 * (see `datasetBundleSwapPlugin` in vite.config.ts). The swap redirects the dataset ROOT
 * only, so a test importing a file INSIDE a dataset folder still gets the real module —
 * which means those two forms are separate object instances.
 *
 * Reading through either is fine; the data is identical and
 * `dataset-bundle-fidelity.test.ts` gates that. WRITING is not. A test that injects a
 * synthetic fixture into a dataset-internal module's export is writing to a copy the
 * resolver never reads, and the assertion that follows grades the unmodified data. That
 * cost one real test (`destiny-decay.test.ts`, a Destiny twin-tier guard) — it threw,
 * loudly, but a write whose absence merely leaves a value at its default would have gone
 * quiet and green.
 *
 * So: inject through `getActiveDataset()`, which is the object the code under test reads,
 * whichever instance provided it. This scan is the enforcement.
 */

const SRC = resolve(__dirname, '..');
const DATASET_INTERNAL = /datasets\/(?:[a-z0-9-]+)\/[^']+$/;

function testFiles(dir: string, out: string[] = []): string[] {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) {
      // The generated dataset folders hold their own colocated tests and no app code.
      if (entry === 'datasets') continue;
      testFiles(full, out);
    } else if (/\.test\.tsx?$/.test(entry)) {
      out.push(full);
    }
  }
  return out;
}

/** Local names bound by a value import from a module inside a dataset folder. */
function datasetInternalBindings(source: string): string[] {
  const names: string[] = [];
  const importRe = /import\s+(type\s+)?(\{[^}]*\}|\*\s+as\s+\w+|\w+)\s+from\s+'([^']+)'/g;
  for (const match of source.matchAll(importRe)) {
    if (match[1]) continue; // `import type` — erased, cannot be mutated
    if (!DATASET_INTERNAL.test(match[3])) continue;
    const clause = match[2];
    if (clause.startsWith('{')) {
      for (const part of clause.slice(1, -1).split(',')) {
        const trimmed = part.trim();
        if (!trimmed || trimmed.startsWith('type ')) continue;
        names.push((trimmed.includes(' as ') ? trimmed.split(' as ')[1] : trimmed).trim());
      }
    } else if (clause.startsWith('*')) {
      names.push(clause.split(/\s+as\s+/)[1].trim());
    } else {
      names.push(clause.trim());
    }
  }
  return names;
}

it('no test mutates a dataset-internal module — inject through getActiveDataset() instead', () => {
  const offences: string[] = [];

  for (const file of testFiles(SRC)) {
    const source = readFileSync(file, 'utf8');
    const bindings = datasetInternalBindings(source);
    if (bindings.length === 0) continue;

    const lines = source.split('\n');
    for (const name of bindings) {
      const mutation = new RegExp(
        `\\b${name}\\s*(?:\\[[^\\]]*\\]|\\.[A-Za-z_$][\\w$]*)+\\s*(?:=[^=]|\\+\\+|--|\\.push\\(|\\.splice\\(|\\.pop\\()` +
          `|\\bdelete\\s+${name}\\b` +
          `|\\bObject\\.assign\\(\\s*${name}\\b`,
      );
      lines.forEach((line, index) => {
        if (mutation.test(line)) {
          offences.push(`${relative(SRC, file)}:${index + 1}  ${line.trim()}`);
        }
      });
    }
  }

  expect(offences, offences.join('\n')).toEqual([]);
});

