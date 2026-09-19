/**
 * Grades the crash-reporting consent — SECURITY_AUDIT.md F36.
 *
 * The finding is three gaps in one feature (no consent, no notice, no opt-out), and the two
 * directions that can go wrong are opposite. If the opt-out does not hold, the finding is
 * unfixed. If the default fails closed, the crash reporter reports nothing and the row is
 * "closed" by removing the feature — which is not a fix, it is a deletion nobody signed off.
 *
 * `main.tsx`'s wiring is asserted over its source, from this file, so the needles are not in
 * the haystack.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { readFileSync } from 'node:fs';

const STORAGE_KEY = 'coh-planner-crash-reporting';

/** The module reads storage once at import, so each case needs a fresh one. */
async function load(stored: string | null) {
  vi.resetModules();
  const store = new Map<string, string>();
  if (stored !== null) store.set(STORAGE_KEY, stored);
  vi.stubGlobal('localStorage', {
    getItem: (k: string) => store.get(k) ?? null,
    setItem: (k: string, v: string) => void store.set(k, v),
    removeItem: (k: string) => void store.delete(k),
    clear: () => store.clear(),
  });
  const module = await import('./crash-consent');
  return { module, store };
}

beforeEach(() => {
  vi.unstubAllGlobals();
});

describe('crash reporting consent', () => {
  it('reports by default, because a reporter nobody opts into reports nothing', async () => {
    const { module } = await load(null);
    expect(module.isCrashReportingEnabled()).toBe(true);
  });

  it('honours a stored opt-out', async () => {
    // The finding. If this goes the other way, nothing is fixed.
    const { module } = await load('false');
    expect(module.isCrashReportingEnabled()).toBe(false);
  });

  it('honours a stored opt-in', async () => {
    const { module } = await load('true');
    expect(module.isCrashReportingEnabled()).toBe(true);
  });

  it('remembers both answers, not only the opt-out', async () => {
    // "Never asked" and "asked for the default" have to stay distinguishable, or the day the
    // default changes it silently changes for people who already chose.
    const { module, store } = await load(null);
    module.setCrashReporting(false);
    expect(store.get(STORAGE_KEY)).toBe('false');
    module.setCrashReporting(true);
    expect(store.get(STORAGE_KEY)).toBe('true');
  });

  it('takes effect immediately, without a reload', async () => {
    const { module } = await load(null);
    module.setCrashReporting(false);
    expect(module.isCrashReportingEnabled()).toBe(false);
    module.setCrashReporting(true);
    expect(module.isCrashReportingEnabled()).toBe(true);
  });

  it('survives storage being unavailable, in both directions', async () => {
    vi.resetModules();
    vi.stubGlobal('localStorage', {
      getItem: () => { throw new Error('private mode'); },
      setItem: () => { throw new Error('private mode'); },
      removeItem: () => {},
      clear: () => {},
    });
    const module = await import('./crash-consent');
    expect(module.isCrashReportingEnabled()).toBe(true);
    expect(() => module.setCrashReporting(false)).not.toThrow();
    // The choice still holds for this session even though it could not be written down.
    expect(module.isCrashReportingEnabled()).toBe(false);
  });

  it('says what a report contains, in words rather than in a link', async () => {
    const { module } = await load(null);
    const notice = module.CRASH_REPORTING_NOTICE;
    // A notice is only a notice if it names the thing sent and the thing not sent.
    expect(notice).toMatch(/error message/i);
    expect(notice).toMatch(/never/i);
    expect(notice.length).toBeGreaterThan(60);
  });
});

describe('the app actually asks before it sends', () => {
  const main = readFileSync(new URL('../main.tsx', import.meta.url), 'utf8');
  const settings = readFileSync(new URL('../pages/GeneralSettings.tsx', import.meta.url), 'utf8');

  it('Sentry drops the event when reporting is off', () => {
    expect(main).toContain("import { isCrashReportingEnabled } from '@/utils/crash-consent'");
    const before = /beforeSend: \(event\) => (.*)$/m.exec(main);
    expect(before, 'Sentry.init has no beforeSend, so the switch reaches nothing').not.toBeNull();
    // The whole expression, not its ingredients. Checking only that it mentions the predicate
    // and `null` passes an INVERTED ternary — which sends exactly the events the user asked
    // not to send, and drops the ones they agreed to. A mutation walked through the weaker
    // version of this assertion.
    expect(before![1].replace(/\s+/g, ' ').trim()).toBe(
      '(isCrashReportingEnabled() ? event : null),',
    );
  });

  it('is checked per event rather than only around init', () => {
    // Guarding `Sentry.init` alone would make the switch need a reload, which people
    // reasonably read as it not having worked.
    const init = main.indexOf('Sentry.init(');
    const check = main.indexOf('isCrashReportingEnabled()', init);
    expect(check).toBeGreaterThan(init);
  });

  it('there is a switch and a notice on a page a person can reach', () => {
    expect(settings).toContain('setCrashReporting');
    expect(settings).toContain('CRASH_REPORTING_NOTICE');
    expect(settings).toContain('Send crash reports');
    expect(settings).toMatch(/aria-checked=\{crashReports\}/);
    // And the switch is connected to something. Reading the state and drawing it is not the
    // same as being able to change it, and a mutation that emptied the handler passed until
    // this line existed.
    expect(settings).toContain('onClick={toggleCrashReports}');
    expect(settings).toMatch(/setCrashReporting\(!on\)/);
  });
});
