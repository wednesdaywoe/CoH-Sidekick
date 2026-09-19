/**
 * Supabase Edge Function: backfill-preview
 *
 * Accepts a preview image captured by a hidden `?previewCapture=` boot for a
 * build that predates the share-preview feature, or was rendered under an
 * older visual template — see streams/BUILD_PREVIEW_BACKFILL_PLAN.md
 * (PREVBF6). No auth: any visitor's browser can be the one that generates
 * it, which is the whole point of "automatic on view". That's bounded on the
 * write side instead - version-gated (never accepts a write when the row's
 * `preview_template_version` is already current), a server-side shape check
 * (must decode to exactly PREVIEW_CARD_WIDTH x PREVIEW_CARD_HEIGHT, under the
 * existing byte cap), and a per-IP rolling window. See the plan doc's
 * "Decision - anonymous-write security".
 *
 * **The window is SECURITY_AUDIT.md F11, and it is there for the one thing
 * that decision did not have in front of it.** It priced the residual as a
 * race on a build an attacker happened upon, and rejected a *per-build* limit
 * as adding nothing the shape check did not already bound. That is true of one
 * build. It is not true of the corpus: `preview_template_version` is in the
 * anon `GRANT SELECT` list on `shared_builds` - it has to be, because it is the
 * fact a visitor's own client reads to decide whether to capture at all - so
 * one anonymous PostgREST query returns the complete list of writable targets.
 * Measured 2026-09-19: **2,469 of 2,668 rows**, in one request. With no limit
 * of any kind here, that is a scripted sweep rather than a race, and the image
 * it plants is what the build-og Worker serves as `og:image` to every crawler
 * that unfurls the link - `previewCacheKey` keys on `preview_template_version`,
 * which is precisely the column this write moves, so the cache-busting built
 * for a legitimate regeneration carries a planted image just as promptly.
 *
 * A per-IP window is the control that decision never weighed, and it restores
 * the bound it thought it had: a visitor still backfills every stale build they
 * actually look at, and a sweep of 2,469 needs 2,469 addresses.
 *
 * Deploy with: supabase functions deploy backfill-preview
 */

import { createClient } from 'https://esm.sh/@supabase/supabase-js@2';
import { callerIp, gradeWindow, windowStart } from '../_shared/rate-window.ts';

const corsHeaders = {
  'Access-Control-Allow-Origin': '*',
  'Access-Control-Allow-Headers': 'authorization, x-client-info, apikey, content-type',
};

// Mirrors src/components/export-image/BuildPreviewCard.tsx's
// CURRENT_PREVIEW_TEMPLATE_VERSION / PREVIEW_CARD_WIDTH / PREVIEW_CARD_HEIGHT
// and share-build/index.ts's MAX_PREVIEW_IMAGE_BYTES — Deno functions can't
// import frontend TS, so these are hand-kept duplicates. Bump every copy
// together whenever BuildPreviewCard's visual template changes.
const CURRENT_PREVIEW_TEMPLATE_VERSION = 6;
const PREVIEW_CARD_WIDTH = 1200;
const PREVIEW_CARD_HEIGHT = 880;
const MAX_PREVIEW_IMAGE_BYTES = 2 * 1024 * 1024;

// Per-IP write allowance (F11). Generous against a human browsing build pages
// - a backfill only fires for a build whose image is missing or stale, and a
// build stops being a trigger for everyone the moment one visitor fills it in
// - and ruinous against a sweep of the 2,469 rows currently in reach. Shares
// the `rate_limits` table and the two-hour pg_cron sweep share-build already
// has, under its own `action` so the two allowances never take from each other.
const BACKFILL_RATE_LIMIT = 30;
const RATE_WINDOW_HOURS = 1;
const RATE_LIMIT_ACTION = 'preview';

const PNG_SIGNATURE = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

/** Read width/height from a PNG's IHDR chunk without decoding pixel data.
 *  `null` for anything that isn't a well-formed PNG with IHDR first (true of
 *  every encoder in practice, including the client's `html-to-image`). */
function readPngDimensions(bytes: Uint8Array): { width: number; height: number } | null {
  if (bytes.byteLength < 24) return null;
  for (let i = 0; i < 8; i++) if (bytes[i] !== PNG_SIGNATURE[i]) return null;
  const chunkType = String.fromCharCode(bytes[12], bytes[13], bytes[14], bytes[15]);
  if (chunkType !== 'IHDR') return null;
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  return { width: view.getUint32(16, false), height: view.getUint32(20, false) };
}

