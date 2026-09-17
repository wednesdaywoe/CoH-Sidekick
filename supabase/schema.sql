-- ============================================
-- Sidekick Shared Builds — Supabase Schema
-- Run this in the Supabase SQL editor after creating your project
-- ============================================

-- Shared builds table
CREATE TABLE shared_builds (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  description TEXT DEFAULT '',
  archetype TEXT NOT NULL,
  archetype_name TEXT NOT NULL,
  primary_set TEXT NOT NULL,
  primary_name TEXT NOT NULL,
  secondary_set TEXT NOT NULL,
  secondary_name TEXT NOT NULL,
  level INTEGER NOT NULL DEFAULT 50,
  author_name TEXT DEFAULT '',
  server TEXT DEFAULT '',
  tags TEXT[] DEFAULT '{}',
  build_json JSONB NOT NULL,
  created_at TIMESTAMPTZ DEFAULT now(),
  updated_at TIMESTAMPTZ DEFAULT now(),
  views INTEGER DEFAULT 0,
  owner_token_hash TEXT,
  user_id UUID REFERENCES auth.users(id),
  -- 'private': owner only. 'unlisted': readable by exact id, never listed.
  -- 'public': readable by exact id AND listed in search/browse.
  visibility TEXT NOT NULL DEFAULT 'public'
    CHECK (visibility IN ('private', 'unlisted', 'public')),
  -- Object path in the `build-previews` Storage bucket for this build's social
  -- share-preview image (e.g. 'previews/<id>.png'), or NULL if none has been
  -- rendered yet. Written only by the share-build edge function (service
  -- role) — never by a direct client write. See streams/BUILD_PREVIEW_IMAGE_PLAN.md.
  preview_image_path TEXT,
  -- The CURRENT_PREVIEW_TEMPLATE_VERSION the image at preview_image_path was
  -- rendered under, or NULL alongside a NULL preview_image_path (never
  -- rendered). Lets a stale image (rendered under an older visual template)
  -- be told apart from a current one without re-deriving anything from the
  -- image itself. See streams/BUILD_PREVIEW_BACKFILL_PLAN.md (PREVBF1).
  preview_template_version INTEGER
);

-- Indexes for search and filtering
CREATE INDEX idx_shared_builds_archetype ON shared_builds(archetype);
CREATE INDEX idx_shared_builds_primary ON shared_builds(primary_set);
CREATE INDEX idx_shared_builds_secondary ON shared_builds(secondary_set);
CREATE INDEX idx_shared_builds_created ON shared_builds(created_at DESC);
CREATE INDEX idx_shared_builds_views ON shared_builds(views DESC);
CREATE INDEX idx_shared_builds_user_id ON shared_builds(user_id);
CREATE INDEX idx_shared_builds_visibility ON shared_builds(visibility) WHERE visibility = 'public';
CREATE INDEX idx_shared_builds_search ON shared_builds
  USING GIN (to_tsvector('english', name || ' ' || coalesce(description, '') || ' ' || coalesce(author_name, '')));
-- The tag filter asks `tags=cs.{"Perma Hasten"}` — array containment, which only an inverted
-- index can answer without reading every row.
CREATE INDEX idx_shared_builds_tags ON shared_builds USING GIN (tags);

-- Row Level Security
ALTER TABLE shared_builds ENABLE ROW LEVEL SECURITY;

-- Anon/public role: only public builds. Unlisted rows get NO grant here —
-- they are deliberately not bulk-readable via the anon key. Non-owner reads
-- of an unlisted (or private-if-owner) build go through the get-build edge
-- function (service role, point lookup by exact id only, never a listing).
CREATE POLICY "Public read" ON shared_builds
  FOR SELECT USING (visibility = 'public');

-- Authenticated users: their own builds at any visibility, plus all public
-- builds (via the "Public read" policy above — permissive policies OR).
CREATE POLICY "Owner read own" ON shared_builds
  FOR SELECT TO authenticated
  USING (user_id = auth.uid());

-- No INSERT/UPDATE/DELETE policies for anon role.
-- The edge functions use the service role key, which bypasses RLS.

-- ============================================
-- Rate limiting table
-- ============================================

CREATE TABLE rate_limits (
  id BIGSERIAL PRIMARY KEY,
  ip TEXT NOT NULL,
  action TEXT NOT NULL,
  created_at TIMESTAMPTZ DEFAULT now()
);

