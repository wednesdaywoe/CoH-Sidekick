/**
 * What a feedback report carries, and the words the form shows for it.
 *
 * F88, and the port of F37's fix from the rebuild. The form disclosed the one field that had a
 * control beside it — the build snapshot — while `userId` and `userName` went with every
 * signed-in report and `userAgent` went with every report at all, none of them mentioned
 * anywhere. The fix is not the wording. A hand-written list is accurate the day it is typed and
 * drifts the first time the payload grows, and nothing says so, because a disclosure is a
 * surface people read once rather than one anybody re-reads.
 *
 * So the form renders [`DISCLOSED`] and the guard grades that table against a payload
 * [`feedbackReport`] actually builds: a field added with no line reds rather than quietly
 * joining the set the user was never told about.
 */

import type { DiagnosticsSnapshot } from '@/utils/diagnostics';

export type FeedbackType = 'bug' | 'suggestion' | 'other';

/** The build summary the form prints above the disclosure. */
export interface BuildContext {
  archetype: string;
  level: number;
  primary: string;
  secondary: string;
  pools: string[];
  epicPool: string | null;
  powerCount: number;
  slotCount: number;
}

/**
 * One submission, as the worker reads it.
 *
 * Absent optionals are `undefined` so `JSON.stringify` omits them rather than sending `null` —
 * the worker's checks are truthiness tests either way, and matching the bytes keeps this client
 * and the rebuild's comparable in a network log.
 */
export interface FeedbackPayload {
  type: FeedbackType;
  description: string;
  globalName?: string;
  userId?: string;
  userName?: string;
  buildContext: BuildContext;
  buildSnapshot?: string;
  diagnostics?: DiagnosticsSnapshot;
  userAgent: string;
  timestamp: string;
}

/** Who is reporting, when the report carries an account. */
export interface Reporter {
  id: string;
  displayName?: string;
}

/**
 * What a report carries, in the words the form shows, keyed to the wire fields each line covers.
 *
 * Keys are dotted paths into the serialized payload, matched by whole components, and each names
 * either a leaf or the object directly above the leaves one sentence covers.
 *
 * **What the guard cannot check is the depth.** A path written shallow covers every leaf beneath
 * it and passes forever while telling the reader nothing, which is the understatement this row is
 * about. Where a path sits is a reading; that nothing travels uncovered, and that no line has
 * outlived its field, is the check.
 *
 * `diagnostics.ui` is the one path deliberately written at the object rather than at its leaves,
 * and the reason is that its key set is not fixed: `getDiagnosticsSnapshot` filters the adjuster
 * records down to what is set and adds the archetype's own mechanic flags, so the leaves differ
 * between two reports from the same build. A sentence naming the category is the honest form of
 * that, and the cost is stated rather than hidden — a new planner setting joins the snapshot under
 * a line that already covers it. A new diagnostics GROUP beside `app`, `env` and `ui` still reds,
 * which is the drift that would actually go unsaid.
 */
export const DISCLOSED: ReadonlyArray<{ paths: readonly string[]; line: string }> = [
  {
    paths: ['type', 'description', 'globalName'],
    line: 'What you typed: the kind of report, the description, and the global name if you gave one.',
  },
  {
    paths: ['userId', 'userName'],
    line: 'Your account, when you are signed in: its id and your display name.',
  },
  { paths: ['buildContext'], line: 'The build summary above.' },
  {
    paths: ['buildSnapshot'],
    line: 'The whole build; powers, slots, enhancements — while "Include build snapshot" is ticked.',
  },
  {
    paths: ['diagnostics.app', 'diagnostics.env.datasetId'],
    line: 'Which Sidekick this is: the version, when it was built, and the dataset it loaded — while the box is ticked.',
  },
  {
    paths: ['diagnostics.ui'],
    line: 'Your planner settings: level-up and combat mode, exemplar, enhancement levels, proc settings, tracked stats and your archetype’s toggles — while the box is ticked.',
  },
  {
    paths: ['userAgent', 'diagnostics.env.userAgent', 'diagnostics.env.url', 'diagnostics.env.viewport'],
    line: 'Your browser’s user-agent string, and — while the box is ticked — the page address and the window size.',
  },
  { paths: ['timestamp'], line: 'When you sent it.' },
];

/**
 * Assemble the submission, given everything already resolved.
 *
 * Split out of the click handler for the reason the rebuild's `feedback::report` is: the shape of
 * a report is gradeable without a DOM, and which fields carry through from the form is the part
 * that can be got wrong silently. The guard needs a real payload to grade the table against, and
 * a payload built inside a React handler is one no test can name.
 */
export function feedbackReport(args: {
  type: FeedbackType;
  description: string;
  globalName: string;
  user: Reporter | null;
  buildContext: BuildContext;
  buildSnapshot?: string;
  diagnostics?: DiagnosticsSnapshot;
  userAgent: string;
  timestamp: string;
}): FeedbackPayload {
  return {
    type: args.type,
    description: args.description.trim(),
    globalName: args.globalName.trim() || undefined,
    userId: args.user?.id,
    userName: args.user?.displayName,
    buildContext: args.buildContext,
    buildSnapshot: args.buildSnapshot,
    diagnostics: args.diagnostics,
    userAgent: args.userAgent,
    timestamp: args.timestamp,
  };
}
