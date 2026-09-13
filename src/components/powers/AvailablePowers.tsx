/**
 * AvailablePowers component - shows powers available to select
 * Renders as a section within the Available Powers column (not a full column itself)
 */

import { useMemo, useState } from 'react';
import { useBuildStore, useUIStore } from '@/stores';
import { useIsTouchDevice } from '@/hooks';

import { getPowerset, getPowerIconPath, MAX_POWER_PICKS, getPickShadowingInherentPowers, getPowerPicksAtLevel } from '@/data';
import { evaluateRequires, isBuyablePick, setKeyFromId, type RequiresContext } from '@/data/power-requires';
import { resolvePath } from '@/utils/paths';
import { ProcPotentialBadge } from './ProcPotentialBadge';
import type { Power } from '@/types';

/**
 * Set of mode-redirect target INTERNAL NAMES in this powerset. A power the game reaches only
 * by redirecting from a base is not a pick, whichever fork it is on — read from the powerset's
 * own `modeVariants` tables rather than from a list of variant names.
 *
 * This used to be seeded with the slottable members of `GRANTED_POWER_GROUPS` as well. That
 * half is gone: a granted power is one the export marks `AutoIssue`, `isBuyablePick` reads the
 * mark directly now, and over all four forks the hand-written list caught nothing the mark does
 * not (ROSTER-2). The redirect half stays because it is the half the mark misses — Rebirth
 * reaches its Nova attacks by PowerRedirector without ever granting them, 7 powers the export
 * axis alone would offer for sale.
 *
 * Still computed inside the component: the dataset-backed powerset resolves per active server,
 * so this is re-evaluated when `build.serverId` or the set changes.
 */
function redirectTargetNames(powers: Power[]): Set<string> {
  const names = new Set<string>();
  for (const p of powers) {
    for (const variant of Object.values(p.modeVariants ?? {})) {
      if (variant.internalName) names.add(variant.internalName);
    }
  }
  return names;
}

interface AvailablePowersProps {
  powersetId: string | null;
  category: 'primary' | 'secondary';
  selectedPowerNames: string[];
  onSelectPower: (power: Power) => void;
  /** Compact mode: no section header/collapse, tighter rows, used in side-by-side layout */
  compact?: boolean;
}

export interface PowerItemProps {
  power: Power;
  powersetId: string;
  powersetName: string;
  /** Pre-resolved icon path (overrides default getPowerIconPath) */
  iconSrc?: string;
  /** Accent color for hover border */
  accentColor?: 'blue' | 'purple';
  isSelected: boolean;
  isAvailable: boolean;
  isDisabled: boolean;
  isLocked: boolean;
  onSelect: () => void;
  onRemove?: () => void;
  onHover: () => void;
  onLeave: () => void;
  onLockToggle: () => void;
  onShowInfo: (e?: React.MouseEvent) => void;
}

