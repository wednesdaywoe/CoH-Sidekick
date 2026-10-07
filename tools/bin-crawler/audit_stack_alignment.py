#!/usr/bin/env python3
"""Audit StackType alignment: compare raw .def AttribMod blocks to parser output.

For each player power:
  1. Parse the .powers def file to extract ordered AttribMod blocks
     and their fields (StackType, StackLimit, Type, Flags, Messages, FX, etc.)
  2. Compare with parser-emitted templates from powers.bin
  3. Report mismatches grouped by candidate discriminators

Usage:
  py -3 audit_stack_alignment.py [--assets-dir G:/Homecoming/assets/live]
                                 [--defs-dir "<reference tree>/raw defs"]
                                 [--limit N]
"""

import argparse
import re
import sys
from collections import Counter
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import refdata
from bin_crawler.parser._powers import parse_powers
from bin_crawler.parser._pigg import BinResolver
from bin_crawler.assets_dir import resolve_assets_dir


# Map .def kStack* names to the parser's enum string labels
DEF_STACK_TO_LABEL = {
    'kStack':           'Stack',
    'kReplace':         'Replace',
    'kIgnore':          'Ignore',
    'kExtend':          'Extend',
    'kSuppress':        'Suppress',
    'kRefreshToCount':  'RefreshToCount',
    'kContinuous':      'Continuous',
    'kCollective':      'Collective',
    'kOverlap':         'Overlap',
    'kStackThenIgnore': 'StackThenIgnore',
    'kRefresh':         'Refresh',
    'kMaximize':        'Maximize',
    # Some defs use shortened forms
    'kReplaceMag':      'Replace',
}


def parse_def_attrib_mods(def_text: str) -> list[dict]:
    """Extract ordered AttribMod blocks from a .powers def file.

    Returns list of dicts with keys: stack_type, stack_limit, type, flags,
      has_messages, has_fx, has_params, has_dur_expr, has_mag_expr, attribs.
    """
    mods = []
    # Walk character-by-character to track brace depth and find AttribMod blocks
    i = 0
    n = len(def_text)
    while i < n:
        m = re.search(r'\bAttribMod\b\s*\{', def_text[i:])
        if not m:
            break
        start = i + m.end()  # position right after {
        # Find matching close brace
        depth = 1
        j = start
        while j < n and depth > 0:
            c = def_text[j]
            if c == '{':
                depth += 1
            elif c == '}':
                depth -= 1
            j += 1
        body = def_text[start:j-1]
        i = j

        mods.append(_parse_attribmod_body(body))
    return mods


def _parse_attribmod_body(body: str) -> dict:
    """Extract fields from one AttribMod block body."""
    info = {
        'attribs': [],
        'aspect': None,
        'type': None,           # kMagnitude / kDuration / kExpression / kConstant ...
        'stack_type': None,
        'stack_limit': None,
        'flags': [],
        'has_messages': False,
        'has_fx': False,
        'has_params': False,
        'has_dur_expr': False,
        'has_mag_expr': False,
        'has_dur_value': False,
        'has_mag_value': False,
        'has_table': False,
        'has_caster_stack': False,
    }
    # Strip nested blocks first to scan top-level fields cleanly
    # but record which nested blocks exist
    info['has_messages'] = bool(re.search(r'\bMessages\s*\{', body))
    info['has_fx']       = bool(re.search(r'\bFX\s*\{', body))
    info['has_params']   = bool(re.search(r'\bParams\s*\{', body))

    top = _strip_nested(body)

    for line in top.splitlines():
        s = line.strip()
        if not s:
            continue
        # Tokenize: first token is field name, rest are values
        parts = s.split(None, 1)
        if not parts:
            continue
        key = parts[0]
        val = parts[1].strip() if len(parts) > 1 else ''
        if key == 'Attrib':
            info['attribs'] = val.split()
        elif key == 'Aspect':
            info['aspect'] = val.strip()
        elif key == 'Type':
            info['type'] = val.strip()
        elif key == 'StackType':
            info['stack_type'] = val.strip()
        elif key == 'StackLimit':
            try:
                info['stack_limit'] = int(val.strip())
            except ValueError:
                pass
        elif key == 'Flags':
            info['flags'] = val.split()
        elif key == 'Duration':
            info['has_dur_value'] = True
        elif key == 'DurationExpression':
            info['has_dur_expr'] = True
        elif key == 'Magnitude':
            info['has_mag_value'] = True
        elif key == 'MagnitudeExpression':
            info['has_mag_expr'] = True
        elif key == 'Table':
            info['has_table'] = True
        elif key == 'CasterStack':
            info['has_caster_stack'] = True
    return info


