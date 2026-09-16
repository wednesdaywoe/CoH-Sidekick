/**
 * Road to 1.0 roadmap content + derivation helpers.
 *
 * Drives the pinned "Road to 1.0" tab in AnnouncementModal (see RoadmapPanel).
 * A group's node state is DERIVED from its items — never hand-author it. To
 * advance the roadmap: edit item `state`s here and bump ROADMAP_VERSION so the
 * "new" dot re-surfaces for everyone (dismissal key `roadmap-v{VERSION}`).
 */

import type { ReactNode } from 'react';

export type StepState = 'done' | 'in-progress' | 'planned';

export interface RoadmapItem {
  label: string;
  /** Optional "what this entails" line shown under the item when expanded. */
  detail?: string;
  state: StepState;
}

export interface RoadmapGroup {
  /** Stable key — used for React keys and the open/closed accordion set. */
  id: string;
  title: string;
  items: RoadmapItem[];
}

/** Bump whenever a milestone advances, to re-flag the tab as "new". */
export const ROADMAP_VERSION = 1;

/** Teaser is always visible; `full` expands to the author's explainer. */
export const ROADMAP_INTRO: { teaser: ReactNode; full: ReactNode } = {
  teaser: 'A Sidekick 1.0 is on the way, but the app is being rebuilt from the ground up.',
  full: (
    <div className="space-y-4 text-sm text-gray-300 leading-relaxed">
      <section className="space-y-2">
        <h4 className="text-gray-100 font-semibold">Why do this?</h4>
        <p>
          When I started the proof-of-concept CoH-Planner in HTML and JavaScript, I
          knew nothing about how City of Heroes worked internally. I didn&apos;t
          understand its data structure. I was just building with my own
          reinterpretation of the game&apos;s mechanics as I understood them,
          supplemented by wiki, reddit, and forum searches where I had knowledge
          gaps (and there were many). But CoH-Planner was just a proof of concept,
          and I honestly didn&apos;t think it would get this far. I thought maybe the
          concept would inspire someone else with more knowledge to build it
          correctly.
        </p>
        <p>As the project grew, I needed the ability to parse CoH&apos;s binary myself…</p>
      </section>

      <section className="space-y-2">
        <h4 className="text-gray-100 font-semibold">The Parser</h4>
        <p className="italic">CoH is far more complex than many of us realize.</p>
        <p>
          The data I exported was the foundation for everything else, but it
          was incomplete. I decided what data to pull before I understood the game
          well enough to know what mattered. I thought character planning needed
          power names and their headline numbers, and some tables those numbers
          scale against. What I didn&apos;t grab was the discriminators that say how
          an effect applies and the runtime conditions that gate it.
        </p>
        <p className="italic text-sk-magenta">
          This is, objectively, a terrible way to make a CoH Character Planner.
        </p>
        <p>
          That&apos;s the original sin that Sidekick has been paying for ever since.
          It&apos;s why the alpha version was so wild, and why users continue to find
          missing effects or values being calculated incorrectly even though the
          calculation pipeline is solid (and literally the same length as Mids:
          Mids&apos; calculation engine is 3,803 lines of C# and my
          character-totals.ts is also exactly 3,803 lines). You can&apos;t just
          cherry-pick the relevant parts of a system you don&apos;t understand, and
          you can&apos;t tell just from looking; it&apos;s like cooking from a recipe
          with just a list of ingredients but no instructions on what to do with
          them. Maybe a professional chef can make that work, but I&apos;m not a
          professional chef. I have virtually no experience in game development,
          which should probably be a prerequisite before taking on a project like
          this.
        </p>
      </section>

      <section className="space-y-2">
        <h4 className="text-gray-100 font-semibold">What I&apos;m Building to Replace it</h4>
        <p>
          The next incarnation of Sidekick is simply a machine that reads all of the
          data exported directly from the game binary, and uses that data to
          determine how to present it to the user. No hardcoding the
          level that a power unlocks. The planner is <em>told</em> by the data when
          the power unlocks. This way, when servers make changes to powers,
          it&apos;s picked up automatically. When Regen gets nerfed, those nerfs are
          replicated faithfully instead of through reinterpretation.
        </p>
        <p>
          This version is being built entirely in Rust with Dioxus providing the UI
          framework, instead of Tauri. It&apos;s also being built with mobile support
          as a main feature rather than an afterthought, and a planned standalone
          desktop app (I haven&apos;t fully worked out whether the desktop app will
          have access to the build database).
        </p>
        <ul className="list-disc pl-5 space-y-1">
          <li>No JavaScript/TypeScript needed, no jumping between ecosystems. It&apos;s crabs all the way down.</li>
          <li>No IPC overhead, no separate processes for the frontend and backend, no serialization layers.</li>
          <li>Full compile-time safety.</li>
          <li>No synchronization bugs from mismatched JSON payloads between JS and Rust.</li>
          <li>Smaller footprint.</li>
        </ul>
      </section>

      <section className="space-y-2">
        <h4 className="text-gray-100 font-semibold">Thank you</h4>
        <p>
          The support and encouragement for Sidekick has been amazing. And bug
          reporters (you know who you are)...Sidekick could never get this far
          without your help!
        </p>
      </section>
    </div>
  ),
};

