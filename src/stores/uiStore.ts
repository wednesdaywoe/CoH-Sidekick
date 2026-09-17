/**
 * UI Store - manages UI state (modals, settings, tooltips)
 *
 * Uses Zustand for state management.
 * This replaces the legacy global AppState.ui object.
 */

import { create } from 'zustand';
import { persist, createJSONStorage } from 'zustand/middleware';
import type {
  ModalView,
  EnhancementPickerState,
  GenericPickerState,
  OriginPickerState,
  TooltipState,
  TooltipContent,
  InfoPanelState,
  InfoPanelContent,
  StatDisplayConfig,
  PlannerSectionId,
  PlannerSectionConfig,
  PlannerLayoutState,
  EnhancementStatType,
  EnhancementTier,
  IOSetRarity,
  IncarnateSlotId,
  IncarnateActiveState,
  ToggleableIncarnateSlot,
  ArchetypeBranchId,
  Enhancement,
} from '@/types';
import { createDefaultIncarnateActiveState } from '@/types';
import { reconcilePlannerColumns, isPreAtomicSplitCategory } from '@/utils/planner-layout';
import { type ColorThemeId, DEFAULT_COLOR_THEME, applyColorTheme, type ColorMode, DEFAULT_COLOR_MODE, applyColorMode } from '@/data/core/themes';
import type { SlotLevelRef } from '@/utils/slot-levels';
import type { PowerMetric } from '@/utils/calculations/attack-chain';
import { getCommonIOLevels } from '@/data/boost-index';
import { enhancementLevelRange } from '@/utils/calculations';

/** Persisted geometry of a floating window (see `FloatingWindow`). */
export interface FloatingWindowRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

// ============================================
// PROC SETTINGS
// ============================================

/** Per-category toggles for proc effects in dashboard stats */
export interface ProcSettings {
  damage: boolean;
  recovery: boolean;
  regeneration: boolean;
  recharge: boolean;
  toHit: boolean;
  defense: boolean;
  resistance: boolean;
  buildUp: boolean;
  movement: boolean;
}

export type ProcSettingsKey = keyof ProcSettings;

const DEFAULT_PROC_SETTINGS: ProcSettings = {
  damage: true,
  recovery: true,
  regeneration: true,
  recharge: true,
  toHit: true,
  defense: true,
  resistance: true,
  buildUp: true,
  movement: true,
};

// ============================================
// DAMAGE DISPLAY MODE
// ============================================

/**
 * Which damage figure the InfoPanel shows for an attack.
 *   damage         — total damage of one activation (single hit / DoT total)
 *   damagePerAnim  — DPA, damage / cast (animation) time; honors ArcanaTime
 *   damagePerSec   — DPS, damage / full cycle time (cast + recharge)
 *   damagePerEnd   — DPE, damage / endurance cost
 */
export type DamageDisplayMode = 'damage' | 'damagePerAnim' | 'damagePerSec' | 'damagePerEnd';

/**
 * One alternative slotting under test in the Compare Slotting modal.
 *
 * `id` is unique only within a power's own list; the "Current" row the modal
 * renders first is not one of these and reserves id 0.
 */
export interface ComparisonCopy {
  id: number;
  slots: (Enhancement | null)[];
}

// ============================================
// UI STORE INTERFACE
// ============================================

interface UIState {
  /** Enhancement picker modal state */
  enhancementPicker: EnhancementPickerState;

  /** Generic enhancement picker state */
  genericPicker: GenericPickerState;

  /** Origin enhancement picker state */
  originPicker: OriginPickerState;

  /** Stats config modal open state */
  statsConfigModalOpen: boolean;

  /** When set, the modal scrolls to the category containing this stat ID
   *  on open and briefly highlights it. Cleared after the modal consumes it. */
  statsConfigScrollTo: string | null;

  /** Accolades modal open state */
  accoladesModalOpen: boolean;

  /** About modal open state */
  aboutModalOpen: boolean;

  /** AnnouncementModal manual (non-auto) open state — lets the roadmap tab
   *  be opened on demand even when no featurette is unseen. */
  announcementModalOpen: boolean;
  /** Which tab AnnouncementModal should focus when opened manually. */
  announcementInitialTab: 'roadmap' | null;

  /** Donate ("Support Sidekick") modal open state */
  donateModalOpen: boolean;

  /** Incarnate modal open state */
  incarnateModalOpen: boolean;

  /** Incarnate crafting modal open state */
  incarnateCraftingModalOpen: boolean;

  /** Currently selected incarnate slot for modal */
  currentIncarnateSlot: IncarnateSlotId | null;

  /** T4 crafting path combo index per power (keyed by powerId) */
  incarnateT4ComboIndex: Record<string, number>;

  /** Export/Import modal open state */
  exportImportModalOpen: boolean;
  /** Which tab the Export/Import modal should open to */
  exportImportModalTab: 'save' | 'load-import' | 'share-export' | null;

  /** Export-as-Image modal open state */
  buildImageModalOpen: boolean;

  /** Feedback modal open state */
  feedbackModalOpen: boolean;

  /** Changelog modal open state */
  changelogModalOpen: boolean;

  /** Welcome ("What's New") modal open state */
  welcomeModalOpen: boolean;

  /** Enhancement list (shopping list) modal open state */
  enhancementListModalOpen: boolean;

  /** Controls modal open state */
  controlsModalOpen: boolean;

  /** Help modal open state */
  helpModalOpen: boolean;

  /** Detailed Totals modal open state */
  detailedTotalsModalOpen: boolean;

  /** Powerset Compare modal open state */
  powersetCompareModalOpen: boolean;

  /** Set Bonus Lookup modal open state */
  setBonusLookupModalOpen: boolean;

  /** Floating "Set Bonus Totals" popup — a lightweight draggable window that
   *  lists active set-bonus totals with per-source (set + power) drill-down. */
  setBonusPopupOpen: boolean;

  /** Power Info modal open state (mobile only) */
  powerInfoModalOpen: boolean;

  /** Global IO level for calculations */
  globalIOLevel: number;

  /** Attunement toggle */
  attunementEnabled: boolean;

  /**
   * The picker's level-offset default, split by axis because the two are
   * different mechanics off different curves and no enhancement has both (see
   * `enhancementLevelAxis`). One shared number meant the narrower axis clamped
   * the wider one down and nothing ever widened it back — slot a special, and
   * the next IO quietly took +3 instead of the +5 you left it on.
   */
  /** Enhancement Booster combines for IOs. Unsigned; domain from the curves. */
  globalBoostLevel: number;

  /** Relative level for origin/special enhancements. Signed — below even is a penalty. */
  globalRelativeLevel: number;

  /** IO set sort preference in enhancement picker */
  ioSetSortBy: 'name' | 'level';

  /**
   * Per-power memory of the last enhancement-picker filter the user
   * landed on (typeFilter + sidebarFilter). Keyed by the power's
   * `internalName`. Restored when the picker reopens for the same
   * power so e.g. slotting an ATO once makes the next slot on the
   * same power default to the ATO category.
   */
  lastPickerFilterByPower: Record<string, { typeFilter: string; sidebarFilter: string }>;

  /** Exemplar mode - when ON, respects build level for set bonus suppression */
  exemplarMode: boolean;

  /** Exemplar level - the level to exemplar down to (1-50, default: 50) */
  exemplarLevel: number;

  /** Target enemy level offset for hit chance calculation (-7 to +7, 0 = even level) */
  targetLevelOffset: number;

  /**
   * Content-mode toggle for defense-softcap math. Incarnate trial encounters
   * carry an empirical extra ToHit buff on enemies (~+14%), pushing the
   * softcap from 45 → 59 on even-level enemies. Stored separately from
   * `targetLevelOffset` so the two compose cleanly.
   */
  contentMode: 'standard' | 'incarnate';

  /** Per-category proc settings for dashboard stat calculations */
  procSettings: ProcSettings;

  /** Proc settings modal open state */
  procSettingsModalOpen: boolean;

  /** Enhancement Tools modal open state */
  enhancementToolsModalOpen: boolean;

  /** Attack Chain Builder modal open state */
  attackChainModalOpen: boolean;

  /** What-if team-buffs modal open state */
  whatIfBuffsModalOpen: boolean;

  /** Persisted: ids of feature announcements the user has permanently dismissed
   *  ("don't show again"). Keyed by Announcement.id — see data/core/announcements. */
  dismissedAnnouncements: string[];

  /** Which metric ranks powers in the Attack Chain Builder — drives palette
   *  order, bar/chip color intensity, and compactness weighting. */
  chainPowerMetric: PowerMetric;

  /** Show the buff/debuff "active window" bands on the Attack Chain timeline. */
  chainShowEffectWindows: boolean;

  /** Include proc damage in per-power DPS calculations */
  includeProcDamageInDPS: boolean;

  /**
   * Fold that proc damage into the InfoPanel's FINAL damage number instead of
   * annotating it on a line of its own.
   *
   * Off by default, and the default is the honest one: proc damage is FLAT —
   * cap-exempt, untouched by damage strength — so a Final that quietly contains
   * it stops being "your damage × your strength" and its (+%) stops meaning
   * anything. On, for the reader who wants one number to compare powers by.
   */
  foldProcsIntoFinalDamage: boolean;

  /** Use ArcanaTime (server-tick-adjusted cast time) for DPS calculations */
  useArcanaTime: boolean;

  /**
   * How the damage figure is displayed in the InfoPanel:
   *   'damage'         — total damage of one activation
   *   'damagePerAnim'  — damage / cast (animation) time, honoring ArcanaTime
   *   'damagePerSec'   — damage / full cycle time (cast + recharge)
   *   'damagePerEnd'   — damage / endurance cost
   */
  damageDisplayMode: DamageDisplayMode;

  /** Combat mode: suppress defense buffs from stealth/travel powers */
  combatMode: boolean;

  /** Pin the Power Info "Proc Chance" detail section to stay expanded
   *  across power selections. Without this, the section's expansion is
   *  a per-selection local state in the InfoPanel that resets every
   *  time the user clicks a different power. */
  procChancePinned: boolean;

  /** Hints/help visibility */
  hintsEnabled: boolean;

  /** Info panel state */
  infoPanel: InfoPanelState;

  /** Stats display configuration */
  statsConfig: StatDisplayConfig[];

  /** Rearrangeable planner column layout, per view mode (desktop / lg+ only) */
  plannerLayout: PlannerLayoutState;

