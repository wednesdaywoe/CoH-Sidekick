/**
 * SECURITY_AUDIT.md F08 — a preview object may exist only for a build that
 * `get-build` will hand to anyone holding the id.
 *
 * The rule's reasoning is in `preview-visibility.ts`. What is graded here is
 * the rule itself, and then the four functions that used to each hold their
 * own opinion of it: `share-build` (both paths), `update-build-visibility`,
 * `backfill-preview` and `delete-build`. The source-guard shape follows the
 * `backfill-preview` block in `rate-window.test.ts`.
 */
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { previewMayExist, previewObjectPath } from './preview-visibility';

const read = (name: string) =>
  readFileSync(new URL(`../${name}/index.ts`, import.meta.url), 'utf8');

describe('previewMayExist', () => {
  it('lets the two visibilities that answer by id keep a preview', () => {
    // `get-build` serves both to any caller holding the id, so the object
    // discloses nothing the id did not already. Unlisted is not an oversight:
    // its Discord embed is the whole point of unlisted.
    expect(previewMayExist('public')).toBe(true);
    expect(previewMayExist('unlisted')).toBe(true);
  });

  it('refuses a private build, which is the finding', () => {
    // The id yields a 404 from `get-build` and the PNG from Storage, and the
    // PNG renders name, archetype, powers and stats.
    expect(previewMayExist('private')).toBe(false);
  });

  it('refuses anything it does not recognise, rather than allowing it', () => {
    // There is no `storage.objects` policy behind this predicate, so an
    // unknown visibility has to fail closed - it is the only thing refusing.
    expect(previewMayExist(null)).toBe(false);
    expect(previewMayExist(undefined)).toBe(false);
    expect(previewMayExist('')).toBe(false);
    expect(previewMayExist('Public')).toBe(false);
    expect(previewMayExist('deleted')).toBe(false);
  });
});

describe('previewObjectPath', () => {
  it('is a pure function of the id, because delete-build rebuilds it', () => {
    expect(previewObjectPath('abc1234567')).toBe('previews/abc1234567.png');
    expect(previewObjectPath('abc1234567')).toBe(previewObjectPath('abc1234567'));
  });

  it('is the only place that path is spelled', () => {
    for (const fn of ['share-build', 'delete-build', 'backfill-preview']) {
      expect(read(fn)).not.toMatch(/`previews\/\$\{/);
      expect(read(fn)).toContain('previewObjectPath');
    }
  });
});

describe('share-build writes no preview a stranger should not reach (F08)', () => {
  const source = read('share-build');

  it('gates the insert path on the rule', () => {
    expect(source).toContain("from '../_shared/preview-visibility.ts'");
    expect(source).toContain('previewMayExist(visibility)');
  });

  it('reads the current visibility, so a preserved one is knowable', () => {
    // An update may omit visibility entirely. Without this column in the
    // select there is no way to tell a preserved `private` from a preserved
    // `public`, and the gate would be deciding on `undefined`.
    expect(source).toContain("select('id, user_id, owner_token_hash, visibility')");
    expect(source).toContain('visibilityProvided ? visibility : existing.visibility');
  });

  it('takes the object away when an update lands the build private', () => {
    // The public -> private case: the object is already in the bucket, so
    // declining to upload a new one is not enough.
    const gate = source.indexOf('previewMayExist(effectiveVisibility)');
    const removed = source.indexOf("remove([previewObjectPath(body.existing_id)])");
    expect(gate).toBeGreaterThan(-1);
    expect(removed).toBeGreaterThan(gate);
    expect(source).toContain('updateFields.preview_image_path = null;');
  });
});

describe('update-build-visibility takes the preview with it (F08)', () => {
  const source = read('update-build-visibility');

  it('nulls the column in the same update as the visibility', () => {
    // A row pointing at an object about to be removed is the other half of
    // the same lie, and a separate update is a window where both are wrong.
    expect(source).toContain('const keepsPreview = previewMayExist(visibility);');
    expect(source).toContain('preview_image_path: null, preview_template_version: null');
  });

  it('removes the object after the row, and does not fail the flip over it', () => {
    const rowUpdated = source.indexOf("from('shared_builds')");
    const removed = source.indexOf("remove([previewObjectPath(id)])");
    expect(removed).toBeGreaterThan(rowUpdated);
    expect(source).toContain("console.error('Preview image removal failed:'");
  });
});

describe('backfill-preview asks the same question as the writers (F08)', () => {
  it('states the gate through the shared rule rather than its own', () => {
    const source = read('backfill-preview');
    expect(source).toContain('previewMayExist(row.visibility)');
    expect(source).not.toContain("row.visibility === 'private'");
  });
});
