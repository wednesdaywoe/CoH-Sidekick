-- Adversarial fixtures. Blank-renderers written as E'' escapes for the same
-- reason the queries are: a literal one is invisible in this file.
INSERT INTO auth.users (id, email, raw_user_meta_data) VALUES
  ('11111111-1111-1111-1111-111111111111', 'wednesday@example.com',
   '{"custom_claims":{"global_name":"Wednesday Woe"},"provider_id":"1001","full_name":"wednesdaywoe","avatar_url":"https://cdn.example/a.png"}'),
  ('22222222-2222-2222-2222-222222222222', 'tiger@example.com',
   '{"custom_claims":{"global_name":"TigerEyes"},"provider_id":"1002","full_name":"tigereyes"}'),
  ('33333333-3333-3333-3333-333333333333', 'longname@example.com',
   '{"custom_claims":{"global_name":"Short Then"},"provider_id":"1003"}'),
  ('44444444-4444-4444-4444-444444444444', 'plain@example.com', '{}');

UPDATE auth.users
   SET raw_user_meta_data = jsonb_set(raw_user_meta_data, '{custom_claims,global_name}',
                                      '"A Global Name Far Longer Than Thirty Characters"')
 WHERE id = '33333333-3333-3333-3333-333333333333';

UPDATE public.profiles SET handle = 'wednesdaywoe' WHERE user_id = '11111111-1111-1111-1111-111111111111';
UPDATE public.profiles SET handle = 'tigereyes'    WHERE user_id = '22222222-2222-2222-2222-222222222222';
-- a display_name that claims another account's handle, hidden behind U+3164
UPDATE public.profiles SET display_name = E'\u3164@tigereyes' WHERE user_id = '44444444-4444-4444-4444-444444444444';
-- a display_name sitting exactly at the cap
UPDATE public.profiles SET display_name = repeat('x', 30) WHERE user_id = '33333333-3333-3333-3333-333333333333';

INSERT INTO public.shared_builds
  (id, name, archetype, archetype_name, primary_set, primary_name, secondary_set, secondary_name,
   author_name, build_json, user_id, visibility, preview_image_path, owner_token_hash)
VALUES
  ('b01','Clean','blaster','Blaster','fire','Fire','dev','Dev', 'Wednesday Woe', '{}', '11111111-1111-1111-1111-111111111111','public','previews/b01.png','abc'),
  ('b02','Sigil','blaster','Blaster','fire','Fire','dev','Dev', '@admin',        '{}', NULL,'public','previews/b02.png','abc'),
  ('b03','SpaceSigil','blaster','Blaster','fire','Fire','dev','Dev', '@ @admin',  '{}', NULL,'public',NULL,'abc'),
  -- U+3164 HANGUL FILLER, the reopening's bypass
  ('b04','Filler','blaster','Blaster','fire','Fire','dev','Dev', E'\u3164@admin', '{}', NULL,'public',NULL,'abc'),
  -- U+2800 BRAILLE PATTERN BLANK, then a handle a DIFFERENT account holds
  ('b05','Braille','blaster','Blaster','fire','Fire','dev','Dev', E'\u2800@tigereyes','{}','11111111-1111-1111-1111-111111111111','public',NULL,'abc'),
  -- U+FFA0 HALFWIDTH HANGUL FILLER, the one the property caught and no list had
  ('b06','Halfwidth','blaster','Blaster','fire','Fire','dev','Dev', E'\uffa0@wednesdaywoe','{}',NULL,'public',NULL,'abc'),
  -- zero-width joiner class: what the documented hidden_sigil query CAN see
  ('b07','ZeroWidth','blaster','Blaster','fire','Fire','dev','Dev', E'\u200b@admin','{}',NULL,'public',NULL,'abc'),
  -- signed-in account naming its OWN handle: not a claim
  ('b08','Own','blaster','Blaster','fire','Fire','dev','Dev', '@tigereyes','{}','22222222-2222-2222-2222-222222222222','public',NULL,'abc'),
  -- ordinary non-ASCII openers: must appear in the census and NOT be findings
  ('b09','Accent','blaster','Blaster','fire','Fire','dev','Dev', E'Ævar the Bold','{}',NULL,'public',NULL,'abc'),
  ('b10','CJK','blaster','Blaster','fire','Fire','dev','Dev', E'日本語','{}',NULL,'public',NULL,'abc'),
  -- F08: non-public builds whose preview PNG is in the public bucket
  ('b11','Private','blaster','Blaster','fire','Fire','dev','Dev', 'Someone','{}','11111111-1111-1111-1111-111111111111','private','previews/b11.png','abc'),
  ('b12','Unlisted','blaster','Blaster','fire','Fire','dev','Dev', '','{}','22222222-2222-2222-2222-222222222222','unlisted','previews/b12.png','abc'),
  -- a row pointing at an object that is gone
  ('b13','MissingObj','blaster','Blaster','fire','Fire','dev','Dev', 'Nobody','{}',NULL,'public','previews/b13.png','abc');

INSERT INTO storage.objects (bucket_id, name) VALUES
  ('build-previews','previews/b01.png'),
  ('build-previews','previews/b02.png'),
  ('build-previews','previews/b11.png'),
  ('build-previews','previews/b12.png'),
  ('build-previews','previews/b99.png');   -- orphan: no build row at all
