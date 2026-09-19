/**
 * Grades `storableAvatarUrl` and the three places the same rule now lives —
 * SECURITY_AUDIT.md F07.
 *
 * The rule has to survive two opposite mistakes. A rule that refuses too much
 * blanks 452 real avatars, which is a visible product regression and the reason
 * the allow-list is measured rather than guessed. A rule that admits too much is
 * the finding: the column is rendered raw into an `<img src>` for strangers, so
 * whoever chooses the host learns every viewer's IP and user-agent, and on the
 * desktop client a relative url reaches an asset resolver that aborts the
 * process on a malformed percent-escape (F81).
 */

import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { storableAvatarUrl, AVATAR_HOST } from './avatar-url';

const real = 'https://cdn.discordapp.com/avatars/123456789/a1b2c3d4.png';

describe('storableAvatarUrl', () => {
  it('stores the avatars that actually exist', () => {
    // The half a "return null always" implementation would fail, which is the
    // half that decides whether this rule can ship. All 452 stored avatars in
    // production are this shape.
    for (const url of [
      real,
      `${real}?size=128`,
      'https://cdn.discordapp.com/embed/avatars/3.png',
      'https://CDN.DiscordApp.COM/avatars/1/a.png',
    ]) {
      expect(storableAvatarUrl(url), url).toBe(url);
    }
  });

  it('refuses any other host, including ones that read like ours', () => {
    for (const url of [
      'https://evil.example/pixel.png',
      'https://cdn.discordapp.com.evil.example/a.png',
      'https://notcdn.discordapp.com.evil/a.png',
      'https://discordapp.com/a.png',
    ]) {
      expect(storableAvatarUrl(url), url).toBeNull();
    }
  });

  it('refuses a userinfo section, which is how a host is made to read as another', () => {
    // The authority of `https://cdn.discordapp.com@evil.example/` is
    // evil.example. Reading the host as "the part before the first slash" is
    // exactly the trick.
    for (const url of [
      'https://cdn.discordapp.com@evil.example/a.png',
      'https://user:pw@evil.example/a.png',
      'https://cdn.discordapp.com:pw@evil.example/a.png',
    ]) {
      expect(storableAvatarUrl(url), url).toBeNull();
    }
  });

  it('refuses any supabase project, which is an origin a stranger can register into', () => {
    // This was allowed by both the CSP and the desktop rule until 2026-09-19,
    // for an avatar upload neither client has.
    for (const url of [
      'https://abcdefgh.supabase.co/storage/v1/object/public/avatars/me.png',
      'https://attacker.supabase.co/storage/v1/object/public/pixel/x.png',
    ]) {
      expect(storableAvatarUrl(url), url).toBeNull();
    }
  });

  it('refuses every scheme but https, and every relative url', () => {
    // The relative ones are F81's trigger on the desktop client: they resolve
    // against `dioxus://` and reach an asset resolver that aborts, not throws.
    for (const url of [
      'http://cdn.discordapp.com/a.png',
      '//cdn.discordapp.com/a.png',
      'javascript:alert(1)',
      'data:image/png;base64,AA',
      'file:///etc/passwd',
      '/%FF',
      '%FF',
      './a.png',
      '../%C0',
    ]) {
      expect(storableAvatarUrl(url), url).toBeNull();
    }
  });

  it('refuses a url carrying whitespace or a control character', () => {
    const control = String.fromCharCode(0);
    const del = String.fromCharCode(0x7f);
    const nbsp = String.fromCharCode(0xa0);
    for (const url of [
      'https://cdn.discordapp.com/a .png',
      'https://cdn.discordapp.com/a\n.png',
      'https://cdn.discordapp.com/a\t.png',
      `https://cdn.discordapp.com/a${control}.png`,
      `https://cdn.discordapp.com/a${del}.png`,
      `https://cdn.discordapp.com/a${nbsp}.png`,
      ` ${real}`,
      `${real} `,
    ]) {
      expect(storableAvatarUrl(url), JSON.stringify(url)).toBeNull();
    }
  });

  it('refuses an overlong url before parsing it', () => {
    expect(storableAvatarUrl(`https://cdn.discordapp.com/${'a'.repeat(600)}.png`)).toBeNull();
  });

  it('refuses anything that is not a non-empty string', () => {
    // `user_metadata` is free-form JSON, so this column's input can be any
    // JSON type at all.
    for (const value of [null, undefined, '', 0, 1, true, {}, [], { toString: () => real }]) {
      expect(storableAvatarUrl(value)).toBeNull();
    }
  });

  it('names one host and only one', () => {
    expect(AVATAR_HOST).toBe('cdn.discordapp.com');
  });
});

/**
 * The rule has four homes — this module, `update-profile`, the signup trigger in
 * `schema.sql`, and the built page's CSP. The first is graded above; the other
 * three are asserted over their source, from a file that is not one of them.
 */
describe('every writer and every renderer applies it', () => {
  const read = (rel: string) => readFileSync(new URL(rel, import.meta.url), 'utf8');

  it('update-profile stores the checked value, never the raw claim', () => {
    const source = read('../update-profile/index.ts');
    expect(source).toContain("import { storableAvatarUrl }");
    const assignments = source.match(/updates\.avatar_url = .*/g) ?? [];
    expect(assignments).toEqual(['updates.avatar_url = storableAvatarUrl(meta.avatar_url);']);
  });

  it('the signup trigger and its backfill both apply the SQL twin', () => {
    const schema = read('../../schema.sql');
    expect(schema).toContain('CREATE OR REPLACE FUNCTION storable_avatar_url(raw TEXT)');
    // Two writers: the AFTER INSERT trigger, and the one-off backfill beside it.
    const raw = schema.match(/raw_user_meta_data->>'avatar_url'/g) ?? [];
    const wrapped = schema.match(/storable_avatar_url\((?:NEW\.)?raw_user_meta_data->>'avatar_url'\)/g) ?? [];
    expect(wrapped.length).toBe(2);
    expect(raw.length).toBe(wrapped.length);
  });

  it('the built page CSP names the same one image host', () => {
    // `https://*.supabase.co` lived here and admitted any Supabase project on
    // the internet — an attacker-controllable origin class inside the directive
    // that decides where a stranger's avatar_url can send a viewer's browser.
    const config = read('../../../vite.config.ts');
    const directive = /`(img-src [^`]*)`/.exec(config);
    expect(directive, 'no img-src directive found in vite.config.ts').not.toBeNull();
    expect(directive![1]).toBe(`img-src 'self' data: https://${AVATAR_HOST}`);
  });
});
