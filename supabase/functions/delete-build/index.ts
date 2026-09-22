/**
 * Supabase Edge Function: delete-build
 *
 * Deletes a shared build after verifying ownership. An unclaimed build answers
 * to its owner token; once an account owns it, only that account (Discord
 * OAuth) can delete it. See _shared/build-ownership.ts.
 *
 * Deploy with: supabase functions deploy delete-build
 */

import { createClient } from 'https://esm.sh/@supabase/supabase-js@2';

import { mayWriteBuild } from '../_shared/build-ownership.ts';
import { previewObjectPath } from '../_shared/preview-visibility.ts';

const corsHeaders = {
  'Access-Control-Allow-Origin': '*',
  'Access-Control-Allow-Headers': 'authorization, x-client-info, apikey, content-type',
};

/** SHA-256 hash a string, returning hex digest */
async function sha256(input: string): Promise<string> {
  const data = new TextEncoder().encode(input);
  const hashBuffer = await crypto.subtle.digest('SHA-256', data);
  return [...new Uint8Array(hashBuffer)].map(b => b.toString(16).padStart(2, '0')).join('');
}

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
    const { id, owner_token } = await req.json();

    if (!id) {
      return new Response(
        JSON.stringify({ error: 'Build ID is required' }),
        { status: 400, headers: { ...corsHeaders, 'Content-Type': 'application/json' } }
      );
    }

    const supabaseUrl = Deno.env.get('SUPABASE_URL')!;
    const supabaseServiceKey = Deno.env.get('SUPABASE_SERVICE_ROLE_KEY')!;
    const supabase = createClient(supabaseUrl, supabaseServiceKey);

    // Extract authenticated user (if logged in)
    const authUserId = await getUserIdFromAuth(req, supabaseUrl, supabaseServiceKey);

    if (!owner_token && !authUserId) {
      return new Response(
        JSON.stringify({ error: 'Owner token or authentication required' }),
        { status: 400, headers: { ...corsHeaders, 'Content-Type': 'application/json' } }
      );
    }

    // One read answers both questions the rule needs: whether the build exists,
    // and whether an account owns it. A matching owner token used to be enough
    // on its own, which let a token lifted from localStorage delete a claimed
    // private build (F80) - see _shared/build-ownership.ts.
    const { data: build } = await supabase
      .from('shared_builds')
      .select('id, user_id, owner_token_hash')
      .eq('id', id)
      .single();

    const tokenMatches =
      !!owner_token &&
      !!build?.owner_token_hash &&
      (await sha256(owner_token)) === build.owner_token_hash;

    if (!build || !mayWriteBuild({ buildUserId: build.user_id, tokenMatches, authUserId })) {
      return new Response(
        JSON.stringify({ error: 'Build not found or not authorized' }),
        { status: 403, headers: { ...corsHeaders, 'Content-Type': 'application/json' } }
      );
    }

    const { error: deleteError } = await supabase
      .from('shared_builds')
      .delete()
      .eq('id', id);

    if (deleteError) {
      console.error('Delete error:', deleteError);
      return new Response(
        JSON.stringify({ error: 'Failed to delete build' }),
        { status: 500, headers: { ...corsHeaders, 'Content-Type': 'application/json' } }
      );
    }

    // Best-effort: the row is already gone, so a leftover preview image in
    // Storage is orphaned either way — don't fail the delete over it. Path is
    // deterministic (see share-build's uploadPreviewImage), so no need to
    // read preview_image_path first; removing a nonexistent object is a no-op.
    const { error: storageError } = await supabase.storage
      .from('build-previews')
      .remove([previewObjectPath(id)]);
    if (storageError) console.error('Preview image cleanup failed:', storageError);

    return new Response(
      JSON.stringify({ success: true }),
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
