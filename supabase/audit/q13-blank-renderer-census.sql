-- Q13 / F69 (the thirteenth, and the new one) — the count the register records as
-- still unmeasured: author_name rows opening with a blank-rendering codepoint.
--
-- Q8 answers it with a named class of five, and a named class is the thing that
-- went stale twice already: the hidden_sigil query could not see U+3164, and the
-- row's own enumeration could not see U+FFA0 until a property found it. So this
-- one names nothing. It takes every author_name that does not open with an ASCII
-- alphanumeric, walks the leading run character by character, and reports the
-- codepoints themselves. A sixth blank-renderer shows up here as an unfamiliar
-- U+ value with a non-zero handle-shaped tail, without anybody having listed it.
--
-- The cost of naming nothing is that ordinary non-ASCII letters appear too — a
-- name opening with "Æ" or a CJK character is in this output and is not a finding.
-- Read the U+ column, not the row count: the run is capped at six characters so
-- a wholly non-Latin name contributes a bounded prefix rather than itself.
--
-- rows_whose_tail_is_handle_shaped is the payload. A blank-renderer with a
-- handle-shaped tail behind it is F69's bypass in stored data; the same codepoint
-- with a zero there is somebody's punctuation.
--
-- profiles.display_name rides along under `source` — F83 has no stale-proof
-- census of its own and the second column renders in the same author surfaces.
--
-- Two limits, both deliberate. This is a census of LEADING RUNS only: a
-- blank-renderer buried mid-name cannot manufacture a leading sigil, and Q8's
-- contains_blank_renderer_anywhere covers that case for the named class. And
-- NO COLUMN HERE EMITS A STORED NAME. An earlier version returned min(value)
-- as a convenience sample; it put nine real display names into the committed
-- transcripts, which is user data this repo should not carry. The codepoint
-- and U+ columns are the identification, shortest_len/longest_len bound the
-- shape, and rows_whose_tail_is_handle_shaped carries the security signal --
-- together they answer everything the sample was there for.
WITH named AS (
  SELECT 'shared_builds.author_name'                                   AS source,
         b.id                                                          AS row_key,
         b.author_name                                                 AS value,
         left(substring(b.author_name FROM '^[^A-Za-z0-9]*'), 6)       AS run,
         lower(regexp_replace(b.author_name, '^[^A-Za-z0-9]+', ''))    AS tail
  FROM public.shared_builds b
  WHERE b.author_name IS NOT NULL
    AND b.author_name <> ''
    AND b.author_name ~ '^[^A-Za-z0-9]'
  UNION ALL
  SELECT 'profiles.display_name',
         p.user_id::text,
         p.display_name,
         left(substring(p.display_name FROM '^[^A-Za-z0-9]*'), 6),
         lower(regexp_replace(p.display_name, '^[^A-Za-z0-9]+', ''))
  FROM public.profiles p
  WHERE p.display_name <> ''
    AND p.display_name ~ '^[^A-Za-z0-9]'
),
chars AS (
  SELECT n.source, n.row_key, n.value, n.tail,
         g.ord,
         substring(n.run FROM g.ord FOR 1) AS ch
  FROM named n
  CROSS JOIN LATERAL generate_series(1, char_length(n.run)) AS g(ord)
)
SELECT source,
       ascii(ch)                                          AS codepoint,
       'U+' || upper(lpad(to_hex(ascii(ch)),
                       greatest(4, length(to_hex(ascii(ch)))), '0')) AS u,
       min(ord)                                           AS earliest_position_in_run,
       count(*)                                           AS occurrences,
       count(DISTINCT row_key)                            AS n_rows,
       count(DISTINCT row_key) FILTER (WHERE tail ~ '^[a-z0-9][a-z0-9_-]{2,29}$')
                                                          AS rows_whose_tail_is_handle_shaped,
       min(length(value))                                 AS shortest_len,
       max(length(value))                                 AS longest_len
FROM chars
GROUP BY source, ascii(ch)
ORDER BY rows_whose_tail_is_handle_shaped DESC, n_rows DESC, source, codepoint;