  /** Persisted position + size of floating windows, keyed by `FloatingWindow` persistKey */
  floatingWindows: Record<string, FloatingWindowRect>;

  /** Tooltip state */
  tooltip: TooltipState;

  /** Collapse the stats dashboard to a slim summary row (toggled by `D` hotkey) */
  dashboardCollapsed: boolean;

  /** App-wide UI zoom scale (0.85 to 1.3, default 1.0) */
  uiScale: number;

  /** Selected color theme. Re-skins the chrome via [data-theme] on <html>. */
  colorTheme: ColorThemeId;

  /** Light/dark mode — orthogonal to colorTheme; flips the ramp via [data-mode]. */
  colorMode: ColorMode;

  /** Incarnate active state - which incarnate slots are active for stat calculations */
  incarnateActive: IncarnateActiveState;

  /** How many of the loadout's EARNED incarnate level shifts to read the build with, or
   *  `null` for all of them. Independent of the per-slot stat toggles: equipping a shifting
   *  slot is what earns the shift, and this says how much of it the content grants.
   *
   *  A full Alpha/Destiny/Lore loadout has earned +3, but only incarnate-flagged content
   *  grants all three; everywhere else the same character fights at a smaller shift, and
   *  reading the build at +3 overstates every purple-patch number. Which content grants what
   *  is a game rule that appears nowhere in the export, so the planner does not derive it —
   *  the player says. Deliberately NOT wired to `contentMode`: that would move a number for a
   *  reason nothing on screen states.
   *
   *  A CEILING, not a magnitude — the engine spends it down the earned grants and can never
   *  exceed them, so a value above what is equipped simply reads as "all of them". Replaces
   *  the boolean `incarnateLevelShiftActive`, which had a setter, a persisted slot and no UI
   *  that ever called it, and which the engine has ignored since the adapter landed. */
  incarnateLevelShift: number | null;

  /** Seconds after cast to evaluate the diminishing Destiny buff at. `null` = auto
   *  = the equipped power's sustained floor (the conservative default a perma-
   *  Destiny build relies on); 0 = additive peak (Mids-style time slider). */
  destinyTime: number | null;

  /** How many foes are standing inside an active Melee Hybrid's sphere. The Melee line
   *  stacks its Regen + Resistance (Core) or Regen + Defense (Radial) buff once per nearby
   *  enemy, up to a ceiling the equipped tier states (4, 7 or 9) and the engine clamps to.
   *  0 = solo, the default: a build's totals should not assume a crowd. */
  hybridTargetsHit: number;

  /** Domination active state - for Dominators to see enhanced mez values */
  dominationActive: boolean;

  /** Scourge active state - for Corruptors to see average Scourge damage bonus */
  scourgeActive: boolean;

  /** Fury level for Brutes (0-100) */
  furyLevel: number;

  /** Supremacy active state - for Masterminds to see henchmen buffs */
  supremacyActive: boolean;

  /** Vigilance team size for Defenders (0 = solo, 1-7 = teammates) */
  vigilanceTeamSize: number;

  /** Critical Hits active state - for Scrappers to see average critical damage bonus */
  criticalHitsActive: boolean;

  /** Stalker Hidden state - whether attacking from Hide */
  stalkerHidden: boolean;

  /** The what-if TEAM-BUFF layer: how much of each stat to pretend a teammate is handing the
   *  build, keyed by the `GlobalBonuses` field name the buff lands in (`damage`, `toHit`,
   *  `recharge`, …). The engine injects it into the accumulators before projection, so the
   *  archetype ceilings bind against it and every surface agrees.
   *
   *  Session-scoped on purpose: deliberately absent from `partialize`, for the same reason
   *  exemplar mode is. A simulated +damage surviving a reload would silently present a
   *  team-buffed number as the build's own, and the whole risk of this feature is exactly
   *  that confusion. */
  whatIfBuffs: Record<string, number>;

  /** Stalker team size for Assassination bonus (0 = solo, 1-7 = teammates) */
  stalkerTeamSize: number;

  /** Stalker critical hits active state - show average crit damage bonus */
  stalkerCritActive: boolean;

  /** Containment active state - for Controllers to see double damage vs controlled targets */
  containmentActive: boolean;

  /** Sentinel critical hits active state */
  sentinelCritActive: boolean;

  /** Selected branch for Arachnos Epic ATs (Soldier: bane-spider/crab-spider, Widow: night-widow/fortunata) */
  selectedBranch: ArchetypeBranchId | null;

  /** Compare Slotting modal */
  compareSlottingOpen: boolean;
  compareSlottingPower: { powerName: string; powerSet: string } | null;
  /** Compare Slotting: the user-made comparison configurations, keyed by
   *  `powerSet::powerName`. Holds only the "Copy 1..N" rows — the "Current"
   *  row is mirrored live from the build every time it is read, so it is
   *  never stored here and can never go stale against actual slotting.
   *
   *  Session-scoped on purpose: deliberately absent from `partialize`.
   *  Persisting it to localStorage would outlive the build it describes,
   *  and this store is global while builds are per-server — saved copies
   *  would surface against a different build, or against a dataset whose
   *  enhancements do not resolve. Surviving close/reopen and power
   *  switching is the part that was actually missing. */
  compareSlottingCopies: Record<string, ComparisonCopy[]>;

  /** Power view mode: 'chronological' (Mids-style, default) or 'category' */
  powerViewMode: 'category' | 'chronological';

  /** Contextual hint text shown at the bottom of the planner (driven by mouseenter on slots/ghosts/etc). Null hides the bar. */
  hoverHint: string | null;

  /** When set, the planner is in "pick a destination slot" mode for a
   *  Mids-style slot-level move: the user chose "Move slot level…" on this
   *  slot and the next slot they click becomes the swap target. Null = idle. */
  slotMoveSource: SlotLevelRef | null;

  /** When set, the planner is in "pick a destination power" mode for relocating
   *  a slot between powers (the "Move slot to another power" gesture): the user
   *  armed this slot and the next eligible power they click receives it. Null =
   *  idle. Mutually exclusive with `slotMoveSource`. */
  slotRelocateSource: SlotLevelRef | null;

  /** Tracked stats — breakdownKey values for stats the user wants to chase via set bonuses */
  trackedStats: string[];

  /** Per-target slider values keyed by power name (0 = buff inactive, 1+ = targets hit) */
  targetsHitValues: Record<string, number>;

  /**
   * Mechanic Adjuster toggle state for `power.conditionalEffects` with
   * `scope: 'per-power'`. Keyed by `<powerInternalName>:<adjusterId>`.
   * Used for target-state mechanics like drowning or Disintegrating that
   * apply to one cast/target at a time.
   */
  mechanicAdjusters: Record<string, boolean>;

  /**
   * Mechanic Adjuster toggle state for `power.conditionalEffects` with
   * `scope: 'global'`. Keyed by just the `<adjusterId>` — flipping a
   * global toggle on one power flips it on every power that references
   * the same id. Used for caster-state mechanics like Bio Armor's
   * Defensive/Offensive/Rested Adaptation, Hide, Domination, and snipe
   * Quick mode (In Combat).
   */
  globalAdjusters: Record<string, boolean>;

  /** Show slot level labels on enhancement slots */
  showSlotLevels: boolean;

  /**
   * Show the proc-potential lens: a badge on powers that are unusually good
   * vehicles for procs at BASE recharge, independent of how they're slotted.
   * Off by default — it's an analysis overlay, not build state.
   */
  showProcPotential: boolean;

  /** Power names being tracked for "perma" (recharge <= duration) */
  permaTrackedPowers: string[];

  /** Level Up mode: gate powers/slots/enhancements by the character's current level */
  levelUpMode: boolean;

  /** Which mobile nav sheet is open (dashboard/menu/settings). Incarnate uses its own modal. */
  mobileSheet: 'dashboard' | 'menu' | 'settings' | null;

  /** Active toast notifications (newest first). */
  toasts: Toast[];

  /** Whether to show the educational banner when a build has any capped
   *  (Rule-of-5-rejected) set bonus. Default on so first-time users learn
   *  what the strikethrough/orange-ring indicators mean. */
  ruleOf5AlertEnabled: boolean;

  /** Display recharge as Mids' speed-multiplier "Haste" (100% base + bonuses,
   *  e.g. +70% Hasten → 170%) when true. When false, show just the bonus
   *  portion (+70%) — matches what the in-game UI displays. Default is
   *  false (game-style); toggle on for Mids parity. */
  rechargeMidsStyle: boolean;
}

export interface ToastAction {
  label: string;
  /** Action handler. Toast is dismissed after this fires unless it returns false. */
  onClick: () => void | false;
}

export interface Toast {
  id: string;
  message: string;
  /** Optional action button shown on the right. */
  action?: ToastAction;
  /** Auto-dismiss after this many ms. 0 = sticky until clicked away. Default 8000. */
  durationMs?: number;
  /** Tone for color/icon. Defaults to 'info'. */
  tone?: 'info' | 'success' | 'warning';
}

interface UIActions {
  // Enhancement Picker Modal
  openEnhancementPicker: (powerName: string, powerSet: string, slotIndex: number, overrideSelect?: (slotIndex: number, enhancement: Enhancement) => void, virtualSlots?: (Enhancement | null)[], powerCategory?: string) => void;
  closeEnhancementPicker: () => void;
  setPickerView: (view: ModalView) => void;
  pushPickerView: (view: ModalView) => void;
  popPickerView: () => void;
  setPickerCategory: (category: IOSetRarity | 'all' | null) => void;
  setSelectedSetId: (setId: string | null) => void;

  // Generic Picker
  setGenericType: (type: EnhancementStatType) => void;

  // Origin Picker
  setOriginType: (type: EnhancementStatType) => void;
  setOriginTier: (tier: EnhancementTier) => void;

