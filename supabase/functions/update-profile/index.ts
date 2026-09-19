/**
 * Supabase Edge Function: update-profile
 *
 * Updates the authenticated user's profile (handle, display_name, bio) and
 * refreshes Discord-derived fields (discord_username, avatar_url) from the JWT.
 *
 * Auth required.
 *
 * Body (all fields optional; omitted fields are left untouched):
 *   handle        string  ^[a-z0-9][a-z0-9_-]{2,29}$, not reserved, unique.
 *                         First claim is free; subsequent changes gated by
 *                         HANDLE_COOLDOWN_DAYS (30), tracked via handle_changed_at.
 *   display_name  string  trimmed, max 30 chars, allows empty.
 *   bio           string  trimmed, max 280 chars, allows empty.
 *
 * Deploy with: supabase functions deploy update-profile
 */

import { createClient, type User } from 'https://esm.sh/@supabase/supabase-js@2';
import { storableAvatarUrl } from '../_shared/avatar-url.ts';

const HANDLE_REGEX = /^[a-z0-9][a-z0-9_-]{2,29}$/;
const HANDLE_COOLDOWN_DAYS = 30;
const HANDLE_COOLDOWN_MS = HANDLE_COOLDOWN_DAYS * 24 * 60 * 60 * 1000;
const DAY_MS = 24 * 60 * 60 * 1000;

const corsHeaders = {
  'Access-Control-Allow-Origin': '*',
  'Access-Control-Allow-Headers': 'authorization, x-client-info, apikey, content-type',
};

const jsonResponse = (body: unknown, status: number) =>
  new Response(JSON.stringify(body), {
    status,
    headers: { ...corsHeaders, 'Content-Type': 'application/json' },
  });

const jsonError = (message: string, status: number) => jsonResponse({ error: message }, status);

/** Extract authenticated user from JWT in Authorization header */
async function getUserFromAuth(
  req: Request,
  supabaseUrl: string,
  supabaseServiceKey: string,
): Promise<User | null> {
  const authHeader = req.headers.get('authorization');
  if (!authHeader?.startsWith('Bearer ')) return null;

  try {
    const token = authHeader.replace('Bearer ', '');
    const supabase = createClient(supabaseUrl, supabaseServiceKey);
    const { data, error } = await supabase.auth.getUser(token);
    if (error) {
      // Surface JWT validation failures (expired token, bad signature, etc.)
      // — silent rejection here is what made initial deployment so painful.
      console.warn('[auth] getUser failed:', error.message);
      return null;
    }
    return data.user ?? null;
  } catch (e) {
    console.warn('[auth] exception:', e instanceof Error ? e.message : String(e));
    return null;
  }
}

Deno.serve(async (req: Request) => {
  if (req.method === 'OPTIONS') {
    return new Response('ok', { headers: corsHeaders });
  }

  try {
    const supabaseUrl = Deno.env.get('SUPABASE_URL')!;
    const supabaseServiceKey = Deno.env.get('SUPABASE_SERVICE_ROLE_KEY')!;
    const supabase = createClient(supabaseUrl, supabaseServiceKey);

    const user = await getUserFromAuth(req, supabaseUrl, supabaseServiceKey);
    if (!user) {
      return jsonError('Authentication required', 401);
    }

    const body = await req.json().catch(() => ({}));

    const { data: currentProfile, error: loadError } = await supabase
      .from('profiles')
      .select('*')
      .eq('user_id', user.id)
      .single();

    if (loadError || !currentProfile) {
      console.error('Profile load error:', loadError);
      return jsonError('Profile not found', 404);
    }

    const updates: Record<string, unknown> = {};

    // ---- display_name ----
    if (body.display_name !== undefined) {
      const dn = String(body.display_name).trim();
      if (dn.length > 30) {
        return jsonError('Display name must be 30 characters or fewer', 400);
      }
      updates.display_name = dn;
    }

    // ---- bio ----
    if (body.bio !== undefined) {
      const bio = String(body.bio).trim();
      if (bio.length > 280) {
        return jsonError('Bio must be 280 characters or fewer', 400);
      }
      updates.bio = bio;
    }

    // ---- handle ----
    if (body.handle !== undefined) {
      const newHandle = String(body.handle).trim().toLowerCase();

      if (!HANDLE_REGEX.test(newHandle)) {
        return jsonError(
          'Handle must be 3-30 characters: lowercase letters, digits, underscore or dash, with no leading dash or underscore.',
          400,
        );
      }

      const currentHandle = currentProfile.handle?.toLowerCase() ?? null;
      const isFirstClaim = currentHandle === null;
      const isChange = !isFirstClaim && currentHandle !== newHandle;

      if (isChange && currentProfile.handle_changed_at) {
        const lastChanged = new Date(currentProfile.handle_changed_at).getTime();
        const cooldownExpires = lastChanged + HANDLE_COOLDOWN_MS;
        if (cooldownExpires > Date.now()) {
          const daysLeft = Math.ceil((cooldownExpires - Date.now()) / DAY_MS);
          return jsonError(
            `Handle can only be changed once every ${HANDLE_COOLDOWN_DAYS} days. Try again in ${daysLeft} day${daysLeft === 1 ? '' : 's'}.`,
            429,
          );
        }
      }

      if (isFirstClaim || isChange) {
        // Reserved-handle check (CITEXT column → case-insensitive comparison)
        const { data: reserved } = await supabase
          .from('reserved_handles')
          .select('handle')
          .eq('handle', newHandle)
          .maybeSingle();

        if (reserved) {
          return jsonError('Sorry, that handle is reserved.', 400);
        }

        updates.handle = newHandle;
        updates.handle_changed_at = new Date().toISOString();
      }
      // else: same handle as current — no-op, don't touch handle_changed_at
    }

    // ---- Refresh Discord-derived fields from JWT (only when present) ----
    // Non-Discord providers (e.g. SimpleLogin) don't populate these — leaving
    // them alone preserves any prior Discord identity rather than clobbering it.
    const meta = (user.user_metadata ?? {}) as Record<string, unknown>;
    if (typeof meta.full_name === 'string') {
      updates.discord_username = meta.full_name;
    }
    // F07: `user_metadata` is writable by its own user, so this string is not
    // Discord's — it is whatever the account holder last PUT. Checked against
    // the one host an avatar has ever legitimately come from before it reaches
    // a column four render sites put straight into an `<img src>` for
    // strangers. A refusal stores NULL, which is the placeholder those sites
    // already draw for an account with no Discord avatar.
    if (typeof meta.avatar_url === 'string') {
      updates.avatar_url = storableAvatarUrl(meta.avatar_url);
    }

    if (Object.keys(updates).length === 0) {
      return jsonResponse({ profile: currentProfile }, 200);
    }

    const { data: updated, error: updateError } = await supabase
      .from('profiles')
      .update(updates)
      .eq('user_id', user.id)
      .select()
      .single();

    if (updateError) {
      if (updateError.code === '23505') {
        return jsonError('Sorry, that handle is already in use.', 409);
      }
      console.error('Update error:', updateError);
      return jsonError('Failed to update profile', 500);
    }

    return jsonResponse({ profile: updated }, 200);
  } catch (e) {
    console.error('Unexpected error:', e);
    return jsonError('Internal server error', 500);
  }
});
