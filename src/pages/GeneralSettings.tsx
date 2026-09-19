/**
 * GeneralSettings — index sub-page of /settings.
 *
 * Account sign-in/out, build claiming, and developer toggles. The shared
 * heading and tab navigation are owned by SettingsLayout.
 */

import { useState, useCallback } from 'react';
import { useAuthStore, useUIStore } from '@/stores';
import { COLOR_THEMES, COLOR_THEME_ORDER } from '@/data/core/themes';
import { supabase } from '@/lib/supabase';
import { Button } from '@/components/ui/Button';
import { getOwnedBuildIds, claimBuilds } from '@/services/sharedBuilds';
import { isCalcDebugEnabled, enableCalcDebug, disableCalcDebug } from '@/utils/calc-debug';
import { isCrashReportingEnabled, setCrashReporting, CRASH_REPORTING_NOTICE } from '@/utils/crash-consent';

export function GeneralSettings() {
  const user = useAuthStore((s) => s.user);
  const loading = useAuthStore((s) => s.loading);
  const login = useAuthStore((s) => s.login);
  const logout = useAuthStore((s) => s.logout);

  const colorTheme = useUIStore((s) => s.colorTheme);
  const setColorTheme = useUIStore((s) => s.setColorTheme);
  const colorMode = useUIStore((s) => s.colorMode);
  const setColorMode = useUIStore((s) => s.setColorMode);

  const [claimLoading, setClaimLoading] = useState(false);
  const [claimResult, setClaimResult] = useState<{ claimed: number; failed: number } | null>(null);
  const [claimError, setClaimError] = useState<string | null>(null);

  const tokenOwnedIds = getOwnedBuildIds();

  const [crashReports, setCrashReports] = useState(isCrashReportingEnabled);
  const toggleCrashReports = useCallback(() => {
    setCrashReports((on) => {
      setCrashReporting(!on);
      return !on;
    });
  }, []);

  const [calcDebug, setCalcDebug] = useState(isCalcDebugEnabled);
  const toggleCalcDebug = useCallback(() => {
    if (calcDebug) {
      disableCalcDebug();
      setCalcDebug(false);
    } else {
      enableCalcDebug();
      setCalcDebug(true);
    }
  }, [calcDebug]);

  const handleClaim = async () => {
    setClaimLoading(true);
    setClaimError(null);
    setClaimResult(null);
    try {
      const result = await claimBuilds();
      setClaimResult({ claimed: result.claimed.length, failed: result.failed.length });
    } catch (e) {
      setClaimError(e instanceof Error ? e.message : 'Failed to claim builds');
    } finally {
      setClaimLoading(false);
    }
  };

  if (loading) {
    return (
      <div className="text-center py-12">
        <p className="text-gray-400">Loading...</p>
      </div>
    );
  }

  return (
    <>
      {/* Account section */}
      <div className="bg-gray-800 border border-gray-700 rounded-lg p-5 mb-6">
        <h2 className="text-sm font-semibold text-gray-300 mb-4">Account</h2>

        {!supabase ? (
          <p className="text-sm text-gray-500">Sharing features are not configured.</p>
        ) : !user ? (
          <div className="space-y-4">
            <p className="text-sm text-gray-400">
              Sign in to manage your shared builds from any device or browser.
              This is optional — you can still share builds anonymously using owner tokens.
            </p>
            <Button variant="primary" onClick={() => login('discord')}>
              Sign in with Discord
            </Button>
          </div>
        ) : (
          <div className="space-y-4">
            {/* User info */}
            <div className="flex items-center gap-3">
              {user.user_metadata?.avatar_url && (
                <img
                  src={user.user_metadata.avatar_url}
                  alt=""
                  className="w-10 h-10 rounded-full"
                />
              )}
              <div>
                <p className="text-white font-medium">
                  {user.user_metadata?.full_name || user.user_metadata?.name || 'User'}
                </p>
                <p className="text-xs text-gray-500">
                  Signed in via Discord
                </p>
              </div>
            </div>

            <Button variant="secondary" size="sm" onClick={() => logout()}>
              Log Out
            </Button>
          </div>
        )}
      </div>

      {/* Appearance / color theme */}
      <div className="bg-gray-800 border border-gray-700 rounded-lg p-5 mb-6">
        <h2 className="text-sm font-semibold text-gray-300 mb-1">Appearance</h2>
        <p className="text-sm text-gray-400 mb-4">
          Choose a color theme and mode. Changes apply instantly and are saved on this device.
        </p>

        {/* Light/dark mode — orthogonal to the theme; flips each theme's ramp. */}
        <div className="flex items-center justify-between gap-4 mb-4">
          <div className="min-w-0">
            <span className="text-sm font-medium text-gray-100">Mode</span>
            <p className="text-xs text-gray-400 leading-snug">
              Light or dark canvas. Each theme keeps its colors either way.
            </p>
          </div>
          <div
            role="radiogroup"
            aria-label="Color mode"
            className="shrink-0 flex rounded-lg border border-gray-700 p-0.5 bg-gray-900/40"
          >
            {(['dark', 'light'] as const).map((mode) => {
              const active = colorMode === mode;
              return (
                <button
                  key={mode}
                  type="button"
                  role="radio"
                  aria-checked={active}
                  onClick={() => setColorMode(mode)}
                  className={`px-3 py-1 text-xs font-medium rounded-md capitalize transition-colors ${
                    active
                      ? 'bg-[var(--color-primary)] text-on-primary'
                      : 'text-gray-400 hover:text-gray-200'
                  }`}
                >
                  {mode}
                </button>
              );
            })}
          </div>
        </div>

        <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
          {COLOR_THEME_ORDER.map((id) => {
            const theme = COLOR_THEMES[id];
            const selected = colorTheme === id;
            const chips = [
              theme.swatch.base,
              theme.swatch.surface,
              theme.swatch.panel,
              theme.swatch.primary,
              theme.swatch.accent,
              theme.swatch.highlight,
            ];
            return (
              <button
                key={id}
                type="button"
                onClick={() => setColorTheme(id)}
                aria-pressed={selected}
                className={`text-left rounded-lg border p-3 transition-colors ${
                  selected
                    ? 'border-[var(--color-selected)] ring-2 ring-[var(--color-selected)]/40 bg-gray-700/40'
                    : 'border-gray-700 hover:border-gray-600 hover:bg-gray-700/30'
                }`}
              >
                <div className="flex items-center justify-between mb-2">
                  <div className="flex items-baseline gap-2 min-w-0">
                    <span className="text-sm font-medium text-gray-100">{theme.label}</span>
                    <span className="text-xs text-gray-500 truncate">{theme.tagline}</span>
                  </div>
                  {selected && (
                    <span className="shrink-0 text-[10px] font-semibold uppercase tracking-wide text-link">
                      Active
                    </span>
                  )}
                </div>
                <div className="flex gap-0.5 mb-2 rounded overflow-hidden ring-1 ring-black/30">
                  {chips.map((c, i) => (
                    <div key={i} className="h-6 flex-1" style={{ backgroundColor: c }} />
                  ))}
                </div>
                <p className="text-xs text-gray-400 leading-snug">{theme.description}</p>
              </button>
            );
          })}
        </div>
      </div>

      {/* Claim builds section (only when logged in and has local tokens) */}
      {user && tokenOwnedIds.length > 0 && (
        <div className="bg-gray-800 border border-gray-700 rounded-lg p-5 mb-6">
          <h2 className="text-sm font-semibold text-gray-300 mb-3">Link Builds to Account</h2>
          <p className="text-sm text-gray-400 mb-4">
            You have {tokenOwnedIds.length} build{tokenOwnedIds.length !== 1 ? 's' : ''} saved
            in this browser via owner tokens. Link them to your Discord account so you can manage
            them from any device.
          </p>

          <Button
            variant="primary"
            size="sm"
            onClick={handleClaim}
            isLoading={claimLoading}
          >
            Link All Builds to Account
          </Button>

          {claimResult && (
            <p className="text-sm text-green-400 mt-3">
              {claimResult.claimed} build{claimResult.claimed !== 1 ? 's' : ''} linked to your account.
              {claimResult.failed > 0 && (
                <span className="text-yellow-400">
                  {' '}{claimResult.failed} failed (token may be invalid).
                </span>
              )}
            </p>
          )}
          {claimError && (
            <p className="text-sm text-red-400 mt-3">{claimError}</p>
          )}
        </div>
      )}

      {/* Debug section */}
      <div className="bg-gray-800 border border-gray-700 rounded-lg p-5 mb-6">
        <h2 className="text-sm font-semibold text-gray-300 mb-3">Privacy</h2>

        {/* F36: crash reporting ran on every production load with no consent, no notice and
            no way to stop it. The notice is the point as much as the switch — see
            utils/crash-consent.ts for what a report actually contains. */}
        <label className="flex items-center gap-3 cursor-pointer mb-6">
          <button
            type="button"
            role="switch"
            aria-checked={crashReports}
            aria-label="Send crash reports"
            onClick={toggleCrashReports}
            className={`relative inline-flex h-5 w-9 shrink-0 items-center rounded-full transition-colors ${
              crashReports ? 'bg-[var(--color-primary)]' : 'bg-gray-600'
            }`}
          >
            <span
              className={`inline-block h-3.5 w-3.5 rounded-full bg-white transition-transform ${
                crashReports ? 'translate-x-4.5' : 'translate-x-0.5'
              }`}
            />
          </button>
          <div>
            <p className="text-sm text-white">Send crash reports</p>
            <p className="text-xs text-gray-500">{CRASH_REPORTING_NOTICE}</p>
          </div>
        </label>

        <h2 className="text-sm font-semibold text-gray-300 mb-3">Developer</h2>

        <label className="flex items-center gap-3 cursor-pointer">
          <button
            type="button"
            role="switch"
            aria-checked={calcDebug}
            onClick={toggleCalcDebug}
            className={`relative inline-flex h-5 w-9 shrink-0 items-center rounded-full transition-colors ${
              calcDebug ? 'bg-[var(--color-primary)]' : 'bg-gray-600'
            }`}
          >
            <span
              className={`inline-block h-3.5 w-3.5 rounded-full bg-white transition-transform ${
                calcDebug ? 'translate-x-4.5' : 'translate-x-0.5'
              }`}
            />
          </button>
          <div>
            <p className="text-sm text-white">Calculation Debug Logging</p>
            <p className="text-xs text-gray-500">
              Prints detailed calculation traces to the browser console.
              Open DevTools (F12) to view.
            </p>
          </div>
        </label>

        <p className="text-xs text-gray-600 mt-3">
          Also available via console: <code className="text-gray-500">window.cohDebug.enable()</code>
        </p>
      </div>
    </>
  );
}
