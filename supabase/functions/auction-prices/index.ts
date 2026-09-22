/**
 * Supabase Edge Function: auction-prices
 *
 * Returns cached price data (avg/min/max) for a list of HC auction identifiers.
 * Stale or missing entries are fetched from hcvault.cityofheroes.dev with the
 * server-held HC_AUCTION_API_KEY, then cached for CACHE_TTL_HOURS.
 *
 * Request body:  { identifiers: string[] }
 * Response:      { prices: Record<string, PriceRow | null> }
 *
 * Deploy with: supabase functions deploy auction-prices
 * Set secret with: supabase secrets set HC_AUCTION_API_KEY=<key>
 */

import { createClient } from 'https://esm.sh/@supabase/supabase-js@2';
import { callerIp, gradeWindow, windowStart } from '../_shared/rate-window.ts';

const CACHE_TTL_HOURS = 1;
const HISTORY_LIMIT = 20;
const MAX_IDENTIFIERS_PER_REQUEST = 100;
const HC_BASE = 'https://hcvault.cityofheroes.dev/api/v1';

// Throttle to stay under HC API rate limits. Batches of CONCURRENCY run in
// parallel, with BATCH_DELAY_MS between batches. On 429, we back off and retry
// once before giving up on that identifier.
const CONCURRENCY = 4;
const BATCH_DELAY_MS = 250;
const RETRY_BACKOFF_MS = 1500;

// Per-IP allowance on the UPSTREAM work - SECURITY_AUDIT.md F38. This function
// is `verify_jwt = false` and spends a server-held third-party key, so without
// this it is an open proxy onto somebody else's API quota. Shares the
// `rate_limits` table and the pg_cron sweep `share-build` and
// `backfill-preview` already use, under its own `action` so the three
// allowances never take from each other.
//
// Generous against a real page: one build asks for up to
// MAX_IDENTIFIERS_PER_REQUEST at a time, the answers cache for
// CACHE_TTL_HOURS, and a request that needs no upstream fetch spends nothing
// at all (see below) - so browsing costs a slot only when it asks something
// genuinely new.
const AUCTION_RATE_LIMIT = 40;
const RATE_WINDOW_HOURS = 1;
const RATE_LIMIT_ACTION = 'auction';

const corsHeaders = {
  'Access-Control-Allow-Origin': '*',
  'Access-Control-Allow-Headers': 'authorization, x-client-info, apikey, content-type',
};

interface PriceRow {
  raw_identifier: string;
  avg_price: number | null;
  min_price: number | null;
  max_price: number | null;
  sample_count: number | null;
  last_sale_at: string | null;
  fetched_at: string;
  not_found: boolean;
}

interface HcHistoryResponse {
  ok: boolean;
  identifier?: string;
  summary?: {
    total_rows: number;
    min_price: number;
    max_price: number;
    avg_price: string | number;
    latest_sale_at: string;
  };
}

function isFresh(fetched_at: string): boolean {
  const ageMs = Date.now() - new Date(fetched_at).getTime();
  return ageMs < CACHE_TTL_HOURS * 60 * 60 * 1000;
}

async function fetchPriceOnce(identifier: string, apiKey: string): Promise<PriceRow | 'retry'> {
  const url = `${HC_BASE}/auction/history/${encodeURIComponent(identifier)}?limit=${HISTORY_LIMIT}`;
  const now = new Date().toISOString();

  const res = await fetch(url, { headers: { Authorization: `Bearer ${apiKey}` } });

  if (res.status === 429) return 'retry';
  if (res.status === 404) {
    return { raw_identifier: identifier, avg_price: null, min_price: null, max_price: null,
             sample_count: 0, last_sale_at: null, fetched_at: now, not_found: true };
  }
  if (!res.ok) throw new Error(`HC API ${res.status}`);

  const json: HcHistoryResponse = await res.json();
  if (!json.ok || !json.summary || json.summary.total_rows === 0) {
    return { raw_identifier: identifier, avg_price: null, min_price: null, max_price: null,
             sample_count: 0, last_sale_at: null, fetched_at: now, not_found: true };
  }

  return {
    raw_identifier: identifier,
    avg_price: Math.round(Number(json.summary.avg_price)),
    min_price: json.summary.min_price,
    max_price: json.summary.max_price,
    sample_count: json.summary.total_rows,
    last_sale_at: json.summary.latest_sale_at,
    fetched_at: now,
    not_found: false,
  };
}

async function fetchPrice(identifier: string, apiKey: string): Promise<PriceRow> {
  try {
    const first = await fetchPriceOnce(identifier, apiKey);
    if (first !== 'retry') return first;

    // Got 429 — back off and try once more
    await new Promise(r => setTimeout(r, RETRY_BACKOFF_MS));
    const second = await fetchPriceOnce(identifier, apiKey);
    if (second === 'retry') throw new Error('HC API 429 (after retry)');
    return second;
  } catch (err) {
    console.error(`fetchPrice(${identifier})`, err);
    throw err;
  }
}

