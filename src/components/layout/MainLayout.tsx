/**
 * MainLayout component - overall app layout wrapper
 */

import { useEffect, useState, type ReactNode } from 'react';
import { DISCORD_INVITE_URL } from '@/lib/links';
import { Header } from './Header';
import { StatsDashboard } from './StatsDashboard';
import { UpdateBanner } from './UpdateBanner';
import { StatusBanner } from './StatusBanner';
import { RebuildBanner } from './RebuildBanner';
import { EngineErrorBanner } from './EngineErrorBanner';
import { CalcErrorBanner } from './CalcErrorBanner';
import { RuleOf5Banner } from './RuleOf5Banner';
import { ExemplarModeBanner } from './ExemplarModeBanner';
import { MobileBottomNav } from './MobileBottomNav';
import { MobileBuildBar } from './MobileBuildBar';
import { EnhancementPicker } from '@/components/enhancements/EnhancementPicker';
import { PowerInfoTooltip } from '@/components/info';
import { PowerInfoModal } from '@/components/modals';
// OnboardingBeacon disabled — UX research showed users didn't recognize the
// glowing outline as instructional. Component preserved for a future tutorial
// revisit; the help-discovery toast handles first-run pointers for now.
// import { OnboardingBeacon } from '@/components/onboarding/OnboardingBeacon';
import { ToastContainer } from '@/components/ui/Toast';
import { useUIStore, useAuthStore } from '@/stores';
import { useStatusCheck } from '@/hooks/useStatusCheck';
import { useUndoRedoKeyboard } from '@/hooks/useUndoRedoKeyboard';
import { useTooltipHotkey } from '@/hooks/useTooltipHotkey';
import { useDashboardCollapseHotkey } from '@/hooks/useDashboardCollapseHotkey';
import { useInfoPanelLockHotkey } from '@/hooks/useInfoPanelLockHotkey';

// CSS `zoom` on the root div creates a coordinate-system mismatch with
// portals that render to document.body (notably the OnboardingBeacon). The
// UI-scale control is desktop-only now, but a value saved from a prior desktop
// session can still be read from localStorage on mobile — forcing zoom on a
// viewport that never exposes the control to change it. Gate the zoom by
// viewport width so mobile always renders unscaled regardless of stored value.
const MOBILE_MAX_WIDTH = 1024;

interface MainLayoutProps {
  children: ReactNode;
}

