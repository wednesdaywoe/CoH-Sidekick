-- Q10 / F83 — the same census on the second free-text identity string.
-- update-profile does `String(body.display_name).trim()` and a 30-char cap and
-- nothing else (update-profile/index.ts:96-101); F69's rule was scoped to a
-- column, not to the concept, and this column renders in the same author surfaces
-- (browser.rs:1215-1226, BuildFilters.tsx:257).
--
-- JS .trim() removes U+00A0 and U+3000 but no Lo/So blank-renderer, so untrimmed
-- and leading_run_has_blank_renderer are asking different questions here.
SELECT count(*)                                                            AS profiles,
       count(*) FILTER (WHERE display_name <> '')                          AS named,
       count(*) FILTER (WHERE display_name ~ '^[@[:space:]]')              AS open_sigil,
       count(*) FILTER (WHERE display_name ~ E'[\u200b-\u200f\u202a-\u202e\u2066-\u2069\ufeff]')
                                                                           AS hidden_sigil,
       count(*) FILTER (WHERE display_name ~ E'^[^A-Za-z0-9]*[\u3164\u115f\u1160\uffa0\u2800]')
                                                                           AS leading_run_has_blank_renderer,
       count(*) FILTER (WHERE display_name ~ E'[\u3164\u115f\u1160\uffa0\u2800]')
                                                                           AS contains_blank_renderer_anywhere,
       count(*) FILTER (WHERE display_name <> '' AND display_name ~ '^[^A-Za-z0-9]')
                                                                           AS opens_with_a_leading_run,
       count(*) FILTER (WHERE display_name IS DISTINCT FROM btrim(display_name))
                                                                           AS untrimmed,
       count(*) FILTER (WHERE char_length(display_name) = 30)              AS at_the_30_char_cap
FROM public.profiles;
