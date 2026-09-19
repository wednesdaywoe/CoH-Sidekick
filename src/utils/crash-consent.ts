/**
 * Whether this browser sends crash reports — SECURITY_AUDIT.md F36.
 *
 * The finding is three gaps in one feature: no consent, no notice, no opt-out. Sentry was
 * initialised on every production load and there was no way for anyone to know it was
 * happening or to stop it. The only thing the app said about it was the error screen's
 * "The issue has been reported", which tells you after the fact and offers no choice.
 *
 * **What a report actually contains**, because a notice that cannot say that is not a
 * notice: the exception type and message, the source location, the release and whether it
 * is a dev or production build. No account id, no session token, no build data, and
 * `sendDefaultPii` is left at its default of `false`. Sentry's own server sees the
 * request's IP, as any server does.
 *
 * **The default is on, and that is a choice rather than an oversight.** A crash reporter
 * nobody opts into reports nothing, and the crashes worth fixing are the ones on machines
 * whose owners never visit a settings page. The opt-out is one click, it takes effect
 * without a reload, and it is stated in plain words next to the switch. Flipping this to
 * opt-in is one line — `DEFAULT_ENABLED` — if that trade is judged differently later.
 */

const STORAGE_KEY = 'coh-planner-crash-reporting';

/** Sent unless the user has said otherwise. See the note above. */
const DEFAULT_ENABLED = true;

/** Read once at module load, then kept here so a check costs nothing on a crash path. */
let enabled = DEFAULT_ENABLED;
try {
  const stored = localStorage.getItem(STORAGE_KEY);
  if (stored !== null) enabled = stored === 'true';
} catch {
  /* private mode, or storage disabled — the default stands */
}

/** Whether a crash report may leave this browser right now. */
export function isCrashReportingEnabled(): boolean {
  return enabled;
}

/**
 * Turn reporting on or off, and remember it.
 *
 * Both values are written rather than only the opt-out, so "never asked" and "asked for the
 * default" stay distinguishable — which matters the day the default changes.
 */
export function setCrashReporting(on: boolean): void {
  enabled = on;
  try {
    localStorage.setItem(STORAGE_KEY, String(on));
  } catch {
    /* the in-memory value still holds for this session */
  }
}

/** What the notice beside the switch says, kept here so the words and the rule cannot drift. */
export const CRASH_REPORTING_NOTICE =
  'Sends the error message, where in the code it happened, and the app version when Sidekick ' +
  'crashes. Never your builds, your account, or anything you typed.';
