-- Q1 / F34 — which columns can `anon` and `authenticated` actually read?
-- has_column_privilege is the only answer that cannot be wrong: a column GRANT
-- is ignored while a table GRANT stands (F73's mechanism), and information_schema
-- shows the grant rather than the effect.
-- Read `profiles` against `shared_builds`: shared_builds is the post-F73 state
-- (21 granted columns, owner_token_hash withheld); profiles is the untreated one.
SELECT c.relname                                                     AS relation,
       a.attnum                                                      AS ord,
       a.attname                                                     AS column_name,
       has_column_privilege('anon',          c.oid, a.attnum, 'SELECT') AS anon_select,
       has_column_privilege('authenticated', c.oid, a.attnum, 'SELECT') AS auth_select
FROM pg_class c
JOIN pg_attribute a ON a.attrelid = c.oid
WHERE c.relnamespace = 'public'::regnamespace
  AND c.relname IN ('profiles', 'shared_builds', 'shared_builds_with_author',
                    'reserved_handles', 'favorites', 'rate_limits')
  AND a.attnum > 0
  AND NOT a.attisdropped
ORDER BY c.relname, a.attnum;
