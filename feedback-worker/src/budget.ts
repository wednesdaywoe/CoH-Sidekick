/**
 * SECURITY_AUDIT.md F12 — the part a rate-limit binding cannot do.
 *
 * The `[[ratelimits]]` binding in `wrangler.toml` bounds a burst and is
 * best-effort while doing it: measured against the deployed worker on
 * 2026-09-23, a configured 3-per-60s admitted 19 of 30 requests in 2.75
 * seconds. Cloudflare's counters are per-location and approximate, which is a
 * reasonable thing for them to be and a fatal thing to cite as a bound. What
 * the row actually needs is a number nobody can exceed, and that needs one
 * place doing the counting.
 *
 * So: one Durable Object, one instance, two counters.
 *
 * - **A global daily budget** is the bound on Resend spend, which is what
 *   "unrate-limited mail relay" names. It is exact because every admitted
 *   submission in the world serialises through this one object.
 * - **A per-address daily cap** is what stops one caller eating that budget.
 *   Without it the global cap is a denial-of-service switch anybody can flip;
 *   with it, flipping it costs `GLOBAL / PER_IP` distinct addresses.
 *
 * **Both are charged at the send, not at the door.** A budget charged before
 * validation can be drained with bodies that 400, which would turn a spend
 * bound into an outage for everyone else — the same probe that measured the
 * burst limiter used exactly those bodies precisely because they cost nothing.
 * `spend()` is therefore called immediately before the Resend `fetch` and
 * nowhere else, so the counter and the mail are the same number.
 */

/** One decision, and everything a caller needs to answer with. */
export interface Verdict {
  /** Was the submission charged? A `false` here means no mail may be sent. */
  ok: boolean;
  /** Which cap refused it, for the message and the log. `null` when admitted. */
  refused: 'per-ip' | 'global' | null;
  /** Seconds until the counters reset, for `Retry-After`. */
  resetIn: number;
}

/** The counters, as they sit in storage. */
export interface Ledger {
  /** UTC date, `YYYY-MM-DD`. A different one means every count below is stale. */
  day: string;
  /** Admitted submissions today, all callers. */
  global: number;
  /** Admitted submissions today, per `CF-Connecting-IP`. */
  perIp: Record<string, number>;
}

export interface Caps {
  global: number;
  perIp: number;
}

/**
 * The defaults, and why these numbers.
 *
 * `global` sits **below the mail provider's own daily allowance** rather than
 * at some round number, which is the only principled place for it: a cap above
 * the provider's turns the bound into a Resend refusal, and a Resend refusal
 * reaches the submitter as a 502 with no explanation and reaches nobody as a
 * log. Resend's free tier is 100 a day; 80 leaves the headroom.
 *
 * `perIp` is generous against what the surface is for. The beta has 530
 * profiles and this is a feedback form; ten a day from one address is somebody
 * having a bad afternoon, not somebody being throttled. It also fixes the cost
 * of draining the global budget at eight distinct addresses, which is low —
 * the burst limiter and the origin check are what make collecting eight
 * addresses cost anything, and neither is load-bearing on its own.
 *
 * Both are overridable from `wrangler.toml`, so moving them is a config edit
 * and a deploy rather than a code change.
 */
export const DEFAULT_CAPS: Caps = { global: 80, perIp: 10 };

/** Seconds from `now` to the next UTC midnight, which is when the day rolls. */
export function untilReset(now: number): number {
  return Math.ceil((Date.UTC(
    new Date(now).getUTCFullYear(),
    new Date(now).getUTCMonth(),
    new Date(now).getUTCDate() + 1,
  ) - now) / 1000);
}

/** The UTC day `now` falls in. */
export function dayOf(now: number): string {
  return new Date(now).toISOString().slice(0, 10);
}

/**
 * The whole decision, as a pure function of the ledger.
 *
 * Separated from storage on purpose: every rule worth grading is in here, and
 * grading it needs no Durable Object runtime, no `miniflare` and no network.
 * What the class below adds is `get`, `put` and the guarantee that no two
 * callers run this at once — which is the one property that cannot be unit
 * tested and is also the only reason a Durable Object is the answer.
 *
 * Returns the ledger to store. On a refusal it is returned unchanged apart
 * from the day roll, because **a refused submission is not charged**: charging
 * it would let a caller past their own cap push the global counter, and the
 * global counter is supposed to count mail.
 */
