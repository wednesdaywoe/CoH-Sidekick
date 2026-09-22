/**
 * Supabase Edge Function: share-build
 *
 * Validates build data, applies rate limiting, generates a short ID,
 * and inserts the shared build into the database.
 *
 * Supports both creating new builds and updating existing ones via owner token
 * or authenticated user identity (Discord OAuth).
 *
 * Deploy with: supabase functions deploy share-build
 */

import { createClient } from 'https://esm.sh/@supabase/supabase-js@2';
import { nanoid } from 'https://esm.sh/nanoid@5';
import { handleCandidate, sanitizeAuthorName } from '../_shared/author-name.ts';
import { mayWriteBuild } from '../_shared/build-ownership.ts';
import { classifyCaller, type Caller, type Verdict } from '../_shared/caller-identity.ts';
import { previewMayExist, previewObjectPath } from '../_shared/preview-visibility.ts';
import { callerIp, gradeWindow, windowStart } from '../_shared/rate-window.ts';

const SHARE_RATE_LIMIT = 10;  // max public shares per hour
const VAULT_RATE_LIMIT = 50;  // max vault saves per hour (private library — more generous)
const RATE_WINDOW_HOURS = 1;

const corsHeaders = {
  'Access-Control-Allow-Origin': '*',
  'Access-Control-Allow-Headers': 'authorization, x-client-info, apikey, content-type',
};

const VALID_VISIBILITIES = ['private', 'unlisted', 'public'] as const;
type Visibility = typeof VALID_VISIBILITIES[number];

/**
 * The half of the author_name rule that needs the database — SECURITY_AUDIT.md
 * F69. `sanitizeAuthorName` has already taken the characters that let one name
 * render as another; what is left is a name that reads, in plain text, as an
 * identity somebody else holds.
 *
 * Returns a refusal message, or null to allow.
 *
 * **Two checks, and deliberately not a third.**
 *
 *   1. **`reserved_handles` where `reason = 'system'`** — admin, api, auth,
 *      support, help, system. The service's own names, and a build card
 *      reading "support" is the impersonation with the most leverage. The list
 *      is read from the table rather than spelled here, so adding a name is an
 *      INSERT and not a deploy. The `reason` filter is what keeps it usable:
 *      the same table's `route` rows (me, new, edit, builds) and `sentinel`
 *      rows (null, undefined, **anonymous**) are ordinary things to type in an
 *      author box, and refusing "Anonymous" on an anonymous share would be the
 *      fix doing more damage than the finding.
 *   2. **`profiles.handle`, when it is not the caller's own** — a claimed,
 *      unique slug that already resolves at `/author/@handle`. An anonymous
 *      sharer typing an exact registered handle is the vector; the same string
 *      from the account that owns it is that user writing their own name.
 *
 * **Not checked: `profiles.display_name`.** Display names are not unique and
 * never were — two people genuinely called "Savant" both get to be Savant, and
 * a rule against that would refuse far more real shares than fake ones. The
 * collision that remains is answered where it belongs, by the client drawing
 * the proved `@handle` beside a name whose account holds one. Both clients do
 * that as of 2026-09-18 — the desktop card and detail view via
 * `cloud/profile.rs::author_identity`, this one via
 * `utils/author-identity.ts`.
 *
 * Both lookups are skipped entirely unless the name could be a handle at all
 * (`handleCandidate`), so an ordinary "Wednesday Woe" costs no round trip.
 */
async function impersonatedIdentity(
  supabase: ReturnType<typeof createClient>,
  authorName: string,
  authUserId: string | null,
): Promise<string | null> {
  const candidate = handleCandidate(authorName);
  if (candidate === null) return null;

  // Both columns are CITEXT, so `eq` is already case-insensitive at the
  // database — "SAVANT" and "savant" are the same key.
  const [reserved, claimed] = await Promise.all([
    supabase
      .from('reserved_handles')
      .select('handle')
      .eq('handle', candidate)
      .eq('reason', 'system')
      .maybeSingle(),
    supabase
      .from('profiles')
      .select('user_id')
      .eq('handle', candidate)
      .maybeSingle(),
  ]);

  if (reserved.data) {
    return `"${authorName}" is a reserved name. Please use a different author name.`;
  }
  if (claimed.data && claimed.data.user_id !== authUserId) {
    return `"${authorName}" is the handle of a registered account. Please use a different author name.`;
  }
  return null;
}

