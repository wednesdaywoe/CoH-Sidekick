/**
 * BuildCard — compact card for displaying a shared build in search results
 */

import { useState } from 'react';
import { useNavigate } from '@tanstack/react-router';
import { isFavorite, toggleFavorite, isOwnedBuild, deleteBuild } from '@/services/sharedBuilds';
import type { SharedBuild, BuildVisibility } from '@/types/shared';
import { authorIdentity } from '@/utils/author-identity';

/** Click cycles forward through all three states. */
const NEXT_VISIBILITY: Record<BuildVisibility, BuildVisibility> = {
  public: 'unlisted',
  unlisted: 'private',
  private: 'public',
};

const VISIBILITY_BADGE: Record<Exclude<BuildVisibility, 'public'>, string> = {
  private: 'Private',
  unlisted: 'Unlisted',
};

interface BuildCardProps {
  build: SharedBuild;
  /** Show inline delete button */
  showDelete?: boolean;
  /** Callback after successful delete */
  onDeleted?: (id: string) => void;
  /** Callback when author name is clicked */
  onAuthorClick?: (authorId: string | null, authorName: string) => void;
  /** Show visibility control (cycles private/unlisted/public) for the My Builds tab */
  onVisibilityToggle?: (id: string, visibility: BuildVisibility) => Promise<void>;
}