  // Settings
  setGlobalIOLevel: (level: number) => void;
  toggleAttunement: () => void;
  setGlobalBoostLevel: (level: number) => void;
  setGlobalRelativeLevel: (level: number) => void;
  setIOSetSortBy: (sort: 'name' | 'level') => void;
  setLastPickerFilter: (powerName: string, typeFilter: string, sidebarFilter: string) => void;
  toggleExemplarMode: () => void;
  setExemplarLevel: (level: number) => void;
  setTargetLevelOffset: (offset: number) => void;
  setContentMode: (mode: 'standard' | 'incarnate') => void;
  toggleProcCategory: (category: ProcSettingsKey) => void;
  setProcSettings: (settings: ProcSettings) => void;
  openProcSettingsModal: () => void;
  closeProcSettingsModal: () => void;
  openEnhancementToolsModal: () => void;
  closeEnhancementToolsModal: () => void;
  openAttackChainModal: () => void;
  openWhatIfBuffsModal: () => void;
  closeWhatIfBuffsModal: () => void;
  closeAttackChainModal: () => void;
  dismissAnnouncement: (id: string) => void;
  setChainPowerMetric: (metric: PowerMetric) => void;
  setChainShowEffectWindows: (show: boolean) => void;
  toggleIncludeProcDamageInDPS: () => void;
  toggleFoldProcsIntoFinalDamage: () => void;
  toggleUseArcanaTime: () => void;
  setDamageDisplayMode: (mode: DamageDisplayMode) => void;
  toggleCombatMode: () => void;
  toggleProcChancePinned: () => void;
  toggleHints: () => void;
  toggleDashboardCollapsed: () => void;
  setUIScale: (scale: number) => void;
  setColorTheme: (theme: ColorThemeId) => void;
  setColorMode: (mode: ColorMode) => void;

  // Info Panel
  setInfoPanelEnabled: (enabled: boolean) => void;
  setInfoPanelContent: (content: InfoPanelContent | null) => void;
  showPowerInfo: (powerName: string, powerSet: string) => void;
  showEnhancementInfo: (enhancementId: string) => void;
  showSetInfo: (setId: string) => void;
  clearInfoPanel: () => void;
  lockInfoPanel: (content: InfoPanelContent) => void;
  unlockInfoPanel: () => void;
  toggleInfoPanelLock: () => void;
  setInfoPanelTooltipEnabled: (enabled: boolean) => void;
  toggleInfoPanelTooltip: () => void;
  undockInfoPanel: () => void;
  dockInfoPanel: () => void;

  // Tooltip
  showTooltip: (content: TooltipContent, x: number, y: number) => void;
  hideTooltip: () => void;
  moveTooltip: (x: number, y: number) => void;

  // Stats Config
  openStatsConfigModal: (scrollToStatId?: string) => void;
  closeStatsConfigModal: () => void;
  /** Called by the modal once it has acted on the scroll-to hint, so the
   *  next open without an explicit hint doesn't replay the old one. */
  clearStatsConfigScrollTo: () => void;
  setStatVisible: (stat: string, visible: boolean) => void;
  reorderStats: (stats: StatDisplayConfig[]) => void;
  resetStatsConfig: () => void;

  /** Replace the ordered section list for a planner view mode (drag reorder) */
  reorderPlannerSections: (view: keyof PlannerLayoutState, sections: PlannerSectionConfig[]) => void;
  /** Show/hide a single planner section in a view mode */
  setPlannerSectionVisible: (view: keyof PlannerLayoutState, id: PlannerSectionId, visible: boolean) => void;
  /** Set horizontal fr-weights for one or more columns (drag-to-resize). Merges
   *  the given `{ id: weight }` map onto the existing layout, leaving other
   *  columns' weights untouched. */
  setPlannerSectionWeights: (view: keyof PlannerLayoutState, weights: Partial<Record<PlannerSectionId, number>>) => void;
  /** Set vertical fr-weights (`rowWeight`) for one or more stacked sections
   *  (LAY11 vertical resize). Same merge semantics as `setPlannerSectionWeights`
   *  but on the within-column axis. */
  setPlannerSectionRowWeights: (view: keyof PlannerLayoutState, rowWeights: Partial<Record<PlannerSectionId, number>>) => void;
  /** Toggle a single planner section between collapsed (header only) and expanded. */
  togglePlannerSectionCollapsed: (view: keyof PlannerLayoutState, id: PlannerSectionId) => void;
  /** Restore a view mode's columns to the default order + visibility */
  resetPlannerLayout: (view: keyof PlannerLayoutState) => void;

  /** Persist a floating window's position + size (keyed by its `persistKey`) */
  setFloatingWindow: (key: string, rect: FloatingWindowRect) => void;

  // Accolades Modal
  openAccoladesModal: () => void;
  closeAccoladesModal: () => void;

  // About Modal
  openAboutModal: () => void;
  closeAboutModal: () => void;

  // Announcement Modal (manual open / roadmap tab)
  openAnnouncementModal: (tab?: 'roadmap') => void;
  closeAnnouncementModal: () => void;

  // Donate ("Support Sidekick") Modal
  openDonateModal: () => void;
  closeDonateModal: () => void;

  // Incarnate Modal
  openIncarnateModal: (slotId?: IncarnateSlotId) => void;
  closeIncarnateModal: () => void;
  setCurrentIncarnateSlot: (slotId: IncarnateSlotId) => void;

  // Incarnate Crafting Modal
  openIncarnateCraftingModal: () => void;
  closeIncarnateCraftingModal: () => void;
  setIncarnateT4ComboIndex: (powerId: string, index: number) => void;

  // Export/Import Modal
  openExportImportModal: (tab?: 'save' | 'load-import' | 'share-export') => void;
  closeExportImportModal: () => void;

  // Export-as-Image Modal
  openBuildImageModal: () => void;
  closeBuildImageModal: () => void;

  // Feedback Modal
  openFeedbackModal: () => void;
  closeFeedbackModal: () => void;

  // Changelog Modal
  openChangelogModal: () => void;
  closeChangelogModal: () => void;

  // Welcome Modal ("What's New")
  openWelcomeModal: () => void;
  closeWelcomeModal: () => void;
  openEnhancementListModal: () => void;
  closeEnhancementListModal: () => void;

  // Controls Modal
  openControlsModal: () => void;
  closeControlsModal: () => void;

  // Help Modal
  /** Optional topic id to expand + scroll to when the help modal opens.
   *  Cleared on close so subsequent opens start fresh. */
  helpModalInitialTopic: string | null;
  openHelpModal: (initialTopicId?: string) => void;
  closeHelpModal: () => void;

  // Detailed Totals Modal
  openDetailedTotalsModal: () => void;
  closeDetailedTotalsModal: () => void;

  // Powerset Compare Modal
  openPowersetCompareModal: () => void;
  closePowersetCompareModal: () => void;

  // Set Bonus Lookup Modal
  openSetBonusLookupModal: () => void;
  closeSetBonusLookupModal: () => void;

  // Set Bonus Totals popup (floating)
  openSetBonusPopup: () => void;
  closeSetBonusPopup: () => void;

  // Power Info Modal (mobile only)
  openPowerInfoModal: () => void;
  closePowerInfoModal: () => void;

  // Incarnate Active State
  toggleIncarnateActive: (slotId: ToggleableIncarnateSlot) => void;
  setIncarnateActive: (slotId: ToggleableIncarnateSlot, active: boolean) => void;
  resetIncarnateActive: () => void;
  /** Set the incarnate level-shift ceiling (`null` = every earned shift). */
  setIncarnateLevelShift: (shift: number | null) => void;
  /** Set the Destiny time-slider position (seconds after cast; `null` = auto floor). */
  setDestinyTime: (seconds: number | null) => void;
  /** Set how many foes are in the Melee Hybrid's sphere (clamped at 0; the engine applies
   *  the equipped tier's own ceiling). */
  setHybridTargetsHit: (foes: number) => void;

  // Domination Active State (Dominator inherent)
  toggleDomination: () => void;
  setDominationActive: (active: boolean) => void;

  // Scourge Active State (Corruptor inherent)
  toggleScourge: () => void;
  setScourgeActive: (active: boolean) => void;

  // Fury Level (Brute inherent) - slider 0-100
  setFuryLevel: (level: number) => void;

  // Supremacy Active State (Mastermind inherent)
  toggleSupremacy: () => void;
  setSupremacyActive: (active: boolean) => void;

  // Vigilance Team Size (Defender inherent) - slider 0-7
  setVigilanceTeamSize: (size: number) => void;

  // Critical Hits Active State (Scrapper inherent)
  toggleCriticalHits: () => void;
  setCriticalHitsActive: (active: boolean) => void;

  // Stalker Hidden State (Stalker inherent)
  toggleStalkerHidden: () => void;
  setStalkerHidden: (hidden: boolean) => void;

  /** Set one what-if entry. A magnitude of 0 REMOVES it, so "is anything simulated?" stays a
   *  plain key count rather than a scan for non-zero values. */
  setWhatIfBuff: (stat: string, magnitude: number) => void;
  /** Drop the whole what-if layer. */
  clearWhatIfBuffs: () => void;

  // Stalker Team Size (Stalker inherent) - slider 0-7
  setStalkerTeamSize: (size: number) => void;

  // Stalker Crit Active State (Stalker inherent)
  toggleStalkerCrit: () => void;

  // Containment Active State (Controller inherent)
  toggleContainment: () => void;
  setContainmentActive: (active: boolean) => void;

  // Opportunity Level (Sentinel inherent) - slider 0-100

  // Sentinel Critical Hits Active State (Sentinel inherent)
  toggleSentinelCrit: () => void;
  setSentinelCritActive: (active: boolean) => void;

  // Arachnos Branch Selection (Epic ATs)
  /**
   * Point the picker at a branch WITHOUT touching the build — for the paths that follow the
   * build rather than change it (import, rehydrate, undo). Changing branch is a build edit
   * that drops the old branch's picks: use `buildStore.switchBranch`.
   */
  setSelectedBranch: (branch: ArchetypeBranchId | null) => void;
  clearSelectedBranch: () => void;

  // Compare Slotting Modal
  openCompareSlotting: (powerName?: string, powerSet?: string) => void;
  closeCompareSlotting: () => void;
  /** Replace a power's saved comparison copies. An empty list drops the entry
   *  entirely, so powers the user has stopped comparing don't accumulate.
   *  Takes an updater as well as a list: a multi-slot pick writes once per
   *  piece in one tick, and a caller passing a list computed from its own
   *  render snapshot would have every write but the last overwritten. */
  setCompareSlottingCopies: (
    key: string,
    copies: ComparisonCopy[] | ((prev: ComparisonCopy[]) => ComparisonCopy[])
  ) => void;
  clearCompareSlottingCopies: () => void;

  // Power View Mode
  setPowerViewMode: (mode: 'category' | 'chronological') => void;
  togglePowerViewMode: () => void;

