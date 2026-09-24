-- F33 / F53 — two SECURITY DEFINER paths that answered about rows the caller
-- cannot read, executed rather than read.
--
--   cd fixture && ./build.sh
--   docker exec -i sk-sqlcheck psql -U postgres -d skcheck -f - < check-f33-f53-access.sql
--
-- Expected:
--   F33  privf53 views 0; pubf53 and unlif53 views 1
--   F53  public, unlisted and own-private INSERT 0 1
--        another account's private AND a nonexistent id both: ERROR "Build not
--        found" -- the SAMENESS is the fix; two different errors is the oracle
--
-- Needs a fresh fixture: these ids persist, and auth.uid() is stubbed below.
INSERT INTO auth.users (id, email, raw_user_meta_data) VALUES
  ('bbbbbbbb-0000-4000-8000-00000000f531','me@example.com','{}'),
  ('bbbbbbbb-0000-4000-8000-00000000f532','other@example.com','{}');
INSERT INTO shared_builds (id,name,archetype,archetype_name,primary_set,primary_name,secondary_set,secondary_name,level,build_json,visibility,user_id) VALUES
  ('pubf53','p','a','a','p','p','s','s',50,'{}','public',NULL),
  ('unlif53','p','a','a','p','p','s','s',50,'{}','unlisted',NULL),
  ('privf53','p','a','a','p','p','s','s',50,'{}','private','bbbbbbbb-0000-4000-8000-00000000f532'),
  ('minef53','p','a','a','p','p','s','s',50,'{}','private','bbbbbbbb-0000-4000-8000-00000000f531');

-- The fixture has no GoTrue, so the caller is stubbed in.
CREATE OR REPLACE FUNCTION auth.uid() RETURNS uuid AS $$
  SELECT 'bbbbbbbb-0000-4000-8000-00000000f531'::uuid;
$$ LANGUAGE sql STABLE;

-- increment_views identifies its caller from PostgREST's `request.headers`,
-- which a direct psql connection does not set -- and it counts NOTHING for a
-- caller it cannot identify (F33's second clause). So the GUC is stubbed here,
-- and without this line every count below is 0 and reads like a regression in
-- the visibility bound rather than an absent header.
SELECT set_config('request.headers', '{"cf-connecting-ip":"203.0.113.9"}', false);

\echo '=== F33: a private build must not be countable by a stranger'
SELECT increment_views('pubf53'), increment_views('unlif53'), increment_views('privf53');
SELECT id, visibility, views FROM shared_builds
 WHERE id IN ('pubf53','unlif53','privf53') ORDER BY id;

\echo '=== F53: public - expect INSERT 0 1'
INSERT INTO favorites (user_id, build_id) VALUES ('bbbbbbbb-0000-4000-8000-00000000f531','pubf53');
\echo '=== F53: unlisted - expect INSERT 0 1 (RLS alone would have refused this)'
INSERT INTO favorites (user_id, build_id) VALUES ('bbbbbbbb-0000-4000-8000-00000000f531','unlif53');
\echo '=== F53: own private - expect INSERT 0 1'
INSERT INTO favorites (user_id, build_id) VALUES ('bbbbbbbb-0000-4000-8000-00000000f531','minef53');
\echo '=== F53: another account private - expect ERROR Build not found'
INSERT INTO favorites (user_id, build_id) VALUES ('bbbbbbbb-0000-4000-8000-00000000f531','privf53');
\echo '=== F53: nonexistent - expect the SAME ERROR, not 23503'
INSERT INTO favorites (user_id, build_id) VALUES ('bbbbbbbb-0000-4000-8000-00000000f531','nosuchid99');


\echo '=== F33 second clause: the same viewer counting twice must move nothing'
-- One viewer, five clicks. The counter moved once above and must not move
-- again, which is the whole of F33's remaining clause: a public counter that
-- one machine in a loop could drive to any number.
SELECT increment_views('pubf53'), increment_views('pubf53'), increment_views('pubf53'),
       increment_views('pubf53'), increment_views('pubf53');
\echo '--- expect pubf53 views 1, and ONE metering row'
SELECT id, views FROM shared_builds WHERE id = 'pubf53';
SELECT count(*) AS metering_rows FROM build_views WHERE build_id = 'pubf53';

\echo '=== F33: a different address is a different viewer, and does count'
SELECT set_config('request.headers', '{"cf-connecting-ip":"198.51.100.4"}', false);
SELECT increment_views('pubf53');
\echo '--- expect pubf53 views 2'
SELECT id, views FROM shared_builds WHERE id = 'pubf53';

\echo '=== F33: the window reopens because the DIGEST carries the day'
-- Aging the `day` column is NOT what reopens it, and this test said so for one
-- run before being corrected: the primary key is (build_id, viewer), `viewer`
-- is a digest over today's date, so a row whose `day` was moved still collides
-- and still blocks. What reopens the window is that tomorrow's digest is a
-- different string. The `day` column only tells the purge job what to delete.
--
-- So: replace today's row with the one yesterday would have written, and watch
-- today's call sail past it.
DELETE FROM build_views WHERE build_id = 'pubf53';
INSERT INTO build_views (build_id, viewer, day) VALUES (
  'pubf53',
  encode(sha256(convert_to(((now() AT TIME ZONE 'utc')::date - 1)::text || '|203.0.113.9|pubf53', 'UTF8')), 'hex'),
  (now() AT TIME ZONE 'utc')::date - 1
);
SELECT set_config('request.headers', '{"cf-connecting-ip":"203.0.113.9"}', false);
SELECT increment_views('pubf53');
\echo '--- expect pubf53 views 3: yesterday-s row did not block today-s view'
SELECT id, views FROM shared_builds WHERE id = 'pubf53';

\echo '--- and the purge job removes exactly the stale one: expect 1 remaining'
DELETE FROM public.build_views WHERE day < (now() AT TIME ZONE 'utc')::date;
SELECT count(*) AS rows_after_purge FROM build_views WHERE build_id = 'pubf53';

\echo '=== F33: a caller with no address header is not counted at all'
SELECT set_config('request.headers', '{}', false);
SELECT increment_views('unlif53');
\echo '--- expect unlif53 views 1, unchanged from the block above'
SELECT id, views FROM shared_builds WHERE id = 'unlif53';

\echo '=== F33: x-forwarded-for is the fallback when cf-connecting-ip is absent'
SELECT set_config('request.headers', '{"x-forwarded-for":"192.0.2.44, 10.0.0.1"}', false);
SELECT increment_views('unlif53');
\echo '--- expect unlif53 views 2'
SELECT id, views FROM shared_builds WHERE id = 'unlif53';

\echo '=== F33: a private build leaves no metering row behind either'
SELECT set_config('request.headers', '{"cf-connecting-ip":"203.0.113.77"}', false);
SELECT increment_views('privf53');
\echo '--- expect 0 rows and views 0'
SELECT count(*) AS private_metering_rows FROM build_views WHERE build_id = 'privf53';
SELECT id, views FROM shared_builds WHERE id = 'privf53';

\echo '=== F33: no raw address is stored anywhere in the metering table'
\echo '--- expect 0'
SELECT count(*) AS rows_holding_an_address FROM build_views
 WHERE viewer LIKE '%203.0.113%' OR viewer LIKE '%198.51.100%' OR viewer LIKE '%192.0.2%';
