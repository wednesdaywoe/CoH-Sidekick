-- Q9 / F69 — the impersonation population, generalised past the sigil.
-- The stored rows predate sanitizeAuthorName and were never rewritten, so this
-- is the residual the client rule has to carry. Strip the whole leading run of
-- non-ASCII-alphanumerics (sigil, space, or blank-renderer, in any order — the
-- same subsumption OPENER/opens_a_name does) and ask whether what is left is a
-- handle somebody holds.
--
-- The NULL split is deliberate: an earlier draft of F69's row joined with
-- `IS DISTINCT FROM` on a NULLABLE user_id, so an anonymous share counted as
-- "a different account". Anonymous and other-account are separate columns here.
WITH cand AS (
  SELECT b.id,
         b.user_id,
         b.visibility,
         b.author_name,
         b.author_name ~ '^[^A-Za-z0-9]'                              AS has_leading_run,
         lower(regexp_replace(b.author_name, '^[^A-Za-z0-9]+', ''))   AS tail
  FROM public.shared_builds b
  WHERE b.author_name IS NOT NULL AND b.author_name <> ''
)
SELECT count(*) FILTER (WHERE c.tail ~ '^[a-z0-9][a-z0-9_-]{2,29}$')      AS tail_is_handle_shaped,
       count(*) FILTER (WHERE c.has_leading_run)                          AS has_a_leading_run,
       count(*) FILTER (WHERE p.user_id IS NOT NULL)                      AS names_a_real_handle,
       count(*) FILTER (WHERE p.user_id IS NOT NULL AND c.user_id IS NULL)
                                                                          AS anonymous_share_naming_a_handle,
       count(*) FILTER (WHERE p.user_id IS NOT NULL AND c.user_id = p.user_id)
                                                                          AS signed_in_naming_own_handle,
       count(*) FILTER (WHERE p.user_id IS NOT NULL AND c.user_id IS NOT NULL
                          AND c.user_id <> p.user_id)                     AS signed_in_naming_ANOTHER_handle,
       count(*) FILTER (WHERE p.user_id IS NOT NULL AND c.has_leading_run) AS and_behind_a_leading_run,
       count(*) FILTER (WHERE r.handle IS NOT NULL AND r.reason = 'system')
                                                                          AS names_a_system_reserved_handle,
       count(*) FILTER (WHERE r.handle IS NOT NULL AND r.reason <> 'system')
                                                                          AS names_a_route_or_sentinel_handle
FROM cand c
LEFT JOIN public.profiles         p ON p.handle = c.tail::citext
LEFT JOIN public.reserved_handles r ON r.handle = c.tail::citext;
