/**
 * EnhancementPicker component - single-screen modal for selecting enhancements
 *
 * Layout:
 * - Top: Enhancement type filters (IO Sets, Generic IO, Special, Origin)
 * - Left: Category filters (based on power's allowed set categories)
 * - Main: Scrollable list of all matching enhancements
 */

import { useState, useMemo, useEffect, useRef } from 'react';
import { useBuildStore, useUIStore } from '@/stores';
import {
  getIOSetsForPower, getIOSet, getMostCommonSetSize, lookupPower,
  ORIGIN_TIER_INFO,
  sortCategoriesByPriority,
  createIOSetEnhancement, createGenericIOEnhancement, createSpecialEnhancement, createOriginEnhancement, isInherentlyAttuned,
  getAvailableGenericIOs, getAvailableHamidons, getAvailableTitans, getAvailableHydras, getAvailableDSyncs, getAvailablePrestige,
  getRarityColor, getTierTextColor, getTierBorderColor,
  findProcData, resolveProcPieceName, procEffectSummary, getProcEffectLabel, getProcEffectColor, isProcAlwaysOn, interpolateProcDamage,
} from '@/data';
import { normalizeAspectName, readAspectDisplayValue, getEffectiveAspectCount, calculateSingleEnhancementValues, enhancementLevelAxis, enhancementLevelRange, genericIOValueAtLevel, getOriginTierValue } from '@/utils/calculations';
import { Modal, ModalBody } from '@/components/modals';
import { Tooltip, Toggle, LevelSpinner, formatSignedOffset } from '@/components/ui';
import { IOSetIcon, GenericIOIcon, OriginEnhancementIcon, SpecialEnhancementIcon } from './EnhancementIcon';
import { SetBonusList } from './SetBonusList';
import type { IOSet, IOSetPiece, EnhancementStatType, SpecialEnhancementDef, IOSetCategory, SpecialEnhancement, Enhancement } from '@/types';
import { getCommonIOLevels, craftedCommonIOLevel } from '@/data/boost-index';
import { getSetTrackedBonuses, type TrackedBonusMatch } from '@/data/set-bonus-index';
import { statKeyToChipLabel, formatTrackedBonusAmount } from '@/data/set-bonus-groups';
import { formatBonusDesc } from '@/utils/set-bonus-format';
import { isBonusCapped, getTotalBonusCount } from '@/utils/calculations/set-bonuses';
import { useBonusTracking, useIsTouchDevice } from '@/hooks';
import { getEnhancementOutline } from '@/utils/enhancement-outline';
import { pieceSlottableNow } from '@/utils/enhancement-eligibility';

// Max finger travel (px) between touchstart and touchend still counted as a tap
// rather than a scroll. Generous enough for thumbs on a moving list.
const TAP_MOVE_TOLERANCE = 12;

type EnhancementTypeFilter = 'io-sets' | 'generic' | 'special' | 'origin';

// Sidebar filter can be 'all', a category name, or a special group
type SidebarFilter =
  | 'all'
  | 'universal'
  | 'very-rare'
  | 'event'
  | 'archetype'
  | 'procs'
  | string; // Category name like "Ranged Damage"

