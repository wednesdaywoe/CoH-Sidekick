-- Q12 / F82 — the band. seed_profile_on_signup writes the provider name straight
-- into a column with CHECK (char_length(display_name) <= 30) (schema.sql:339-363,
-- constraint at :312), and the trigger is AFTER INSERT ON auth.users, so the CHECK
-- failing aborts the whole signup.
--
-- Nobody who hit this leaves a row behind — a failed signup is invisible by
-- construction, and the constraint makes it so: reproduced against the real
-- schema.sql on a throwaway Postgres, the profiles INSERT raises
-- display_name_length and takes the auth.users INSERT down with it.
--
-- So a row here with would_seed > 30 did NOT sign up with that name. It got it
-- afterwards: raw_user_meta_data is refreshed on every login, and the trigger is
-- AFTER INSERT only, so a Discord rename lands in the column without re-running
-- the CHECK. Such a user is already in and stays in; what they cannot do is sign
-- up again. That is what makes a non-zero count possible, and what it means.
--
-- What CAN be measured is how close the existing population is:
-- re-derive the COALESCE chain over every existing auth.users row and look at the
-- length distribution. would_abort_today > 0 means the constraint is already
-- reachable with names this project has actually seen.
WITH seeded AS (
  SELECT u.id,
         u.created_at,
         coalesce(
           nullif(u.raw_user_meta_data->'custom_claims'->>'global_name', ''),
           nullif(u.raw_user_meta_data->>'full_name', ''),
           nullif(u.raw_user_meta_data->>'name', ''),
           nullif(split_part(coalesce(u.email, ''), '@', 1), ''),
           ''
         ) AS would_seed
  FROM auth.users u
)
SELECT count(*)                                                          AS auth_users,
       count(*) FILTER (WHERE char_length(would_seed) > 30)              AS would_abort_signup_today,
       count(*) FILTER (WHERE char_length(would_seed) BETWEEN 26 AND 30) AS within_5_of_the_cap,
       count(*) FILTER (WHERE would_seed = '')                           AS seeds_empty,
       max(char_length(would_seed))                                      AS longest_would_seed,
       round(avg(char_length(would_seed)), 1)                            AS mean_would_seed,
       (SELECT count(*) FROM auth.users u
          LEFT JOIN public.profiles p ON p.user_id = u.id
         WHERE p.user_id IS NULL)                                        AS users_with_no_profile_row,
       (SELECT count(*) FROM public.profiles WHERE char_length(display_name) = 30)
                                                                         AS profiles_sitting_exactly_at_the_cap
FROM seeded;
