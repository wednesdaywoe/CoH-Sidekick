/**
 * Popmenu (.mnu) export for CoH test server
 *
 * Generates a .mnu file that can be placed in the game's
 * data/texts/English/Menus/ folder and used with /popmenu <name>
 * to grant all build enhancements on the test server.
 */

import type { Build, Enhancement } from '@/types';
import type { IOSetEnhancement, GenericIOEnhancement, SpecialEnhancement } from '@/types/enhancement';
import { getIOSet, isInherentlyAttuned } from '@/data';
import { getCommonIOBoostUid, getIOSetBoostUid, getSpecialBoostUid } from '@/data/boost-index';

// ============================================
// WARNINGS
// ============================================

/**
 * One enhancement the popmenu could not grant, and why.
 *
 * Reported rather than dropped. A `boost` command naming a record the game does
 * not have is answered with a console line the user never sees, so an
 * unnameable enhancement is indistinguishable in game from one this file never
 * wrote — which is exactly how `Crafted_Flight` survived eleven months of use
 * (POPMENU-1). The exporter now says so here instead.
 */
export interface PopmenuWarning {
  kind: 'unnameable' | 'not-grantable';
  /** The power the slot belongs to, as the user sees it named. */
  power: string;
  /** 1-based slot within that power. */
  slot: number;
  detail: string;
}

// Max boost commands per Option line (game has a command length limit)
const MAX_BOOSTS_PER_OPTION = 70;

// ============================================
// ENHANCEMENT → BOOST COMMAND
// ============================================

/**
 * The record name the game knows this enhancement by, or why it has none.
 *
 * Every arm is a lookup into the dataset's boost index — the export's own
 * roster, keyed by the spelling the game client prints. Nothing here assembles
 * a name out of our ids: the two tables that used to (a planner-stat map and a
 * prefix/PascalCase composition for set pieces) are what POPMENU-1 was.
 */
function boostRecordFor(enh: Enhancement): { uid: string; level: number } | { reason: string } {
  switch (enh.type) {
    case 'io-set': {
      const ioSet = enh as IOSetEnhancement;
      const setDef = getIOSet(ioSet.setId);
      // The shared attunement source of truth, so this cannot drift from the
      // picker or the calc. Covers ATO + Winter/Summer/Anniversary event sets,
      // which ship attuned and have no crafted record at all.
      const attuned = ioSet.attuned === true || (setDef ? isInherentlyAttuned(setDef) : false);
      const uid = getIOSetBoostUid(ioSet.setId, ioSet.pieceNum, attuned);
      if (!uid) {
        const name = setDef?.name ?? ioSet.setId;
        return {
          reason: attuned
            ? `this server's export names no attuned ${name} piece ${ioSet.pieceNum}`
            : `this server's export names no ${name} piece ${ioSet.pieceNum}`,
        };
      }
      // An attuned piece states no craft level; the game holds it at the
      // character's, and 50 is what the command has to say to mean that.
      return { uid, level: attuned ? 50 : ioSet.level || 50 };
    }

    case 'io-generic': {
      const generic = enh as GenericIOEnhancement;
      const uid = getCommonIOBoostUid(generic.stat);
      if (!uid) return { reason: `this server's export names no ${generic.stat} generic IO` };
      return { uid, level: generic.level || 50 };
    }

    case 'special': {
      const special = enh as SpecialEnhancement;
      // A special's id is `${category}-${id}` (see createSpecialEnhancement) and
      // the index is keyed on the same two parts — but the seam is not the first
      // dash. `d-sync-optimization` has three, and splitting at the first one
      // asks the index for family `d`. The category is carried on the
      // enhancement, so take the boundary from it rather than from the string.
      const prefix = `${special.category}-`;
      const uid = special.id.startsWith(prefix)
        ? getSpecialBoostUid(special.category, special.id.slice(prefix.length))
        : null;
      if (!uid) return { reason: `this server's export names no ${special.name} record` };
      // Specials pin no craft level; the game holds them at 50.
      return { uid, level: 50 };
    }

    case 'origin':
      // TO/DO/SO records exist and `boost` would grant them, but WHICH one is a
      // question the slot does not answer: the game names a DO for each PAIR of
      // origins, so a Magic character has two spellings and the planner stores
      // neither. Left out deliberately rather than guessed at — they cost a few
      // thousand influence in game, which is the cheapest thing in the build.
      return { reason: 'origin enhancements are bought in game, not granted by popmenu' };

    default:
      return { reason: `no boost record for a ${(enh as Enhancement).type} enhancement` };
  }
}

