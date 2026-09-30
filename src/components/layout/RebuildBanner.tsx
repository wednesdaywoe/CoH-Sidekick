import { useState } from 'react';
import { ROADMAP_CALLOUT } from '@/data/core/roadmap';

// The announcement modal already carries this call, but people close pop-ups
// without reading them. A banner in the page is harder to miss. Dismissal
// lasts the session only, so the banner comes back on the next visit.
const SESSION_KEY = 'coh-rebuild-banner-dismissed';

export function RebuildBanner() {
  const [dismissed, setDismissed] = useState(() => {
    try {
      return sessionStorage.getItem(SESSION_KEY) === '1';
    } catch {
      return false;
    }
  });

  if (dismissed) return null;

  const handleDismiss = () => {
    setDismissed(true);
    try {
      sessionStorage.setItem(SESSION_KEY, '1');
    } catch {
      // sessionStorage unavailable — the dismissal just won't outlive this render tree.
    }
  };

  return (
    <div className="bg-[var(--color-sk-magenta)] text-white text-sm flex items-center justify-center gap-3 px-4 py-1.5 flex-wrap">
      <span>
        <strong>Sidekick 1.0 is in public testing</strong> at next.coh-sidekick.com. Your saved builds open in it.
      </span>
      <a
        href={ROADMAP_CALLOUT.url}
        target="_blank"
        rel="noopener"
        className="px-2.5 py-0.5 bg-white/20 hover:bg-white/30 rounded text-white font-medium transition-colors"
      >
        Try it <span aria-hidden>&#8599;</span>
      </a>
      <button
        onClick={handleDismiss}
        className="ml-1 text-white/70 hover:text-white transition-colors"
        aria-label="Dismiss"
      >
        <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
        </svg>
      </button>
    </div>
  );
}
