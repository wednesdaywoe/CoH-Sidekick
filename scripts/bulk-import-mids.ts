/**
 * Bulk Mids .mbd → Supabase shared_builds importer.
 *
 * Three modes:
 *   1. --lookup-discord-username <name>
 *        Query auth.users for matching Discord usernames and print UUIDs.
 *   2. --dir <path> --user-id <uuid> --dry-run
 *        Parse every .mbd, aggregate warnings, produce mapping report. No DB writes.
 *   3. --dir <path> --user-id <uuid> [--public]
 *        Real import: parse + insert rows into shared_builds.
 *
 * Run with:
 *   npx tsx --import ./scripts/env-register.mjs scripts/bulk-import-mids.ts -- <flags>
 * Or via npm:
 *   npm run import-mids -- <flags>
 *
 * Required env vars (see .env.example):
 *   SUPABASE_URL
 *   SUPABASE_SERVICE_ROLE_KEY
 */

import * as fs from 'node:fs';
import * as path from 'node:path';
import { parseArgs } from 'node:util';
import { createClient } from '@supabase/supabase-js';
import { nanoid } from 'nanoid';

import { importMidsBuild } from '@/utils/mids-import';
import { slimBuild } from '@/utils/build-serialization';
import { loadDataset, type DatasetId } from '@/data/dataset';
import type { MidsImportWarning } from '@/utils/mids-import/types';
import type { Build } from '@/types';

const MBD_DATABASE_TO_DATASET: Record<string, DatasetId> = {
  Homecoming: 'homecoming',
  Rebirth: 'rebirth',
};

function detectDatasetFromJson(json: string): DatasetId {
  try {
    const parsed = JSON.parse(json) as { BuiltWith?: { Database?: string } };
    const db = parsed.BuiltWith?.Database;
    return (db && MBD_DATABASE_TO_DATASET[db]) || 'homecoming';
  } catch {
    return 'homecoming';
  }
}

// ============================================================================
// CLI parsing
// ============================================================================

const { values: args } = parseArgs({
  options: {
    dir: { type: 'string' },
    'user-id': { type: 'string' },
    'author-name': { type: 'string', default: '' },
    public: { type: 'boolean', default: false },
    'dry-run': { type: 'boolean', default: false },
    'lookup-discord-username': { type: 'string' },
    'output-dir': { type: 'string', default: './import-output' },
    help: { type: 'boolean', short: 'h', default: false },
  },
  strict: false,
});

function printHelp() {
  console.log(`
Bulk Mids .mbd → Supabase shared_builds importer

Modes:
  --lookup-discord-username <name>     Find UUID by Discord username
  --dir <path> --user-id <uuid> --dry-run    Parse + report, no DB writes
  --dir <path> --user-id <uuid> [--public]   Real import

Options:
  --dir <path>                         Directory containing .mbd files (recursive)
  --user-id <uuid>                     Supabase auth.users.id to attribute builds to
  --author-name <name>                 Display name for the build's original author
                                       (populates the author_name column; default: "")
  --public                             Make imports public (default: private/vault)
  --dry-run                            Parse only, no database writes
  --output-dir <path>                  Where to write state/reports (default: ./import-output)
  --lookup-discord-username <name>     Look up UUID by Discord username
  -h, --help                           Show this help

Environment:
  SUPABASE_URL                         Your Supabase project URL
  SUPABASE_SERVICE_ROLE_KEY            Service role key (bypasses RLS and rate limits)
`);
}

if (args.help) {
  printHelp();
  process.exit(0);
}

// ============================================================================
// Env + Supabase client
// ============================================================================

const SUPABASE_URL = process.env.SUPABASE_URL;
const SUPABASE_SERVICE_ROLE_KEY = process.env.SUPABASE_SERVICE_ROLE_KEY;

function getSupabase() {
  if (!SUPABASE_URL || !SUPABASE_SERVICE_ROLE_KEY) {
    console.error('ERROR: SUPABASE_URL and SUPABASE_SERVICE_ROLE_KEY must be set.');
    console.error('Create a .env file or export them in your shell. See .env.example.');
    process.exit(1);
  }
  return createClient(SUPABASE_URL, SUPABASE_SERVICE_ROLE_KEY, {
    auth: { persistSession: false, autoRefreshToken: false },
  });
}

// ============================================================================
// Mode 1: Discord username lookup
// ============================================================================

