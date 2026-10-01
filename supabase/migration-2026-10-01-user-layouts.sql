-- Migration, 2026-10-01 — user_layouts, for the 1.0 planner's layout sync.
--
-- Lifted verbatim from `schema.sql`, which is where it lives permanently; this
-- file exists so the change can be applied to a database that already carries
-- the rest, and is safe to delete once it has been. Run it whole in the SQL
-- editor. Everything in it is CREATE TABLE IF NOT EXISTS, DROP/CREATE POLICY,
-- or a REVOKE/GRANT, so re-running it replaces rather than stacks.
--
-- Adds one table and touches nothing else: no existing table, function or
-- policy changes, so the beta is unaffected.

-- ============================================
-- User layouts — the 1.0 planner's dashboard and grid, synced to the account
-- ============================================
-- Before this, the 1.0 planner kept its layout (stat panels, grid positions,
-- phone panel order, quickbar pins, Powers view) only in localStorage, so it
-- was lost on a new browser, a new device or a cleared cache. One row per
-- user holds the whole layout as one document. The beta never reads it.
--
-- Written straight from the client under RLS, as favorites is: the data is
-- the user's own preference and each policy is scoped to auth.uid().
--
-- `edited_at` is the client's clock at the last edit, not the server's at
-- receipt. Sync keeps whichever copy was edited last, and the time a device
-- uploaded is not that: a layout edited offline and uploaded later would win
-- over a newer edit made elsewhere in between.
--
-- The size cap is far above a real layout (a few KB) and only bounds what one
-- account can store.
CREATE TABLE IF NOT EXISTS user_layouts (
  user_id   UUID PRIMARY KEY REFERENCES auth.users(id) ON DELETE CASCADE,
  layout    JSONB NOT NULL,
  edited_at TIMESTAMPTZ NOT NULL,
  CONSTRAINT user_layouts_size CHECK (octet_length(layout::text) <= 65536)
);

ALTER TABLE user_layouts ENABLE ROW LEVEL SECURITY;

-- Read, create and replace your own row; no delete, since nothing in the
-- client removes a layout and account deletion cascades. An upsert is an
-- INSERT ... ON CONFLICT DO UPDATE, which needs all three policies.
DROP POLICY IF EXISTS "own layout read" ON user_layouts;
CREATE POLICY "own layout read" ON user_layouts
  FOR SELECT TO authenticated USING (user_id = auth.uid());
DROP POLICY IF EXISTS "own layout insert" ON user_layouts;
CREATE POLICY "own layout insert" ON user_layouts
  FOR INSERT TO authenticated WITH CHECK (user_id = auth.uid());
DROP POLICY IF EXISTS "own layout update" ON user_layouts;
CREATE POLICY "own layout update" ON user_layouts
  FOR UPDATE TO authenticated
  USING (user_id = auth.uid()) WITH CHECK (user_id = auth.uid());

-- Grants stated rather than inherited from the project's defaults, which give
-- anon and authenticated ALL on a new public table. Anon has no use for it.
REVOKE ALL ON public.user_layouts FROM anon, authenticated;
GRANT SELECT, INSERT, UPDATE ON public.user_layouts TO authenticated;