  // Hover hint (contextual help text at bottom of planner)
  setHoverHint: (hint: string | null) => void;

  // Slot-level move (Mids-style): arm a source slot, then click a destination
  beginSlotLevelMove: (source: SlotLevelRef) => void;
  cancelSlotLevelMove: () => void;

  // Slot relocation (move a slot between powers): arm a source slot, then click
  // a destination power. Mutually exclusive with the slot-level move above.
  beginSlotRelocate: (source: SlotLevelRef) => void;
  cancelSlotRelocate: () => void;

  // Tracked Stats
  toggleTrackedStat: (breakdownKey: string) => void;
  ensureTrackedStats: (keys: string[]) => void;
  clearTrackedStats: () => void;

  // Per-target slider
  setTargetsHit: (powerName: string, value: number) => void;
  /** Bulk-replace the targets-hit map. Used after a Mids import to copy
   *  the .mbd file's per-power `VariableValue` sliders in one shot so the
   *  dashboard reproduces Mids' totals without per-power toggling. */
  setTargetsHitBulk: (values: Record<string, number>) => void;

  // Mechanic Adjuster toggle (per-power conditional effect)
  setMechanicAdjuster: (powerName: string, adjusterId: string, active: boolean) => void;
  toggleMechanicAdjuster: (powerName: string, adjusterId: string) => void;
  /** Clear all toggles for a single power (e.g. on power deselection). */
  clearMechanicAdjusters: (powerName: string) => void;

  // Global Mechanic Adjusters (caster-state — apply across all powers)
  setGlobalAdjuster: (adjusterId: string, active: boolean) => void;
  toggleGlobalAdjuster: (adjusterId: string) => void;
  /**
   * Activate a single member of a mutually-exclusive group (Bio Armor
   * adaptations, Tidal Power stacks, Combo Levels). Sets the named id to
   * true; sets every other id in `siblingIds` to false in one update.
   * Pass `null` for `activeId` to clear the whole group.
   */
  setGlobalAdjusterGroup: (
    activeId: string | null,
    siblingIds: readonly string[],
  ) => void;

  // Slot level labels
  toggleShowSlotLevels: () => void;

  // Proc-potential lens
  toggleShowProcPotential: () => void;

  // Perma tracker
  togglePermaTracked: (powerName: string) => void;

  // Level Up mode
  toggleLevelUpMode: () => void;
  setLevelUpMode: (enabled: boolean) => void;

  // Mobile bottom-nav sheets
  openMobileSheet: (sheet: 'dashboard' | 'menu' | 'settings') => void;
  closeMobileSheet: () => void;

  // Toasts
  showToast: (toast: Omit<Toast, 'id'>) => string;
  dismissToast: (id: string) => void;
  clearToasts: () => void;
  setRuleOf5AlertEnabled: (enabled: boolean) => void;
  toggleRuleOf5AlertEnabled: () => void;
  setRechargeMidsStyle: (enabled: boolean) => void;
  toggleRechargeMidsStyle: () => void;

  // Hard reset of build-specific UI state (for New Build)
  resetForNewBuild: () => void;
}

type UIStore = UIState & UIActions;

// ============================================
// DEFAULT STATES
// ============================================

const defaultEnhancementPicker: EnhancementPickerState = {
  isOpen: false,
  currentView: 'category',
  viewStack: [],
  currentPowerName: null,
  currentPowerSet: null,
  currentPowerCategory: null,
  currentSlotIndex: 0,
  currentCategory: null,
  selectedSetId: null,
  onOverrideSelect: null,
  virtualSlots: null,
};

const defaultGenericPicker: GenericPickerState = {
  selectedType: 'Damage',
};

const defaultOriginPicker: OriginPickerState = {
  selectedType: 'Damage',
  selectedTier: 'SO',
};

const defaultTooltip: TooltipState = {
  visible: false,
  content: null,
  position: { x: 0, y: 0 },
};

const defaultInfoPanel: InfoPanelState = {
  enabled: true,
  content: null,
  locked: false,
  lockedContent: null,
  tooltipEnabled: false,
  undocked: false,
};

const defaultStatsConfig: StatDisplayConfig[] = [
  { stat: 'damage', visible: true, order: 0 },
  { stat: 'accuracy', visible: true, order: 1 },
  { stat: 'tohit', visible: true, order: 2 },
  { stat: 'recharge', visible: true, order: 3 },
  { stat: 'health', visible: true, order: 4 },
  { stat: 'regeneration', visible: true, order: 5 },
  { stat: 'recovery', visible: true, order: 6 },
  { stat: 'endcost', visible: true, order: 7 },
  { stat: 'netend', visible: true, order: 8 },
  { stat: 'level_shift', visible: true, order: 9 },
  { stat: 'defense_melee', visible: true, order: 10 },
  { stat: 'defense_ranged', visible: true, order: 11 },
  { stat: 'res_smashing', visible: true, order: 12 },
];

// Default planner column arrangement. Order = left-to-right column order and
// reproduces the historical fixed layout (all sections visible). Chronological's
// "Powers by Level" grid keeps its 2fr width via `weight`.
const defaultPlannerLayout: PlannerLayoutState = {
  category: [
    // 5-column default tuned for space use: atomic sections stack per column,
    // content-first sizing (goal 1) letting each read as its own tile and some
    // start collapsed to keep tall columns compact.
    // Col 0: Available
    { id: 'available', visible: true, column: 0 },
    // Col 1: Primary + Fitness
    { id: 'primary', visible: true, column: 1 },
    { id: 'inherent-fitness', visible: true, column: 1 },
    // Col 2: Secondary + Basic (collapsed) + Archetype + Prestige Sprints (collapsed)
    { id: 'secondary', visible: true, column: 2 },
    { id: 'inherent-basic', visible: true, column: 2, collapsed: true },
    { id: 'inherent-archetype', visible: true, column: 2 },
    { id: 'inherent-prestige', visible: true, column: 2, collapsed: true },
    // Col 3: Epic + Pool
    { id: 'epic', visible: true, column: 3 },
    { id: 'pool', visible: true, column: 3 },
    // Col 4: Info
    { id: 'info', visible: true, column: 4 },
  ],
  chronological: [
    { id: 'available', visible: true, column: 0 },
    { id: 'bylevel', visible: true, weight: 2, column: 1 },
    { id: 'info', visible: true, column: 2 },
  ],
};

// ============================================
// STORE CREATION
// ============================================

