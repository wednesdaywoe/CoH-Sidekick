#!/usr/bin/env python3
"""Dump raw template bytes for one or more powers by instrumenting the parser.

Wraps `_parse_effect_template` itself — each template's sub-reader byte range
is snapshotted before delegating to the real parser, so there is no hand-kept
mirror of the record layout to drift out of date. (An earlier version mirrored
the effect-group header reads and silently captured nothing once the real
parser's field order changed.)

Usage:
  py -3 dump_template_bytes.py <power_full_name>[,<power_full_name>...] [--assets-dir ...]
"""
import argparse
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from bin_crawler.parser import _powers as P
from bin_crawler.parser._pigg import BinResolver
from bin_crawler.assets_dir import resolve_assets_dir


def hex_groups(data: bytes, per_line: int = 16) -> str:
    out = []
    for i in range(0, len(data), per_line):
        chunk = data[i:i+per_line]
        hexpart = ' '.join(f'{b:02x}' for b in chunk)
        asc = ''.join(chr(b) if 32 <= b < 127 else '.' for b in chunk)
        out.append(f'  {i:04x}  {hexpart:<{per_line*3}}  {asc}')
    return '\n'.join(out)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('power', help='power full_name(s) to dump, comma-separated')
    ap.add_argument('--assets-dir', default=None,
                    help='Assets directory; omit to use the remembered path or a folder picker')
    ap.add_argument('--pick', action='store_true',
                    help='Open a folder picker to choose/change the assets directory')
    args = ap.parse_args()
    assets_dir = resolve_assets_dir(args.assets_dir, pick=args.pick)

    targets = {t.strip().lower() for t in args.power.split(',') if t.strip()}
    captured = []  # (power_full_name, template_bytes, parsed EffectTemplate | None)
    current_power = {'name': None}

    real_pp = P._parse_power
    real_pet = P._parse_effect_template

    def wrapped_power(r, **kw):
        # Peek the record's first field (full_name string offset) so template
        # captures can be attributed without re-parsing anything.
        offset = struct.unpack_from('<I', r._data, r._pos)[0]
        addr = r._strtab_base + offset
        end = addr
        while end < len(r._data) and r._data[end] != 0:
            end += 1
        current_power['name'] = bytes(r._data[addr:end]).decode('utf-8', 'replace')
        return real_pp(r, **kw)

    def wrapped_template(r, **kw):
        raw = bytes(r._data[r._pos:r._end])
        template = real_pet(r, **kw)
        if current_power['name'] and current_power['name'].lower() in targets:
            captured.append((current_power['name'], raw, template))
        return template

    P._parse_power = wrapped_power
    P._parse_effect_template = wrapped_template

    resolver = BinResolver(assets_dir)
    print('Parsing powers.bin (~1-3 min)...', flush=True)
    powers = P.parse_powers(resolver.read('powers.bin'))
    print(f'  parsed {len(powers)} powers', flush=True)

    if not captured:
        print(f'  no templates captured for {sorted(targets)}')
        return

    print(f'\n=== Raw template bytes for {sorted(targets)} ===')
    for i, (name, raw, template) in enumerate(captured):
        attribs = template.attribs if template else '?'
        flags = (hex(template.flags_raw), hex(template.flags2_raw)) if template else '?'
        print(f'\n-- {name} template {i}: attribs={attribs} '
              f'flags={flags} ({len(raw)} bytes)')
        print(hex_groups(raw))


if __name__ == '__main__':
    main()