async function lookupDiscordUsername(username: string) {
  const supabase = getSupabase();
  console.log(`Searching auth.users for Discord username: "${username}"...\n`);

  // Paginate through users. 4000+ user archives should fit in a few pages.
  const matches: Array<{ id: string; email: string | null; discord: string | null; created: string }> = [];
  let page = 1;
  const perPage = 1000;

  while (true) {
    const { data, error } = await supabase.auth.admin.listUsers({ page, perPage });
    if (error) {
      console.error('Failed to list users:', error.message);
      process.exit(1);
    }
    const users = data.users ?? [];
    if (users.length === 0) break;

    for (const u of users) {
      const meta = (u.user_metadata ?? {}) as Record<string, unknown>;
      const custom = (meta.custom_claims ?? {}) as Record<string, unknown>;
      const discord =
        (custom.discord_username as string | undefined) ??
        (meta.name as string | undefined) ??
        (meta.full_name as string | undefined) ??
        null;

      if (discord && discord.toLowerCase().includes(username.toLowerCase())) {
        matches.push({
          id: u.id,
          email: u.email ?? null,
          discord,
          created: u.created_at,
        });
      }
    }

    if (users.length < perPage) break;
    page += 1;
  }

  if (matches.length === 0) {
    console.log('No matching users found.');
    process.exit(1);
  }

  console.log(`Found ${matches.length} match${matches.length === 1 ? '' : 'es'}:\n`);
  for (const m of matches) {
    console.log(`  UUID:    ${m.id}`);
    console.log(`  Discord: ${m.discord}`);
    console.log(`  Email:   ${m.email ?? '(none)'}`);
    console.log(`  Created: ${m.created}`);
    console.log();
  }
}

// ============================================================================
// Mode 2/3: file walker + import pipeline
// ============================================================================

function walkMbdFiles(dir: string): string[] {
  const out: string[] = [];
  const entries = fs.readdirSync(dir, { withFileTypes: true });
  for (const entry of entries) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      out.push(...walkMbdFiles(full));
    } else if (entry.isFile() && entry.name.toLowerCase().endsWith('.mbd')) {
      out.push(full);
    }
  }
  return out;
}

// ============================================================================
// Warnings aggregator
// ============================================================================

interface AggregatedIssue {
  count: number;
  samples: string[]; // filenames (up to 3)
  messages: Set<string>; // distinct messages seen
}

class WarningAggregator {
  private byTypeAndName = new Map<string, Map<string, AggregatedIssue>>();

  record(filename: string, warnings: MidsImportWarning[]) {
    for (const w of warnings) {
      let byName = this.byTypeAndName.get(w.type);
      if (!byName) {
        byName = new Map();
        this.byTypeAndName.set(w.type, byName);
      }
      const key = w.midsName || '(empty)';
      let issue = byName.get(key);
      if (!issue) {
        issue = { count: 0, samples: [], messages: new Set() };
        byName.set(key, issue);
      }
      issue.count += 1;
      if (issue.samples.length < 3 && !issue.samples.includes(filename)) {
        issue.samples.push(filename);
      }
      issue.messages.add(w.message);
    }
  }

  toMarkdown(totalBuilds: number, imported: number, failed: number, withWarnings: number): string {
    const lines: string[] = [];
    lines.push('# Mids Import Mapping Report');
    lines.push(`Generated ${new Date().toISOString()} from ${totalBuilds} builds.`);
    lines.push('');
    lines.push('## Summary');
    lines.push(`- Total builds: ${totalBuilds}`);
    lines.push(`- Successfully imported: ${imported} (${((imported / Math.max(totalBuilds, 1)) * 100).toFixed(1)}%)`);
    lines.push(`- Failed to parse: ${failed} (${((failed / Math.max(totalBuilds, 1)) * 100).toFixed(1)}%)`);
    lines.push(`- Builds with at least one warning: ${withWarnings}`);
    lines.push('');

    const typeLabels: Record<string, string> = {
      archetype: 'Unmapped Archetypes',
      powerset: 'Powerset Resolution Failures / Fallbacks',
      power: 'Powers Not Found',
      pool: 'Pool Power Resolution Failures',
      epic: 'Epic Pool Resolution Failures',
      enhancement: 'Unmapped Enhancements',
      general: 'General / JSON Errors',
    };

    const order = ['enhancement', 'power', 'powerset', 'pool', 'epic', 'archetype', 'general'];

    for (const type of order) {
      const byName = this.byTypeAndName.get(type);
      if (!byName || byName.size === 0) continue;

      const label = typeLabels[type] ?? type;
      lines.push(`## ${label} (${byName.size} distinct · ${totalOccurrences(byName)} total)`);
      lines.push('');
      lines.push('| Mids name | Builds affected | Example files | Message |');
      lines.push('|---|---|---|---|');

      const sorted = [...byName.entries()].sort((a, b) => b[1].count - a[1].count).slice(0, 25);
      for (const [name, issue] of sorted) {
        const samples = issue.samples.map((f) => `\`${path.basename(f)}\``).join(', ');
        const msg = [...issue.messages][0]?.replace(/\|/g, '\\|') ?? '';
        lines.push(`| \`${escapeMd(name)}\` | ${issue.count} | ${samples} | ${msg} |`);
      }
      lines.push('');
    }

    if (this.byTypeAndName.size === 0) {
      lines.push('_No warnings across any builds._');
    }

    return lines.join('\n');
  }
}

