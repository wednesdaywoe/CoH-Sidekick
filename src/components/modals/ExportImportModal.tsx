/**
 * ExportImportModal component - handles build save/load, import, and sharing
 */

import { useState, useRef, useEffect } from 'react';
import { Modal, ModalBody, ModalFooter, ModalHeader } from './Modal';
import { ForumExportModal } from './ForumExportModal';
import { Button } from '../ui/Button';
import { useBuildStore, useUIStore, useAuthStore } from '@/stores';
import type { ArchetypeBranchId } from '@/types';
import { importMidsBuild } from '@/utils/mids-import';
import type { MidsImportResult } from '@/utils/mids-import';
import { parseMxdText, importMxdBuild } from '@/utils/mxd-import';
import { importGameExport } from '@/utils/game-importer';
import type { GameImportResult } from '@/utils/game-importer';
import { shareBuild, getSharedBuild, getOwnedBuildIds, getOwnerToken, getMyBuilds, RateLimitError, formatRateLimitMessage, rateLimitHint } from '@/services/sharedBuilds';
import type { BuildExport } from '@/types/build';
import type { SharedBuild } from '@/types/shared';
import { getActiveDataset, getAllDatasetMetadata, type DatasetId } from '@/data/dataset';
import type { HydrationNote } from '@/utils/build-serialization';
import { crossDatasetChoice } from '@/utils/build-open-route';
import { generatePopmenu } from '@/utils/export-popmenu';
import { openPrintView } from '@/utils/export-print';
import { exportToMidsWithReport, type MidsExportWarning } from '@/utils/mids-export';
import { findIllegalSlots } from '@/utils/build-enhancement-validation';

interface ExportImportModalProps {
  isOpen: boolean;
  onClose: () => void;
}

type TabType = 'save' | 'load-import' | 'share-export';
type LoadSource = 'local' | 'mids' | 'game';
type ShareExportSubTab = 'share' | 'export';

/** Human-readable label for a build's dataset, used to seed the free-text
 *  `server` field on a fresh library save (e.g. 'homecoming' → 'Homecoming'). */
function serverLabel(serverId: string | undefined): string {
  if (!serverId) return '';
  const meta = getAllDatasetMetadata().find((d) => d.id === serverId);
  return meta?.displayName ?? serverId.charAt(0).toUpperCase() + serverId.slice(1);
}

