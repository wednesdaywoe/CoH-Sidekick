/**
 * BuildDetailPage — read-only preview of a shared build
 */

import { useState, useEffect, useRef } from 'react';
import { useParams, useNavigate, useSearch, Link } from '@tanstack/react-router';
import { Button } from '@/components/ui/Button';
import { getSharedBuild, incrementViews, isOwnedBuild, deleteBuild, reclaimBuild, updateBuildVisibility, updateBuildMetadata } from '@/services/sharedBuilds';
import { useBuildStore } from '@/stores/buildStore';
import { useAuthStore } from '@/stores/authStore';
import { getActiveDataset } from '@/data/dataset';
import { buildDocumentTitle, useDocumentTitle, DEFAULT_DOCUMENT_TITLE } from '@/utils/document-title';
import { CURRENT_PREVIEW_TEMPLATE_VERSION } from '@/components/export-image/BuildPreviewCard';
import type { SharedBuild, BuildVisibility } from '@/types/shared';
import { authorIdentity } from '@/utils/author-identity';

/** Hard cap on how long a hidden preview-capture iframe can run before this
 *  page gives up on it — matches SharePreviewCapture's own timeout, plus
 *  headroom for the iframe's own app boot (dataset load, wasm engine).
 *  See streams/BUILD_PREVIEW_BACKFILL_PLAN.md (PREVBF7). */
const PREVIEW_CAPTURE_TIMEOUT_MS = 25000;

function needsPreviewCapture(build: SharedBuild): boolean {
  if (build.visibility === 'private') return false;
  if (!build.preview_image_path) return true;
  const version = build.preview_template_version;
  return version == null || version < CURRENT_PREVIEW_TEMPLATE_VERSION;
}

