-- Q4 / F34 — the second door. "Fix it with `search_authors` in the same change
-- or it closes one door of two": search_authors is a SQL function over `profiles`,
-- so narrowing the table grant does nothing to what this RPC hands back.
-- Both halves matter: WHO may execute it, and HOW MUCH of the table it returns.
-- q = '' degenerates to ILIKE '%%', i.e. every row — that is the enumeration test.
-- All three calls are STABLE SELECTs; nothing here writes.
SELECT p.proname                                            AS function_name,
       p.prosecdef                                          AS security_definer,
       p.provolatile                                        AS volatility,
       has_function_privilege('anon',          p.oid, 'EXECUTE') AS anon_execute,
       has_function_privilege('authenticated', p.oid, 'EXECUTE') AS auth_execute,
       pg_get_function_result(p.oid)                        AS returns,
       (SELECT count(*) FROM public.profiles)                                      AS profiles_total,
       (SELECT count(*) FROM public.search_authors('',  1000000))                  AS returned_for_empty_q,
       (SELECT count(*) FROM public.search_authors('a', 1000000))                  AS returned_for_a,
       (SELECT count(*) FROM public.search_authors('',  1000000) WHERE build_count = 0)
                                                                                   AS returned_with_no_public_builds
FROM pg_proc p
WHERE p.pronamespace = 'public'::regnamespace
  AND p.proname IN ('search_authors', 'resolve_author')
ORDER BY p.proname;
