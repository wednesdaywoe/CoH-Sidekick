-- Q3 / F34 — what the grant is actually exposing. The row names discord_id and
-- discord_username; this says how many rows carry them, and how much
-- discord_username adds over the display_name a card already draws.
SELECT count(*)                                                                AS profiles,
       count(handle)                                                           AS handles_claimed,
       count(*) FILTER (WHERE display_name <> '')                              AS with_display_name,
       count(discord_id)                                                       AS with_discord_id,
       count(discord_username)                                                 AS with_discord_username,
       count(DISTINCT discord_id)                                              AS distinct_discord_ids,
       count(avatar_url)                                                       AS with_avatar_url,
       count(*) FILTER (WHERE bio <> '')                                       AS with_bio,
       count(*) FILTER (WHERE discord_username IS NOT NULL
                          AND discord_username IS DISTINCT FROM display_name)  AS discord_username_differs_from_display_name,
       count(*) FILTER (WHERE handle IS NULL AND discord_id IS NOT NULL)       AS discord_id_on_an_unclaimed_profile,
       min(created_at)                                                         AS first_profile,
       max(created_at)                                                         AS latest_profile
FROM public.profiles;
