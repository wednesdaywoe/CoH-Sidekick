/**
 * The "Road to 1.0" panel rendered inside AnnouncementModal's pinned tab.
 * A git-graph of the roadmap: group nodes sit on a vertical trunk; expanding a
 * group sprouts its item nodes on a branch curving off the trunk. State is
 * carried entirely by the node ring — no emoji. Pure presentation over the
 * roadmap data model — see data/core/roadmap.
 */

import { useState } from 'react';
import {
  ROADMAP_CALLOUT,
  ROADMAP_INTRO,
  ROADMAP_GROUPS,
  deriveGroupState,
  roadmapProgress,
  type StepState,
  type RoadmapGroup,
} from '@/data/core/roadmap';

/**
 * A single graph node. State is drawn by the ring, not an emoji:
 *   done        → magenta ring with a filled magenta center dot
 *   in-progress → magenta ring, hollow center, subtle glow (the "current work")
 *   planned     → grey ring, hollow center
 * `size` scales the same visual between the trunk (group) and branch (item).
 */
function NodeMarker({ state, size }: { state: StepState; size: 'group' | 'item' }) {
  const isGroup = size === 'group';
  const ringColor =
    state === 'planned' ? 'border-gray-600' : 'border-[var(--color-sk-magenta)]';
  const glow = state === 'in-progress' ? 'roadmap-glow' : '';
  return (
    <span
      className={`inline-flex items-center justify-center rounded-full border-2 bg-transparent ${
        isGroup ? 'w-6 h-6' : 'w-3.5 h-3.5'
      } ${ringColor} ${glow}`}
      aria-hidden
    >
      {state === 'done' && (
        <span
          className={`rounded-full bg-[var(--color-sk-magenta)] ${
            isGroup ? 'w-2.5 h-2.5' : 'w-1.5 h-1.5'
          }`}
        />
      )}
    </span>
  );
}

function groupProgress(group: RoadmapGroup): string {
  const done = group.items.filter((i) => i.state === 'done').length;
  return `${done}/${group.items.length}`;
}

/**
 * The call to action at the head of the tab: a screenshot of 1.0 with a
 * finished build open, fading into the headline, then the blurb and the link
 * out. The screenshot is a link too, since it is the thing people aim at.
 */
function RebuildCallout() {
  return (
    <div className="overflow-hidden rounded-lg border border-gray-700 bg-gray-800/60">
      <a href={ROADMAP_CALLOUT.url} target="_blank" rel="noopener" className="group relative block">
        <img
          src={ROADMAP_CALLOUT.image}
          alt={ROADMAP_CALLOUT.imageAlt}
          className="block w-full transition-transform duration-500 group-hover:scale-[1.02] motion-reduce:transition-none motion-reduce:group-hover:scale-100"
        />
        {/* Fades the screenshot into the card, so the headline sits on it. */}
        <span className="absolute inset-x-0 bottom-0 h-2/5 bg-gradient-to-t from-gray-900 via-gray-900/80 to-transparent" aria-hidden />
        <span className="absolute left-4 bottom-3 space-y-0.5">
          <span className="block text-xs font-semibold uppercase tracking-wider text-[var(--color-sk-magenta)]">
            {ROADMAP_CALLOUT.eyebrow}
          </span>
          <span className="block text-2xl font-bold text-white drop-shadow">{ROADMAP_CALLOUT.headline}</span>
        </span>
      </a>
      <div className="p-4 space-y-3">
        <p className="text-sm text-gray-300 leading-relaxed [&_code]:text-gray-100 [&_strong]:text-gray-100">
          {ROADMAP_CALLOUT.blurb}
        </p>
        {/* A real link in a new tab, so this planner stays open behind it. */}
        <a
          href={ROADMAP_CALLOUT.url}
          target="_blank"
          rel="noopener"
          className="inline-flex items-center gap-1.5 rounded-md bg-[var(--color-sk-magenta)] px-4 py-2 text-sm font-semibold text-white transition-transform hover:scale-[1.03] hover:brightness-110"
        >
          {ROADMAP_CALLOUT.button}
          <span aria-hidden>&#8599;</span>
        </a>
      </div>
    </div>
  );
}

