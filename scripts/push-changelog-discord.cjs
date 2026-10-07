#!/usr/bin/env node
/**
 * push-changelog-discord.cjs — post new changelog entries to Discord. Run by hand, never by a hook.
 *
 * Reads crates/app/changelog.json (the same file the app's Changelog and Welcome windows read),
 * compares each entry's `id` against the committed record in .changelog-posted.json, and posts
 * only the ones not yet posted, as one embed per date. The record is written only after Discord
 * accepts the post, and it travels in git, so a second machine does not repost.
 *
 * Dedup is on the id, not the text: rewording an entry does not repost it, changing its id does.
 *
 *   npm run changelog:push                     # post new entries, update the record
 *   npm run changelog:push -- --dry-run        # show what would be posted; post and write nothing
 *   npm run changelog:push -- --init           # mark every current entry posted WITHOUT posting
 *   npm run changelog:push -- --force-backlog  # post every entry, even ones already posted
 *
 * The webhook URL is read from DISCORD_CHANGELOG_WEBHOOK_URL (put it in .env, which is gitignored).
 * It is the beta's channel (decision 2026-10-01, user-chosen), so every post names 1.0 in its
 * title and links to the 1.0 site: a reader must be able to tell which app an entry is about.
 *
 * Ported from the beta's scripts/push-changelog-discord.ts. The beta ran it after every commit and
 * so exited 0 on any failure to keep git quiet; this one is only ever run on purpose, so every
 * failure exits 1 and says what went wrong.
 */
'use strict';

const fs = require('node:fs');
const path = require('node:path');

const ROOT = path.join(__dirname, '..');
const CHANGELOG_PATH = path.join(ROOT, 'crates/app/changelog.json');
const STATE_PATH = path.join(ROOT, '.changelog-posted.json');

// The beta's record format, kept so the two scripts agree on what a record looks like.
const STATE_VERSION = 2;

const KNOWN_FLAGS = ['--dry-run', '--init', '--force-backlog'];

// An unknown flag is an error, never ignored: `--dry run` must not read as "not a dry run" and
// post for real.
const argv = process.argv.slice(2);
const unknown = argv.filter((a) => !KNOWN_FLAGS.includes(a));
if (unknown.length > 0) {
  console.error(
    `[changelog] Unrecognized argument(s): ${unknown.join(' ')}\n` +
      `            Known flags: ${KNOWN_FLAGS.join(', ')}\n` +
      `            Refusing to run — did you mean --dry-run?`,
  );
  process.exit(1);
}
const args = new Set(argv);
const DRY_RUN = args.has('--dry-run');
const INIT = args.has('--init');
const FORCE_BACKLOG = args.has('--force-backlog');

const TYPE_META = {
  feat: { emoji: '✨', label: 'New', color: 0x3ba55d },
  fix: { emoji: '🐛', label: 'Fix', color: 0xed4245 },
  update: { emoji: '🔧', label: 'Update', color: 0x5865f2 },
  'known-issue': { emoji: '⚠️', label: 'Known issue', color: 0xfaa61a },
};

// Every title says which app it is about, since the beta posts to the same channel.
const PRODUCT = 'Sidekick 1.0';
const SITE = 'https://next.coh-sidekick.com';

// A date with several types takes the colour of the first type in this order.
const COLOR_PRIORITY = ['feat', 'fix', 'update', 'known-issue'];

/** Flatten the groups, refusing a missing or repeated id: either would post twice or never. */
function flattenEntries() {
  const groups = JSON.parse(fs.readFileSync(CHANGELOG_PATH, 'utf8'));
  const items = [];
  const seen = new Map();
  for (const group of groups) {
    for (const item of group.items) {
      const id = typeof item.id === 'string' ? item.id.trim() : '';
      if (!id) {
        throw new Error(`An entry on ${group.date} has no "id": ${JSON.stringify(String(item.message).slice(0, 60))}`);
      }
      if (seen.has(id)) {
        throw new Error(`Duplicate id "${id}" (${seen.get(id)} and ${group.date}). Ids must be unique.`);
      }
      if (!TYPE_META[item.type]) {
        throw new Error(`Entry "${id}" has type "${item.type}"; expected one of ${Object.keys(TYPE_META).join(', ')}.`);
      }
      seen.set(id, group.date);
      items.push({ date: group.date, type: item.type, message: item.message, id });
    }
  }
  return items;
}

function loadState() {
  if (!fs.existsSync(STATE_PATH)) {
    // Missing is not "nothing posted": that would repost everything. The file ships in git, so
    // its absence means something is off, and the two ways forward are both deliberate.
    throw new Error(
      `${STATE_PATH} is missing. Run with --init to mark every current entry as posted, ` +
        `or --force-backlog to post all of them.`,
    );
  }
  let state;
  try {
    state = JSON.parse(fs.readFileSync(STATE_PATH, 'utf8'));
  } catch (err) {
    throw new Error(`${STATE_PATH} is not valid JSON (${err.message}). Refusing to run.`, { cause: err });
  }
  if (!state || !Array.isArray(state.posted)) {
    throw new Error(`${STATE_PATH} has no "posted" array. Refusing to run.`);
  }
  if (state.version !== STATE_VERSION) {
    throw new Error(`${STATE_PATH} is version ${state.version}; this script expects ${STATE_VERSION}.`);
  }
  return state;
}