export function EnhancementPicker() {
  const picker = useUIStore((s) => s.enhancementPicker);
  const globalIOLevel = useUIStore((s) => s.globalIOLevel);
  const attunementEnabled = useUIStore((s) => s.attunementEnabled);
  const toggleAttunement = useUIStore((s) => s.toggleAttunement);
  const setGlobalIOLevel = useUIStore((s) => s.setGlobalIOLevel);
  const globalBoostLevel = useUIStore((s) => s.globalBoostLevel);
  const setGlobalBoostLevel = useUIStore((s) => s.setGlobalBoostLevel);
  const globalRelativeLevel = useUIStore((s) => s.globalRelativeLevel);
  const setGlobalRelativeLevel = useUIStore((s) => s.setGlobalRelativeLevel);
  const ioSortBy = useUIStore((s) => s.ioSetSortBy);
  const setIOSortBy = useUIStore((s) => s.setIOSetSortBy);
  const lastPickerFilterByPower = useUIStore((s) => s.lastPickerFilterByPower);
  const setLastPickerFilter = useUIStore((s) => s.setLastPickerFilter);
  const closeEnhancementPicker = useUIStore((s) => s.closeEnhancementPicker);
  const setEnhancement = useBuildStore((s) => s.setEnhancement);
  const buildOrigin = useBuildStore((s) => s.build.settings.origin);
  const build = useBuildStore((s) => s.build);
  const isUniqueEnhancementSlotted = useBuildStore((s) => s.isUniqueEnhancementSlotted);

  // Local filter state
  const [typeFilter, setTypeFilter] = useState<EnhancementTypeFilter>('io-sets');
  const [sidebarFilter, setSidebarFilter] = useState<SidebarFilter>('all');
  // Set size (piece count) facet; null = any size. Cuts across the category
  // filter rather than replacing it — "3-piece Holds" is the useful question.
  const [sizeFilter, setSizeFilter] = useState<number | null>(null);

  // The header spinner feeds every slotting path, but the two enhancement
  // families sit on DIFFERENT axes: IOs take booster combines (0..+5), while
  // origin/special carry a relative level that is legitimately negative. The
  // active tab decides which, and the domain comes from the dataset's own
  // curves rather than a hardcoded +-3 (Rebirth reaches -9; Thunderspy applies
  // no attenuation at all).
  const levelOffsetType: Enhancement['type'] =
    typeFilter === 'origin' ? 'origin' : typeFilter === 'special' ? 'special' : 'io-set';
  const levelOffsetAxis = enhancementLevelAxis(levelOffsetType);
  const levelOffsetRange = useMemo(
    () => enhancementLevelRange(levelOffsetType),
    [levelOffsetType],
  );
  // What the spinner shows and edits: the active tab's axis.
  const levelOffset = levelOffsetAxis === 'relative' ? globalRelativeLevel : globalBoostLevel;
  const setLevelOffset =
    levelOffsetAxis === 'relative' ? setGlobalRelativeLevel : setGlobalBoostLevel;

  // The crafting levels the export names a record at. Set pieces are craftable
  // at every integer inside their own range, common IOs only at the roster's
  // nine — so the spinner takes its ends from the roster everywhere, and walks
  // it entry by entry on the tab where that is all there is. Its band used to
  // be `min={10} max={53}` typed in, which offered a level-37 Accuracy IO and
  // three levels past the top of the game's roster; 51-53 is the pre-booster
  // spelling of a level-50 IO with three combines, which this picker already
  // carries on its own axis as the Boost dial (BOOST-6).
  const ioCraftLevels = getCommonIOLevels();
  const onGenericTab = typeFilter === 'generic';
  // What a generic IO picked right now is crafted at: a level carried in from
  // the set tab (27 is legal there) floors onto the roster.
  const genericIOLevel = craftedCommonIOLevel(globalIOLevel, ioCraftLevels);

  // A level persisted from before the band was read off the export comes back
  // out of range; the setter clamps, so handing it its own value folds it in.
  useEffect(() => {
    if (globalIOLevel < ioCraftLevels[0] || globalIOLevel > ioCraftLevels[ioCraftLevels.length - 1]) {
      setGlobalIOLevel(globalIOLevel);
    }
  }, [globalIOLevel, ioCraftLevels, setGlobalIOLevel]);

  // The level offset above is a PLACEMENT default — it is stamped into an
  // enhancement when the picker mints it and nothing revisits a slot after.
  // A user who sets it to +5 mid-build therefore boosts only what they slot
  // NEXT, while the spinner keeps reading +5 over a build full of unboosted
  // enhancements (reported 2026-08-18: a Scrapper's max HP sat 21 short of the
  // in-game character, exactly the +5 his 94 already-placed slots never got).
  // This applies the current offset to what is already slotted, on the same
  // store action the Enhancement Tools modal uses — one history checkpoint,
  // one set of eligibility rules. Result count included because those rules
  // can legitimately move nothing, and a silent no-op reads as a dead button.
  const maximizeEnhancementLevels = useBuildStore((s) => s.maximizeEnhancementLevels);
  const [bulkApplied, setBulkApplied] = useState<number | null>(null);
  useEffect(() => setBulkApplied(null), [levelOffset, levelOffsetAxis]);
  const applyOffsetToSlotted = () => {
    setBulkApplied(
      maximizeEnhancementLevels(
        levelOffsetAxis === 'relative'
          ? { relativeLevel: levelOffset }
          : { boostLevel: levelOffset },
      ),
    );
  };

  // Switching to a booster-axis tab must not leave a negative behind — it would
  // silently read as +0 there while still showing as a penalty in the spinner.
  useEffect(() => {
    if (levelOffset < levelOffsetRange.min) setLevelOffset(levelOffsetRange.min);
    else if (levelOffset > levelOffsetRange.max) setLevelOffset(levelOffsetRange.max);
  }, [levelOffsetRange, levelOffset, setLevelOffset]);

  // Drag selection state (mouse/desktop only)
  const [isDragging, setIsDragging] = useState(false);
  const [dragStartIndex, setDragStartIndex] = useState<number | null>(null);
  const [dragEndIndex, setDragEndIndex] = useState<number | null>(null);
  const [dragSet, setDragSet] = useState<IOSet | null>(null);

  // Multi-select state: setId → Set of piece indices. Populated by shift+click (desktop)
  // or tap in selectMode. Shared UI model: sticky bottom bar commits, "Cancel" clears.
  const [shiftSelected, setShiftSelected] = useState<Map<string, Set<number>>>(new Map());

  // Stack-select state for the generic / special / origin tabs. Set pieces
  // are inherently unique-per-power so they use a Set; common IOs / HOs /
  // origins are stackable, so we keep an explicit count per tile id along
  // with a thunk that builds the resulting Enhancement at commit time
  // (lazy so global IO level / boost / origin reflect the current value
  // when the user clicks Slot, not when they first added the tile).
  type StackedEntry = { count: number; build: () => Enhancement; label: string };
  const [stackedSelected, setStackedSelected] = useState<Map<string, StackedEntry>>(new Map());

  const incStacked = (id: string, build: () => Enhancement, label: string) => {
    setStackedSelected((prev) => {
      const next = new Map(prev);
      const existing = next.get(id);
      next.set(id, { count: (existing?.count ?? 0) + 1, build, label });
      return next;
    });
  };

  const decStacked = (id: string) => {
    setStackedSelected((prev) => {
      const next = new Map(prev);
      const existing = next.get(id);
      if (!existing) return prev;
      if (existing.count <= 1) next.delete(id);
      else next.set(id, { ...existing, count: existing.count - 1 });
      return next;
    });
  };

  const stackedCountFor = (id: string) => stackedSelected.get(id)?.count ?? 0;

  // Explicit select-mode toggle (visible on both desktop and mobile) — makes every
  // click/tap toggle selection instead of slotting immediately.
  const [selectMode, setSelectMode] = useState(false);

  // Shift+click has done the same job since 2026-04-23 and is listed in the
  // Controls reference, which is the one place a user has no reason to look
  // while they are mid-slot. Reported 2026-09-17 by someone who had been using
  // the toggle for months without knowing the modifier existed. The button
  // stays — it is the only multi-select a touch device has, there being no
  // shift key — so the fix is to say so next to it, where the choice is made.
  const isTouch = useIsTouchDevice();

  // When opening the picker to change an already-slotted IO set piece, this
  // holds that set's id so its row scrolls into view + briefly highlights.
  // Null for empty slots / non-set enhancements (nothing specific to scroll to).
  const [jumpToSetId, setJumpToSetId] = useState<string | null>(null);

  // Get the current power definition (unified lookup across all categories)
  const currentPower = useMemo(() => {
    if (!picker.currentPowerName || !picker.currentPowerSet) return null;
    return lookupPower(picker.currentPowerSet, picker.currentPowerName)?.power ?? null;
  }, [picker.currentPowerName, picker.currentPowerSet]);

  // Get the current power's slots from the build.
  //
  // Matches on (powerSet, internalName), NOT internalName alone: internal names
  // are reused across powersets (a Dominator's epic Rain of Fire is internally
  // `Fire_Blast`, same as the secondary's tier-1 Fire Blast), so a bare-name
  // search returns whichever category comes first and shows the WRONG power's
  // slots. See `findSelectedPowerInBuild` for the full collision list.
  const currentPowerSlots = useMemo(() => {
    if (!picker.currentPowerName || !picker.currentPowerSet) return [];

    // Search in all power categories
    const findInPowers = (powers: { name: string; internalName: string; powerSet: string; slots: (unknown | null)[] }[]) =>
      powers.find(p => p.internalName === picker.currentPowerName
                    && p.powerSet === picker.currentPowerSet)?.slots || [];

    let slots = findInPowers(build.primary.powers);
    if (slots.length > 0) return slots;

    slots = findInPowers(build.secondary.powers);
    if (slots.length > 0) return slots;

    for (const pool of build.pools) {
      slots = findInPowers(pool.powers);
      if (slots.length > 0) return slots;
    }

    if (build.epicPool) {
      slots = findInPowers(build.epicPool.powers);
      if (slots.length > 0) return slots;
    }

    slots = findInPowers(build.inherents);
    return slots;
  }, [picker.currentPowerName, picker.currentPowerSet, build]);

  // Get indices of empty slots (starting from currentSlotIndex)
  // When virtualSlots is set (compare modal), use those instead of the build's slots
  const emptySlotIndices = useMemo(() => {
    const slots = picker.virtualSlots ?? currentPowerSlots;
    const indices: number[] = [];
    for (let i = picker.currentSlotIndex; i < slots.length; i++) {
      if (!slots[i]) {
        indices.push(i);
      }
    }
    return indices;
  }, [currentPowerSlots, picker.currentSlotIndex, picker.virtualSlots]);

  // Get available IO sets for the current power based on its allowedSetCategories
  // ATO categories are already included in the power data for eligible powers
  const availableSets = useMemo(() => {
    if (!currentPower) return [];
    const categories = [...(currentPower.allowedSetCategories || [])] as IOSetCategory[];
    return getIOSetsForPower(categories);
  }, [currentPower]);

  // Derive all standard set categories from the available sets, sorted by priority
  const standardCategories = useMemo(() => {
    const cats = new Set<string>();
    for (const set of availableSets) {
      // Group every set under its real in-game Set Category (its `type`).
      // "Event" is not a game category — it's a rarity/source — so event sets
      // live under their functional type (Resist Damage, Holds, …) just like
      // uncommon/rare sets. Purple/ATO/PvP keep their own dedicated rarity tabs
      // (they ARE distinct game groupings), and Universal Damage owns its tab.
      if ((set.category === 'uncommon' || set.category === 'rare' || set.category === 'event') && set.type !== 'Universal Damage Sets') {
        cats.add(set.type);
      }
    }
    return sortCategoriesByPriority(Array.from(cats));
  }, [availableSets]);

  // Primary category is the first standard one (used for auto-select on open)
  const primaryCategory = standardCategories[0] || null;

  // Check which special groups have sets available
  const hasUniversal = useMemo(() =>
    availableSets.some((set) => set.type === 'Universal Damage Sets'), [availableSets]);
  const hasVeryRare = useMemo(() =>
    availableSets.some((set) => set.category === 'purple'), [availableSets]);
  const hasArchetype = useMemo(() =>
    availableSets.some((set) => set.category === 'ato'), [availableSets]);
  const hasPvP = useMemo(() =>
    availableSets.some((set) => set.category === 'pvp'), [availableSets]);
  const hasProcs = useMemo(() =>
    availableSets.some((set) => set.pieces.some((p) => p.proc)), [availableSets]);

  // Helper to check if a set is a "special" category (excluded from the
  // standard Set Category tabs because it owns a dedicated rarity tab). NB
  // 'event' is intentionally NOT here — event sets group under their real Set
  // Category. Purple / ATO / PvP / Universal Damage are genuine game groupings.
  const isSpecialSet = (set: IOSet) =>
    set.category === 'purple' ||
    set.category === 'ato' ||
    set.category === 'pvp' ||
    set.type === 'Universal Damage Sets';

  // Sort priority for special categories (higher = sorted later)
  const specialSortOrder = (set: IOSet): number => {
    if (set.type === 'Universal Damage Sets') return 1;
    if (set.category === 'purple') return 2;
    if (set.category === 'pvp') return 4;
    if (set.category === 'ato') return 5;
    return 0; // standard sets (incl. event) sort first in the 'all' view
  };

  const levelUpMode = useUIStore((s) => s.levelUpMode);

  // Sets for the chosen sidebar category, BEFORE the set-size facet. Split out
  // so the facet's counts are taken from what it would actually filter — a
  // "3 pieces (12)" button that lands on an empty list is worse than no button.
  const categorySets = useMemo(() => {
    let sets: IOSet[];
    switch (sidebarFilter) {
      case 'all':
        sets = [...availableSets];
        break;
      case 'universal':
        sets = availableSets.filter((set) => set.type === 'Universal Damage Sets');
        break;
      case 'very-rare':
        sets = availableSets.filter((set) => set.category === 'purple');
        break;
      case 'event':
        // Back-compat only: event sets now live under their real Set Category,
        // so there is no longer an "Event" sidebar button. This case still
        // resolves any stale persisted 'event' filter without showing nothing.
        sets = availableSets.filter((set) => set.category === 'event');
        break;
      case 'archetype':
        sets = availableSets.filter((set) => set.category === 'ato');
        break;
      case 'pvp':
        sets = availableSets.filter((set) => set.category === 'pvp');
        break;
      case 'procs':
        sets = availableSets.filter((set) => set.pieces.some((p) => p.proc));
        break;
      default:
        // Standard Set Category tab: every set of this type that isn't in a
        // dedicated rarity tab (purple / ATO / PvP / Universal Damage). Event
        // sets are not special, so they appear here under their real category.
        sets = availableSets.filter((set) => set.type === sidebarFilter && !isSpecialSet(set));
    }
    // Level Up mode: only show sets the character can use at their current level
    if (levelUpMode) {
      sets = sets.filter((set) => set.minLevel <= build.level);
    }
    return sets;
  }, [availableSets, sidebarFilter, levelUpMode, build.level]);

  // Distinct set sizes present in the current category, ascending, with counts.
  const sizeCounts = useMemo(() => {
    const tally = new Map<number, number>();
    for (const set of categorySets) {
      tally.set(set.pieces.length, (tally.get(set.pieces.length) ?? 0) + 1);
    }
    return Array.from(tally.entries()).sort((a, b) => a[0] - b[0]);
  }, [categorySets]);

  // A size the player picked in one category may not exist in the next. Resolve
  // it rather than clearing it, so switching categories can't strand them on an
  // empty list, and coming BACK to a category restores the size they chose.
  const effectiveSizeFilter =
    sizeFilter !== null && sizeCounts.some(([size]) => size === sizeFilter) ? sizeFilter : null;

  // Apply the size facet, then sort
  const filteredSets = useMemo(() => {
    let sets: IOSet[] = effectiveSizeFilter === null
      ? [...categorySets]
      : categorySets.filter((set) => set.pieces.length === effectiveSizeFilter);
    if (sidebarFilter === 'all') {
      // In 'all' view, group standard sets first, then special sets at bottom
      if (ioSortBy === 'level') {
        sets.sort((a, b) => specialSortOrder(a) - specialSortOrder(b) || a.minLevel - b.minLevel || a.maxLevel - b.maxLevel || a.name.localeCompare(b.name));
      } else {
        sets.sort((a, b) => specialSortOrder(a) - specialSortOrder(b) || a.name.localeCompare(b.name));
      }
    } else if (ioSortBy === 'level') {
      sets = [...sets].sort((a, b) => a.minLevel - b.minLevel || a.maxLevel - b.maxLevel || a.name.localeCompare(b.name));
    } else {
      sets = [...sets].sort((a, b) => a.name.localeCompare(b.name));
    }
    return sets;
  }, [categorySets, effectiveSizeFilter, sidebarFilter, ioSortBy]);

  // Flat list of individual proc pieces for the Procs filter
  const procPieces = useMemo(() => {
    if (sidebarFilter !== 'procs') return [];
    const pieces: { set: IOSet; piece: IOSetPiece; pieceIndex: number }[] = [];
    for (const set of availableSets) {
      set.pieces.forEach((piece, idx) => {
        if (piece.proc) {
          pieces.push({ set, piece, pieceIndex: idx });
        }
      });
    }
    return pieces;
  }, [availableSets, sidebarFilter]);

  // Check if a specific set piece is already slotted in the current power
  // In compare mode (virtualSlots), check the comparison copy's slots instead of the build
  const isPieceInCurrentPower = (setId: string, pieceNum: number) => {
    const slots = picker.virtualSlots ?? currentPowerSlots;
    return slots.some((enh) => {
      if (!enh || typeof enh !== 'object') return false;
      const ioEnh = enh as { type?: string; setId?: string; pieceNum?: number };
      return ioEnh.type === 'io-set' && ioEnh.setId === setId && ioEnh.pieceNum === pieceNum;
    });
  };

  // Whether a piece may be PLACED here right now — the rule the piece rows
  // render as their disabled state (`isPieceDisabled`), re-asked at placement
  // time because the bulk paths sweep pieces the user never clicked (see
  // `pieceSlottableNow`'s doc for the 2026-08-17 duplicate-ATO drag report).
  const isPiecePlaceable = (set: IOSet, piece: IOSetPiece) =>
    pieceSlottableNow(set, piece, picker.virtualSlots ?? currentPowerSlots, {
      compareMode: picker.virtualSlots != null,
      isUniqueSlotted: isUniqueEnhancementSlotted,
    });

  // Auto-select category when modal opens; clear shift-selection.
  // Restore the per-power last-used filter (typeFilter + sidebarFilter)
  // if there is one for this power, otherwise fall back to the power's
  // primary set category. This makes repeat slotting on the same power
  // land on the user's most recent choice (e.g. ATOs after slotting an
  // ATO once into Footstomp).
  // Note: the IO level intentionally does NOT reset to character level —
  // `globalIOLevel` persists across picker opens (and across page reloads
  // via the UI store) so the user's most recent choice sticks instead of
  // snapping back every time the modal reopens.
  // Map a set to the sidebar group it lives under (mirrors the sidebar buttons):
  // its special-rarity group when applicable, else its standard set type.
  const sidebarFilterForSet = (set: IOSet): SidebarFilter => {
    if (set.type === 'Universal Damage Sets') return 'universal';
    if (set.category === 'purple') return 'very-rare';
    if (set.category === 'ato') return 'archetype';
    if (set.category === 'pvp') return 'pvp';
    // Everything else — including event sets — lives under its real Set Category.
    return set.type;
  };

  // Derive the picker section (type tab + sidebar filter) that an already-slotted
  // enhancement belongs to, so opening "change" lands on it directly.
  const sectionForEnhancement = (
    enh: Enhancement,
  ): { typeFilter: EnhancementTypeFilter; sidebarFilter: SidebarFilter } | null => {
    switch (enh.type) {
      case 'io-generic':
        return { typeFilter: 'generic', sidebarFilter: 'all' };
      case 'origin':
        return { typeFilter: 'origin', sidebarFilter: 'all' };
      case 'special':
        return { typeFilter: 'special', sidebarFilter: 'all' };
      case 'io-set': {
        const set = availableSets.find((s) => (s.id || s.name) === enh.setId);
        return {
          typeFilter: 'io-sets',
          sidebarFilter: set ? sidebarFilterForSet(set) : 'all',
        };
      }
    }
  };

  const prevIsOpen = useRef(false);
  useEffect(() => {
    if (picker.isOpen && !prevIsOpen.current) {
      // Changing a filled slot → jump straight to that enhancement's section
      // (and, for IO sets, scroll its set into view). Empty slots have no
      // specific target, so fall back to the remembered / primary category.
      const slots = picker.virtualSlots ?? currentPowerSlots;
      const existing = slots[picker.currentSlotIndex] as Enhancement | null | undefined;
      const section = existing ? sectionForEnhancement(existing) : null;
      if (section) {
        setTypeFilter(section.typeFilter);
        setSidebarFilter(section.sidebarFilter);
        setJumpToSetId(existing?.type === 'io-set' ? (existing.setId ?? null) : null);
      } else {
        const remembered = picker.currentPowerName
          ? lastPickerFilterByPower[picker.currentPowerName]
          : undefined;
        if (remembered) {
          setTypeFilter(remembered.typeFilter as EnhancementTypeFilter);
          setSidebarFilter(remembered.sidebarFilter as SidebarFilter);
        } else if (primaryCategory) {
          setTypeFilter('io-sets');
          setSidebarFilter(primaryCategory);
        }
        setJumpToSetId(null);
      }
      setShiftSelected(new Map());
      setStackedSelected(new Map());
    }
    prevIsOpen.current = picker.isOpen;
  }, [picker.isOpen, picker.currentPowerName, picker.currentSlotIndex, picker.virtualSlots, currentPowerSlots, primaryCategory, lastPickerFilterByPower]);

  // Persist the filter the user lands on for this power so the next open
  // defaults to it. Runs whenever the active filter changes while open.
  useEffect(() => {
    if (!picker.isOpen || !picker.currentPowerName) return;
    const prev = lastPickerFilterByPower[picker.currentPowerName];
    if (prev?.typeFilter === typeFilter && prev?.sidebarFilter === sidebarFilter) return;
    setLastPickerFilter(picker.currentPowerName, typeFilter, sidebarFilter);
  }, [picker.isOpen, picker.currentPowerName, typeFilter, sidebarFilter, lastPickerFilterByPower, setLastPickerFilter]);

  // Helper to create IO set enhancement via registry factory
  const makeIOSetEnhancement = (set: IOSet, piece: IOSetPiece, pieceIndex: number) => {
    const clampedLevel = Math.max(set.minLevel, Math.min(globalIOLevel, set.maxLevel));
    return createIOSetEnhancement(set, piece, pieceIndex, { attuned: attunementEnabled, level: clampedLevel, boost: globalBoostLevel });
  };

  // Unified placement: routes to override handler (Compare Slotting) or build store
  const placeEnhancement = (powerName: string, slotIndex: number, enhancement: Enhancement) => {
    if (picker.onOverrideSelect) {
      picker.onOverrideSelect(slotIndex, enhancement);
    } else {
      setEnhancement(powerName, slotIndex, enhancement, picker.currentPowerCategory as import('@/stores').PowerCategory | undefined);
    }
  };

  // Handle selecting an IO set piece (single click)
  const handleSelectSetPiece = (set: IOSet, piece: IOSetPiece, pieceIndex: number) => {
    if (!picker.currentPowerName) return;
    placeEnhancement(picker.currentPowerName, picker.currentSlotIndex, makeIOSetEnhancement(set, piece, pieceIndex));
    closeEnhancementPicker();
  };

  // Handle slotting all shift-selected pieces (across sets)
  const handleSlotMultiSelect = () => {
    if (!picker.currentPowerName) return;

    // Gather all selected pieces in order
    const allPieces: { set: IOSet; piece: IOSetPiece; pieceIndex: number }[] = [];
    for (const [setId, indices] of shiftSelected) {
      const set = availableSets.find((s) => (s.id || s.name) === setId);
      if (!set) continue;
      for (const idx of Array.from(indices).sort((a, b) => a - b)) {
        // Re-check at placement: a queued piece can have become illegal since it
        // was selected (its twin slotted into this power, or a unique consumed
        // elsewhere in the build) — same skip the drag path applies.
        if (set.pieces[idx] && isPiecePlaceable(set, set.pieces[idx])) {
          allPieces.push({ set, piece: set.pieces[idx], pieceIndex: idx });
        }
      }
    }

    // Append stacked generic / special / origin selections after IO-set pieces.
    // Each stacked entry expands into `count` enhancements built lazily.
    const stackedEnhancements: Enhancement[] = [];
    for (const entry of stackedSelected.values()) {
      for (let i = 0; i < entry.count; i++) {
        stackedEnhancements.push(entry.build());
      }
    }

    const totalToFill = allPieces.length + stackedEnhancements.length;
    if (totalToFill === 0) return;

    // Fill empty slots: IO-set pieces first, then stacked items.
    const slotsToFill = emptySlotIndices.slice(0, totalToFill);
    allPieces.forEach(({ set, piece, pieceIndex }, idx) => {
      if (idx < slotsToFill.length) {
        placeEnhancement(
          picker.currentPowerName!,
          slotsToFill[idx],
          makeIOSetEnhancement(set, piece, pieceIndex)
        );
      }
    });
    stackedEnhancements.forEach((enh, idx) => {
      const slotIdx = allPieces.length + idx;
      if (slotIdx < slotsToFill.length) {
        placeEnhancement(picker.currentPowerName!, slotsToFill[slotIdx], enh);
      }
    });

    setShiftSelected(new Map());
    setStackedSelected(new Map());
    closeEnhancementPicker();
  };

  // Toggle a piece in the shift-selection
  const toggleShiftSelect = (set: IOSet, pieceIndex: number) => {
    const setId = set.id || set.name;
    setShiftSelected((prev) => {
      const next = new Map(prev);
      const indices = new Set(next.get(setId) || []);
      if (indices.has(pieceIndex)) {
        indices.delete(pieceIndex);
        if (indices.size === 0) next.delete(setId);
        else next.set(setId, indices);
      } else {
        indices.add(pieceIndex);
        next.set(setId, indices);
      }
      return next;
    });
  };

  // Check if any pieces or stacked tiles are queued for multi-slot
  const hasShiftSelection = shiftSelected.size > 0 || stackedSelected.size > 0;

  // Total count of pieces queued across set pieces + stacked tiles (used by
  // the sticky action bar and slot-cap warning).
  const totalSelectedPieces = useMemo(() => {
    let n = 0;
    for (const indices of shiftSelected.values()) n += indices.size;
    for (const entry of stackedSelected.values()) n += entry.count;
    return n;
  }, [shiftSelected, stackedSelected]);

  // Check if a piece is shift-selected
  const isShiftSelected = (set: IOSet, pieceIndex: number) => {
    const setId = set.id || set.name;
    return shiftSelected.get(setId)?.has(pieceIndex) || false;
  };

  // Handle drag selection - slot selected range of pieces
  const handleDragSelect = (set: IOSet, startIndex: number, endIndex: number) => {
    if (!picker.currentPowerName) return;

    const minIndex = Math.min(startIndex, endIndex);
    const maxIndex = Math.max(startIndex, endIndex);
    // The range sweeps every piece between the endpoints, including ones the
    // rows render disabled — skip those instead of placing them positionally.
    const selectedPieces = set.pieces
      .map((piece, i) => ({ piece, pieceIndex: i }))
      .slice(minIndex, maxIndex + 1)
      .filter(({ piece }) => isPiecePlaceable(set, piece));

    // Get empty slots starting from current slot
    const slotsToFill = emptySlotIndices.slice(0, selectedPieces.length);
    if (slotsToFill.length === 0) return;

    // Fill slots with selected pieces
    selectedPieces.forEach(({ piece, pieceIndex }, idx) => {
      if (idx < slotsToFill.length) {
        placeEnhancement(
          picker.currentPowerName!,
          slotsToFill[idx],
          makeIOSetEnhancement(set, piece, pieceIndex)
        );
      }
    });

    closeEnhancementPicker();
  };

  // Mouse handlers — desktop supports drag-to-select-range, shift+click toggle,
  // and (when selectMode is on or pieces are already selected) click-to-toggle.
  const handlePieceMouseDown = (set: IOSet, pieceIndex: number, e: React.MouseEvent) => {
    // No drag in selectMode or when using shift+click toggle
    if (e.shiftKey || selectMode) return;
    setIsDragging(true);
    setDragStartIndex(pieceIndex);
    setDragEndIndex(pieceIndex);
    setDragSet(set);
  };

  const handlePieceMouseEnter = (pieceIndex: number) => {
    if (isDragging) {
      setDragEndIndex(pieceIndex);
    }
  };

  const handlePieceMouseUp = (set: IOSet, pieceIndex: number, e: React.MouseEvent) => {
    const resetDrag = () => {
      setIsDragging(false);
      setDragStartIndex(null);
      setDragEndIndex(null);
      setDragSet(null);
    };

    // Select mode or shift+click: toggle selection
    if (selectMode || e.shiftKey) {
      toggleShiftSelect(set, pieceIndex);
      resetDrag();
      return;
    }

    if (isDragging && dragStartIndex !== null && dragSet?.id === set.id) {
      const start = dragStartIndex;
      const end = pieceIndex;

      if (start === end) {
        // Single click — slot this piece
        handleSelectSetPiece(set, set.pieces[pieceIndex], pieceIndex);
      } else {
        // Drag selection
        handleDragSelect(set, start, end);
      }
    }

    resetDrag();
  };

  // Touch handlers — tap toggles in selectMode, otherwise taps slot immediately.
  // The piece list scrolls freely (listeners are passive, so we never
  // preventDefault during the gesture), and we discriminate tap-from-scroll at
  // touchEnd by how far the finger traveled. Without this, lifting your finger
  // over a piece mid-scroll slots it and closes the picker — the "fat thumb"
  // trap a user reported.
  const pieceTouchStartRef = useRef<{ x: number; y: number } | null>(null);

  const handlePieceTouchStart = (_set: IOSet, _pieceIndex: number, e: React.TouchEvent) => {
    const t = e.touches[0];
    pieceTouchStartRef.current = t ? { x: t.clientX, y: t.clientY } : null;
  };

  const handlePieceTouchMove = (_e: React.TouchEvent) => {
    // No-op — movement is measured at touchEnd from changedTouches.
  };

  const handlePieceTouchEnd = (set: IOSet, pieceIndex: number, e: React.TouchEvent) => {
    // If the finger traveled past the tap tolerance, it was a scroll — ignore it
    // (don't slot, and don't preventDefault; let the scroll gesture stand).
    const start = pieceTouchStartRef.current;
    pieceTouchStartRef.current = null;
    const t = e.changedTouches[0];
    if (start && t && Math.hypot(t.clientX - start.x, t.clientY - start.y) > TAP_MOVE_TOLERANCE) {
      return;
    }

    e.preventDefault(); // Genuine tap — prevent the synthetic click double-firing

    if (selectMode) {
      toggleShiftSelect(set, pieceIndex);
    } else if (set.pieces[pieceIndex]) {
      handleSelectSetPiece(set, set.pieces[pieceIndex], pieceIndex);
    }
  };

  // Cancel multi-select — clear selected pieces and exit selectMode
  const handleCancelSelection = () => {
    setShiftSelected(new Map());
    setStackedSelected(new Map());
    setSelectMode(false);
  };

  // Global mouse/touch up to cancel drag if released outside
  useEffect(() => {
    const handleGlobalEnd = () => {
      if (isDragging) {
        setIsDragging(false);
        setDragStartIndex(null);
        setDragEndIndex(null);
        setDragSet(null);
      }
    };
    window.addEventListener('mouseup', handleGlobalEnd);
    window.addEventListener('touchend', handleGlobalEnd);
    window.addEventListener('touchcancel', handleGlobalEnd);
    return () => {
      window.removeEventListener('mouseup', handleGlobalEnd);
      window.removeEventListener('touchend', handleGlobalEnd);
      window.removeEventListener('touchcancel', handleGlobalEnd);
    };
  }, [isDragging]);

  // Whether a click on a stackable tile (generic / special / origin) should
  // queue into the multi-select stack instead of slotting immediately.
  // Triggered by the explicit Select-multiple toggle, by holding shift on
  // desktop, or by an existing in-progress multi-selection (so adding a
  // common IO on top of already-queued set pieces "just works").
  const isStackingClick = (e?: { shiftKey?: boolean }) =>
    selectMode || !!e?.shiftKey || hasShiftSelection;

  // Handle selecting a generic IO
  const handleSelectGenericIO = (stat: EnhancementStatType, e?: React.MouseEvent) => {
    if (!picker.currentPowerName) return;
    if (isStackingClick(e)) {
      incStacked(
        `generic:${stat}`,
        () => createGenericIOEnhancement(stat, genericIOLevel, globalBoostLevel),
        `${stat} IO`,
      );
      return;
    }
    placeEnhancement(picker.currentPowerName, picker.currentSlotIndex, createGenericIOEnhancement(stat, genericIOLevel, globalBoostLevel));
    closeEnhancementPicker();
  };

  // Handle selecting an origin enhancement
  const handleSelectOrigin = (stat: EnhancementStatType, tier: 'TO' | 'DO' | 'SO', e?: React.MouseEvent) => {
    if (!picker.currentPowerName) return;
    if (isStackingClick(e)) {
      incStacked(
        `origin:${tier}:${stat}`,
        () => createOriginEnhancement(stat, tier, buildOrigin, globalRelativeLevel),
        `${stat} ${tier}`,
      );
      return;
    }
    placeEnhancement(picker.currentPowerName, picker.currentSlotIndex, createOriginEnhancement(stat, tier, buildOrigin, globalRelativeLevel));
    closeEnhancementPicker();
  };

  // Handle selecting a special enhancement (Hamidon, Titan, Hydra, D-Sync)
  const handleSelectSpecial = (id: string, def: SpecialEnhancementDef, category: SpecialEnhancement['category'], e?: React.MouseEvent) => {
    if (!picker.currentPowerName) return;
    if (isStackingClick(e)) {
      incStacked(
        `special:${category}:${id}`,
        () => createSpecialEnhancement(id, def, category, globalRelativeLevel),
        def.name,
      );
      return;
    }
    placeEnhancement(picker.currentPowerName, picker.currentSlotIndex, createSpecialEnhancement(id, def, category, globalRelativeLevel));
    closeEnhancementPicker();
  };

  // Get available generic IOs and special enhancements for this power (via registry)
  const availableGenericIOs = useMemo(() => getAvailableGenericIOs(currentPower ?? null), [currentPower]);
  const availableHamidons = useMemo(() => getAvailableHamidons(currentPower ?? null), [currentPower]);
  const availableTitans = useMemo(() => getAvailableTitans(currentPower ?? null), [currentPower]);
  const availableHydras = useMemo(() => getAvailableHydras(currentPower ?? null), [currentPower]);
  const availableDSyncs = useMemo(() => getAvailableDSyncs(currentPower ?? null), [currentPower]);
  const availablePrestige = useMemo(() => getAvailablePrestige(currentPower ?? null), [currentPower]);

  return (
    <Modal
      isOpen={picker.isOpen}
      onClose={closeEnhancementPicker}
      title={`Select Enhancement for ${currentPower?.name || picker.currentPowerName || 'Power'}`}
      size="full"
      mobileFullscreen
      scrollBody={false}
    >
      <ModalBody className="p-0 h-full">
        {/* Fills the modal below lg (where it's fullscreen, matching Modal's
            mobileFullscreen breakpoint) and is a fixed 70vh card on desktop.
            The single scroll lives in the content pane below — the modal body
            itself does not scroll (scrollBody={false}). */}
        <div className="flex flex-col h-full lg:h-[70vh] min-h-0">
        {/* Type filter tabs at top */}
        <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between border-b border-gray-700 bg-gray-900/50 flex-shrink-0">
          <div className="flex overflow-x-auto scrollbar-thin">
            {[
              { id: 'io-sets' as const, label: 'IO Sets', title: 'Set IOs that grant set bonuses when slotted together' },
              { id: 'generic' as const, label: 'Generic IO', title: 'Single-aspect Invention enhancements (no set bonuses)' },
              { id: 'special' as const, label: 'Special', title: 'Special enhancements: Hamidon Origin (HO), D-Sync, Titan, etc.' },
              { id: 'origin' as const, label: 'Origin', title: 'Training, Dual, and Single Origin enhancements' },
            ].map((tab) => (
              <button
                key={tab.id}
                onClick={() => {
                  setTypeFilter(tab.id);
                  setSidebarFilter('all');
                }}
                title={tab.title}
                className={`px-3 sm:px-4 py-2 text-xs sm:text-sm font-medium transition-colors whitespace-nowrap ${
                  typeFilter === tab.id
                    ? 'text-[var(--color-link)] border-b-2 border-[var(--color-selected)] bg-gray-800/50'
                    : 'text-gray-400 hover:text-gray-200 hover:bg-gray-800/30'
                }`}
              >
                {tab.label}
              </button>
            ))}
          </div>
          {/* IO Level adjuster + Attunement toggle + Boost dial */}
          <div className="px-3 sm:px-4 py-1.5 sm:py-0 border-t sm:border-t-0 border-gray-700 flex items-center gap-4">
            <div
              className={`flex items-center gap-1.5 ${attunementEnabled ? 'opacity-40 pointer-events-none' : ''}`}
              title={
                onGenericTab
                  ? `Crafting level for generic IOs — the ${ioCraftLevels.length} levels the game crafts them at (${ioCraftLevels.join(', ')}). Higher level = stronger effect, but exemplaring below the level disables it.`
                  : `Crafting level for IO enhancements (${ioCraftLevels[0]}–${ioCraftLevels[ioCraftLevels.length - 1]}). Higher level = stronger effect, but exemplaring below the level disables it.`
              }
            >
              <span className="text-xs text-gray-400">Lv</span>
              <LevelSpinner
                value={onGenericTab ? genericIOLevel : globalIOLevel}
                min={ioCraftLevels[0]}
                max={ioCraftLevels[ioCraftLevels.length - 1]}
                allowedValues={onGenericTab ? ioCraftLevels : undefined}
                onChange={setGlobalIOLevel}
                disabled={attunementEnabled}
                decreaseTitle="Decrease IO crafting level"
                increaseTitle="Increase IO crafting level"
                valueTitle={
                  onGenericTab
                    ? `Drag up/down to change, click to type (${ioCraftLevels.join(', ')})`
                    : `Drag up/down to change, click to type (${ioCraftLevels[0]}–${ioCraftLevels[ioCraftLevels.length - 1]})`
                }
                valueColorClass={attunementEnabled ? 'text-gray-500' : 'text-blue-400'}
                widthClass="w-6"
              />
            </div>
            <Toggle
              id="attunement-toggle-picker"
              name="attunement"
              checked={attunementEnabled}
              onChange={toggleAttunement}
              label="Attuned"
              title="Attuned IOs scale with your current level — never lose effect when exemplaring."
            />
            <div
              className="flex items-center gap-1.5"
              title={
                levelOffsetAxis === 'relative'
                  ? `Relative level (${levelOffsetRange.min} to +${levelOffsetRange.max}) — how far the enhancement's level sits from yours. Below even it is weaker.`
                  : `Catalyst boost level (+0 to +${levelOffsetRange.max}). Each boost increases enhancement strength.`
              }
            >
              <span className="text-xs text-gray-400">
                {levelOffsetAxis === 'relative' ? 'Rel. Level' : 'Boost'}
              </span>
              <LevelSpinner
                value={levelOffset}
                min={levelOffsetRange.min}
                max={levelOffsetRange.max}
                onChange={setLevelOffset}
                showPlus
                decreaseTitle="Decrease level offset"
                increaseTitle="Increase level offset"
                valueTitle={`Drag up/down to change, click to type (${levelOffsetRange.min}–${levelOffsetRange.max})`}
                valueColorClass={
                  levelOffset > 0
                    ? 'text-green-400'
                    : levelOffset < 0
                      ? 'text-red-400'
                      : 'text-gray-500'
                }
              />
            </div>
            <button
              onClick={applyOffsetToSlotted}
              title={
                levelOffsetAxis === 'relative'
                  ? `Set every slotted origin and special enhancement to ${levelOffset === 0 ? 'even' : levelOffset > 0 ? `+${levelOffset}` : levelOffset} relative level. The spinner alone only affects enhancements you slot from here on.`
                  : `Apply +${levelOffset} to every enhancement already slotted in this build. The spinner alone only affects enhancements you slot from here on; attuned and sub-50 IOs cannot carry boosters.`
              }
              className="px-2 py-0.5 text-xs rounded border border-gray-600 text-gray-300 hover:text-gray-100 hover:border-gray-400 hover:bg-gray-800/60 transition-colors whitespace-nowrap"
            >
              Apply to slotted
            </button>
            {bulkApplied !== null && (
              <span className={`text-xs whitespace-nowrap ${bulkApplied > 0 ? 'text-green-400' : 'text-gray-500'}`}>
                {bulkApplied > 0
                  ? `${bulkApplied} updated`
                  : 'nothing eligible'}
              </span>
            )}
          </div>
        </div>

        {/* Category filter — wraps to multiple rows on mobile so the picker
            scrolls vertically only (no horizontal scrollbar mid-content). */}
        {typeFilter === 'io-sets' && availableSets.length > 0 && (
          <div className="flex sm:hidden flex-wrap border-b border-gray-700 bg-gray-900/30 flex-shrink-0 gap-1 px-2 py-1.5">
            <MobileCategoryButton
              label="All"
              count={availableSets.filter((s) => !isSpecialSet(s)).length}
              isActive={sidebarFilter === 'all'}
              onClick={() => setSidebarFilter('all')}
              title="Show every standard set category for this power"
            />
            {standardCategories.map((cat) => (
              <MobileCategoryButton
                key={cat}
                label={cat.replace(' Damage', '').replace(' Sets', '')}
                count={availableSets.filter((s) => s.type === cat && (cat === 'Universal Control Duration Sets' || cat === 'Rest Buff' || cat === 'Universal Debuff' || cat === 'Rez Sets' || !isSpecialSet(s))).length}
                isActive={sidebarFilter === cat}
                onClick={() => setSidebarFilter(cat)}
                textColor={cat === primaryCategory ? 'text-yellow-400' : undefined}
                title={cat === primaryCategory ? `${cat} (this power's primary set category)` : cat}
              />
            ))}
            {hasUniversal && (
              <MobileCategoryButton
                label="Universal"
                count={availableSets.filter((s) => s.type === 'Universal Damage Sets').length}
                isActive={sidebarFilter === 'universal'}
                onClick={() => setSidebarFilter('universal')}
                title="Universal Damage sets — slot in any damaging power regardless of category"
              />
            )}
            {hasVeryRare && (
              <MobileCategoryButton
                label="Very Rare"
                count={availableSets.filter((s) => s.category === 'purple').length}
                isActive={sidebarFilter === 'very-rare'}
                onClick={() => setSidebarFilter('very-rare')}
                textColor="text-purple-400"
                title="Very Rare (Purple) sets — level 50 only, but always exemplar-safe"
              />
            )}
            {hasArchetype && (
              <MobileCategoryButton
                label="ATO"
                count={availableSets.filter((s) => s.category === 'ato').length}
                isActive={sidebarFilter === 'archetype'}
                onClick={() => setSidebarFilter('archetype')}
                textColor="text-orange-400"
                title="Archetype Origin sets — exclusive to your AT, slottable from level 10"
              />
            )}
            {hasPvP && (
              <MobileCategoryButton
                label="PvP"
                count={availableSets.filter((s) => s.category === 'pvp').length}
                isActive={sidebarFilter === 'pvp'}
                onClick={() => setSidebarFilter('pvp')}
                textColor="text-red-400"
                title="PvP IO sets — earned through PvP zones/arenas; attuned by default"
              />
            )}
            {hasProcs && (
              <MobileCategoryButton
                label="Procs"
                count={availableSets.reduce((n, s) => n + s.pieces.filter((p) => p.proc).length, 0)}
                isActive={sidebarFilter === 'procs'}
                onClick={() => setSidebarFilter('procs')}
                textColor="text-amber-400"
                title="All proc enhancements (chance-for-X effects) across every set"
              />
            )}
            {/* Set size — a second axis, so it sits after a divider rather than
                joining the category run. Hidden when every set here is the same
                size (nothing to choose) or in Procs, which lists loose pieces. */}
            {sidebarFilter !== 'procs' && sizeCounts.length > 1 && (
              <>
                <span className="self-stretch w-px bg-gray-700 mx-0.5" aria-hidden="true" />
                <MobileCategoryButton
                  label="Any size"
                  count={categorySets.length}
                  isActive={effectiveSizeFilter === null}
                  onClick={() => setSizeFilter(null)}
                  title="Sets of every size"
                />
                {sizeCounts.map(([size, count]) => (
                  <MobileCategoryButton
                    key={size}
                    label={`${size}pc`}
                    count={count}
                    isActive={effectiveSizeFilter === size}
                    onClick={() => setSizeFilter(size)}
                    title={`Only ${size}-piece sets — the full set fits in ${size} slots`}
                  />
                ))}
              </>
            )}
          </div>
        )}

        <div className="flex flex-1 min-h-0">
          {/* Category sidebar - desktop only */}
          {typeFilter === 'io-sets' && availableSets.length > 0 && (
            <div className="hidden sm:block w-48 border-r border-gray-700 overflow-y-auto flex-shrink-0 bg-gray-900/30">
              {/* All (standard sets only) */}
              <SidebarButton
                label="All"
                count={availableSets.filter((s) => !isSpecialSet(s)).length}
                isActive={sidebarFilter === 'all'}
                onClick={() => setSidebarFilter('all')}
                title="Show every standard set category for this power"
              />

              {/* Standard set categories (data-driven from power's allowed sets) */}
              {standardCategories.map((cat) => (
                <SidebarButton
                  key={cat}
                  label={cat}
                  count={availableSets.filter((s) => s.type === cat && (cat === 'Universal Control Duration Sets' || cat === 'Rest Buff' || cat === 'Universal Debuff' || cat === 'Rez Sets' || !isSpecialSet(s))).length}
                  isActive={sidebarFilter === cat}
                  onClick={() => setSidebarFilter(cat)}
                  textColor={cat === primaryCategory ? 'text-yellow-400' : undefined}
                  title={cat === primaryCategory ? `${cat} (this power's primary set category)` : cat}
                />
              ))}

              {/* Universal Damage */}
              {hasUniversal && (
                <SidebarButton
                  label="Universal Damage"
                  count={availableSets.filter((s) => s.type === 'Universal Damage Sets').length}
                  isActive={sidebarFilter === 'universal'}
                  onClick={() => setSidebarFilter('universal')}
                  title="Universal Damage sets — slot in any damaging power regardless of category"
                />
              )}

              {/* Very Rare (Purple) */}
              {hasVeryRare && (
                <SidebarButton
                  label="Very Rare"
                  count={availableSets.filter((s) => s.category === 'purple').length}
                  isActive={sidebarFilter === 'very-rare'}
                  onClick={() => setSidebarFilter('very-rare')}
                  textColor="text-purple-400"
                  title="Very Rare (Purple) sets — level 50 only, but always exemplar-safe"
                />
              )}

              {/* Archetype (ATO) */}
              {hasArchetype && (
                <SidebarButton
                  label="Archetype"
                  count={availableSets.filter((s) => s.category === 'ato').length}
                  isActive={sidebarFilter === 'archetype'}
                  onClick={() => setSidebarFilter('archetype')}
                  textColor="text-orange-400"
                  title="Archetype Origin sets — exclusive to your AT, slottable from level 10"
                />
              )}

              {/* PvP */}
              {hasPvP && (
                <SidebarButton
                  label="PvP"
                  count={availableSets.filter((s) => s.category === 'pvp').length}
                  isActive={sidebarFilter === 'pvp'}
                  onClick={() => setSidebarFilter('pvp')}
                  textColor="text-red-400"
                  title="PvP IO sets — earned through PvP zones/arenas; attuned by default"
                />
              )}

              {/* Procs */}
              {hasProcs && (
                <SidebarButton
                  label="Procs"
                  count={availableSets.reduce((n, s) => n + s.pieces.filter((p) => p.proc).length, 0)}
                  isActive={sidebarFilter === 'procs'}
                  onClick={() => setSidebarFilter('procs')}
                  textColor="text-amber-400"
                  title="All proc enhancements (chance-for-X effects) across every set"
                />
              )}

              {/* Set size — a second axis that cuts ACROSS the categories above,
                  so it gets its own headed section instead of extending the
                  category run. Hidden when every set in the current category is
                  the same size (nothing to choose), and in Procs, which lists
                  loose pieces rather than sets. */}
              {sidebarFilter !== 'procs' && sizeCounts.length > 1 && (
                <div className="mt-1 border-t border-gray-700 pt-1">
                  <div className="px-3 pt-1 pb-0.5 text-[10px] font-semibold uppercase tracking-wider text-gray-500">
                    Set size
                  </div>
                  <SidebarButton
                    label="Any size"
                    count={categorySets.length}
                    isActive={effectiveSizeFilter === null}
                    onClick={() => setSizeFilter(null)}
                    title="Sets of every size"
                  />
                  {sizeCounts.map(([size, count]) => (
                    <SidebarButton
                      key={size}
                      label={`${size} pieces`}
                      count={count}
                      isActive={effectiveSizeFilter === size}
                      onClick={() => setSizeFilter(size)}
                      title={`Only ${size}-piece sets — the full set fits in ${size} slots`}
                    />
                  ))}
                </div>
              )}
            </div>
          )}

          {/* Main content area — the picker's single scroll region.
              overflow-x-hidden keeps it vertical-only (overflow-y-auto alone
              implies overflow-x:auto, which surfaced a stray horizontal
              scrollbar on mobile). */}
          <div
            className="flex-1 overflow-y-auto overflow-x-hidden p-2 pr-4 sm:p-3"
            onContextMenu={(e) => { if (e.shiftKey) e.preventDefault(); }}
          >
            <div className="flex items-center justify-end gap-1 mb-2">
              {!isTouch && !selectMode && (
                <span className="text-xs text-gray-500 mr-1 whitespace-nowrap">
                  or hold{' '}
                  <kbd className="px-1 py-px rounded border border-gray-600 bg-gray-800 text-gray-300 font-sans text-[10px]">
                    Shift
                  </kbd>{' '}
                  and click
                </span>
              )}
              <button
                onClick={() => setSelectMode((m) => !m)}
                title={
                  isTouch
                    ? 'When on, taps queue selections instead of slotting immediately. For common IOs / HOs / Origins, each tap adds another copy. Use the action bar at the bottom to slot all queued enhancements at once.'
                    : 'When on, clicks queue selections instead of slotting immediately — the same thing holding Shift does, without having to hold it. For common IOs / HOs / Origins, each click adds another copy. Use the action bar at the bottom to slot all queued enhancements at once.'
                }
                className={`text-xs px-2 py-0.5 rounded mr-1 transition-colors ${
                  selectMode
                    ? 'bg-green-600 text-on-success hover:bg-green-500'
                    : 'bg-gray-700 text-gray-300 hover:bg-gray-600 hover:text-white'
                }`}
              >
                {selectMode ? 'Select: On' : 'Select multiple'}
              </button>
              {typeFilter === 'io-sets' && sidebarFilter !== 'procs' && (
                  <>
                    <span className="text-xs text-gray-500 mr-1">Sort:</span>
                    <button
                      onClick={() => setIOSortBy('name')}
                      className={`text-xs px-1.5 py-0.5 rounded ${ioSortBy === 'name' ? 'bg-[var(--color-selected)] text-[var(--color-primary-fg)]' : 'text-gray-400 hover:text-gray-200'}`}
                    >
                      A-Z
                    </button>
                    <button
                      onClick={() => setIOSortBy('level')}
                      className={`text-xs px-1.5 py-0.5 rounded ${ioSortBy === 'level' ? 'bg-[var(--color-selected)] text-[var(--color-primary-fg)]' : 'text-gray-400 hover:text-gray-200'}`}
                    >
                      Level
                    </button>
                  </>
                )}
            </div>
            {typeFilter === 'io-sets' && sidebarFilter === 'procs' && (
              <ProcsContent
                pieces={procPieces}
                attunementEnabled={attunementEnabled}
                isPieceInCurrentPower={isPieceInCurrentPower}
                onPieceMouseDown={handlePieceMouseDown}
                onPieceMouseUp={handlePieceMouseUp}
                onPieceTouchStart={handlePieceTouchStart}
                onPieceTouchMove={handlePieceTouchMove}
                onPieceTouchEnd={handlePieceTouchEnd}
                isShiftSelected={isShiftSelected}
              />
            )}
            {typeFilter === 'io-sets' && sidebarFilter !== 'procs' && (
              <IOSetsContent
                sets={filteredSets}
                globalIOLevel={globalIOLevel}
                attunementEnabled={attunementEnabled}
                onPieceMouseDown={handlePieceMouseDown}
                onPieceMouseEnter={handlePieceMouseEnter}
                onPieceMouseUp={handlePieceMouseUp}
                onPieceTouchStart={handlePieceTouchStart}
                onPieceTouchMove={handlePieceTouchMove}
                onPieceTouchEnd={handlePieceTouchEnd}
                isDragging={isDragging}
                dragSet={dragSet}
                dragStartIndex={dragStartIndex}
                dragEndIndex={dragEndIndex}
                isPieceInCurrentPower={isPieceInCurrentPower}
                isShiftSelected={isShiftSelected}
                jumpToSetId={jumpToSetId}
              />
            )}

            {typeFilter === 'generic' && (
              <GenericIOContent
                availableIOs={availableGenericIOs}
                craftLevel={genericIOLevel}
                onSelect={handleSelectGenericIO}
                stackedCountFor={(stat) => stackedCountFor(`generic:${stat}`)}
                onDecrement={(stat) => decStacked(`generic:${stat}`)}
              />
            )}

            {typeFilter === 'special' && (
              <SpecialContent
                availableHamidons={availableHamidons}
                availableTitans={availableTitans}
                availableHydras={availableHydras}
                availableDSyncs={availableDSyncs}
                availablePrestige={availablePrestige}
                onSelect={handleSelectSpecial}
                stackedCountFor={(category, id) => stackedCountFor(`special:${category}:${id}`)}
                onDecrement={(category, id) => decStacked(`special:${category}:${id}`)}
              />
            )}

            {typeFilter === 'origin' && (
              <OriginContent
                availableTypes={availableGenericIOs}
                onSelect={handleSelectOrigin}
                stackedCountFor={(stat, tier) => stackedCountFor(`origin:${tier}:${stat}`)}
                onDecrement={(stat, tier) => decStacked(`origin:${tier}:${stat}`)}
              />
            )}


          </div>
        </div>

        {/* Sticky commit bar — visible when any pieces are selected for multi-slot */}
        {hasShiftSelection && (
          <div className="flex-shrink-0 border-t border-gray-700 bg-gray-900/95 backdrop-blur px-3 py-2 flex items-center gap-2">
            <span className="text-xs sm:text-sm text-gray-300 flex-1 min-w-0 truncate">
              {totalSelectedPieces} piece{totalSelectedPieces === 1 ? '' : 's'} selected
              {emptySlotIndices.length < totalSelectedPieces && (
                <span className="text-amber-400 ml-2">
                  (only {emptySlotIndices.length} empty slot{emptySlotIndices.length === 1 ? '' : 's'} — extras ignored)
                </span>
              )}
            </span>
            <button
              onClick={handleCancelSelection}
              className="text-xs sm:text-sm px-3 py-1.5 rounded bg-gray-700 hover:bg-gray-600 text-gray-200 transition-colors"
            >
              Cancel
            </button>
            <button
              onClick={handleSlotMultiSelect}
              disabled={emptySlotIndices.length === 0}
              className="text-xs sm:text-sm px-3 py-1.5 rounded bg-green-600 hover:bg-green-500 text-on-success font-medium transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
            >
              Slot {Math.min(totalSelectedPieces, emptySlotIndices.length)}
            </button>
          </div>
        )}
        </div>
      </ModalBody>
    </Modal>
  );
}

