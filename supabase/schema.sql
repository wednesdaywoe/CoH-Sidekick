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