CREATE INDEX idx_rate_limits_lookup ON rate_limits(ip, action, created_at);

-- RLS enabled with no policies = only service role can access
ALTER TABLE rate_limits ENABLE ROW LEVEL SECURITY;

-- Auto-cleanup: delete rate limit entries older than 2 hours
-- (Run as a cron job via pg_cron or Supabase scheduled function)

-- ============================================
-- View counter RPC function
-- ============================================

CREATE OR REPLACE FUNCTION increment_views(build_id TEXT)
RETURNS void AS $$
BEGIN
  UPDATE shared_builds SET views = views + 1 WHERE id = build_id;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER SET search_path = public;

-- ============================================
-- Migration: Owner token support (run on existing databases)
-- ============================================
-- ALTER TABLE shared_builds ADD COLUMN IF NOT EXISTS owner_token_hash TEXT;
-- ALTER TABLE shared_builds ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ DEFAULT now();

-- ============================================
-- Migration: Discord OAuth support (run on existing databases)
-- ============================================
-- ALTER TABLE shared_builds ADD COLUMN IF NOT EXISTS user_id UUID REFERENCES auth.users(id);
-- CREATE INDEX IF NOT EXISTS idx_shared_builds_user_id ON shared_builds(user_id);

-- ============================================
-- Migration: Personal Vault support (run on existing databases)
-- ============================================
-- 1. Add the is_public column (defaults TRUE — all existing builds remain public)
-- ALTER TABLE shared_builds ADD COLUMN IF NOT EXISTS is_public BOOLEAN NOT NULL DEFAULT TRUE;
--
-- 2. Index for the filtered public browse query
-- CREATE INDEX IF NOT EXISTS idx_shared_builds_is_public ON shared_builds(is_public) WHERE is_public = TRUE;
--
-- 3. Replace the permissive read policy with visibility-aware policies
-- DROP POLICY IF EXISTS "Public read" ON shared_builds;
--
-- Anon/public role: only public builds
-- CREATE POLICY "Public read" ON shared_builds
--   FOR SELECT
--   USING (is_public = TRUE);
--
-- Authenticated users: their own builds (public or private) + all public builds
-- CREATE POLICY "Owner read private" ON shared_builds
--   FOR SELECT
--   TO authenticated
--   USING (is_public = TRUE OR user_id = auth.uid());

-- ============================================
-- Migration: Unlisted visibility (run on existing databases)
-- ============================================
-- Replaces the is_public boolean with a 3-state visibility enum so an
-- "unlisted" build can be readable by exact id (via the get-build edge
-- function) without being bulk-listable or appearing in search/browse.
--
-- 1. Add visibility, backfilled conservatively — existing is_public=false
--    rows become 'private', never 'unlisted', so no row gains
--    stranger-readable access it didn't already have.
-- ALTER TABLE shared_builds ADD COLUMN IF NOT EXISTS visibility TEXT
--   CHECK (visibility IN ('private', 'unlisted', 'public'));
-- UPDATE shared_builds SET visibility = CASE WHEN is_public THEN 'public' ELSE 'private' END
--   WHERE visibility IS NULL;
-- ALTER TABLE shared_builds ALTER COLUMN visibility SET NOT NULL;
-- ALTER TABLE shared_builds ALTER COLUMN visibility SET DEFAULT 'public';
--
-- 2. Drop and recreate shared_builds_with_author — a `b.*`-view's column
--    list is frozen at creation, so dropping is_public needs a drop+recreate,
--    not CREATE OR REPLACE.
-- DROP VIEW IF EXISTS shared_builds_with_author;
-- CREATE VIEW shared_builds_with_author
-- WITH (security_invoker = on) AS
-- SELECT b.*,
--        p.handle       AS author_handle,
--        p.display_name AS author_display_name,
--        p.avatar_url   AS author_avatar_url
-- FROM shared_builds b
-- LEFT JOIN profiles p ON p.user_id = b.user_id;
--
-- 3. Replace the RLS policies.
-- DROP POLICY IF EXISTS "Public read" ON shared_builds;
-- DROP POLICY IF EXISTS "Owner read private" ON shared_builds;
-- CREATE POLICY "Public read" ON shared_builds
--   FOR SELECT USING (visibility = 'public');
-- CREATE POLICY "Owner read own" ON shared_builds
--   FOR SELECT TO authenticated
--   USING (user_id = auth.uid());
--
-- 4. Index for the public browse query, then drop is_public and its old index.
-- CREATE INDEX IF NOT EXISTS idx_shared_builds_visibility ON shared_builds(visibility) WHERE visibility = 'public';
-- DROP INDEX IF EXISTS idx_shared_builds_is_public;
-- ALTER TABLE shared_builds DROP COLUMN is_public;
--
-- 5. search_authors below must be re-applied (CREATE OR REPLACE FUNCTION)
--    after this migration — its build_count now filters on visibility.

