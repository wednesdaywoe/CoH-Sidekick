/**
 * FeedbackModal - allows users to submit bug reports, suggestions, and general feedback.
 * Sends data to a Cloudflare Worker which forwards it as a formatted email via Resend.
 */

import { useState } from 'react';
import { DISCORD_INVITE_URL, DISCORD_FEEDBACK_DM_URL } from '@/lib/links';
import { Modal, ModalBody, ModalFooter, ModalHeader } from './Modal';
import { Button } from '../ui/Button';
import { useBuildStore } from '@/stores/buildStore';
import { useAuthStore } from '@/stores/authStore';
import { getDiagnosticsSnapshot } from '@/utils/diagnostics';
import { DISCLOSED, feedbackReport } from './feedbackReport';
import type { FeedbackType } from './feedbackReport';

// Worker endpoint URL - update this after deploying the Cloudflare Worker
const FEEDBACK_API_URL = 'https://coh-planner-feedback.wedswoe.workers.dev';

interface FeedbackModalProps {
  isOpen: boolean;
  onClose: () => void;
}

type SubmitStatus = 'idle' | 'sending' | 'success' | 'error';

const TYPE_CONFIG = {
  bug: {
    label: 'Bug Report',
    placeholder:
      'Please include:\n' +
      '1. What you were doing (e.g. "Imported a Claws/Dark Scrapper .mbd file")\n' +
      '2. What went wrong (e.g. "S/L Res totals show 0% despite having Dark Embrace toggled on")\n' +
      '3. What you expected (e.g. "Should show ~25% S/L Res from Dark Embrace")',
    color: 'red',
  },
  suggestion: {
    label: 'Suggestion',
    placeholder:
      'Describe the feature or improvement you\'d like to see.\n' +
      'What problem would it solve? How would you use it?',
    color: 'blue',
  },
  other: {
    label: 'Other',
    placeholder: 'Share your thoughts...',
    color: 'purple',
  },
} as const;