// ============================================
// SIDEBAR BUTTON
// ============================================

interface SidebarButtonProps {
  label: string;
  count: number;
  isActive: boolean;
  onClick: () => void;
  textColor?: string;
  title?: string;
}

function SidebarButton({ label, count, isActive, onClick, textColor, title }: SidebarButtonProps) {
  return (
    <button
      onClick={onClick}
      title={title}
      className={`w-full px-3 py-2 text-left text-sm transition-colors ${
        isActive
          ? 'bg-[var(--color-selected)]/20 text-link border-l-2 border-[var(--color-selected)]'
          : `${textColor || 'text-gray-400'} hover:bg-gray-800/50 hover:text-gray-200`
      }`}
    >
      {label} ({count})
    </button>
  );
}

// ============================================
// MOBILE CATEGORY BUTTON
// ============================================

interface MobileCategoryButtonProps {
  label: string;
  count: number;
  isActive: boolean;
  onClick: () => void;
  textColor?: string;
  title?: string;
}

function MobileCategoryButton({ label, count, isActive, onClick, textColor, title }: MobileCategoryButtonProps) {
  return (
    <button
      onClick={onClick}
      title={title}
      className={`px-2.5 py-1 text-xs font-medium rounded-full whitespace-nowrap transition-colors ${
        isActive
          ? 'bg-[var(--color-selected)] text-selected-fg'
          : `bg-gray-700 ${textColor || 'text-gray-300'} hover:bg-gray-600`
      }`}
    >
      {label} ({count})
    </button>
  );
}