function totalOccurrences(byName: Map<string, AggregatedIssue>): number {
  let total = 0;
  for (const i of byName.values()) total += i.count;
  return total;
}

function escapeMd(s: string): string {
  return s.replace(/\|/g, '\\|').replace(/\n/g, ' ');
}

// ============================================================================
// State file (filename dedup across runs)
// ============================================================================

function loadState(stateFile: string): Set<string> {
  try {
    const raw = fs.readFileSync(stateFile, 'utf-8');
    const parsed = JSON.parse(raw) as { processed?: string[] };
    return new Set(parsed.processed ?? []);
  } catch {
    return new Set();
  }
}

function saveState(stateFile: string, processed: Set<string>) {
  const data = { processed: [...processed].sort(), updatedAt: new Date().toISOString() };
  fs.writeFileSync(stateFile, JSON.stringify(data, null, 2), 'utf-8');
}

// ============================================================================
// Build → DB row
// ============================================================================

interface SharedBuildRow {
  id: string;
  name: string;
  description: string;
  archetype: string;
  archetype_name: string;
  primary_set: string;
  primary_name: string;
  secondary_set: string;
  secondary_name: string;
  level: number;
  author_name: string;
  server: string;
  tags: string[];
  build_json: unknown;
  is_public: boolean;
  user_id: string;
  owner_token_hash: string | null;
}

function buildToRow(
  build: Build,
  userId: string,
  isPublic: boolean,
  authorName: string,
): SharedBuildRow {
  const exportJson = {
    version: 4, // match the app's current export envelope (slimBuild is current-shape)
    build: slimBuild(build),
    meta: {
      exportedAt: new Date().toISOString(),
      ...(authorName ? { authorName } : {}),
    },
  };

  return {
    id: nanoid(10),
    name: build.name || 'Untitled Build',
    description: '',
    archetype: build.archetype.id ?? '',
    archetype_name: build.archetype.name ?? '',
    primary_set: build.primary.id ?? '',
    primary_name: build.primary.name ?? '',
    secondary_set: build.secondary.id ?? '',
    secondary_name: build.secondary.name ?? '',
    level: build.level ?? 50,
    author_name: authorName,
    server: '',
    tags: [],
    build_json: exportJson,
    is_public: isPublic,
    user_id: userId,
    owner_token_hash: null,
  };
}

// ============================================================================
// Main import run
// ============================================================================