// ============================================
// FULL POPMENU GENERATION
// ============================================

/** One slotted enhancement, with the power and slot a warning has to name. */
interface SlottedEnhancement {
  power: string;
  slot: number;
  enh: Enhancement;
}

/** Every non-empty slot in the build, in the order the popmenu grants them. */
function collectEnhancements(build: Build): SlottedEnhancement[] {
  const slotted: SlottedEnhancement[] = [];

  const processPower = (power: { name: string; slots: (Enhancement | null)[] }) => {
    power.slots.forEach((slot, i) => {
      if (slot) slotted.push({ power: power.name, slot: i + 1, enh: slot });
    });
  };

  for (const power of build.primary.powers) processPower(power);
  for (const power of build.secondary.powers) processPower(power);
  for (const pool of build.pools) {
    for (const power of pool.powers) processPower(power);
  }
  if (build.epicPool) {
    for (const power of build.epicPool.powers) processPower(power);
  }
  for (const power of build.inherents) processPower(power);

  return slotted;
}

/**
 * Generate a .mnu popmenu file from a build, with what it could not grant.
 *
 * @param build - The build to export
 * @param menuName - Name for the popmenu (used with /popmenu <name> in-game)
 */
export function generatePopmenuWithReport(
  build: Build,
  menuName: string,
): { content: string; warnings: PopmenuWarning[] } {
  const warnings: PopmenuWarning[] = [];
  const boostCmds: string[] = [];

  for (const { power, slot, enh } of collectEnhancements(build)) {
    const record = boostRecordFor(enh);
    if ('reason' in record) {
      warnings.push({
        kind: enh.type === 'origin' ? 'not-grantable' : 'unnameable',
        power,
        slot,
        detail: record.reason,
      });
      continue;
    }
    boostCmds.push(`boost ${record.uid} ${record.uid} ${record.level}`);
  }

  if (boostCmds.length === 0) {
    return { content: `// No exportable enhancements found in build\n`, warnings };
  }

  // Split into chunks for multiple Option lines
  const chunks: string[][] = [];
  for (let i = 0; i < boostCmds.length; i += MAX_BOOSTS_PER_OPTION) {
    chunks.push(boostCmds.slice(i, i + MAX_BOOSTS_PER_OPTION));
  }

  const now = new Date();
  const dateStr = now.toLocaleDateString('en-US', {
    month: '2-digit',
    day: '2-digit',
    year: 'numeric',
  }) + ' ' + now.toLocaleTimeString('en-US', {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
  });

  const lines: string[] = [];
  lines.push(`// Generated by Sidekick - ${dateStr}`);
  lines.push(`// Open the menu in game: /popmenu ${menuName}`);
  lines.push('');
  lines.push(`Menu "${menuName}"`);
  lines.push('{');
  lines.push('\tTitle "Test build"');
  lines.push('\tDIVIDER');

  for (let i = 0; i < chunks.length; i++) {
    const label = chunks.length === 1
      ? 'Give enhancements'
      : `Give enhancements (part ${i + 1})`;
    const cmdString = chunks[i].join('$$');
    lines.push(`\tOption "${label}" "${cmdString}"`);
  }

  lines.push('\tDIVIDER');
  lines.push('\tLockedOption');
  lines.push('\t{');
  lines.push('\t\tDisplayName "Sidekick"');
  lines.push('\t\tBadge "X"');
  lines.push('\t}');
  lines.push('\tLockedOption');
  lines.push('\t{');
  lines.push(`\t\tDisplayName "Generated: ${dateStr}"`);
  lines.push('\t\tBadge "X"');
  lines.push('\t}');
  lines.push('}');
  lines.push('');

  return { content: lines.join('\n'), warnings };
}
