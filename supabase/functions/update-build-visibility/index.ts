/**
 * Supabase Edge Function: update-build-visibility
 *
 * Sets the visibility ('private' | 'unlisted' | 'public') on a shared build.
 * Requires Discord OAuth authentication — only the authenticated owner can
 * change visibility. Anonymous (token-only) builds cannot be made private
 * or unlisted because there is no persistent identity to enforce ownership.
 *
 * Also accepts a legacy boolean `is_public` body (true→'public',
 * false→'private') for an old frontend deployed before this function —
 * the same deploy-skew tolerance share-build already uses.
 *
 * Deploy with: supabase functions deploy update-build-visibility
 */

import { createClient } from 'https://esm.sh/@supabase/supabase-js@2';
import { previewMayExist, previewObjectPath } from '../_shared/preview-visibility.ts';

const corsHeaders = {
  'Access-Control-Allow-Origin': '*',
  'Access-Control-Allow-Headers': 'authorization, x-client-info, apikey, content-type',
};

const VALID_VISIBILITIES = ['private', 'unlisted', 'public'] as const;
type Visibility = typeof VALID_VISIBILITIES[number];

/** Extract authenticated user ID from JWT in Authorization header */
async function getUserIdFromAuth(
  req: Request,
  supabaseUrl: string,
  supabaseServiceKey: string,
): Promise<string | null> {
  const authHeader = req.headers.get('authorization');
  if (!authHeader?.startsWith('Bearer ')) return null;

  try {
    const token = authHeader.replace('Bearer ', '');
    const supabase = createClient(supabaseUrl, supabaseServiceKey);
    const { data: { user } } = await supabase.auth.getUser(token);
    return user?.id ?? null;
  } catch {
    return null;
  }
}

Deno.serve(async (req: Request) => {
  if (req.method === 'OPTIONS') {
    return new Response('ok', { headers: corsHeaders });
  }

  try {
    const body = await req.json();
    const { id } = body;

    if (!id) {
      return new Response(
        JSON.stringify({ error: 'Build ID is required' }),
        { status: 400, headers: { ...corsHeaders, 'Content-Type': 'application/json' } },
      );
    }

    // Legacy callers send a boolean is_public; new callers send visibility.
    let visibility: Visibility;
    if (typeof body.visibility === 'string') {
      if (!VALID_VISIBILITIES.includes(body.visibility)) {
        return new Response(
          JSON.stringify({ error: `visibility must be one of: ${VALID_VISIBILITIES.join(', ')}` }),
          { status: 400, headers: { ...corsHeaders, 'Content-Type': 'application/json' } },
        );
      }
      visibility = body.visibility;
    } else if (typeof body.is_public === 'boolean') {
      visibility = body.is_public ? 'public' : 'private';
    } else {
      return new Response(
        JSON.stringify({ error: 'visibility (or legacy is_public boolean) is required' }),
        { status: 400, headers: { ...corsHeaders, 'Content-Type': 'application/json' } },
      );
    }

    const supabaseUrl = Deno.env.get('SUPABASE_URL')!;
    const supabaseServiceKey = Deno.env.get('SUPABASE_SERVICE_ROLE_KEY')!;
    const supabase = createClient(supabaseUrl, supabaseServiceKey);

    // Authentication required — visibility is a login-only feature
    const authUserId = await getUserIdFromAuth(req, supabaseUrl, supabaseServiceKey);
    if (!authUserId) {
      return new Response(
        JSON.stringify({ error: 'Authentication required to change build visibility' }),
        { status: 401, headers: { ...corsHeaders, 'Content-Type': 'application/json' } },
      );
    }

    // Verify the authenticated user owns this build
    const { data: build } = await supabase
      .from('shared_builds')
      .select('id, user_id')
      .eq('id', id)
      .eq('user_id', authUserId)
      .single();

    if (!build) {
      return new Response(
        JSON.stringify({ error: 'Build not found or not authorized' }),
        { status: 403, headers: { ...corsHeaders, 'Content-Type': 'application/json' } },
      );
    }

    // F08: flipping a build private must take its preview with it. The
    // `build-previews` bucket is public and the object path is the build id,
    // so a preview left behind keeps answering for an id this call has just
    // made `get-build` refuse. The column is nulled in the same update as the
    // visibility - a row pointing at an object we are about to remove is the
    // other half of the same lie.
    const keepsPreview = previewMayExist(visibility);
    const { error: updateError } = await supabase
      .from('shared_builds')
      .update({
        visibility,
        updated_at: new Date().toISOString(),
        ...(keepsPreview ? {} : { preview_image_path: null, preview_template_version: null }),
      })
      .eq('id', id);

    if (updateError) {
      console.error('Update error:', updateError);
      return new Response(
        JSON.stringify({ error: 'Failed to update build visibility' }),
        { status: 500, headers: { ...corsHeaders, 'Content-Type': 'application/json' } },
      );
    }

    // Best-effort and after the row, matching `delete-build`: the column no
    // longer points at it either way, so a failed removal is an orphan rather
    // than a live exposure, and it must not fail the visibility change.
    if (!keepsPreview) {
      const { error: previewError } = await supabase.storage
        .from('build-previews')
        .remove([previewObjectPath(id)]);
      if (previewError) console.error('Preview image removal failed:', previewError);
    }

    return new Response(
      JSON.stringify({ id, visibility, is_public: visibility === 'public' }),
      { status: 200, headers: { ...corsHeaders, 'Content-Type': 'application/json' } },
    );
  } catch (e) {
    console.error('Unexpected error:', e);
    return new Response(
      JSON.stringify({ error: 'Internal server error' }),
      { status: 500, headers: { ...corsHeaders, 'Content-Type': 'application/json' } },
    );
  }
});