def _strip_nested(body: str) -> str:
    """Remove nested {…} blocks (Messages, FX, Params, Suppress, etc.)."""
    out = []
    depth = 0
    for c in body:
        if c == '{':
            depth += 1
            continue
        if c == '}':
            depth -= 1
            continue
        if depth == 0:
            out.append(c)
    return ''.join(out)


def collect_defs(defs_dir: Path) -> dict[str, list[dict]]:
    """Walk all .powers files; return {full_name_lower: [attribmod_dicts]}.

    `full_name_lower` is the def's "Power <full_name>" header lowered.
    """
    out = {}
    files = list(defs_dir.rglob('*.powers'))
    print(f'  found {len(files)} .powers def files', flush=True)
    for fp in files:
        try:
            text = fp.read_text(encoding='utf-8', errors='replace')
        except Exception:
            continue
        # Each file may contain multiple Power records
        for pm in re.finditer(r'^\s*Power\s+([\w.]+)\s*\{', text, flags=re.MULTILINE):
            full_name = pm.group(1)
            block_start = pm.end()
            depth = 1
            j = block_start
            while j < len(text) and depth > 0:
                c = text[j]
                if c == '{': depth += 1
                elif c == '}': depth -= 1
                j += 1
            body = text[block_start:j-1]
            mods = parse_def_attrib_mods(body)
            out[full_name.lower()] = mods
    return out


def compare(defs_map: dict[str, list[dict]], powers, *, limit: int) -> dict:
    """Compare def AttribMod stack values against parsed templates.

    Returns dict with summary stats and per-discriminator mismatch counts.
    """
    matched_powers = 0
    matched_mods = 0
    template_count_mismatch = 0
    stack_match = 0
    result_extra = {}
    stack_mismatch = 0
    mismatches = []  # list of dicts (capped at limit*5 for memory)

    for pw in powers:
        key = pw.full_name.lower()
        def_mods = defs_map.get(key)
        if def_mods is None:
            continue
        matched_powers += 1
        # Flatten parser templates (recursive — effect groups can contain
        # child effect groups with their own templates).
        parser_templates = []
        # noqa B023: `_walk` is redefined each iteration and invoked immediately below,
        # so it always appends to the list built in its own iteration.
        def _walk(eg):
            for t in eg.templates:
                parser_templates.append(t)  # noqa: B023
            for child in getattr(eg, 'child_groups', []):
                _walk(child)
        for eg in pw.effects:
            _walk(eg)
        if len(def_mods) != len(parser_templates):
            template_count_mismatch += 1
            if 'count_examples' not in result_extra:
                result_extra['count_examples'] = []
            if len(result_extra['count_examples']) < 20:
                result_extra['count_examples'].append({
                    'power': pw.full_name,
                    'def_count': len(def_mods),
                    'parser_count': len(parser_templates),
                    'def_stacks': [d['stack_type'] for d in def_mods],
                    'def_attribs': [d['attribs'] for d in def_mods],
                    'parser_attribs': [t.attribs for t in parser_templates],
                })
            # Skip — can't safely zip mismatched counts
            continue
        # strict=True: the unequal-length case is counted and `continue`d above.
        for d, t in zip(def_mods, parser_templates, strict=True):
            matched_mods += 1
            if d['stack_type'] is None:
                # Def didn't specify (unlikely); skip
                continue
            expected = DEF_STACK_TO_LABEL.get(d['stack_type'], d['stack_type'])
            actual = t.stack
            if expected == actual:
                stack_match += 1
            else:
                stack_mismatch += 1
                if len(mismatches) < limit * 50:
                    mismatches.append({
                        'power': pw.full_name,
                        'def': d,
                        'parser_stack': actual,
                        'parser_stack_limit': t.stack_limit,
                        'parser_type': t.type,
                        'parser_aspect': t.aspect,
                        'parser_attribs': t.attribs,
                        'parser_table': t.table,
                        'parser_dur_expr': t.duration_expression,
                        'parser_mag_expr': t.magnitude_expression,
                    })

    return {
        'matched_powers': matched_powers,
        'matched_mods': matched_mods,
        'template_count_mismatch': template_count_mismatch,
        'stack_match': stack_match,
        'stack_mismatch': stack_mismatch,
        'mismatches': mismatches,
        'count_examples': result_extra.get('count_examples', []),
    }