export function PowerItem({
  power,
  powersetId: _powersetId,
  powersetName: _powersetName,
  iconSrc,
  accentColor = 'blue',
  isSelected,
  isAvailable,
  isDisabled,
  isLocked,
  onSelect,
  onRemove,
  onHover,
  onLeave,
  onLockToggle,
  onShowInfo,
}: PowerItemProps) {
  const handleRightClick = (e: React.MouseEvent) => {
    e.preventDefault();
    onLockToggle();
  };

  const handleClick = (e: React.MouseEvent) => {
    if (isSelected && onRemove) {
      onRemove();
    } else if (!isDisabled) {
      onSelect();
    }
    // Drop focus after activation. Chrome/Edge auto-focus role=button divs
    // on click; without this blur, a subsequent Space press on the still-
    // focused row re-fires onSelect and duplicates the power.
    (e.currentTarget as HTMLDivElement).blur();
  };

  const hoverBorderClass = accentColor === 'purple'
    ? 'hover:border-purple-500'
    : 'hover:border-[var(--color-selected)]';
  const infoBtnBgClass = accentColor === 'purple'
    ? 'hover:bg-purple-600/20'
    : 'hover:bg-[var(--color-selected)]/20';
  const infoBtnTextClass = accentColor === 'purple'
    ? 'text-purple-400'
    : 'text-[var(--color-link)]';

  const isTouch = useIsTouchDevice();

  return (
    <div
      onMouseEnter={isTouch ? undefined : onHover}
      onMouseLeave={isTouch ? undefined : onLeave}
      onContextMenu={handleRightClick}
      onClick={handleClick}
      data-info-hover="power"
      title={
        isTouch
          ? (isLocked ? 'Tap ⓘ to unlock' : 'Tap ⓘ for info')
          : (isLocked ? 'Right-click to unlock' : 'Right-click for info')
      }
      className={`
        w-full flex items-center gap-1.5 px-1.5 py-1 rounded-sm
        transition-colors text-left text-xs select-none
        ${
          isLocked
            ? 'border-amber-500 shadow-[0_0_4px_rgba(245,158,11,0.4)] bg-gradient-to-r from-amber-500/10 to-slate-800'
            : isSelected
              ? 'bg-[var(--color-selected)]/25 border border-[var(--color-selected)]/60 hover:border-[var(--color-danger)]/70 cursor-pointer'
              : isDisabled
                ? 'bg-slate-800/50 border border-slate-700/50 opacity-70 cursor-not-allowed'
                : `bg-slate-800 border border-slate-700 ${hoverBorderClass} cursor-pointer`
        }
      `}
      style={{
        WebkitUserSelect: 'none',
        userSelect: 'none',
        WebkitTouchCallout: 'none',
        touchAction: 'manipulation',
      }}
      role="button"
      tabIndex={isDisabled ? -1 : 0}
      onKeyDown={(e) => {
        if (isDisabled) return;
        if (e.key !== 'Enter' && e.key !== ' ') return;
        // Ignore auto-repeat (held key generates a flood of events) — without
        // this, holding Space adds the same power dozens of times per second.
        if (e.repeat) {
          e.preventDefault();
          return;
        }
        e.preventDefault();
        onSelect();
        // Drop focus after activation so a subsequent unrelated Space press
        // (e.g. user scrolling with keys) doesn't re-add the same power.
        (e.currentTarget as HTMLDivElement).blur();
      }}
    >
      {/* Level badge */}
      <span
        className={`text-[10px] font-semibold flex-shrink-0 w-5 text-right pointer-events-none ${
          isAvailable ? 'text-slate-400' : 'text-amber-400'
        }`}
        title={isAvailable ? `Available at level ${power.available + 1}` : `Requires level ${power.available + 1}`}
      >
        {power.available + 1}
      </span>
      {/* Power icon */}
      <img
        src={iconSrc || getPowerIconPath(power.icon)}
        alt=""
        className="w-4 h-4 rounded-sm flex-shrink-0 pointer-events-none"
        draggable={false}
        onError={(e) => {
          (e.target as HTMLImageElement).src = resolvePath('/img/Unknown.png');
        }}
      />
      {/* Power name — wraps to a second line on narrow screens (the
          primary|secondary two-column grid is cramped at ~390px) instead of
          truncating mid-word. Names that fit stay on one line; line-clamp-2
          caps the rare very-long name. */}
      <span className="line-clamp-2 leading-snug flex-1 min-w-0 text-slate-200 pointer-events-none">
        {power.name}
      </span>
      <ProcPotentialBadge power={power} />
      {/* Mobile info button - only visible on small screens */}
      <button
        onClick={onShowInfo}
        className={`lg:hidden flex-shrink-0 w-5 h-5 flex items-center justify-center rounded ${infoBtnBgClass} transition-colors`}
        title="View power info"
        aria-label="View power info"
      >
        <svg className={`w-3.5 h-3.5 ${infoBtnTextClass}`} fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
        </svg>
      </button>
    </div>
  );
}