Deno.serve(async (req: Request) => {
  if (req.method === 'OPTIONS') {
    return new Response('ok', { headers: corsHeaders });
  }

  const fail = (status: number, error: string) =>
    new Response(JSON.stringify({ error }), {
      status,
      headers: { ...corsHeaders, 'Content-Type': 'application/json' },
    });

  try {
    const { id, preview_image_base64: base64 } = await req.json();

    if (typeof id !== 'string' || !id) return fail(400, 'Build ID is required');
    if (typeof base64 !== 'string' || base64.length === 0) return fail(400, 'preview_image_base64 is required');

    let bytes: Uint8Array;
    try {
      bytes = Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
    } catch {
      return fail(400, 'preview_image_base64 is not valid base64');
    }
    if (bytes.byteLength === 0 || bytes.byteLength > MAX_PREVIEW_IMAGE_BYTES) {
      return fail(400, 'Image is empty or too large');
    }
    const dimensions = readPngDimensions(bytes);
    if (!dimensions || dimensions.width !== PREVIEW_CARD_WIDTH || dimensions.height !== PREVIEW_CARD_HEIGHT) {
      return fail(400, `Image must be a ${PREVIEW_CARD_WIDTH}x${PREVIEW_CARD_HEIGHT} PNG`);
    }

    const supabaseUrl = Deno.env.get('SUPABASE_URL')!;
    const supabaseServiceKey = Deno.env.get('SUPABASE_SERVICE_ROLE_KEY')!;
    const supabase = createClient(supabaseUrl, supabaseServiceKey);

    const { data: row, error: rowError } = await supabase
      .from('shared_builds')
      .select('visibility, preview_template_version')
      .eq('id', id)
      .maybeSingle();
    if (rowError || !row) return fail(404, 'Build not found');
    if (row.visibility === 'private') return fail(403, 'Build is private');

    const storedVersion = row.preview_template_version as number | null;
    if (storedVersion !== null && storedVersion >= CURRENT_PREVIEW_TEMPLATE_VERSION) {
      // Not an error — just nothing to do. Version-gated write: an owner's
      // real share (share-build) is the only path that overwrites a current
      // image; this one only fills in missing or stale ones.
      return new Response(JSON.stringify({ success: true, skipped: true }), {
        status: 200,
        headers: { ...corsHeaders, 'Content-Type': 'application/json' },
      });
    }

    // ---- Per-IP rolling window (F11) ----
    // Metered HERE, after the version gate: a request for a build that is
    // already current is answered `skipped` above and costs nothing, so a
    // visitor who reloads a finished page forever spends no allowance. What is
    // metered is the thing worth bounding, which is the write.
    const ip = callerIp(req.headers);
    const since = windowStart(Date.now(), RATE_WINDOW_HOURS);

    const { count } = await supabase
      .from('rate_limits')
      .select('*', { count: 'exact', head: true })
      .eq('ip', ip)
      .eq('action', RATE_LIMIT_ACTION)
      .gte('created_at', since);

    const used = count ?? 0;
    if (used >= BACKFILL_RATE_LIMIT) {
      const { data: oldest } = await supabase
        .from('rate_limits')
        .select('created_at')
        .eq('ip', ip)
        .eq('action', RATE_LIMIT_ACTION)
        .gte('created_at', since)
        .order('created_at', { ascending: true })
        .limit(1)
        .maybeSingle();

      const verdict = gradeWindow({
        used,
        limit: BACKFILL_RATE_LIMIT,
        oldest: oldest?.created_at as string | null | undefined,
        now: Date.now(),
        windowHours: RATE_WINDOW_HOURS,
      });

      return new Response(
        JSON.stringify({
          error: 'Too many preview backfills from this address. Please try again later.',
          code: 'rate_limited',
          action: RATE_LIMIT_ACTION,
          limit: BACKFILL_RATE_LIMIT,
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
        },
      );
    }

    // Spent before the write, not after: a slot that is only recorded on
    // success is not a limit, because the way to exceed it is to fail.
    await supabase.from('rate_limits').insert({ ip, action: RATE_LIMIT_ACTION });

    const path = `previews/${id}.png`;
    const { error: uploadError } = await supabase.storage
      .from('build-previews')
      .upload(path, bytes, { contentType: 'image/png', upsert: true });
    if (uploadError) {
      console.error('Preview image upload failed:', uploadError);
      return fail(500, 'Failed to upload preview image');
    }

    const { error: updateError } = await supabase
      .from('shared_builds')
      .update({ preview_image_path: path, preview_template_version: CURRENT_PREVIEW_TEMPLATE_VERSION })
      .eq('id', id);
    if (updateError) {
      console.error('Preview backfill update failed:', updateError);
      return fail(500, 'Failed to record preview image');
    }

    return new Response(JSON.stringify({ success: true }), {
      status: 200,
      headers: { ...corsHeaders, 'Content-Type': 'application/json' },
    });
  } catch (e) {
    console.error('Unexpected error:', e);
    return fail(500, 'Internal server error');
  }
});
