/**
 * Tabbed feature-announcement spotlight ("What's New"). Auto-opens whenever the
 * ANNOUNCEMENTS registry has at least one entry the user hasn't dismissed, or
 * the current ROADMAP_VERSION hasn't been seen (announcementAutoOpens), landing
 * on the roadmap tab. Each entry is a tab (newest first), so the user can browse
 * every spotlight in one place.
 *
 * Dismissal (keyed by id, persisted in uiStore.dismissedAnnouncements):
 *   - "Maybe later" / backdrop: hide for the session only — returns next load.
 *   - Clicking a featurette's CTA: marks THAT featurette seen and acts on it.
 *   - "Don't show these again": marks every listed featurette seen.
 *
 * Self-managing, like WelcomeModal — render it once. Adding a new spotlight is
 * data-only: see data/core/announcements.
 */

import { useLayoutEffect, useState } from 'react';
import { useNavigate } from '@tanstack/react-router';
import { Modal, ModalBody, ModalFooter } from './Modal';
import { RoadmapPanel } from './RoadmapPanel';
import { Button } from '@/components/ui';
import { useUIStore } from '@/stores';
import { resolvePath } from '@/utils/paths';
import { ANNOUNCEMENTS, type AnnouncementAction } from '@/data/core/announcements';
import { announcementAutoOpens, roadmapSeenKey } from '@/data/core/roadmap';

type ActiveTab = 'roadmap' | number;

export function AnnouncementModal() {
  const dismissed = useUIStore((s) => s.dismissedAnnouncements);
  const dismiss = useUIStore((s) => s.dismissAnnouncement);
  const openChain = useUIStore((s) => s.openAttackChainModal);
  const announcementModalOpen = useUIStore((s) => s.announcementModalOpen);
  const announcementInitialTab = useUIStore((s) => s.announcementInitialTab);
  const closeAnnouncementModal = useUIStore((s) => s.closeAnnouncementModal);
  const navigate = useNavigate();

  const [visible, setVisible] = useState(true);
  const [dontShow, setDontShow] = useState(false);
  // The roadmap is the landing tab; featurettes are browsed from their own tabs.
  const [activeTab, setActiveTab] = useState<ActiveTab>('roadmap');

  const roadmapKey = roadmapSeenKey();
  const roadmapUnseen = !dismissed.includes(roadmapKey);
  const isOpen =
    announcementModalOpen ||
    (visible && announcementAutoOpens(dismissed, ANNOUNCEMENTS.map((a) => a.id)));

  // Manual open (menu) → snap back to the roadmap tab.
  useLayoutEffect(() => {
    if (announcementModalOpen && announcementInitialTab === 'roadmap') {
      setActiveTab('roadmap');
    }
  }, [announcementModalOpen, announcementInitialTab]);

  // Mark the roadmap seen once it's shown, so its "new" dot clears (covers both
  // the default landing and a manual open).
  useLayoutEffect(() => {
    if (isOpen && activeTab === 'roadmap' && roadmapUnseen) dismiss(roadmapKey);
  }, [isOpen, activeTab, roadmapUnseen, dismiss, roadmapKey]);

  if (!isOpen) return null;

  const onRoadmapTab = activeTab === 'roadmap';
  const active = typeof activeTab === 'number' ? (ANNOUNCEMENTS[activeTab] ?? ANNOUNCEMENTS[0]) : null;

  const runAction = (action: AnnouncementAction) => {
    if (action.kind === 'openModal' && action.modal === 'attackChain') openChain();
    else if (action.kind === 'navigate') navigate({ to: action.to });
  };

  const close = () => {
    if (dontShow) ANNOUNCEMENTS.forEach((a) => dismiss(a.id));
    setVisible(false);
    closeAnnouncementModal();
  };

  const act = () => {
    if (!active) return;
    dismiss(active.id);
    setVisible(false);
    closeAnnouncementModal();
    if (active.cta) runAction(active.cta.action);
  };

  const selectRoadmap = () => {
    setActiveTab('roadmap');
    dismiss(roadmapKey);
  };

  return (
    <Modal isOpen={isOpen} onClose={close} size="lg" showCloseButton={false}>
      <div className="flex gap-1 px-6 pt-5 border-b border-gray-700">
        {/* Pinned roadmap tab */}
        <button
          type="button"
          onClick={selectRoadmap}
          className={`flex items-center gap-1.5 px-3 py-2 text-sm border-b-2 -mb-px transition-colors ${
            onRoadmapTab
              ? 'text-gray-100 border-[var(--color-selected)]'
              : 'text-gray-400 border-transparent hover:text-gray-200'
          }`}
        >
          {roadmapUnseen && (
            <span className="w-1.5 h-1.5 rounded-full bg-[var(--color-sk-magenta)]" aria-label="new" />
          )}
          Road to 1.0
        </button>
        {/* Featurette tabs */}
        {ANNOUNCEMENTS.map((a, i) => {
          const isActive = activeTab === i;
          const unseen = !dismissed.includes(a.id);
          return (
            <button
              key={a.id}
              type="button"
              onClick={() => setActiveTab(i)}
              className={`flex items-center gap-1.5 px-3 py-2 text-sm border-b-2 -mb-px transition-colors ${
                isActive
                  ? 'text-gray-100 border-[var(--color-selected)]'
                  : 'text-gray-400 border-transparent hover:text-gray-200'
              }`}
            >
              {unseen && (
                <span className="w-1.5 h-1.5 rounded-full bg-[var(--color-sk-magenta)]" aria-label="new" />
              )}
              {a.tabLabel}
            </button>
          );
        })}
      </div>

      <ModalBody>
        {onRoadmapTab ? (
          <RoadmapPanel />
        ) : (
          <div className="space-y-4">
            <div className="flex items-center gap-2">
              {active?.badge && (
                <span className="text-[10px] font-medium px-1.5 py-0.5 rounded border bg-purple-900/50 text-purple-300 border-purple-700/50">
                  {active.badge}
                </span>
              )}
              <h2 className="text-lg font-medium text-gray-100">{active?.title}</h2>
            </div>
            <p className="text-sm text-gray-300 leading-relaxed">{active?.body}</p>
            {active?.image && (
              <img
                src={resolvePath(active.image)}
                alt=""
                className="w-full rounded-lg border border-gray-700"
              />
            )}
          </div>
        )}
      </ModalBody>

      <ModalFooter className="flex items-center justify-between gap-3">
        <label className="flex items-center gap-2 text-xs text-gray-400 cursor-pointer select-none">
          <input
            type="checkbox"
            checked={dontShow}
            onChange={(e) => setDontShow(e.target.checked)}
            className="w-4 h-4 rounded border-gray-600 bg-gray-800 accent-purple-500"
          />
          Don&apos;t show this again
        </label>
        <div className="flex items-center gap-2">
          <Button variant="ghost" onClick={close}>
            Ok!
          </Button>
          {active?.cta && !onRoadmapTab && (
            <Button variant="primary" onClick={act}>
              {active.cta.label}
            </Button>
          )}
        </div>
      </ModalFooter>
    </Modal>
  );
}