export const useUIStore = create<UIStore>()(
  persist(
    (set, get) => ({
      // Initial state
      enhancementPicker: defaultEnhancementPicker,
      genericPicker: defaultGenericPicker,
      originPicker: defaultOriginPicker,
      statsConfigModalOpen: false,
      statsConfigScrollTo: null,
      accoladesModalOpen: false,
      aboutModalOpen: false,
      announcementModalOpen: false,
      announcementInitialTab: null,
      donateModalOpen: false,
      incarnateModalOpen: false,
      incarnateCraftingModalOpen: false,
      currentIncarnateSlot: null,
      incarnateT4ComboIndex: {},
      exportImportModalOpen: false,
      exportImportModalTab: null,
      buildImageModalOpen: false,
      feedbackModalOpen: false,
      changelogModalOpen: false,
      welcomeModalOpen: false,
      enhancementListModalOpen: false,
      controlsModalOpen: false,
      helpModalOpen: false,
      helpModalInitialTopic: null,
      detailedTotalsModalOpen: false,
      powersetCompareModalOpen: false,
      setBonusLookupModalOpen: false,
      setBonusPopupOpen: false,
      powerInfoModalOpen: false,
      globalIOLevel: 50,
      attunementEnabled: false,
      globalBoostLevel: 0,
      globalRelativeLevel: 0,
      ioSetSortBy: 'name' as const,
      lastPickerFilterByPower: {},
      exemplarMode: false,
      exemplarLevel: 50,
      targetLevelOffset: 0,
      contentMode: 'standard' as const,
      procSettings: { ...DEFAULT_PROC_SETTINGS },
      procSettingsModalOpen: false,
      enhancementToolsModalOpen: false,
      attackChainModalOpen: false,
      whatIfBuffsModalOpen: false,
      dismissedAnnouncements: [],
      chainPowerMetric: 'damage' as PowerMetric,
      chainShowEffectWindows: true,
      includeProcDamageInDPS: true,
      foldProcsIntoFinalDamage: false,
      useArcanaTime: true,
      damageDisplayMode: 'damage' as DamageDisplayMode,
      combatMode: false,
      procChancePinned: false,
      hintsEnabled: true,
      infoPanel: defaultInfoPanel,
      statsConfig: defaultStatsConfig,
      plannerLayout: defaultPlannerLayout,
      floatingWindows: {},
      tooltip: defaultTooltip,
      dashboardCollapsed: false,
      uiScale: 1.0,
      colorTheme: DEFAULT_COLOR_THEME,
      colorMode: DEFAULT_COLOR_MODE,
      incarnateActive: createDefaultIncarnateActiveState(),
      incarnateLevelShift: null,
      destinyTime: null,
      hybridTargetsHit: 0,
      dominationActive: false,
      scourgeActive: false,
      furyLevel: 75, // Default to 75 fury (reasonable combat average)
      supremacyActive: true, // Default to ON since henchmen are typically nearby
      vigilanceTeamSize: 0, // Default to solo (0 teammates) for max damage bonus
      criticalHitsActive: false, // Default to OFF (like Scourge)
      stalkerHidden: false, // Default to not hidden (showing out-of-hide damage)
      whatIfBuffs: {}, // Nothing simulated until the user asks for it
      stalkerTeamSize: 0, // Default to solo (0 teammates)
      stalkerCritActive: false, // Default to OFF (like Critical Hits)
      containmentActive: false, // Default to OFF (like Critical Hits)
      sentinelCritActive: false, // Default to OFF (like Critical Hits)
      selectedBranch: null, // No branch selected by default
      compareSlottingOpen: false,
      compareSlottingPower: null,
      compareSlottingCopies: {},
      powerViewMode: 'category', // powerViewMode: 'category' | 'chronological';
      hoverHint: null,
      slotMoveSource: null,
      slotRelocateSource: null,
      trackedStats: [], // No tracked stats by default
      targetsHitValues: {}, // No per-target overrides by default
      mechanicAdjusters: {}, // No per-power conditional toggles overridden by default
      globalAdjusters: {}, // No global conditional toggles overridden by default
      showSlotLevels: true, // Show slot level labels by default
      showProcPotential: false, // Analysis overlay — opt in
      permaTrackedPowers: [], // No perma-tracked powers by default
      levelUpMode: false, // Off by default — classic "respec" flow
      mobileSheet: null,
      toasts: [],
      ruleOf5AlertEnabled: true,
      rechargeMidsStyle: false,

      // Enhancement Picker Modal
      openEnhancementPicker: (powerName, powerSet, slotIndex, overrideSelect, virtualSlots, powerCategory) =>
        set({
          enhancementPicker: {
            ...defaultEnhancementPicker,
            isOpen: true,
            currentPowerName: powerName,
            currentPowerSet: powerSet,
            currentPowerCategory: powerCategory ?? null,
            currentSlotIndex: slotIndex,
            onOverrideSelect: overrideSelect ?? null,
            virtualSlots: virtualSlots ?? null,
          },
        }),

      closeEnhancementPicker: () =>
        set({
          enhancementPicker: defaultEnhancementPicker,
        }),

      setPickerView: (view) =>
        set((state) => ({
          enhancementPicker: {
            ...state.enhancementPicker,
            currentView: view,
          },
        })),

      pushPickerView: (view) =>
        set((state) => ({
          enhancementPicker: {
            ...state.enhancementPicker,
            viewStack: [...state.enhancementPicker.viewStack, state.enhancementPicker.currentView],
            currentView: view,
          },
        })),

      popPickerView: () =>
        set((state) => {
          const viewStack = [...state.enhancementPicker.viewStack];
          const previousView = viewStack.pop() || 'category';
          return {
            enhancementPicker: {
              ...state.enhancementPicker,
              viewStack,
              currentView: previousView,
            },
          };
        }),

      setPickerCategory: (category) =>
        set((state) => ({
          enhancementPicker: {
            ...state.enhancementPicker,
            currentCategory: category,
          },
        })),

      setSelectedSetId: (setId) =>
        set((state) => ({
          enhancementPicker: {
            ...state.enhancementPicker,
            selectedSetId: setId,
          },
        })),

      // Generic Picker
      setGenericType: (type) =>
        set((state) => ({
          genericPicker: {
            ...state.genericPicker,
            selectedType: type,
          },
        })),

      // Origin Picker
      setOriginType: (type) =>
        set((state) => ({
          originPicker: {
            ...state.originPicker,
            selectedType: type,
          },
        })),

      setOriginTier: (tier) =>
        set((state) => ({
          originPicker: {
            ...state.originPicker,
            selectedTier: tier,
          },
        })),

      // Settings
      // Clamped to the crafted-record roster the export names, not to a typed
      // band: 51-53 were never recipes, and a persisted one from before this
      // read the export comes back as the ceiling (BOOST-6).
      setGlobalIOLevel: (level) => {
        const craftLevels = getCommonIOLevels();
        set({
          globalIOLevel: Math.max(
            craftLevels[0],
            Math.min(craftLevels[craftLevels.length - 1], level),
          ),
        });
      },

      toggleAttunement: () =>
        set((state) => ({
          attunementEnabled: !state.attunementEnabled,
        })),

      // `Math.max(0, Math.min(5, level))` until 2026-09-17. Both ends were wrong:
      // 5 is a number the export owns (`boosters.length - 1`, and the datasets
      // disagree), and the 0 floored the whole signed half of the OTHER axis that
      // was sharing this setter — every below-even relative level the spinner
      // offered was stored as even, so an under-level SO could not be placed from
      // the picker while the dial claimed it had been.
      setGlobalBoostLevel: (level) => {
        const { min, max } = enhancementLevelRange('io-set');
        set({ globalBoostLevel: Math.max(min, Math.min(max, level)) });
      },

      setGlobalRelativeLevel: (level) => {
        const { min, max } = enhancementLevelRange('origin');
        set({ globalRelativeLevel: Math.max(min, Math.min(max, level)) });
      },

      setIOSetSortBy: (sort) =>
        set({ ioSetSortBy: sort }),

      setLastPickerFilter: (powerName, typeFilter, sidebarFilter) =>
        set((state) => ({
          lastPickerFilterByPower: {
            ...state.lastPickerFilterByPower,
            [powerName]: { typeFilter, sidebarFilter },
          },
        })),

      toggleExemplarMode: () =>
        set((state) => ({
          exemplarMode: !state.exemplarMode,
        })),

      setExemplarLevel: (level) =>
        set({
          exemplarLevel: Math.max(1, Math.min(50, level)),
        }),

      setTargetLevelOffset: (offset) =>
        set({
          // HC supports enemy levels up to +7 above the player (incarnate
          // trial bosses, +4-shifted AVs in late-Story arcs, +7 in some
          // hard mode content). The defense softcap shifts +5% per
          // enemy level above, so the offset range needs to cover the
          // full span for the dashboard cap to reflect reality.
          targetLevelOffset: Math.max(-7, Math.min(7, offset)),
        }),

      setContentMode: (mode) => set({ contentMode: mode }),

      toggleProcCategory: (category: ProcSettingsKey) =>
        set((state) => ({
          procSettings: {
            ...state.procSettings,
            [category]: !state.procSettings[category],
          },
        })),

      setProcSettings: (settings: ProcSettings) =>
        set({ procSettings: settings }),

      openProcSettingsModal: () =>
        set({ procSettingsModalOpen: true }),

      closeProcSettingsModal: () =>
        set({ procSettingsModalOpen: false }),

      openEnhancementToolsModal: () =>
        set({ enhancementToolsModalOpen: true }),

      closeEnhancementToolsModal: () =>
        set({ enhancementToolsModalOpen: false }),

      openAttackChainModal: () =>
        set({ attackChainModalOpen: true }),

      openWhatIfBuffsModal: () =>
        set({ whatIfBuffsModalOpen: true }),

      closeWhatIfBuffsModal: () =>
        set({ whatIfBuffsModalOpen: false }),

      closeAttackChainModal: () =>
        set({ attackChainModalOpen: false }),

      dismissAnnouncement: (id: string) =>
        set((state) =>
          state.dismissedAnnouncements.includes(id)
            ? state
            : { dismissedAnnouncements: [...state.dismissedAnnouncements, id] }
        ),

      setChainPowerMetric: (metric) => set({ chainPowerMetric: metric }),
      setChainShowEffectWindows: (show) => set({ chainShowEffectWindows: show }),

      toggleIncludeProcDamageInDPS: () =>
        set((state) => ({
          includeProcDamageInDPS: !state.includeProcDamageInDPS,
        })),

      toggleFoldProcsIntoFinalDamage: () =>
        set((state) => ({
          foldProcsIntoFinalDamage: !state.foldProcsIntoFinalDamage,
        })),

      toggleCombatMode: () =>
        set((state) => ({
          combatMode: !state.combatMode,
        })),

      toggleProcChancePinned: () =>
        set((state) => ({
          procChancePinned: !state.procChancePinned,
        })),

      toggleUseArcanaTime: () =>
        set((state) => ({
          useArcanaTime: !state.useArcanaTime,
        })),

      setDamageDisplayMode: (mode) => set({ damageDisplayMode: mode }),

      toggleHints: () =>
        set((state) => ({
          hintsEnabled: !state.hintsEnabled,
        })),

      toggleDashboardCollapsed: () =>
        set((state) => ({
          dashboardCollapsed: !state.dashboardCollapsed,
        })),

      setUIScale: (scale: number) =>
        set({ uiScale: Math.max(0.85, Math.min(1.3, scale)) }),

      setColorTheme: (theme: ColorThemeId) => {
        applyColorTheme(theme);
        set({ colorTheme: theme });
      },
      setColorMode: (mode: ColorMode) => {
        applyColorMode(mode);
        set({ colorMode: mode });
      },

      // Info Panel
      setInfoPanelEnabled: (enabled) =>
        set((state) => ({
          infoPanel: {
            ...state.infoPanel,
            enabled,
          },
        })),

      setInfoPanelContent: (content) =>
        set((state) => ({
          infoPanel: {
            ...state.infoPanel,
            content,
          },
        })),

      showPowerInfo: (powerName, powerSet) =>
        set((state) => ({
          infoPanel: {
            ...state.infoPanel,
            content: { type: 'power', powerName, powerSet },
          },
        })),

      showEnhancementInfo: (enhancementId) =>
        set((state) => ({
          infoPanel: {
            ...state.infoPanel,
            content: { type: 'enhancement', enhancementId },
          },
        })),

      showSetInfo: (setId) =>
        set((state) => ({
          infoPanel: {
            ...state.infoPanel,
            content: { type: 'set', setId },
          },
        })),

      clearInfoPanel: () =>
        set((state) => ({
          infoPanel: {
            ...state.infoPanel,
            content: null,
          },
        })),

      lockInfoPanel: (content) =>
        set((state) => {
          // On mobile/tablet (<= 1024px), also open the power info modal
          const isMobile = typeof window !== 'undefined' && window.innerWidth <= 1024;
          return {
            infoPanel: {
              ...state.infoPanel,
              locked: true,
              lockedContent: content,
            },
            powerInfoModalOpen: isMobile || state.powerInfoModalOpen,
          };
        }),

      unlockInfoPanel: () =>
        set((state) => ({
          infoPanel: {
            ...state.infoPanel,
            locked: false,
            lockedContent: null,
          },
        })),

      toggleInfoPanelLock: () =>
        set((state) => {
          const isMobile = typeof window !== 'undefined' && window.innerWidth <= 1024;

          if (state.infoPanel.locked) {
            return {
              infoPanel: {
                ...state.infoPanel,
                locked: false,
                lockedContent: null,
              },
              // Close modal when unlocking on mobile
              powerInfoModalOpen: isMobile ? false : state.powerInfoModalOpen,
            };
          }
          // Lock with current content
          return {
            infoPanel: {
              ...state.infoPanel,
              locked: true,
              lockedContent: state.infoPanel.content,
            },
            // Open modal when locking on mobile
            powerInfoModalOpen: isMobile || state.powerInfoModalOpen,
          };
        }),

      setInfoPanelTooltipEnabled: (enabled) =>
        set((state) => ({
          infoPanel: {
            ...state.infoPanel,
            tooltipEnabled: enabled,
          },
        })),

      toggleInfoPanelTooltip: () =>
        set((state) => ({
          infoPanel: {
            ...state.infoPanel,
            tooltipEnabled: !state.infoPanel.tooltipEnabled,
          },
        })),

      undockInfoPanel: () =>
        set((state) => ({
          infoPanel: {
            ...state.infoPanel,
            undocked: true,
          },
        })),

      dockInfoPanel: () =>
        set((state) => ({
          infoPanel: {
            ...state.infoPanel,
            undocked: false,
          },
        })),

      // Tooltip
      showTooltip: (content, x, y) =>
        set({
          tooltip: {
            visible: true,
            content,
            position: { x, y },
          },
        }),

      hideTooltip: () =>
        set({
          tooltip: {
            ...get().tooltip,
            visible: false,
          },
        }),

      moveTooltip: (x, y) =>
        set((state) => ({
          tooltip: {
            ...state.tooltip,
            position: { x, y },
          },
        })),

      // Stats Config
      openStatsConfigModal: (scrollToStatId?: string) =>
        set({ statsConfigModalOpen: true, statsConfigScrollTo: scrollToStatId ?? null }),

      closeStatsConfigModal: () =>
        set({ statsConfigModalOpen: false }),

      clearStatsConfigScrollTo: () =>
        set({ statsConfigScrollTo: null }),

      // About Modal
      openAboutModal: () =>
        set({ aboutModalOpen: true }),

      closeAboutModal: () =>
        set({ aboutModalOpen: false }),

      // Announcement Modal (manual open / roadmap tab)
      openAnnouncementModal: (tab?: 'roadmap') =>
        set({ announcementModalOpen: true, announcementInitialTab: tab ?? null }),

      closeAnnouncementModal: () =>
        set({ announcementModalOpen: false, announcementInitialTab: null }),

      // Donate ("Support Sidekick") Modal
      openDonateModal: () =>
        set({ donateModalOpen: true }),

      closeDonateModal: () =>
        set({ donateModalOpen: false }),

      setStatVisible: (stat, visible) =>
        set((state) => ({
          statsConfig: state.statsConfig.map((s) =>
            s.stat === stat ? { ...s, visible } : s
          ),
        })),

      reorderStats: (stats) =>
        set({
          statsConfig: stats,
        }),

      resetStatsConfig: () =>
        set({
          statsConfig: defaultStatsConfig,
        }),

      reorderPlannerSections: (view, sections) =>
        set((state) => ({
          plannerLayout: { ...state.plannerLayout, [view]: sections },
        })),

      setPlannerSectionVisible: (view, id, visible) =>
        set((state) => ({
          plannerLayout: {
            ...state.plannerLayout,
            [view]: state.plannerLayout[view].map((s) =>
              s.id === id ? { ...s, visible } : s
            ),
          },
        })),

      setPlannerSectionWeights: (view, weights) =>
        set((state) => ({
          plannerLayout: {
            ...state.plannerLayout,
            [view]: state.plannerLayout[view].map((s) =>
              weights[s.id] !== undefined ? { ...s, weight: weights[s.id] } : s
            ),
          },
        })),

      setPlannerSectionRowWeights: (view, rowWeights) =>
        set((state) => ({
          plannerLayout: {
            ...state.plannerLayout,
            [view]: state.plannerLayout[view].map((s) =>
              rowWeights[s.id] !== undefined ? { ...s, rowWeight: rowWeights[s.id] } : s
            ),
          },
        })),

      togglePlannerSectionCollapsed: (view, id) =>
        set((state) => ({
          plannerLayout: {
            ...state.plannerLayout,
            [view]: state.plannerLayout[view].map((s) =>
              s.id === id ? { ...s, collapsed: !s.collapsed } : s
            ),
          },
        })),

      resetPlannerLayout: (view) =>
        set((state) => ({
          plannerLayout: {
            ...state.plannerLayout,
            [view]: defaultPlannerLayout[view],
          },
        })),

      setFloatingWindow: (key, rect) =>
        set((state) => ({
          floatingWindows: { ...state.floatingWindows, [key]: rect },
        })),

      // Accolades Modal
      openAccoladesModal: () =>
        set({ accoladesModalOpen: true }),

      closeAccoladesModal: () =>
        set({ accoladesModalOpen: false }),

      // Incarnate Modal
      openIncarnateModal: (slotId) =>
        set({
          incarnateModalOpen: true,
          currentIncarnateSlot: slotId || 'alpha',
        }),

      closeIncarnateModal: () =>
        set({
          incarnateModalOpen: false,
        }),

      setCurrentIncarnateSlot: (slotId) =>
        set({
          currentIncarnateSlot: slotId,
        }),

      // Incarnate Crafting Modal
      openIncarnateCraftingModal: () =>
        set({ incarnateCraftingModalOpen: true }),

      closeIncarnateCraftingModal: () =>
        set({ incarnateCraftingModalOpen: false }),

      setIncarnateT4ComboIndex: (powerId, index) =>
        set((state) => ({
          incarnateT4ComboIndex: {
            ...state.incarnateT4ComboIndex,
            [powerId]: index,
          },
        })),

      // Export/Import Modal
      openExportImportModal: (tab) =>
        set({ exportImportModalOpen: true, exportImportModalTab: tab ?? null }),

      closeExportImportModal: () =>
        set({ exportImportModalOpen: false, exportImportModalTab: null }),

      // Export-as-Image Modal
      openBuildImageModal: () => set({ buildImageModalOpen: true }),
      closeBuildImageModal: () => set({ buildImageModalOpen: false }),

      // Feedback Modal
      openFeedbackModal: () =>
        set({ feedbackModalOpen: true }),

      closeFeedbackModal: () =>
        set({ feedbackModalOpen: false }),

      // Changelog Modal
      openChangelogModal: () =>
        set({ changelogModalOpen: true }),

      closeChangelogModal: () =>
        set({ changelogModalOpen: false }),

      openWelcomeModal: () =>
        set({ welcomeModalOpen: true }),

      closeWelcomeModal: () =>
        set({ welcomeModalOpen: false }),

      openEnhancementListModal: () =>
        set({ enhancementListModalOpen: true }),

      closeEnhancementListModal: () =>
        set({ enhancementListModalOpen: false }),

      // Controls Modal
      openControlsModal: () =>
        set({ controlsModalOpen: true }),

      closeControlsModal: () =>
        set({ controlsModalOpen: false }),

      // Help Modal
      openHelpModal: (initialTopicId) =>
        set({ helpModalOpen: true, helpModalInitialTopic: initialTopicId ?? null }),

      closeHelpModal: () =>
        set({ helpModalOpen: false, helpModalInitialTopic: null }),

      // Detailed Totals Modal
      openDetailedTotalsModal: () =>
        set({ detailedTotalsModalOpen: true }),

      closeDetailedTotalsModal: () =>
        set({ detailedTotalsModalOpen: false }),

      // Powerset Compare Modal
      openPowersetCompareModal: () =>
        set({ powersetCompareModalOpen: true }),

      closePowersetCompareModal: () =>
        set({ powersetCompareModalOpen: false }),

      // Set Bonus Lookup Modal
      openSetBonusPopup: () =>
        set({ setBonusPopupOpen: true }),

      closeSetBonusPopup: () =>
        set({ setBonusPopupOpen: false }),

      openSetBonusLookupModal: () =>
        set({ setBonusLookupModalOpen: true }),

      closeSetBonusLookupModal: () =>
        set({ setBonusLookupModalOpen: false }),

      // Power Info Modal (mobile only)
      openPowerInfoModal: () =>
        set({ powerInfoModalOpen: true }),

      closePowerInfoModal: () =>
        set({ powerInfoModalOpen: false }),

      // Incarnate Active State
      toggleIncarnateActive: (slotId) =>
        set((state) => ({
          incarnateActive: {
            ...state.incarnateActive,
            [slotId]: !state.incarnateActive[slotId],
          },
        })),

      setIncarnateActive: (slotId, active) =>
        set((state) => ({
          incarnateActive: {
            ...state.incarnateActive,
            [slotId]: active,
          },
        })),

      resetIncarnateActive: () =>
        set({ incarnateActive: createDefaultIncarnateActiveState() }),

      setIncarnateLevelShift: (shift) =>
        set({ incarnateLevelShift: shift === null ? null : Math.max(0, shift) }),

      setDestinyTime: (seconds) =>
        set({ destinyTime: seconds === null ? null : Math.max(0, seconds) }),

      setHybridTargetsHit: (foes) => set({ hybridTargetsHit: Math.max(0, Math.round(foes)) }),

      // Domination Active State
      toggleDomination: () =>
        set((state) => ({
          dominationActive: !state.dominationActive,
        })),

      setDominationActive: (active) =>
        set({ dominationActive: active }),

      // Scourge Active State
      toggleScourge: () =>
        set((state) => ({
          scourgeActive: !state.scourgeActive,
        })),

      setScourgeActive: (active) =>
        set({ scourgeActive: active }),

      // Fury Level (Brute)
      setFuryLevel: (level) =>
        set({ furyLevel: Math.max(0, Math.min(100, level)) }),

      // Supremacy Active State (Mastermind)
      toggleSupremacy: () =>
        set((state) => ({
          supremacyActive: !state.supremacyActive,
        })),

      setSupremacyActive: (active) =>
        set({ supremacyActive: active }),

      // Vigilance Team Size (Defender)
      setVigilanceTeamSize: (size) =>
        set({ vigilanceTeamSize: Math.max(0, Math.min(7, size)) }),

      // Critical Hits Active State (Scrapper)
      toggleCriticalHits: () =>
        set((state) => ({
          criticalHitsActive: !state.criticalHitsActive,
        })),

      setCriticalHitsActive: (active) =>
        set({ criticalHitsActive: active }),

      // Stalker Hidden State
      toggleStalkerHidden: () =>
        set((state) => ({
          stalkerHidden: !state.stalkerHidden,
        })),

      setStalkerHidden: (hidden) =>
        set({ stalkerHidden: hidden }),

      // What-if team buffs
      setWhatIfBuff: (stat, magnitude) =>
        set((state) => {
          const next = { ...state.whatIfBuffs };
          if (magnitude === 0) delete next[stat];
          else next[stat] = magnitude;
          return { whatIfBuffs: next };
        }),

      clearWhatIfBuffs: () => set({ whatIfBuffs: {} }),

      // Stalker Team Size
      setStalkerTeamSize: (size) =>
        set({ stalkerTeamSize: Math.max(0, Math.min(7, size)) }),

      // Stalker Crit Active State
      toggleStalkerCrit: () =>
        set((state) => ({
          stalkerCritActive: !state.stalkerCritActive,
        })),

      // Containment Active State (Controller)
      toggleContainment: () =>
        set((state) => ({
          containmentActive: !state.containmentActive,
        })),

      setContainmentActive: (active) =>
        set({ containmentActive: active }),

      // Sentinel Critical Hits
      toggleSentinelCrit: () =>
        set((state) => ({
          sentinelCritActive: !state.sentinelCritActive,
        })),

      setSentinelCritActive: (active) =>
        set({ sentinelCritActive: active }),

      // Arachnos Branch Selection
      setSelectedBranch: (branch) =>
        set({ selectedBranch: branch }),

      clearSelectedBranch: () =>
        set({ selectedBranch: null }),

      // Compare Slotting Modal
      openCompareSlotting: (powerName?, powerSet?) =>
        set({
          compareSlottingOpen: true,
          compareSlottingPower: powerName && powerSet ? { powerName, powerSet } : null,
        }),

      closeCompareSlotting: () =>
        set({ compareSlottingOpen: false, compareSlottingPower: null }),

      setCompareSlottingCopies: (key, copies) =>
        set((state) => {
          const resolved =
            typeof copies === 'function'
              ? copies(state.compareSlottingCopies[key] ?? [])
              : copies;
          const next = { ...state.compareSlottingCopies };
          if (resolved.length === 0) delete next[key];
          else next[key] = resolved;
          return { compareSlottingCopies: next };
        }),

      clearCompareSlottingCopies: () => set({ compareSlottingCopies: {} }),

      // Power View Mode
      setPowerViewMode: (mode) =>
        set({ powerViewMode: mode }),

      togglePowerViewMode: () =>
        set((state) => ({
          powerViewMode: state.powerViewMode === 'category' ? 'chronological' : 'category',
        })),

      // Hover hint — ephemeral; never persisted
      setHoverHint: (hint) => set({ hoverHint: hint }),

      beginSlotLevelMove: (source) => set({ slotMoveSource: source, slotRelocateSource: null }),
      cancelSlotLevelMove: () => set({ slotMoveSource: null }),

      beginSlotRelocate: (source) => set({ slotRelocateSource: source, slotMoveSource: null }),
      cancelSlotRelocate: () => set({ slotRelocateSource: null }),

      // Tracked Stats
      toggleTrackedStat: (breakdownKey) =>
        set((state) => ({
          trackedStats: state.trackedStats.includes(breakdownKey)
            ? state.trackedStats.filter((k) => k !== breakdownKey)
            : [...state.trackedStats, breakdownKey],
        })),
      ensureTrackedStats: (keys) =>
        set((state) => {
          const toAdd = keys.filter((k) => !state.trackedStats.includes(k));
          if (toAdd.length === 0) return state;
          return { trackedStats: [...state.trackedStats, ...toAdd] };
        }),
      clearTrackedStats: () => set({ trackedStats: [] }),

      // Per-target slider
      setTargetsHit: (powerName, value) =>
        set((state) => ({
          targetsHitValues: { ...state.targetsHitValues, [powerName]: value },
        })),

      setTargetsHitBulk: (values) =>
        set((state) => ({
          targetsHitValues: { ...state.targetsHitValues, ...values },
        })),

      // Mechanic Adjuster toggles
      setMechanicAdjuster: (powerName, adjusterId, active) =>
        set((state) => ({
          mechanicAdjusters: {
            ...state.mechanicAdjusters,
            [`${powerName}:${adjusterId}`]: active,
          },
        })),
      toggleMechanicAdjuster: (powerName, adjusterId) =>
        set((state) => {
          const key = `${powerName}:${adjusterId}`;
          return {
            mechanicAdjusters: {
              ...state.mechanicAdjusters,
              [key]: !state.mechanicAdjusters[key],
            },
          };
        }),
      clearMechanicAdjusters: (powerName) =>
        set((state) => {
          const prefix = `${powerName}:`;
          const next: Record<string, boolean> = {};
          for (const k of Object.keys(state.mechanicAdjusters)) {
            if (!k.startsWith(prefix)) next[k] = state.mechanicAdjusters[k];
          }
          return { mechanicAdjusters: next };
        }),

      // Global Mechanic Adjusters
      setGlobalAdjuster: (adjusterId, active) =>
        set((state) => ({
          globalAdjusters: { ...state.globalAdjusters, [adjusterId]: active },
        })),
      toggleGlobalAdjuster: (adjusterId) =>
        set((state) => ({
          globalAdjusters: {
            ...state.globalAdjusters,
            [adjusterId]: !state.globalAdjusters[adjusterId],
          },
        })),
      setGlobalAdjusterGroup: (activeId, siblingIds) =>
        set((state) => {
          const next = { ...state.globalAdjusters };
          for (const id of siblingIds) next[id] = false;
          if (activeId !== null) next[activeId] = true;
          return { globalAdjusters: next };
        }),

      // Slot level labels
      toggleShowSlotLevels: () =>
        set((state) => ({
          showSlotLevels: !state.showSlotLevels,
        })),

      toggleShowProcPotential: () =>
        set((state) => ({
          showProcPotential: !state.showProcPotential,
        })),

      togglePermaTracked: (powerName) =>
        set((state) => ({
          permaTrackedPowers: state.permaTrackedPowers.includes(powerName)
            ? state.permaTrackedPowers.filter((n) => n !== powerName)
            : [...state.permaTrackedPowers, powerName],
        })),

      toggleLevelUpMode: () =>
        set((state) => ({ levelUpMode: !state.levelUpMode })),

      setLevelUpMode: (enabled) =>
        set({ levelUpMode: enabled }),

      openMobileSheet: (sheet) => set({ mobileSheet: sheet }),
      closeMobileSheet: () => set({ mobileSheet: null }),

      // Toasts
      showToast: (toast) => {
        const id = `toast-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`;
        set((state) => ({ toasts: [{ id, ...toast }, ...state.toasts] }));
        return id;
      },
      dismissToast: (id) =>
        set((state) => ({ toasts: state.toasts.filter((t) => t.id !== id) })),
      clearToasts: () => set({ toasts: [] }),
      setRuleOf5AlertEnabled: (enabled) => set({ ruleOf5AlertEnabled: enabled }),
      toggleRuleOf5AlertEnabled: () =>
        set((state) => ({ ruleOf5AlertEnabled: !state.ruleOf5AlertEnabled })),
      setRechargeMidsStyle: (enabled) => set({ rechargeMidsStyle: enabled }),
      toggleRechargeMidsStyle: () =>
        set((state) => ({ rechargeMidsStyle: !state.rechargeMidsStyle })),

      resetForNewBuild: () =>
        set({
          enhancementPicker: defaultEnhancementPicker,
          genericPicker: defaultGenericPicker,
          originPicker: defaultOriginPicker,
          infoPanel: defaultInfoPanel,
          tooltip: defaultTooltip,
          compareSlottingOpen: false,
          compareSlottingPower: null,
          // Copies describe slotting on a specific build's powers; a new build
          // shares neither the powers nor the slot counts.
          compareSlottingCopies: {},
          selectedBranch: null,
          targetsHitValues: {},
          mechanicAdjusters: {},
          globalAdjusters: {},
          // A new build must never open already simulating (WHAT-IF-BUFFS-PLAN WIF15).
          whatIfBuffs: {},
          incarnateActive: createDefaultIncarnateActiveState(),
          incarnateLevelShift: null,
          destinyTime: 0,
          hybridTargetsHit: 0,
          dominationActive: false,
          scourgeActive: false,
          furyLevel: 0,
          supremacyActive: false,
          vigilanceTeamSize: 0,
          criticalHitsActive: false,
          stalkerHidden: false,
          stalkerTeamSize: 0,
          stalkerCritActive: false,
          containmentActive: false,
          sentinelCritActive: false,
          trackedStats: [],
          permaTrackedPowers: [],
          // Close all modals
          statsConfigModalOpen: false,
          statsConfigScrollTo: null,
          accoladesModalOpen: false,
          incarnateModalOpen: false,
          incarnateCraftingModalOpen: false,
          exportImportModalOpen: false,
          exportImportModalTab: null,
          powerInfoModalOpen: false,
          detailedTotalsModalOpen: false,
          powersetCompareModalOpen: false,
          setBonusLookupModalOpen: false,
        }),
    }),
    {
      name: 'coh-planner-ui',
      storage: createJSONStorage(() => localStorage),
      // Only persist settings, not transient state
      partialize: (state) => ({
        globalIOLevel: state.globalIOLevel,
        attunementEnabled: state.attunementEnabled,
        globalBoostLevel: state.globalBoostLevel,
        globalRelativeLevel: state.globalRelativeLevel,
        ioSetSortBy: state.ioSetSortBy,
        lastPickerFilterByPower: state.lastPickerFilterByPower,
        // exemplarMode / exemplarLevel intentionally NOT persisted. Leaving
        // exemplar on across sessions silently changes every recharge /
        // damage / defense number on reload, which is hard to debug when
        // the user forgets the toggle is on.
        targetLevelOffset: state.targetLevelOffset,
        contentMode: state.contentMode,
        procSettings: state.procSettings,
        combatMode: state.combatMode,
        procChancePinned: state.procChancePinned,
        hintsEnabled: state.hintsEnabled,
        infoPanel: { enabled: state.infoPanel.enabled, content: null, locked: false, lockedContent: null, tooltipEnabled: state.infoPanel.tooltipEnabled, undocked: false },
        statsConfig: state.statsConfig,
        plannerLayout: state.plannerLayout,
        floatingWindows: state.floatingWindows,
        dashboardCollapsed: state.dashboardCollapsed,
        uiScale: state.uiScale,
        colorTheme: state.colorTheme,
        colorMode: state.colorMode,
        incarnateActive: state.incarnateActive,
        incarnateLevelShift: state.incarnateLevelShift,
        destinyTime: state.destinyTime,
        hybridTargetsHit: state.hybridTargetsHit,
        dominationActive: state.dominationActive,
        scourgeActive: state.scourgeActive,
        furyLevel: state.furyLevel,
        supremacyActive: state.supremacyActive,
        vigilanceTeamSize: state.vigilanceTeamSize,
        criticalHitsActive: state.criticalHitsActive,
        stalkerHidden: state.stalkerHidden,
        stalkerTeamSize: state.stalkerTeamSize,
        stalkerCritActive: state.stalkerCritActive,
        containmentActive: state.containmentActive,
        sentinelCritActive: state.sentinelCritActive,
        selectedBranch: state.selectedBranch,
        powerViewMode: state.powerViewMode,
        trackedStats: state.trackedStats,
        showSlotLevels: state.showSlotLevels,
        showProcPotential: state.showProcPotential,
        permaTrackedPowers: state.permaTrackedPowers,
        levelUpMode: state.levelUpMode,
        mechanicAdjusters: state.mechanicAdjusters,
        globalAdjusters: state.globalAdjusters,
        ruleOf5AlertEnabled: state.ruleOf5AlertEnabled,
        rechargeMidsStyle: state.rechargeMidsStyle,
        dismissedAnnouncements: state.dismissedAnnouncements,
        chainPowerMetric: state.chainPowerMetric,
        chainShowEffectWindows: state.chainShowEffectWindows,
        foldProcsIntoFinalDamage: state.foldProcsIntoFinalDamage,
      }),
      merge: (persisted, current) => {
        const merged = { ...current, ...(persisted as Partial<UIStore>) };
        // Migrate old infoPanelScale → uiScale
        const raw = persisted as Record<string, unknown> | undefined;
        if (raw && 'infoPanelScale' in raw && !('uiScale' in raw)) {
          merged.uiScale = raw.infoPanelScale as number;
        }
        // Migrate old includeProcsInStats → procSettings
        if (raw && 'includeProcsInStats' in raw && !('procSettings' in raw)) {
          const allOn = raw.includeProcsInStats !== false;
          merged.procSettings = {
            damage: allOn,
            recovery: allOn,
            regeneration: allOn,
            recharge: allOn,
            toHit: allOn,
            defense: allOn,
            resistance: allOn,
            buildUp: allOn,
            movement: allOn,
          };
        }
        // Migrate the old single attack-chain dismissal flag → the
        // dismissedAnnouncements registry, so users who already opted out of
        // that spotlight don't see it return.
        if (!Array.isArray(merged.dismissedAnnouncements)) {
          merged.dismissedAnnouncements = [];
        }
        if (raw && raw.attackChainAnnounceDismissed === true &&
            !merged.dismissedAnnouncements.includes('attack-chain-builder')) {
          merged.dismissedAnnouncements = [...merged.dismissedAnnouncements, 'attack-chain-builder'];
        }
        // Migrate the retired 'imperial-light' theme → imperial + light mode.
        // The generated light ramp reproduces the old inverted theme exactly,
        // so users who picked it keep their look on the new mode axis.
        if ((merged.colorTheme as string) === 'imperial-light') {
          merged.colorTheme = 'imperial';
          merged.colorMode = 'light';
        }
        // Ensure procSettings has all keys (in case new categories are added)
        if (merged.procSettings) {
          merged.procSettings = { ...DEFAULT_PROC_SETTINGS, ...merged.procSettings };
        }
        // Ensure incarnateActive has all slot keys (judgement/lore added later as cosmetic toggles)
        if (merged.incarnateActive) {
          merged.incarnateActive = { ...createDefaultIncarnateActiveState(), ...merged.incarnateActive };
        }
        // Reconcile the persisted planner layout against the current defaults:
        // keep the user's order/visibility, append any sections added since they
        // last saved, and drop any ids the app no longer knows. Done per view so
        // a schema change to one view can't strand the other.
        const persistedLayout = (persisted as Partial<UIStore>)?.plannerLayout;
        if (persistedLayout) {
          const reconcileView = (view: keyof PlannerLayoutState): PlannerSectionConfig[] => {
            const defaults = defaultPlannerLayout[view];
            const known = new Set(defaults.map((s) => s.id));
            const saved = Array.isArray(persistedLayout[view]) ? persistedLayout[view] : [];
            // One-time migration for the atomic-section split: a pre-split saved
            // category layout would otherwise scatter the five new sections into
            // their own trailing columns on upgrade (the "every cell its own
            // column" regression). Adopt the curated new default instead.
            // Chronological never split, so it's exempt.
            if (view === 'category' && isPreAtomicSplitCategory(saved)) {
              return defaults;
            }
            const kept = saved.filter((s) => known.has(s.id));
            const present = new Set(kept.map((s) => s.id));
            const missing = defaults.filter((s) => !present.has(s.id));
            // reconcilePlannerColumns backfills `column` for legacy (pre-LAY11)
            // entries and appends new sections in fresh trailing columns.
            return reconcilePlannerColumns(kept, missing);
          };
          merged.plannerLayout = {
            category: reconcileView('category'),
            chronological: reconcileView('chronological'),
          };
        } else {
          merged.plannerLayout = defaultPlannerLayout;
        }

        // Inject any new default stats that aren't in the persisted config
        const persistedStats = (persisted as Partial<UIStore>)?.statsConfig;
        if (persistedStats) {
          const existingStatIds = new Set(persistedStats.map((s) => s.stat));
          const missing = defaultStatsConfig.filter((s) => !existingStatIds.has(s.stat));
          if (missing.length > 0) {
            const maxOrder = Math.max(...persistedStats.map((s) => s.order), -1);
            merged.statsConfig = [
              ...persistedStats,
              ...missing.map((s, i) => ({ ...s, order: maxOrder + 1 + i })),
            ];
          }
        }
        return merged;
      },
    })
  );