/** SHA-256 hash a string, returning hex digest */
async function sha256(input: string): Promise<string> {
  const data = new TextEncoder().encode(input);
  const hashBuffer = await crypto.subtle.digest('SHA-256', data);
  return [...new Uint8Array(hashBuffer)].map(b => b.toString(16).padStart(2, '0')).join('');
}

// Generous headroom over the compact 1200×800 share-preview PNG this is meant
// for (typically well under 300KB) — just enough to reject an abusive payload
// without rejecting a legitimate one.
const MAX_PREVIEW_IMAGE_BYTES = 2 * 1024 * 1024;

// Mirrors src/components/export-image/BuildPreviewCard.tsx's
// CURRENT_PREVIEW_TEMPLATE_VERSION — Deno functions can't import frontend TS,
// so this is a hand-kept duplicate. Bump both together whenever that file's
// visual template changes. See streams/BUILD_PREVIEW_BACKFILL_PLAN.md (PREVBF1).
const CURRENT_PREVIEW_TEMPLATE_VERSION = 6;

/**
 * Best-effort: upload a base64-encoded PNG (from the client's off-screen
 * BuildPreviewCard capture, see src/utils/preview-capture.ts) to the
 * `build-previews` Storage bucket and return its object path, or null on any
 * problem (missing/oversized/malformed input, upload failure). Never throws —
 * a broken preview image must not break the share itself.
 */
async function uploadPreviewImage(
  supabase: ReturnType<typeof createClient>,
  buildId: string,
  base64: unknown,
): Promise<string | null> {
  if (typeof base64 !== 'string' || base64.length === 0) return null;
  try {
    const bytes = Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
    if (bytes.byteLength === 0 || bytes.byteLength > MAX_PREVIEW_IMAGE_BYTES) return null;
    const path = previewObjectPath(buildId);
    const { error } = await supabase.storage
      .from('build-previews')
      .upload(path, bytes, { contentType: 'image/png', upsert: true });
    if (error) {
      console.error('Preview image upload failed:', error);
      return null;
    }
    return path;
  } catch (e) {
    console.error('Preview image decode failed:', e);
    return null;
  }
}

/**
 * Who is calling - SECURITY_AUDIT.md F09. The rule and the reasoning live in
 * `_shared/caller-identity.ts`; this is the half that needs the network.
 *
 * What this replaced returned `null` both for a signed-out visitor and for a
 * session it could not confirm, and this function is the one place where that
 * conflation costs something: `null` is the *more permissive* branch here,
 * twice. It forces a requested `private` to `public` (below), and it files the
 * row with `user_id: null`, which both hides the build from the library its
 * owner expects it in and leaves it answering to the owner token alone. All of
 * that under a 200 that tells the client the save worked.
 *
 * GoTrue reports a rejected token and an unreachable service the same way, as
 * an `error` rather than a throw, so they are told apart by the error's name -
 * auth-js raises `AuthRetryableFetchError` for the transport and an
 * `AuthApiError` for a verdict. Both refuse; only the message differs.
 */
async function classifyRequestCaller(
  req: Request,
  supabaseUrl: string,
  supabaseServiceKey: string,
): Promise<Caller> {
  return await classifyCaller(req.headers.get('authorization'), async (token): Promise<Verdict> => {
    const supabase = createClient(supabaseUrl, supabaseServiceKey);
    const { data, error } = await supabase.auth.getUser(token);
    if (error) {
      return { outcome: error.name === 'AuthRetryableFetchError' ? 'unreachable' : 'rejected' };
    }
    return data.user ? { outcome: 'user', userId: data.user.id } : { outcome: 'rejected' };
  });
}