// ============================================
// IO SETS CONTENT
// ============================================

interface IOSetsContentProps {
  sets: IOSet[];
  globalIOLevel: number;
  attunementEnabled: boolean;
  onPieceMouseDown: (set: IOSet, pieceIndex: number, e: React.MouseEvent) => void;
  onPieceMouseEnter: (pieceIndex: number) => void;
  onPieceMouseUp: (set: IOSet, pieceIndex: number, e: React.MouseEvent) => void;
  onPieceTouchStart: (set: IOSet, pieceIndex: number, e: React.TouchEvent) => void;
  onPieceTouchMove: (e: React.TouchEvent) => void;
  onPieceTouchEnd: (set: IOSet, pieceIndex: number, e: React.TouchEvent) => void;
  isDragging: boolean;
  dragSet: IOSet | null;
  dragStartIndex: number | null;
  dragEndIndex: number | null;
  isPieceInCurrentPower: (setId: string, pieceNum: number) => boolean;
  isShiftSelected: (set: IOSet, pieceIndex: number) => boolean;
  /** Set id to scroll into view + highlight on open (changing a slotted piece). */
  jumpToSetId?: string | null;
}

function IOSetsContent({
  sets,
  onPieceMouseDown,
  onPieceMouseEnter,
  onPieceMouseUp,
  onPieceTouchStart,
  onPieceTouchMove,
  onPieceTouchEnd,
  isDragging,
  dragSet,
  dragStartIndex,
  dragEndIndex,
  isPieceInCurrentPower,
  isShiftSelected,
  jumpToSetId,
}: IOSetsContentProps) {
  if (sets.length === 0) {
    return <div className="text-center text-gray-500 py-8">No IO sets available for this power</div>;
  }

  return (
    <div className="space-y-3">
      {sets.map((set) => (
        <IOSetRow
          key={set.id || set.name}
          set={set}
          onPieceMouseDown={onPieceMouseDown}
          onPieceMouseEnter={onPieceMouseEnter}
          onPieceMouseUp={onPieceMouseUp}
          onPieceTouchStart={onPieceTouchStart}
          onPieceTouchMove={onPieceTouchMove}
          onPieceTouchEnd={onPieceTouchEnd}
          isDragging={isDragging && dragSet?.id === set.id}
          dragStartIndex={dragSet?.id === set.id ? dragStartIndex : null}
          dragEndIndex={dragSet?.id === set.id ? dragEndIndex : null}
          isPieceInCurrentPower={isPieceInCurrentPower}
          isShiftSelected={isShiftSelected}
          jumpTarget={!!jumpToSetId && (set.id || set.name) === jumpToSetId}
        />
      ))}
    </div>
  );
}