// ============================================
// SELECTOR HOOKS
// ============================================

/** Select enhancement picker state */
export const useEnhancementPicker = () => useUIStore((state) => state.enhancementPicker);

/** Select if enhancement picker is open */
export const useIsPickerOpen = () => useUIStore((state) => state.enhancementPicker.isOpen);

/** Select global IO level */
export const useGlobalIOLevel = () => useUIStore((state) => state.globalIOLevel);

/** Select attunement setting */
export const useAttunement = () => useUIStore((state) => state.attunementEnabled);

/** Select exemplar mode setting */
export const useExemplarMode = () => useUIStore((state) => state.exemplarMode);

/** Select exemplar level */
export const useExemplarLevel = () => useUIStore((state) => state.exemplarLevel);

/** Select hints setting */
export const useHintsEnabled = () => useUIStore((state) => state.hintsEnabled);

/** Select info panel state */
export const useInfoPanel = () => useUIStore((state) => state.infoPanel);

/** Select tooltip state */
export const useTooltip = () => useUIStore((state) => state.tooltip);

/** Select stats config */
export const useStatsConfig = () => useUIStore((state) => state.statsConfig);

/** Select visible stats only */
export const useVisibleStats = () =>
  useUIStore((state) =>
    state.statsConfig.filter((s) => s.visible).sort((a, b) => a.order - b.order)
  );

