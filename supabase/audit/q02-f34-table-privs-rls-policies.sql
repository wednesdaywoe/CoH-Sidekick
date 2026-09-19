-- Q2 / F34 — the table-level grant, the RLS flag, and the policy expression,
-- for every relation in `public`. This is where `profiles`' "Public read profiles
-- ... USING (TRUE)" sits beside shared_builds' visibility predicate, and where a
-- table-wide SELECT that would swallow any column grant shows up.
SELECT c.relname                                              AS relation,
       c.relkind                                              AS kind,
       c.relrowsecurity                                       AS rls_enabled,
       c.relforcerowsecurity                                  AS rls_forced,
       has_table_privilege('anon',          c.oid, 'SELECT')  AS anon_table_select,
       has_table_privilege('authenticated', c.oid, 'SELECT')  AS auth_table_select,
       has_table_privilege('anon',          c.oid, 'INSERT')  AS anon_insert,
       has_table_privilege('anon',          c.oid, 'UPDATE')  AS anon_update,
       has_table_privilege('anon',          c.oid, 'DELETE')  AS anon_delete,
       (SELECT string_agg(p.polname || ' [' || p.polcmd::text || '] ' ||
                          coalesce(pg_get_expr(p.polqual, p.polrelid), '-'), '  |  ')
          FROM pg_policy p WHERE p.polrelid = c.oid)          AS policies,
       c.relacl::text                                         AS acl
FROM pg_class c
WHERE c.relnamespace = 'public'::regnamespace
  AND c.relkind IN ('r', 'v', 'm', 'p')
ORDER BY c.relname;