// ============================================
// PROCS FLAT LIST
// ============================================

interface ProcsContentProps {
  pieces: { set: IOSet; piece: IOSetPiece; pieceIndex: number }[];
  attunementEnabled: boolean;
  isPieceInCurrentPower: (setId: string, pieceNum: number) => boolean;
  onPieceMouseDown: (set: IOSet, pieceIndex: number, e: React.MouseEvent) => void;
  onPieceMouseUp: (set: IOSet, pieceIndex: number, e: React.MouseEvent) => void;
  onPieceTouchStart: (set: IOSet, pieceIndex: number, e: React.TouchEvent) => void;
  onPieceTouchMove: (e: React.TouchEvent) => void;
  onPieceTouchEnd: (set: IOSet, pieceIndex: number, e: React.TouchEvent) => void;
  isShiftSelected: (set: IOSet, pieceIndex: number) => boolean;
}

function ProcsContent({
  pieces,
  attunementEnabled,
  isPieceInCurrentPower,
  onPieceMouseDown,
  onPieceMouseUp,
  onPieceTouchStart,
  onPieceTouchMove,
  onPieceTouchEnd,
  isShiftSelected,
}: ProcsContentProps) {
  const isUniqueEnhancementSlotted = useBuildStore((s) => s.isUniqueEnhancementSlotted);
  const isCompareMode = useUIStore((s) => s.enhancementPicker.virtualSlots) !== null;
  // Proc damage scales with the CHARACTER's level, not the IO's crafted/global-IO
  // level — preview at the level it'll actually deal once slotted. (@Redlynne, 2026-06-12)
  const procDamageLevel = useBuildStore((s) => s.build.level);

  if (pieces.length === 0) {
    return <div className="text-center text-gray-500 py-8">No procs available for this power</div>;
  }

  return (
    <div className="space-y-1">
      {pieces.map(({ set, piece, pieceIndex }) => {
        const setId = set.id || set.name;
        const outline = getEnhancementOutline(
          { name: piece.name, proc: piece.proc, unique: piece.unique },
          set.name,
        );

        // Check disabled state
        // In compare mode, skip build-wide unique check — each configuration is independent
        let disabledReason: string | null = null;
        if (isPieceInCurrentPower(setId, piece.num)) {
          disabledReason = 'Already in this power';
        } else if (!isCompareMode) {
          // Event is NOT wholesale unique-per-build — its uniqueness lives in the
          // per-piece `unique` flag (see isPieceDisabled). Keep purple/ATO as
          // category-unique (tspy Primalist ATO ships unique=0).
          const isSpecialRarity = set.category === 'purple' || set.category === 'ato';
          if ((piece.unique || isSpecialRarity) && isUniqueEnhancementSlotted(setId, piece.num)) {
            disabledReason = 'Already slotted in build';
          }
        }
        const isDisabled = !!disabledReason;

        // Get proc effect info
        const procData = findProcData(piece.name, set.name);
        const procEffect = procData ? procEffectSummary(procData) : null;
        const procLabel = procEffect ? getProcEffectLabel(procEffect.category) : null;
        const procColor = procEffect ? getProcEffectColor(procEffect.category) : outline.color;

        const shiftSel = isShiftSelected(set, pieceIndex);

        const tooltipContent = procData && procEffect ? (
          <div className="space-y-1 text-xs">
            <div className="text-slate-300">{procData.mechanics}</div>
            <div className="flex flex-wrap gap-x-3 gap-y-0.5">
              {procData.ppm !== null && (
                <span>
                  <span className="text-slate-400">PPM:</span>
                  <span className="text-amber-300 ml-1 font-medium">{procData.ppm}</span>
                </span>
              )}
              <span>
                <span className="text-slate-400">Type:</span>
                <span className={`ml-1 ${
                  procData.type === 'Proc120s' ? 'text-purple-400' :
                  procData.type === 'Global' ? 'text-green-400' :
                  'text-amber-300'
                }`}>
                  {procData.type === 'Proc120s' ? '100% (120s)' : procData.type}
                </span>
              </span>
              {procEffect.category === 'Damage' && procEffect.value !== undefined && procEffect.valueMax !== undefined && (
                <span>
                  <span className="text-slate-400">Dmg:</span>
                  <span className="text-red-400 ml-1">
                    {interpolateProcDamage(procEffect.value, procEffect.valueMax, procData.levelRange, procDamageLevel)} {procEffect.effectType}
                  </span>
                </span>
              )}
              {procEffect.value !== undefined && !(procEffect.category === 'Damage' && procEffect.valueMax !== undefined) && (
                <span>
                  <span className="text-slate-400">Value:</span>
                  <span className="ml-1" style={{ color: procColor }}>
                    {procEffect.category === 'KnockbackProtection' ? `Mag ${procEffect.value}` :
                     procEffect.category === 'Stealth' ? `${procEffect.value} ft` :
                     `${procEffect.value}%`}
                    {procEffect.effectType ? ` ${procEffect.effectType}` : ''}
                  </span>
                </span>
              )}
              {isProcAlwaysOn(procData) && (
                <span className="text-green-400">Always On</span>
              )}
            </div>
          </div>
        ) : (
          <div className="text-xs text-slate-300">{resolveProcPieceName(piece.name, set.name, piece.proc)}</div>
        );

        return (
          <Tooltip key={`${setId}-${piece.num}`} content={tooltipContent} position="right" className="!max-w-sm" triggerClassName="block">
          <button
            onMouseDown={(e) => !isDisabled && onPieceMouseDown(set, pieceIndex, e)}
            onMouseUp={(e) => !isDisabled && onPieceMouseUp(set, pieceIndex, e)}
            onTouchStart={(e) => !isDisabled && onPieceTouchStart(set, pieceIndex, e)}
            onTouchMove={(e) => !isDisabled && onPieceTouchMove(e)}
            onTouchEnd={(e) => !isDisabled && onPieceTouchEnd(set, pieceIndex, e)}
            disabled={isDisabled}
            className={`w-full flex items-center gap-2 p-2 rounded border transition-all ${
              isDisabled
                ? 'border-gray-700 opacity-40 cursor-not-allowed bg-gray-900/30'
                : shiftSel
                  ? 'border-green-400 bg-green-900/20 ring-1 ring-green-400/50'
                  : 'border-gray-600 bg-gray-800/40 hover:border-[var(--color-selected)] hover:bg-[var(--color-selected)]/10'
            }`}
            style={{ touchAction: 'pan-y' }}
          >
            {/* Set icon */}
            <div className="relative flex-shrink-0">
              <IOSetIcon
                icon={set.icon || 'Unknown.png'}
                attuned={attunementEnabled || isInherentlyAttuned(set)}
                category={set.category}
                size={30}
                alt={piece.name}
                className="pointer-events-none"
              />
              {outline.show && (
                <div
                  className="absolute -top-0.5 right-0.5 w-2 h-2 rounded-full border border-gray-900 pointer-events-none"
                  style={{
                    background: outline.secondaryColor
                      ? `linear-gradient(135deg, ${outline.color} 50%, ${outline.secondaryColor} 50%)`
                      : outline.color,
                  }}
                />
              )}
            </div>

            {/* Piece info */}
            <div className="flex-1 text-left min-w-0">
              <div className="text-sm font-medium text-gray-200 truncate">
                {resolveProcPieceName(piece.name, set.name, piece.proc)}
              </div>
              <div className="text-xs text-gray-500 truncate">
                {set.name}
                <span className="text-gray-600"> · Lv {set.minLevel}-{set.maxLevel}</span>
              </div>
            </div>

            {/* Proc badge */}
            <div className="flex-shrink-0 text-right">
              <span className="text-xs font-medium" style={{ color: procColor }}>
                {procLabel || 'Proc'}
              </span>
              {disabledReason && (
                <div className="text-xs text-orange-400">{disabledReason}</div>
              )}
            </div>
          </button>
          </Tooltip>
        );
      })}
    </div>
  );
}