export function MainLayout({ children }: MainLayoutProps) {
  const openFeedbackModal = useUIStore((s) => s.openFeedbackModal);
  const openDonateModal = useUIStore((s) => s.openDonateModal);
  const openHelpModal = useUIStore((s) => s.openHelpModal);
  const openWelcomeModal = useUIStore((s) => s.openWelcomeModal);
  const uiScale = useUIStore((s) => s.uiScale);
  const activeStatus = useStatusCheck();
  useUndoRedoKeyboard();
  useTooltipHotkey();
  useDashboardCollapseHotkey();
  useInfoPanelLockHotkey();
  const initializeAuth = useAuthStore((s) => s.initialize);

  const [isMobile, setIsMobile] = useState(() =>
    typeof window !== 'undefined' && window.innerWidth <= MOBILE_MAX_WIDTH
  );
  useEffect(() => {
    const update = () => setIsMobile(window.innerWidth <= MOBILE_MAX_WIDTH);
    window.addEventListener('resize', update);
    window.addEventListener('orientationchange', update);
    return () => {
      window.removeEventListener('resize', update);
      window.removeEventListener('orientationchange', update);
    };
  }, []);

  // Initialize auth on mount (checks existing session, listens for changes)
  useEffect(() => {
    const unsubscribe = initializeAuth();
    return unsubscribe;
  }, [initializeAuth]);

  const applyZoom = !isMobile && uiScale !== 1;

  return (
    <div
      className="min-h-screen bg-gray-950 text-gray-100 flex flex-col"
      style={applyZoom ? { zoom: uiScale, overflowX: 'clip' as const } : undefined}
    >
      <EngineErrorBanner />
      <CalcErrorBanner />
      <StatusBanner active={activeStatus} />
      <UpdateBanner />
      <RebuildBanner />
      <Header />
      {/* Mobile-only: keeps level + pick/slot budget pinned while the page scrolls */}
      <MobileBuildBar />
      {/* StatsDashboard is accessible on mobile via the bottom nav's Dashboard tab */}
      <div className="hidden lg:block">
        <StatsDashboard />
      </div>
      {/* Educational banner — placed below the dashboard so it sits in an
       *  unexpected spot relative to the top-anchored update/status banners.
       *  Top-of-page banner blindness was causing users to miss it. */}
      <RuleOf5Banner />
      <ExemplarModeBanner />
      {/* `lg:overflow-visible` lets `main` grow past the viewport so the whole
          page can scroll. On a tall screen the planner pins itself to `main` with
          `absolute inset-0` and scrolls its columns internally (nothing overflows
          `main`, so this is a no-op); on a short screen the planner switches to
          content-flow (see PlannerPage's `tallViewport`), grows taller than the
          viewport, and the page scrolls instead of trapping content in tiny
          internal scrollboxes. Base stays `overflow-hidden` for the mobile
          fallback, which manages its own scroll. */}
      <main className="flex-1 overflow-hidden lg:overflow-visible relative">
        {children}
      </main>

      <MobileBottomNav />

      {/* Global modals */}
      <EnhancementPicker />
      <PowerInfoModal />

      {/* Power info tooltip (follows mouse when enabled) */}
      <PowerInfoTooltip />

      {/* Toasts (status notifications, help-discovery hint) */}
      <ToastContainer />

      {/* Floating buttons — hidden on mobile (those actions live in the bottom nav's Menu sheet) */}
      <div className="hidden lg:flex fixed bottom-4 right-4 z-40 items-center gap-2">
        <button
          onClick={() => openHelpModal()}
          className="flex items-center justify-center w-9 h-9 bg-[var(--color-primary)] hover:bg-[var(--color-primary-hover)] text-on-primary rounded-full shadow-lg transition-colors border border-[var(--color-primary-hover)]"
          title="Help"
          aria-label="Open help"
          data-onboarding="help"
        >
          <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8.228 9c.549-1.165 2.03-2 3.772-2 2.21 0 4 1.343 4 3 0 1.4-1.278 2.575-3.006 2.907-.542.104-.994.54-.994 1.093m0 3h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
          </svg>
        </button>
        <button
          onClick={openWelcomeModal}
          className="flex items-center justify-center w-9 h-9 bg-slate-700 hover:bg-slate-600 text-amber-400 hover:text-amber-300 rounded-full shadow-lg transition-colors border border-amber-400"
          title="What's New"
          aria-label="What's new in Sidekick"
        >
          <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 3v4M3 5h4M6 17v4m-2-2h4m5-16l2.286 6.857L21 12l-5.714 2.143L13 21l-2.286-6.857L5 12l5.714-2.143L13 3z" />
          </svg>
        </button>
        <button
          onClick={openFeedbackModal}
          className="flex items-center justify-center w-9 h-9 bg-slate-700 hover:bg-slate-600 text-slate-300 hover:text-white rounded-full shadow-lg transition-colors"
          style={{ border: '1px solid #d632ce' }}
          title="Send feedback or report a bug"
          aria-label="Send feedback or report a bug"
        >
          <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 10h.01M12 10h.01M16 10h.01M9 16H5a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v8a2 2 0 01-2 2h-5l-5 5v-5z" />
          </svg>
        </button>
        <a
          href={DISCORD_INVITE_URL}
          target="_blank"
          rel="noopener noreferrer"
          className="flex items-center justify-center w-9 h-9 bg-slate-700 hover:bg-slate-600 rounded-full shadow-lg transition-colors border border-[#5865F2]"
          style={{ color: '#5865F2' }}
          title="Join the Sidekick Discord"
          aria-label="Join the Sidekick Discord"
        >
          <svg className="w-5 h-5" fill="currentColor" viewBox="0 0 16 16">
            <path d="M13.545 2.907a13.2 13.2 0 0 0-3.257-1.011.05.05 0 0 0-.052.025c-.141.25-.297.577-.406.833a12.2 12.2 0 0 0-3.658 0 8 8 0 0 0-.412-.833.05.05 0 0 0-.052-.025c-1.125.194-2.22.534-3.257 1.011a.04.04 0 0 0-.021.018C.356 6.024-.213 9.047.066 12.032q.003.022.021.037a13.3 13.3 0 0 0 3.995 2.02.05.05 0 0 0 .056-.019q.463-.63.818-1.329a.05.05 0 0 0-.01-.059l-.018-.011a9 9 0 0 1-1.248-.595.05.05 0 0 1-.02-.066l.015-.019q.127-.095.248-.195a.05.05 0 0 1 .051-.007c2.619 1.196 5.454 1.196 8.041 0a.05.05 0 0 1 .053.007q.121.1.248.195a.05.05 0 0 1-.004.085 8 8 0 0 1-1.249.594.05.05 0 0 0-.03.03.05.05 0 0 0 .003.041c.24.465.515.909.817 1.329a.05.05 0 0 0 .056.019 13.2 13.2 0 0 0 4.001-2.02.05.05 0 0 0 .021-.037c.334-3.451-.559-6.449-2.366-9.106a.03.03 0 0 0-.02-.019m-8.198 7.307c-.789 0-1.438-.724-1.438-1.612s.637-1.613 1.438-1.613c.807 0 1.45.73 1.438 1.613 0 .888-.637 1.612-1.438 1.612m5.316 0c-.788 0-1.438-.724-1.438-1.612s.637-1.613 1.438-1.613c.807 0 1.451.73 1.438 1.613 0 .888-.631 1.612-1.438 1.612"/>
          </svg>
        </a>
        <button
          onClick={openDonateModal}
          className="flex items-center gap-1.5 h-9 px-3 bg-slate-700 hover:bg-slate-600 text-slate-300 hover:text-white rounded-full shadow-lg transition-colors text-sm border border-purple-500"
          title="Support Sidekick — buy me a coffee!"
          aria-label="Support Sidekick"
        >
          <svg className="w-4 h-4 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 18h10a2 2 0 002-2V8H5v8a2 2 0 002 2zM17 8h2a2 2 0 010 4h-2M8 2v3M12 2v3" />
          </svg>
          Support Sidekick
        </button>
      </div>
    </div>
  );
}
