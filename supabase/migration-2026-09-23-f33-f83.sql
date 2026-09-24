-- Migration, 2026-09-23 — SECURITY_AUDIT.md F33 (second clause) and F83 (last clause).
--
-- Both blocks are lifted verbatim from `schema.sql`, which is where they live
-- permanently; this file exists so the change can be applied to a database
-- that already carries the rest, and is safe to delete once it has been. Run
-- it whole in the SQL editor. Everything in it is CREATE OR REPLACE, CREATE
-- TABLE IF NOT EXISTS, or a `cron.schedule` upsert on the job name, so
-- re-running it replaces rather than stacks.
--
-- One behaviour change to expect and watch: `increment_views` now identifies
-- its caller from PostgREST's `request.headers` and counts NOTHING for a
-- caller it cannot identify. If that GUC does not carry an address on this
-- deployment, view counting stops rather than degrades — which is the first
-- thing to measure after applying this, not the last.

-- ============================================
-- Migration: increment_views counts a viewer once a day, not once a click (F33)
-- ============================================
--
-- The second clause of F33, and the one the 2026-09-22 pass left open: anyone
-- who can view a public build could call this RPC as often as they liked, so
-- the counter was a number the caller chose. The row named the two honest
-- options -- a metering table, or accepting that a public vanity counter is
-- inflatable -- and this is the first.
--
-- **What it is, and what it is not.** This is de-duplication, not rate
-- limiting, which is what a view counter actually wants: the question "how
-- many people looked at this" is not answered better by counting one person's
-- refreshes. One viewer contributes at most 1 per build per UTC day, so
-- inflating a count by N now costs N distinct addresses or N days rather than
-- N requests. That is a bound. It is not a claim that the number cannot be
-- moved -- a botnet still moves it, and a public counter with no account
-- behind it never stops being approximate. What ends here is the version
-- anybody could move from one machine with a loop.
--
-- **No address is stored.** `viewer` is a SHA-256 over the day, the caller's
-- address and the build id, so the table cannot answer "what did this person
-- look at" even to somebody holding it: a row is unlinkable to any other row
-- for the same viewer on a different build, and yesterday's rows are deleted
-- rather than aged. Including the day in the digest is also what makes the
-- window a window -- at midnight UTC every viewer is new, with no rotation
-- step to get wrong.
--
-- **It fails closed on an unidentifiable caller, and that is a real cost.**
-- The address comes from PostgREST's `request.headers` GUC, which is set on a
-- call through the API and is ABSENT on a direct database connection. A caller
-- this function cannot identify is not counted, because the alternative is one
-- shared bucket that the first call of the day fills for everybody. So an RPC
-- invoked from psql silently does nothing, and `check-f33-f53-access.sql`
-- stubs the GUC for exactly this reason.
CREATE TABLE IF NOT EXISTS build_views (
  build_id TEXT NOT NULL,
  -- sha256(day | caller address | build id). Not an address, and not reversible
  -- to one without guessing the address -- which is cheap for IPv4, hence the
  -- daily delete rather than a long retention.
  viewer   TEXT NOT NULL,
  day      DATE NOT NULL,
  PRIMARY KEY (build_id, viewer)
);

-- RLS on with no policies = service role and the owner only, the same shape
-- rate_limits uses. The REVOKE is belt and braces against a Supabase project's
-- default privileges, which GRANT ALL on new public tables to anon and
-- authenticated: RLS already refuses them, and a grant nobody needs is still a
-- grant somebody has to reason about.
ALTER TABLE build_views ENABLE ROW LEVEL SECURITY;
REVOKE ALL ON public.build_views FROM anon, authenticated;

-- The window is a day, so anything from a previous one is dead weight. Same
-- shape and same reasoning as purge-rate-limits above, and the same reason for
-- no index on the predicate: the job deletes most of what it reads.
--
-- 41 past the hour rather than :17, to sit off the other purge.
SELECT cron.schedule(
  'purge-build-views',
  '41 * * * *',
  $purge$DELETE FROM public.build_views WHERE day < (now() AT TIME ZONE 'utc')::date$purge$
);

