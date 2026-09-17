/**
 * Diagnostic snapshot for bug reports.
 *
 * Captures UI/runtime state that affects what the user sees but is NOT
 * part of the build export — settings like Level Up Mode, Combat Mode,
 * AT mechanic toggles, mechanic adjusters, etc. Without these, many bug
 * reports can't be reproduced because the missing flag is the cause.
 *
 * This is intentionally separate from the build serialization layer so
 * the build JSON stays slim and shareable; diagnostics are only attached
 * when a user submits a bug report.
 */

import { useUIStore } from '@/stores/uiStore';
import { useBuildStore } from '@/stores/buildStore';
import { APP_VERSION, BUILD_TIME } from '@/buildTime';

/**
 * AT-specific mechanic toggles. Each archetype only sees its own flags
 * in the snapshot — keeps the blob readable.
 */
const AT_MECHANIC_KEYS: Record<string, readonly string[]> = {
  defender: ['vigilanceTeamSize'],
  brute: ['furyLevel'],
  stalker: ['stalkerHidden', 'stalkerTeamSize', 'stalkerCritActive'],
  scrapper: ['criticalHitsActive'],
  sentinel: ['sentinelCritActive'],
  controller: ['containmentActive'],
  dominator: ['dominationActive'],
  corruptor: ['scourgeActive'],
  mastermind: ['supremacyActive'],
};

export interface DiagnosticsSnapshot {
  app: { version: string; buildTime: string };
  env: {
    userAgent: string;
    viewport: { width: number; height: number };
    datasetId: string;
    url: string;
  };
  ui: Record<string, unknown>;
}

function pickTrue(record: Record<string, boolean>): Record<string, boolean> {
  return Object.fromEntries(Object.entries(record).filter(([, v]) => v === true));
}

function pickNonZero(record: Record<string, number>): Record<string, number> {
  return Object.fromEntries(Object.entries(record).filter(([, v]) => v !== 0));
}

export function getDiagnosticsSnapshot(): DiagnosticsSnapshot {
  const ui = useUIStore.getState();
  const build = useBuildStore.getState().build;

  const uiSnapshot: Record<string, unknown> = {
    levelUpMode: ui.levelUpMode,
    combatMode: ui.combatMode,
    exemplarMode: ui.exemplarMode,
    exemplarLevel: ui.exemplarLevel,
    attunementEnabled: ui.attunementEnabled,
    globalIOLevel: ui.globalIOLevel,
    globalBoostLevel: ui.globalBoostLevel,
    globalRelativeLevel: ui.globalRelativeLevel,
    targetLevelOffset: ui.targetLevelOffset,
    procSettings: ui.procSettings,
    includeProcDamageInDPS: ui.includeProcDamageInDPS,
    useArcanaTime: ui.useArcanaTime,
    damageDisplayMode: ui.damageDisplayMode,
    incarnateActive: ui.incarnateActive,
    incarnateLevelShift: ui.incarnateLevelShift,
    selectedBranch: ui.selectedBranch,
    powerViewMode: ui.powerViewMode,
    targetsHitValues: pickNonZero(ui.targetsHitValues),
    mechanicAdjusters: pickTrue(ui.mechanicAdjusters),
    globalAdjusters: pickTrue(ui.globalAdjusters),
    trackedStats: ui.trackedStats,
    permaTrackedPowers: ui.permaTrackedPowers,
  };

  const atKeys = AT_MECHANIC_KEYS[build.archetype.id ?? ''] ?? [];
  const uiAny = ui as unknown as Record<string, unknown>;
  for (const key of atKeys) {
    uiSnapshot[key] = uiAny[key];
  }

  return {
    app: {
      version: APP_VERSION,
      buildTime: new Date(BUILD_TIME).toISOString(),
    },
    env: {
      userAgent: navigator.userAgent,
      viewport: { width: window.innerWidth, height: window.innerHeight },
      datasetId: build.serverId,
      url: window.location.href,
    },
    ui: uiSnapshot,
  };
}