export function FeedbackModal({ isOpen, onClose }: FeedbackModalProps) {
  const [feedbackType, setFeedbackType] = useState<FeedbackType>('bug');
  const [description, setDescription] = useState('');
  const [globalName, setGlobalName] = useState('');
  const [includeSnapshot, setIncludeSnapshot] = useState(true);
  const [snapshotCopied, setSnapshotCopied] = useState(false);
  const [status, setStatus] = useState<SubmitStatus>('idle');
  const [errorMessage, setErrorMessage] = useState('');

  const build = useBuildStore((s) => s.build);
  const exportBuild = useBuildStore((s) => s.exportBuild);
  const user = useAuthStore((s) => s.user);

  const getBuildContext = () => {
    // Count total powers and slots across all sets
    let powerCount = 0;
    let slotCount = 0;

    const countPowers = (powers: Array<{ name: string; slots: unknown[] }>) => {
      for (const p of powers) {
        if (p.name) {
          powerCount++;
          slotCount += p.slots.filter(Boolean).length;
        }
      }
    };

    countPowers(build.primary.powers);
    countPowers(build.secondary.powers);
    for (const pool of build.pools) {
      countPowers(pool.powers);
    }
    if (build.epicPool) {
      countPowers(build.epicPool.powers);
    }

    return {
      archetype: build.archetype.name || 'None',
      level: build.level,
      primary: build.primary.name || 'None',
      secondary: build.secondary.name || 'None',
      pools: build.pools.map((p) => p.name).filter(Boolean),
      epicPool: build.epicPool?.name || null,
      powerCount,
      slotCount,
    };
  };

  const handleSubmit = async () => {
    if (!description.trim()) return;

    setStatus('sending');
    setErrorMessage('');

    // Assembled by `feedbackReport` rather than inline, so the disclosure guard can grade
    // DISCLOSED against a payload this function actually builds. Signed-in account identity is
    // auto-attached; the display-name derivation is the app's usual one (Discord OAuth metadata).
    const payload = feedbackReport({
      type: feedbackType,
      description,
      globalName,
      user: user
        ? {
            id: user.id,
            displayName:
              (user.user_metadata?.full_name as string | undefined) ||
              (user.user_metadata?.name as string | undefined) ||
              user.email ||
              undefined,
          }
        : null,
      buildContext: getBuildContext(),
      buildSnapshot: includeSnapshot ? exportBuild() : undefined,
      diagnostics: includeSnapshot ? getDiagnosticsSnapshot() : undefined,
      userAgent: navigator.userAgent,
      timestamp: new Date().toISOString(),
    });

    try {
      const response = await fetch(FEEDBACK_API_URL, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
      });

      if (response.ok) {
        setStatus('success');
        setTimeout(() => {
          handleClose();
        }, 2000);
      } else {
        const data = await response.json().catch(() => ({}));
        setErrorMessage(data.error || 'Failed to send feedback. Please try again.');
        setStatus('error');
      }
    } catch {
      setErrorMessage('Network error. Please check your connection and try again.');
      setStatus('error');
    }
  };

  const handleClose = () => {
    setFeedbackType('bug');
    setDescription('');
    setGlobalName('');
    setIncludeSnapshot(true);
    setSnapshotCopied(false);
    setStatus('idle');
    setErrorMessage('');
    onClose();
  };

  const handleCopySnapshot = async () => {
    try {
      await navigator.clipboard.writeText(exportBuild());
      setSnapshotCopied(true);
      setTimeout(() => setSnapshotCopied(false), 2000);
    } catch {
      // Fallback for browsers that don't support clipboard API
      const textarea = document.createElement('textarea');
      textarea.value = exportBuild();
      document.body.appendChild(textarea);
      textarea.select();
      document.execCommand('copy');
      document.body.removeChild(textarea);
      setSnapshotCopied(true);
      setTimeout(() => setSnapshotCopied(false), 2000);
    }
  };

  const config = TYPE_CONFIG[feedbackType];
  const canSubmit = description.trim().length >= 10 && status !== 'sending' && status !== 'success';

  return (
    <Modal isOpen={isOpen} onClose={handleClose} size="lg" showCloseButton={true}>
      <ModalHeader>Feedback</ModalHeader>

      <ModalBody>
        {status === 'success' ? (
          <div className="bg-[var(--color-success)]/15 border border-[var(--color-success)]/50 rounded p-4 text-center">
            <p className="text-[var(--color-success-fg)] font-semibold text-lg mb-1">Thank you!</p>
            <p className="text-[var(--color-success-fg)]/80 text-sm">Your feedback has been sent.</p>
          </div>
        ) : (
          <div className="space-y-4">
            {/* Contact notice */}
            <div className="bg-amber-900/30 border-2 border-amber-500/60 rounded-lg p-3">
              <p className="text-center text-amber-400 font-bold text-sm mb-2">!!! PLEASE READ !!!</p>
              <p className="text-sm text-amber-200 font-medium">
                If you have a question and would like a response, please contact{' '}
                <a
                  href={DISCORD_FEEDBACK_DM_URL}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-amber-300 underline hover:text-amber-100"
                >
                  WW on Discord
                </a>
                , message{' '}
                <a
                  href="https://www.reddit.com/message/compose/?to=wednesdaywoe13"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-amber-300 underline hover:text-amber-100"
                >
                  WW on Reddit
                </a>
                {' '}or join the{' '}
                <a
                  href={DISCORD_INVITE_URL}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-amber-300 underline hover:text-amber-100"
                >
                  Sidekick Discord channel (if the link doesn't work, just reach out WW directly on Discord)
                </a>
                . I'm not able to respond to questions submitted through the reporting tool.
              </p>
            </div>

            {/* Type selector */}
            <div>
              <label className="block text-sm font-medium text-gray-300 mb-2">
                Feedback Type
              </label>
              <div className="flex gap-2">
                {(Object.entries(TYPE_CONFIG) as [FeedbackType, typeof TYPE_CONFIG[FeedbackType]][]).map(
                  ([type, cfg]) => {
                    const isActive = feedbackType === type;
                    const colorMap = {
                      red: isActive
                        ? 'bg-[var(--color-selected)]/20 border-[var(--color-selected)] text-[var(--color-link)]'
                        : 'border-gray-600 text-gray-400 hover:border-gray-500 hover:text-gray-300',
                      blue: isActive
                        ? 'bg-[var(--color-selected)]/20 border-[var(--color-selected)] text-[var(--color-link)]'
                        : 'border-gray-600 text-gray-400 hover:border-gray-500 hover:text-gray-300',
                      purple: isActive
                        ? 'bg-[var(--color-selected)]/20 border-[var(--color-selected)] text-[var(--color-link)]'
                        : 'border-gray-600 text-gray-400 hover:border-gray-500 hover:text-gray-300',
                    };
                    return (
                      <button
                        key={type}
                        onClick={() => setFeedbackType(type)}
                        className={`flex-1 px-3 py-2 rounded border text-sm font-medium transition-colors ${colorMap[cfg.color]}`}
                      >
                        {cfg.label}
                      </button>
                    );
                  }
                )}
              </div>
            </div>

            {/* Guidance tips per feedback type */}
            {feedbackType === 'bug' && (
              <div>
                <div className="bg-slate-800/50 border border-slate-700/50 rounded p-3">
                  <p className="text-xs font-medium text-slate-400 mb-1.5">Tips for a helpful bug report:</p>
                  <ul className="text-xs text-slate-500 space-y-0.5 list-disc list-inside">
                    <li>Describe what you were doing when the bug occurred</li>
                    <li>Note what happened vs. what you expected</li>
                    <li>Pease ensure the build snapshot below is enabled, it will help me reproduce the issue</li>
                  </ul>
                </div>
              </div>
            )}
            {feedbackType === 'suggestion' && (
              <div className="bg-[var(--color-info)]/15 border border-[var(--color-info)]/50 rounded p-3">
                <p className="text-xs font-medium text-[var(--color-info-fg)] mb-1.5">Tips for a great suggestion:</p>
                <ul className="text-xs text-[var(--color-info-fg)]/70 space-y-0.5 list-disc list-inside">
                  <li>Describe the problem or gap you've noticed</li>
                  <li>Explain how the feature would help your workflow</li>
                  <li>If possible, reference how other tools handle it</li>
                </ul>
              </div>
            )}

            {/* Description */}
            <div>
              <label className="block text-sm font-medium text-gray-300 mb-2">
                Description
              </label>
              <textarea
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                placeholder={config.placeholder}
                className="w-full h-40 bg-gray-800 border border-gray-600 rounded px-3 py-2 text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-[var(--color-ring)] text-sm"
                maxLength={5000}
              />
              <p className="text-xs text-gray-500 mt-1">
                {description.length < 10
                  ? `Minimum 10 characters (${10 - description.length} more needed)`
                  : `${description.length}/5000`}
              </p>
            </div>

            {/* Global Name */}
            <div>
              <label className="block text-sm font-medium text-gray-300 mb-2">
                In-game Global Name <span className="text-gray-500 font-normal">(optional)</span>
              </label>
              <input
                type="text"
                value={globalName}
                onChange={(e) => setGlobalName(e.target.value)}
                placeholder="@YourName"
                className="w-full bg-gray-800 border border-gray-600 rounded px-3 py-2 text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-[var(--color-ring)] text-sm"
              />
            </div>

            {/* Build snapshot */}
            <div className="bg-gray-800 border border-gray-600 rounded p-3 space-y-2">
              <div className="flex items-center justify-between">
                <label className="flex items-center gap-2 cursor-pointer">
                  <input
                    type="checkbox"
                    checked={includeSnapshot}
                    onChange={(e) => setIncludeSnapshot(e.target.checked)}
                    className="w-3.5 h-3.5 rounded border-gray-500 bg-gray-700 text-[var(--color-primary)] focus:ring-[var(--color-ring)] focus:ring-offset-0"
                  />
                  <span className="text-xs font-medium text-gray-300">
                    Include build snapshot
                  </span>
                </label>
                <button
                  type="button"
                  onClick={handleCopySnapshot}
                  className="text-xs text-[var(--color-link)] hover:brightness-110 transition-colors flex items-center gap-1"
                >
                  <svg className="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    {snapshotCopied ? (
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
                    ) : (
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z" />
                    )}
                  </svg>
                  {snapshotCopied ? 'Copied!' : 'Copy snapshot'}
                </button>
              </div>
              <p className="text-xs text-gray-500">
                {includeSnapshot
                  ? 'Your full build (powers, slots, enhancements) will be attached to help reproduce issues.'
                  : 'Of the build, only the basic info below (archetype, powersets) is included.'}
              </p>
              <div className="text-xs text-gray-400 space-y-0.5">
                <p>
                  <span className="text-gray-500">Archetype:</span>{' '}
                  {build.archetype.name || 'None'} (Lv {build.level})
                </p>
                <p>
                  <span className="text-gray-500">Primary:</span>{' '}
                  {build.primary.name || 'None'}
                </p>
                <p>
                  <span className="text-gray-500">Secondary:</span>{' '}
                  {build.secondary.name || 'None'}
                </p>
                {build.pools.length > 0 && (
                  <p>
                    <span className="text-gray-500">Pools:</span>{' '}
                    {build.pools.map((p) => p.name).filter(Boolean).join(', ') || 'None'}
                  </p>
                )}
              </div>
            </div>

            {/* F88. Below the snapshot box and above Send: a reader who has just decided whether
                to attach their build is the reader asking what else goes with it. Folded, with the
                categories in the summary line, so an unexpanded fold still names the account and
                the browser rather than hiding them behind a neutral title. The list is DISCLOSED
                rather than written here -- a second copy is the one that goes stale, and the guard
                grades the table, so a copy would keep passing while the form understated again. */}
            <details className="bg-gray-800/50 border border-gray-700 rounded p-3">
              <summary className="text-xs text-gray-400 cursor-pointer select-none">
                What this report includes &mdash; your account, your build, and app and browser details
              </summary>
              <ul className="mt-2 space-y-1 text-xs text-gray-500 list-disc list-inside">
                {DISCLOSED.map(({ line }) => (
                  <li key={line}>{line}</li>
                ))}
              </ul>
            </details>

            {/* Error message */}
            {status === 'error' && errorMessage && (
              <div className="bg-red-900/20 border border-red-700/50 rounded p-3 text-sm text-red-300">
                {errorMessage}
              </div>
            )}
          </div>
        )}
      </ModalBody>

      {status !== 'success' && (
        <ModalFooter>
          <div className="flex gap-2 justify-end">
            <Button variant="secondary" onClick={handleClose}>
              Cancel
            </Button>
            <Button
              variant="primary"
              onClick={handleSubmit}
              disabled={!canSubmit}
              isLoading={status === 'sending'}
            >
              Send Feedback
            </Button>
          </div>
        </ModalFooter>
      )}
    </Modal>
  );
}