export function BuildCard({ build, showDelete, onDeleted, onAuthorClick, onVisibilityToggle }: BuildCardProps) {
  const navigate = useNavigate();
  const [favorited, setFavorited] = useState(() => isFavorite(build.id));
  const [deleteConfirm, setDeleteConfirm] = useState(false);
  const [deleteLoading, setDeleteLoading] = useState(false);
  const [visibilityLoading, setVisibilityLoading] = useState(false);
  const [visibilityError, setVisibilityError] = useState<string | null>(null);

  const timeAgo = getTimeAgo(build.created_at);
  const owned = showDelete && isOwnedBuild(build.id, build);

  const handleFavoriteClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    const nowFav = toggleFavorite(build.id);
    setFavorited(nowFav);
  };

  const handleDelete = async (e: React.MouseEvent) => {
    e.stopPropagation();
    setDeleteLoading(true);
    try {
      await deleteBuild(build.id);
      onDeleted?.(build.id);
    } catch {
      // Fall back to detail page if delete fails
      setDeleteConfirm(false);
    } finally {
      setDeleteLoading(false);
    }
  };

  // The author line, decided once. `author_name` is free text a sharer typed;
  // the handle is the profile join and the only part of this a client can
  // prove, so the `@` is written from the handle and never from the name —
  // SECURITY_AUDIT.md F69, and `utils/author-identity.ts` carries the why.
  const author = authorIdentity(build.author_name, build.author_handle);

  const handleAuthorClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    // Verified author with a handle → public author page
    if (author.kind === 'verified') {
      navigate({ to: '/author/$handle', params: { handle: author.handle } });
      return;
    }
    // Otherwise fall back to filtering the build list by author. The stripped
    // display name is what goes to the filter, not the raw column: a filter
    // that searched for `@savant` would search for a string this card is
    // deliberately not showing.
    if (onAuthorClick && author.kind === 'unverified') {
      onAuthorClick(build.user_id ?? null, author.display);
    }
  };

  // The control reflects `build.visibility` from the list, not a local copy —
  // the caller owns the optimistic flip and the revert. Keeping a second copy
  // here meant any remount (a refetch flipping the page to its loading state,
  // say) silently reset the icon to whatever the list still held.
  const handleVisibilityToggle = async (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!onVisibilityToggle || visibilityLoading) return;
    setVisibilityLoading(true);
    setVisibilityError(null);
    try {
      await onVisibilityToggle(build.id, NEXT_VISIBILITY[build.visibility]);
    } catch (err) {
      // updateBuildVisibility unwraps the edge function's error body into a real
      // message ('Build not found or not authorized', 'Session expired', ...).
      // Swallowing it made a 403 and a successful write look identical.
      setVisibilityError(err instanceof Error ? err.message : 'Failed to update visibility');
    } finally {
      setVisibilityLoading(false);
    }
  };

  return (
    <div className="relative">
      <button
        type="button"
        onClick={() => navigate({ to: '/builds/$id', params: { id: build.id } })}
        className="w-full text-left bg-gray-800 border border-gray-700 rounded-lg p-4 hover:border-gray-500 hover:bg-gray-750 transition-colors cursor-pointer"
      >
        {/* Header */}
        <div className="flex items-start justify-between gap-2 mb-2">
          <div className="flex items-center gap-1.5 min-w-0">
            <h3 className="font-semibold text-white text-sm truncate">{build.name}</h3>
            {build.visibility !== 'public' && (
              <span
                className={`shrink-0 px-1 py-0.5 rounded text-[10px] font-medium border ${
                  build.visibility === 'private'
                    ? 'bg-indigo-900/60 border-indigo-700/50 text-indigo-300'
                    : 'bg-amber-900/60 border-amber-700/50 text-amber-300'
                }`}
              >
                {VISIBILITY_BADGE[build.visibility]}
              </span>
            )}
          </div>
          <div className="flex items-center gap-1 shrink-0">
            <span className="text-xs text-gray-500">Lv {build.level}</span>
            {/* Favorite star */}
            <span
              role="button"
              onClick={handleFavoriteClick}
              className={`text-sm transition-colors ${favorited ? 'text-yellow-400' : 'text-gray-600 hover:text-yellow-400/60'}`}
              title={favorited ? 'Remove from favorites' : 'Add to favorites'}
            >
              {favorited ? '\u2605' : '\u2606'}
            </span>
            {/* Visibility control (My Builds tab) — click cycles public → unlisted → private */}
            {onVisibilityToggle && !deleteConfirm && (
              <span
                role="button"
                onClick={handleVisibilityToggle}
                className={`p-0.5 transition-colors rounded ${visibilityLoading ? 'opacity-50' : ''} ${
                  build.visibility === 'public'
                    ? 'text-gray-500 hover:text-indigo-400'
                    : build.visibility === 'unlisted'
                      ? 'text-amber-400 hover:text-amber-300'
                      : 'text-indigo-400 hover:text-indigo-300'
                }`}
                title={`Currently ${build.visibility}. Click to make ${NEXT_VISIBILITY[build.visibility]}.`}
              >
                {build.visibility === 'public' && (
                  <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
                  </svg>
                )}
                {build.visibility === 'unlisted' && (
                  <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13.828 10.172a4 4 0 010 5.656l-3 3a4 4 0 01-5.656-5.656l1.5-1.5M10.172 13.828a4 4 0 010-5.656l3-3a4 4 0 015.656 5.656l-1.5 1.5" />
                  </svg>
                )}
                {build.visibility === 'private' && (
                  <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 11V7a4 4 0 118 0m-4 8v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2z" />
                  </svg>
                )}
              </span>
            )}
            {/* Edit details (My Builds tab) — deep-links into the edit form */}
            {owned && !deleteConfirm && (
              <span
                role="button"
                onClick={(e) => {
                  e.stopPropagation();
                  navigate({ to: '/builds/$id', params: { id: build.id }, search: { edit: true } });
                }}
                className="p-0.5 text-gray-600 hover:text-gray-300 transition-colors rounded"
                title="Edit details"
              >
                <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
                </svg>
              </span>
            )}
            {/* Delete (My Builds tab) */}
            {owned && !deleteConfirm && (
              <span
                role="button"
                onClick={(e) => { e.stopPropagation(); setDeleteConfirm(true); }}
                className="p-0.5 text-gray-600 hover:text-red-400 transition-colors rounded"
                title="Delete build"
              >
                <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                </svg>
              </span>
            )}
          </div>
        </div>

        {/* Visibility toggle failure — the icon has already reverted by now */}
        {visibilityError && (
          <p
            role="alert"
            onClick={(e) => e.stopPropagation()}
            className="mb-2 px-2 py-1 bg-red-900/25 border border-red-700/40 rounded text-[11px] text-red-300"
          >
            {visibilityError}
          </p>
        )}

        {/* Archetype + Powersets */}
        <div className="text-xs text-gray-400 space-y-0.5 mb-3">
          <p className="text-blue-400 font-medium">{build.archetype_name}</p>
          <p>{build.primary_name} / {build.secondary_name}</p>
        </div>

        {/* Description preview */}
        {build.description && (
          <p className="text-xs text-gray-500 mb-3 line-clamp-2">{build.description}</p>
        )}

        {/* Tags */}
        {build.tags.length > 0 && (
          <div className="flex flex-wrap gap-1 mb-3">
            {build.tags.slice(0, 4).map((tag) => (
              <span
                key={tag}
                className="px-1.5 py-0.5 bg-gray-700 text-gray-400 rounded text-[10px]"
              >
                {tag}
              </span>
            ))}
            {build.tags.length > 4 && (
              <span className="text-[10px] text-gray-500">+{build.tags.length - 4}</span>
            )}
          </div>
        )}

        {/* Footer */}
        <div className="flex items-center justify-between text-[10px] text-gray-500">
          <span>
            {author.kind === 'anonymous' ? (
              'Anonymous'
            ) : (
              <span
                role="button"
                onClick={handleAuthorClick}
                className={
                  author.kind === 'verified' || onAuthorClick
                    ? 'hover:text-blue-400 transition-colors'
                    : ''
                }
              >
                {author.display}
                {author.kind === 'verified' && (
                  <span
                    className={author.display ? 'ml-1 text-gray-300 font-semibold' : 'text-gray-300 font-semibold'}
                    title="A handle this account holds"
                  >
                    @{author.handle}
                  </span>
                )}
              </span>
            )}
            {build.server ? ` · ${build.server}` : ''}
          </span>
          <span className="flex items-center gap-2">
            <span>{build.views} views</span>
            <span>{timeAgo}</span>
          </span>
        </div>
      </button>

      {/* Delete confirmation overlay */}
      {deleteConfirm && (
        <div
          className="absolute inset-0 bg-gray-900/95 border border-red-700/50 rounded-lg flex flex-col items-center justify-center gap-3 p-4 z-10"
          onClick={(e) => e.stopPropagation()}
        >
          <p className="text-sm text-red-300 font-medium text-center">Delete this build?</p>
          <div className="flex gap-2">
            <button
              onClick={(e) => { e.stopPropagation(); setDeleteConfirm(false); }}
              disabled={deleteLoading}
              className="px-3 py-1.5 text-xs bg-gray-700 text-gray-300 rounded hover:bg-gray-600 transition-colors disabled:opacity-50"
            >
              Cancel
            </button>
            <button
              onClick={handleDelete}
              disabled={deleteLoading}
              className="px-3 py-1.5 text-xs bg-red-600 text-on-danger rounded hover:bg-red-700 transition-colors disabled:opacity-50"
            >
              {deleteLoading ? 'Deleting...' : 'Delete'}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

function getTimeAgo(dateStr: string): string {
  const date = new Date(dateStr);
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffMins = Math.floor(diffMs / 60000);
  const diffHours = Math.floor(diffMins / 60);
  const diffDays = Math.floor(diffHours / 24);

  if (diffMins < 1) return 'just now';
  if (diffMins < 60) return `${diffMins}m ago`;
  if (diffHours < 24) return `${diffHours}h ago`;
  if (diffDays < 30) return `${diffDays}d ago`;
  return date.toLocaleDateString();
}
