-- Q7 / F08 — the premise the row rests on: is `build-previews` actually public?
-- A public bucket serves every object to an unauthenticated GET regardless of RLS,
-- which is what makes Q5's count an exposure rather than a tidiness problem.
SELECT b.id                                                    AS bucket,
       b.public,
       b.file_size_limit,
       b.allowed_mime_types,
       b.created_at,
       (SELECT count(*) FROM storage.objects o WHERE o.bucket_id = b.id)   AS objects,
       (SELECT sum((o.metadata->>'size')::bigint) FROM storage.objects o
         WHERE o.bucket_id = b.id)                                         AS bytes,
       (SELECT string_agg(p.polname || ' [' || p.polcmd::text || '] ' ||
                          coalesce(pg_get_expr(p.polqual, p.polrelid), '-'), '  |  ')
          FROM pg_policy p WHERE p.polrelid = 'storage.objects'::regclass) AS storage_objects_policies
FROM storage.buckets b
ORDER BY b.id;