export function BuildDetailPage() {
  const { id } = useParams({ from: '/builds/$id' });
  const { edit: editParam } = useSearch({ from: '/builds/$id' });
  const navigate = useNavigate();
  const importBuild = useBuildStore((s) => s.importBuild);
  const setVaultId = useBuildStore((s) => s.setVaultId);
  const user = useAuthStore((s) => s.user);

  const [build, setBuild] = useState<SharedBuild | null>(null);
  const [previewCaptureUrl, setPreviewCaptureUrl] = useState<string | null>(null);
  const previewCaptureAttempted = useRef(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [loadConfirm, setLoadConfirm] = useState(false);
  const [deleteConfirm, setDeleteConfirm] = useState(false);
  const [deleteLoading, setDeleteLoading] = useState(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);
  const [showReclaim, setShowReclaim] = useState(false);
  const [reclaimToken, setReclaimToken] = useState('');
  const [reclaimSuccess, setReclaimSuccess] = useState(false);
  const [owned, setOwned] = useState(false);
  const [visibilityLoading, setVisibilityLoading] = useState(false);
  const [visibilityError, setVisibilityError] = useState<string | null>(null);

  // Metadata edit form (owners only)
  const [editing, setEditing] = useState(false);
  const [editName, setEditName] = useState('');
  const [editDescription, setEditDescription] = useState('');
  const [editServer, setEditServer] = useState('');
  const [editTags, setEditTags] = useState('');
  const [editLoading, setEditLoading] = useState(false);
  const [editError, setEditError] = useState<string | null>(null);

  // Name the tab (and so any bookmark of this link) after the shared build.
  useDocumentTitle(
    build
      ? buildDocumentTitle({
          name: build.name,
          archetypeName: build.archetype_name,
          primaryName: build.primary_name,
          secondaryName: build.secondary_name,
        })
      : DEFAULT_DOCUMENT_TITLE,
  );

  useEffect(() => {
    let cancelled = false;

    async function fetch() {
      setLoading(true);
      setError(null);
      try {
        const data = await getSharedBuild(id);
        if (cancelled) return;
        if (!data) {
          setError('Build not found');
        } else {
          setBuild(data);
          setOwned(isOwnedBuild(id, data));
          incrementViews(id);
        }
      } catch (e) {
        if (!cancelled) {
          setError(e instanceof Error ? e.message : 'Failed to load build');
        }
      } finally {
        if (!cancelled) setLoading(false);
      }
    }

    fetch();
    return () => { cancelled = true; };
  }, [id]);

  // Automatic preview-image backfill/refresh (PREVBF7): a hidden capture-mode
  // iframe renders and uploads this build's image if it's missing or stale.
  // `previewCaptureAttempted` guards this to once per pageview — a later
  // `setBuild` (e.g. from the visibility toggle below) must not re-trigger it.
  useEffect(() => {
    if (!build || previewCaptureAttempted.current || !needsPreviewCapture(build)) return;
    previewCaptureAttempted.current = true;
    const serverId = build.build_json.build.serverId ?? 'homecoming';
    setPreviewCaptureUrl(`/?previewCapture=${encodeURIComponent(id)}&serverId=${encodeURIComponent(serverId)}`);
  }, [build, id]);

  useEffect(() => {
    if (!previewCaptureUrl) return;
    const timeout = setTimeout(() => setPreviewCaptureUrl(null), PREVIEW_CAPTURE_TIMEOUT_MS);
    const onMessage = (event: MessageEvent) => {
      if (event.origin !== window.location.origin) return;
      if (event.data?.type === 'coh-sidekick-preview-capture' && event.data?.id === id) {
        clearTimeout(timeout);
        setPreviewCaptureUrl(null);
      }
    };
    window.addEventListener('message', onMessage);
    return () => {
      clearTimeout(timeout);
      window.removeEventListener('message', onMessage);
    };
  }, [previewCaptureUrl, id]);

  const handleCopyLink = async () => {
    try {
      await navigator.clipboard.writeText(window.location.href);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // Clipboard not available
    }
  };

  const handleLoadBuild = () => {
    if (!build) return;
    const success = importBuild(JSON.stringify(build.build_json));
    if (success) {
      // Link the in-memory build to this library entry only when the
      // viewer is the owner — otherwise "Save to Library" would attempt
      // to update someone else's entry (which the backend rejects, but
      // we surface a "Save as new" instead). For non-owners, treat the
      // load as a fresh fork.
      setVaultId(owned ? build.id : null);

      // If the loaded build belongs to a different dataset than the one
      // currently active, a TanStack client-side navigation to `/` keeps
      // the wrong dataset live — every power lookup then reads the wrong
      // server's data. Force a full reload via `?serverId=` so main.tsx
      // boots the matching dataset. localStorage already holds the
      // imported build (Zustand persist wrote it synchronously above), so
      // rehydration on the new page picks it up.
      const loadedServerId = useBuildStore.getState().build.serverId;
      try {
        if (loadedServerId !== getActiveDataset().id) {
          window.location.assign(`/?serverId=${loadedServerId}`);
          return;
        }
      } catch {
        // No active dataset (shouldn't happen post-boot) — fall through.
      }
      navigate({ to: '/' });
    }
  };

  const handleDelete = async () => {
    setDeleteLoading(true);
    setDeleteError(null);
    try {
      await deleteBuild(id);
      navigate({ to: '/builds' });
    } catch (e) {
      setDeleteError(e instanceof Error ? e.message : 'Failed to delete build');
    } finally {
      setDeleteLoading(false);
    }
  };

  const handleChangeVisibility = async (visibility: BuildVisibility) => {
    if (!build || visibilityLoading || visibility === build.visibility) return;
    setVisibilityLoading(true);
    setVisibilityError(null);
    try {
      await updateBuildVisibility(id, visibility);
      setBuild({ ...build, visibility });
    } catch (e) {
      setVisibilityError(e instanceof Error ? e.message : 'Failed to update visibility');
    } finally {
      setVisibilityLoading(false);
    }
  };

  const enterEdit = (b: SharedBuild) => {
    setEditName(b.name);
    setEditDescription(b.description ?? '');
    setEditServer(b.server ?? '');
    setEditTags((b.tags ?? []).join(', '));
    setEditError(null);
    setEditing(true);
  };

  const closeEdit = () => {
    setEditing(false);
    setEditError(null);
    // Drop the ?edit deep-link so a refresh doesn't reopen the form.
    if (editParam) navigate({ to: '/builds/$id', params: { id }, search: {}, replace: true });
  };

  const handleSaveEdit = async () => {
    if (!build) return;
    setEditLoading(true);
    setEditError(null);
    try {
      const tags = editTags.split(',').map((t) => t.trim()).filter(Boolean);
      const name = editName.trim() || build.name;
      await updateBuildMetadata(build, { name, description: editDescription, server: editServer, tags });
      setBuild({ ...build, name, description: editDescription, server: editServer, tags });
      closeEdit();
    } catch (e) {
      setEditError(e instanceof Error ? e.message : 'Failed to save changes');
    } finally {
      setEditLoading(false);
    }
  };

  // Deep-link (?edit=1) from a My Builds card auto-opens the edit form once the
  // build has loaded and the viewer is confirmed as an owner.
  useEffect(() => {
    if (editParam && owned && build && !editing) {
      enterEdit(build);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [editParam, owned, build]);

  if (loading) {
    return (
      <div className="max-w-4xl mx-auto p-6 text-center py-12">
        <p className="text-gray-400">Loading build...</p>
      </div>
    );
  }

  if (error || !build) {
    return (
      <div className="max-w-4xl mx-auto p-6 text-center py-12">
        <h1 className="text-2xl font-bold text-white mb-4">Build Not Found</h1>
        <p className="text-gray-400 mb-6">{error || 'This build does not exist or has been removed.'}</p>
        <Button variant="secondary" onClick={() => navigate({ to: '/builds' })}>
          Browse Builds
        </Button>
      </div>
    );
  }

  const buildData = build.build_json.build;
  const pools = buildData.pools ?? [];
  const epicPool = buildData.epicPool;
  // See `utils/author-identity.ts`: the `@` comes from the proved handle and
  // never from the free-text name (SECURITY_AUDIT.md F69).
  const author = authorIdentity(build.author_name, build.author_handle);

  return (
    <div className="max-w-4xl mx-auto p-6">
      {/* Back link */}
      <button
        onClick={() => navigate({ to: '/builds' })}
        className="text-sm text-gray-400 hover:text-gray-300 mb-4 inline-block transition-colors"
      >
        &larr; Back to Builds
      </button>

      {/* Header */}
      <div className="bg-gray-800 border border-gray-700 rounded-lg p-6 mb-6">
        <div className="flex items-start justify-between gap-4 mb-4">
          <div>
            <h1 className="text-2xl font-bold text-white mb-1">{build.name}</h1>
            <p className="text-blue-400 font-medium">
              {build.archetype_name} — Level {build.level}
            </p>
          </div>
          <div className="flex gap-2 shrink-0 flex-wrap justify-end">
            <Button
              variant={copied ? 'secondary' : 'ghost'}
              size="sm"
              onClick={handleCopyLink}
            >
              {copied ? 'Copied!' : 'Copy Link'}
            </Button>
            {/* Edit metadata — any owner (token or account) */}
            {owned && !editing && (
              <Button
                variant="ghost"
                size="sm"
                onClick={() => enterEdit(build)}
                className="text-gray-400 hover:text-gray-200"
              >
                Edit
              </Button>
            )}
            {/* Visibility control — only for Discord-linked owners */}
            {owned && user && build?.user_id === user.id && (
              <select
                value={build.visibility}
                disabled={visibilityLoading}
                onChange={(e) => handleChangeVisibility(e.target.value as BuildVisibility)}
                className="px-2 py-1.5 text-sm rounded bg-gray-700 border border-gray-600 text-gray-200 hover:border-gray-500 disabled:opacity-50 focus:outline-none focus:ring-1 focus:ring-indigo-500"
                title="Build visibility"
              >
                <option value="public">Public</option>
                <option value="unlisted">Unlisted</option>
                <option value="private">Private</option>
              </select>
            )}
            {owned ? (
              <Button
                variant="ghost"
                size="sm"
                onClick={() => setDeleteConfirm(true)}
                className="text-red-400 hover:text-red-300"
              >
                Delete
              </Button>
            ) : (
              <Button
                variant="ghost"
                size="sm"
                onClick={() => setShowReclaim(!showReclaim)}
                className="text-gray-400 hover:text-gray-300"
              >
                Reclaim
              </Button>
            )}
          </div>
        </div>

        {/* Powersets */}
        <div className="grid grid-cols-2 gap-4 text-sm mb-4">
          <div>
            <span className="text-gray-500">Primary:</span>{' '}
            <span className="text-white">{build.primary_name}</span>
          </div>
          <div>
            <span className="text-gray-500">Secondary:</span>{' '}
            <span className="text-white">{build.secondary_name}</span>
          </div>
        </div>

        {/* Meta */}
        <div className="flex flex-wrap gap-4 text-xs text-gray-500">
          {author.kind !== 'anonymous' && (
            <span>
              By{' '}
              {author.kind === 'verified' ? (
                <Link
                  to="/author/$handle"
                  params={{ handle: author.handle }}
                  className="text-gray-300 hover:text-blue-400 transition-colors"
                >
                  {author.display}
                  {author.display ? ' ' : ''}
                  <span className="font-semibold" title="A handle this account holds">
                    @{author.handle}
                  </span>
                </Link>
              ) : (
                <span className="text-gray-400">{author.display}</span>
              )}
            </span>
          )}
          {build.server && <span>{build.server}</span>}
          <span>{new Date(build.created_at).toLocaleDateString()}</span>
          <span>{build.views} views</span>
        </div>
      </div>

      {/* Edit metadata form (owners) */}
      {editing && (
        <div className="bg-gray-800 border border-gray-700 rounded-lg p-4 mb-6 space-y-3">
          <div className="flex items-center justify-between">
            <h2 className="text-sm font-semibold text-gray-300">Edit Details</h2>
            <span className="text-xs text-gray-500">The build's powers aren't changed — only its details.</span>
          </div>
          <div>
            <label className="block text-xs font-medium text-gray-400 mb-1">Name</label>
            <input
              type="text"
              value={editName}
              onChange={(e) => setEditName(e.target.value)}
              placeholder="Build name"
              className="w-full bg-gray-700 border border-gray-600 rounded px-3 py-2 text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-[var(--color-ring)] text-sm"
              maxLength={200}
            />
          </div>
          <div>
            <label className="block text-xs font-medium text-gray-400 mb-1">Description</label>
            <textarea
              value={editDescription}
              onChange={(e) => setEditDescription(e.target.value)}
              placeholder="What it's for, how to play it, etc."
              className="w-full h-24 bg-gray-700 border border-gray-600 rounded px-3 py-2 text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-[var(--color-ring)] text-sm"
              maxLength={500}
            />
            <p className="text-xs text-gray-500 mt-1">{editDescription.length}/500</p>
          </div>
          <div>
            <label className="block text-xs font-medium text-gray-400 mb-1">Server</label>
            <input
              type="text"
              value={editServer}
              onChange={(e) => setEditServer(e.target.value)}
              placeholder="e.g., Homecoming"
              className="w-full bg-gray-700 border border-gray-600 rounded px-3 py-2 text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-[var(--color-ring)] text-sm"
              maxLength={50}
            />
          </div>
          <div>
            <label className="block text-xs font-medium text-gray-400 mb-1">
              Tags <span className="text-gray-500">(comma-separated)</span>
            </label>
            <input
              type="text"
              value={editTags}
              onChange={(e) => setEditTags(e.target.value)}
              placeholder="e.g., PvP, farming, budget, softcap"
              className="w-full bg-gray-700 border border-gray-600 rounded px-3 py-2 text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-[var(--color-ring)] text-sm"
              maxLength={200}
            />
          </div>
          {editError && (
            <p className="text-xs text-red-300">{editError}</p>
          )}
          <div className="flex gap-2 justify-end">
            <Button variant="secondary" size="sm" onClick={closeEdit} disabled={editLoading}>
              Cancel
            </Button>
            <Button variant="primary" size="sm" onClick={handleSaveEdit} isLoading={editLoading}>
              Save Changes
            </Button>
          </div>
        </div>
      )}

      {/* Private/unlisted build badge */}
      {build && build.visibility === 'private' && (
        <div className="bg-indigo-900/20 border border-indigo-700/50 rounded-lg px-4 py-2.5 mb-6 flex items-center gap-2 text-sm text-indigo-300">
          <svg className="w-4 h-4 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
          </svg>
          This build is private. Only you can see it.
        </div>
      )}
      {build && build.visibility === 'unlisted' && (
        <div className="bg-amber-900/20 border border-amber-700/50 rounded-lg px-4 py-2.5 mb-6 flex items-center gap-2 text-sm text-amber-300">
          <svg className="w-4 h-4 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13.828 10.172a4 4 0 010 5.656l-3 3a4 4 0 01-5.656-5.656l1.5-1.5M10.172 13.828a4 4 0 010-5.656l3-3a4 4 0 015.656 5.656l-1.5 1.5" />
          </svg>
          This build is unlisted. Anyone with this link can see it, but it won't appear in search.
        </div>
      )}

      {/* Visibility error */}
      {visibilityError && (
        <div className="bg-red-900/20 border border-red-700/50 rounded-lg p-3 mb-6 text-sm text-red-300">
          {visibilityError}
        </div>
      )}

      {/* Delete confirmation */}
      {deleteConfirm && (
        <div className="bg-red-900/20 border border-red-700/50 rounded-lg p-4 mb-6 space-y-3">
          <p className="text-sm text-red-300 font-semibold">Are you sure you want to delete this build?</p>
          <p className="text-xs text-red-400">This action cannot be undone. The build will be permanently removed.</p>
          {deleteError && (
            <p className="text-xs text-red-300">{deleteError}</p>
          )}
          <div className="flex gap-2 justify-end">
            <Button variant="secondary" size="sm" onClick={() => setDeleteConfirm(false)} disabled={deleteLoading}>
              Cancel
            </Button>
            <Button
              variant="primary"
              size="sm"
              onClick={handleDelete}
              isLoading={deleteLoading}
              className="bg-red-600 hover:bg-red-700"
            >
              Delete Permanently
            </Button>
          </div>
        </div>
      )}

      {/* Reclaim ownership */}
      {showReclaim && (
        <div className="bg-gray-800 border border-gray-700 rounded-lg p-4 mb-6 space-y-3">
          <p className="text-sm text-gray-300 font-semibold">Reclaim Build</p>
          <p className="text-xs text-gray-400">
            Enter the owner token you received when you originally shared this build.
          </p>
          <div className="flex gap-2">
            <input
              type="text"
              value={reclaimToken}
              onChange={(e) => setReclaimToken(e.target.value)}
              placeholder="Paste owner token..."
              className="flex-1 bg-gray-700 border border-gray-600 rounded px-3 py-1.5 text-white text-sm font-mono placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-[var(--color-ring)]"
            />
            <Button
              variant="primary"
              size="sm"
              disabled={!reclaimToken.trim()}
              onClick={() => {
                reclaimBuild(id, reclaimToken.trim());
                setOwned(true);
                setShowReclaim(false);
                setReclaimToken('');
                setReclaimSuccess(true);
                setTimeout(() => setReclaimSuccess(false), 3000);
              }}
            >
              Reclaim
            </Button>
          </div>
          <p className="text-xs text-gray-500">
            If the token is incorrect, future update or delete attempts will fail.
          </p>
        </div>
      )}

      {reclaimSuccess && (
        <div className="bg-green-900/20 border border-green-700/50 rounded-lg p-3 mb-6 text-sm text-green-300">
          Ownership reclaimed. You can now update or delete this build.
        </div>
      )}

      {/* Description */}
      {build.description && (
        <div className="bg-gray-800 border border-gray-700 rounded-lg p-4 mb-6">
          <h2 className="text-sm font-semibold text-gray-300 mb-2">Description</h2>
          <p className="text-sm text-gray-400 whitespace-pre-wrap">{build.description}</p>
        </div>
      )}

      {/* Tags */}
      {build.tags.length > 0 && (
        <div className="flex flex-wrap gap-2 mb-6">
          {build.tags.map((tag) => (
            <span
              key={tag}
              className="px-2 py-1 bg-gray-800 border border-gray-700 text-gray-400 rounded text-xs"
            >
              {tag}
            </span>
          ))}
        </div>
      )}

      {/* Power Summary */}
      <div className="bg-gray-800 border border-gray-700 rounded-lg p-4 mb-6">
        <h2 className="text-sm font-semibold text-gray-300 mb-3">Powers</h2>
        <div className="space-y-4">
          {/* Primary */}
          <PowersetSummary label="Primary" name={build.primary_name} powers={buildData.primary.powers} />

          {/* Secondary */}
          <PowersetSummary label="Secondary" name={build.secondary_name} powers={buildData.secondary.powers} />

          {/* Pools */}
          {pools.map((pool) => (
            <PowersetSummary key={pool.id} label="Pool" name={pool.name} powers={pool.powers} />
          ))}

          {/* Epic */}
          {epicPool && (
            <PowersetSummary label="Epic" name={epicPool.name} powers={epicPool.powers} />
          )}
        </div>
      </div>

      {/* Load into Planner */}
      <div className="bg-gray-800 border border-gray-700 rounded-lg p-4 mb-20">
        {!loadConfirm ? (
          <div className="flex items-center justify-between">
            <p className="text-sm text-gray-400">Want to use this build?</p>
            <Button variant="primary" onClick={() => setLoadConfirm(true)}>
              Load into Planner
            </Button>
          </div>
        ) : (
          <div className="space-y-3">
            <div className="bg-yellow-900/20 border border-yellow-700/50 rounded p-3 text-sm text-yellow-300">
              <p className="font-semibold mb-1">Warning:</p>
              <p>This will replace your current build. Make sure to export your current build first if you want to keep it.</p>
            </div>
            <div className="flex gap-2 justify-end">
              <Button variant="secondary" onClick={() => setLoadConfirm(false)}>
                Cancel
              </Button>
              <Button variant="primary" onClick={handleLoadBuild}>
                Confirm & Load
              </Button>
            </div>
          </div>
        )}
      </div>

      {/* Hidden preview-image backfill/refresh capture (PREVBF7) — see
          streams/BUILD_PREVIEW_BACKFILL_PLAN.md. Never visible; removed on
          completion or PREVIEW_CAPTURE_TIMEOUT_MS. */}
      {previewCaptureUrl && (
        <iframe
          src={previewCaptureUrl}
          aria-hidden
          title="preview capture"
          // Wide/tall enough for the 1200x630 card to actually lay out and
          // render inside the iframe's own viewport — a 1x1 iframe produced
          // no rendered output at all (found live, not theoretical).
          style={{ position: 'fixed', left: -100000, top: 0, width: 1300, height: 950, border: 0, opacity: 0 }}
        />
      )}
    </div>
  );
}

/** Simple power list for a powerset */
function PowersetSummary({
  label,
  name,
  powers,
}: {
  label: string;
  name: string;
  powers: { name: string; level: number; slots: unknown[] }[];
}) {
  if (powers.length === 0) return null;

  return (
    <div>
      <p className="text-xs text-gray-500 mb-1">
        {label}: <span className="text-gray-300">{name}</span>
      </p>
      <div className="flex flex-wrap gap-1.5">
        {powers.map((power) => (
          <span
            key={power.name}
            className="px-2 py-0.5 bg-gray-700 text-gray-300 rounded text-xs"
            title={`Level ${power.level} · ${power.slots.length} slot${power.slots.length !== 1 ? 's' : ''}`}
          >
            {power.name}
          </span>
        ))}
      </div>
    </div>
  );
}
