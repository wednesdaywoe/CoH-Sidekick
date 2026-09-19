-- Q8 / F69 — the documented measurement, re-run, with the classes it cannot see
-- placed beside it. open_sigil and hidden_sigil are exactly the query in
-- crates/app/src/cloud/profile.rs's AuthorIdentity doc comment; the three columns
-- after them are what the reopening found, and the gap between hidden_sigil and
-- leading_run_has_blank_renderer IS the finding.
--
-- Written with Postgres E'' escapes rather than the characters themselves, per
-- that doc comment: a blank-rendering character pasted into a source file is
-- invisible to the next reader of it.
--   U+3164 HANGUL FILLER, U+115F / U+1160 (Lo), U+FFA0 HALFWIDTH HANGUL FILLER,
--   U+2800 BRAILLE PATTERN BLANK (So).
-- The blank-renderer may sit ANYWHERE in the leading run (`@` U+3164 `@admin`),
-- so the pattern steps over the run rather than anchoring on character one.
SELECT count(*)                                                            AS rows_total,
       count(*) FILTER (WHERE author_name IS NOT NULL AND author_name <> '') AS named,
       count(*) FILTER (WHERE author_name ~ '^[@[:space:]]')               AS open_sigil,
       count(*) FILTER (WHERE author_name ~ E'[\u200b-\u200f\u202a-\u202e\u2066-\u2069\ufeff]')
                                                                           AS hidden_sigil,
       count(*) FILTER (WHERE author_name ~ E'^[^A-Za-z0-9]*[\u3164\u115f\u1160\uffa0\u2800]')
                                                                           AS leading_run_has_blank_renderer,
       count(*) FILTER (WHERE author_name ~ E'[\u3164\u115f\u1160\uffa0\u2800]')
                                                                           AS contains_blank_renderer_anywhere,
       count(*) FILTER (WHERE author_name <> '' AND author_name ~ '^[^A-Za-z0-9]')
                                                                           AS opens_with_a_leading_run,
       count(*) FILTER (WHERE author_name <> '' AND author_name IS DISTINCT FROM btrim(author_name))
                                                                           AS untrimmed
FROM public.shared_builds;