export function ExportImportModal({ isOpen, onClose }: ExportImportModalProps) {
  const [activeTab, setActiveTab] = useState<TabType>('save');
  // Enhancements Mids has no name for. Mids drops an unresolvable slot without
  // saying so, so the only place the user can learn the file is short is here.
  const [midsExportWarnings, setMidsExportWarnings] = useState<MidsExportWarning[]>([]);
  const requestedTab = useUIStore((s) => s.exportImportModalTab);
  const levelUpMode = useUIStore((s) => s.levelUpMode);

  // Sync tab when modal opens with a specific tab requested
  useEffect(() => {
    if (isOpen && requestedTab) {
      setActiveTab(requestedTab);
    }
  }, [isOpen, requestedTab]);

  // Save state
  const [buildAlias, setBuildAlias] = useState('');

  // Load local state
  const [importText, setImportText] = useState('');
  const [importError, setImportError] = useState<string | null>(null);
  const [importSuccess, setImportSuccess] = useState(false);
  // A file from another server, held until the user says which of the two opens they meant.
  // Held as the raw text: the port has to hydrate the ORIGINAL payload, not anything this
  // dataset has already resolved it into.
  const [crossDataset, setCrossDataset] = useState<{ text: string; from: DatasetId } | null>(null);
  const [portNotes, setPortNotes] = useState<HydrationNote[] | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);

  // Forum-export modal state — opens a sibling dialog rather than
  // expanding inline, since the preview needs more room than the share
  // tab can comfortably give it.
  const [showForumExport, setShowForumExport] = useState(false);

  // Popmenu state
  const [showPopmenu, setShowPopmenu] = useState(false);
  const [popmenuName, setPopmenuName] = useState('');
  const [popmenuStatus, setPopmenuStatus] = useState<string | null>(null);
  const [popmenuError, setPopmenuError] = useState<string | null>(null);

  // Load/Import source toggle
  const [loadSource, setLoadSource] = useState<LoadSource>('local');

  // Mids import state
  const [midsText, setMidsText] = useState('');
  const [midsResult, setMidsResult] = useState<MidsImportResult | null>(null);
  const [midsError, setMidsError] = useState<string | null>(null);
  const [showWarnings, setShowWarnings] = useState(false);
  const midsFileInputRef = useRef<HTMLInputElement>(null);

  // Game import state
  const [gameText, setGameText] = useState('');
  const [gameResult, setGameResult] = useState<GameImportResult | null>(null);
  const [gameError, setGameError] = useState<string | null>(null);
  const [showGameWarnings, setShowGameWarnings] = useState(false);
  const gameFileInputRef = useRef<HTMLInputElement>(null);

  // Share/Export sub-tab
  const [shareExportSubTab, setShareExportSubTab] = useState<ShareExportSubTab>('share');

  // Share state
  const [shareDescription, setShareDescription] = useState('');
  const [shareAuthor, setShareAuthor] = useState('');
  const [shareServer, setShareServer] = useState('');
  const [shareTags, setShareTags] = useState('');
  const [shareLoading, setShareLoading] = useState(false);
  const [shareError, setShareError] = useState<string | null>(null);
  const [shareUrl, setShareUrl] = useState<string | null>(null);
  const [shareCopied, setShareCopied] = useState(false);
  const [shareUpdated, setShareUpdated] = useState(false);
  const [updateExistingId, setUpdateExistingId] = useState<string | null>(null);
  const [sharedBuildId, setSharedBuildId] = useState<string | null>(null);
  const [showOwnerToken, setShowOwnerToken] = useState(false);
  const [tokenCopied, setTokenCopied] = useState(false);
  const [accountBuilds, setAccountBuilds] = useState<SharedBuild[]>([]);

  // Vault save state
  const [vaultSaveSuccess, setVaultSaveSuccess] = useState(false);
  const [vaultSaveError, setVaultSaveError] = useState<string | null>(null);
  const [vaultSaveLoading, setVaultSaveLoading] = useState(false);

  // Vault (private library) metadata. Description/server/tags are surfaced in an
  // optional details panel; author is preserved silently on update (the edge
  // function overwrites all metadata columns wholesale, so an un-sent field
  // would be blanked). Server seeds from the build's dataset on a fresh save.
  const [vaultDescription, setVaultDescription] = useState('');
  const [vaultServer, setVaultServer] = useState('');
  const [vaultTags, setVaultTags] = useState('');
  const [vaultAuthor, setVaultAuthor] = useState('');
  const [vaultDetailsOpen, setVaultDetailsOpen] = useState(false);

  // Last-known remaining quota for this hour (from the server, after a save).
  const [vaultRemaining, setVaultRemaining] = useState<number | null>(null);
  const [shareRemaining, setShareRemaining] = useState<number | null>(null);

  const { exportBuild, importBuild, importMidsBuild: applyMidsBuild, build } = useBuildStore();
  const setVaultId = useBuildStore((s) => s.setVaultId);
  const user = useAuthStore((s) => s.user);

  // Fetch account-owned builds when modal opens and user is logged in
  useEffect(() => {
    if (isOpen && user) {
      getMyBuilds().then(setAccountBuilds).catch(() => {});
    }
  }, [isOpen, user]);

  // Whether the in-memory build is linked to a library entry the user can
  // update. Requires both a vaultId on the build (set when loaded from
  // BuildDetailPage as the owner) AND the owner token still being present
  // — without the token the backend rejects the update. Stale links (the
  // entry was deleted, or the user cleared their tokens) fall through to
  // the new-entry path.
  const linkedVaultId = build.vaultId ?? null;
  const canUpdateVault = !!(linkedVaultId && (getOwnerToken(linkedVaultId) || user));

  // Pre-populate the alias input with the build's name when the modal
  // opens against a library-linked build, so users can see (and adjust)
  // the name that will overwrite their saved entry.
  useEffect(() => {
    if (isOpen && canUpdateVault && !buildAlias.trim() && build.name) {
      setBuildAlias(build.name);
    }
    // Intentionally only depend on isOpen — we don't want to re-prefill
    // every time the user types or the build state changes after open.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isOpen]);

  // Seed the vault metadata fields when the modal opens. If the build is linked
  // to an existing library entry, prefill from that row so a re-save doesn't
  // wipe the saved description/server/tags/author (the edge function overwrites
  // all metadata columns wholesale on update) — and auto-open the details panel
  // when there's existing metadata to show. For a brand-new save, seed the
  // free-text server from the build's dataset. Keyed on the vault link only so a
  // late account-list load doesn't clobber edits the user made after opening.
  useEffect(() => {
    if (!isOpen) return;
    if (canUpdateVault && linkedVaultId) {
      let cancelled = false;
      const apply = (b: SharedBuild) => {
        if (cancelled) return;
        setVaultDescription(b.description ?? '');
        setVaultServer(b.server ?? '');
        setVaultTags((b.tags ?? []).join(', '));
        setVaultAuthor(b.author_name ?? '');
        if ((b.description ?? '') || (b.server ?? '') || (b.tags ?? []).length) {
          setVaultDetailsOpen(true);
        }
      };
      const known = accountBuilds.find((b) => b.id === linkedVaultId);
      if (known) apply(known);
      else getSharedBuild(linkedVaultId).then((b) => { if (b) apply(b); }).catch(() => {});
      return () => { cancelled = true; };
    }
    // Fresh entry — default the server label from the build's dataset, clear the rest.
    setVaultDescription('');
    setVaultServer(serverLabel(build.serverId));
    setVaultTags('');
    setVaultAuthor('');
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isOpen, linkedVaultId]);

  // Merge token-owned IDs with account-owned IDs (deduplicated)
  const tokenIds = getOwnedBuildIds();
  const accountIds = accountBuilds.map((b) => b.id);
  const allOwnedIds = [...new Set([...tokenIds, ...accountIds])];
  const accountBuildMap = new Map(accountBuilds.map((b) => [b.id, b]));

  // When the user selects an existing build to update, pre-fill the metadata
  // fields with that build's current details. The public share form overwrites
  // ALL metadata columns wholesale on update (server-side `updateFields`), so
  // without this a blank field would silently wipe the saved description/
  // author/server/tags — forcing the user to retype them every time. Account-
  // owned builds are already loaded (instant); token-only anonymous builds are
  // fetched by ID. Keyed only on updateExistingId so a late account-list load
  // doesn't clobber edits the user made after selecting.
  useEffect(() => {
    if (!updateExistingId) return;
    let cancelled = false;
    const apply = (b: SharedBuild) => {
      if (cancelled) return;
      setShareDescription(b.description ?? '');
      setShareAuthor(b.author_name ?? '');
      setShareServer(b.server ?? '');
      setShareTags((b.tags ?? []).join(', '));
    };
    const known = accountBuilds.find((b) => b.id === updateExistingId);
    if (known) {
      apply(known);
    } else {
      getSharedBuild(updateExistingId).then((b) => { if (b) apply(b); }).catch(() => {});
    }
    return () => { cancelled = true; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [updateExistingId]);

  // ============================================
  // SAVE HANDLERS
  // ============================================

  const handleExport = () => {
    const exportData = JSON.parse(exportBuild());

    if (buildAlias.trim()) {
      exportData.meta = {
        ...exportData.meta,
        buildAlias: buildAlias.trim(),
      };
    }

    const json = JSON.stringify(exportData, null, 2);
    const blob = new Blob([json], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');

    const filename = (buildAlias.trim() || build.name || 'build')
      .replace(/[^a-z0-9]/gi, '_')
      .toLowerCase();

    link.href = url;
    link.download = `${filename}_${Date.now()}.skif`;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(url);

    setBuildAlias('');
    onClose();
  };

  const handleVaultSave = async (asNew = false) => {
    setVaultSaveError(null);
    setVaultSaveLoading(true);
    setVaultSaveSuccess(false);

    try {
      const exportData = JSON.parse(exportBuild()) as BuildExport;
      // Generate a meaningful name if the user hasn't set one
      let vaultName = buildAlias.trim() || build.name;
      if (!vaultName || /^untitled\s*build$/i.test(vaultName.trim())) {
        const at = build.archetype.name || '';
        const pri = build.primary.name || '';
        const sec = build.secondary.name || '';
        vaultName = ['Generic ' + at, pri, sec].filter(Boolean).join(' - ') || 'Saved Build';
      }

      // Update an existing entry when the build is linked and the user
      // can authorize it (owner token or login session). asNew=true
      // forks: drop the link and create a fresh entry.
      const updateExisting = !asNew && canUpdateVault && linkedVaultId;
      const tags = vaultTags.split(',').map((t) => t.trim()).filter(Boolean);
      const result = await shareBuild({
        name: vaultName,
        description: vaultDescription,
        author_name: vaultAuthor,
        server: vaultServer,
        tags,
        build_json: exportData,
        // Force private only when creating a fresh vault entry. On an update,
        // omit visibility so the backend preserves the current visibility —
        // otherwise re-saving edits would silently revert a build the user made
        // public via the visibility toggle.
        ...(updateExisting ? {} : { visibility: 'private' as const }),
        existingId: updateExisting ? linkedVaultId : undefined,
      });
      // Link the in-memory build to the (new or updated) entry so a
      // follow-up Save continues to update in place.
      if (result?.id) setVaultId(result.id);
      if (result?.rateLimit) setVaultRemaining(result.rateLimit.remaining);
      setVaultSaveSuccess(true);
    } catch (e) {
      setVaultSaveError(
        e instanceof RateLimitError ? formatRateLimitMessage(e)
          : e instanceof Error ? e.message : 'Failed to save to library',
      );
    } finally {
      setVaultSaveLoading(false);
    }
  };

  // ============================================
  // LOAD LOCAL HANDLERS
  // ============================================

  const handleImportFromText = () => {
    setImportError(null);
    setImportSuccess(false);

    if (!importText.trim()) {
      setImportError('Please paste build data to import');
      return;
    }

    const fromOtherServer = crossDatasetChoice(importText, getActiveDataset().id);
    if (fromOtherServer) {
      setCrossDataset({ text: importText, from: fromOtherServer });
      return;
    }

    try {
      const success = importBuild(importText);
      if (success) {
        setImportSuccess(true);
        setImportText('');
        setTimeout(() => {
          onClose();
          setImportSuccess(false);
        }, 1500);
      } else {
        setImportError('Failed to import build. Please check the format.');
      }
    } catch {
      setImportError('Invalid build data. Please check the format.');
    }
  };

  /**
   * Finish a cross-server open the user has now answered.
   *
   * `onItsOwnServer` reloads onto the file's dataset and never returns here — the page is
   * replaced. The port stays and reports, because what a port could not carry is the whole
   * reason it needs a receipt.
   */
  const resolveCrossDataset = (intoLoadedDataset: boolean) => {
    if (!crossDataset) return;
    const { text } = crossDataset;
    setCrossDataset(null);
    try {
      if (!importBuild(text, { intoLoadedDataset })) {
        setImportError('Failed to import build. Please check the format.');
        return;
      }
    } catch {
      setImportError('Invalid build data. Please check the format.');
      return;
    }
    if (!intoLoadedDataset) return; // the reload is already underway
    setImportText('');
    setPortNotes(useBuildStore.getState().lastImportNotes);
  };

  const handleImportFromFile = (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    if (!file) return;

    setImportError(null);
    setImportSuccess(false);

    const reader = new FileReader();
    reader.onload = (e) => {
      const content = e.target?.result as string;
      const fromOtherServer = crossDatasetChoice(content, getActiveDataset().id);
      if (fromOtherServer) {
        setCrossDataset({ text: content, from: fromOtherServer });
        if (fileInputRef.current) fileInputRef.current.value = '';
        return;
      }
      try {
        const success = importBuild(content);
        if (success) {
          setImportSuccess(true);
          if (fileInputRef.current) {
            fileInputRef.current.value = '';
          }
          setTimeout(() => {
            onClose();
            setImportSuccess(false);
          }, 1500);
        } else {
          setImportError('Failed to import build. Please check the file format.');
        }
      } catch {
        setImportError('Invalid build file. Please check the format.');
      }
    };
    reader.onerror = () => {
      setImportError('Failed to read file');
    };
    reader.readAsText(file);
  };

  // ============================================
  // POPMENU HANDLER
  // ============================================

  const handlePopmenuSave = async () => {
    const name = popmenuName || build.name?.replace(/[^a-zA-Z0-9]/g, '_') || 'MyBuild';
    const content = generatePopmenu(build, name);

    if ('showSaveFilePicker' in window) {
      try {
        setPopmenuStatus(null);
        setPopmenuError(null);

        const handle = await (window as any).showSaveFilePicker({
          suggestedName: `${name}.mnu`,
          types: [{
            description: 'Popmenu file',
            accept: { 'text/plain': ['.mnu'] },
          }],
        });

        const writable = await handle.createWritable();
        await writable.write(content);
        await writable.close();

        setPopmenuStatus(`Saved ${name}.mnu`);
      } catch (e: any) {
        if (e?.name === 'AbortError') return;
        setPopmenuError(`Failed to save: ${e?.message || 'Unknown error'}`);
      }
      return;
    }

    const blob = new Blob([content], { type: 'text/plain' });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url;
    link.download = `${name}.mnu`;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(url);
  };

  // ============================================
  // MIDS IMPORT HANDLERS
  // ============================================

  const parseMidsContent = (content: string) => {
    setMidsError(null);
    setMidsResult(null);
    setShowWarnings(false);

    if (!content.trim()) {
      setMidsError('No data to import');
      return;
    }

    // Detect MXD text format vs MBD JSON format
    const trimmed = content.trim();
    if (trimmed.startsWith('This Hero') || trimmed.startsWith('This Villain')) {
      // MXD legacy text format
      try {
        const parsed = parseMxdText(trimmed);
        if (parsed) {
          // Convert MXD parsed data to a Mids import result
          const result = importMxdBuild(parsed);
          if (result.success) {
            setMidsResult(result);
          } else {
            const msg = result.warnings.length > 0
              ? result.warnings[0].message
              : 'Failed to parse .mxd file';
            setMidsError(msg);
          }
        } else {
          setMidsError('Could not parse MXD file format. Make sure it is a valid Mids Reborn .mxd file.');
        }
      } catch {
        setMidsError('Failed to parse .mxd file.');
      }
      return;
    }

    try {
      const result = importMidsBuild(content);
      if (result.success) {
        setMidsResult(result);
      } else {
        const msg = result.warnings.length > 0
          ? result.warnings[0].message
          : 'Failed to parse .mbd file';
        setMidsError(msg);
      }
    } catch {
      setMidsError('Failed to parse .mbd file. Make sure it is valid Mids Reborn JSON.');
    }
  };

  const handleMidsParseFromText = () => {
    parseMidsContent(midsText);
  };

  const handleMidsFileUpload = (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    if (!file) return;

    const reader = new FileReader();
    reader.onload = (e) => {
      const content = e.target?.result as string;
      setMidsText(content);
      parseMidsContent(content);
    };
    reader.onerror = () => {
      setMidsError('Failed to read file');
    };
    reader.readAsText(file);
  };

  const handleMidsApply = () => {
    if (!midsResult?.build) return;
    applyMidsBuild(midsResult.build);
    // Auto-set branch for VEAT imports (e.g., Crab Spider, Bane Spider)
    if (midsResult.detectedBranch) {
      useUIStore.getState().setSelectedBranch(midsResult.detectedBranch as ArchetypeBranchId);
    }
    // Copy Mids' per-power slider values (Siphon Speed stacks, etc.) so the
    // dashboard reproduces Mids' default totals after import.
    if (midsResult.targetsHit) {
      useUIStore.getState().setTargetsHitBulk(midsResult.targetsHit);
    }
    handleClose();
  };

  // ============================================
  // GAME IMPORT HANDLERS
  // ============================================

  const handleGameParse = () => {
    setGameError(null);
    setGameResult(null);
    setShowGameWarnings(false);

    if (!gameText.trim()) {
      setGameError('No data to import');
      return;
    }

    try {
      const result = importGameExport(gameText);
      if (result.success) {
        setGameResult(result);
      } else {
        const msg = result.warnings.length > 0
          ? result.warnings[0].message
          : 'Failed to parse game export';
        setGameError(msg);
      }
    } catch {
      setGameError('Failed to parse game export. Make sure it is a valid Homecoming build export.');
    }
  };

  const handleGameFileUpload = (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    if (!file) return;

    const reader = new FileReader();
    reader.onload = (e) => {
      const content = e.target?.result as string;
      setGameText(content);
      setGameError(null);
      setGameResult(null);
      setShowGameWarnings(false);

      // Auto-parse
      try {
        const result = importGameExport(content);
        if (result.success) {
          setGameResult(result);
        } else {
          const msg = result.warnings.length > 0
            ? result.warnings[0].message
            : 'Failed to parse game export';
          setGameError(msg);
        }
      } catch {
        setGameError('Failed to parse game export. Make sure it is a valid Homecoming build export.');
      }
    };
    reader.onerror = () => {
      setGameError('Failed to read file');
    };
    reader.readAsText(file);
  };

  const handleGameApply = () => {
    if (!gameResult?.build) return;
    applyMidsBuild(gameResult.build);
    handleClose();
  };

  // ============================================
  // SHARE HANDLERS
  // ============================================

  const handleShare = async () => {
    setShareError(null);
    setShareLoading(true);
    setShareUrl(null);
    setShareUpdated(false);

    try {
      const exportData = JSON.parse(exportBuild()) as BuildExport;
      const tags = shareTags
        .split(',')
        .map((t) => t.trim())
        .filter(Boolean);

      // Generate a meaningful name if the user hasn't set one
      let shareName = build.name;
      if (!shareName || /^untitled\s*build$/i.test(shareName.trim())) {
        const at = build.archetype.name || '';
        const pri = build.primary.name || '';
        const sec = build.secondary.name || '';
        shareName = ['Generic ' + at, pri, sec].filter(Boolean).join(' - ') || 'Shared Build';
      }

      const result = await shareBuild({
        name: shareName,
        description: shareDescription,
        author_name: shareAuthor,
        server: shareServer,
        tags,
        build_json: exportData,
        existingId: updateExistingId ?? undefined,
        visibility: 'public',
      });

      setShareUrl(result.url);
      setShareUpdated(result.updated ?? false);
      setSharedBuildId(result.id);
      if (result.rateLimit) setShareRemaining(result.rateLimit.remaining);
    } catch (e) {
      setShareError(
        e instanceof RateLimitError ? formatRateLimitMessage(e)
          : e instanceof Error ? e.message : 'Failed to share build',
      );
    } finally {
      setShareLoading(false);
    }
  };

  const handleCopyShareUrl = async () => {
    if (!shareUrl) return;
    try {
      await navigator.clipboard.writeText(shareUrl);
      setShareCopied(true);
      setTimeout(() => setShareCopied(false), 2000);
    } catch {
      // Fallback: select the text in the input
    }
  };

  // ============================================
  // CLOSE
  // ============================================

  const handleClose = () => {
    setBuildAlias('');
    setImportText('');
    setImportError(null);
    setImportSuccess(false);
    setShowPopmenu(false);
    setPopmenuName('');
    setPopmenuStatus(null);
    setPopmenuError(null);
    setLoadSource('local');
    setMidsText('');
    setMidsResult(null);
    setMidsError(null);
    setShowWarnings(false);
    setGameText('');
    setGameResult(null);
    setGameError(null);
    setShowGameWarnings(false);
    setShareExportSubTab('share');
    setShareDescription('');
    setShareAuthor('');
    setShareServer('');
    setShareTags('');
    setShareError(null);
    setShareUrl(null);
    setShareLoading(false);
    setShareCopied(false);
    setShareUpdated(false);
    setUpdateExistingId(null);
    setSharedBuildId(null);
    setShowOwnerToken(false);
    setTokenCopied(false);
    setVaultSaveSuccess(false);
    setVaultSaveError(null);
    setVaultSaveLoading(false);
    setActiveTab('save');
    if (midsFileInputRef.current) midsFileInputRef.current.value = '';
    if (gameFileInputRef.current) gameFileInputRef.current.value = '';
    onClose();
  };

  // ============================================
  // RENDER HELPERS
  // ============================================

  const renderWarningsToggle = (
    warnings: { type: string; message: string; midsName?: string; name?: string }[],
    show: boolean,
    setShow: (v: boolean) => void,
    useNameField = false,
  ) => {
    if (warnings.length === 0) return null;
    return (
      <div className="bg-yellow-900/20 border border-yellow-700/50 rounded p-3 text-sm">
        <button
          type="button"
          onClick={() => setShow(!show)}
          className="flex items-center gap-1 text-yellow-300 hover:text-yellow-200 font-medium text-sm w-full"
        >
          <svg
            className={`w-4 h-4 transition-transform ${show ? 'rotate-90' : ''}`}
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
          >
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
          </svg>
          {warnings.length} warning{warnings.length !== 1 ? 's' : ''}
        </button>
        {show && (
          <ul className="mt-2 space-y-1 text-xs text-yellow-200 max-h-40 overflow-y-auto">
            {warnings.map((w, i) => (
              <li key={i} className="flex gap-2">
                <span className="text-yellow-500 shrink-0">[{w.type}]</span>
                <span>{w.message}{useNameField ? (w.name ? `: ${w.name}` : '') : (w.midsName ? `: ${w.midsName}` : '')}</span>
              </li>
            ))}
          </ul>
        )}
      </div>
    );
  };

  const renderImportResultSummary = (
    resultBuild: NonNullable<MidsImportResult['build']> | NonNullable<GameImportResult['build']>,
    summary: {
      powersImported: number; powersFailed: number;
      enhancementsImported: number; enhancementsFailed: number;
      accoladesImported?: number; incarnatesImported?: number;
    },
    characterName?: string,
  ) => {
    // Enhancements the source tool routed into a power the game won't allow them in.
    // Shield Defense's permuted internal names used to be the common cause and are no
    // longer (MBDIMPORT-2 resolves the rotation before binding), so what reaches here now
    // is a genuine mismatch. They're excluded from totals and flagged in the build; tell
    // the user so they can move or clear them.
    const illegal = findIllegalSlots(resultBuild);
    const illegalByPower = new Map<string, string[]>();
    for (const s of illegal) {
      const list = illegalByPower.get(s.powerName) ?? [];
      list.push(s.enhancement.name);
      illegalByPower.set(s.powerName, list);
    }
    return (
    <>
    <div className="bg-[var(--color-success)]/15 border border-[var(--color-success)]/50 rounded p-3 text-sm text-[var(--color-success-fg)]">
      <p className="font-semibold mb-2">Parse Successful</p>
      <div className="grid grid-cols-2 gap-x-4 gap-y-1 text-xs">
        {characterName && (
          <>
            <span>Character:</span>
            <span className="text-white">{characterName}</span>
          </>
        )}
        <span>Archetype:</span>
        <span className="text-white">{resultBuild.archetype.name}</span>
        <span>Primary:</span>
        <span className="text-white">{resultBuild.primary.name || 'None'}</span>
        <span>Secondary:</span>
        <span className="text-white">{resultBuild.secondary.name || 'None'}</span>
        <span>Level:</span>
        <span className="text-white">{resultBuild.level}</span>
        <span>Powers:</span>
        <span className="text-white">
          {summary.powersImported} picked
          {summary.powersFailed > 0 && (
            <span className="text-red-400"> / {summary.powersFailed} failed</span>
          )}
        </span>
        <span>Enhancements:</span>
        <span className="text-white">
          {summary.enhancementsImported} imported
          {summary.enhancementsFailed > 0 && (
            <span className="text-red-400"> / {summary.enhancementsFailed} failed</span>
          )}
        </span>
        {/* Accolades and incarnates get their own lines rather than being folded into
            "Powers": neither consumes a power pick, and folding them in is what made the
            old count disagree with the dashboard's Pwr chip. Hidden when the importer
            does not report the split (the .mxd path) or the build carries none. */}
        {!!summary.accoladesImported && (
          <>
            <span>Accolades:</span>
            <span className="text-white">{summary.accoladesImported} imported</span>
          </>
        )}
        {!!summary.incarnatesImported && (
          <>
            <span>Incarnates:</span>
            <span className="text-white">{summary.incarnatesImported} imported</span>
          </>
        )}
        <span>Pools:</span>
        <span className="text-white">
          {resultBuild.pools.map((p) => p.name).join(', ') || 'None'}
        </span>
        {resultBuild.epicPool && (
          <>
            <span>Epic/Patron:</span>
            <span className="text-white">{resultBuild.epicPool.name}</span>
          </>
        )}
      </div>
    </div>
    {illegalByPower.size > 0 && (
      <div className="mt-2 bg-amber-500/10 border border-amber-500/50 rounded p-3 text-xs text-amber-200">
        <p className="font-semibold mb-1">
          {illegal.length} enhancement{illegal.length === 1 ? '' : 's'} can&apos;t be slotted where the source put {illegal.length === 1 ? 'it' : 'them'}
        </p>
        <p className="mb-2 text-amber-200/80">
          The originating tool routed {illegal.length === 1 ? 'it' : 'them'} into {illegalByPower.size === 1 ? 'a power' : 'powers'} the game won&apos;t allow (usually a power-name mismatch). {illegal.length === 1 ? 'It is' : 'They are'} excluded from all totals and flagged red in the build — move or clear {illegal.length === 1 ? 'it' : 'them'} to fix your slotting.
        </p>
        <ul className="space-y-0.5">
          {[...illegalByPower.entries()].map(([powerName, enhNames]) => (
            <li key={powerName}>
              <span className="text-white">{powerName}</span>: {enhNames.join(', ')}
            </li>
          ))}
        </ul>
      </div>
    )}
    </>
    );
  };

  // ============================================
  // RENDER
  // ============================================

  return (
    <>
    <Modal isOpen={isOpen} onClose={handleClose} size="lg" showCloseButton={true}>
      <ModalHeader>
        <div className="flex gap-4 border-b border-gray-700 -mb-4">
          <button
            className={`px-4 py-2 font-semibold transition-colors ${
              activeTab === 'save'
                ? 'text-[var(--color-link)] border-b-2 border-[var(--color-selected)]'
                : 'text-gray-400 hover:text-gray-300'
            }`}
            onClick={() => setActiveTab('save')}
          >
            Save
          </button>
          <button
            className={`px-4 py-2 font-semibold transition-colors ${
              activeTab === 'load-import'
                ? 'text-[var(--color-link)] border-b-2 border-[var(--color-selected)]'
                : 'text-gray-400 hover:text-gray-300'
            }`}
            onClick={() => setActiveTab('load-import')}
          >
            Load / Import
          </button>
          <button
            className={`px-4 py-2 font-semibold transition-colors ${
              activeTab === 'share-export'
                ? 'text-[var(--color-link)] border-b-2 border-[var(--color-selected)]'
                : 'text-gray-400 hover:text-gray-300'
            }`}
            onClick={() => setActiveTab('share-export')}
          >
            Share / Export
          </button>
        </div>
      </ModalHeader>

      {/* Fixed body height + internal scroll keeps the card a constant size, so
          the centered modal doesn't jump when switching tabs or expanding inline
          sections (popmenu, import warnings). Header/footer stay pinned. */}
      <ModalBody className="h-[60vh] overflow-y-auto">
        {activeTab === 'save' ? (
          /* ========== SAVE TAB ========== */
          <div className="space-y-4">
            {/* Save section */}
            <div className="space-y-3">
              <h3 className="text-sm font-semibold text-gray-300 uppercase tracking-wide">Save Build</h3>

              <div>
                <label className="block text-sm font-medium text-gray-300 mb-2">
                  Build Name/Alias (Optional)
                </label>
                <input
                  type="text"
                  value={buildAlias}
                  onChange={(e) => setBuildAlias(e.target.value)}
                  placeholder="e.g., Fire/Kin Tank - Farm Build"
                  className="w-full bg-gray-800 border border-gray-600 rounded px-3 py-2 text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-[var(--color-ring)]"
                  maxLength={100}
                />
              </div>

              <div className="bg-gray-800 border border-gray-600 rounded p-4 space-y-2">
                <h3 className="font-semibold text-gray-300">Current Build:</h3>
                <div className="text-sm text-gray-400 space-y-1">
                  <p><span className="text-gray-500">Name:</span> {build.name || 'Unnamed Build'}</p>
                  <p><span className="text-gray-500">Archetype:</span> {build.archetype.name || 'None'}</p>
                  <p><span className="text-gray-500">Level:</span> {build.level}</p>
                  {build.primary.name && (
                    <p><span className="text-gray-500">Primary:</span> {build.primary.name}</p>
                  )}
                  {build.secondary.name && (
                    <p><span className="text-gray-500">Secondary:</span> {build.secondary.name}</p>
                  )}
                </div>
              </div>

              <Button variant="primary" onClick={handleExport} className="w-full">
                Download Build (.skif)
              </Button>
            </div>

            {/* Vault divider */}
            <div className="relative">
              <div className="absolute inset-0 flex items-center">
                <div className="w-full border-t border-gray-600"></div>
              </div>
              <div className="relative flex justify-center text-sm">
                <span className="px-2 bg-gray-900 text-gray-500">BUILD LIBRARY</span>
              </div>
            </div>

            {/* Vault save section */}
            <div className="space-y-3">
              {user ? (
                <>
                  {vaultSaveSuccess ? (
                    <div className="bg-[var(--color-info)]/15 border border-[var(--color-info)]/50 rounded p-4 text-sm text-[var(--color-info-fg)]">
                      <p className="font-semibold">
                        {canUpdateVault ? 'Build updated in your library!' : 'Build saved to your library!'}
                      </p>
                      <p className="text-xs text-[var(--color-info-fg)] mt-1">
                        This build is private and only visible to you in My Builds.
                      </p>
                    </div>
                  ) : (
                    <>
                      {canUpdateVault && (
                        <p className="text-xs text-[var(--color-info-fg)]">
                          This build is linked to a saved entry in your library. Saving will overwrite the existing entry; choose "Save as new" to fork it instead.
                        </p>
                      )}

                      {/* Optional metadata — description / server / tags. Kept
                          collapsed so a quick save stays one click, but these are
                          saved with the private build (and editable later from the
                          build's page). */}
                      <div className="border border-gray-700 rounded">
                        <button
                          type="button"
                          onClick={() => setVaultDetailsOpen((o) => !o)}
                          className="w-full flex items-center justify-between px-3 py-2 text-xs text-gray-400 hover:text-gray-200 transition-colors"
                        >
                          <span>Add description, server &amp; tags (optional)</span>
                          <svg
                            className={`w-4 h-4 transition-transform ${vaultDetailsOpen ? 'rotate-180' : ''}`}
                            fill="none" viewBox="0 0 24 24" stroke="currentColor"
                          >
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
                          </svg>
                        </button>
                        {vaultDetailsOpen && (
                          <div className="px-3 pb-3 pt-1 space-y-3 border-t border-gray-700">
                            <div>
                              <label className="block text-xs font-medium text-gray-400 mb-1">Description</label>
                              <textarea
                                value={vaultDescription}
                                onChange={(e) => setVaultDescription(e.target.value)}
                                placeholder="What it's for, how to play it, etc."
                                className="w-full h-20 bg-gray-800 border border-gray-600 rounded px-3 py-2 text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-[var(--color-ring)] text-sm"
                                maxLength={500}
                              />
                            </div>
                            <div>
                              <label className="block text-xs font-medium text-gray-400 mb-1">Server</label>
                              <input
                                type="text"
                                value={vaultServer}
                                onChange={(e) => setVaultServer(e.target.value)}
                                placeholder="e.g., Homecoming"
                                className="w-full bg-gray-800 border border-gray-600 rounded px-3 py-2 text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-[var(--color-ring)] text-sm"
                                maxLength={50}
                              />
                            </div>
                            <div>
                              <label className="block text-xs font-medium text-gray-400 mb-1">
                                Tags <span className="text-gray-500">(comma-separated)</span>
                              </label>
                              <input
                                type="text"
                                value={vaultTags}
                                onChange={(e) => setVaultTags(e.target.value)}
                                placeholder="e.g., PvP, farming, budget, softcap"
                                className="w-full bg-gray-800 border border-gray-600 rounded px-3 py-2 text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-[var(--color-ring)] text-sm"
                                maxLength={200}
                              />
                            </div>
                          </div>
                        )}
                      </div>

                      <Button
                        variant="primary"
                        onClick={() => handleVaultSave(false)}
                        isLoading={vaultSaveLoading}
                        disabled={vaultSaveLoading || !build.archetype.id}
                        className="w-full !bg-[var(--color-primary)] hover:!bg-[var(--color-primary-hover)]"
                      >
                        {canUpdateVault ? 'Update Saved Build' : 'Save to Library (Private)'}
                      </Button>
                      {canUpdateVault && (
                        <Button
                          variant="secondary"
                          onClick={() => handleVaultSave(true)}
                          isLoading={vaultSaveLoading}
                          disabled={vaultSaveLoading || !build.archetype.id}
                          className="w-full"
                        >
                          Save as new copy
                        </Button>
                      )}
                      <p className="text-xs text-gray-500">
                        {vaultRemaining !== null
                          ? `${vaultRemaining} save${vaultRemaining === 1 ? '' : 's'} left this hour.`
                          : rateLimitHint('vault')}
                      </p>
                      {vaultSaveError && (
                        <div className="bg-red-900/20 border border-red-700/50 rounded p-3 text-sm text-red-300">
                          {vaultSaveError}
                        </div>
                      )}
                    </>
                  )}
                </>
              ) : (
                <div className="text-center py-3">
                  <p className="text-sm text-gray-500">Log in to save builds to your library</p>
                </div>
              )}
            </div>
          </div>

        ) : activeTab === 'load-import' ? (
          /* ========== LOAD / IMPORT TAB ========== */
          <div className="space-y-4">
            {/* 3-way source toggle */}
            <div className="flex gap-2">
              <button
                className={`flex-1 py-2 px-3 rounded text-sm font-semibold transition-colors ${
                  loadSource === 'local'
                    ? 'bg-[var(--color-primary)] text-[var(--color-primary-fg)]'
                    : 'bg-gray-800 text-gray-400 hover:text-gray-300 hover:bg-gray-700'
                }`}
                onClick={() => setLoadSource('local')}
              >
                Local (.skif)
              </button>
              <button
                className={`flex-1 py-2 px-3 rounded text-sm font-semibold transition-colors ${
                  loadSource === 'mids'
                    ? 'bg-[var(--color-primary)] text-[var(--color-primary-fg)]'
                    : 'bg-gray-800 text-gray-400 hover:text-gray-300 hover:bg-gray-700'
                }`}
                onClick={() => setLoadSource('mids')}
              >
                Mids Reborn (.mbd)
              </button>
              <button
                className={`flex-1 py-2 px-3 rounded text-sm font-semibold transition-colors ${
                  loadSource === 'game'
                    ? 'bg-[var(--color-primary)] text-[var(--color-primary-fg)]'
                    : 'bg-gray-800 text-gray-400 hover:text-gray-300 hover:bg-gray-700'
                }`}
                onClick={() => setLoadSource('game')}
              >
                Game (/buildsave)
              </button>
            </div>

            {loadSource === 'local' ? (
              /* Local JSON load */
              <div className="space-y-3">
                <div>
                  <label className="block text-sm font-medium text-gray-300 mb-2">
                    Load from File
                  </label>
                  <input
                    ref={fileInputRef}
                    type="file"
                    accept=".json,.skif,application/json,application/octet-stream,text/plain"
                    onChange={handleImportFromFile}
                    className="w-full text-sm text-gray-400
                      file:mr-4 file:py-2 file:px-4
                      file:rounded file:border-0
                      file:text-sm file:font-semibold
                      file:bg-[var(--color-primary)] file:text-[var(--color-primary-fg)]
                      hover:file:bg-[var(--color-primary-hover)]
                      file:cursor-pointer cursor-pointer"
                  />
                </div>

                <div className="relative">
                  <div className="absolute inset-0 flex items-center">
                    <div className="w-full border-t border-gray-600"></div>
                  </div>
                  <div className="relative flex justify-center text-sm">
                    <span className="px-2 bg-gray-900 text-gray-500">OR</span>
                  </div>
                </div>

                <div>
                  <label className="block text-sm font-medium text-gray-300 mb-2">
                    Paste Build Data
                  </label>
                  <textarea
                    value={importText}
                    onChange={(e) => setImportText(e.target.value)}
                    placeholder="Paste exported build JSON here..."
                    className="w-full h-24 bg-gray-800 border border-gray-600 rounded px-3 py-2 text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-[var(--color-ring)] font-mono text-sm"
                  />
                  {importText.trim() && (
                    <Button
                      variant="primary"
                      size="sm"
                      onClick={handleImportFromText}
                      disabled={importSuccess}
                      className="mt-2"
                    >
                      Import from Text
                    </Button>
                  )}
                </div>

                {crossDataset && (
                  <div className="bg-[var(--color-warning)]/15 border border-[var(--color-warning)]/50 rounded p-3 text-sm text-[var(--color-warning-fg)] space-y-2">
                    <p>
                      This build was saved on <strong>{serverLabel(crossDataset.from)}</strong>,
                      and <strong>{getActiveDataset().displayName}</strong> is loaded.
                    </p>
                    <p className="text-xs opacity-80">
                      Reading it here keeps every pick, but anything{' '}
                      {getActiveDataset().displayName} does not carry contributes nothing to the
                      totals — you'll get a list of what those are.
                    </p>
                    <div className="flex flex-wrap gap-2 pt-1">
                      <Button variant="primary" size="sm" onClick={() => resolveCrossDataset(false)}>
                        Open on {serverLabel(crossDataset.from)}
                      </Button>
                      <Button variant="secondary" size="sm" onClick={() => resolveCrossDataset(true)}>
                        Read it on {getActiveDataset().displayName}
                      </Button>
                      <Button variant="ghost" size="sm" onClick={() => setCrossDataset(null)}>
                        Cancel
                      </Button>
                    </div>
                  </div>
                )}
                {portNotes && (
                  <div className="bg-[var(--color-success)]/15 border border-[var(--color-success)]/50 rounded p-3 text-sm text-[var(--color-success-fg)] space-y-1">
                    <p>
                      Build read on {getActiveDataset().displayName}
                      {portNotes.length === 0
                        ? ' — everything in it resolved.'
                        : `. ${portNotes.length} ${portNotes.length === 1 ? 'entry' : 'entries'} this dataset does not carry:`}
                    </p>
                    {portNotes.length > 0 && (
                      <ul className="text-xs opacity-90 list-disc pl-5 max-h-32 overflow-y-auto">
                        {portNotes.map((note, i) => (
                          <li key={`${note.context}-${note.detail}-${i}`}>
                            {note.context}: {note.detail}
                          </li>
                        ))}
                      </ul>
                    )}
                    <Button variant="ghost" size="sm" onClick={() => { setPortNotes(null); onClose(); }}>
                      Done
                    </Button>
                  </div>
                )}
                {importError && (
                  <div className="bg-red-900/20 border border-red-700/50 rounded p-3 text-sm text-red-300">
                    {importError}
                  </div>
                )}
                {importSuccess && (
                  <div className="bg-[var(--color-success)]/15 border border-[var(--color-success)]/50 rounded p-3 text-sm text-[var(--color-success-fg)]">
                    Build imported successfully!
                  </div>
                )}
              </div>

            ) : loadSource === 'mids' ? (
              /* Mids Import */
              <div className="space-y-4">
                <div className="bg-[var(--color-warning)]/15 border border-[var(--color-warning)]/50 rounded p-3 text-sm text-[var(--color-warning-fg)]">
                  <p>Upload an .MBD file or paste its contents below. </p>
                  <p>!!! PLEASE READ !!! Here's the reality: Mids name patterns are all over the place. Some entries have no AT designation at all, others use abbreviated or combined AT names, and a few use prefixes instead of suffixes. There are also cases where the AT name appears as a prefix instead of a suffix, which makes the naming even more inconsistent. Importing and exporting Mids files reliably is going to be a long work in progress.</p>
                </div>

                <div>
                  <label className="block text-sm font-medium text-gray-300 mb-2">
                    Upload .mbd File
                  </label>
                  <input
                    ref={midsFileInputRef}
                    type="file"
                    accept=".mbd,.mxd,.json,.skif,application/json,application/octet-stream,text/plain"
                    onChange={handleMidsFileUpload}
                    className="w-full text-sm text-gray-400
                      file:mr-4 file:py-2 file:px-4
                      file:rounded file:border-0
                      file:text-sm file:font-semibold
                      file:bg-[var(--color-primary)] file:text-[var(--color-primary-fg)]
                      hover:file:bg-[var(--color-primary-hover)]
                      file:cursor-pointer cursor-pointer"
                  />
                </div>

                <div className="relative">
                  <div className="absolute inset-0 flex items-center">
                    <div className="w-full border-t border-gray-600"></div>
                  </div>
                  <div className="relative flex justify-center text-sm">
                    <span className="px-2 bg-gray-900 text-gray-500">OR</span>
                  </div>
                </div>

                <div>
                  <label className="block text-sm font-medium text-gray-300 mb-2">
                    Paste .mbd Contents
                  </label>
                  <textarea
                    value={midsText}
                    onChange={(e) => { setMidsText(e.target.value); setMidsResult(null); setMidsError(null); }}
                    placeholder="Paste .mbd JSON here..."
                    className="w-full h-32 bg-gray-800 border border-gray-600 rounded px-3 py-2 text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-amber-500 font-mono text-sm"
                  />
                </div>

                {midsError && (
                  <div className="bg-red-900/20 border border-red-700/50 rounded p-3 text-sm text-red-300">
                    {midsError}
                  </div>
                )}

                {midsResult && midsResult.success && midsResult.build && (
                  <div className="space-y-3">
                    {renderImportResultSummary(midsResult.build, midsResult.summary)}
                    {renderWarningsToggle(midsResult.warnings, showWarnings, setShowWarnings)}
                    <div className="bg-yellow-900/20 border border-yellow-700/50 rounded p-3 text-sm text-yellow-300">
                      <p className="font-semibold mb-1">Warning:</p>
                      <p>Applying this import will replace your current build.</p>
                    </div>
                  </div>
                )}
              </div>

            ) : (
              /* Game Import */
              <div className="space-y-4">
                <div className="bg-[var(--color-info)]/15 border border-[var(--color-info)]/50 rounded p-3 text-sm text-[var(--color-info-fg)]">
                  <p>Import a build from the <span className="font-semibold">Homecoming</span> in-game build export. Use the <span className="text-sk-magenta font-semibold">/buildsave</span> command in-game, then upload the text file or paste its contents below.</p>
                </div>

                <div>
                  <label className="block text-sm font-medium text-gray-300 mb-2">
                    Upload Build Export (.txt)
                  </label>
                  <input
                    ref={gameFileInputRef}
                    type="file"
                    accept=".txt"
                    onChange={handleGameFileUpload}
                    className="w-full text-sm text-gray-400
                      file:mr-4 file:py-2 file:px-4
                      file:rounded file:border-0
                      file:text-sm file:font-semibold
                      file:bg-[var(--color-primary)] file:text-[var(--color-primary-fg)]
                      hover:file:bg-[var(--color-primary-hover)]
                      file:cursor-pointer cursor-pointer"
                  />
                </div>

                <div className="relative">
                  <div className="absolute inset-0 flex items-center">
                    <div className="w-full border-t border-gray-600"></div>
                  </div>
                  <div className="relative flex justify-center text-sm">
                    <span className="px-2 bg-gray-900 text-gray-500">OR</span>
                  </div>
                </div>

                <div>
                  <label className="block text-sm font-medium text-gray-300 mb-2">
                    Paste Build Export
                  </label>
                  <textarea
                    value={gameText}
                    onChange={(e) => { setGameText(e.target.value); setGameResult(null); setGameError(null); }}
                    placeholder={"CharName: Level 50 Science Class_Tanker\n\nCharacter Profile:\n------------------\nLevel 1: Tanker_Defense Radiation_Armor Alpha_Barrier\n\tAttuned_Unbreakable_Guard_A (1)\n..."}
                    className="w-full h-48 bg-gray-800 border border-gray-600 rounded px-3 py-2 text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-cyan-500 font-mono text-sm"
                  />
                </div>

                {gameError && (
                  <div className="bg-red-900/20 border border-red-700/50 rounded p-3 text-sm text-red-300">
                    {gameError}
                  </div>
                )}

                {gameResult && gameResult.success && gameResult.build && (
                  <div className="space-y-3">
                    {renderImportResultSummary(gameResult.build, gameResult.summary, gameResult.build.name)}
                    {renderWarningsToggle(gameResult.warnings, showGameWarnings, setShowGameWarnings, true)}
                    <div className="bg-yellow-900/20 border border-yellow-700/50 rounded p-3 text-sm text-yellow-300">
                      <p className="font-semibold mb-1">Warning:</p>
                      <p>Applying this import will replace your current build.</p>
                    </div>
                  </div>
                )}
              </div>
            )}
          </div>

        ) : (
          /* ========== SHARE / EXPORT TAB ========== */
          <div className="space-y-4">
            {/* Sub-tab toggle */}
            <div className="flex gap-2">
              <button
                className={`flex-1 py-2 px-3 rounded text-sm font-semibold transition-colors ${
                  shareExportSubTab === 'share'
                    ? 'bg-[var(--color-primary)] text-[var(--color-primary-fg)]'
                    : 'bg-gray-800 text-gray-400 hover:text-gray-300 hover:bg-gray-700'
                }`}
                onClick={() => setShareExportSubTab('share')}
              >
                Share Publicly
              </button>
              <button
                className={`flex-1 py-2 px-3 rounded text-sm font-semibold transition-colors ${
                  shareExportSubTab === 'export'
                    ? 'bg-gray-600 text-white'
                    : 'bg-gray-800 text-gray-400 hover:text-gray-300 hover:bg-gray-700'
                }`}
                onClick={() => setShareExportSubTab('export')}
              >
                Export Utilities
              </button>
            </div>

            {shareExportSubTab === 'share' ? (
              /* Share Publicly */
              <div className="space-y-4">
                {shareUrl ? (
                  <div className="space-y-4">
                    {/* Public share success */}
                    <div className="bg-[var(--color-success)]/15 border border-[var(--color-success)]/50 rounded p-4 text-sm text-[var(--color-success-fg)]">
                      <p className="font-semibold mb-2">
                        {shareUpdated ? 'Build updated successfully!' : 'Build shared successfully!'}
                      </p>
                      <p className="text-xs text-[var(--color-success-fg)] mb-3">Anyone with this link can view your build:</p>
                      <div className="flex gap-2">
                        <input
                          type="text"
                          readOnly
                          value={shareUrl}
                          className="flex-1 bg-gray-800 border border-gray-600 rounded px-3 py-2 text-white text-sm font-mono"
                          onFocus={(e) => e.target.select()}
                        />
                        <Button
                          variant={shareCopied ? 'secondary' : 'primary'}
                          size="sm"
                          onClick={handleCopyShareUrl}
                        >
                          {shareCopied ? 'Copied!' : 'Copy'}
                        </Button>
                      </div>
                      {!shareUpdated && (
                        <p className="text-xs text-[var(--color-success-fg)] mt-2">
                          You can update or delete this build later from this browser.
                        </p>
                      )}
                    </div>

                    {/* Owner token backup */}
                    {sharedBuildId && getOwnerToken(sharedBuildId) && (
                      <div className="bg-gray-800 border border-gray-600 rounded p-3 text-xs">
                        <button
                          type="button"
                          onClick={() => setShowOwnerToken(!showOwnerToken)}
                          className="flex items-center gap-1 text-gray-400 hover:text-gray-300 font-medium w-full"
                        >
                          <svg
                            className={`w-3 h-3 transition-transform ${showOwnerToken ? 'rotate-90' : ''}`}
                            fill="none"
                            viewBox="0 0 24 24"
                            stroke="currentColor"
                          >
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
                          </svg>
                          Owner Token (backup)
                        </button>
                        {showOwnerToken && (
                          <div className="mt-2 space-y-2">
                            <p className="text-gray-500">
                              Save this token to manage your build from another browser or after clearing cache.
                              You can enter it on the build's detail page to reclaim ownership.
                            </p>
                            <div className="flex gap-2">
                              <input
                                type="text"
                                readOnly
                                value={getOwnerToken(sharedBuildId) ?? ''}
                                className="flex-1 bg-gray-700 border border-gray-600 rounded px-2 py-1 text-white font-mono text-xs"
                                onFocus={(e) => e.target.select()}
                              />
                              <Button
                                variant={tokenCopied ? 'secondary' : 'ghost'}
                                size="sm"
                                onClick={async () => {
                                  const token = getOwnerToken(sharedBuildId);
                                  if (token) {
                                    await navigator.clipboard.writeText(token);
                                    setTokenCopied(true);
                                    setTimeout(() => setTokenCopied(false), 2000);
                                  }
                                }}
                              >
                                {tokenCopied ? 'Copied!' : 'Copy'}
                              </Button>
                            </div>
                          </div>
                        )}
                      </div>
                    )}
                  </div>
                ) : (
                  <>
                    {/* Update existing build option */}
                    {allOwnedIds.length > 0 && (
                      <div className="bg-gray-800 border border-gray-600 rounded p-3 space-y-2">
                        <label className="flex items-center gap-2 text-sm text-gray-300 cursor-pointer">
                          <input
                            type="checkbox"
                            checked={updateExistingId !== null}
                            onChange={(e) => {
                              if (e.target.checked) {
                                setUpdateExistingId(allOwnedIds[0]);
                              } else {
                                setUpdateExistingId(null);
                              }
                            }}
                            className="rounded border-gray-600 bg-gray-700 text-[var(--color-primary)] focus:ring-[var(--color-ring)]"
                          />
                          Update an existing shared build
                        </label>
                        {updateExistingId !== null && (
                          <div>
                            <label className="block text-xs font-medium text-gray-400 mb-1">Build to update</label>
                            <select
                              value={updateExistingId}
                              onChange={(e) => setUpdateExistingId(e.target.value)}
                              className="w-full bg-gray-700 border border-gray-600 rounded px-3 py-1.5 text-white text-sm focus:outline-none focus:ring-2 focus:ring-[var(--color-ring)]"
                            >
                              {allOwnedIds.map((id) => {
                                const info = accountBuildMap.get(id);
                                const label = info ? `${info.name} (${id})` : id;
                                return <option key={id} value={id}>{label}</option>;
                              })}
                            </select>
                            <p className="text-xs text-gray-500 mt-1">Replaces the build at the existing URL. Its current details are pre-filled below — edit only what you want to change.</p>
                          </div>
                        )}
                      </div>
                    )}

                    <div className="bg-gray-800 border border-gray-600 rounded p-4 space-y-2">
                      <h3 className="font-semibold text-gray-300">Build to Share:</h3>
                      <div className="text-sm text-gray-400 space-y-1">
                        <p><span className="text-gray-500">Name:</span> {build.name || 'Unnamed Build'}</p>
                        <p><span className="text-gray-500">Archetype:</span> {build.archetype.name || 'None'}</p>
                        <p><span className="text-gray-500">Level:</span> {build.level}</p>
                        {build.primary.name && (
                          <p><span className="text-gray-500">Primary:</span> {build.primary.name}</p>
                        )}
                        {build.secondary.name && (
                          <p><span className="text-gray-500">Secondary:</span> {build.secondary.name}</p>
                        )}
                      </div>
                    </div>

                    <div>
                      <label className="block text-sm font-medium text-gray-300 mb-1">
                        Description <span className="text-gray-500">(optional)</span>
                      </label>
                      <textarea
                        value={shareDescription}
                        onChange={(e) => setShareDescription(e.target.value)}
                        placeholder="Describe your build — what it's for, how to play it, etc."
                        className="w-full h-24 bg-gray-800 border border-gray-600 rounded px-3 py-2 text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-[var(--color-ring)] text-sm"
                        maxLength={500}
                      />
                      <p className="text-xs text-gray-500 mt-1">{shareDescription.length}/500</p>
                    </div>

                    <div className="grid grid-cols-2 gap-3">
                      <div>
                        <label className="block text-sm font-medium text-gray-300 mb-1">
                          Author Name <span className="text-gray-500">(optional)</span>
                        </label>
                        <input
                          type="text"
                          value={shareAuthor}
                          onChange={(e) => setShareAuthor(e.target.value)}
                          placeholder="Your global name"
                          className="w-full bg-gray-800 border border-gray-600 rounded px-3 py-2 text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-[var(--color-ring)] text-sm"
                          maxLength={50}
                        />
                      </div>
                      <div>
                        <label className="block text-sm font-medium text-gray-300 mb-1">
                          Server <span className="text-gray-500">(optional)</span>
                        </label>
                        <input
                          type="text"
                          value={shareServer}
                          onChange={(e) => setShareServer(e.target.value)}
                          placeholder="e.g., Homecoming"
                          className="w-full bg-gray-800 border border-gray-600 rounded px-3 py-2 text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-[var(--color-ring)] text-sm"
                          maxLength={50}
                        />
                      </div>
                    </div>

                    <div>
                      <label className="block text-sm font-medium text-gray-300 mb-1">
                        Tags <span className="text-gray-500">(optional, comma-separated)</span>
                      </label>
                      <input
                        type="text"
                        value={shareTags}
                        onChange={(e) => setShareTags(e.target.value)}
                        placeholder="e.g., PvP, farming, budget, softcap"
                        className="w-full bg-gray-800 border border-gray-600 rounded px-3 py-2 text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-[var(--color-ring)] text-sm"
                        maxLength={200}
                      />
                    </div>

                    <p className="text-xs text-gray-500">
                      {shareRemaining !== null
                        ? `${shareRemaining} public share${shareRemaining === 1 ? '' : 's'} left this hour.`
                        : rateLimitHint('share')}
                    </p>

                    {shareError && (
                      <div className="bg-red-900/20 border border-red-700/50 rounded p-3 text-sm text-red-300">
                        {shareError}
                      </div>
                    )}

                    <div className="bg-[var(--color-success)]/15 border border-[var(--color-success)]/50 rounded p-3 text-sm text-[var(--color-success-fg)]">
                      <p className="font-semibold mb-1">
                        {updateExistingId ? 'Update Info:' : 'Share Info:'}
                      </p>
                      <p>
                        {updateExistingId
                          ? 'This will replace the existing shared build with your current build data. The URL will stay the same.'
                          : 'Your build will be shared publicly. Anyone with the link can view it. No account required.'
                        }
                      </p>
                    </div>
                  </>
                )}
              </div>
            ) : (
              /* Export Utilities */
              <div className="space-y-4">
                {!levelUpMode && (
                  <p className="text-[11px] text-amber-400/90 bg-amber-400/10 border border-amber-400/20 rounded px-2 py-1.5">
                    This build wasn't planned in a specific leveling order, so the slot levels in
                    the Mids export and print sheet below are a computed placement, not your
                    actual leveling history.
                  </p>
                )}
                <div className="flex gap-3">
                  <Button
                    variant="secondary"
                    size="sm"
                    onClick={() => {
                      const { json, warnings } = exportToMidsWithReport(build, levelUpMode);
                      setMidsExportWarnings(warnings);
                      const blob = new Blob([json], { type: 'application/json' });
                      const url = URL.createObjectURL(blob);
                      const link = document.createElement('a');
                      const filename = (build.name || 'build')
                        .replace(/[^a-z0-9]/gi, '_')
                        .toLowerCase();
                      link.href = url;
                      link.download = `${filename}.mbd`;
                      document.body.appendChild(link);
                      link.click();
                      document.body.removeChild(link);
                      URL.revokeObjectURL(url);
                    }}
                    className="flex-1"
                  >
                    Export to Mids (experimental)
                  </Button>
                  <Button
                    variant="secondary"
                    size="sm"
                    onClick={() => { openPrintView(build, levelUpMode); handleClose(); }}
                    className="flex-1"
                  >
                    Print Build
                  </Button>
                  <Button
                    variant="secondary"
                    size="sm"
                    onClick={() => setShowForumExport(true)}
                    className="flex-1"
                  >
                    Forum Post
                  </Button>
                  <Button
                    variant="secondary"
                    size="sm"
                    onClick={() => setShowPopmenu(!showPopmenu)}
                    className="flex-1"
                  >
                    Test Server Popmenu
                  </Button>
                </div>

                {midsExportWarnings.length > 0 && (
                  <div className="rounded border border-amber-600/50 bg-amber-950/30 p-3 text-xs">
                    <p className="font-semibold text-amber-300">
                      {midsExportWarnings.length} thing{midsExportWarnings.length === 1 ? '' : 's'} could not be named for Mids
                    </p>
                    <p className="mt-1 text-amber-200/80">
                      Mids opens a name it does not know as a blank row that still holds the slots, so
                      these are the parts of the build that may arrive empty. Everything else exported normally.
                    </p>
                    <ul className="mt-2 space-y-0.5 text-amber-200/70">
                      {midsExportWarnings.map((w, i) => (
                        /* `slot: 0` is a warning about the power or powerset itself rather than
                           about one of its enhancements — see MidsExportWarning. */
                        <li key={`${w.power}-${w.slot}-${i}`}>
                          {w.slot > 0 ? `${w.power} slot ${w.slot}` : w.power}: {w.detail}
                        </li>
                      ))}
                    </ul>
                  </div>
                )}

                {/* Popmenu expanded section */}
                {showPopmenu && (
                  <div className="space-y-3 border border-gray-700 rounded p-3 bg-gray-800/50">
                    <div className="text-xs text-emerald-300">
                      Generate a <span className="font-semibold">.mnu</span> popmenu file for the test server. Save to <span className="text-sk-magenta font-semibold">Homecoming/data/texts/English/Menus/</span> and use <span className="text-sk-magenta font-semibold">/popmenu {popmenuName || 'YourMenuName'}</span> in-game.
                    </div>
                    <div className="flex gap-2 items-end">
                      <div className="flex-1">
                        <label className="block text-xs font-medium text-gray-400 mb-1">
                          Menu Name
                        </label>
                        <input
                          type="text"
                          value={popmenuName}
                          onChange={(e) => setPopmenuName(e.target.value)}
                          placeholder={build.name?.replace(/[^a-zA-Z0-9]/g, '_') || 'MyBuild'}
                          className="w-full bg-gray-800 border border-gray-600 rounded px-3 py-1.5 text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-[var(--color-ring)] text-sm"
                          maxLength={50}
                        />
                      </div>
                      <Button variant="primary" size="sm" onClick={handlePopmenuSave}>
                        Save .mnu
                      </Button>
                    </div>
                    {popmenuStatus && (
                      <div className="text-xs text-green-300">{popmenuStatus}</div>
                    )}
                    {popmenuError && (
                      <div className="text-xs text-red-300">{popmenuError}</div>
                    )}
                  </div>
                )}
              </div>
            )}
          </div>
        )}
      </ModalBody>

      <ModalFooter>
        <div className="flex gap-2 justify-end">
          <Button variant="secondary" onClick={handleClose}>
            Cancel
          </Button>
          {activeTab === 'share-export' && shareExportSubTab === 'share' ? (
            !shareUrl && (
              <Button
                variant="primary"
                onClick={handleShare}
                isLoading={shareLoading}
                disabled={shareLoading || !build.archetype.id}
              >
                {updateExistingId ? 'Update Build' : 'Share Build'}
              </Button>
            )
          ) : activeTab === 'load-import' ? (
            loadSource === 'mids' ? (
              midsResult?.success ? (
                <Button variant="primary" onClick={handleMidsApply}>
                  Apply Build
                </Button>
              ) : (
                <Button
                  variant="primary"
                  onClick={handleMidsParseFromText}
                  disabled={!midsText.trim()}
                >
                  Parse .mbd
                </Button>
              )
            ) : loadSource === 'game' ? (
              gameResult?.success ? (
                <Button variant="primary" onClick={handleGameApply}>
                  Apply Build
                </Button>
              ) : (
                <Button
                  variant="primary"
                  onClick={handleGameParse}
                  disabled={!gameText.trim()}
                >
                  Parse Export
                </Button>
              )
            ) : null
          ) : null}
        </div>
      </ModalFooter>
    </Modal>
    <ForumExportModal
      isOpen={showForumExport}
      onClose={() => setShowForumExport(false)}
    />
  </>
  );
}