/** Select incarnate modal state */
export const useIncarnateModal = () =>
  useUIStore((state) => ({
    isOpen: state.incarnateModalOpen,
    currentSlot: state.currentIncarnateSlot,
  }));

/** Select incarnate active state */
export const useIncarnateActive = () => useUIStore((state) => state.incarnateActive);

/** Select if a specific incarnate slot is active */
export const useIsIncarnateSlotActive = (slotId: ToggleableIncarnateSlot) =>
  useUIStore((state) => state.incarnateActive[slotId]);

/** Select the Destiny time-slider position (seconds after cast). */
export const useDestinyTime = () => useUIStore((state) => state.destinyTime);
export const useHybridTargetsHit = () => useUIStore((state) => state.hybridTargetsHit);

/** Select domination active state */
export const useDominationActive = () => useUIStore((state) => state.dominationActive);

/** Select scourge active state */
export const useScourgeActive = () => useUIStore((state) => state.scourgeActive);

/** Select fury level */
export const useFuryLevel = () => useUIStore((state) => state.furyLevel);

/** Select supremacy active state */
export const useSupremacyActive = () => useUIStore((state) => state.supremacyActive);

/** Select vigilance team size */
export const useVigilanceTeamSize = () => useUIStore((state) => state.vigilanceTeamSize);

