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

-- Auto-cleanup: delete rate limit entries older than 2 hours.
--
-- This was two lines of comment promising a cron job that did not exist —
-- SECURITY_AUDIT.md F70. Nothing read rate_limits except the sliding-window
-- COUNT in share-build, which filters on `created_at >= windowStart`, so an
-- aged-out row was never wrong, only permanent: the table grew by one row per
-- share or vault save, for the life of the project, and fastest under exactly
-- the flood the rate limit exists to absorb.
--
-- Idempotent, so this block is also the migration for an existing database —
-- run it as-is. pg_cron >= 1.4 makes cron.schedule an upsert on the job name,
-- and Supabase ships 1.6, so re-running replaces the job rather than stacking
-- a second copy of it.
--
-- If CREATE EXTENSION is refused in the SQL editor, enable pg_cron from the
-- dashboard instead (Database -> Extensions) and run the SELECT alone.
CREATE EXTENSION IF NOT EXISTS pg_cron;

-- Hourly at :17 rather than on the hour, to sit off the top-of-hour pile.
--
-- Two hours, against a RATE_WINDOW_HOURS of 1 in share-build: one full window
-- of headroom, so a row is only ever deleted well after the last query that
-- could have counted it. Deleting at exactly the window would race the count.
--
-- The job runs as the role that scheduled it (postgres, the table's owner),
-- and an owner is not subject to its own RLS unless the table is set FORCE ROW
-- LEVEL SECURITY, which this one is not — so "RLS enabled, no policies" does
-- not block the purge.
--
-- No index for this predicate on purpose. idx_rate_limits_lookup leads with
-- (ip, action) and cannot serve a created_at-only scan, but once this job is
-- running the table holds at most two hours of traffic, and under the flood
-- that makes it big the DELETE is removing most of what it reads — which is a
-- sequential scan's best case, not an index's.
SELECT cron.schedule(
  'purge-rate-limits',
  '17 * * * *',
  $$DELETE FROM public.rate_limits WHERE created_at < now() - INTERVAL '2 hours'$$
);

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
-- Field-name mapping (the SHAPE was verified against live auth.users rows on
-- 2026-05-01; the example values below are synthetic and must stay that way --
-- the three that stood here were one real account's Discord identity,
-- transcribed off the row that verified the mapping, in the same file whose
-- F34 block at the end exists to stop publishing that exact column):
--   Discord (iss = discord.com/api):
--     provider_id                  Discord snowflake (immutable)
--     full_name                    Discord username, e.g. "tigereyes"
--     name                         Username + legacy '#0' suffix, e.g. "tigereyes#0"
--     custom_claims.global_name    Discord display name, e.g. "Tiger Eyes"  (NESTED — not top-level;
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

-- The one host an avatar may be served from -- SECURITY_AUDIT.md F07, and the
-- SQL half of `supabase/functions/_shared/avatar-url.ts`.
--
-- `update-profile` is the live writer and applies the TypeScript rule; this
-- covers the OTHER writer, the signup trigger below, whose input is the
-- provider's claim at first insert. That is not attacker-controlled today --
-- both providers are OAuth and neither lets the account holder choose what
-- arrives -- but which providers are enabled lives in a hosted setting no file
-- in this repository records, which is the F45 lesson, and the column is the
-- one place a rule covers every writer at once.
--
-- Deliberately STRICTER than the TypeScript twin rather than equal to it: no
-- port, and the host matched case-sensitively. Both are things Discord never
-- sends, and a rule with two implementations should diverge toward refusal.
-- A refused url becomes NULL, which is the same "no avatar" placeholder every
-- render site already draws for an account without Discord.
CREATE OR REPLACE FUNCTION storable_avatar_url(raw TEXT)
RETURNS TEXT AS $$
  SELECT CASE
    WHEN raw IS NULL THEN NULL
    WHEN length(raw) > 512 THEN NULL
    -- A control character or a space is never part of a real url, and is how
    -- one url is made to read as two.
    WHEN raw ~ '[[:space:][:cntrl:]]' THEN NULL
    -- The separator after the host must be present, so that
    -- `https://cdn.discordapp.com@evil.example/` and
    -- `https://cdn.discordapp.com.evil.example/` both fall through: in each the
    -- character after `.com` is not one that ends an authority.
    WHEN raw LIKE 'https://cdn.discordapp.com/%' THEN raw
    WHEN raw LIKE 'https://cdn.discordapp.com?%' THEN raw
    WHEN raw LIKE 'https://cdn.discordapp.com#%' THEN raw
    WHEN raw = 'https://cdn.discordapp.com' THEN raw
    ELSE NULL
  END;
$$ LANGUAGE sql IMMUTABLE;

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
    storable_avatar_url(NEW.raw_user_meta_data->>'avatar_url')
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
  storable_avatar_url(raw_user_meta_data->>'avatar_url')
FROM auth.users
ON CONFLICT (user_id) DO NOTHING;

-- Joined view for build queries — replaces direct selects from shared_builds
-- in the search/list paths so cards can render handle + verified avatar.
-- security_invoker = on is REQUIRED so the view respects the caller's RLS
-- context instead of running with the view-creator's privileges. Without it,
-- anon role would see private builds through this view.
--
-- The projection NAMES its columns. It was `b.*` until F73 (see the migration
-- at the end of this file): `b.*` carried owner_token_hash, the sha256 of the
-- credential that owns an anonymous build, to every anonymous caller. A named
-- list cannot pick up a column nobody decided to publish — including the next
-- one somebody adds to the table.
CREATE VIEW shared_builds_with_author
WITH (security_invoker = on) AS
SELECT b.id,
       b.name,
       b.description,
       b.archetype,
       b.archetype_name,
       b.primary_set,
       b.primary_name,
       b.secondary_set,
       b.secondary_name,
       b.level,
       b.author_name,
       b.server,
       b.tags,
       b.build_json,
       b.created_at,
       b.updated_at,
       b.views,
       b.user_id,
       b.visibility,
       b.preview_image_path,
       b.preview_template_version,
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
--
-- **DO NOT COPY THIS PROJECTION.** `b.*` is what F73 was — it published
-- owner_token_hash to every anonymous caller. This block is left as the SQL
-- that actually ran, and the F73 migration at the end of this file supersedes
-- it; the shape to copy when adding a column is the one there.
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
--
-- **DO NOT COPY THIS PROJECTION** — see the note on the EMBED1 block above.
-- Superseded by the F73 migration at the end of this file.
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

-- ============================================
-- Migration: Stop publishing the ownership credential's hash (F73)
-- ============================================
-- SECURITY_AUDIT.md F73. owner_token_hash is the sha256 of the owner_token
-- that share-build hands an anonymous sharer, and that claim-builds,
-- delete-build and share-build compare against to authorise an update or a
-- delete. It was readable by anyone holding the public anon key.
--
-- **Measured against the deployed project, 2026-09-18, with the anon key:**
--
--   GET /rest/v1/shared_builds?select=id,owner_token_hash        -> hashes
--   GET /rest/v1/shared_builds?select=id&owner_token_hash=not.is.null
--                                                    -> 1575 of 2665 public rows
--   GET /rest/v1/shared_builds?select=id&owner_token_hash=like.0*        -> 97
--
-- The third line is the one that decides the shape of this fix. The column was
-- not merely readable, it was FILTERABLE — a prefix probe answers a question
-- about the credential rather than returning it.
--
-- **The fix the finding described was incomplete, and the middle query above
-- is why.** F73 called for "a projection on the view (or dropping the column
-- from it) plus a named select in get-build". Both of those are here, and both
-- together close nothing: every query above names `shared_builds`, the BASE
-- TABLE, not `shared_builds_with_author`. The view was only ever one of two
-- doors. PostgREST exposes the table directly, the "Public read" policy grants
-- anon every public row, and a table-level SELECT grant covers every column in
-- it — so narrowing the view would have left the census, the filter and the
-- hashes exactly where they were.
--
-- **Why Low, still.** The token is a `crypto.randomUUID`, so an unsalted
-- sha256 of it is not invertible and the hash buys no takeover. What it buys
-- is a free census of which builds are token-owned versus account-owned, and a
-- credential-derived value on a public wire that a future weaker token would
-- make fatal.
--
-- **Weighed and not taken: moving the column to its own table.** A
-- `build_owner_tokens(build_id, token_hash)` with RLS and no policies would
-- put the credential structurally out of reach instead of relying on a grant
-- staying correct, and would leave `select=*` working. It was rejected for
-- this pass on cost, not on merit: it is a data migration over 1,575 live rows
-- plus edits to three edge functions and the import script, against a finding
-- whose severity is entropy rather than design. If a weaker token is ever
-- issued, that migration is the fix and this one is not enough.

-- 1. The view. `b.*` freezes its expansion at creation, so this is a
--    DROP+CREATE for the same reason the two preview migrations above are —
--    and the projection is named this time, which also means the NEXT column
--    added to shared_builds is published only if someone decides to publish
--    it. See step 3 for what adding a column now costs.
DROP VIEW IF EXISTS shared_builds_with_author;
CREATE VIEW shared_builds_with_author
WITH (security_invoker = on) AS
SELECT b.id,
       b.name,
       b.description,
       b.archetype,
       b.archetype_name,
       b.primary_set,
       b.primary_name,
       b.secondary_set,
       b.secondary_name,
       b.level,
       b.author_name,
       b.server,
       b.tags,
       b.build_json,
       b.created_at,
       b.updated_at,
       b.views,
       b.user_id,
       b.visibility,
       b.preview_image_path,
       b.preview_template_version,
       p.handle       AS author_handle,
       p.display_name AS author_display_name,
       p.avatar_url   AS author_avatar_url
FROM shared_builds b
LEFT JOIN profiles p ON p.user_id = b.user_id;

-- 2. The base table, which is the door that actually mattered.
--
--    Postgres has no way to hide one column from a role that holds table-level
--    SELECT — a table grant covers every column, present and future. Getting
--    column granularity means revoking the table grant and re-granting the
--    columns by name. That is the whole of the mechanism, and the order
--    matters: REVOKE first, because a column-level GRANT is simply ignored
--    while the table-level one is still held.
--
--    This must stay in step with the view's projection above. The view is
--    `security_invoker = on`, so reading it checks the INVOKER's privileges on
--    these base columns — grant fewer than the view projects and every
--    anonymous browse starts failing, loudly and immediately.
REVOKE SELECT ON public.shared_builds FROM anon, authenticated;
GRANT SELECT (
  id,
  name,
  description,
  archetype,
  archetype_name,
  primary_set,
  primary_name,
  secondary_set,
  secondary_name,
  level,
  author_name,
  server,
  tags,
  build_json,
  created_at,
  updated_at,
  views,
  user_id,
  visibility,
  preview_image_path,
  preview_template_version
) ON public.shared_builds TO anon, authenticated;

--    service_role is deliberately NOT revoked. The edge functions authorise an
--    update or a delete by comparing owner_token_hash (share-build,
--    claim-builds, delete-build), and they run under the service role key,
--    which keeps its table grant and bypasses RLS. Same for the postgres owner
--    and the two admin scripts in scripts/, which use the service role key.
--
--    increment_views is SECURITY DEFINER and so unaffected. search_authors is
--    invoker-rights SQL and joins shared_builds, but reads only id, user_id
--    and visibility — all three granted above.

-- 3. **What this costs, and it is a real cost: adding a column to
--    shared_builds is now a three-step change, not one.**
--
--      a. ALTER TABLE shared_builds ADD COLUMN ...
--      b. add it to the view's projection (DROP+CREATE, as always)
--      c. add it to the GRANT SELECT list above
--
--    Skip (c) and the column is invisible to anon through the view — but not
--    silently: a security_invoker view whose projection names a column the
--    caller cannot read fails the whole read with "permission denied for
--    column", which is a loud failure and the right kind. Skip (b) and the
--    column simply never reaches a client, which is the quiet one to watch
--    for.
--
--    A column that SHOULD stay private now needs nothing done to it, which is
--    the point: the default flipped from published to withheld.

-- 4. **Behaviour change worth knowing before it surprises someone.** An
--    anonymous or logged-in `select=*` against the BASE table now fails —
--    PostgREST expands `*` to every column, owner_token_hash included, and
--    that column is no longer granted:
--
--      GET /rest/v1/shared_builds                 -> 403, permission denied
--      GET /rest/v1/shared_builds?select=id,name  -> fine
--
--    Nothing in either repo does this: the web client reads the view in all
--    four of its build queries (src/services/sharedBuilds.ts), the Rust client
--    reads the view through BUILDS_VIEW and names its columns
--    (crates/app/src/cloud/shared_builds.rs), and the two scripts that touch
--    the table directly hold the service role key. The failure is loud if
--    anything is missed, which is the outcome to want.

-- 5. Confirming it took. The SQL editor answers the first two; the third needs
--    the public anon key, because the thing being checked is what a stranger
--    can do and the editor is not a stranger.
--
--    a. No table-level SELECT left, and exactly 21 column grants, for each of
--       the two public roles — 42 rows, none of them owner_token_hash:
--
--         SELECT grantee, column_name
--         FROM information_schema.column_privileges
--         WHERE table_name = 'shared_builds' AND privilege_type = 'SELECT'
--           AND grantee IN ('anon', 'authenticated')
--         ORDER BY grantee, column_name;
--
--         SELECT grantee, privilege_type
--         FROM information_schema.table_privileges
--         WHERE table_name = 'shared_builds' AND grantee IN ('anon', 'authenticated');
--         -- SELECT must not appear for either role.
--
--    b. The view publishes 24 columns and none of them is the hash:
--
--         SELECT column_name FROM information_schema.columns
--         WHERE table_name = 'shared_builds_with_author' ORDER BY ordinal_position;
--
--    c. From a shell, with the anon key — the three probes that measured the
--       finding on 2026-09-18. All three must now fail rather than answer:
--
--         curl -sS -H "apikey: $ANON" -H "Authorization: Bearer $ANON" \
--           "$URL/rest/v1/shared_builds?select=id,owner_token_hash&limit=1"
--         curl -sS -H "apikey: $ANON" -H "Authorization: Bearer $ANON" \
--           "$URL/rest/v1/shared_builds?select=id&owner_token_hash=like.0*&limit=1"
--         curl -sS -H "apikey: $ANON" -H "Authorization: Bearer $ANON" \
--           "$URL/rest/v1/shared_builds_with_author?select=id,owner_token_hash&limit=1"
--         # each: 42501 / "permission denied for ..." or "column does not exist",
--         # where before they returned hashes, a filtered id list, and hashes.
--
--       And the two that must KEEP working, because a fix that breaks the
--       browse is not a fix:
--
--         curl -sS -H "apikey: $ANON" -H "Authorization: Bearer $ANON" \
--           "$URL/rest/v1/shared_builds_with_author?select=id,name,author_handle&visibility=eq.public&limit=3"
--         curl -sS -X POST -H "apikey: $ANON" -H "Authorization: Bearer $ANON" \
--           -H "Content-Type: application/json" -d '{"id":"<any public build id>"}' \
--           "$URL/functions/v1/get-build"
--         # the second must answer 24 keys, WITHOUT owner_token_hash — it was
--         # 25 with it. That one is the redeploy's check, not the SQL's.
--
--    The view is DROPped and recreated here with no GRANT restored after it,
--    which is safe for the reason the EMBED1 block above states and the two
--    prior migrations demonstrated by running: this project has no explicit
--    GRANTs on the view, and access comes from Supabase's project-wide default
--    privileges, which apply to newly created objects. If the browse 401s
--    after this migration, that assumption is what to check first.

-- ============================================
-- F34 — profiles is readable column by column, not whole (DEPLOYED 2026-09-18)
-- ============================================
-- SECURITY_AUDIT.md F34: "profiles table fully readable by anon, including
-- discord_id and discord_username". Measured on 2026-09-18 by
-- supabase/audit/q01-q04 and q14, against production:
--
--   q02: profiles.relacl is {postgres=arwdDxtm/postgres, anon=arwdDxtm/postgres,
--        authenticated=arwdDxtm/postgres, service_role=arwdDxtm/postgres} —
--        anon holds EVERY table privilege, and the only policy is
--        "Public read profiles [r] true".
--   q01: has_column_privilege('anon', ...) is true for all ten columns.
--   q03: 530 rows; 452 carry discord_id, discord_username and avatar_url;
--        452 distinct discord_ids; 405 of those usernames differ from the
--        display_name a card already shows, so the column is not a duplicate
--        of something public — it is 452 Discord identities, additional.
--        428 of them sit on profiles that never claimed a handle.
--   q14: shared_builds_with_author is security_invoker = on, so it runs as the
--        CALLER and a column REVOKE binds through it. Had it been owner-rights
--        this migration would have been a no-op on the surface that matters
--        most, which is why that query exists.
--
-- This is F73's mechanism, one table over, and the last untreated public table
-- on it. The comment in step 2 of the F73 block above is the whole explanation
-- and is not repeated here: a table-level SELECT grant covers every column,
-- present and future, so column granularity means REVOKE first and re-GRANT by
-- name, in that order.
--
-- **The list below is derived, not chosen.** It is the union of what the three
-- anon-reachable readers of this table need, and every one of them is
-- invoker-rights, so each needs the privilege in its own right:
--
--   shared_builds_with_author  p.handle, p.display_name, p.avatar_url,
--                              and p.user_id for the join predicate
--   search_authors             p.user_id, p.handle, p.display_name,
--                              p.avatar_url  (prosecdef = false, q04)
--   resolve_author             p.user_id, p.handle, p.display_name,
--                              p.avatar_url, p.bio  (prosecdef = false, q04)
--
--   union  = user_id, handle, display_name, avatar_url, bio
--   withheld = discord_id, discord_username, handle_changed_at,
--              created_at, updated_at
--
-- Nothing was traded away to reach that split: the five withheld columns are
-- read by no anon-reachable path. discord_id and discord_username are the
-- finding. handle_changed_at gates the 30-day cooldown and is a server
-- decision (update-profile, service role). created_at and updated_at are
-- account-activity timestamps that no public surface renders, and they are
-- withheld on the "default flipped from published to withheld" principle the
-- F73 block states rather than because a threat was named for them.
REVOKE SELECT ON public.profiles FROM anon;
GRANT SELECT (
  user_id,
  handle,
  display_name,
  avatar_url,
  bio
) ON public.profiles TO anon;

-- **`authenticated` is deliberately NOT revoked here, and that is a gap, not a
-- conclusion.** It holds the same full table grant (q02, above), so every
-- signed-in account can still read all 452 Discord identities — and sign-up is
-- free Discord OAuth, so this migration raises the cost of the harvest from
-- "hold the public anon key" to "hold an account". That is a real reduction
-- and it is not a closure.
--
-- It is left open because the authenticated list is NOT determined the way the
-- anon list above is, and guessing it would break the app: both clients read
-- their OWN profile with a wildcard —
--
--   src/services/profile.ts:38            .from('profiles').select('*')
--   crates/app/src/cloud/profile.rs:409   .select("profiles", "*", ...)
--
-- both reached only with a session (Header.tsx:889 and
-- ProfileSettingsPage.tsx:56 pass user.id; profile.rs:542 returns early
-- without one), so both run as `authenticated` and both would start failing
-- with "permission denied for table profiles" the moment that role is
-- revoked.
-- Closing this half therefore means naming the columns in two clients across
-- the repo boundary first, and deciding what a user may read of a profile that
-- is not theirs — a question this table cannot answer with a grant, because a
-- grant does not know whose row it is looking at. The honest shape of that fix
-- is probably RLS, or a `profiles_public` view, not a longer GRANT list.
--
-- Exit condition: when both clients select named columns, revoke
-- `authenticated` down to the same five plus whatever the own-profile surface
-- genuinely needs (discord_username for the verified badge and
-- handle_changed_at for the cooldown are the likely two), gated so it applies
-- only to auth.uid() = user_id.
--
-- service_role keeps its grant, for the reason F73's step 2 gives: update-profile
-- and share-build read and write this table under it, and seed_profile_on_signup
-- is a trigger running as the definer.
--
-- Not touched, and worth someone's attention: anon also holds INSERT, UPDATE
-- and DELETE on this table (q02's `arwdDxtm`). Nothing exploits that today
-- because RLS is enabled and the only policy is FOR SELECT, so a write finds
-- no permissive policy and is denied — but the grant is what would be left if
-- a write policy were ever added for some other reason. It is out of F34's
-- scope, which is SELECT.

-- The second door. Narrowing the table grant does nothing to an RPC that
-- selects from the table on the caller's behalf, and q04 measured what this one
-- hands out:
--
--   search_authors('',  1000000) -> 530 rows, the whole table
--   search_authors('a', 1000000) -> 328
--   of the 530, 323 have no public builds at all
--
-- `q = ''` degenerates to ILIKE '%%'. One anonymous call returns every profile
-- in the project, including the 500 that never claimed a handle and the 323
-- that have never shared anything — people who are not, by any action of their
-- own, publishing an author identity.
--
-- Both clients already honour a two-character minimum and ask for 8
-- (AUTHOR_SEARCH_MIN / AUTHOR_SEARCH_LIMIT in profile.rs:440,445;
-- `q.length < 2` and `limit = 8` in profile.ts:122,118). profile.rs:449 even
-- states the reason in its doc comment — "the RPC would answer, and the answer
-- would be most of the table". This moves that contract server-side, where it
-- binds on a caller that is not one of our clients. No real call changes: the
-- guard rejects only what both clients already refuse to send, and the clamp
-- sits at 25, well above the 8 either asks for.
--
-- **What this buys, stated exactly, because F73's row is on this page for
-- overstating its own fix.** It removes the single-call full-table dump. It
-- does NOT make enumeration impossible: two-character prefixes still walk the
-- table 25 rows at a time, and anyone willing to spend ~1,300 calls gets most
-- of it. Closing THAT means rate-limiting or requiring a session, neither of
-- which is a grant or a function body, and neither of which is in this change.
-- What the REVOKE above does close completely is the Discord identities — they
-- are on no path this function can reach, whatever it returns.
--
-- The guard is on btrim() so that '', ' ' and '  ' are all rejected; the
-- MATCHING still uses the raw `q`, so a query with meaningful internal or
-- surrounding whitespace behaves exactly as it does today. Trimming the match
-- too would have been a behaviour change smuggled in beside a security fix.
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
  -- F34: below two characters this matched every row. The clients never send
  -- such a query; a direct caller did not have to be one of the clients.
  WHERE char_length(btrim(COALESCE(q, ''))) >= 2
    AND (p.display_name ILIKE '%' || q || '%'
      OR p.handle::text   ILIKE '%' || q || '%'
      OR p.display_name % q
      OR p.handle::text   % q)
  GROUP BY p.user_id, p.handle, p.display_name, p.avatar_url
  ORDER BY sim DESC, build_count DESC
  -- F34: `lim` was whatever the caller said. q04 passed 1000000 and was served.
  LIMIT LEAST(GREATEST(COALESCE(lim, 10), 1), 25);
$$;

-- resolve_author is left exactly as it is. It takes a handle and returns the
-- one row holding it, so it enumerates nothing: the 500 profiles with a NULL
-- handle are unreachable through it by construction, and the five columns it
-- returns are the five granted above.

-- **Applying this: the REVOKE and the GRANT must not be separable.** Between
-- them `anon` can read NOTHING of this table, so a deploy that commits the
-- first and fails the second takes the whole author surface down -- the browse
-- view, `search_authors` and `resolve_author` are all invoker-rights over these
-- columns. One transaction, therefore.
--
-- And a trap measured on 2026-09-18 rather than assumed: **`supabase db query
-- --linked` does not keep one session across the statements in a file.** A temp
-- table created by the first statement is gone by the last, so a file reading
-- `BEGIN; REVOKE ...; GRANT ...; COMMIT;` is not one transaction through that
-- client -- each statement stands alone and the REVOKE commits by itself. Send
-- the pair as ONE statement (a `DO $$ ... $$` block is one), or apply it
-- through a client that holds the session. The same property is what makes a
-- rehearsal safe when it is written as a single DO block ending in `RAISE
-- EXCEPTION`: it cannot commit, whatever the client does with semicolons.
--
-- Rehearsed against production that way on 2026-09-18 and rolled back. After
-- the REVOKE+GRANT, inside the aborted transaction: table SELECT false; the
-- five withheld columns all false; the five granted all true; an `anon` browse
-- of `shared_builds_with_author` still returned **2,667 public rows**; the five
-- columns still read all **530** profiles; `resolve_author` still answered;
-- `SELECT discord_id` failed with "permission denied for table profiles" and so
-- did the `WHERE discord_id IS NOT NULL` filter. `authenticated` table SELECT
-- remained true, as intended and as the gap above describes.
--
-- **Applied 2026-09-18 and measured closed.** The grant pair went as the single
-- DO block described above; `search_authors` followed as its own statement,
-- which is atomic by itself. Column state after: `anon` reads exactly
-- user_id, handle, display_name, avatar_url, bio and none of the other five;
-- `authenticated` still reads all ten, as the gap above describes.
--
-- As `anon`, in production: `SELECT discord_id`, `SELECT *` and the
-- `WHERE discord_id IS NOT NULL` filter all denied; the five columns still read
-- all 530 profiles; the browse view still served 2,668 public rows;
-- `resolve_author` still answered. Through PostgREST with the public anon key,
-- the same three reads answer `42501 permission denied for table profiles`
-- where they previously returned Discord snowflakes and usernames.
--
-- `search_authors`, which returned 530 rows for an empty q and 328 for 'a' on
-- the morning of the same day: **0 and 0**. A two-character query with
-- `lim = 1000000` returns 8, the number that actually match, under the clamp of
-- 25. The real client call still answers.
--
-- Confirming it took. (a) and (b) are SQL; (c) needs the public anon key,
-- because what is being checked is what a stranger can do.
--
--   a. Exactly five column grants for anon, and no table-level SELECT:
--
--        SELECT grantee, column_name
--        FROM information_schema.column_privileges
--        WHERE table_name = 'profiles' AND privilege_type = 'SELECT'
--          AND grantee = 'anon'
--        ORDER BY column_name;
--        -- avatar_url, bio, display_name, handle, user_id. Five rows.
--
--        SELECT grantee, privilege_type FROM information_schema.table_privileges
--        WHERE table_name = 'profiles' AND grantee = 'anon'
--          AND privilege_type = 'SELECT';
--        -- must be empty. `authenticated` still has its row; see above.
--
--      Or re-run supabase/audit/q01 and q02 and read the profiles rows:
--      anon_select flips to false for the five withheld columns, and
--      anon_table_select flips to false.
--
--   b. The RPC no longer answers an empty query, and no longer takes a
--      caller's word for the limit — this is q04's first two lines, which
--      returned 530 and 328 on 2026-09-18:
--
--        SELECT count(*) FROM public.search_authors('',  1000000);  -- 0
--        SELECT count(*) FROM public.search_authors('a', 1000000);  -- 0
--        SELECT count(*) FROM public.search_authors('ab', 1000000); -- <= 25
--
--   c. From a shell, with the anon key. The first two must fail where they
--      previously answered; the rest must keep working, because a fix that
--      breaks the author surface is not a fix:
--
--        curl -sS -H "apikey: $ANON" -H "Authorization: Bearer $ANON" \
--          "$URL/rest/v1/profiles?select=discord_id,discord_username&limit=1"
--        curl -sS -H "apikey: $ANON" -H "Authorization: Bearer $ANON" \
--          "$URL/rest/v1/profiles?select=*&limit=1"
--        # each: 42501 / "permission denied for table profiles" — Postgres
--        # reports the TABLE form even when the denial is column-level, so
--        # that string is what a column REVOKE looks like. Before, the first
--        # returned a Discord snowflake and username and the second ten keys.
--
--        curl -sS -H "apikey: $ANON" -H "Authorization: Bearer $ANON" \
--          "$URL/rest/v1/profiles?select=handle,display_name,avatar_url&limit=3"
--        curl -sS -H "apikey: $ANON" -H "Authorization: Bearer $ANON" \
--          "$URL/rest/v1/shared_builds_with_author?select=id,name,author_handle,author_display_name,author_avatar_url&visibility=eq.public&limit=3"
--        curl -sS -H "apikey: $ANON" -H "Authorization: Bearer $ANON" \
--          -X POST -H "Content-Type: application/json" -d '{"h":"<a claimed handle>"}' \
--          "$URL/rest/v1/rpc/resolve_author"
--        # all three must answer as they do today. The middle one is the
--        # browse, and is the read that q14 was run to protect.
--
--      A filter is a read too, and this one must also stop answering — the
--      lesson F73's row records about itself:
--
--        curl -sS -H "apikey: $ANON" -H "Authorization: Bearer $ANON" \
--          "$URL/rest/v1/profiles?select=handle&discord_id=not.is.null&limit=1"
--        # 42501. Filtering on a column needs SELECT on it, so the census
--        # "how many accounts are Discord-linked" closes with the values.

-- ============================================
-- Migration: the signup seed must not be able to refuse the signup (F82, F83)
-- ============================================
--
-- F82. `profiles.display_name` carries `CHECK (char_length(display_name) <= 30)`
-- and `seed_profile_on_signup` wrote a provider-supplied name into it
-- untruncated. A Discord global name of 31 characters therefore aborted the
-- seed, and because the trigger is `AFTER INSERT` on `auth.users` in the same
-- transaction, **the abort took the signup with it** — reproduced against a
-- throwaway Postgres running this file, not reasoned about. The account could
-- not be created, and the person was told nothing useful.
--
-- The measurement could not see its own victims, which is the part worth
-- keeping: every row in `profiles` is an account that SUCCEEDED, so "0 rows
-- would abort today" is the number of survivors, not the number of victims.
-- The longest surviving provider name is exactly 30 — the cap itself — and the
-- margin is one character wide.
--
-- F83, the same seam. `update-profile` now normalises `display_name` through
-- `_shared/author-name.ts`, and the seed is the OTHER writer to that column:
-- a Discord global name of `@savant` was seeded verbatim, so the sigil rule
-- could be walked straight past by choosing a Discord name. The leading `@`
-- is stripped here for that reason.
--
-- What this deliberately does NOT do is re-implement the whole TypeScript rule
-- in plpgsql. The invisible-character classes (U+3164, the bidi and zero-width
-- families, `\p{Zs}`) are not handled on this path. A second copy of a rule in
-- a second language is the drift F10 and F85 were both filed for, and the
-- exposure here is bounded differently: a seeded name is re-normalised the
-- moment its owner edits it, and the client draws the proved `@handle` beside
-- it either way. Named rather than duplicated.

-- One copy of the seeding rule, so the trigger and the backfill below cannot
-- disagree about it. `left()` counts characters rather than bytes, so this
-- cannot halve an astral character the way a byte truncation would.
CREATE OR REPLACE FUNCTION seeded_display_name(raw TEXT)
RETURNS TEXT AS $$
  SELECT left(regexp_replace(btrim(COALESCE(raw, '')), '^@+\s*', ''), 30);
$$ LANGUAGE sql IMMUTABLE;

CREATE OR REPLACE FUNCTION seed_profile_on_signup()
RETURNS trigger AS $$
BEGIN
  INSERT INTO profiles (user_id, display_name, discord_id, discord_username, avatar_url)
  VALUES (
    NEW.id,
    seeded_display_name(COALESCE(
      NULLIF(NEW.raw_user_meta_data->'custom_claims'->>'global_name', ''),
      NULLIF(NEW.raw_user_meta_data->>'full_name', ''),
      NULLIF(NEW.raw_user_meta_data->>'name', ''),
      NULLIF(split_part(NEW.email, '@', 1), ''),
      ''
    )),
    NEW.raw_user_meta_data->>'provider_id',
    NEW.raw_user_meta_data->>'full_name',
    storable_avatar_url(NEW.raw_user_meta_data->>'avatar_url')
  )
  ON CONFLICT (user_id) DO NOTHING;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER SET search_path = public;

-- The trigger itself is unchanged and is not re-created here; CREATE OR
-- REPLACE FUNCTION swaps the body under it.

-- ============================================
-- Migration: increment_views answers only for a build the caller could read (F33)
-- ============================================
--
-- `increment_views` is SECURITY DEFINER, so it writes past RLS, and it is
-- callable by `anon` through PostgREST for any id at all. Two clauses, and
-- only one of them closes here.
--
-- **Any id.** The body was `WHERE id = build_id` and nothing else, so an
-- anonymous caller could bump the counter on a PRIVATE build — a write to a
-- row the caller cannot read, on a build it cannot know exists. It is bounded
-- to the visibilities that already answer by id, which is the same line
-- `preview-visibility.ts` draws for F08 and `get-build` draws for its own
-- reads: public and unlisted yield the whole build to anyone holding the id,
-- so counting a view of one discloses nothing further.
--
-- One behaviour changes and it is the intended one: an owner opening their own
-- PRIVATE build no longer increments it. The client calls this straight after
-- a successful read (`BuildDetailPage.tsx`), and for a private build the only
-- successful read is the owner's, so what stops being counted is an owner
-- counting themselves.
--
-- **Unbounded.** Not closed HERE, and closed further down — see "increment_views
-- counts a viewer once a day, not once a click (F33)", which took the first of
-- the two options this paragraph names. Left standing rather than rewritten
-- because this block is the migration as it was applied, and the function
-- below is not the one that runs; the note is here so the next reader is not
-- told by a live file that a closed clause is open.
CREATE OR REPLACE FUNCTION increment_views(build_id TEXT)
RETURNS void AS $$
BEGIN
  UPDATE shared_builds
     SET views = views + 1
   WHERE id = build_id
     AND visibility IN ('public', 'unlisted');
END;
$$ LANGUAGE plpgsql SECURITY DEFINER SET search_path = public;

-- ============================================
-- Migration: increment_views counts a viewer once a day, not once a click (F33)
-- ============================================
--
-- The second clause of F33, and the one the 2026-09-22 pass left open: anyone
-- who can view a public build could call this RPC as often as they liked, so
-- the counter was a number the caller chose. The row named the two honest
-- options -- a metering table, or accepting that a public vanity counter is
-- inflatable -- and this is the first.
--
-- **What it is, and what it is not.** This is de-duplication, not rate
-- limiting, which is what a view counter actually wants: the question "how
-- many people looked at this" is not answered better by counting one person's
-- refreshes. One viewer contributes at most 1 per build per UTC day, so
-- inflating a count by N now costs N distinct addresses or N days rather than
-- N requests. That is a bound. It is not a claim that the number cannot be
-- moved -- a botnet still moves it, and a public counter with no account
-- behind it never stops being approximate. What ends here is the version
-- anybody could move from one machine with a loop.
--
-- **No address is stored.** `viewer` is a SHA-256 over the day, the caller's
-- address and the build id, so the table cannot answer "what did this person
-- look at" even to somebody holding it: a row is unlinkable to any other row
-- for the same viewer on a different build, and yesterday's rows are deleted
-- rather than aged. Including the day in the digest is also what makes the
-- window a window -- at midnight UTC every viewer is new, with no rotation
-- step to get wrong.
--
-- **It fails closed on an unidentifiable caller, and that is a real cost.**
-- The address comes from PostgREST's `request.headers` GUC, which is set on a
-- call through the API and is ABSENT on a direct database connection. A caller
-- this function cannot identify is not counted, because the alternative is one
-- shared bucket that the first call of the day fills for everybody. So an RPC
-- invoked from psql silently does nothing, and `check-f33-f53-access.sql`
-- stubs the GUC for exactly this reason.
CREATE TABLE IF NOT EXISTS build_views (
  build_id TEXT NOT NULL,
  -- sha256(day | caller address | build id). Not an address, and not reversible
  -- to one without guessing the address -- which is cheap for IPv4, hence the
  -- daily delete rather than a long retention.
  viewer   TEXT NOT NULL,
  day      DATE NOT NULL,
  PRIMARY KEY (build_id, viewer)
);

-- RLS on with no policies = service role and the owner only, the same shape
-- rate_limits uses. The REVOKE is belt and braces against a Supabase project's
-- default privileges, which GRANT ALL on new public tables to anon and
-- authenticated: RLS already refuses them, and a grant nobody needs is still a
-- grant somebody has to reason about.
ALTER TABLE build_views ENABLE ROW LEVEL SECURITY;
REVOKE ALL ON public.build_views FROM anon, authenticated;

-- The window is a day, so anything from a previous one is dead weight. Same
-- shape and same reasoning as purge-rate-limits above, and the same reason for
-- no index on the predicate: the job deletes most of what it reads.
--
-- 41 past the hour rather than :17, to sit off the other purge.
SELECT cron.schedule(
  'purge-build-views',
  '41 * * * *',
  $purge$DELETE FROM public.build_views WHERE day < (now() AT TIME ZONE 'utc')::date$purge$
);

CREATE OR REPLACE FUNCTION increment_views(build_id TEXT)
RETURNS void AS $$
DECLARE
  -- The parameter under another name. `build_id` is also a COLUMN of
  -- build_views, and plpgsql raises on the ambiguity rather than guessing.
  target   TEXT := build_id;
  today    DATE := (now() AT TIME ZONE 'utc')::date;
  address  TEXT;
  digest   TEXT;
BEGIN
  -- Cloudflare sits in front of Supabase, so cf-connecting-ip is the address
  -- the edge saw and the one header a caller cannot choose. x-forwarded-for is
  -- the fallback and is a client-supplied list, so its first element is a
  -- claim -- which costs nothing here: a caller who forges it splits their own
  -- views across buckets they invented, which is the behaviour we already
  -- accept from someone holding several addresses.
  address := coalesce(
    current_setting('request.headers', true)::json->>'cf-connecting-ip',
    split_part(current_setting('request.headers', true)::json->>'x-forwarded-for', ',', 1)
  );
  -- No identifiable caller, no count. See the note above: the alternative is
  -- one bucket for the world.
  IF address IS NULL OR address = '' THEN
    RETURN;
  END IF;

  -- The visibility bound from the first half of F33, asked before anything is
  -- written: a private build must not even leave a metering row behind.
  PERFORM 1 FROM shared_builds
   WHERE id = target AND visibility IN ('public', 'unlisted');
  IF NOT FOUND THEN
    RETURN;
  END IF;

  digest := encode(sha256(convert_to(today::text || '|' || address || '|' || target, 'UTF8')), 'hex');

  -- ON CONSTRAINT rather than `ON CONFLICT (build_id, viewer)`: a conflict
  -- target's column list is resolved against plpgsql variables as well as
  -- columns, and `build_id` is both. That is an ambiguity error at RUN time,
  -- not at CREATE time, so the function was created happily and every call
  -- raised -- which is how the fixture caught it and reading would not have.
  INSERT INTO build_views (build_id, viewer, day)
  VALUES (target, digest, today)
  ON CONFLICT ON CONSTRAINT build_views_pkey DO NOTHING;

  -- FOUND is false when the conflict swallowed the insert, which is the whole
  -- mechanism: this viewer has already been counted for this build today.
  IF NOT FOUND THEN
    RETURN;
  END IF;

  UPDATE shared_builds
     SET views = views + 1
   WHERE id = target
     AND visibility IN ('public', 'unlisted');
END;
$$ LANGUAGE plpgsql SECURITY DEFINER SET search_path = public;

-- ============================================
-- Migration: favoriting answers the same way for "not yours" and "not there" (F53)
-- ============================================
--
-- `favorites.build_id` carries `REFERENCES shared_builds(id)`, and a foreign
-- key is checked by a system trigger that runs as the constraint's owner, past
-- RLS. So inserting a favourite told the caller which ids exist: a real
-- private build inserted cleanly, and a made-up one came back
-- `23503 foreign_key_violation`. Two different answers about a row the caller
-- is not allowed to see.
--
-- Closed with a BEFORE INSERT trigger, which fires before the FK's own AFTER
-- ROW check, so the FK's error never reaches the caller. Both cases now raise
-- the same message.
--
-- **The predicate is `get-build`'s, not the RLS policy's, and that distinction
-- is the trap here.** RLS on `shared_builds` grants `visibility = 'public'`
-- plus an owner's own rows; UNLISTED is deliberately not in it, because an
-- unlisted build is read through the `get-build` edge function on a point
-- lookup rather than through a policy that would make it listable. A trigger
-- that asked RLS would therefore have refused to favourite an unlisted build,
-- which is a thing people can do today and the whole point of unlisted. So
-- this is SECURITY DEFINER and states the readable set directly: the two
-- visibilities that answer by id, plus the caller's own rows at any
-- visibility.
--
-- The FK stays. It is what makes `ON DELETE CASCADE` clean up favourites when
-- a build is deleted, and the trigger is a gate in front of it, not a
-- replacement for it.
CREATE OR REPLACE FUNCTION favorites_build_must_be_reachable()
RETURNS trigger AS $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM shared_builds b
     WHERE b.id = NEW.build_id
       AND (b.visibility IN ('public', 'unlisted') OR b.user_id = auth.uid())
  ) THEN
    -- Deliberately the same refusal for "no such build" and "not yours": that
    -- sameness IS the fix, and a message naming which one would undo it.
    RAISE EXCEPTION 'Build not found' USING ERRCODE = 'no_data_found';
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER SET search_path = public;

DROP TRIGGER IF EXISTS favorites_reachable_build ON favorites;
CREATE TRIGGER favorites_reachable_build
  BEFORE INSERT ON favorites
  FOR EACH ROW EXECUTE FUNCTION favorites_build_must_be_reachable();

-- ============================================
-- Migration: the seed path runs the same identity rule as everything else (F83)
-- ============================================
--
-- F83's last open clause. `seed_profile_on_signup` writes a provider-supplied
-- name at signup and never passes through `update-profile`, so a Discord
-- global name reached `profiles.display_name` having met F82's sigil handling
-- -- trim, strip a leading `@` run, truncate by character -- and none of the
-- invisible-character classes: U+3164 and the HANGUL FILLER family, the bidi
-- overrides, the zero-width family, `\p{Zs}`.
--
-- The row said why nobody had written those here: "a plpgsql re-implementation
-- of the TypeScript rule would be a second copy of it in a second language,
-- which is the drift F10 and F85 were both filed for." That is right about a
-- COPY. What follows is not one -- `identity_name_normalized` below is
-- GENERATED from `_shared/author-name.ts` by putting all 1,114,112 code points
-- through `normalizeIdentityName` and recording what came back, so the two
-- halves cannot disagree about a character without the regeneration gate
-- saying so. The objection to a second implementation is an objection to one
-- that can drift, and a derived one can only be stale.
--
-- Three things this does NOT change:
--
--   * The truncation stays where it was. `seeded_display_name` still cuts at
--     30 and `update-profile` still REFUSES over-length rather than cutting --
--     the deliberate asymmetry F83 records, because a name silently shortened
--     in a form you are looking at is the kind of thing you find out later. A
--     seed has no form and no person in front of it, so it cuts.
--   * Rows already stored are not rewritten. F69's convention, stated on its
--     own row: the rule runs at write time. The 2026-09-22 census found 0
--     blank-renderers in this column and the one live claim was deleted, so
--     there is nothing here to rewrite in any case.
--   * The trigger is untouched. CREATE OR REPLACE FUNCTION swaps the body
--     under it.

-- >>> GENERATED by scripts/gen-identity-sql.mjs -- do not edit by hand
--
-- Derived from `supabase/functions/_shared/author-name.ts` by putting all
-- 1114112 code points through `normalizeIdentityName` and sorting them by
-- what came back. Regenerate with `node scripts/gen-identity-sql.mjs --write`;
-- `supabase/identity-sql-generated.test.ts` fails if this block and that file
-- have drifted apart.
--
-- 28 ranges removed outright, 9 collapsed to a space,
-- 750 strippable from the front of a name.
CREATE OR REPLACE FUNCTION identity_name_normalized(raw TEXT)
RETURNS TEXT AS $identity$
  SELECT btrim(
    -- 4. Whatever may not open a name comes off the front: the @ sigil, the
    --    blank openers, in any order and any number. The positive-class
    --    reasoning behind the set is in author-name.ts's OPENER comment; here
    --    it is just the answer.
    regexp_replace(
      -- 3. Collapse the runs the step below produced, then the outer btrim.
      regexp_replace(
        -- 2. The gaps become one space; the widthless ones vanish. Both in one
        --    pass each, and the order between them does not matter because a
        --    removed character cannot be what keeps two spaces apart once the
        --    collapse in step 3 runs after both.
        regexp_replace(
          regexp_replace(
            -- 1. NFC first, so nothing downstream measures a decomposed name.
            normalize(COALESCE(raw, ''), NFC),
            '[\u0000-\u0008\u000E-\u001F\u007F-\u009F\u00AD\u034F\u0600-\u0605\u061C\u06DD\u070F\u0890-\u0891\u08E2\u115F-\u1160\u17B4-\u17B5\u180B-\u180F\u200B-\u200F\u202A-\u202E\u2060-\u206F\u3164\uFE00-\uFE0F\uFEFF\uFFA0\uFFF0-\uFFFB\U000110BD\U000110CD\U00013430-\U0001343F\U0001BCA0-\U0001BCA3\U0001D173-\U0001D17A\U000E0000-\U000E0FFF]', '', 'g'),
          '[\u0009-\u000D\u0020\u00A0\u1680\u2002-\u200A\u2028-\u2029\u202F\u205F\u3000]', ' ', 'g'),
        '[ ]+', ' ', 'g'),
      '^[\u0000-\u0020\u0040\u007F-\u00A0\u00AD\u034F\u0378-\u0379\u0380-\u0383\u038B\u038D\u03A2\u0530\u0557-\u0558\u058B-\u058C\u0590\u05C8-\u05CF\u05EB-\u05EE\u05F5-\u0605\u061C\u06DD\u070E-\u070F\u074B-\u074C\u07B2-\u07BF\u07FB-\u07FC\u082E-\u082F\u083F\u085C-\u085D\u085F\u086B-\u086F\u0890-\u0896\u08E2\u0984\u098D-\u098E\u0991-\u0992\u09A9\u09B1\u09B3-\u09B5\u09BA-\u09BB\u09C5-\u09C6\u09C9-\u09CA\u09CF-\u09D6\u09D8-\u09DB\u09DE\u09E4-\u09E5\u09FF-\u0A00\u0A04\u0A0B-\u0A0E\u0A11-\u0A12\u0A29\u0A31\u0A34\u0A37\u0A3A-\u0A3B\u0A3D\u0A43-\u0A46\u0A49-\u0A4A\u0A4E-\u0A50\u0A52-\u0A58\u0A5D\u0A5F-\u0A65\u0A77-\u0A80\u0A84\u0A8E\u0A92\u0AA9\u0AB1\u0AB4\u0ABA-\u0ABB\u0AC6\u0ACA\u0ACE-\u0ACF\u0AD1-\u0ADF\u0AE4-\u0AE5\u0AF2-\u0AF8\u0B00\u0B04\u0B0D-\u0B0E\u0B11-\u0B12\u0B29\u0B31\u0B34\u0B3A-\u0B3B\u0B45-\u0B46\u0B49-\u0B4A\u0B4E-\u0B54\u0B58-\u0B5B\u0B5E\u0B64-\u0B65\u0B78-\u0B81\u0B84\u0B8B-\u0B8D\u0B91\u0B96-\u0B98\u0B9B\u0B9D\u0BA0-\u0BA2\u0BA5-\u0BA7\u0BAB-\u0BAD\u0BBA-\u0BBD\u0BC3-\u0BC5\u0BC9\u0BCE-\u0BCF\u0BD1-\u0BD6\u0BD8-\u0BE5\u0BFB-\u0BFF\u0C0D\u0C11\u0C29\u0C3A-\u0C3B\u0C45\u0C49\u0C4E-\u0C54\u0C57\u0C5B\u0C5E-\u0C5F\u0C64-\u0C65\u0C70-\u0C76\u0C8D\u0C91\u0CA9\u0CB4\u0CBA-\u0CBB\u0CC5\u0CC9\u0CCE-\u0CD4\u0CD7-\u0CDB\u0CDF\u0CE4-\u0CE5\u0CF0\u0CF4-\u0CFF\u0D0D\u0D11\u0D45\u0D49\u0D50-\u0D53\u0D64-\u0D65\u0D80\u0D84\u0D97-\u0D99\u0DB2\u0DBC\u0DBE-\u0DBF\u0DC7-\u0DC9\u0DCB-\u0DCE\u0DD5\u0DD7\u0DE0-\u0DE5\u0DF0-\u0DF1\u0DF5-\u0E00\u0E3B-\u0E3E\u0E5C-\u0E80\u0E83\u0E85\u0E8B\u0EA4\u0EA6\u0EBE-\u0EBF\u0EC5\u0EC7\u0ECF\u0EDA-\u0EDB\u0EE0-\u0EFF\u0F48\u0F6D-\u0F70\u0F98\u0FBD\u0FCD\u0FDB-\u0FFF\u10C6\u10C8-\u10CC\u10CE-\u10CF\u115F-\u1160\u1249\u124E-\u124F\u1257\u1259\u125E-\u125F\u1289\u128E-\u128F\u12B1\u12B6-\u12B7\u12BF\u12C1\u12C6-\u12C7\u12D7\u1311\u1316-\u1317\u135B-\u135C\u137D-\u137F\u139A-\u139F\u13F6-\u13F7\u13FE-\u13FF\u1680\u169D-\u169F\u16F9-\u16FF\u1716-\u171E\u1737-\u173F\u1754-\u175F\u176D\u1771\u1774-\u177F\u17B4-\u17B5\u17DE-\u17DF\u17EA-\u17EF\u17FA-\u17FF\u180B-\u180F\u181A-\u181F\u1879-\u187F\u18AB-\u18AF\u18F6-\u18FF\u191F\u192C-\u192F\u193C-\u193F\u1941-\u1943\u196E-\u196F\u1975-\u197F\u19AC-\u19AF\u19CA-\u19CF\u19DB-\u19DD\u1A1C-\u1A1D\u1A5F\u1A7D-\u1A7E\u1A8A-\u1A8F\u1A9A-\u1A9F\u1AAE-\u1AAF\u1ADE-\u1ADF\u1AEC-\u1AFF\u1B4D\u1BF4-\u1BFB\u1C38-\u1C3A\u1C4A-\u1C4C\u1C8B-\u1C8F\u1CBB-\u1CBC\u1CC8-\u1CCF\u1CFB-\u1CFF\u1F16-\u1F17\u1F1E-\u1F1F\u1F46-\u1F47\u1F4E-\u1F4F\u1F58\u1F5A\u1F5C\u1F5E\u1F7E-\u1F7F\u1FB5\u1FC5\u1FD4-\u1FD5\u1FDC\u1FF0-\u1FF1\u1FF5\u1FFF\u2002-\u200F\u2028-\u202F\u205F-\u206F\u2072-\u2073\u208F\u209D-\u209F\u20C2-\u20CF\u20F1-\u20FF\u218C-\u218F\u242A-\u243F\u244B-\u245F\u2800\u2B74-\u2B75\u2CF4-\u2CF8\u2D26\u2D28-\u2D2C\u2D2E-\u2D2F\u2D68-\u2D6E\u2D71-\u2D7E\u2D97-\u2D9F\u2DA7\u2DAF\u2DB7\u2DBF\u2DC7\u2DCF\u2DD7\u2DDF\u2E5E-\u2E7F\u2E9A\u2EF4-\u2EFF\u2FD6-\u2FEF\u3000\u3040\u3097-\u3098\u3100-\u3104\u3130\u3164\u318F\u31E6-\u31EE\u321F\uA48D-\uA48F\uA4C7-\uA4CF\uA62C-\uA63F\uA6F8-\uA6FF\uA7DD-\uA7F0\uA82D-\uA82F\uA83A-\uA83F\uA878-\uA87F\uA8C6-\uA8CD\uA8DA-\uA8DF\uA954-\uA95E\uA97D-\uA97F\uA9CE\uA9DA-\uA9DD\uA9FF\uAA37-\uAA3F\uAA4E-\uAA4F\uAA5A-\uAA5B\uAAC3-\uAADA\uAAF7-\uAB00\uAB07-\uAB08\uAB0F-\uAB10\uAB17-\uAB1F\uAB27\uAB2F\uAB6C-\uAB6F\uABEE-\uABEF\uABFA-\uABFF\uD7A4-\uD7AF\uD7C7-\uD7CA\uD7FC-\uF8FF\uFA6E-\uFA6F\uFADA-\uFAFF\uFB07-\uFB12\uFB18-\uFB1C\uFB37\uFB3D\uFB3F\uFB42\uFB45\uFDD0-\uFDEF\uFE00-\uFE0F\uFE1A-\uFE1F\uFE53\uFE67\uFE6C-\uFE6F\uFE75\uFEFD-\uFF00\uFFA0\uFFBF-\uFFC1\uFFC8-\uFFC9\uFFD0-\uFFD1\uFFD8-\uFFD9\uFFDD-\uFFDF\uFFE7\uFFEF-\uFFFB\uFFFE-\uFFFF\U0001000C\U00010027\U0001003B\U0001003E\U0001004E-\U0001004F\U0001005E-\U0001007F\U000100FB-\U000100FF\U00010103-\U00010106\U00010134-\U00010136\U0001018F\U0001019D-\U0001019F\U000101A1-\U000101CF\U000101FE-\U0001027F\U0001029D-\U0001029F\U000102D1-\U000102DF\U000102FC-\U000102FF\U00010324-\U0001032C\U0001034B-\U0001034F\U0001037B-\U0001037F\U0001039E\U000103C4-\U000103C7\U000103D6-\U000103FF\U0001049E-\U0001049F\U000104AA-\U000104AF\U000104D4-\U000104D7\U000104FC-\U000104FF\U00010528-\U0001052F\U00010564-\U0001056E\U0001057B\U0001058B\U00010593\U00010596\U000105A2\U000105B2\U000105BA\U000105BD-\U000105BF\U000105F4-\U000105FF\U00010737-\U0001073F\U00010756-\U0001075F\U00010768-\U0001077F\U00010786\U000107B1\U000107BB-\U000107FF\U00010806-\U00010807\U00010809\U00010836\U00010839-\U0001083B\U0001083D-\U0001083E\U00010856\U0001089F-\U000108A6\U000108B0-\U000108DF\U000108F3\U000108F6-\U000108FA\U0001091C-\U0001091E\U0001093A-\U0001093E\U0001095A-\U0001097F\U000109B8-\U000109BB\U000109D0-\U000109D1\U00010A04\U00010A07-\U00010A0B\U00010A14\U00010A18\U00010A36-\U00010A37\U00010A3B-\U00010A3E\U00010A49-\U00010A4F\U00010A59-\U00010A5F\U00010AA0-\U00010ABF\U00010AE7-\U00010AEA\U00010AF7-\U00010AFF\U00010B36-\U00010B38\U00010B56-\U00010B57\U00010B73-\U00010B77\U00010B92-\U00010B98\U00010B9D-\U00010BA8\U00010BB0-\U00010BFF\U00010C49-\U00010C7F\U00010CB3-\U00010CBF\U00010CF3-\U00010CF9\U00010D28-\U00010D2F\U00010D3A-\U00010D3F\U00010D66-\U00010D68\U00010D86-\U00010D8D\U00010D90-\U00010E5F\U00010E7F\U00010EAA\U00010EAE-\U00010EAF\U00010EB2-\U00010EC1\U00010EC8-\U00010ECF\U00010ED9-\U00010EF9\U00010F28-\U00010F2F\U00010F5A-\U00010F6F\U00010F8A-\U00010FAF\U00010FCC-\U00010FDF\U00010FF7-\U00010FFF\U0001104E-\U00011051\U00011076-\U0001107E\U000110BD\U000110C3-\U000110CF\U000110E9-\U000110EF\U000110FA-\U000110FF\U00011135\U00011148-\U0001114F\U00011177-\U0001117F\U000111E0\U000111F5-\U000111FF\U00011212\U00011242-\U0001127F\U00011287\U00011289\U0001128E\U0001129E\U000112AA-\U000112AF\U000112EB-\U000112EF\U000112FA-\U000112FF\U00011304\U0001130D-\U0001130E\U00011311-\U00011312\U00011329\U00011331\U00011334\U0001133A\U00011345-\U00011346\U00011349-\U0001134A\U0001134E-\U0001134F\U00011351-\U00011356\U00011358-\U0001135C\U00011364-\U00011365\U0001136D-\U0001136F\U00011375-\U0001137F\U0001138A\U0001138C-\U0001138D\U0001138F\U000113B6\U000113C1\U000113C3-\U000113C4\U000113C6\U000113CB\U000113D6\U000113D9-\U000113E0\U000113E3-\U000113FF\U0001145C\U00011462-\U0001147F\U000114C8-\U000114CF\U000114DA-\U0001157F\U000115B6-\U000115B7\U000115DE-\U000115FF\U00011645-\U0001164F\U0001165A-\U0001165F\U0001166D-\U0001167F\U000116BA-\U000116BF\U000116CA-\U000116CF\U000116E4-\U000116FF\U0001171B-\U0001171C\U0001172C-\U0001172F\U00011747-\U000117FF\U0001183C-\U0001189F\U000118F3-\U000118FE\U00011907-\U00011908\U0001190A-\U0001190B\U00011914\U00011917\U00011936\U00011939-\U0001193A\U00011947-\U0001194F\U0001195A-\U0001199F\U000119A8-\U000119A9\U000119D8-\U000119D9\U000119E5-\U000119FF\U00011A48-\U00011A4F\U00011AA3-\U00011AAF\U00011AF9-\U00011AFF\U00011B0A-\U00011B5F\U00011B68-\U00011BBF\U00011BE2-\U00011BEF\U00011BFA-\U00011BFF\U00011C09\U00011C37\U00011C46-\U00011C4F\U00011C6D-\U00011C6F\U00011C90-\U00011C91\U00011CA8\U00011CB7-\U00011CFF\U00011D07\U00011D0A\U00011D37-\U00011D39\U00011D3B\U00011D3E\U00011D48-\U00011D4F\U00011D5A-\U00011D5F\U00011D66\U00011D69\U00011D8F\U00011D92\U00011D99-\U00011D9F\U00011DAA-\U00011DAF\U00011DDC-\U00011DDF\U00011DEA-\U00011EDF\U00011EF9-\U00011EFF\U00011F11\U00011F3B-\U00011F3D\U00011F5B-\U00011FAF\U00011FB1-\U00011FBF\U00011FF2-\U00011FFE\U0001239A-\U000123FF\U0001246F\U00012475-\U0001247F\U00012544-\U00012F8F\U00012FF3-\U00012FFF\U00013430-\U0001343F\U00013456-\U0001345F\U000143FB-\U000143FF\U00014647-\U000160FF\U0001613A-\U000167FF\U00016A39-\U00016A3F\U00016A5F\U00016A6A-\U00016A6D\U00016ABF\U00016ACA-\U00016ACF\U00016AEE-\U00016AEF\U00016AF6-\U00016AFF\U00016B46-\U00016B4F\U00016B5A\U00016B62\U00016B78-\U00016B7C\U00016B90-\U00016D3F\U00016D7A-\U00016E3F\U00016E9B-\U00016E9F\U00016EB9-\U00016EBA\U00016ED4-\U00016EFF\U00016F4B-\U00016F4E\U00016F88-\U00016F8E\U00016FA0-\U00016FDF\U00016FE5-\U00016FEF\U00016FF7-\U00016FFF\U00018CD6-\U00018CFE\U00018D1F-\U00018D7F\U00018DF3-\U0001AFEF\U0001AFF4\U0001AFFC\U0001AFFF\U0001B123-\U0001B131\U0001B133-\U0001B14F\U0001B153-\U0001B154\U0001B156-\U0001B163\U0001B168-\U0001B16F\U0001B2FC-\U0001BBFF\U0001BC6B-\U0001BC6F\U0001BC7D-\U0001BC7F\U0001BC89-\U0001BC8F\U0001BC9A-\U0001BC9B\U0001BCA0-\U0001CBFF\U0001CCFD-\U0001CCFF\U0001CEB4-\U0001CEB9\U0001CED1-\U0001CEDF\U0001CEF1-\U0001CEFF\U0001CF2E-\U0001CF2F\U0001CF47-\U0001CF4F\U0001CFC4-\U0001CFFF\U0001D0F6-\U0001D0FF\U0001D127-\U0001D128\U0001D173-\U0001D17A\U0001D1EB-\U0001D1FF\U0001D246-\U0001D2BF\U0001D2D4-\U0001D2DF\U0001D2F4-\U0001D2FF\U0001D357-\U0001D35F\U0001D379-\U0001D3FF\U0001D455\U0001D49D\U0001D4A0-\U0001D4A1\U0001D4A3-\U0001D4A4\U0001D4A7-\U0001D4A8\U0001D4AD\U0001D4BA\U0001D4BC\U0001D4C4\U0001D506\U0001D50B-\U0001D50C\U0001D515\U0001D51D\U0001D53A\U0001D53F\U0001D545\U0001D547-\U0001D549\U0001D551\U0001D6A6-\U0001D6A7\U0001D7CC-\U0001D7CD\U0001DA8C-\U0001DA9A\U0001DAA0\U0001DAB0-\U0001DEFF\U0001DF1F-\U0001DF24\U0001DF2B-\U0001DFFF\U0001E007\U0001E019-\U0001E01A\U0001E022\U0001E025\U0001E02B-\U0001E02F\U0001E06E-\U0001E08E\U0001E090-\U0001E0FF\U0001E12D-\U0001E12F\U0001E13E-\U0001E13F\U0001E14A-\U0001E14D\U0001E150-\U0001E28F\U0001E2AF-\U0001E2BF\U0001E2FA-\U0001E2FE\U0001E300-\U0001E4CF\U0001E4FA-\U0001E5CF\U0001E5FB-\U0001E5FE\U0001E600-\U0001E6BF\U0001E6DF\U0001E6F6-\U0001E6FD\U0001E700-\U0001E7DF\U0001E7E7\U0001E7EC\U0001E7EF\U0001E7FF\U0001E8C5-\U0001E8C6\U0001E8D7-\U0001E8FF\U0001E94C-\U0001E94F\U0001E95A-\U0001E95D\U0001E960-\U0001EC70\U0001ECB5-\U0001ED00\U0001ED3E-\U0001EDFF\U0001EE04\U0001EE20\U0001EE23\U0001EE25-\U0001EE26\U0001EE28\U0001EE33\U0001EE38\U0001EE3A\U0001EE3C-\U0001EE41\U0001EE43-\U0001EE46\U0001EE48\U0001EE4A\U0001EE4C\U0001EE50\U0001EE53\U0001EE55-\U0001EE56\U0001EE58\U0001EE5A\U0001EE5C\U0001EE5E\U0001EE60\U0001EE63\U0001EE65-\U0001EE66\U0001EE6B\U0001EE73\U0001EE78\U0001EE7D\U0001EE7F\U0001EE8A\U0001EE9C-\U0001EEA0\U0001EEA4\U0001EEAA\U0001EEBC-\U0001EEEF\U0001EEF2-\U0001EFFF\U0001F02C-\U0001F02F\U0001F094-\U0001F09F\U0001F0AF-\U0001F0B0\U0001F0C0\U0001F0D0\U0001F0F6-\U0001F0FF\U0001F1AE-\U0001F1E5\U0001F203-\U0001F20F\U0001F23C-\U0001F23F\U0001F249-\U0001F24F\U0001F252-\U0001F25F\U0001F266-\U0001F2FF\U0001F6D9-\U0001F6DB\U0001F6ED-\U0001F6EF\U0001F6FD-\U0001F6FF\U0001F7DA-\U0001F7DF\U0001F7EC-\U0001F7EF\U0001F7F1-\U0001F7FF\U0001F80C-\U0001F80F\U0001F848-\U0001F84F\U0001F85A-\U0001F85F\U0001F888-\U0001F88F\U0001F8AE-\U0001F8AF\U0001F8BC-\U0001F8BF\U0001F8C2-\U0001F8CF\U0001F8D9-\U0001F8FF\U0001FA58-\U0001FA5F\U0001FA6E-\U0001FA6F\U0001FA7D-\U0001FA7F\U0001FA8B-\U0001FA8D\U0001FAC7\U0001FAC9-\U0001FACC\U0001FADD-\U0001FADE\U0001FAEB-\U0001FAEE\U0001FAF9-\U0001FAFF\U0001FB93\U0001FBFB-\U0001FFFF\U0002A6E0-\U0002A6FF\U0002B81E-\U0002B81F\U0002CEAE-\U0002CEAF\U0002EBE1-\U0002EBEF\U0002EE5E-\U0002F7FF\U0002FA1E-\U0002FFFF\U0003134B-\U0003134F\U0003347A-\U0010FFFF ]*', '')
  );
$identity$ LANGUAGE sql IMMUTABLE;
-- <<< GENERATED

-- The seed's own step, which is the generated rule plus the truncation that
-- belongs to this column and not to the concept.
CREATE OR REPLACE FUNCTION seeded_display_name(raw TEXT)
RETURNS TEXT AS $$
  SELECT left(identity_name_normalized(raw), 30);
$$ LANGUAGE sql IMMUTABLE;
