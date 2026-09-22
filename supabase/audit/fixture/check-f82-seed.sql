-- F82/F83 — the signup seed, executed rather than read.
--
-- Run against the fixture, which derives its schema from ../../schema.sql:
--   cd fixture && ./build.sh
--   docker exec -i sk-sqlcheck psql -U postgres -d skcheck -f - < check-f82-seed.sql
--
-- Expected, and each line is a separate claim:
--   over_the_cap   len 30, survived 1   -- the signup completes; before the fix
--                                          display_name_length fired and took the
--                                          auth.users INSERT with it
--   sigil          display_name savant  -- the leading @ is stripped, so a Discord
--                                          name cannot walk past F83's rule
--   astral         len 30, bytes 120    -- left() counts characters, so 30 astral
--                                          code points survive whole; a byte
--                                          truncation would have halved one
--
-- Re-running needs a fresh fixture: these ids persist.
INSERT INTO auth.users (id, email, raw_user_meta_data) VALUES
  ('aaaaaaaa-0000-4000-8000-00000000f821', 'long@example.com',
   jsonb_build_object('custom_claims', jsonb_build_object('global_name', repeat('x', 31)))),
  ('aaaaaaaa-0000-4000-8000-00000000f822', 'sigil@example.com',
   jsonb_build_object('custom_claims', jsonb_build_object('global_name', '@savant'))),
  ('aaaaaaaa-0000-4000-8000-00000000f823', 'astral@example.com',
   jsonb_build_object('custom_claims', jsonb_build_object('global_name', repeat(U&'\+01D400', 35))));

SELECT 'over_the_cap' AS case,
       char_length(p.display_name) AS len,
       octet_length(p.display_name) AS bytes,
       (SELECT count(*) FROM auth.users u WHERE u.id = p.user_id) AS survived,
       p.display_name
  FROM profiles p WHERE p.user_id = 'aaaaaaaa-0000-4000-8000-00000000f821'
UNION ALL
SELECT 'sigil', char_length(p.display_name), octet_length(p.display_name),
       (SELECT count(*) FROM auth.users u WHERE u.id = p.user_id), p.display_name
  FROM profiles p WHERE p.user_id = 'aaaaaaaa-0000-4000-8000-00000000f822'
UNION ALL
SELECT 'astral', char_length(p.display_name), octet_length(p.display_name),
       (SELECT count(*) FROM auth.users u WHERE u.id = p.user_id), '(35 U+1D400)'
  FROM profiles p WHERE p.user_id = 'aaaaaaaa-0000-4000-8000-00000000f823';
