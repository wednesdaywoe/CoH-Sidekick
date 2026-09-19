/**
 * The rolling-window arithmetic behind a per-IP rate limit — SECURITY_AUDIT.md F11.
 *
 * `share-build` has carried a window like this inline since before the audit;
 * this is the same shape, extracted so it can be graded, and so the second
 * caller (`backfill-preview`) is not a second hand-rolled copy of it. The
 * window is rolling rather than fixed: a slot frees up when the OLDEST request
 * still inside the window ages out, not on the hour, so a caller who spent
 * their whole allowance in one burst waits a full window and a caller who
 * spread it out waits seconds.
 *
 * Pure on purpose — it takes a count and a timestamp, and the caller does the
 * two queries. That is what lets vitest grade the arithmetic, which is where
 * the off-by-one and the negative-duration live, without a database.
 */

/** What the two queries came back with, plus the clock. */
export interface WindowInput {
  /** How many requests this caller already has inside the window. */
  used: number;
  /** The most the window allows. */
  limit: number;
  /** `created_at` of the oldest request still inside the window, or null when
   *  there is none (or the query failed — treated the same, see `resetAt`). */
  oldest: string | null | undefined;
  /** Now, in epoch milliseconds. */
  now: number;
  /** How wide the window is. */
  windowHours: number;
}

/** What the caller needs to answer with. */
export interface WindowVerdict {
  /** True when this request must be refused. */
  exceeded: boolean;
  /** Slots left AFTER this request is counted. Never negative. */
  remaining: number;
  /** Whole seconds until a slot frees. Never negative, so a clock that has
   *  drifted backwards cannot produce a `Retry-After` in the past. */
  retryAfterSeconds: number;
  /** When that slot frees, as an ISO timestamp. */
  resetAt: string;
}

/** The ISO timestamp a `created_at >= ...` filter should use for this window. */
export function windowStart(now: number, windowHours: number): string {
  return new Date(now - windowHours * 60 * 60 * 1000).toISOString();
}

/**
 * Grade a caller against the window.
 *
 * An absent or unparseable `oldest` falls back to a full window from now. That
 * is deliberately the pessimistic end: the alternative is telling a caller to
 * retry immediately on the strength of a timestamp we could not read, which
 * turns a failed query into an un-metered request.
 */
export function gradeWindow({ used, limit, oldest, now, windowHours }: WindowInput): WindowVerdict {
  const windowMs = windowHours * 60 * 60 * 1000;
  const parsed = typeof oldest === 'string' ? Date.parse(oldest) : NaN;
  const resetMs = (Number.isNaN(parsed) ? now : parsed) + windowMs;
  return {
    exceeded: used >= limit,
    remaining: Math.max(0, limit - (used + 1)),
    retryAfterSeconds: Math.max(0, Math.ceil((resetMs - now) / 1000)),
    resetAt: new Date(resetMs).toISOString(),
  };
}

/**
 * The caller's IP, as the edge runtime reports it.
 *
 * `x-forwarded-for` is a list and the client is the first entry; `cf-connecting-ip`
 * is Cloudflare's single-value header. Both are attacker-spoofable in general,
 * and neither is here: the request reaches the function through Supabase's own
 * proxy, which overwrites `x-forwarded-for` with the socket peer rather than
 * appending to it. `'unknown'` is a real bucket, not a bypass — every caller we
 * cannot place shares one allowance, which is the strict reading.
 *
 * The `||` chain rather than `??` is load-bearing: an empty `x-forwarded-for`
 * trims to `''`, which is not nullish, so `??` would hand back the empty string
 * as an IP and give every such caller their own private allowance.
 */
export function callerIp(headers: Headers): string {
  return headers.get('x-forwarded-for')?.split(',')[0]?.trim()
    || headers.get('cf-connecting-ip')
    || 'unknown';
}