Deno.serve(async (req: Request) => {
  // Handle CORS preflight
  if (req.method === 'OPTIONS') {
    return new Response('ok', { headers: corsHeaders });
  }

  try {
    const body = await req.json();

    // ---- Who is calling (F09) ----
    const supabaseUrl = Deno.env.get('SUPABASE_URL')!;
    const supabaseServiceKey = Deno.env.get('SUPABASE_SERVICE_ROLE_KEY')!;
    const caller = await classifyRequestCaller(req, supabaseUrl, supabaseServiceKey);

    // A session we cannot confirm is refused, not demoted to anonymous (F09).
    // First thing after reading the body, before validation and before the
    // rate-limit slot is spent, because none of that work is this caller's to
    // have done.
    if (caller.kind === 'unverified') {
      return new Response(
        JSON.stringify({ error: caller.detail, code: 'auth_unverified' }),
        { status: 401, headers: { ...corsHeaders, 'Content-Type': 'application/json' } }
      );
    }
    const authUserId = caller.kind === 'user' ? caller.userId : null;

    const isUpdate = !!(body.existing_id && (body.owner_token || authUserId));

    // visibility defaults to 'public'. A build is 'private' or 'unlisted' ONLY
    // when an *authenticated* user asked for it — an anonymous request for
    // anything but public is forced to 'public', because there is no
    // persistent identity for them to reclaim a private/unlisted link with.
    //
    // This downgrade is only defensible because `authUserId` is now null for
    // one reason and not two: a caller who asserted a session we could not
    // confirm was refused above rather than arriving here as "anonymous" and
    // having their private build published (F09).
    // (Previously inverted: `authUserId !== null` made logged-in "private"
    // saves PUBLIC and metered them against the public-share bucket, which is
    // why vault rows never appeared and library saves hit the strict share
    // limit.)
    //
    // Legacy callers send a boolean is_public; new callers send visibility.
    // When neither is provided on an update, the caller wants to preserve the
    // row's current visibility (a re-save that must not touch it) — we leave
    // the column out of the update payload in that case.
    let requestedVisibility: Visibility | undefined;
    if (typeof body.visibility === 'string' && VALID_VISIBILITIES.includes(body.visibility)) {
      requestedVisibility = body.visibility;
    } else if (typeof body.is_public === 'boolean') {
      requestedVisibility = body.is_public ? 'public' : 'private';
    }
    const visibilityProvided = requestedVisibility !== undefined;
    const visibility: Visibility = requestedVisibility === undefined
      ? 'public'
      : (authUserId === null ? 'public' : requestedVisibility);

    // ---- Validate required fields ----
    const { name, archetype, archetype_name, primary_set, primary_name, secondary_set, secondary_name, level, build_json } = body;

    if (!archetype || !primary_set || !secondary_set) {
      return new Response(
        JSON.stringify({ error: 'Build must have an archetype, primary, and secondary powerset' }),
        { status: 400, headers: { ...corsHeaders, 'Content-Type': 'application/json' } }
      );
    }

    if (!build_json || !build_json.version || !build_json.build) {
      return new Response(
        JSON.stringify({ error: 'Invalid build data format' }),
        { status: 400, headers: { ...corsHeaders, 'Content-Type': 'application/json' } }
      );
    }

    const buildLevel = typeof level === 'number' ? level : 50;
    if (buildLevel < 1 || buildLevel > 50) {
      return new Response(
        JSON.stringify({ error: 'Level must be between 1 and 50' }),
        { status: 400, headers: { ...corsHeaders, 'Content-Type': 'application/json' } }
      );
    }

    // ---- Supabase client (service role for inserts) ----
    const supabase = createClient(supabaseUrl, supabaseServiceKey);

    // ---- author_name (F69) ----
    // Normalised before anything else looks at it, and checked against the two
    // identity namespaces BEFORE the rate-limit slot is spent — a refusal here
    // is a validation error like the three above it, and burning a share on a
    // typo'd name would be a worse rule than the one being enforced.
    //
    // This does NOT enumerate anything that was not already public: whether a
    // handle is claimed is answerable by anyone through `resolve_author` and by
    // loading /author/@handle, so there is nothing here to meter.
    const authorName = sanitizeAuthorName(body.author_name);
    const impersonation = await impersonatedIdentity(supabase, authorName, authUserId);
    if (impersonation) {
      return new Response(
        JSON.stringify({ error: impersonation, code: 'author_name_conflict' }),
        { status: 400, headers: { ...corsHeaders, 'Content-Type': 'application/json' } }
      );
    }

    // ---- Rate limiting ----
    // The IP key and the window arithmetic are `_shared/rate-window.ts` (F10),
    // not a third hand-rolled copy. This function is the one the helper was
    // extracted FROM, and the copy left behind had drifted from it in the one
    // place that helper's docstring calls load-bearing: `??` where it uses
    // `||`, so an empty `x-forwarded-for` trimmed to `''` and was handed back
    // as an IP, giving every such caller their own private allowance.
    const clientIp = callerIp(req.headers);
    const since = windowStart(Date.now(), RATE_WINDOW_HOURS);

    // Vault saves ('private' and 'unlisted' — personal link-sharing gets the
    // same generosity as the private vault) and public shares use separate
    // rate limit buckets. A visibility-preserving update (neither field sent)
    // is a low-stakes re-save — meter it as a vault action.
    const meterAsPublic = visibilityProvided && visibility === 'public';
    const rateLimitAction = meterAsPublic ? 'share' : 'vault';
    const rateLimit = meterAsPublic ? SHARE_RATE_LIMIT : VAULT_RATE_LIMIT;

    const { count } = await supabase
      .from('rate_limits')
      .select('*', { count: 'exact', head: true })
      .eq('ip', clientIp)
      .eq('action', rateLimitAction)
      .gte('created_at', since);

    const used = count ?? 0;
    if (used >= rateLimit) {
      // Rolling window: a slot frees up when the OLDEST in-window request ages
      // out (its created_at + window). Surface that so the client can show a
      // precise "try again in ~N min" instead of a vague "try later".
      const { data: oldest } = await supabase
        .from('rate_limits')
        .select('created_at')
        .eq('ip', clientIp)
        .eq('action', rateLimitAction)
        .gte('created_at', since)
        .order('created_at', { ascending: true })
        .limit(1)
        .maybeSingle();

      const verdict = gradeWindow({
        used,
        limit: rateLimit,
        oldest: oldest?.created_at as string | null | undefined,
        now: Date.now(),
        windowHours: RATE_WINDOW_HOURS,
      });

      return new Response(
        JSON.stringify({
          error: 'Rate limit exceeded. Please try again later.',
          code: 'rate_limited',
          action: rateLimitAction,        // 'share' (public) | 'vault' (saved)
          limit: rateLimit,
          remaining: 0,
          retryAfterSeconds: verdict.retryAfterSeconds,
          resetAt: verdict.resetAt,
        }),
        {
          status: 429,
          headers: {
            ...corsHeaders,
            'Content-Type': 'application/json',
            'Retry-After': String(verdict.retryAfterSeconds),
          },
        }
      );
    }

    // Record this request for rate limiting
    await supabase.from('rate_limits').insert({ ip: clientIp, action: rateLimitAction });

    // Returned on success so the client can show "N of LIMIT used this hour".
    const rateLimitInfo = {
      action: rateLimitAction,
      limit: rateLimit,
      remaining: Math.max(0, rateLimit - (used + 1)),
    };

    const tags = Array.isArray(body.tags)
      ? body.tags.filter((t: unknown) => typeof t === 'string').slice(0, 10)
      : [];

    const buildData = {
      name: (name?.trim() || `${primary_name || 'Unknown'}/${secondary_name || 'Unknown'} ${archetype_name || 'Build'}`).slice(0, 200),
      description: (body.description || '').slice(0, 500),
      archetype,
      archetype_name: (archetype_name || '').slice(0, 100),
      primary_set,
      primary_name: (primary_name || '').slice(0, 100),
      secondary_set,
      secondary_name: (secondary_name || '').slice(0, 100),
      level: buildLevel,
      // Normalised and checked above (F69). The `.slice(0, 50)` this replaces
      // counted UTF-16 units, so a name of 50 astral characters was cut
      // between a surrogate pair and stored broken.
      author_name: authorName,
      server: (body.server || '').slice(0, 50),
      tags,
      build_json,
      // visibility is applied per-operation below: preserved on update when
      // the caller omitted it, always set on insert.
    };

    // ---- UPDATE existing build ----
    if (isUpdate) {
      // One read, then the shared rule: the owner token is the whole authority
      // on an unclaimed build, and no authority at all once an account owns it
      // (F30) - see _shared/build-ownership.ts. This path can also write
      // `visibility`, so a token that outranked a session made a private build
      // public.
      const { data: existing } = await supabase
        .from('shared_builds')
        .select('id, user_id, owner_token_hash, visibility')
        .eq('id', body.existing_id)
        .single();

      const tokenMatches =
        !!body.owner_token &&
        !!existing?.owner_token_hash &&
        (await sha256(body.owner_token)) === existing.owner_token_hash;

      if (
        !existing ||
        !mayWriteBuild({ buildUserId: existing.user_id, tokenMatches, authUserId })
      ) {
        return new Response(
          JSON.stringify({ error: 'Build not found or not authorized' }),
          { status: 403, headers: { ...corsHeaders, 'Content-Type': 'application/json' } }
        );
      }

      // Only write visibility when the caller explicitly provided it;
      // otherwise leave the column untouched so the current visibility is
      // preserved.
      const updateFields: Record<string, unknown> = { ...buildData, updated_at: new Date().toISOString() };
      if (visibilityProvided) updateFields.visibility = visibility;
      // A build's stats/powers can change between shares, so re-render on every
      // update too. Left out entirely (not nulled) when capture failed, so a
      // stale-but-present image beats no image rather than being wiped.
      // F08, and the reason the select above reads `visibility`: an update may
      // preserve the current visibility rather than state one, so the effective
      // visibility is the caller's when they gave it and the row's when they
      // did not. A build that ends this request private must not end it with a
      // readable preview either - including the case where it was public a
      // moment ago and the object is already sitting in the bucket.
      const effectiveVisibility = visibilityProvided ? visibility : existing.visibility;
      if (previewMayExist(effectiveVisibility)) {
        const previewPath = await uploadPreviewImage(supabase, body.existing_id, body.preview_image_base64);
        if (previewPath) {
          updateFields.preview_image_path = previewPath;
          updateFields.preview_template_version = CURRENT_PREVIEW_TEMPLATE_VERSION;
        }
      } else {
        const { error: previewError } = await supabase.storage
          .from('build-previews')
          .remove([previewObjectPath(body.existing_id)]);
        if (previewError) console.error('Preview image removal failed:', previewError);
        updateFields.preview_image_path = null;
        updateFields.preview_template_version = null;
      }

      const { error: updateError } = await supabase
        .from('shared_builds')
        .update(updateFields)
        .eq('id', body.existing_id);

      if (updateError) {
        console.error('Update error:', updateError);
        return new Response(
          JSON.stringify({ error: 'Failed to update build' }),
          { status: 500, headers: { ...corsHeaders, 'Content-Type': 'application/json' } }
        );
      }

      return new Response(
        JSON.stringify({ id: body.existing_id, url: `/builds/${body.existing_id}`, updated: true, rateLimit: rateLimitInfo }),
        { status: 200, headers: { ...corsHeaders, 'Content-Type': 'application/json' } }
      );
    }

    // ---- CREATE new build ----
    const id = nanoid(10);
    const ownerToken = crypto.randomUUID();
    const ownerTokenHash = await sha256(ownerToken);
    // A private build gets no preview object (F08): the bucket is public and
    // the path is the build id, so the PNG would answer for an id that
    // `get-build` refuses. Public and unlisted both already answer by id.
    const previewPath = previewMayExist(visibility)
      ? await uploadPreviewImage(supabase, id, body.preview_image_base64)
      : null;

    const { error: insertError } = await supabase.from('shared_builds').insert({
      id,
      ...buildData,
      visibility,  // new rows always set visibility explicitly
      owner_token_hash: ownerTokenHash,
      user_id: authUserId,  // null if not logged in, UUID if authenticated
      preview_image_path: previewPath,  // null when capture wasn't provided or failed
      preview_template_version: previewPath ? CURRENT_PREVIEW_TEMPLATE_VERSION : null,
    });

    if (insertError) {
      console.error('Insert error:', insertError);
      return new Response(
        JSON.stringify({ error: 'Failed to save build' }),
        { status: 500, headers: { ...corsHeaders, 'Content-Type': 'application/json' } }
      );
    }

    return new Response(
      JSON.stringify({ id, url: `/builds/${id}`, owner_token: ownerToken, rateLimit: rateLimitInfo }),
      { status: 200, headers: { ...corsHeaders, 'Content-Type': 'application/json' } }
    );
  } catch (e) {
    console.error('Unexpected error:', e);
    return new Response(
      JSON.stringify({ error: 'Internal server error' }),
      { status: 500, headers: { ...corsHeaders, 'Content-Type': 'application/json' } }
    );
  }
});
