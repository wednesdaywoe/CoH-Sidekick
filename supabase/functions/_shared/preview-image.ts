/**
 * What counts as a preview image — SECURITY_AUDIT.md F85.
 *
 * `preview-visibility.ts` answers which builds may have one; this answers what
 * the bytes have to be. They were two different answers in two files:
 * `backfill-preview` checked the PNG signature, the IHDR chunk and the exact
 * card dimensions, and `share-build` — the path an owner's real share takes,
 * and the one that writes most of the bucket — checked `byteLength` and
 * nothing else. F08's census of the bucket found neither a MIME allow-list nor
 * a size limit on the bucket itself, so the function's check is the only check
 * there is, and on one of the two paths it was a length.
 *
 * **What this does and does not settle.** The objects are served from the
 * project's own origin with `access-control-allow-origin: *` and **without**
 * `x-content-type-options: nosniff` — probed against a live object 2026-09-19,
 * and note that the storage API's *error* responses do send it, so a probe
 * against a bad path answers reassuringly while grading nothing. It resolves
 * toward blob-hosting rather than stored XSS because the upload declares
 * `image/png` and current browsers do not sniff a declared image type up into
 * HTML. Validating the bytes is what closes the blob-hosting half. The header
 * cannot be set from a function — Storage's upload options carry no custom
 * headers — so it is named rather than fixed.
 *
 * The four constants below were hand-duplicated across both functions, with a
 * comment on each copy asking the next person to keep them in step. One of the
 * comments had already gone stale (share-build's said 1200x800 against a card
 * that is 1200x880). They mirror `BuildPreviewCard.tsx` across a boundary a
 * Deno function genuinely cannot import; that copy stays, and this is the one
 * copy on this side of it.
 */

/** Bump with `BuildPreviewCard.tsx` whenever the visual template changes. */
export const CURRENT_PREVIEW_TEMPLATE_VERSION = 6;
export const PREVIEW_CARD_WIDTH = 1200;
export const PREVIEW_CARD_HEIGHT = 880;

/**
 * Headroom over the real card (typically well under 300KB) — enough to refuse
 * an abusive payload without refusing a legitimate one.
 */
export const MAX_PREVIEW_IMAGE_BYTES = 2 * 1024 * 1024;

const PNG_SIGNATURE = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

/**
 * Width and height from a PNG's IHDR chunk, without decoding pixel data.
 * `null` for anything that is not a well-formed PNG with IHDR first, which is
 * true of every encoder in practice including the client's `html-to-image`.
 */
export function readPngDimensions(bytes: Uint8Array): { width: number; height: number } | null {
  if (bytes.byteLength < 24) return null;
  for (let i = 0; i < 8; i++) if (bytes[i] !== PNG_SIGNATURE[i]) return null;
  const chunkType = String.fromCharCode(bytes[12], bytes[13], bytes[14], bytes[15]);
  if (chunkType !== 'IHDR') return null;
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  return { width: view.getUint32(16, false), height: view.getUint32(20, false) };
}

/** Why a candidate was refused, or `null` when it is a preview image. */
export type PreviewImageRefusal = 'empty' | 'too-large' | 'not-a-png' | 'wrong-size';

/**
 * Grade caller-supplied bytes as a preview image.
 *
 * Both writers ask this, so the bucket holds one kind of object. Returns the
 * refusal rather than throwing, because the two callers answer differently: a
 * bad `backfill-preview` payload is a 400, and a bad `share-build` payload
 * drops the preview and lets the share succeed.
 */
export function gradePreviewImage(bytes: Uint8Array): PreviewImageRefusal | null {
  if (bytes.byteLength === 0) return 'empty';
  if (bytes.byteLength > MAX_PREVIEW_IMAGE_BYTES) return 'too-large';
  const dimensions = readPngDimensions(bytes);
  if (!dimensions) return 'not-a-png';
  if (dimensions.width !== PREVIEW_CARD_WIDTH || dimensions.height !== PREVIEW_CARD_HEIGHT) {
    return 'wrong-size';
  }
  return null;
}
