-- user_layouts — who may read and write a layout, executed rather than read.
--
--   cd fixture && ./build.sh
--   docker exec -i sk-sqlcheck psql -U postgres -d skcheck -f - < check-user-layouts.sql
--
-- Expected, in order:
--   own upsert twice      INSERT 0 1, INSERT 0 1 (the second is the update arm)
--   own read              one row, edited_at 2026-10-01 12:05
--   another's read        count 0
--   insert as another     ERROR new row violates row-level security policy
--   update another's      UPDATE 0
--   give own row away     ERROR new row violates row-level security policy
--   oversized layout      ERROR violates check constraint "user_layouts_size"
--   delete own            ERROR permission denied for table user_layouts
--   anon read             ERROR permission denied for table user_layouts
--   B's row afterwards    still B's, layout untouched
--
-- Needs a fresh fixture: these ids persist.
INSERT INTO auth.users (id, email) VALUES
  ('cccccccc-0000-4000-8000-0000000000a1', 'a@example.com'),
  ('cccccccc-0000-4000-8000-0000000000b2', 'b@example.com');

-- The fixture has no GoTrue, so the caller comes from a setting each block sets.
CREATE OR REPLACE FUNCTION auth.uid() RETURNS uuid AS $$
  SELECT nullif(current_setting('test.uid', true), '')::uuid;
$$ LANGUAGE sql STABLE;
GRANT USAGE ON SCHEMA auth TO authenticated, anon;

-- B's row, written as B.
SET test.uid = 'cccccccc-0000-4000-8000-0000000000b2';
SET ROLE authenticated;
INSERT INTO user_layouts (user_id, layout, edited_at)
  VALUES ('cccccccc-0000-4000-8000-0000000000b2', '{"owner":"b"}', '2026-10-01 11:00Z');
RESET ROLE;

SET test.uid = 'cccccccc-0000-4000-8000-0000000000a1';
SET ROLE authenticated;

\echo '=== own upsert, twice: expect INSERT 0 1 both times'
INSERT INTO user_layouts (user_id, layout, edited_at)
  VALUES ('cccccccc-0000-4000-8000-0000000000a1', '{"v":1}', '2026-10-01 12:00Z')
  ON CONFLICT (user_id) DO UPDATE SET layout = EXCLUDED.layout, edited_at = EXCLUDED.edited_at;
INSERT INTO user_layouts (user_id, layout, edited_at)
  VALUES ('cccccccc-0000-4000-8000-0000000000a1', '{"v":2}', '2026-10-01 12:05Z')
  ON CONFLICT (user_id) DO UPDATE SET layout = EXCLUDED.layout, edited_at = EXCLUDED.edited_at;

\echo '=== own read: expect one row, v 2'
SELECT user_id, layout, edited_at FROM user_layouts;

\echo '=== another account''s row: expect count 0'
SELECT count(*) FROM user_layouts WHERE user_id = 'cccccccc-0000-4000-8000-0000000000b2';

\echo '=== insert as another account: expect RLS error'
INSERT INTO user_layouts (user_id, layout, edited_at)
  VALUES ('cccccccc-0000-4000-8000-0000000000b2', '{"owner":"a"}', now());

\echo '=== update another account''s row: expect UPDATE 0'
UPDATE user_layouts SET layout = '{"owner":"a"}'
  WHERE user_id = 'cccccccc-0000-4000-8000-0000000000b2';

\echo '=== give own row to another account: expect RLS error'
UPDATE user_layouts SET user_id = 'cccccccc-0000-4000-8000-0000000000b2'
  WHERE user_id = 'cccccccc-0000-4000-8000-0000000000a1';

\echo '=== oversized layout: expect check constraint error'
UPDATE user_layouts SET layout = jsonb_build_object('pad', repeat('x', 70000))
  WHERE user_id = 'cccccccc-0000-4000-8000-0000000000a1';

\echo '=== delete own: expect permission denied'
DELETE FROM user_layouts WHERE user_id = 'cccccccc-0000-4000-8000-0000000000a1';

RESET ROLE;
SET test.uid = '';
SET ROLE anon;
\echo '=== anon read: expect permission denied'
SELECT count(*) FROM user_layouts;
RESET ROLE;

\echo '=== B''s row afterwards: expect owner b'
SELECT user_id, layout FROM user_layouts ORDER BY user_id;
