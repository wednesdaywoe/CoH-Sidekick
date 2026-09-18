/**
 * Supabase Edge Function: get-build
 *
 * Point lookup of a single shared build by its exact id. This is the ONLY
 * path a non-owner reads an unlisted build through — RLS grants unlisted
 * rows no anon/authenticated read access (see schema.sql), because a looser
 * policy would make them bulk-listable by anyone holding the public anon
 * key. This function only ever accepts one id and never a filter/listing
 * parameter, so it cannot be used to enumerate unlisted builds.
 *
 * Visibility rules:
 *   - 'public' or 'unlisted': readable by anyone who has the id.
 *   - 'private': readable only by the authenticated owner.
 *   - unknown id, or private and not the owner: 404 (same response either
 *     way — a 403 would confirm the id exists).
 *
 * Deploy with: supabase functions deploy get-build
 */

import { createClient } from 'https://esm.sh/@supabase/supabase-js@2';

const corsHeaders = {
  'Access-Control-Allow-Origin': '*',
  'Access-Control-Allow-Headers': 'authorization, x-client-info, apikey, content-type',
};

/**
 * What a detail read answers with, named one column at a time — the whole of
 * shared_builds_with_author's projection, which is the build document plus the
 * card fields plus the joined author profile.
 *
 * **This was `'*'`, and it is the one place the F73 fix cannot be delegated to
 * a grant.** The column grants in schema.sql narrow what `anon` and
 * `authenticated` may read from shared_builds; this function holds the SERVICE
 * ROLE key, which bypasses both RLS and those grants by design, because it is
 * how an unlisted build is read at all. So `select('*')` here would keep
 * answering with owner_token_hash — the sha256 of the credential that owns the
 * build — to any anonymous caller who has the id, for unlisted rows as much as
 * public ones. Measured on 2026-09-18: an anon answer was 25 keys with the
 * hash among them and non-null.
 *
 * Kept in step with the view by `supabase/schema-projection.test.ts`, which
 * parses this list out of this file and compares it against the view's
 * projection in schema.sql rather than restating either. The two failure modes
 * are not symmetric and the guard exists for the quiet one: naming a column the
 * view does not have fails the read loudly, while missing one the view does
 * have just means no client ever sees it.
 */
const BUILD_COLUMNS = [
  'id',
  'name',
  'description',
  'archetype',
  'archetype_name',
  'primary_set',
  'primary_name',
  'secondary_set',
  'secondary_name',
  'level',
  'author_name',
  'server',
  'tags',
  'build_json',
  'created_at',
  'updated_at',
  'views',
  'user_id',
  'visibility',
  'preview_image_path',
  'preview_template_version',
  'author_handle',
  'author_display_name',
  'author_avatar_url',
].join(',');

/** Extract authenticated user ID from JWT in Authorization header (if present) */
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

function notFound() {
  return new Response(
    JSON.stringify({ error: 'Build not found' }),
    { status: 404, headers: { ...corsHeaders, 'Content-Type': 'application/json' } },
  );
}

Deno.serve(async (req: Request) => {
  if (req.method === 'OPTIONS') {
    return new Response('ok', { headers: corsHeaders });
  }

  try {
    const { id } = await req.json();

    if (!id || typeof id !== 'string') {
      return new Response(
        JSON.stringify({ error: 'Build ID is required' }),
        { status: 400, headers: { ...corsHeaders, 'Content-Type': 'application/json' } },
      );
    }

    const supabaseUrl = Deno.env.get('SUPABASE_URL')!;
    const supabaseServiceKey = Deno.env.get('SUPABASE_SERVICE_ROLE_KEY')!;
    const supabase = createClient(supabaseUrl, supabaseServiceKey);

    const authUserId = await getUserIdFromAuth(req, supabaseUrl, supabaseServiceKey);

    const { data: build } = await supabase
      .from('shared_builds_with_author')
      .select(BUILD_COLUMNS)
      .eq('id', id)
      .single();

    if (!build) return notFound();

    const readable = build.visibility === 'public'
      || build.visibility === 'unlisted'
      || (build.visibility === 'private' && authUserId !== null && build.user_id === authUserId);

    if (!readable) return notFound();

    // Point reads don't count toward the public view counter — that's
    // incremented separately via the increment_views RPC, which any client
    // (including anon, via RLS-visible public rows) can already call.
    return new Response(
      JSON.stringify(build),
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