async function runImport(opts: {
  dir: string;
  userId: string;
  isPublic: boolean;
  dryRun: boolean;
  outputDir: string;
  authorName: string;
}) {
  const { dir, userId, isPublic, dryRun, outputDir, authorName } = opts;

  if (!fs.existsSync(dir)) {
    console.error(`ERROR: Directory not found: ${dir}`);
    process.exit(1);
  }

  fs.mkdirSync(outputDir, { recursive: true });
  const stateFile = path.join(outputDir, 'import-state.json');
  const failuresFile = path.join(outputDir, 'import-failures.csv');
  const reportFile = path.join(outputDir, 'mapping-report.md');

  const processed = loadState(stateFile);
  const allFiles = walkMbdFiles(dir);
  const todo = allFiles.filter((f) => !processed.has(f));

  console.log(`Found ${allFiles.length} .mbd files in ${dir}`);
  console.log(`  Already processed (skipping): ${allFiles.length - todo.length}`);
  console.log(`  To process this run: ${todo.length}`);
  console.log(`  Mode: ${dryRun ? 'DRY RUN (no DB writes)' : 'LIVE IMPORT'}`);
  console.log(`  Target user_id: ${userId}`);
  console.log(`  Visibility: ${isPublic ? 'PUBLIC' : 'private (vault)'}`);
  console.log(`  Author name: ${authorName ? `"${authorName}"` : '(none — author_name stays blank)'}`);
  console.log();

  const supabase = dryRun ? null : getSupabase();

  if (!dryRun && supabase) {
    // Quick sanity check: does this user_id exist?
    const { data: userCheck, error: userErr } = await supabase.auth.admin.getUserById(userId);
    if (userErr || !userCheck?.user) {
      console.error(`ERROR: user_id ${userId} not found in auth.users`);
      console.error(`  ${userErr?.message ?? '(user missing)'}`);
      process.exit(1);
    }
    console.log(`Verified target user: ${userCheck.user.email ?? '(no email)'}`);
    console.log();
  }

  // Prepare failure CSV header if new
  if (!fs.existsSync(failuresFile)) {
    fs.writeFileSync(failuresFile, 'filename,archetype,error_message\n', 'utf-8');
  }

  const aggregator = new WarningAggregator();
  let imported = 0;
  let failed = 0;
  let buildsWithWarnings = 0;
  const totalBuilds = todo.length;

  // Batch staging
  const BATCH_SIZE = 200;
  const pendingRows: Array<{ row: SharedBuildRow; filename: string }> = [];

  async function flushBatch() {
    if (pendingRows.length === 0 || !supabase) return;
    const rows = pendingRows.map((p) => p.row);
    const { error } = await supabase.from('shared_builds').insert(rows);
    if (error) {
      console.error(`\n!! Batch insert failed (${rows.length} rows): ${error.message}`);
      console.error(`   First file in batch: ${pendingRows[0].filename}`);
      // Don't record these as processed — next run retries.
      pendingRows.length = 0;
      return;
    }
    for (const p of pendingRows) processed.add(p.filename);
    saveState(stateFile, processed);
    pendingRows.length = 0;
  }

  for (let i = 0; i < todo.length; i++) {
    const filepath = todo[i];
    const basename = path.basename(filepath);

    if ((i + 1) % 50 === 0 || i === 0) {
      process.stdout.write(`\r[${i + 1}/${totalBuilds}] ${basename}`.padEnd(120).slice(0, 120));
    }

    let json: string;
    try {
      json = fs.readFileSync(filepath, 'utf-8');
    } catch (err) {
      failed += 1;
      appendFailure(failuresFile, filepath, '', `Could not read file: ${(err as Error).message}`);
      continue;
    }

    await loadDataset(detectDatasetFromJson(json));
    const result = importMidsBuild(json);

    if (result.warnings.length > 0) {
      aggregator.record(filepath, result.warnings);
      buildsWithWarnings += 1;
    }

    if (!result.success || !result.build) {
      failed += 1;
      const msg = result.warnings[0]?.message ?? 'Unknown parse failure';
      const archetype = result.build?.archetype?.id ?? '';
      appendFailure(failuresFile, filepath, archetype, msg);
      continue;
    }

    if (dryRun) {
      imported += 1;
      continue;
    }

    const row = buildToRow(result.build, userId, isPublic, authorName);
    pendingRows.push({ row, filename: filepath });

    if (pendingRows.length >= BATCH_SIZE) {
      await flushBatch();
      imported = processed.size;
    }
  }

  // Flush trailing batch
  if (!dryRun) {
    await flushBatch();
    imported = processed.size - (allFiles.length - todo.length);
    // ^ rows added by THIS run only
  }

  process.stdout.write('\r'.padEnd(120) + '\r');

  // Write mapping report
  fs.writeFileSync(
    reportFile,
    aggregator.toMarkdown(totalBuilds, imported, failed, buildsWithWarnings),
    'utf-8',
  );

  // Final summary
  console.log('=== Import complete ===');
  console.log(`  Builds this run: ${totalBuilds}`);
  console.log(`  Imported:        ${imported}`);
  console.log(`  Failed:          ${failed}`);
  console.log(`  With warnings:   ${buildsWithWarnings}`);
  console.log(`  Mapping report:  ${reportFile}`);
  console.log(`  Failure log:     ${failuresFile}`);
  console.log(`  State file:      ${stateFile}`);
  if (dryRun) {
    console.log('\n  (dry-run — no database writes)');
  }
}

function appendFailure(csvFile: string, filepath: string, archetype: string, message: string) {
  const escape = (s: string) => `"${s.replace(/"/g, '""')}"`;
  fs.appendFileSync(csvFile, `${escape(filepath)},${escape(archetype)},${escape(message)}\n`, 'utf-8');
}

// ============================================================================
// Entry
// ============================================================================

async function main() {
  // Mode 1: lookup
  if (typeof args['lookup-discord-username'] === 'string') {
    await lookupDiscordUsername(args['lookup-discord-username']);
    return;
  }

  // Mode 2/3: import or dry-run
  const dir = args.dir as string | undefined;
  const userId = args['user-id'] as string | undefined;
  const dryRun = Boolean(args['dry-run']);
  const isPublic = Boolean(args.public);
  const outputDir = (args['output-dir'] as string) || './import-output';
  const authorName = ((args['author-name'] as string) || '').trim();

  if (!dir || !userId) {
    console.error('ERROR: --dir and --user-id are required for import mode.');
    console.error('');
    printHelp();
    process.exit(1);
  }

  await runImport({ dir, userId, isPublic, dryRun, outputDir, authorName });
}

main().catch((err) => {
  console.error('\nUnexpected error:', err);
  process.exit(1);
});
