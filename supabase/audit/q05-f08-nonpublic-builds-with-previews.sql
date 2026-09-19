-- Q5 / F08 — private and unlisted builds whose preview PNG is sitting in the
-- public bucket. `update-build-visibility` has no preview handling at all, so a
-- build flipped to private after sharing keeps both its row pointer and its object.
-- object_present is the one that matters: a path on the row is a claim, an object
-- in the bucket is the exposure.
SELECT b.visibility,
       count(*)                          AS builds,
       count(b.preview_image_path)       AS rows_with_a_preview_path,
       count(o.id)                       AS object_present_in_bucket,
       count(*) FILTER (WHERE b.user_id IS NULL) AS anonymous_shares,
       min(b.created_at)                 AS oldest,
       max(b.updated_at)                 AS newest_touch
FROM public.shared_builds b
LEFT JOIN storage.objects o
       ON o.bucket_id = 'build-previews'
      AND o.name = b.preview_image_path
GROUP BY b.visibility
ORDER BY b.visibility;
