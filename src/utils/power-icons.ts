/**
 * Unified power icon path resolution.
 *
 * All power icons (primary, secondary, pool, epic, inherent, incarnate)
 * live in a single flat folder: /img/powers/
 */

import { resolvePath } from '@/utils/paths';

/**
 * Get the full icon path for any power icon.
 *
 * @param iconFilename The bare icon filename (e.g., "fireblast_fireblast.png")
 * @returns Full resolved path (e.g., "/img/powers/fireblast_fireblast.png")
 */
export function getPowerIconPath(iconFilename: string | undefined): string {
  if (!iconFilename) {
    return resolvePath('/img/Unknown.png');
  }
  return resolvePath(`/img/powers/${vendoredName(iconFilename)}`);
}

/** The client's own extensions for power art, which are never what we ship it as. */
const CLIENT_TEXTURE_EXTENSIONS = ['.texture', '.dds', '.tga', '.png'];

/**
 * An export icon name as the vendored tree spells it: lower-case, and always `.png`.
 *
 * Lower-casing alone left two powers permanently blank — Rebirth's War Cry
 * (`martialmastery_warcry.dds`) and Thunderspy's Upgrade Equipment
 * (`knights_upgradeequipment.texture.png`). Both come out of `normalizeIconPath` in
 * `scripts/convert-powerset.cjs`, which reads `.dds` as an extension it should leave alone and
 * `.texture` as no extension at all. `.dds`/`.texture` are what the art is called inside the
 * game client; everything in `/img/powers/` is a PNG decoded out of it, so what the file is
 * named here is a fact about this tree rather than about the export — the same reason the
 * lower-casing lives here. Idempotent, so it stays harmless once the converter is fixed.
 *
 * Kept in step with `vendored_power_icon` in the Rust app's `view/icons.rs`; the two icon trees
 * are the same tree, and a rule in one of them only is a power that renders on one surface.
 */
function vendoredName(iconFilename: string): string {
  let stem = iconFilename.trim().toLowerCase();
  for (;;) {
    const ext = CLIENT_TEXTURE_EXTENSIONS.find((candidate) => stem.endsWith(candidate));
    if (!ext) return `${stem}.png`;
    stem = stem.slice(0, -ext.length);
  }
}