function writeState(state) {
  fs.writeFileSync(STATE_PATH, JSON.stringify(state, null, 2) + '\n', 'utf8');
}

function stateEntry(item) {
  return { id: item.id, date: item.date, preview: item.message.slice(0, 80) };
}

// Discord limits: at most 10 embeds per message, and 4096 characters per embed description.
const MAX_EMBEDS_PER_MESSAGE = 10;
const MAX_DESC = 4000;

function pickColor(types) {
  for (const t of COLOR_PRIORITY) if (types.includes(t)) return TYPE_META[t].color;
  return 0x5865f2;
}

/** One embed per date, split into more if the text runs past Discord's limit. */
function embedsForGroup(date, items) {
  const color = pickColor(items.map((i) => i.type));
  const lines = items.map((i) => `${TYPE_META[i.type].emoji} **${TYPE_META[i.type].label}** — ${i.message}`);

  const chunks = [];
  let current = '';
  for (const line of lines) {
    const safeLine = line.length > MAX_DESC ? line.slice(0, MAX_DESC - 1) + '…' : line;
    if (current && current.length + safeLine.length + 2 > MAX_DESC) {
      chunks.push(current);
      current = safeLine;
    } else {
      current = current ? `${current}\n\n${safeLine}` : safeLine;
    }
  }
  if (current) chunks.push(current);

  return chunks.map((description, idx) => ({
    title: idx === 0 ? `📋 ${PRODUCT} — What's New — ${date}` : `📋 ${PRODUCT} — What's New — ${date} (cont.)`,
    url: SITE,
    description,
    color,
  }));
}

async function postBatch(webhook, embeds) {
  const res = await fetch(webhook, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      username: 'Sidekick Dispatch',
      avatar_url: 'https://coh-sidekick.com/img/favicon-512x512.png',
      embeds,
    }),
  });
  if (!res.ok) {
    const body = await res.text().catch(() => '');
    throw new Error(`Discord answered ${res.status} ${res.statusText}: ${body.slice(0, 300)}`);
  }
}

async function main() {
  const allItems = flattenEntries();

  if (INIT) {
    if (DRY_RUN) {
      console.log(`[changelog] --init --dry-run: would mark ${allItems.length} entries as posted.`);
      return;
    }
    writeState({ version: STATE_VERSION, posted: allItems.map(stateEntry) });
    console.log(`[changelog] Marked ${allItems.length} entries as already posted. Nothing sent.`);
    return;
  }

  const state = fs.existsSync(STATE_PATH) || !FORCE_BACKLOG ? loadState() : { version: STATE_VERSION, posted: [] };
  const postedIds = new Set(state.posted.map((e) => e.id));
  const newItems = FORCE_BACKLOG ? allItems : allItems.filter((i) => !postedIds.has(i.id));

  if (newItems.length === 0) {
    console.log('[changelog] No new entries to post.');
    return;
  }

  // Grouped by date, in the order the file lists them.
  const byDate = new Map();
  for (const item of newItems) {
    const bucket = byDate.get(item.date) ?? [];
    bucket.push(item);
    byDate.set(item.date, bucket);
  }
  const embeds = [];
  for (const [date, items] of byDate) embeds.push(...embedsForGroup(date, items));

  console.log(`[changelog] ${newItems.length} new entr${newItems.length === 1 ? 'y' : 'ies'} across ${byDate.size} date(s).`);

  if (DRY_RUN) {
    for (const [date, items] of byDate) {
      console.log(`\n── ${date} ──`);
      for (const i of items) console.log(`  ${TYPE_META[i.type].emoji} [${i.type}] ${i.message}`);
    }
    console.log('\n[changelog] --dry-run: nothing posted, record unchanged.');
    return;
  }

  const webhook = process.env.DISCORD_CHANGELOG_WEBHOOK_URL;
  if (!webhook) {
    throw new Error('DISCORD_CHANGELOG_WEBHOOK_URL is not set. Add it to .env. Nothing posted.');
  }

  for (let i = 0; i < embeds.length; i += MAX_EMBEDS_PER_MESSAGE) {
    await postBatch(webhook, embeds.slice(i, i + MAX_EMBEDS_PER_MESSAGE));
  }

  // Keyed by id, so --force-backlog refreshes records in place instead of adding duplicates.
  const byId = new Map(state.posted.map((e) => [e.id, e]));
  for (const item of newItems) byId.set(item.id, stateEntry(item));
  writeState({ version: STATE_VERSION, posted: [...byId.values()] });
  console.log(`[changelog] Posted ${newItems.length} entr${newItems.length === 1 ? 'y' : 'ies'} to Discord and updated ${path.basename(STATE_PATH)}.`);
}

main().catch((err) => {
  console.error(`[changelog] ${err.message}`);
  process.exit(1);
});
