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


-- ============================================
-- F83: the seed path now runs the same rule as everything else
-- ============================================
--
-- The clause F83 stayed PARTIAL on. Before 2026-09-23 the seed got F82's sigil
-- handling and none of the invisible classes, so every name below reached
-- `profiles.display_name` intact. Expected now: `claim` for all five, and a
-- `display_name` with no blank-renderer and no leading sigil in any of them.
--
-- U+3164 HANGUL FILLER is the one that matters most: it is a LETTER that
-- renders as nothing, so it walked past every subtractive rule that tried to
-- enumerate what an invisible is -- SECURITY_AUDIT.md F69's `\u3164@admin`.
INSERT INTO auth.users (id, email, raw_user_meta_data) VALUES
  ('aaaaaaaa-0000-4000-8000-00000000f831', 'filler@example.com',
   jsonb_build_object('custom_claims', jsonb_build_object('global_name', U&'\3164' || '@admin'))),
  ('aaaaaaaa-0000-4000-8000-00000000f832', 'spaced@example.com',
   jsonb_build_object('custom_claims', jsonb_build_object('global_name', '@ @savant'))),
  ('aaaaaaaa-0000-4000-8000-00000000f833', 'bidi@example.com',
   jsonb_build_object('custom_claims', jsonb_build_object('global_name', U&'\202E' || 'savant'))),
  ('aaaaaaaa-0000-4000-8000-00000000f834', 'zwsp@example.com',
   jsonb_build_object('custom_claims', jsonb_build_object('global_name', 'sav' || U&'\200B' || 'ant'))),
  ('aaaaaaaa-0000-4000-8000-00000000f835', 'nbsp@example.com',
   jsonb_build_object('custom_claims', jsonb_build_object('global_name', 'sav' || U&'\00A0' || U&'\00A0' || 'ant')));

\echo '=== F83: expect filler/spaced -> savant or admin, no sigil; the rest with the invisible gone'
SELECT CASE p.user_id::text
         WHEN 'aaaaaaaa-0000-4000-8000-00000000f831' THEN 'hangul_filler_then_sigil'
         WHEN 'aaaaaaaa-0000-4000-8000-00000000f832' THEN 'sigil_space_sigil'
         WHEN 'aaaaaaaa-0000-4000-8000-00000000f833' THEN 'bidi_override'
         WHEN 'aaaaaaaa-0000-4000-8000-00000000f834' THEN 'zero_width_space'
         WHEN 'aaaaaaaa-0000-4000-8000-00000000f835' THEN 'nbsp_run'
       END AS case,
       p.display_name,
       char_length(p.display_name) AS len,
       -- The two things that must be false of every row: it opens with a
       -- sigil, or it still carries something that renders as nothing.
       p.display_name LIKE '@%' AS still_claims_a_handle,
       p.display_name ~ ('[' || U&'\3164' || U&'\200B' || U&'\202E' || U&'\00A0' || ']') AS still_invisible
  FROM profiles p
 WHERE p.user_id::text LIKE 'aaaaaaaa-0000-4000-8000-00000000f83%'
 ORDER BY 1;
