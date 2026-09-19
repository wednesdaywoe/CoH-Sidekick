-- Q14 / F34 -- would a column REVOKE on profiles actually bind, or does a view route around it?
-- shared_builds_with_author is anon-SELECTable, has no RLS of its own, and draws author_handle,
-- author_display_name and author_avatar_url out of profiles. A view owned by postgres WITHOUT
-- security_invoker runs with the OWNER's privileges, which would leave those three columns
-- readable after the REVOKE and make the migration a no-op on the surface that matters most.
-- Added by the session rather than planned: the thirteen measured the table and the functions,
-- and a view that could have invalidated the whole fix was not among them.
-- Answer on 2026-09-18: security_invoker=on, so it runs as the caller and the REVOKE binds.
SELECT c.relname                                         AS view_name,
       pg_get_userbyid(c.relowner)                       AS owner,
       c.reloptions                                      AS reloptions,
       (c.reloptions::text[] @> ARRAY['security_invoker=true'])
         OR (c.reloptions::text[] @> ARRAY['security_invoker=on'])
                                                         AS security_invoker,
       has_table_privilege('anon', c.oid, 'SELECT')      AS anon_select,
       (SELECT string_agg(a.attname, ', ' ORDER BY a.attnum)
          FROM pg_attribute a
         WHERE a.attrelid = c.oid AND a.attnum > 0 AND NOT a.attisdropped)
                                                         AS columns
  FROM pg_class c
  JOIN pg_namespace n ON n.oid = c.relnamespace
 WHERE n.nspname = 'public' AND c.relkind = 'v';