/** Select critical hits active state */
export const useCriticalHitsActive = () => useUIStore((state) => state.criticalHitsActive);

/** Select stalker hidden state */
export const useStalkerHidden = () => useUIStore((state) => state.stalkerHidden);

/**
 * Select the active state of one Mechanic Adjuster toggle. Falls back to
 * the conditional effect's `defaultActive` flag when the user hasn't
 * touched the toggle. Use for `scope: 'per-power'` conditionals.
 */
export const useMechanicAdjuster = (
  powerName: string,
  adjusterId: string,
  defaultActive: boolean = false,
): boolean => useUIStore((state) => {
  const v = state.mechanicAdjusters[`${powerName}:${adjusterId}`];
  return v === undefined ? defaultActive : v;
});

/**
 * Select the active state of one global Mechanic Adjuster (caster-state
 * mechanic — Bio Armor adaptation, Hide, Domination, etc.). Use for
 * `scope: 'global'` conditionals.
 */
export const useGlobalAdjuster = (
  adjusterId: string,
  defaultActive: boolean = false,
): boolean => useUIStore((state) => {
  const v = state.globalAdjusters[adjusterId];
  return v === undefined ? defaultActive : v;
});

/** Select stalker team size */
export const useStalkerTeamSize = () => useUIStore((state) => state.stalkerTeamSize);

/** Select stalker crit active state */
export const useStalkerCritActive = () => useUIStore((state) => state.stalkerCritActive);

/** Select containment active state */
export const useContainmentActive = () => useUIStore((state) => state.containmentActive);

/** Select sentinel crit active state */
export const useSentinelCritActive = () => useUIStore((state) => state.sentinelCritActive);

/** Select power view mode */
export const usePowerViewMode = () => useUIStore((state) => state.powerViewMode);

/** Select targets-hit value for a specific power */
export const useTargetsHit = (powerName: string) =>
  useUIStore((state) => state.targetsHitValues[powerName] ?? 0);

/** Select slot level labels visibility */
export const useShowSlotLevels = () => useUIStore((state) => state.showSlotLevels);
export const useShowProcPotential = () => useUIStore((state) => state.showProcPotential);

/** Select proc damage in DPS toggle */
export const useIncludeProcDamageInDPS = () => useUIStore((state) => state.includeProcDamageInDPS);

/** Select ArcanaTime toggle */
export const useArcanaTime = () => useUIStore((state) => state.useArcanaTime);

/** Select damage per activation toggle */
export const useDamageDisplayMode = () => useUIStore((state) => state.damageDisplayMode);
