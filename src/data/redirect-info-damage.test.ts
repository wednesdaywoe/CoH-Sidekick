import { describe, it, expect } from 'vitest';
import fs from 'node:fs';
import { fileURLToPath } from 'node:url';

/**
 * Regression guard for `*_Info` display-damage resolution (convert-powerset
 * `collectInfoRedirectTemplates`).
 *
 * Remote Bomb (Blaster Devices + all Traps ATs) is a pure mode-conditional
 * redirect shell: the player power has no effects, and its Self/Target/detonation
 * redirects carry only a scale-0 placeholder + the bomb-pet summon. The game's
 * player-facing damage lives on a `<name>_Info` / `_Blaster_Info` power
 * (`show_in_info`, condition `'0'`). The converter now follows that when the
 * mechanical redirect produced no damage, stripping the redundant
 * `arch source> Class_<AT>` selector and the PvP `enttype` KB variant, and
 * bypassing the Fiery-Embrace-bonus heuristic (which would wrongly strip the
 * genuine base Fire damage). See HOMECOMING_PARSER.
 *
 * STRIP-1 restatement: the `_Info` resolution's damage now lands TOP-LEVEL (`power.damage`)
 * instead of in the retired `effects` bag, so the Fire 2.0 / Lethal 3.0 halves are read from
 * the same `"damage"` array the bag used to project. The `_Info` KNOCKBACK half (scale 4,
 * PvE branch — the PvP `enttype` copy was stripped so it never became 8) had its ONLY home
 * in the retired `effects.knockback` slot: the strip deleted the bag and no atom carries a
 * Knockback row for this power. The "KB not doubled" claim now grades an empty population,
 * so it is restated as the measured fact (no `"knockback"` anywhere in the file) and the
 * loss itself is filed as a data gap (STRIP-1: Remote Bomb's tooltip KB no longer reaches
 * any consumer). Register row: <docs/DATA-GAP-REGISTER.md KB-INFO>.
 */
function gen(dataset: string, rel: string): string {
  const p = fileURLToPath(new URL(`./datasets/${dataset}/generated/powersets/${rel}`, import.meta.url));
  return fs.readFileSync(p, 'utf8');
}

describe('Remote Bomb surfaces its _Info display damage', () => {
  it('Blaster Devices: Fire 2.0 + Lethal 3.0 surface; the _Info KB died with the bag (gap)', () => {
    const t = gen('homecoming', 'blaster/secondary/devices/time-bomb.ts');
    // The _Info resolution's damage half: Fire kept (FE heuristic bypassed), Lethal intact.
    expect(t).toMatch(/"type":\s*"Fire",\s*"scale":\s*2\b/);
    expect(t).toMatch(/"type":\s*"Lethal",\s*"scale":\s*3\b/);
    // STRIP-1 measured population: the _Info knockback (scale 4, PvE branch, never 8) lived
    // ONLY in the retired `effects.knockback` slot. The file now contains no knockback at
    // all — atom stream included — so the "not doubled" guard would be green on nothing.
    // Assert the absence, and see the gap register (KB-INFO) for the loss itself.
    expect(t).not.toMatch(/"knockback"/);
  });

  it('Traps (Defender) Time Bomb also surfaces its damage', () => {
    const t = gen('homecoming', 'defender/primary/traps/time-bomb.ts');
    expect(t).toContain('"damage"');
    expect(t).toMatch(/"type":\s*"Fire"/);
  });
});