def discriminator_summary(mismatches: list[dict], goods: list[dict]) -> None:
    """Print per-feature mismatch vs match rates to spot a discriminator."""

    def features(d: dict) -> dict:
        f = d['def'] if 'def' in d else d
        return {
            'type':           f['type'],
            'flags_set':      tuple(sorted(f.get('flags') or [])),
            'has_messages':   f.get('has_messages'),
            'has_fx':         f.get('has_fx'),
            'has_params':     f.get('has_params'),
            'has_dur_expr':   f.get('has_dur_expr'),
            'has_mag_expr':   f.get('has_mag_expr'),
            'has_dur_value':  f.get('has_dur_value'),
            'has_mag_value':  f.get('has_mag_value'),
            'has_table':      f.get('has_table'),
            'has_caster_stack': f.get('has_caster_stack'),
        }

    feature_keys = ['type', 'has_messages', 'has_fx', 'has_params',
                    'has_dur_expr', 'has_mag_expr', 'has_dur_value',
                    'has_mag_value', 'has_table', 'has_caster_stack']

    bad = [features(m) for m in mismatches]
    good = [features(g) for g in goods]

    print(f"\n{'feature':<20} {'value':<25} {'bad%':>8} {'good%':>8} {'lift':>8}")
    print('-' * 75)
    for fk in feature_keys:
        bad_vals = Counter(f[fk] for f in bad)
        good_vals = Counter(f[fk] for f in good)
        all_vals = set(bad_vals) | set(good_vals)
        for v in sorted(all_vals, key=lambda x: (str(x))):
            bp = bad_vals[v] / max(1, len(bad)) * 100
            gp = good_vals[v] / max(1, len(good)) * 100
            lift = bp - gp
            if abs(lift) >= 5:  # only show meaningful spreads
                print(f"  {fk:<18} {str(v):<25} {bp:>7.1f}% {gp:>7.1f}% {lift:>+7.1f}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--assets-dir', default=None,
                    help='Assets directory; omit to use the remembered path or a folder picker')
    ap.add_argument('--pick', action='store_true',
                    help='Open a folder picker to choose/change the assets directory')
    # Was a hardcoded C:\ path until 2026-09-24, i.e. right on one Windows box and
    # broken everywhere else. Resolved through refdata now, same as the two gates.
    ap.add_argument('--defs-dir', default=str(refdata.raw_defs(required=False)),
                    help='The `raw defs/` .powers oracle; overrides COH_RAW_DEFS')
    ap.add_argument('--limit', type=int, default=10,
                    help='Max example mismatches to print')
    args = ap.parse_args()
    assets_dir = resolve_assets_dir(args.assets_dir, pick=args.pick)

    print(f'Loading defs from {args.defs_dir}', flush=True)
    defs_dir = Path(args.defs_dir)
    defs_map = collect_defs(defs_dir)
    print(f'  {len(defs_map)} powers indexed from defs', flush=True)

    print(f'\nParsing powers.bin from {assets_dir}', flush=True)
    resolver = BinResolver(assets_dir)
    powers = parse_powers(resolver.read('powers.bin'))
    print(f'  {len(powers)} powers parsed', flush=True)

    print('\nComparing...', flush=True)
    result = compare(defs_map, powers, limit=args.limit)

    print('\n=== Summary ===')
    print(f'  powers matched (def & binary):       {result["matched_powers"]}')
    print(f'  template counts equal mods checked:  {result["matched_mods"]}')
    print(f'  template_count_mismatch (skipped):   {result["template_count_mismatch"]}')
    print(f'  stack value match:                   {result["stack_match"]}')
    print(f'  stack value MISMATCH:                {result["stack_mismatch"]}')

    # Build "goods" set from the same powers — anything that matched
    goods = []
    for pw in powers:
        key = pw.full_name.lower()
        def_mods = defs_map.get(key)
        if not def_mods:
            continue
        parser_templates = [t for eg in pw.effects for t in eg.templates]
        if len(def_mods) != len(parser_templates):
            continue
        # strict=True: the unequal-length case is skipped two lines above.
        for d, t in zip(def_mods, parser_templates, strict=True):
            if d['stack_type'] is None:
                continue
            expected = DEF_STACK_TO_LABEL.get(d['stack_type'], d['stack_type'])
            if expected == t.stack:
                goods.append({'def': d})

    print('\n=== Count-mismatch examples (def != parser template count) ===')
    for ex in result['count_examples'][:15]:
        print(f'  {ex["power"]}: def={ex["def_count"]} parser={ex["parser_count"]}')
        print(f'    def stacks : {ex["def_stacks"]}')
        print(f'    def attribs: {ex["def_attribs"]}')
        print(f'    parser attribs: {ex["parser_attribs"]}')

    print('\n=== Discriminator candidates (features with bad-vs-good lift >= 5%) ===')
    discriminator_summary(result['mismatches'], goods)

    # Mismatch breakdown by (def_stack, parser_stack) pair
    pair_counts = Counter(
        (m['def']['stack_type'], m['parser_stack']) for m in result['mismatches']
    )
    print('\n=== Mismatch pairs (def -> parser, count) ===')
    for (d, p), c in pair_counts.most_common(20):
        print(f'  {d:<20} -> {p!r:<22}  {c}')

    # Categorize by Create_Entity attrib
    ce_count = sum(1 for m in result['mismatches'] if 'Create_Entity' in m['parser_attribs'])
    print(f'\n  templates whose parser attribs include "Create_Entity": {ce_count}')

    print(f'\n=== {min(args.limit, len(result["mismatches"]))} example mismatches ===')
    for m in result['mismatches'][:args.limit]:
        d = m['def']
        print(f'\n  power: {m["power"]}')
        print(f'    def    : type={d["type"]} StackType={d["stack_type"]} StackLimit={d["stack_limit"]}'
              f' flags={d["flags"]}')
        print(f'           : msgs={d["has_messages"]} fx={d["has_fx"]} params={d["has_params"]}'
              f' dur_expr={d["has_dur_expr"]} mag_expr={d["has_mag_expr"]}'
              f' dur_val={d["has_dur_value"]} mag_val={d["has_mag_value"]}')
        print(f'    parser : stack={m["parser_stack"]!r} stack_limit={m["parser_stack_limit"]}'
              f' type={m["parser_type"]!r} aspect={m["parser_aspect"]!r}'
              f' attribs={m["parser_attribs"]}')


if __name__ == '__main__':
    main()
