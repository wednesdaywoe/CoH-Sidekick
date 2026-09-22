/**
 * SECURITY_AUDIT.md F85 — both writers grade the bytes the same way.
 *
 * The finding was that `share-build`, the path an owner's real share takes and
 * the one that writes most of the bucket, checked `byteLength` and nothing
 * else, while `backfill-preview` checked the signature, the IHDR chunk and the
 * card dimensions. The bucket has neither a MIME allow-list nor a size limit
 * of its own (F08's census), so the function's check is the only check there
 * is.
 */
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import {
  MAX_PREVIEW_IMAGE_BYTES,
  PREVIEW_CARD_HEIGHT,
  PREVIEW_CARD_WIDTH,
  gradePreviewImage,
  readPngDimensions,
} from './preview-image';

const read = (name: string) =>
  readFileSync(new URL(`../${name}/index.ts`, import.meta.url), 'utf8');

/** The first 24 bytes of a PNG: signature, IHDR length, IHDR, width, height. */
function pngHeader(width: number, height: number): Uint8Array {
  const bytes = new Uint8Array(24);
  bytes.set([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a], 0);
  const view = new DataView(bytes.buffer);
  view.setUint32(8, 13, false);
  bytes.set([0x49, 0x48, 0x44, 0x52], 12); // 'IHDR'
  view.setUint32(16, width, false);
  view.setUint32(20, height, false);
  return bytes;
}

describe('gradePreviewImage', () => {
  it('accepts the card the client actually captures', () => {
    expect(gradePreviewImage(pngHeader(PREVIEW_CARD_WIDTH, PREVIEW_CARD_HEIGHT))).toBeNull();
  });

  it('names why it refused, because the two callers answer differently', () => {
    expect(gradePreviewImage(new Uint8Array(0))).toBe('empty');
    expect(gradePreviewImage(new Uint8Array(MAX_PREVIEW_IMAGE_BYTES + 1))).toBe('too-large');
  });

  it('refuses bytes that are not a PNG at all', () => {
    // The clause `share-build` did not have. A length check admits anything.
    const html = new TextEncoder().encode('<html><script>alert(1)</script></html>'.padEnd(64, ' '));
    expect(gradePreviewImage(html)).toBe('not-a-png');
    // A GIF that would sniff as an image but is not the declared type.
    const gif = new Uint8Array(64);
    gif.set([0x47, 0x49, 0x46, 0x38, 0x39, 0x61], 0);
    expect(gradePreviewImage(gif)).toBe('not-a-png');
  });

  it('refuses a real PNG that is not the card', () => {
    expect(gradePreviewImage(pngHeader(1, 1))).toBe('wrong-size');
    expect(gradePreviewImage(pngHeader(PREVIEW_CARD_WIDTH, PREVIEW_CARD_HEIGHT + 1))).toBe('wrong-size');
  });

  it('refuses a PNG signature with no IHDR behind it', () => {
    const bytes = pngHeader(PREVIEW_CARD_WIDTH, PREVIEW_CARD_HEIGHT);
    bytes.set([0x49, 0x44, 0x41, 0x54], 12); // 'IDAT' first
    expect(gradePreviewImage(bytes)).toBe('not-a-png');
  });

  it('refuses a truncated header rather than reading past it', () => {
    expect(readPngDimensions(pngHeader(1200, 880).slice(0, 23))).toBeNull();
  });
});

describe('both writers ask it (F85)', () => {
  it('share-build grades before it uploads', () => {
    const source = read('share-build');
    const graded = source.indexOf('gradePreviewImage(bytes)');
    const uploaded = source.indexOf(".from('build-previews')");
    expect(graded).toBeGreaterThan(-1);
    expect(uploaded).toBeGreaterThan(graded);
  });

  it('backfill-preview grades through the same file', () => {
    expect(read('backfill-preview')).toContain('gradePreviewImage(bytes)');
  });

  it('neither writer keeps its own copy of the rule any more', () => {
    for (const fn of ['share-build', 'backfill-preview']) {
      const source = read(fn);
      expect(source).toContain("from '../_shared/preview-image.ts'");
      expect(source).not.toMatch(/const MAX_PREVIEW_IMAGE_BYTES\s*=/);
      expect(source).not.toContain('PNG_SIGNATURE');
      expect(source).not.toMatch(/function readPngDimensions/);
      expect(source).not.toMatch(/const CURRENT_PREVIEW_TEMPLATE_VERSION\s*=/);
    }
  });
});