export const ROADMAP_GROUPS: RoadmapGroup[] = [
  {
    id: 'the-app',
    title: 'Rust + Dioxus Foundation & Interface',
    items: [
      { label: 'Data audit', state: 'done' },
      { label: 'Data pipeline', state: 'done' },
      { label: 'Unified power format', state: 'done' },
      { label: 'Strict data loading', state: 'done' },
      { label: 'Complete Bin Crawler\'s resolution for all datasets', state: 'done' },
      { label: 'App shell', state: 'done' },
      { label: 'Automated testing', state: 'done' },
      { label: 'Default theme', state: 'done' },
      { label: 'Full React-style grid, make everything re-arrangeable', state: 'done' },
    ],
  },
  {
    id: 'character-stats',
    title: 'Core Stat Engine',
    items: [
      { label: 'To-hit', state: 'done' },
      { label: 'Damage', state: 'done' },
      { label: 'Max HP', state: 'done' },
      { label: 'Regen & recovery', state: 'done' },
      { label: 'Movement', state: 'done' },
      { label: 'Enhancements & ED', state: 'done' },
      { label: 'Build-wide totals', state: 'done' },
    ],
  },
  {
    id: 'building',
    title: 'Build & Slotting',
    items: [
      { label: 'Character build model', state: 'done' },
      { label: 'Active-power resolution', state: 'done' },
      { label: 'Self-buffs', state: 'done' },
      { label: 'Per-power bonuses', state: 'done' },
      { label: 'Complete totals', state: 'done' },
      { label: 'Whole-build verification', state: 'done' },
      { label: 'Save & restore', state: 'done' },
      { label: 'Undo / redo', state: 'done' },
      { label: 'Power picker & slotting UI', state: 'done' },
      { label: 'Level-difference modifiers', state: 'done' },

    ],
  },
  {
    id: 'enhancements',
    title: 'Sets, Procs & Incarnates and Services',
    items: [
      { label: 'Formula engine', state: 'done' },
      { label: 'Set bonuses', state: 'done' },
      { label: 'Incarnate data prep', state: 'done' },
      { label: 'Procs & incarnates', state: 'done' },
      { label: 'Travel powers', state: 'done' },
      { label: 'Server wiring, Auth', state: 'planned' },
      { label: 'Native desktop app', state: 'done' },
      { label: 'Stats dashboard', state: 'done' },
      { label: 'Reimplementation of all helper modals', state: 'in-progress' },
    ],
  },
  {
    id: 'import-export',
    title: 'Importing & exporting builds',
    items: [
      { label: 'Native save format', state: 'done' },
      { label: 'Importer utils', state: 'done' },
      { label: "Export to BBCode, image, SKIF", state: 'done' },
      { label: 'Data cleanup', state: 'planned' },
      { label: "Beta handover", state: 'planned' },
    ],
  },
];
/** Derived node state — all done → done; all planned → planned; else in-progress. */
export function deriveGroupState(group: RoadmapGroup): StepState {
  const states = group.items.map((i) => i.state);
  if (states.every((s) => s === 'done')) return 'done';
  if (states.every((s) => s === 'planned')) return 'planned';
  return 'in-progress';
}

/** Total item count and done count across all groups. */
export function roadmapProgress(
  groups: RoadmapGroup[] = ROADMAP_GROUPS,
): { done: number; total: number } {
  let done = 0;
  let total = 0;
  for (const g of groups) {
    for (const it of g.items) {
      total += 1;
      if (it.state === 'done') done += 1;
    }
  }
  return { done, total };
}