export function decide(ledger: Ledger | undefined, ip: string, now: number, caps: Caps): {
  verdict: Verdict;
  ledger: Ledger;
} {
  const day = dayOf(now);
  const resetIn = untilReset(now);
  // A ledger from another day is not a smaller ledger, it is a different one.
  const current: Ledger = ledger && ledger.day === day
    ? ledger
    : { day, global: 0, perIp: {} };

  const refuse = (refused: 'per-ip' | 'global'): { verdict: Verdict; ledger: Ledger } =>
    ({ verdict: { ok: false, refused, resetIn }, ledger: current });

  // Per-address first. The global budget is the scarcer resource, so the
  // caller who is over their own cap should not be told about it.
  if ((current.perIp[ip] ?? 0) >= caps.perIp) return refuse('per-ip');
  if (current.global >= caps.global) return refuse('global');

  return {
    verdict: { ok: true, refused: null, resetIn },
    ledger: {
      day,
      global: current.global + 1,
      // Only an ADMITTED submission writes a key here, which is what bounds
      // this map: at most `caps.global` distinct addresses can appear in a
      // day, because the global check above refuses everything after that.
      perIp: { ...current.perIp, [ip]: (current.perIp[ip] ?? 0) + 1 },
    },
  };
}

/** What the worker needs from the budget, so the worker can be graded without one. */
export interface Budget {
  spend(ip: string): Promise<Verdict>;
}

/**
 * Read the caps out of an env, refusing a value that is set and unparseable.
 *
 * Falls back only on ABSENCE. A var set to something that is not a
 * non-negative integer is a misconfiguration, and reading it as "use the
 * default" is how a cap silently stops being the cap somebody intended.
 */
export function capsFrom(env: {
  FEEDBACK_DAILY_BUDGET?: string;
  FEEDBACK_DAILY_PER_IP?: string;
}): Caps {
  const read = (raw: string | undefined, fallback: number): number => {
    if (raw === undefined) return fallback;
    const parsed = Number(raw);
    if (!Number.isInteger(parsed) || parsed < 0) {
      throw new Error(`cap must be a non-negative integer, got ${JSON.stringify(raw)}`);
    }
    return parsed;
  };
  return {
    global: read(env.FEEDBACK_DAILY_BUDGET, DEFAULT_CAPS.global),
    perIp: read(env.FEEDBACK_DAILY_PER_IP, DEFAULT_CAPS.perIp),
  };
}

/**
 * The Durable Object. Thin by design — `decide` holds the rules, this holds
 * the one thing only a Durable Object has: a single instance, so two
 * concurrent submissions cannot both read `global: 79` and both be admitted.
 *
 * **The caps arrive with the request rather than being read from this object's
 * own env, and that was measured rather than chosen.** A Durable Object is
 * constructed once and then lives; a `wrangler deploy` that changes a `[vars]`
 * value does not restart a running instance, so the object goes on answering
 * with the configuration it was born with until it happens to be evicted.
 * Probed against production 2026-09-23: two deploys in a row, and each probe
 * was answered by the PREVIOUS deploy's caps — a cap set to 0 admitted a
 * submission, and a cap set to 10 refused one. Reading them per request, in
 * the worker, means a cap change takes effect on the next request after the
 * deploy rather than at some unpredictable point after it.
 */
export class FeedbackBudget {
  constructor(private readonly ctx: DurableObjectState) {}

  async fetch(request: Request): Promise<Response> {
    const params = new URL(request.url).searchParams;
    const ip = params.get('ip') ?? 'unknown';
    const caps: Caps = {
      global: Number(params.get('global')),
      perIp: Number(params.get('perIp')),
    };
    // `blockConcurrencyWhile` rather than a bare read-modify-write: within one
    // object, `await` yields, and two requests interleaving between the `get`
    // and the `put` is exactly the lost update this object exists to prevent.
    const verdict = await this.ctx.blockConcurrencyWhile(async () => {
      const stored = await this.ctx.storage.get<Ledger>('ledger');
      const now = Date.now();
      const result = decide(stored, ip, now, caps);
      // The day roll deletes rather than overwrites, because yesterday's
      // per-address map is the only thing here that grows.
      if (stored && stored.day !== result.ledger.day) await this.ctx.storage.deleteAll();
      await this.ctx.storage.put('ledger', result.ledger);
      return result.verdict;
    });
    return new Response(JSON.stringify(verdict), {
      headers: { 'Content-Type': 'application/json' },
    });
  }
}