interface IOSetRowProps {
  set: IOSet;
  onPieceMouseDown: (set: IOSet, pieceIndex: number, e: React.MouseEvent) => void;
  onPieceMouseEnter: (pieceIndex: number) => void;
  onPieceMouseUp: (set: IOSet, pieceIndex: number, e: React.MouseEvent) => void;
  onPieceTouchStart: (set: IOSet, pieceIndex: number, e: React.TouchEvent) => void;
  onPieceTouchMove: (e: React.TouchEvent) => void;
  onPieceTouchEnd: (set: IOSet, pieceIndex: number, e: React.TouchEvent) => void;
  isDragging: boolean;
  dragStartIndex: number | null;
  dragEndIndex: number | null;
  isPieceInCurrentPower: (setId: string, pieceNum: number) => boolean;
  isShiftSelected: (set: IOSet, pieceIndex: number) => boolean;
  /** True when this is the slotted set being changed — scroll into view + flash. */
  jumpTarget?: boolean;
}

/** How many chips fit before the row starts to dominate the picker. */
const MAX_TRACKED_CHIPS = 5;

/**
 * Inline summary of the bonuses a set grants for the player's tracked stats.
 *
 * The row was already tinted when a set matched a tracked stat, but the match
 * DETAIL was computed and thrown away — so the player still had to hover each
 * piece to learn whether the highlight meant "+1.5% at 6 pieces" or "+5% at 2".
 * That is the whole decision, and on touch there is no hover at all.
 *
 * Rule-of-5 state is folded in because a capped bonus is worth zero: advertising
 * "+3% Res S/L" for the sixth time is worse than showing nothing.
 */
function TrackedBonusChips({ bonuses }: { bonuses: TrackedBonusMatch[] }) {
  const bonusTracking = useBonusTracking();
  if (bonuses.length === 0) return null;

  const shown = bonuses.slice(0, MAX_TRACKED_CHIPS);
  const overflow = bonuses.length - shown.length;

  return (
    <div className="flex flex-wrap items-center gap-1 mb-2">
      {shown.map((b) => {
        const capped = isBonusCapped(bonusTracking, b.normalizedStat, b.value);
        const count = getTotalBonusCount(bonusTracking, b.normalizedStat, b.value);
        return (
          <span
            key={`${b.trackedKey}-${b.pieces}`}
            title={
              formatBonusDesc(b.desc, b.stat, b.value) +
              (capped ? ' — already at the Rule of 5 cap; a 6th copy grants nothing' : '')
            }
            /* Chip text is the neutral ramp, never the accent: the row this sits
               in is ALREADY tinted --color-selected, so accent-colored text at
               10px lands accent-on-accent (blue on blue at ~2:1). The accent
               identity is carried by the border and tint; readability by the
               ramp, which reverses under [data-mode='light']. */
            className={`inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-[10px] leading-none border ${
              capped
                ? 'border-warning-fg/50 bg-warning-fg/15 text-warning-fg'
                : 'border-[var(--color-primary)]/60 bg-[var(--color-primary)]/15 text-gray-100'
            }`}
          >
            <span className="opacity-70 font-medium">{b.pieces}pc</span>
            <span className={`font-semibold ${capped ? 'line-through' : ''}`}>
              {formatTrackedBonusAmount(b.normalizedStat, b.value)}
            </span>
            <span className="opacity-80">{statKeyToChipLabel(b.normalizedStat)}</span>
            {count > 0 && <span className="opacity-70">({count}/5)</span>}
          </span>
        );
      })}
      {overflow > 0 && (
        <span className="text-[10px] text-gray-500">+{overflow} more</span>
      )}
    </div>
  );
}