CREATE OR REPLACE FUNCTION increment_views(build_id TEXT)
RETURNS void AS $$
DECLARE
  -- The parameter under another name. `build_id` is also a COLUMN of
  -- build_views, and plpgsql raises on the ambiguity rather than guessing.
  target   TEXT := build_id;
  today    DATE := (now() AT TIME ZONE 'utc')::date;
  address  TEXT;
  digest   TEXT;
BEGIN
  -- Cloudflare sits in front of Supabase, so cf-connecting-ip is the address
  -- the edge saw and the one header a caller cannot choose. x-forwarded-for is
  -- the fallback and is a client-supplied list, so its first element is a
  -- claim -- which costs nothing here: a caller who forges it splits their own
  -- views across buckets they invented, which is the behaviour we already
  -- accept from someone holding several addresses.
  address := coalesce(
    current_setting('request.headers', true)::json->>'cf-connecting-ip',
    split_part(current_setting('request.headers', true)::json->>'x-forwarded-for', ',', 1)
  );
  -- No identifiable caller, no count. See the note above: the alternative is
  -- one bucket for the world.
  IF address IS NULL OR address = '' THEN
    RETURN;
  END IF;

  -- The visibility bound from the first half of F33, asked before anything is
  -- written: a private build must not even leave a metering row behind.
  PERFORM 1 FROM shared_builds
   WHERE id = target AND visibility IN ('public', 'unlisted');
  IF NOT FOUND THEN
    RETURN;
  END IF;

  digest := encode(sha256(convert_to(today::text || '|' || address || '|' || target, 'UTF8')), 'hex');

  -- ON CONSTRAINT rather than `ON CONFLICT (build_id, viewer)`: a conflict
  -- target's column list is resolved against plpgsql variables as well as
  -- columns, and `build_id` is both. That is an ambiguity error at RUN time,
  -- not at CREATE time, so the function was created happily and every call
  -- raised -- which is how the fixture caught it and reading would not have.
  INSERT INTO build_views (build_id, viewer, day)
  VALUES (target, digest, today)
  ON CONFLICT ON CONSTRAINT build_views_pkey DO NOTHING;

  -- FOUND is false when the conflict swallowed the insert, which is the whole
  -- mechanism: this viewer has already been counted for this build today.
  IF NOT FOUND THEN
    RETURN;
  END IF;

  UPDATE shared_builds
     SET views = views + 1
   WHERE id = target
     AND visibility IN ('public', 'unlisted');
END;
$$ LANGUAGE plpgsql SECURITY DEFINER SET search_path = public;


-- ============================================
-- Migration: the seed path runs the same identity rule as everything else (F83)
-- ============================================
--
-- F83's last open clause. `seed_profile_on_signup` writes a provider-supplied
-- name at signup and never passes through `update-profile`, so a Discord
-- global name reached `profiles.display_name` having met F82's sigil handling
-- -- trim, strip a leading `@` run, truncate by character -- and none of the
-- invisible-character classes: U+3164 and the HANGUL FILLER family, the bidi
-- overrides, the zero-width family, `\p{Zs}`.
--
-- The row said why nobody had written those here: "a plpgsql re-implementation
-- of the TypeScript rule would be a second copy of it in a second language,
-- which is the drift F10 and F85 were both filed for." That is right about a
-- COPY. What follows is not one -- `identity_name_normalized` below is
-- GENERATED from `_shared/author-name.ts` by putting all 1,114,112 code points
-- through `normalizeIdentityName` and recording what came back, so the two
-- halves cannot disagree about a character without the regeneration gate
-- saying so. The objection to a second implementation is an objection to one
-- that can drift, and a derived one can only be stale.
--
-- Three things this does NOT change:
--
--   * The truncation stays where it was. `seeded_display_name` still cuts at
--     30 and `update-profile` still REFUSES over-length rather than cutting --
--     the deliberate asymmetry F83 records, because a name silently shortened
--     in a form you are looking at is the kind of thing you find out later. A
--     seed has no form and no person in front of it, so it cuts.
--   * Rows already stored are not rewritten. F69's convention, stated on its
--     own row: the rule runs at write time. The 2026-09-22 census found 0
--     blank-renderers in this column and the one live claim was deleted, so
--     there is nothing here to rewrite in any case.
--   * The trigger is untouched. CREATE OR REPLACE FUNCTION swaps the body
--     under it.

