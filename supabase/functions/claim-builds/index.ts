/**
 * Supabase Edge Function: claim-builds
 *
 * Links existing token-owned builds to an authenticated user account.
 * Requires BOTH a valid JWT (Discord OAuth) AND valid owner tokens
 * for each build being claimed.
 *
 * Deploy with: supabase functions deploy claim-builds
 */

import { createClient } from 'https://esm.sh/@supabase/supabase-js@2';

import { mayClaimBuild } from '../_shared/build-ownership.ts';

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

Deno.serve(async (req: Request) => {
  if (req.method === 'OPTIONS') {
    return new Response('ok', { headers: corsHeaders });
  }

  try {
    const supabaseUrl = Deno.env.get('SUPABASE_URL')!;
    const supabaseServiceKey = Deno.env.get('SUPABASE_SERVICE_ROLE_KEY')!;
    const supabase = createClient(supabaseUrl, supabaseServiceKey);

    // ---- Authenticate user via JWT ----
    const authHeader = req.headers.get('authorization');
    if (!authHeader?.startsWith('Bearer ')) {
      return new Response(
        JSON.stringify({ error: 'Authentication required' }),
        { status: 401, headers: { ...corsHeaders, 'Content-Type': 'application/json' } }
      );
    }

    const token = authHeader.replace('Bearer ', '');
    const { data: { user } } = await supabase.auth.getUser(token);

    if (!user) {
      return new Response(
        JSON.stringify({ error: 'Invalid or expired session' }),
        { status: 401, headers: { ...corsHeaders, 'Content-Type': 'application/json' } }
      );
    }

    // ---- Parse request body ----
    const { owner_tokens } = await req.json();

    // owner_tokens: Record<string, string> — { buildId: ownerToken, ... }
    if (!owner_tokens || typeof owner_tokens !== 'object' || Object.keys(owner_tokens).length === 0) {
      return new Response(
        JSON.stringify({ error: 'owner_tokens map is required' }),
        { status: 400, headers: { ...corsHeaders, 'Content-Type': 'application/json' } }
      );
    }

    const entries = Object.entries(owner_tokens) as [string, string][];
    if (entries.length > 50) {
      return new Response(
        JSON.stringify({ error: 'Cannot claim more than 50 builds at once' }),
        { status: 400, headers: { ...corsHeaders, 'Content-Type': 'application/json' } }
      );
    }

    const claimed: string[] = [];
    const failed: string[] = [];

    for (const [buildId, ownerToken] of entries) {
      if (typeof ownerToken !== 'string' || !ownerToken) {
        failed.push(buildId);
        continue;
      }

      const tokenHash = await sha256(ownerToken);

      const { data: existing } = await supabase
        .from('shared_builds')
        .select('id, user_id')
        .eq('id', buildId)
        .eq('owner_token_hash', tokenHash)
        .single();

      // A token holder can claim what nobody owns and re-claim what is already
      // theirs. Taking a build from another account is F31: the old code ran
      // the UPDATE regardless, so a token lifted from localStorage moved the
      // row - see _shared/build-ownership.ts.
      if (
        !existing ||
        !mayClaimBuild({
          buildUserId: existing.user_id,
          tokenMatches: true,
          authUserId: user.id,
        })
      ) {
        failed.push(buildId);
        continue;
      }

      // Already ours; the client re-sends its whole token map on every login.
      if (existing.user_id === user.id) {
        claimed.push(buildId);
        continue;
      }

      // Set user_id on the build
      const { error: updateError } = await supabase
        .from('shared_builds')
        .update({ user_id: user.id })
        .eq('id', buildId);

      if (updateError) {
        console.error(`Failed to claim build ${buildId}:`, updateError);
        failed.push(buildId);
      } else {
        claimed.push(buildId);
      }
    }

    return new Response(
      JSON.stringify({ claimed, failed }),
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
