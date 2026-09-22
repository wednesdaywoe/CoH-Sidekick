-- F33 / F53 — two SECURITY DEFINER paths that answered about rows the caller
-- cannot read, executed rather than read.
--
--   cd fixture && ./build.sh
--   docker exec -i sk-sqlcheck psql -U postgres -d skcheck -f - < check-f33-f53-access.sql
--
-- Expected:
--   F33  privf33 views 0; pubf33 and unlif33 views 1
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