-- ============================================
-- Migration: Auction house price cache (run on existing databases)
-- ============================================
-- Caches average/min/max prices fetched from the HC auction API.
-- The auction-prices edge function is the only writer (service role).
-- Anon role can read prices (they're not sensitive).
--
-- CREATE TABLE auction_prices (
--   raw_identifier TEXT PRIMARY KEY,
--   avg_price BIGINT,
--   min_price BIGINT,
--   max_price BIGINT,
--   sample_count INTEGER,
--   last_sale_at TIMESTAMPTZ,
--   fetched_at TIMESTAMPTZ NOT NULL DEFAULT now(),
--   not_found BOOLEAN NOT NULL DEFAULT FALSE
-- );
--
-- CREATE INDEX idx_auction_prices_fetched ON auction_prices(fetched_at);
--
-- ALTER TABLE auction_prices ENABLE ROW LEVEL SECURITY;
--
-- CREATE POLICY "Public read prices" ON auction_prices
--   FOR SELECT USING (TRUE);
-- -- No INSERT/UPDATE policies: only service role (edge function) writes.

-- ============================================
-- Migration: Profiles + author handles (Phase 1 — APPLIED 2026-05-01)
-- ============================================
-- Adds a profiles table so users can pick a public handle (URL slug) and a
-- display name independent of their Discord identity. Per-build author_name
-- on shared_builds is preserved as-is (option 2: per-build name overrides).
--
-- Field-name mapping (verified against live auth.users rows, 2026-05-01):
--   Discord (iss = discord.com/api):
--     provider_id                  Discord snowflake (immutable)
--     full_name                    Discord username, e.g. "savant01"
--     name                         Username + legacy '#0' suffix, e.g. "savant01#0"
--     custom_claims.global_name    Discord display name, e.g. "Savant"  (NESTED — not top-level;
--                                  empty string "" when user hasn't set one)
--     avatar_url                   CDN URL
--   SimpleLogin (iss = app.simplelogin.io) and other providers:
--     none of the Discord fields are present; only email/sub
--
-- The COALESCE chain for display_name uses NULLIF(..., '') so empty Discord
-- global_names fall through to full_name, and a final email-prefix fallback
-- handles non-Discord providers.

CREATE EXTENSION IF NOT EXISTS citext;
CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- Reserved handle list. Stub seeded with system-level terms; CoH-specific
-- reserved words can be added later via plain INSERTs without schema changes.
-- Enforcement happens in the update-profile edge function (CHECK constraints
-- can't reference other tables).
CREATE TABLE reserved_handles (
  handle CITEXT PRIMARY KEY,
  reason TEXT DEFAULT ''
);

INSERT INTO reserved_handles (handle, reason) VALUES
  ('admin','system'), ('api','system'), ('auth','system'),
  ('author','route'), ('builds','route'), ('build','route'),
  ('login','route'), ('logout','route'), ('me','route'),
  ('settings','route'), ('profile','route'), ('public','route'),
  ('signup','route'), ('signin','route'), ('support','system'),
  ('help','system'), ('new','route'), ('edit','route'),
  ('delete','route'), ('undefined','sentinel'), ('null','sentinel'),
  ('anonymous','sentinel'), ('system','system');
-- TODO: add CoH-specific reserved handles here as we identify them.

CREATE TABLE profiles (
  user_id           UUID PRIMARY KEY REFERENCES auth.users(id) ON DELETE CASCADE,
  handle            CITEXT UNIQUE,                -- public URL slug, nullable until claimed
  display_name      TEXT NOT NULL DEFAULT '',     -- shown on cards by default
  discord_id        TEXT,                         -- immutable Discord snowflake
  discord_username  TEXT,                         -- cached username, for "verified" badge
  avatar_url        TEXT,
  bio               TEXT NOT NULL DEFAULT '',
  handle_changed_at TIMESTAMPTZ,                  -- gates 30-day cooldown
  created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),

  CONSTRAINT handle_format CHECK (
    handle IS NULL OR handle::text ~ '^[a-z0-9][a-z0-9_-]{2,29}$'
  ),
  CONSTRAINT display_name_length CHECK (char_length(display_name) <= 30),
  CONSTRAINT bio_length CHECK (char_length(bio) <= 280)
);

CREATE INDEX idx_profiles_display_name_trgm ON profiles USING GIN (display_name gin_trgm_ops);
CREATE INDEX idx_profiles_handle_trgm       ON profiles USING GIN ((handle::text) gin_trgm_ops);

ALTER TABLE profiles ENABLE ROW LEVEL SECURITY;
ALTER TABLE reserved_handles ENABLE ROW LEVEL SECURITY;

CREATE POLICY "Public read profiles" ON profiles FOR SELECT USING (TRUE);
-- reserved_handles has RLS enabled with no policies: only service role reads/writes.
-- Writes to profiles go through the update-profile edge function (service role).

-- Auto-touch updated_at
CREATE OR REPLACE FUNCTION touch_profile_updated_at()
RETURNS trigger AS $$
BEGIN NEW.updated_at = now(); RETURN NEW; END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER profiles_touch_updated
  BEFORE UPDATE ON profiles
  FOR EACH ROW EXECUTE FUNCTION touch_profile_updated_at();

-- Seed a profile on first sign-up. Handle stays NULL until the user picks one.
-- NULLIF(..., '') is required because Discord stores an empty string for
-- global_name when the user hasn't set one — COALESCE only skips NULLs.
CREATE OR REPLACE FUNCTION seed_profile_on_signup()
RETURNS trigger AS $$
BEGIN
  INSERT INTO profiles (user_id, display_name, discord_id, discord_username, avatar_url)
  VALUES (
    NEW.id,
    COALESCE(
      NULLIF(NEW.raw_user_meta_data->'custom_claims'->>'global_name', ''),
      NULLIF(NEW.raw_user_meta_data->>'full_name', ''),
      NULLIF(NEW.raw_user_meta_data->>'name', ''),
      NULLIF(split_part(NEW.email, '@', 1), ''),
      ''
    ),
    NEW.raw_user_meta_data->>'provider_id',
    NEW.raw_user_meta_data->>'full_name',
    NEW.raw_user_meta_data->>'avatar_url'
  )
  ON CONFLICT (user_id) DO NOTHING;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER SET search_path = public;

CREATE TRIGGER on_auth_user_created
  AFTER INSERT ON auth.users
  FOR EACH ROW EXECUTE FUNCTION seed_profile_on_signup();

-- Backfill profiles for users who already exist
INSERT INTO profiles (user_id, display_name, discord_id, discord_username, avatar_url)
SELECT
  id,
  COALESCE(
    NULLIF(raw_user_meta_data->'custom_claims'->>'global_name', ''),
    NULLIF(raw_user_meta_data->>'full_name', ''),
    NULLIF(raw_user_meta_data->>'name', ''),
    NULLIF(split_part(email, '@', 1), ''),
    ''
  ),
  raw_user_meta_data->>'provider_id',
  raw_user_meta_data->>'full_name',
  raw_user_meta_data->>'avatar_url'
FROM auth.users
ON CONFLICT (user_id) DO NOTHING;

-- Joined view for build queries — replaces direct selects from shared_builds
-- in the search/list paths so cards can render handle + verified avatar.
-- security_invoker = on is REQUIRED so the view respects the caller's RLS
-- context instead of running with the view-creator's privileges. Without it,
-- anon role would see private builds through this view.
CREATE VIEW shared_builds_with_author
WITH (security_invoker = on) AS
SELECT b.*,
       p.handle       AS author_handle,
       p.display_name AS author_display_name,
       p.avatar_url   AS author_avatar_url
FROM shared_builds b
LEFT JOIN profiles p ON p.user_id = b.user_id;

-- Author-search RPC for the autocomplete dropdown.
-- Combines ILIKE substring/prefix matching (autocomplete-friendly for short
-- queries) with trigram similarity (catches typos). Pure trigram matching
-- alone failed for short queries like "wed" vs "wednesdaywoe" because the
-- similarity score (~0.23) fell below pg_trgm's default 0.3 threshold.
CREATE OR REPLACE FUNCTION search_authors(q TEXT, lim INT DEFAULT 10)
RETURNS TABLE (
  user_id      UUID,
  handle       CITEXT,
  display_name TEXT,
  avatar_url   TEXT,
  build_count  BIGINT,
  sim          REAL
)
LANGUAGE sql STABLE AS $$
  SELECT p.user_id, p.handle, p.display_name, p.avatar_url,
         COUNT(b.id) FILTER (WHERE b.visibility = 'public') AS build_count,
         GREATEST(
           -- Prefix match: highest priority
           CASE WHEN p.display_name ILIKE q || '%'        THEN 1.0 ELSE 0 END,
           CASE WHEN p.handle::text ILIKE q || '%'        THEN 1.0 ELSE 0 END,
           -- Substring match
           CASE WHEN p.display_name ILIKE '%' || q || '%' THEN 0.8 ELSE 0 END,
           CASE WHEN p.handle::text ILIKE '%' || q || '%' THEN 0.8 ELSE 0 END,
           -- Trigram fuzzy (catches typos)
           similarity(p.display_name, q),
           COALESCE(similarity(p.handle::text, q), 0)
         ) AS sim
  FROM profiles p
  LEFT JOIN shared_builds b ON b.user_id = p.user_id
  WHERE p.display_name ILIKE '%' || q || '%'
     OR p.handle::text   ILIKE '%' || q || '%'
     OR p.display_name % q
     OR p.handle::text % q
  GROUP BY p.user_id, p.handle, p.display_name, p.avatar_url
  ORDER BY sim DESC, build_count DESC
  LIMIT lim;
$$;

-- Resolver for /author/@handle URLs
CREATE OR REPLACE FUNCTION resolve_author(h TEXT)
RETURNS TABLE (
  user_id      UUID,
  handle       CITEXT,
  display_name TEXT,
  avatar_url   TEXT,
  bio          TEXT
)
LANGUAGE sql STABLE AS $$
  SELECT user_id, handle, display_name, avatar_url, bio
  FROM profiles
  WHERE handle = h::citext
$$;

-- ============================================
-- Favorites — account-synced starred builds (APPLIED 2026-07-09)
-- ============================================
-- Before this, favourites lived only in localStorage ('coh-planner-favorites'),
-- so they never followed the user across devices or survived a fresh browser.
-- This table stores one row per (user, favourited build). Unlike shared_builds,
-- writes go DIRECTLY from the client via RLS (no edge function) — the data is
-- low-risk and each policy is scoped to auth.uid(), so a user can only touch
-- their own rows. build_id FKs shared_builds so deleting a build auto-clears
-- any favourite pointing at it.
CREATE TABLE favorites (
  user_id    UUID NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
  build_id   TEXT NOT NULL REFERENCES shared_builds(id) ON DELETE CASCADE,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (user_id, build_id)
);

CREATE INDEX idx_favorites_user ON favorites(user_id);

ALTER TABLE favorites ENABLE ROW LEVEL SECURITY;

-- Each authenticated user may read/insert/delete ONLY their own favourite rows.
CREATE POLICY "own favorites read" ON favorites
  FOR SELECT TO authenticated USING (user_id = auth.uid());
CREATE POLICY "own favorites insert" ON favorites
  FOR INSERT TO authenticated WITH CHECK (user_id = auth.uid());
CREATE POLICY "own favorites delete" ON favorites
  FOR DELETE TO authenticated USING (user_id = auth.uid());

-- ============================================
-- Migration: Build share-preview images (run on existing databases)
-- ============================================
-- Social-preview PNG for a shared build's link (Discord/Slack unfurl etc.).
-- Rendered client-side at share time (the browser already has the computed
-- stats), then uploaded by the share-build edge function using the service
-- role key — never a direct client write, so no INSERT policy on
-- storage.objects is needed here (same reasoning as shared_builds itself:
-- "The edge functions use the service role key, which bypasses RLS").
-- See streams/BUILD_PREVIEW_IMAGE_PLAN.md (EMBED1).

-- 1. Where the build stores which preview image belongs to it.
ALTER TABLE shared_builds ADD COLUMN IF NOT EXISTS preview_image_path TEXT;

-- 2. The bucket the image lives in. `public = true` means Storage serves
--    GET requests unauthenticated with no RLS policy needed — that's the
--    entire read path a Discord/Slack crawler needs. Object paths are keyed
--    by the build's own id (an unguessable token), so this carries the same
--    "readable by exact id" security property 'unlisted' visibility already
--    relies on elsewhere in this schema — not a new exposure.
INSERT INTO storage.buckets (id, name, public)
VALUES ('build-previews', 'build-previews', true)
ON CONFLICT (id) DO NOTHING;

-- 3. shared_builds_with_author is `SELECT b.*, ...` — Postgres freezes that
--    expansion at (re)creation time, so the view will NOT pick up the new
--    column until it is rebuilt. CREATE OR REPLACE only allows appending a
--    column at the very END of the view's output — `preview_image_path`
--    lands via `b.*` BEFORE the trailing author_handle/display_name/
--    avatar_url columns, which counts as reordering, not appending. (Wrongly
--    assumed CREATE OR REPLACE would work here — it doesn't; DROP+CREATE
--    it is, same as the "Unlisted visibility" migration above.) Dropping and
--    recreating is safe: this project has no explicit GRANTs on the view
--    (grep confirms none exist anywhere in this file) — access comes from
--    Supabase's project-wide default privileges, which apply to newly
--    created objects the same as existing ones.
DROP VIEW IF EXISTS shared_builds_with_author;
CREATE VIEW shared_builds_with_author
WITH (security_invoker = on) AS
SELECT b.*,
       p.handle       AS author_handle,
       p.display_name AS author_display_name,
       p.avatar_url   AS author_avatar_url
FROM shared_builds b
LEFT JOIN profiles p ON p.user_id = b.user_id;

-- ============================================
-- Migration: Preview image backfill/refresh (run on existing databases)
-- ============================================
-- Lets a build whose preview image was rendered under an older visual
-- template be told apart from one that's current, so both a never-generated
-- and a stale image can be regenerated automatically. Every pre-existing row
-- lands NULL here, same as preview_image_path — NULL already means
-- "generate regardless of version," which is correct for a row this column
-- predates. See streams/BUILD_PREVIEW_BACKFILL_PLAN.md (PREVBF1).
ALTER TABLE shared_builds ADD COLUMN IF NOT EXISTS preview_template_version INTEGER;

-- shared_builds_with_author freezes its `b.*` expansion at creation, same as
-- the EMBED1 migration above — rebuild it the same way (DROP+CREATE, no
-- GRANTs on the view to preserve).
DROP VIEW IF EXISTS shared_builds_with_author;
CREATE VIEW shared_builds_with_author
WITH (security_invoker = on) AS
SELECT b.*,
       p.handle       AS author_handle,
       p.display_name AS author_display_name,
       p.avatar_url   AS author_avatar_url
FROM shared_builds b
LEFT JOIN profiles p ON p.user_id = b.user_id;

-- ============================================
-- Admin: Assign an owner token to a legacy build
-- ============================================
-- 1. Pick a token (any string, e.g. a UUID):
--    SELECT gen_random_uuid();  -- generates something like 'a1b2c3d4-...'
--
-- 2. Set the hash on the build:
--    UPDATE shared_builds
--    SET owner_token_hash = encode(sha256(convert_to('YOUR-TOKEN-HERE', 'UTF8')), 'hex')
--    WHERE id = 'BUILD-ID-HERE';
--
-- 3. Use that token in the app's "Reclaim" button on the build detail page.

-- ============================================
-- Migration: Tag filtering (run on existing databases)
-- ============================================
-- The build browser's tag filter sends PostgREST array containment
-- (`tags=cs.{"Perma Hasten","Budget"}`). Without an inverted index that is a
-- sequential scan of shared_builds on every filtered browse.
--
-- CREATE INDEX IF NOT EXISTS idx_shared_builds_tags ON shared_builds USING GIN (tags);
--
-- No column change: `tags TEXT[]` already exists and the curated vocabulary is
-- client-side (crates/app/src/cloud/tag_vocab.rs). The server deliberately does
-- NOT validate tags against it — a row tagged before the vocabulary existed, or
-- by an older client, stays readable rather than becoming invalid.
