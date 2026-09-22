/**
 * Where a build's preview image lives, and which builds may have one —
 * SECURITY_AUDIT.md F08.
 *
 * The `build-previews` bucket is public by flag and carries no
 * `storage.objects` policies at all, so an object in it is readable by anyone
 * who knows its path — and the path is `previews/<build id>.png`,
 * deterministic on purpose, because `delete-build` reconstructs it without a
 * read.
 *
 * **So the object's audience is exactly "whoever holds the build id", and the
 * rule is that this must not be a larger audience than the build's own.**
 * A preview has one consumer: the `build-og` Worker, which resolves a build
 * through `get-build` with the anon key and serves the object as `og:image` to
 * crawlers following a share link. `get-build` answers by id for `public` and
 * `unlisted` and refuses `private` to everyone but its owner. Line the two up:
 *
 *   - `public`, `unlisted` — the id already yields the whole build to anyone
 *     who has it, so the preview discloses nothing further. Unlisted needs its
 *     preview for the Discord embed that is the entire point of unlisted.
 *   - `private` — the id yields a 404 from `get-build` and the PNG from
 *     Storage, and that PNG renders the build's name, archetype, powers and
 *     stats. That gap is the finding.
 *
 * Measured 2026-09-18: 630 objects, 371 of them for a build that is not
 * public. Re-read against the rule above, **360 of those are the leak** (the
 * private ones) and 11 are unlisted and legitimate. Three write paths each
 * decided this for themselves and none of them checked. One rule, one place —
 * F10 is the same file's demonstration of what a second copy costs.
 */

/** The visibilities a shared build can have. */
export type Visibility = 'private' | 'unlisted' | 'public';

/**
 * The object path for a build's preview. Deterministic — `delete-build` removes
 * it without first reading `preview_image_path`, so this must stay a pure
 * function of the id.
 */
export function previewObjectPath(buildId: string): string {
  return `previews/${buildId}.png`;
}

/**
 * May a build at this visibility have a preview object?
 *
 * Unknown or absent visibility is `false`: the bucket has no policy behind
 * this, so the refusal has to be the default rather than the exception.
 */
export function previewMayExist(visibility: string | null | undefined): boolean {
  return visibility === 'public' || visibility === 'unlisted';
}
