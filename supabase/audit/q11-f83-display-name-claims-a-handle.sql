-- Q11 / F83 — whether a display_name already claims an identity it does not hold.
-- F69 deliberately does NOT check display_name collisions on the share path
-- ("display names are not unique and never were"), and that argument is about
-- collision. This asks the other question: how many display_names, after the
-- leading run comes off, ARE a handle a different account holds — which is a
-- claim rather than a collision, and is what F83 says is unmeasured.
WITH cand AS (
  SELECT p.user_id,
         p.handle                                                    AS own_handle,
         p.display_name,
         p.display_name ~ '^[^A-Za-z0-9]'                            AS has_leading_run,
         lower(regexp_replace(p.display_name, '^[^A-Za-z0-9]+', '')) AS tail
  FROM public.profiles p
  WHERE p.display_name <> ''
)
SELECT count(*) FILTER (WHERE c.tail ~ '^[a-z0-9][a-z0-9_-]{2,29}$')  AS tail_is_handle_shaped,
       count(*) FILTER (WHERE c.has_leading_run)                      AS has_a_leading_run,
       count(*) FILTER (WHERE h.user_id IS NOT NULL)                  AS names_a_real_handle,
       count(*) FILTER (WHERE h.user_id = c.user_id)                  AS names_own_handle,
       count(*) FILTER (WHERE h.user_id IS NOT NULL
                          AND h.user_id <> c.user_id)                 AS names_ANOTHER_accounts_handle,
       count(*) FILTER (WHERE h.user_id IS NOT NULL
                          AND h.user_id <> c.user_id
                          AND c.has_leading_run)                      AS and_behind_a_leading_run,
       count(*) FILTER (WHERE r.handle IS NOT NULL AND r.reason = 'system')
                                                                      AS names_a_system_reserved_handle,
       count(*) FILTER (WHERE c.own_handle IS NULL AND h.user_id IS NOT NULL)
                                                                      AS claimed_by_a_profile_with_no_handle
FROM cand c
LEFT JOIN public.profiles         h ON h.handle = c.tail::citext
LEFT JOIN public.reserved_handles r ON r.handle = c.tail::citext;