export function AvailablePowers({
  powersetId,
  category,
  selectedPowerNames,
  onSelectPower,
  compact = false,
}: AvailablePowersProps) {
  const [collapsed, setCollapsed] = useState(false);
  const build = useBuildStore((s) => s.build);
  const removePower = useBuildStore((s) => s.removePower);
  const setInfoPanelContent = useUIStore((s) => s.setInfoPanelContent);
  const lockInfoPanel = useUIStore((s) => s.lockInfoPanel);
  const unlockInfoPanel = useUIStore((s) => s.unlockInfoPanel);
  const infoPanelLocked = useUIStore((s) => s.infoPanel.locked);
  const lockedContent = useUIStore((s) => s.infoPanel.lockedContent);
  const levelUpMode = useUIStore((s) => s.levelUpMode);

  const archetypeId = build.archetype.id;
  const categoryLabel = category === 'primary' ? 'Primary' : 'Secondary';

  // Both powersets must be selected before powers can be chosen
  const bothPowersetsSelected = build.primary.id && build.secondary.id;

  // Check if 24-power limit has been reached (exclude auto-granted form sub-powers)
  const countNonGranted = (powers: { isAutoGranted?: boolean }[]) =>
    powers.filter(p => !p.isAutoGranted).length;
  const totalPicksUsed =
    countNonGranted(build.primary.powers) +
    countNonGranted(build.secondary.powers) +
    build.pools.reduce((sum: number, pool: { powers: { isAutoGranted?: boolean }[] }) => sum + countNonGranted(pool.powers), 0) +
    (build.epicPool ? countNonGranted(build.epicPool.powers) : 0);
  const powerLimitReached = totalPicksUsed >= MAX_POWER_PICKS;

  // Level Up mode: also gate by per-level pick quota. The user cannot pick more
  // powers than their current level grants cumulatively — they must advance first.
  const levelUpPickQuotaReached = levelUpMode && totalPicksUsed >= getPowerPicksAtLevel(build.level);

  const powerset = powersetId ? getPowerset(powersetId) : null;

  // Recomputed when the active server changes (via build.serverId) or the powerset does —
  // the redirect targets are the set's own.
  const redirectTargets = useMemo(
    () => redirectTargetNames(powerset?.powers ?? []),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [build.serverId, powersetId],
  );

  // Build context for requires expression evaluation. Include pool + epic
  // display names so prerequisites that reference them (e.g. "Hover" for
  // Afterburner) resolve regardless of whether the referencing power is
  // itself a pool pick.
  const allSelectedPowerNames = new Set<string>([
    ...build.primary.powers.map(p => p.name),
    ...build.secondary.powers.map(p => p.name),
    ...build.pools.flatMap(pool => pool.powers.map(p => p.name)),
    ...(build.epicPool ? build.epicPool.powers.map(p => p.name) : []),
  ]);

  const requiresContext: RequiresContext = (() => {
    const selectedPowerInternalNames = new Set<string>();
    const selectedPowersetKeys = new Set<string>();

    // Collect internal names from selected powers. Include pool + epic
    // picks so intra-pool prerequisites (e.g. Tough requiring Kick or
    // Boxing from the Fighting pool) can resolve.
    for (const p of build.primary.powers) {
      if (p.internalName) selectedPowerInternalNames.add(p.internalName);
    }
    for (const p of build.secondary.powers) {
      if (p.internalName) selectedPowerInternalNames.add(p.internalName);
    }
    for (const pool of build.pools) {
      for (const p of pool.powers) {
        if (p.internalName) selectedPowerInternalNames.add(p.internalName);
      }
    }
    if (build.epicPool) {
      for (const p of build.epicPool.powers) {
        if (p.internalName) selectedPowerInternalNames.add(p.internalName);
      }
    }

    // Collect the build's powersets under the name a `requires` expression would use for
    // them — the set's INTERNAL name, which for a renamed set is not its id slug (the set
    // that ships as "Spines" is `quills` in every expression that names it).
    for (const id of [build.primary.id, build.secondary.id]) {
      const set = id ? getPowerset(id) : undefined;
      const key = setKeyFromId(id ?? undefined, set?.setPath);
      if (key) selectedPowersetKeys.add(key);
    }
    for (const pool of build.pools) {
      const key = setKeyFromId(pool.id);
      if (key) selectedPowersetKeys.add(key);
    }
    const epicKey = setKeyFromId(build.epicPool?.id);
    if (epicKey) selectedPowersetKeys.add(epicKey);

    return {
      selectedPowerDisplayNames: allSelectedPowerNames,
      selectedPowerInternalNames,
      selectedPowersetKeys,
    };
  })();

  // Inherents that double as powerset picks (the Kheldian travel powers),
  // hidden from the powerset list so they don't show twice. Matched on
  // internalName because Rebirth renames some in both places at once
  // (Shadow_Recall shows as "Starless Recall"), which a display-name match
  // would miss. This must stay the shared list, not the merged
  // getArchetypeInherentPowers: Thunderspy reuses the internal names of its
  // server additions (Hide, Placate) for unrelated Stalker set powers, and
  // filtering on the merged list hid one power from all 28 Stalker sets.
  const archetypeInherentInternalNames = new Set(
    getPickShadowingInherentPowers(archetypeId ?? undefined).map(p => p.internalName)
  );

  // Show ALL user-selectable powers, not just ones available at current level
  // Powers not yet available will be shown as disabled
  // Powers with available === -1 are auto-granted and should not be shown in selection
  // Powers with requires field are hidden when their constraint isn't satisfied
  // Form sub-powers (Kheldian Nova/Dwarf attacks) are auto-granted and hidden
  const allPowers = powerset
    ? powerset.powers.filter(p => {
        // The auto-grant sentinel, GlobalBoost procs, and the set mechanics the game hands
        // over — the three checks that need only the power. Shared with the powerset-pairing
        // reader rather than copied here, which is how the hidden-mechanic check came to be
        // wrong in two places at once (SHOWFLAGS-2).
        if (!isBuyablePick(p)) return false;
        // Filter out mode-redirect targets — reached from a base power, never sold. Match on
        // internalName since variant names use that format (`Bright_Nova_Bolt`); the display
        // name (`Bright Nova Bolt`) wouldn't.
        if (p.internalName && redirectTargets.has(p.internalName)) return false;
        // Filter out powers already granted as archetype inherents
        if (p.internalName && archetypeInherentInternalNames.has(p.internalName)) return false;
        // Evaluate requires expression (handles negation, internal names, powersets)
        if (p.requires && !evaluateRequires(p.requires, requiresContext)) return false;
        return true;
      })
    : [];
  const selectedSet = new Set(selectedPowerNames);

  // Enforce level 1 picks: both primary and secondary must each have at least
  // one power before the user can pick any higher-level powers.
  const primaryHasPower = build.primary.powers.length > 0;
  const secondaryHasPower = build.secondary.powers.length > 0;
  const level1PicksDone = primaryHasPower && secondaryHasPower;

  // While level 1 picks aren't done, restrict to level 1 rules
  const isLevel1 = !level1PicksDone;
  const hasPickedPowerThisCategory = selectedPowerNames.length > 0;
  const otherCategoryHasPower = category === 'primary' ? secondaryHasPower : primaryHasPower;
  const isLevel1BlockedForSecondPick = isLevel1 && hasPickedPowerThisCategory && !otherCategoryHasPower;

  const handlePowerHover = (power: Power) => {
    // Always update hover content - tooltip uses this even when panel is locked
    if (powersetId) {
      setInfoPanelContent({
        type: 'power',
        powerName: power.internalName,
        powerSet: powersetId,
      });
    }
  };

  const handlePowerLeave = () => {
    // Don't clear — keep showing the last-hovered power until a new one is hovered
  };

  const handleShowInfo = (power: Power, e?: React.MouseEvent) => {
    if (e) {
      e.stopPropagation(); // Prevent power selection when clicking info button
    }
    if (!powersetId) return;

    lockInfoPanel({
      type: 'power',
      powerName: power.internalName,
      powerSet: powersetId,
    });
  };

  const handleLockToggle = (power: Power) => {
    if (!powersetId) return;

    // If already locked to this power, unlock; otherwise lock to this power
    if (infoPanelLocked && lockedContent?.type === 'power' && lockedContent.powerName === power.internalName) {
      unlockInfoPanel();
    } else {
      lockInfoPanel({
        type: 'power',
        powerName: power.internalName,
        powerSet: powersetId,
      });
    }
  };

  // Helper to check if a power is the currently locked one
  const isPowerLocked = (powerName: string) => {
    return infoPanelLocked && lockedContent?.type === 'power' && lockedContent.powerName === powerName;
  };

  // No archetype selected
  if (!archetypeId) {
    return (
      <div className="mb-3">
        <div className="text-xs font-semibold text-slate-500 uppercase tracking-wide mb-1">
          {categoryLabel}
        </div>
        <div className="text-xs text-slate-400 italic py-2">
          Select an archetype first
        </div>
      </div>
    );
  }

  // Archetype selected but no powerset
  if (!powersetId) {
    return (
      <div className="mb-3">
        <div className="text-xs font-semibold text-slate-500 uppercase tracking-wide mb-1">
          {categoryLabel}
        </div>
        <div className="text-xs text-slate-400 italic py-2">
          Select a {category} powerset
        </div>
      </div>
    );
  }

  return (
    <div className={compact ? '' : 'mb-3'} {...(category === 'primary' ? { 'data-onboarding': 'add-power' } : {})}>
      {/* Section header - compact mode uses a smaller inline header, normal mode has collapse toggle */}
      {compact ? (
        <div className="flex items-center justify-between px-2 h-8 bg-slate-800/80 border-b border-slate-700">
          <div className="text-[10px] font-semibold text-blue-400 uppercase tracking-wide leading-tight">
            {powerset?.name || categoryLabel}
          </div>
          <span className="text-[9px] text-slate-500">({selectedPowerNames.length})</span>
        </div>
      ) : (
        <div
          className="flex items-center justify-between mb-1 cursor-pointer select-none"
          onClick={() => setCollapsed(!collapsed)}
        >
          <div className="flex items-center gap-1">
            <span className={`text-[10px] text-slate-500 transition-transform ${collapsed ? '' : 'rotate-90'}`}>
              ▶
            </span>
            <div className="text-xs font-semibold text-blue-400 uppercase tracking-wide">
              {powerset?.name || categoryLabel}
            </div>
            <span className="text-[9px] text-slate-600">({selectedPowerNames.length})</span>
          </div>
        </div>
      )}

      {/* Collapsible content (compact mode never collapses - parent section handles that) */}
      {(compact || !collapsed) && (
        <>
          {/* Show message if both powersets not selected */}
          {!bothPowersetsSelected && (
            <div className="text-xs text-amber-400 italic py-1 mb-1">
              Select both Primary and Secondary to choose powers
            </div>
          )}

          {/* Level 1 instruction */}
          {bothPowersetsSelected && isLevel1 && !hasPickedPowerThisCategory && !otherCategoryHasPower && (
            <div className="text-xs text-emerald-400 italic py-1 mb-1">
              Pick 1 of the level 1 powers
            </div>
          )}
          {bothPowersetsSelected && isLevel1 && !hasPickedPowerThisCategory && otherCategoryHasPower && (
            <div className="text-xs text-amber-400/80 italic py-1 mb-1">
              Now pick your {categoryLabel.toLowerCase()} power
            </div>
          )}
          {bothPowersetsSelected && isLevel1 && hasPickedPowerThisCategory && (
            <div className="text-xs text-slate-400 italic py-1 mb-1">
              {categoryLabel} power selected
            </div>
          )}

          {/* Power list */}
          {allPowers.length === 0 ? (
            <div className="text-xs text-slate-400 italic py-1">
              No powers in this powerset
            </div>
          ) : (
            <div className="space-y-0.5">
              {allPowers.map((power) => {
                const isSelected = selectedSet.has(power.name);
                // available is 0-indexed: available=0 means level 1, available=1 means level 2
                // Level 1 special restrictions:
                // - Only level-1-available powers can be selected (some powersets have 3, e.g. Ice Armor's Hoarfrost/Rime mutex)
                // - Can only pick 1 power total from this category
                // - If first pick was from this category, block until other category picks
                const isLevel1Restricted = isLevel1 && (power.available > 0 || hasPickedPowerThisCategory || isLevel1BlockedForSecondPick);

                // Grey out powers that aren't available yet OR are blocked by level 1 enforcement
                const isAvailable = power.available < build.level && !isLevel1Restricted;

                // Check if this power is excluded by an already-selected mutually exclusive power
                const isExcluded = power.excludes?.some(ex => selectedSet.has(
                  allPowers.find(p => p.internalName === ex)?.name ?? ''
                )) ?? false;

                // Block selection until both powersets are chosen, or if 24 powers taken,
                // or (in Level Up mode) if the current-level pick quota is full
                // Selected powers are NOT disabled — clicking them will remove them
                const isDisabled = (!isSelected && (isExcluded || !isAvailable || !bothPowersetsSelected || powerLimitReached || levelUpPickQuotaReached));
                const isLocked = isPowerLocked(power.internalName);

                return (
                  <PowerItem
                    key={power.name}
                    power={power}
                    powersetId={powersetId}
                    powersetName={powerset?.name || ''}
                    isSelected={isSelected}
                    isAvailable={isAvailable}
                    isDisabled={isDisabled}
                    isLocked={isLocked}
                    onSelect={() => onSelectPower(power)}
                    onRemove={() => removePower(category, power.internalName)}
                    onHover={() => handlePowerHover(power)}
                    onLeave={handlePowerLeave}
                    onLockToggle={() => handleLockToggle(power)}
                    onShowInfo={(e) => handleShowInfo(power, e)}
                  />
                );
              })}
            </div>
          )}
        </>
      )}
    </div>
  );
}