export function RoadmapPanel() {
  const [introOpen, setIntroOpen] = useState(false);
  // Default-expand the in-progress groups so the "current work" is visible.
  const [openGroups, setOpenGroups] = useState<Set<string>>(
    () =>
      new Set(
        ROADMAP_GROUPS.filter((g) => deriveGroupState(g) === 'in-progress').map((g) => g.id),
      ),
  );
  const { done, total } = roadmapProgress();
  const pct = total === 0 ? 0 : Math.round((done / total) * 100);

  const toggleGroup = (id: string) =>
    setOpenGroups((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  return (
    <div className="space-y-5">
      <RebuildCallout />

      {/* Expandable intro */}
      <div className="rounded-lg border border-gray-700 bg-gray-800/40">
        <button
          type="button"
          onClick={() => setIntroOpen((o) => !o)}
          aria-expanded={introOpen}
          className="w-full flex items-start gap-2.5 text-left px-4 py-3 rounded-lg hover:bg-gray-700/40 transition-colors"
        >
          <span
            className={`mt-px text-lg leading-none text-[var(--color-sk-magenta)] transition-transform ${
              introOpen ? 'rotate-90' : ''
            }`}
            aria-hidden
          >
            ▸
          </span>
          <span className="flex-1 text-sm text-gray-200">{ROADMAP_INTRO.teaser}</span>
          <span className="mt-px shrink-0 text-xs font-semibold text-[var(--color-sk-magenta)]">
            {introOpen ? 'Show less' : 'Read more'}
          </span>
        </button>
        {introOpen && (
          <div className="px-4 pb-4 max-h-72 overflow-y-auto">{ROADMAP_INTRO.full}</div>
        )}
      </div>

      {/* Progress bar */}
      <div className="space-y-1">
        <div className="flex items-center justify-between text-xs text-gray-400">
          <span>Progress</span>
          <span>
            {done} of {total} done
          </span>
        </div>
        <div className="h-2 rounded-full bg-gray-700 overflow-hidden">
          <div
            className="h-full bg-[var(--color-sk-magenta)] rounded-full transition-[width]"
            style={{ width: `${pct}%` }}
          />
        </div>
      </div>

      {/* Node graph: trunk of group nodes, item nodes on branches */}
      <ol className="relative pl-1">
        {ROADMAP_GROUPS.map((group, idx) => {
          const state = deriveGroupState(group);
          const isOpen = openGroups.has(group.id);
          const panelId = `roadmap-group-${group.id}`;
          const isLast = idx === ROADMAP_GROUPS.length - 1;
          return (
            <li key={group.id} className="relative pl-8 pb-4">
              {/* Trunk: vertical line below this node down to the next group.
                  Starts at the node's bottom edge (top-[2px] + h-6 = 26px). */}
              {!isLast && (
                <span
                  className="absolute left-[11px] top-[26px] bottom-0 w-0.5 bg-gray-600"
                  aria-hidden
                />
              )}
              {/* Group node on the trunk. `flex` collapses the wrapper to the marker
                  so top-[2px] centres the ring on the title's first text line. */}
              <span className="absolute left-0 top-[2px] flex" aria-hidden>
                <NodeMarker state={state} size="group" />
              </span>
              <button
                type="button"
                onClick={() => toggleGroup(group.id)}
                aria-expanded={isOpen}
                aria-controls={panelId}
                className="w-full flex items-center gap-2 text-left py-1"
              >
                <span className="text-sm font-medium text-gray-100 flex-1">{group.title}</span>
                <span className="text-xs text-gray-400 tabular-nums">{groupProgress(group)}</span>
                <span
                  className={`text-lg leading-none text-gray-400 transition-transform ${
                    isOpen ? 'rotate-90' : ''
                  }`}
                  aria-hidden
                >
                  ▸
                </span>
              </button>

              {isOpen && (
                // -ml-8 cancels the li's pl-8 so branch elements share the trunk's
                // coordinate origin (trunk at x=11, branch at x=26).
                <div id={panelId} className="relative -ml-8 mt-1">
                  {/* Elbow: peels off the trunk and curves into the branch line */}
                  <span
                    className="absolute left-[11px] top-0 h-3 w-[15px] border-l-2 border-b-2 border-gray-600 rounded-bl-lg"
                    aria-hidden
                  />
                  {/* Branch line the item nodes sit on */}
                  <span
                    className="absolute left-[26px] top-3 bottom-5 w-0.5 bg-gray-600"
                    aria-hidden
                  />
                  <ul>
                    {group.items.map((item, i) => (
                      <li key={i} className="relative pl-10 pb-2.5">
                        {/* Item node on the branch. `flex` collapses the wrapper to
                            the marker (an inline wrapper baseline-drops the ring ~7px);
                            top-[3px] then centres it on the label's first text line. */}
                        <span className="absolute left-[20px] top-[3px] flex" aria-hidden>
                          <NodeMarker state={item.state} size="item" />
                        </span>
                        <div className="text-sm text-gray-300">
                          {item.label}
                          {item.detail && (
                            <span className="block text-xs text-gray-500">{item.detail}</span>
                          )}
                        </div>
                      </li>
                    ))}
                  </ul>
                </div>
              )}
            </li>
          );
        })}
      </ol>
    </div>
  );
}