function IOSetRow({
  set,
  onPieceMouseDown,
  onPieceMouseEnter,
  onPieceMouseUp,
  onPieceTouchStart,
  onPieceTouchMove,
  onPieceTouchEnd,
  isDragging,
  dragStartIndex,
  dragEndIndex,
  isPieceInCurrentPower,
  isShiftSelected,
  jumpTarget,
}: IOSetRowProps) {
  const rowRef = useRef<HTMLDivElement>(null);
  const [jumpFlash, setJumpFlash] = useState(false);
  // Mobile-only: set bonuses are collapsed by default to keep the (often long)
  // set list compact. Desktop reveals the same data via the per-piece hover
  // tooltip, which never fires on touch.
  const [bonusesOpen, setBonusesOpen] = useState(false);

  // On open-to-change, bring the slotted set into view and flash it so the user
  // can see where the picker landed. Runs only when jumpTarget flips true.
  useEffect(() => {
    if (!jumpTarget) return;
    rowRef.current?.scrollIntoView({ block: 'center', behavior: 'smooth' });
    setJumpFlash(true);
    const t = setTimeout(() => setJumpFlash(false), 1600);
    return () => clearTimeout(t);
  }, [jumpTarget]);

  const attunementEnabled = useUIStore((s) => s.attunementEnabled);
  const globalIOLevel = useUIStore((s) => s.globalIOLevel);
  const globalBoostLevel = useUIStore((s) => s.globalBoostLevel);
  /** What each piece would actually carry — the factory decides, not this row. */
  const pieceOffset = (piece: IOSetPiece, pieceIndex: number) =>
    createIOSetEnhancement(set, piece, pieceIndex, {
      attuned: attunementEnabled,
      level: globalIOLevel,
      boost: globalBoostLevel,
    }).boost ?? 0;
  const isUniqueEnhancementSlotted = useBuildStore((s) => s.isUniqueEnhancementSlotted);
  const isCompareMode = useUIStore((s) => s.enhancementPicker.virtualSlots) !== null;
  const trackedStats = useUIStore((s) => s.trackedStats);

  // Level gating: set is unavailable if IO level is outside its range. ATO / event
  // sets are always attuned (see isInherentlyAttuned), so the IO-level slider never
  // applies to them — never gate or show a "will slot at Lv" hint for those.
  const isLevelGated = !attunementEnabled && !isInherentlyAttuned(set)
    && (set.minLevel > globalIOLevel || set.maxLevel < globalIOLevel);

  // Which bonuses this set grants for the stats the player is tracking, with the
  // piece threshold and value — surfaced inline so the row answers "how much, at
  // how many pieces?" without a hover tooltip (which never fires on touch).
  const trackedBonuses = useMemo(
    () => getSetTrackedBonuses(set, trackedStats),
    [set, trackedStats],
  );
  const hasTrackedMatch = trackedBonuses.length > 0;

  // Sets that are not the catalogue's usual size get a badge; the usual size is
  // left unmarked so the list stays quiet and the odd ones out are the ink.
  const isOffSize = set.pieces.length !== getMostCommonSetSize();

  // Compute proc/unique outlines for all pieces
  const pieceOutlines = useMemo(() =>
    set.pieces.map((piece) =>
      getEnhancementOutline(
        { name: piece.name, proc: piece.proc, unique: piece.unique },
        set.name,
      )
    ),
    [set.pieces, set.name],
  );

  // Check if a piece is in the current drag selection
  const isPieceSelected = (pieceIndex: number) => {
    if (!isDragging || dragStartIndex === null || dragEndIndex === null) return false;
    const min = Math.min(dragStartIndex, dragEndIndex);
    const max = Math.max(dragStartIndex, dragEndIndex);
    return pieceIndex >= min && pieceIndex <= max;
  };

  // Check if a piece is disabled (already slotted in this power, or unique/special already in build)
  // In compare mode, skip build-wide unique check — each configuration is independent
  const isPieceDisabled = (piece: IOSetPiece) => {
    const setId = set.id || set.name;
    // Always prevent duplicate of the same piece in the same power
    if (isPieceInCurrentPower(setId, piece.num)) return 'Already in this power';
    // Across-build uniqueness (not in compare mode). Purple/ATO sets are
    // wholesale unique-per-build; every piece is flagged unique in HC/Rebirth,
    // but the tspy Primalist ATO ships unique=0, so keep the category guard for
    // those two rarities. Event is NOT wholesale unique — its uniqueness lives
    // in the per-piece `unique` flag, so a rule-of-5 event global (Liberty's
    // Belt +Dmg, unique=0) slots in up to 5 powers like LotG, while a genuinely
    // unique event global (Winter's Gift Slow Resistance, unique=1) stays capped.
    if (!isCompareMode) {
      const isSpecialRarity = set.category === 'purple' || set.category === 'ato';
      if ((piece.unique || isSpecialRarity) && isUniqueEnhancementSlotted(setId, piece.num)) {
        return 'Already slotted in build';
      }
    }
    return null;
  };

  return (
    <div
      ref={rowRef}
      className={`rounded-lg p-2 transition-shadow ${
        hasTrackedMatch
          ? 'bg-[var(--color-selected)]/20 border-l-2 border-l-[var(--color-primary)]/70'
          : 'bg-gray-800/40'
      } ${jumpFlash ? 'ring-2 ring-[var(--color-primary)] ring-offset-2 ring-offset-gray-900' : ''}`}
    >
      {/* Set header */}
      <div className="flex items-center gap-1 sm:gap-2 mb-2 flex-wrap">
        {isOffSize && (
          <span
            title={`${set.pieces.length}-piece set — the full set fits in ${set.pieces.length} slots`}
            /* Marks the MINORITY size only, so a short set is found by scanning
               ink instead of reading every label (Homecoming: 58 marked rows,
               169 quiet ones). Inverted neutral rather than a hue: every rarity
               already owns a text color in this header, and luminance contrast
               survives colorblindness and the light-mode ramp flip. */
            className="inline-flex items-center rounded-sm bg-gray-200 px-1 py-px text-[10px] font-bold leading-none tabular-nums text-gray-900"
          >
            {set.pieces.length}pc
          </span>
        )}
        <span className={`text-xs sm:text-sm font-medium ${getRarityColor(set.category)}`}>
          {set.name}
        </span>
        <span className="text-[10px] sm:text-xs text-gray-500">
          Lv {set.minLevel}-{set.maxLevel}
          {/* The badge already states an off-size count; repeating it here would
              put the same number on the row twice, in two weights. */}
          {!isOffSize && ` • ${set.pieces.length}pc`}
        </span>
        {isLevelGated && (
          <span className="text-[10px] text-orange-400">
            (will slot at Lv {globalIOLevel < set.minLevel ? set.minLevel : set.maxLevel})
          </span>
        )}
      </div>

      <TrackedBonusChips bonuses={trackedBonuses} />

      {/* Pieces as icons — hidden on mobile, shown on sm+ */}
      <div className="hidden sm:flex flex-wrap gap-1.5 sm:gap-1 select-none">
        {set.pieces.map((piece, pieceIndex) => {
          const dragSelected = isPieceSelected(pieceIndex);
          const shiftSel = isShiftSelected(set, pieceIndex);
          const disabledReason = isPieceDisabled(piece);
          const isDisabled = !!disabledReason;
          return (
            <Tooltip
              key={pieceIndex}
              content={
                <div>
                  {isDisabled && <div className="text-orange-400 text-xs font-medium mb-1">{disabledReason}</div>}
                  <SetPieceTooltip set={set} piece={piece} />
                </div>
              }
            >
              <button
                data-piece-index={pieceIndex}
                onMouseDown={(e) => !isDisabled && onPieceMouseDown(set, pieceIndex, e)}
                onMouseEnter={() => !isDisabled && onPieceMouseEnter(pieceIndex)}
                onMouseUp={(e) => !isDisabled && onPieceMouseUp(set, pieceIndex, e)}
                onTouchStart={(e) => !isDisabled && onPieceTouchStart(set, pieceIndex, e)}
                onTouchMove={(e) => !isDisabled && onPieceTouchMove(e)}
                onTouchEnd={(e) => !isDisabled && onPieceTouchEnd(set, pieceIndex, e)}
                disabled={isDisabled}
                className={`relative w-10 h-10 sm:w-[30px] sm:h-[30px] rounded border transition-all bg-gray-900/50 ${
                  isDisabled
                    ? 'border-gray-700 opacity-40 cursor-not-allowed'
                    : shiftSel
                      ? 'border-green-400 scale-110 ring-2 ring-green-400/50'
                      : dragSelected
                        ? 'border-[var(--color-selected)] scale-110 ring-2 ring-[var(--color-ring)]/50'
                        : 'border-gray-600 hover:border-[var(--color-selected)] hover:scale-110'
                }`}
                style={{ touchAction: 'pan-y' }}
              >
                <IOSetIcon
                  icon={set.icon || 'Unknown.png'}
                  attuned={attunementEnabled || isInherentlyAttuned(set)}
                  category={set.category}
                  size={30}
                  alt={piece.name}
                  className="pointer-events-none"
                />
                {pieceOutlines[pieceIndex].show && (
                  <div
                    className="absolute -top-1 right-0 w-2 h-2 rounded-full border border-gray-900 pointer-events-none z-10"
                    style={{
                      background: pieceOutlines[pieceIndex].secondaryColor
                        ? `linear-gradient(135deg, ${pieceOutlines[pieceIndex].color} 50%, ${pieceOutlines[pieceIndex].secondaryColor} 50%)`
                        : pieceOutlines[pieceIndex].color,
                    }}
                  />
                )}
                <PendingOffsetBadge offset={pieceOffset(piece, pieceIndex)} />
              </button>
            </Tooltip>
          );
        })}
      </div>

      {/* Pieces as list — shown on mobile, hidden on sm+ */}
      <div className="sm:hidden space-y-1 select-none mt-1 mr-2">
        {set.pieces.map((piece, pieceIndex) => {
          const selected = isPieceSelected(pieceIndex);
          const shiftSel = isShiftSelected(set, pieceIndex);
          const disabledReason = isPieceDisabled(piece);
          const isDisabled = !!disabledReason;
          return (
            <button
              key={pieceIndex}
              data-piece-index={pieceIndex}
              onMouseDown={(e) => !isDisabled && onPieceMouseDown(set, pieceIndex, e)}
              onMouseEnter={() => !isDisabled && onPieceMouseEnter(pieceIndex)}
              onMouseUp={(e) => !isDisabled && onPieceMouseUp(set, pieceIndex, e)}
              onTouchStart={(e) => !isDisabled && onPieceTouchStart(set, pieceIndex, e)}
              onTouchMove={(e) => !isDisabled && onPieceTouchMove(e)}
              onTouchEnd={(e) => !isDisabled && onPieceTouchEnd(set, pieceIndex, e)}
              disabled={isDisabled}
              className={`w-full flex items-center gap-2 p-2 rounded border transition-all ${
                isDisabled
                  ? 'border-gray-700 opacity-40 cursor-not-allowed bg-gray-900/30'
                  : shiftSel
                    ? 'border-green-400 bg-green-900/20 ring-1 ring-green-400/50'
                    : selected
                      ? 'border-[var(--color-selected)] bg-[var(--color-selected)]/20'
                      : 'border-gray-600 bg-gray-900/50 active:bg-[var(--color-selected)]/10'
              }`}
              style={{
                WebkitUserSelect: 'none',
                userSelect: 'none',
                WebkitTouchCallout: 'none',
                touchAction: 'pan-y',
              }}
            >
              {/* Icon on left */}
              <div className="relative flex-shrink-0">
                <IOSetIcon
                  icon={set.icon || 'Unknown.png'}
                  attuned={attunementEnabled || isInherentlyAttuned(set)}
                  category={set.category}
                  size={30}
                  alt={piece.name}
                  className="pointer-events-none"
                />
                {pieceOutlines[pieceIndex].show && (
                  <div
                    className="absolute -top-0.5 right-0.5 w-2 h-2 rounded-full border border-gray-900 pointer-events-none"
                    style={{
                      background: pieceOutlines[pieceIndex].secondaryColor
                        ? `linear-gradient(135deg, ${pieceOutlines[pieceIndex].color} 50%, ${pieceOutlines[pieceIndex].secondaryColor} 50%)`
                        : pieceOutlines[pieceIndex].color,
                    }}
                  />
                )}
                <PendingOffsetBadge offset={pieceOffset(piece, pieceIndex)} />
              </div>

              {/* Info on right */}
              <div className="flex-1 text-left min-w-0">
                <div className="text-sm font-medium text-gray-200 truncate">
                  {resolveProcPieceName(piece.name, set.name, piece.proc)}
                </div>
                <div className="text-xs text-gray-400">
                  {piece.aspects.join(', ')}
                </div>
                {piece.proc && (
                  <div className="text-xs" style={{ color: pieceOutlines[pieceIndex].color }}>Proc Effect</div>
                )}
                {(piece.unique || disabledReason) && (
                  <div className="text-xs text-orange-400">
                    {disabledReason || 'Unique'}
                  </div>
                )}
              </div>
            </button>
          );
        })}
      </div>

      {/* Mobile-only: collapsible set bonuses. Desktop shows these in the
          per-piece hover tooltip (SetPieceTooltip), which never fires on touch —
          this is the touch path to the same data. */}
      {set.bonuses.length > 0 && (
        <div className="sm:hidden mt-1.5">
          <button
            type="button"
            onClick={() => setBonusesOpen((o) => !o)}
            aria-expanded={bonusesOpen}
            className="w-full flex items-center gap-1.5 px-2 py-1.5 rounded border border-gray-700 bg-gray-900/40 text-xs text-gray-300 active:bg-gray-800/60"
          >
            <span className={`inline-block transition-transform text-gray-500 ${bonusesOpen ? 'rotate-90' : ''}`}>▸</span>
            <span className="font-medium">Set Bonuses</span>
            <span className="text-gray-500">({set.bonuses.length})</span>
          </button>
          {bonusesOpen && (
            <div className="mt-1 px-2 pb-1">
              <SetBonusList set={set} />
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// ============================================
// SHARED: STACKED-SELECTION COUNT BADGE
// ============================================

/**
 * Badge that overlays a tile to show how many copies of that enhancement
 * are queued for multi-slot. Clicking the badge decrements the count
 * (the only decrement affordance — works on both desktop and mobile and
 * keeps the tile itself a pure +1 target).
 */
function StackedCountBadge({ count, onDecrement }: { count: number; onDecrement: () => void }) {
  return (
    <button
      type="button"
      onClick={(e) => {
        e.stopPropagation();
        onDecrement();
      }}
      title="Click to remove one"
      className="absolute -top-1.5 -right-1.5 min-w-[18px] h-[18px] px-1 rounded-full bg-green-600 hover:bg-red-600 text-on-success text-[10px] font-bold leading-none flex items-center justify-center shadow ring-1 ring-gray-900 transition-colors"
    >
      ×{count}
    </button>
  );
}

/**
 * The level offset this tile would be slotted at, drawn ON the tile.
 *
 * The header spinner was the only place this number appeared, at the top of a
 * grid you scroll away from. Reported 2026-09-17: work on the special tab,
 * whose axis stops at +3, then slot IOs — they went in at +3 rather than the +5
 * the user had chosen, and the first sign of it was an already-slotted piece.
 * (The clamp that leaked between the two axes is fixed in uiStore; this is the
 * other half — a number you can see without re-reading the header.)
 *
 * `offset` is asked of the same factory the click path calls, never recomputed:
 * eligibility is the factory's rule — an attuned piece and a pure proc both drop
 * the boost — and a badge restating it would be a second copy free to drift from
 * what actually gets slotted. Which is the exact failure this badge exists for.
 *
 * A span, not a button: the set-piece tile IS a button, and a nested one makes
 * the parser close the outer element.
 *
 * Top-LEFT is the only free corner. Top-right carries the piece-outline dot on a
 * set tile and `StackedCountBadge` on a stackable one, and bottom sits on the stat
 * label that the generic/origin tiles print under their icon — which bottom-left
 * covered when this was first written.
 */
function PendingOffsetBadge({ offset, className = '-top-1.5 -left-1.5' }: { offset: number; className?: string }) {
  if (!offset) return null;
  return (
    <span
      aria-hidden
      className={`absolute ${className} min-w-[16px] h-[14px] px-0.5 rounded-sm text-[9px] font-bold leading-[14px] text-center pointer-events-none shadow ring-1 ring-gray-900 ${
        offset > 0 ? 'bg-green-600 text-on-success' : 'bg-red-700 text-white'
      }`}
    >
      {formatSignedOffset(offset)}
    </span>
  );
}

// ============================================
// GENERIC IO CONTENT
// ============================================

interface GenericIOContentProps {
  availableIOs: EnhancementStatType[];
  /** The roster level these chips mint at — not the raw picker level, which the
   *  set tab may have left between the roster's steps. */
  craftLevel: number;
  onSelect: (stat: EnhancementStatType, e?: React.MouseEvent) => void;
  stackedCountFor: (stat: EnhancementStatType) => number;
  onDecrement: (stat: EnhancementStatType) => void;
}

function GenericIOContent({ availableIOs, craftLevel, onSelect, stackedCountFor, onDecrement }: GenericIOContentProps) {
  const globalBoostLevel = useUIStore((s) => s.globalBoostLevel);
  if (availableIOs.length === 0) {
    return <div className="text-center text-gray-500 py-8">No generic IOs available for this power</div>;
  }

  return (
    <div className="bg-gray-800/40 rounded-lg p-2">
      <div className="flex items-center gap-2 mb-2">
        <span className="text-sm font-medium text-gray-300">Generic IOs</span>
        {/* The value is per-stat, not per-level: these stats span all four ED
            schedules, so one number here would be right for the Schedule A ones
            and wrong for ToHit / Defense / Resistance / Range / Interrupt /
            Knockback. It lives on each chip's tooltip instead. */}
        <span className="text-xs text-gray-500">Lv {craftLevel}</span>
      </div>
      <div className="flex flex-wrap gap-1">
        {availableIOs.map((stat) => {
          const count = stackedCountFor(stat);
          const value = genericIOValueAtLevel(stat, craftLevel);
          return (
            <Tooltip key={stat} content={value === null ? `${stat} IO` : `${stat} IO (+${value.toFixed(1)}%)`}>
              <div className="relative">
                <button
                  onClick={(e) => onSelect(stat, e)}
                  className="rounded border border-gray-600 hover:border-[var(--color-selected)] hover:scale-110 transition-all bg-gray-900/50 flex flex-col items-center w-[46px] py-0.5"
                >
                  <GenericIOIcon stat={stat} size={30} alt={stat} />
                  <span className="text-[8px] text-gray-400 leading-tight truncate w-full text-center">{stat}</span>
                </button>
                {count > 0 && <StackedCountBadge count={count} onDecrement={() => onDecrement(stat)} />}
                <PendingOffsetBadge
                  offset={createGenericIOEnhancement(stat, craftLevel, globalBoostLevel).boost ?? 0}
                />
              </div>
            </Tooltip>
          );
        })}
      </div>
    </div>
  );
}

// ============================================
// SPECIAL CONTENT
// ============================================

interface SpecialContentProps {
  availableHamidons: [string, SpecialEnhancementDef][];
  availableTitans: [string, SpecialEnhancementDef][];
  availableHydras: [string, SpecialEnhancementDef][];
  availableDSyncs: [string, SpecialEnhancementDef][];
  availablePrestige: [string, SpecialEnhancementDef][];
  onSelect: (id: string, def: SpecialEnhancementDef, category: SpecialEnhancement['category'], e?: React.MouseEvent) => void;
  stackedCountFor: (category: SpecialEnhancement['category'], id: string) => number;
  onDecrement: (category: SpecialEnhancement['category'], id: string) => void;
}

/** Overrides for compound-word IDs whose simple capitalize doesn't match the icon filename */
const SPECIAL_ICON_OVERRIDES: Record<string, string> = {
  antiproton: 'AntiProton',
  clockwork_efficiency: 'ClockworkEfficiency',
  might_of_the_empire: 'MarkoftheEmpire',
  resistance_tactics: 'ResistanceTactics',
  syndicate_techniques: 'SyndicateTechniques',
  will_of_the_seers: 'WilloftheSeers',
};

const SPECIAL_SECTIONS: Array<{
  category: SpecialEnhancement['category'];
  label: string;
  color: string;
  borderColor: string;
  iconPrefix: string;
  key: 'availableHamidons' | 'availableTitans' | 'availableHydras' | 'availableDSyncs' | 'availablePrestige';
}> = [
  { category: 'hamidon', label: 'Hamidon Origin', color: 'text-purple-400', borderColor: 'border-purple-700 hover:border-purple-400', iconPrefix: 'HO', key: 'availableHamidons' },
  { category: 'titan', label: 'Titan Origin', color: 'text-amber-400', borderColor: 'border-amber-700 hover:border-amber-400', iconPrefix: 'TN', key: 'availableTitans' },
  { category: 'hydra', label: 'Hydra Origin', color: 'text-cyan-400', borderColor: 'border-cyan-700 hover:border-cyan-400', iconPrefix: 'HY', key: 'availableHydras' },
  { category: 'd-sync', label: 'D-Sync Origin', color: 'text-green-400', borderColor: 'border-green-700 hover:border-green-400', iconPrefix: 'DS', key: 'availableDSyncs' },
  { category: 'prestige', label: 'Prestige', color: 'text-rose-400', borderColor: 'border-rose-700 hover:border-rose-400', iconPrefix: 'Prestige_', key: 'availablePrestige' },
];

function SpecialContent(props: SpecialContentProps) {
  const { onSelect, stackedCountFor, onDecrement } = props;
  const globalRelativeLevel = useUIStore((s) => s.globalRelativeLevel);
  const totalAvailable = props.availableHamidons.length + props.availableTitans.length + props.availableHydras.length + props.availableDSyncs.length + props.availablePrestige.length;

  if (totalAvailable === 0) {
    return <div className="text-center text-gray-500 py-8">No special enhancements available for this power</div>;
  }

  return (
    <div className="space-y-3">
      {SPECIAL_SECTIONS.map(section => {
        const entries = props[section.key];
        if (entries.length === 0) return null;
        return (
          <div key={section.category} className="bg-gray-800/40 rounded-lg p-2">
            <div className="flex items-center gap-2 mb-2">
              <span className={`text-sm font-medium ${section.color}`}>{section.label}</span>
            </div>
            <div className="flex flex-wrap gap-1">
              {entries.map(([id, def]) => {
                // D-Sync enhancements all share a single icon; others use prefix + capitalized ID
                const iconName = section.category === 'd-sync'
                  ? 'DSO_all.png'
                  : `${section.iconPrefix}${SPECIAL_ICON_OVERRIDES[id] ?? (id.charAt(0).toUpperCase() + id.slice(1))}.png`;
                const count = stackedCountFor(section.category, id);
                return (
                  <Tooltip key={id} content={`${def.name}: ${def.aspects.map(a => `${a.stat} +${a.value}%`).join(', ')}`}>
                    <div className="relative">
                      <button
                        onClick={(e) => onSelect(id, def, section.category, e)}
                        className={`rounded border ${section.borderColor} hover:scale-110 transition-all bg-gray-900/50`}
                      >
                        <SpecialEnhancementIcon icon={iconName} size={30} alt={def.name} />
                      </button>
                      {count > 0 && <StackedCountBadge count={count} onDecrement={() => onDecrement(section.category, id)} />}
                      <PendingOffsetBadge
                        offset={createSpecialEnhancement(id, def, section.category, globalRelativeLevel).boost ?? 0}
                      />
                    </div>
                  </Tooltip>
                );
              })}
            </div>
          </div>
        );
      })}
    </div>
  );
}

// ============================================
// ORIGIN CONTENT
// ============================================

interface OriginContentProps {
  availableTypes: EnhancementStatType[];
  onSelect: (stat: EnhancementStatType, tier: 'TO' | 'DO' | 'SO', e?: React.MouseEvent) => void;
  stackedCountFor: (stat: EnhancementStatType, tier: 'TO' | 'DO' | 'SO') => number;
  onDecrement: (stat: EnhancementStatType, tier: 'TO' | 'DO' | 'SO') => void;
}

function OriginContent({ availableTypes, onSelect, stackedCountFor, onDecrement }: OriginContentProps) {
  const buildOrigin = useBuildStore((s) => s.build.settings.origin);
  const globalRelativeLevel = useUIStore((s) => s.globalRelativeLevel);

  if (availableTypes.length === 0) {
    return <div className="text-center text-gray-500 py-8">No origin enhancements available for this power</div>;
  }

  return (
    <div className="space-y-3">
      {ORIGIN_TIER_INFO.map((tier) => (
        <div key={tier.short} className="bg-gray-800/40 rounded-lg p-2">
          <div className="flex items-center gap-2 mb-2">
            <span className={`text-sm font-medium ${getTierTextColor(tier.short)}`}>
              {tier.name} ({tier.short})
            </span>
          </div>
          <div className="flex flex-wrap gap-1">
            {availableTypes.map((stat) => {
              const tierShort = tier.short as 'TO' | 'DO' | 'SO';
              const count = stackedCountFor(stat, tierShort);
              const tierValue = getOriginTierValue(tierShort, normalizeAspectName(stat) ?? stat);
              return (
                <Tooltip key={stat} content={`${stat} ${tier.short} (+${tierValue.toFixed(1)}%)`}>
                  <div className="relative">
                    <button
                      onClick={(e) => onSelect(stat, tierShort, e)}
                      className={`rounded border hover:scale-110 transition-all bg-gray-900/50 flex flex-col items-center w-[46px] py-0.5 ${getTierBorderColor(tier.short)}`}
                    >
                      <OriginEnhancementIcon
                        stat={stat}
                        tier={tierShort}
                        origin={buildOrigin}
                        size={30}
                        alt={`${stat} ${tier.short}`}
                      />
                      <span className="text-[8px] text-gray-400 leading-tight truncate w-full text-center">{stat}</span>
                    </button>
                    {count > 0 && <StackedCountBadge count={count} onDecrement={() => onDecrement(stat, tierShort)} />}
                    <PendingOffsetBadge
                      offset={createOriginEnhancement(stat, tierShort, buildOrigin, globalRelativeLevel).boost ?? 0}
                    />
                  </div>
                </Tooltip>
              );
            })}
          </div>
        </div>
      ))}
    </div>
  );
}

// ============================================
// SHARED COMPONENTS
// ============================================

interface SetPieceTooltipProps {
  set: IOSet;
  piece: IOSetPiece;
}

function SetPieceTooltip({ set, piece }: SetPieceTooltipProps) {
  const globalIOLevel = useUIStore((s) => s.globalIOLevel);
  const attunementEnabled = useUIStore((s) => s.attunementEnabled);
  const globalBoostLevel = useUIStore((s) => s.globalBoostLevel);
  const exemplarMode = useUIStore((s) => s.exemplarMode);
  const exemplarLevelSetting = useUIStore((s) => s.exemplarLevel);
  const build = useBuildStore((s) => s.build);
  const exemplarLevel = exemplarMode ? exemplarLevelSetting : undefined;

  // Preview the piece exactly as slotting it would land: build the enhancement the picker
  // would store, then read what the dashboard would credit for it (PROD6E-2).
  const effectiveAttuned = attunementEnabled || isInherentlyAttuned(set);
  const effectiveLevel = Math.max(set.minLevel, Math.min(globalIOLevel, set.maxLevel));
  const previewValues = useMemo(() => {
    const slot = createIOSetEnhancement(set, piece, piece.num, {
      attuned: attunementEnabled,
      level: effectiveLevel,
      boost: globalBoostLevel,
    });
    return calculateSingleEnhancementValues(slot, build.level, getIOSet, exemplarLevel);
  }, [set, piece, attunementEnabled, effectiveLevel, globalBoostLevel, build.level, exemplarLevel]);

  // The multi-aspect penalty note under the list, through the same classifier the
  // calculation applies. Picks up the hidden global/proc segments that dilute a
  // named aspect without appearing in the list, which reach the piece as the
  // scale-derived `totalAspects`.
  const rawAspectCount = piece.aspects.filter((a) => normalizeAspectName(a) !== null).length || piece.aspects.length;
  const aspectCount = getEffectiveAspectCount(
    piece.aspects.slice(0, rawAspectCount),
    !!piece.proc,
    piece.totalAspects,
  );

  const calculateAspectValue = (aspect: string): number | null =>
    readAspectDisplayValue(aspect, previewValues);

  return (
    <div className="space-y-2 max-w-[320px]">
      {/* Enhancement header with set name */}
      <div className="flex items-center gap-2">
        <IOSetIcon
          icon={set.icon || 'Unknown.png'}
          attuned={effectiveAttuned}
          category={set.category}
          size={28}
          alt={piece.name}
          className="flex-shrink-0"
        />
        <div className="min-w-0">
          <h3 className="text-xs font-semibold text-yellow-400 leading-tight">{set.name}</h3>
          <span className="text-[10px] text-link">{resolveProcPieceName(piece.name, set.name, piece.proc)}</span>
        </div>
      </div>

      {/* Proc Effect section */}
      {piece.proc && (
        <div className="bg-amber-900/30 border border-amber-700/50 rounded p-1.5">
          <div className="text-[9px] text-amber-400 uppercase mb-1 font-semibold">Proc Effect</div>
          {(() => {
            const procData = findProcData(piece.name, set.name);
            if (procData) {
              const effect = procEffectSummary(procData);
              const effectColorClass = getProcEffectColor(effect.category);
              const categoryLabel = getProcEffectLabel(effect.category);
              const isAlwaysOn = isProcAlwaysOn(procData);

              const badgeColors: Record<string, string> = {
                'Damage': 'bg-red-900/50 text-red-300',
                'Endurance': 'bg-blue-900/50 text-blue-300',
                'Heal': 'bg-emerald-900/50 text-emerald-300',
                'Absorb': 'bg-cyan-900/50 text-cyan-300',
                'Resistance': 'bg-orange-900/50 text-orange-300',
                'Defense': 'bg-purple-900/50 text-purple-300',
                'ToHit': 'bg-yellow-900/50 text-yellow-300',
                'Regeneration': 'bg-green-900/50 text-green-300',
                'Recovery': 'bg-blue-900/50 text-blue-300',
                'Recharge': 'bg-amber-900/50 text-amber-300',
                'RunSpeed': 'bg-teal-900/50 text-teal-300',
                'MaxHP': 'bg-pink-900/50 text-pink-300',
                'KnockbackProtection': 'bg-slate-700 text-slate-300',
                'Stealth': 'bg-gray-700 text-gray-300',
                'Control': 'bg-indigo-900/50 text-indigo-300',
                'Debuff': 'bg-rose-900/50 text-rose-300',
                'Special': 'bg-slate-700 text-slate-300',
              };

              return (
                <div className="space-y-1">
                  <div className="flex items-center gap-2 flex-wrap">
                    <span className={`text-[10px] font-medium ${effectColorClass}`}>
                      {procData.ioName}
                    </span>
                    <span className={`text-[8px] px-1 py-0.5 rounded ${badgeColors[effect.category] || 'bg-slate-700 text-slate-300'}`}>
                      {categoryLabel}
                    </span>
                    {isAlwaysOn && (
                      <span className="text-[8px] px-1 py-0.5 rounded bg-green-900/50 text-green-300">
                        Always On
                      </span>
                    )}
                  </div>
                  <div className="text-[9px] text-slate-300 bg-slate-800/50 rounded px-1.5 py-1">
                    {procData.mechanics}
                  </div>
                  <div className="flex flex-wrap gap-x-3 gap-y-0.5 text-[9px]">
                    {procData.ppm !== null && (
                      <div>
                        <span className="text-slate-500">PPM:</span>
                        <span className="text-amber-300 ml-1 font-medium">{procData.ppm}</span>
                      </div>
                    )}
                    <div>
                      <span className="text-slate-500">Type:</span>
                      <span className={`ml-1 ${
                        procData.type === 'Proc120s' ? 'text-purple-400' :
                        procData.type === 'Global' ? 'text-green-400' :
                        'text-amber-300'
                      }`}>
                        {procData.type === 'Proc120s' ? '100% (120s)' : procData.type}
                      </span>
                    </div>
                    {effect.value !== undefined && effect.category === 'Damage' && effect.valueMax && (
                      <div>
                        <span className="text-slate-500">Dmg:</span>
                        <span className="text-red-400 ml-1">
                          {/* Proc damage uses CHARACTER level, not crafted/global-IO level (@Redlynne, 2026-06-12) */}
                          {interpolateProcDamage(effect.value, effect.valueMax, procData.levelRange, build.level || 50)} {effect.effectType}
                        </span>
                      </div>
                    )}
                    {effect.value !== undefined && !(effect.category === 'Damage' && effect.valueMax !== undefined) && (
                      <div>
                        <span className="text-slate-500">Value:</span>
                        <span className={`${effectColorClass} ml-1`}>
                          {effect.category === 'KnockbackProtection' ? `Mag ${effect.value}` :
                           effect.category === 'Stealth' ? `${effect.value} ft` :
                           `${effect.value}%`}
                          {effect.effectType ? ` ${effect.effectType}` : ''}
                        </span>
                      </div>
                    )}
                    {effect.secondaryCategory && effect.secondaryValue !== undefined && (
                      <div>
                        <span className="text-slate-500">+{getProcEffectLabel(effect.secondaryCategory)}:</span>
                        <span className={`${getProcEffectColor(effect.secondaryCategory)} ml-1`}>
                          {effect.secondaryValue}%
                          {effect.secondaryEffectType ? ` ${effect.secondaryEffectType}` : ''}
                        </span>
                      </div>
                    )}
                  </div>
                </div>
              );
            }
            return <div className="text-[10px] text-amber-200">{resolveProcPieceName(piece.name, set.name, piece.proc)}</div>;
          })()}
        </div>
      )}

      {/* Enhances section — also shown for hybrid global/proc IOs (LotG
          Defense/+Recharge, Steadfast Resistance/+Def) that both proc and
          enhance a stat. Pure procs have an empty `aspects` array. */}
      {piece.aspects.length > 0 && (
        <div className="bg-slate-800/50 rounded p-1.5">
          <div className="text-[9px] text-slate-500 uppercase mb-1">Enhances:</div>
          {piece.aspects.map((aspect, i) => {
            const value = calculateAspectValue(aspect);
            return (
              <div key={i} className="flex justify-between items-baseline text-[10px]">
                <span className="text-slate-300">{aspect}</span>
                {value !== null && (
                  <span className="text-green-400 font-mono">
                    +{(value * 100).toFixed(2)}%
                  </span>
                )}
              </div>
            );
          })}
          {aspectCount > 1 && (
            <div className="text-[8px] text-slate-500 mt-1 italic">
              {aspectCount === 2 ? '62.5%' : aspectCount === 3 ? '50%' : '43.75%'} per aspect ({aspectCount} aspects)
            </div>
          )}
        </div>
      )}

      {/* Level and flags */}
      <div className="text-[10px] flex gap-3">
        <span className="text-slate-400">
          {effectiveAttuned ? (
            <span className="text-purple-400">Attuned</span>
          ) : (
            <>Level: <span className="text-slate-200">{effectiveLevel}</span>
              {effectiveLevel !== globalIOLevel && (
                <span className="text-orange-400 ml-1">({effectiveLevel < globalIOLevel ? 'max' : 'min'})</span>
              )}
            </>
          )}
        </span>
        <span className="text-slate-400">Range: {set.minLevel}-{set.maxLevel}</span>
        {!effectiveAttuned && !(piece.proc && piece.aspects.length === 0) && globalBoostLevel > 0 && <span className="text-green-400">+{globalBoostLevel} Boosted</span>}
        {piece.unique && <span className="text-red-400">Unique</span>}
      </div>

      {/* Set Bonuses */}
      <SetBonusList set={set} />
    </div>
  );
}


// ============================================
// HELPER FUNCTIONS
// ============================================