Deno.serve(async (req: Request) => {
  if (req.method === 'OPTIONS') return new Response('ok', { headers: corsHeaders });

  try {
    const body = await req.json();
    const identifiers: unknown = body?.identifiers;

    if (!Array.isArray(identifiers) || identifiers.some(i => typeof i !== 'string')) {
      return new Response(JSON.stringify({ error: 'identifiers must be string[]' }),
        { status: 400, headers: { ...corsHeaders, 'Content-Type': 'application/json' } });
    }

    const unique = [...new Set(identifiers as string[])].slice(0, MAX_IDENTIFIERS_PER_REQUEST);
    if (unique.length === 0) {
      return new Response(JSON.stringify({ prices: {} }),
        { headers: { ...corsHeaders, 'Content-Type': 'application/json' } });
    }

    const supabaseUrl = Deno.env.get('SUPABASE_URL')!;
    const serviceKey = Deno.env.get('SUPABASE_SERVICE_ROLE_KEY')!;
    const apiKey = Deno.env.get('HC_AUCTION_API_KEY');
    if (!apiKey) {
      return new Response(JSON.stringify({ error: 'HC_AUCTION_API_KEY not configured' }),
        { status: 500, headers: { ...corsHeaders, 'Content-Type': 'application/json' } });
    }

    const supabase = createClient(supabaseUrl, serviceKey);

    // Fetch existing cached rows
    const { data: cached } = await supabase
      .from('auction_prices')
      .select('*')
      .in('raw_identifier', unique);

    const cacheMap = new Map<string, PriceRow>();
    for (const row of (cached ?? []) as PriceRow[]) cacheMap.set(row.raw_identifier, row);

    const stale = unique.filter(id => {
      const row = cacheMap.get(id);
      return !row || !isFresh(row.fetched_at);
    });

    // ---- Metered HERE, after the cache read (F38) ----
    // A request every one of whose identifiers is already fresh costs no
    // upstream call and no API-key spend, so it spends no allowance either -
    // the same reasoning `backfill-preview` meters on. What is bounded is the
    // thing that actually costs, which is the fetch below.
    if (stale.length > 0) {
      const ip = callerIp(req.headers);
      const since = windowStart(Date.now(), RATE_WINDOW_HOURS);

      const { count } = await supabase
        .from('rate_limits')
        .select('*', { count: 'exact', head: true })
        .eq('ip', ip)
        .eq('action', RATE_LIMIT_ACTION)
        .gte('created_at', since);

      const used = count ?? 0;
      if (used >= AUCTION_RATE_LIMIT) {
        const { data: oldest } = await supabase
          .from('rate_limits')
          .select('created_at')
          .eq('ip', ip)
          .eq('action', RATE_LIMIT_ACTION)
          .gte('created_at', since)
          .order('created_at', { ascending: true })
          .limit(1)
          .maybeSingle();

        const verdict = gradeWindow({
          used,
          limit: AUCTION_RATE_LIMIT,
          oldest: oldest?.created_at as string | null | undefined,
          now: Date.now(),
          windowHours: RATE_WINDOW_HOURS,
        });

        return new Response(
          JSON.stringify({
            error: 'Too many price lookups from this address. Please try again later.',
            code: 'rate_limited',
            action: RATE_LIMIT_ACTION,
            limit: AUCTION_RATE_LIMIT,
            remaining: 0,
            retryAfterSeconds: verdict.retryAfterSeconds,
            resetAt: verdict.resetAt,
          }),
          {
            status: 429,
            headers: {
              ...corsHeaders,
              'Content-Type': 'application/json',
              'Retry-After': String(verdict.retryAfterSeconds),
            },
          },
        );
      }

      // Spent before the fetch, not after: a slot recorded only on success is
      // not a limit, because the way to exceed it would be to fail.
      await supabase.from('rate_limits').insert({ ip, action: RATE_LIMIT_ACTION });
    }

    // Fetch stale entries in throttled batches so we don't trip HC's rate limit.
    const newRows: PriceRow[] = [];
    for (let i = 0; i < stale.length; i += CONCURRENCY) {
      const batch = stale.slice(i, i + CONCURRENCY);
      const fetched = await Promise.allSettled(batch.map(id => fetchPrice(id, apiKey)));
      for (let j = 0; j < fetched.length; j++) {
        const result = fetched[j];
        if (result.status === 'fulfilled') {
          newRows.push(result.value);
          cacheMap.set(batch[j], result.value);
        }
        // on rejection: leave any existing (stale) cached row in place; next request will retry
      }
      if (i + CONCURRENCY < stale.length) {
        await new Promise(r => setTimeout(r, BATCH_DELAY_MS));
      }
    }

    if (newRows.length > 0) {
      const { error: upsertErr } = await supabase
        .from('auction_prices')
        .upsert(newRows, { onConflict: 'raw_identifier' });
      if (upsertErr) console.error('upsert error', upsertErr);
    }

    const prices: Record<string, PriceRow | null> = {};
    for (const id of unique) prices[id] = cacheMap.get(id) ?? null;

    return new Response(JSON.stringify({ prices }),
      { headers: { ...corsHeaders, 'Content-Type': 'application/json' } });
  } catch (err) {
    // Logged, not returned (F38). `String(err)` handed the caller whatever an
    // unexpected throw carried - upstream URLs and response text among them -
    // from a function whose whole job is to hold a key the caller must not see.
    console.error('auction-prices error', err);
    return new Response(JSON.stringify({ error: 'Price lookup failed' }),
      { status: 500, headers: { ...corsHeaders, 'Content-Type': 'application/json' } });
  }
});