-- >>> GENERATED by scripts/gen-identity-sql.mjs -- do not edit by hand
--
-- Derived from `supabase/functions/_shared/author-name.ts` by putting all
-- 1114112 code points through `normalizeIdentityName` and sorting them by
-- what came back. Regenerate with `node scripts/gen-identity-sql.mjs --write`;
-- `supabase/identity-sql-generated.test.ts` fails if this block and that file
-- have drifted apart.
--
-- 28 ranges removed outright, 9 collapsed to a space,
-- 750 strippable from the front of a name.
CREATE OR REPLACE FUNCTION identity_name_normalized(raw TEXT)
RETURNS TEXT AS $identity$
  SELECT btrim(
    -- 4. Whatever may not open a name comes off the front: the @ sigil, the
    --    blank openers, in any order and any number. The positive-class
    --    reasoning behind the set is in author-name.ts's OPENER comment; here
    --    it is just the answer.
    regexp_replace(
      -- 3. Collapse the runs the step below produced, then the outer btrim.
      regexp_replace(
        -- 2. The gaps become one space; the widthless ones vanish. Both in one
        --    pass each, and the order between them does not matter because a
        --    removed character cannot be what keeps two spaces apart once the
        --    collapse in step 3 runs after both.
        regexp_replace(
          regexp_replace(
            -- 1. NFC first, so nothing downstream measures a decomposed name.
            normalize(COALESCE(raw, ''), NFC),
            '[\u0000-\u0008\u000E-\u001F\u007F-\u009F\u00AD\u034F\u0600-\u0605\u061C\u06DD\u070F\u0890-\u0891\u08E2\u115F-\u1160\u17B4-\u17B5\u180B-\u180F\u200B-\u200F\u202A-\u202E\u2060-\u206F\u3164\uFE00-\uFE0F\uFEFF\uFFA0\uFFF0-\uFFFB\U000110BD\U000110CD\U00013430-\U0001343F\U0001BCA0-\U0001BCA3\U0001D173-\U0001D17A\U000E0000-\U000E0FFF]', '', 'g'),
          '[\u0009-\u000D\u0020\u00A0\u1680\u2002-\u200A\u2028-\u2029\u202F\u205F\u3000]', ' ', 'g'),
        '[ ]+', ' ', 'g'),
      '^[\u0000-\u0020\u0040\u007F-\u00A0\u00AD\u034F\u0378-\u0379\u0380-\u0383\u038B\u038D\u03A2\u0530\u0557-\u0558\u058B-\u058C\u0590\u05C8-\u05CF\u05EB-\u05EE\u05F5-\u0605\u061C\u06DD\u070E-\u070F\u074B-\u074C\u07B2-\u07BF\u07FB-\u07FC\u082E-\u082F\u083F\u085C-\u085D\u085F\u086B-\u086F\u0890-\u0896\u08E2\u0984\u098D-\u098E\u0991-\u0992\u09A9\u09B1\u09B3-\u09B5\u09BA-\u09BB\u09C5-\u09C6\u09C9-\u09CA\u09CF-\u09D6\u09D8-\u09DB\u09DE\u09E4-\u09E5\u09FF-\u0A00\u0A04\u0A0B-\u0A0E\u0A11-\u0A12\u0A29\u0A31\u0A34\u0A37\u0A3A-\u0A3B\u0A3D\u0A43-\u0A46\u0A49-\u0A4A\u0A4E-\u0A50\u0A52-\u0A58\u0A5D\u0A5F-\u0A65\u0A77-\u0A80\u0A84\u0A8E\u0A92\u0AA9\u0AB1\u0AB4\u0ABA-\u0ABB\u0AC6\u0ACA\u0ACE-\u0ACF\u0AD1-\u0ADF\u0AE4-\u0AE5\u0AF2-\u0AF8\u0B00\u0B04\u0B0D-\u0B0E\u0B11-\u0B12\u0B29\u0B31\u0B34\u0B3A-\u0B3B\u0B45-\u0B46\u0B49-\u0B4A\u0B4E-\u0B54\u0B58-\u0B5B\u0B5E\u0B64-\u0B65\u0B78-\u0B81\u0B84\u0B8B-\u0B8D\u0B91\u0B96-\u0B98\u0B9B\u0B9D\u0BA0-\u0BA2\u0BA5-\u0BA7\u0BAB-\u0BAD\u0BBA-\u0BBD\u0BC3-\u0BC5\u0BC9\u0BCE-\u0BCF\u0BD1-\u0BD6\u0BD8-\u0BE5\u0BFB-\u0BFF\u0C0D\u0C11\u0C29\u0C3A-\u0C3B\u0C45\u0C49\u0C4E-\u0C54\u0C57\u0C5B\u0C5E-\u0C5F\u0C64-\u0C65\u0C70-\u0C76\u0C8D\u0C91\u0CA9\u0CB4\u0CBA-\u0CBB\u0CC5\u0CC9\u0CCE-\u0CD4\u0CD7-\u0CDB\u0CDF\u0CE4-\u0CE5\u0CF0\u0CF4-\u0CFF\u0D0D\u0D11\u0D45\u0D49\u0D50-\u0D53\u0D64-\u0D65\u0D80\u0D84\u0D97-\u0D99\u0DB2\u0DBC\u0DBE-\u0DBF\u0DC7-\u0DC9\u0DCB-\u0DCE\u0DD5\u0DD7\u0DE0-\u0DE5\u0DF0-\u0DF1\u0DF5-\u0E00\u0E3B-\u0E3E\u0E5C-\u0E80\u0E83\u0E85\u0E8B\u0EA4\u0EA6\u0EBE-\u0EBF\u0EC5\u0EC7\u0ECF\u0EDA-\u0EDB\u0EE0-\u0EFF\u0F48\u0F6D-\u0F70\u0F98\u0FBD\u0FCD\u0FDB-\u0FFF\u10C6\u10C8-\u10CC\u10CE-\u10CF\u115F-\u1160\u1249\u124E-\u124F\u1257\u1259\u125E-\u125F\u1289\u128E-\u128F\u12B1\u12B6-\u12B7\u12BF\u12C1\u12C6-\u12C7\u12D7\u1311\u1316-\u1317\u135B-\u135C\u137D-\u137F\u139A-\u139F\u13F6-\u13F7\u13FE-\u13FF\u1680\u169D-\u169F\u16F9-\u16FF\u1716-\u171E\u1737-\u173F\u1754-\u175F\u176D\u1771\u1774-\u177F\u17B4-\u17B5\u17DE-\u17DF\u17EA-\u17EF\u17FA-\u17FF\u180B-\u180F\u181A-\u181F\u1879-\u187F\u18AB-\u18AF\u18F6-\u18FF\u191F\u192C-\u192F\u193C-\u193F\u1941-\u1943\u196E-\u196F\u1975-\u197F\u19AC-\u19AF\u19CA-\u19CF\u19DB-\u19DD\u1A1C-\u1A1D\u1A5F\u1A7D-\u1A7E\u1A8A-\u1A8F\u1A9A-\u1A9F\u1AAE-\u1AAF\u1ADE-\u1ADF\u1AEC-\u1AFF\u1B4D\u1BF4-\u1BFB\u1C38-\u1C3A\u1C4A-\u1C4C\u1C8B-\u1C8F\u1CBB-\u1CBC\u1CC8-\u1CCF\u1CFB-\u1CFF\u1F16-\u1F17\u1F1E-\u1F1F\u1F46-\u1F47\u1F4E-\u1F4F\u1F58\u1F5A\u1F5C\u1F5E\u1F7E-\u1F7F\u1FB5\u1FC5\u1FD4-\u1FD5\u1FDC\u1FF0-\u1FF1\u1FF5\u1FFF\u2002-\u200F\u2028-\u202F\u205F-\u206F\u2072-\u2073\u208F\u209D-\u209F\u20C2-\u20CF\u20F1-\u20FF\u218C-\u218F\u242A-\u243F\u244B-\u245F\u2800\u2B74-\u2B75\u2CF4-\u2CF8\u2D26\u2D28-\u2D2C\u2D2E-\u2D2F\u2D68-\u2D6E\u2D71-\u2D7E\u2D97-\u2D9F\u2DA7\u2DAF\u2DB7\u2DBF\u2DC7\u2DCF\u2DD7\u2DDF\u2E5E-\u2E7F\u2E9A\u2EF4-\u2EFF\u2FD6-\u2FEF\u3000\u3040\u3097-\u3098\u3100-\u3104\u3130\u3164\u318F\u31E6-\u31EE\u321F\uA48D-\uA48F\uA4C7-\uA4CF\uA62C-\uA63F\uA6F8-\uA6FF\uA7DD-\uA7F0\uA82D-\uA82F\uA83A-\uA83F\uA878-\uA87F\uA8C6-\uA8CD\uA8DA-\uA8DF\uA954-\uA95E\uA97D-\uA97F\uA9CE\uA9DA-\uA9DD\uA9FF\uAA37-\uAA3F\uAA4E-\uAA4F\uAA5A-\uAA5B\uAAC3-\uAADA\uAAF7-\uAB00\uAB07-\uAB08\uAB0F-\uAB10\uAB17-\uAB1F\uAB27\uAB2F\uAB6C-\uAB6F\uABEE-\uABEF\uABFA-\uABFF\uD7A4-\uD7AF\uD7C7-\uD7CA\uD7FC-\uF8FF\uFA6E-\uFA6F\uFADA-\uFAFF\uFB07-\uFB12\uFB18-\uFB1C\uFB37\uFB3D\uFB3F\uFB42\uFB45\uFDD0-\uFDEF\uFE00-\uFE0F\uFE1A-\uFE1F\uFE53\uFE67\uFE6C-\uFE6F\uFE75\uFEFD-\uFF00\uFFA0\uFFBF-\uFFC1\uFFC8-\uFFC9\uFFD0-\uFFD1\uFFD8-\uFFD9\uFFDD-\uFFDF\uFFE7\uFFEF-\uFFFB\uFFFE-\uFFFF\U0001000C\U00010027\U0001003B\U0001003E\U0001004E-\U0001004F\U0001005E-\U0001007F\U000100FB-\U000100FF\U00010103-\U00010106\U00010134-\U00010136\U0001018F\U0001019D-\U0001019F\U000101A1-\U000101CF\U000101FE-\U0001027F\U0001029D-\U0001029F\U000102D1-\U000102DF\U000102FC-\U000102FF\U00010324-\U0001032C\U0001034B-\U0001034F\U0001037B-\U0001037F\U0001039E\U000103C4-\U000103C7\U000103D6-\U000103FF\U0001049E-\U0001049F\U000104AA-\U000104AF\U000104D4-\U000104D7\U000104FC-\U000104FF\U00010528-\U0001052F\U00010564-\U0001056E\U0001057B\U0001058B\U00010593\U00010596\U000105A2\U000105B2\U000105BA\U000105BD-\U000105BF\U000105F4-\U000105FF\U00010737-\U0001073F\U00010756-\U0001075F\U00010768-\U0001077F\U00010786\U000107B1\U000107BB-\U000107FF\U00010806-\U00010807\U00010809\U00010836\U00010839-\U0001083B\U0001083D-\U0001083E\U00010856\U0001089F-\U000108A6\U000108B0-\U000108DF\U000108F3\U000108F6-\U000108FA\U0001091C-\U0001091E\U0001093A-\U0001093E\U0001095A-\U0001097F\U000109B8-\U000109BB\U000109D0-\U000109D1\U00010A04\U00010A07-\U00010A0B\U00010A14\U00010A18\U00010A36-\U00010A37\U00010A3B-\U00010A3E\U00010A49-\U00010A4F\U00010A59-\U00010A5F\U00010AA0-\U00010ABF\U00010AE7-\U00010AEA\U00010AF7-\U00010AFF\U00010B36-\U00010B38\U00010B56-\U00010B57\U00010B73-\U00010B77\U00010B92-\U00010B98\U00010B9D-\U00010BA8\U00010BB0-\U00010BFF\U00010C49-\U00010C7F\U00010CB3-\U00010CBF\U00010CF3-\U00010CF9\U00010D28-\U00010D2F\U00010D3A-\U00010D3F\U00010D66-\U00010D68\U00010D86-\U00010D8D\U00010D90-\U00010E5F\U00010E7F\U00010EAA\U00010EAE-\U00010EAF\U00010EB2-\U00010EC1\U00010EC8-\U00010ECF\U00010ED9-\U00010EF9\U00010F28-\U00010F2F\U00010F5A-\U00010F6F\U00010F8A-\U00010FAF\U00010FCC-\U00010FDF\U00010FF7-\U00010FFF\U0001104E-\U00011051\U00011076-\U0001107E\U000110BD\U000110C3-\U000110CF\U000110E9-\U000110EF\U000110FA-\U000110FF\U00011135\U00011148-\U0001114F\U00011177-\U0001117F\U000111E0\U000111F5-\U000111FF\U00011212\U00011242-\U0001127F\U00011287\U00011289\U0001128E\U0001129E\U000112AA-\U000112AF\U000112EB-\U000112EF\U000112FA-\U000112FF\U00011304\U0001130D-\U0001130E\U00011311-\U00011312\U00011329\U00011331\U00011334\U0001133A\U00011345-\U00011346\U00011349-\U0001134A\U0001134E-\U0001134F\U00011351-\U00011356\U00011358-\U0001135C\U00011364-\U00011365\U0001136D-\U0001136F\U00011375-\U0001137F\U0001138A\U0001138C-\U0001138D\U0001138F\U000113B6\U000113C1\U000113C3-\U000113C4\U000113C6\U000113CB\U000113D6\U000113D9-\U000113E0\U000113E3-\U000113FF\U0001145C\U00011462-\U0001147F\U000114C8-\U000114CF\U000114DA-\U0001157F\U000115B6-\U000115B7\U000115DE-\U000115FF\U00011645-\U0001164F\U0001165A-\U0001165F\U0001166D-\U0001167F\U000116BA-\U000116BF\U000116CA-\U000116CF\U000116E4-\U000116FF\U0001171B-\U0001171C\U0001172C-\U0001172F\U00011747-\U000117FF\U0001183C-\U0001189F\U000118F3-\U000118FE\U00011907-\U00011908\U0001190A-\U0001190B\U00011914\U00011917\U00011936\U00011939-\U0001193A\U00011947-\U0001194F\U0001195A-\U0001199F\U000119A8-\U000119A9\U000119D8-\U000119D9\U000119E5-\U000119FF\U00011A48-\U00011A4F\U00011AA3-\U00011AAF\U00011AF9-\U00011AFF\U00011B0A-\U00011B5F\U00011B68-\U00011BBF\U00011BE2-\U00011BEF\U00011BFA-\U00011BFF\U00011C09\U00011C37\U00011C46-\U00011C4F\U00011C6D-\U00011C6F\U00011C90-\U00011C91\U00011CA8\U00011CB7-\U00011CFF\U00011D07\U00011D0A\U00011D37-\U00011D39\U00011D3B\U00011D3E\U00011D48-\U00011D4F\U00011D5A-\U00011D5F\U00011D66\U00011D69\U00011D8F\U00011D92\U00011D99-\U00011D9F\U00011DAA-\U00011DAF\U00011DDC-\U00011DDF\U00011DEA-\U00011EDF\U00011EF9-\U00011EFF\U00011F11\U00011F3B-\U00011F3D\U00011F5B-\U00011FAF\U00011FB1-\U00011FBF\U00011FF2-\U00011FFE\U0001239A-\U000123FF\U0001246F\U00012475-\U0001247F\U00012544-\U00012F8F\U00012FF3-\U00012FFF\U00013430-\U0001343F\U00013456-\U0001345F\U000143FB-\U000143FF\U00014647-\U000160FF\U0001613A-\U000167FF\U00016A39-\U00016A3F\U00016A5F\U00016A6A-\U00016A6D\U00016ABF\U00016ACA-\U00016ACF\U00016AEE-\U00016AEF\U00016AF6-\U00016AFF\U00016B46-\U00016B4F\U00016B5A\U00016B62\U00016B78-\U00016B7C\U00016B90-\U00016D3F\U00016D7A-\U00016E3F\U00016E9B-\U00016E9F\U00016EB9-\U00016EBA\U00016ED4-\U00016EFF\U00016F4B-\U00016F4E\U00016F88-\U00016F8E\U00016FA0-\U00016FDF\U00016FE5-\U00016FEF\U00016FF7-\U00016FFF\U00018CD6-\U00018CFE\U00018D1F-\U00018D7F\U00018DF3-\U0001AFEF\U0001AFF4\U0001AFFC\U0001AFFF\U0001B123-\U0001B131\U0001B133-\U0001B14F\U0001B153-\U0001B154\U0001B156-\U0001B163\U0001B168-\U0001B16F\U0001B2FC-\U0001BBFF\U0001BC6B-\U0001BC6F\U0001BC7D-\U0001BC7F\U0001BC89-\U0001BC8F\U0001BC9A-\U0001BC9B\U0001BCA0-\U0001CBFF\U0001CCFD-\U0001CCFF\U0001CEB4-\U0001CEB9\U0001CED1-\U0001CEDF\U0001CEF1-\U0001CEFF\U0001CF2E-\U0001CF2F\U0001CF47-\U0001CF4F\U0001CFC4-\U0001CFFF\U0001D0F6-\U0001D0FF\U0001D127-\U0001D128\U0001D173-\U0001D17A\U0001D1EB-\U0001D1FF\U0001D246-\U0001D2BF\U0001D2D4-\U0001D2DF\U0001D2F4-\U0001D2FF\U0001D357-\U0001D35F\U0001D379-\U0001D3FF\U0001D455\U0001D49D\U0001D4A0-\U0001D4A1\U0001D4A3-\U0001D4A4\U0001D4A7-\U0001D4A8\U0001D4AD\U0001D4BA\U0001D4BC\U0001D4C4\U0001D506\U0001D50B-\U0001D50C\U0001D515\U0001D51D\U0001D53A\U0001D53F\U0001D545\U0001D547-\U0001D549\U0001D551\U0001D6A6-\U0001D6A7\U0001D7CC-\U0001D7CD\U0001DA8C-\U0001DA9A\U0001DAA0\U0001DAB0-\U0001DEFF\U0001DF1F-\U0001DF24\U0001DF2B-\U0001DFFF\U0001E007\U0001E019-\U0001E01A\U0001E022\U0001E025\U0001E02B-\U0001E02F\U0001E06E-\U0001E08E\U0001E090-\U0001E0FF\U0001E12D-\U0001E12F\U0001E13E-\U0001E13F\U0001E14A-\U0001E14D\U0001E150-\U0001E28F\U0001E2AF-\U0001E2BF\U0001E2FA-\U0001E2FE\U0001E300-\U0001E4CF\U0001E4FA-\U0001E5CF\U0001E5FB-\U0001E5FE\U0001E600-\U0001E6BF\U0001E6DF\U0001E6F6-\U0001E6FD\U0001E700-\U0001E7DF\U0001E7E7\U0001E7EC\U0001E7EF\U0001E7FF\U0001E8C5-\U0001E8C6\U0001E8D7-\U0001E8FF\U0001E94C-\U0001E94F\U0001E95A-\U0001E95D\U0001E960-\U0001EC70\U0001ECB5-\U0001ED00\U0001ED3E-\U0001EDFF\U0001EE04\U0001EE20\U0001EE23\U0001EE25-\U0001EE26\U0001EE28\U0001EE33\U0001EE38\U0001EE3A\U0001EE3C-\U0001EE41\U0001EE43-\U0001EE46\U0001EE48\U0001EE4A\U0001EE4C\U0001EE50\U0001EE53\U0001EE55-\U0001EE56\U0001EE58\U0001EE5A\U0001EE5C\U0001EE5E\U0001EE60\U0001EE63\U0001EE65-\U0001EE66\U0001EE6B\U0001EE73\U0001EE78\U0001EE7D\U0001EE7F\U0001EE8A\U0001EE9C-\U0001EEA0\U0001EEA4\U0001EEAA\U0001EEBC-\U0001EEEF\U0001EEF2-\U0001EFFF\U0001F02C-\U0001F02F\U0001F094-\U0001F09F\U0001F0AF-\U0001F0B0\U0001F0C0\U0001F0D0\U0001F0F6-\U0001F0FF\U0001F1AE-\U0001F1E5\U0001F203-\U0001F20F\U0001F23C-\U0001F23F\U0001F249-\U0001F24F\U0001F252-\U0001F25F\U0001F266-\U0001F2FF\U0001F6D9-\U0001F6DB\U0001F6ED-\U0001F6EF\U0001F6FD-\U0001F6FF\U0001F7DA-\U0001F7DF\U0001F7EC-\U0001F7EF\U0001F7F1-\U0001F7FF\U0001F80C-\U0001F80F\U0001F848-\U0001F84F\U0001F85A-\U0001F85F\U0001F888-\U0001F88F\U0001F8AE-\U0001F8AF\U0001F8BC-\U0001F8BF\U0001F8C2-\U0001F8CF\U0001F8D9-\U0001F8FF\U0001FA58-\U0001FA5F\U0001FA6E-\U0001FA6F\U0001FA7D-\U0001FA7F\U0001FA8B-\U0001FA8D\U0001FAC7\U0001FAC9-\U0001FACC\U0001FADD-\U0001FADE\U0001FAEB-\U0001FAEE\U0001FAF9-\U0001FAFF\U0001FB93\U0001FBFB-\U0001FFFF\U0002A6E0-\U0002A6FF\U0002B81E-\U0002B81F\U0002CEAE-\U0002CEAF\U0002EBE1-\U0002EBEF\U0002EE5E-\U0002F7FF\U0002FA1E-\U0002FFFF\U0003134B-\U0003134F\U0003347A-\U0010FFFF ]*', '')
  );
$identity$ LANGUAGE sql IMMUTABLE;
-- <<< GENERATED

-- The seed's own step, which is the generated rule plus the truncation that
-- belongs to this column and not to the concept.
CREATE OR REPLACE FUNCTION seeded_display_name(raw TEXT)
RETURNS TEXT AS $$
  SELECT left(identity_name_normalized(raw), 30);
$$ LANGUAGE sql IMMUTABLE;
