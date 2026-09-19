-- Q6 / F08 — "and never removed". Four populations, because the row pointer and
-- the object can go out of step in both directions:
--   objects nothing points at, objects whose build no longer exists, and
--   rows pointing at an object that is gone.
-- The build id is recovered from the object name because share-build writes
-- exactly `previews/<id>.png` (share-build/index.ts:132).
SELECT (SELECT count(*) FROM storage.objects WHERE bucket_id = 'build-previews')
         AS objects_in_bucket,
       (SELECT count(*) FROM storage.objects o
         WHERE o.bucket_id = 'build-previews'
           AND NOT EXISTS (SELECT 1 FROM public.shared_builds b
                            WHERE b.preview_image_path = o.name))
         AS objects_no_row_points_at,
       (SELECT count(*) FROM storage.objects o
         WHERE o.bucket_id = 'build-previews'
           AND NOT EXISTS (SELECT 1 FROM public.shared_builds b
                            WHERE b.id = regexp_replace(o.name, '^previews/(.*)\.png$', '\1')))
         AS objects_whose_build_row_is_gone,
       (SELECT count(*) FROM public.shared_builds b
         WHERE b.preview_image_path IS NOT NULL
           AND NOT EXISTS (SELECT 1 FROM storage.objects o
                            WHERE o.bucket_id = 'build-previews' AND o.name = b.preview_image_path))
         AS rows_pointing_at_a_missing_object,
       (SELECT count(*) FROM storage.objects o
          JOIN public.shared_builds b ON b.preview_image_path = o.name
         WHERE o.bucket_id = 'build-previews' AND b.visibility <> 'public')
         AS objects_belonging_to_a_nonpublic_build;
