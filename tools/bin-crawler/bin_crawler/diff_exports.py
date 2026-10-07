#!/usr/bin/env python3
"""Run live + experimental exports and summarize the diff.

Usage:
  py -3 tools/bin-crawler/bin_crawler/diff_exports.py
  py -3 tools/bin-crawler/bin_crawler/diff_exports.py --skip-export   # reuse existing output
  py -3 tools/bin-crawler/bin_crawler/diff_exports.py --left live --right experimental
"""

import argparse
import json
import subprocess
import sys
from pathlib import Path

from bin_crawler.assets_dir import resolve_dir


SCRIPT_DIR = Path(__file__).resolve().parent
EXPORT_SCRIPT = SCRIPT_DIR / 'export_powers.py'
DEFAULT_OUTPUT_ROOT = Path('./exported_powers')


def run_export(assets_dir: Path, output_dir: Path) -> None:
    print(f'\n=== Exporting {assets_dir} -> {output_dir} ===', flush=True)
    result = subprocess.run(
        [sys.executable, str(EXPORT_SCRIPT),
         '--assets-dir', str(assets_dir),
         '--output-dir', str(output_dir)],
        check=False,
    )
    if result.returncode != 0:
        sys.exit(f'Export failed for {assets_dir} (exit {result.returncode})')


def load_powers(root: Path) -> dict[str, dict]:
    """Load all power JSONs keyed by 'category/powerset/power'."""
    powers: dict[str, dict] = {}
    if not root.exists():
        return powers
    for cat_dir in root.iterdir():
        if not cat_dir.is_dir():
            continue
        for ps_dir in cat_dir.iterdir():
            if not ps_dir.is_dir():
                continue
            for json_file in ps_dir.glob('*.json'):
                if json_file.name == 'index.json':
                    continue
                key = f'{cat_dir.name}/{ps_dir.name}/{json_file.stem}'
                try:
                    with open(json_file) as f:
                        powers[key] = json.load(f)
                except (json.JSONDecodeError, OSError) as e:
                    print(f'  WARN: could not read {json_file}: {e}')
    return powers


def shallow_diff(a: dict, b: dict) -> list[str]:
    """Return list of top-level field names whose values differ."""
    changed = []
    keys = set(a) | set(b)
    for k in sorted(keys):
        if a.get(k) != b.get(k):
            changed.append(k)
    return changed


# --- Deep (effect-level) diff -------------------------------------------
#
# shallow_diff only reports THAT `effects` changed, which is useless for a
# balance pass. The deep diff pairs individual effect templates by IDENTITY
# rather than by array position: a patch that inserts one template would
# otherwise shift every later index and report the whole power as rewritten.
#
# A power is modelled as a flat list of atomic effects (docs/COH-DATA-MODEL.md),
# so templates are flattened out of their group/child_effects nesting and
# matched across the whole power. `is_pvp` rides in the identity because a
# PvE/PvP pair is otherwise indistinguishable (and has been clobbered before).

TEMPLATE_ID_FIELDS = ('attribs', 'aspect', 'table', 'target',
                      'type', 'application_type', 'stack_key')

TEMPLATE_VALUE_FIELDS = ('scale', 'magnitude', 'duration', 'delay',
                         'application_period', 'tick_chance',
                         'tick_mag_multiplier', 'tick_mag_additive',
                         'stack', 'stack_limit', 'caster_stack', 'flags_raw',
                         'duration_expression', 'magnitude_expression',
                         'jit_requires')


def _flatten_templates(power: dict):
    """Yield (identity, template) for every template in a power, at any depth."""
    def walk(groups, is_pvp):
        for g in groups or []:
            pvp = g.get('is_pvp', is_pvp)
            for t in g.get('templates') or []:
                ident = (pvp,) + tuple(
                    tuple(t[f]) if isinstance(t.get(f), list) else t.get(f)
                    for f in TEMPLATE_ID_FIELDS)
                yield ident, t
            yield from walk(g.get('child_effects'), pvp)
    # is_pvp is a tri-state string ('EITHER' | 'PVE_ONLY' | 'PVP_ONLY'), not a
    # bool; child_effects inherit the parent group's value when absent.
    yield from walk(power.get('effects'), 'EITHER')


def _bucket(power: dict) -> dict[tuple, list[dict]]:
    out: dict[tuple, list[dict]] = {}
    for ident, t in _flatten_templates(power):
        out.setdefault(ident, []).append(t)
    return out


def deep_diff_power(a: dict, b: dict):
    """Compare one power's templates by identity.

    Returns (added, removed, changes) where changes is a list of
    (identity, field, old, new). Templates sharing an identity are paired in
    order; the surplus on either side counts as added/removed.
    """
    A, B = _bucket(a), _bucket(b)
    added, removed, changes = [], [], []
    for ident in set(A) | set(B):
        la, lb = A.get(ident, []), B.get(ident, [])
        for _t in lb[len(la):]:
            added.append(ident)
        for _t in la[len(lb):]:
            removed.append(ident)
        # strict=False deliberately: the two loops above already consumed whichever tail
        # is longer, so this pass is the shared prefix by design.
        for ta, tb in zip(la, lb, strict=False):
            for f in TEMPLATE_VALUE_FIELDS:
                if f in ta and f in tb and ta[f] != tb[f]:
                    changes.append((ident, f, ta[f], tb[f]))
    return added, removed, changes


def _ident_str(ident: tuple) -> str:
    pvp, attribs, aspect, table, target = ident[0], ident[1], ident[2], ident[3], ident[4]
    label = '/'.join(attribs) if attribs else '-'
    mode = '' if pvp in (None, 'EITHER') else f' ({pvp})'
    return f"{label} [{aspect} {table} ->{target}]{mode}"


def summarize_deep(left: dict[str, dict], right: dict[str, dict],
                   limit: int = 25) -> None:
    from collections import Counter
    common = set(left) & set(right)
    field_counts: Counter = Counter()
    per_power: list[tuple[str, int, int, int]] = []
    examples: dict[str, list] = {}
    added_ct: Counter = Counter()
    removed_ct: Counter = Counter()

    for k in sorted(common):
        added, removed, changes = deep_diff_power(left[k], right[k])
        if not (added or removed or changes):
            continue
        per_power.append((k, len(added), len(removed), len(changes)))
        for ident in added:
            added_ct[_ident_str(ident)] += 1
        for ident in removed:
            removed_ct[_ident_str(ident)] += 1
        for ident, f, old, new in changes:
            field_counts[f] += 1
            examples.setdefault(f, [])
            if len(examples[f]) < 6:
                examples[f].append((k, _ident_str(ident), old, new))

    print('\n=== Deep diff (templates paired by identity) ===')
    print(f'  Powers with effect-level changes: {len(per_power)}')
    print(f'  Templates added:   {sum(added_ct.values())}')
    print(f'  Templates removed: {sum(removed_ct.values())}')
    print(f'  Value changes:     {sum(field_counts.values())}')

    if field_counts:
        print('\n--- Changed template fields ---')
        for f, c in field_counts.most_common():
            print(f'  {c:6d}  {f}')
            for k, ident, old, new in examples[f][:3]:
                print(f'            {k}')
                print(f'              {ident}: {old!r} -> {new!r}')

    if added_ct:
        print('\n--- Most-added template identities ---')
        for ident, c in added_ct.most_common(limit):
            print(f'  {c:5d}  + {ident}')
    if removed_ct:
        print('\n--- Most-removed template identities ---')
        for ident, c in removed_ct.most_common(limit):
            print(f'  {c:5d}  - {ident}')

    if per_power:
        print(f'\n--- Powers by churn (top {limit}) ---')
        for k, a, r, c in sorted(per_power, key=lambda x: -(x[1] + x[2] + x[3]))[:limit]:
            print(f'  {k}: +{a} -{r} ~{c}')


# --- Generic sweeps ------------------------------------------------------
#
# Two note-free detectors, run by default because the whole reason they exist is
# that they are easy to forget. Neither reads a patch note: each asks whether the
# NEW build is consistent with itself and with the shape of the old one, so both
# survive a note being wrong, missing, or written against a different build.
#
# On the 2026-07-30 Homecoming beta they produced two of the four Tier 1 findings
# in docs/HC-BETA-2026-07.md — Tanker Foot Stomp's PvP branch carrying a sibling
# power's templates, and Rain of Arrows losing its Avoid on four of five player
# pets — and nothing else in the pass found either of them.


def _template_signature(power: dict) -> tuple:
    """A power's effects, order-independent, with the per-copy fields stripped.

    Equal signatures mean template-for-template the same behaviour. Name,
    powerset, icon and display name are not in it, so one archetype's copy of a
    power compares equal to another's whenever the two behave alike. Built from
    the same identity/value field lists the deep diff pairs on, so the two stay
    in step when a field is added there.
    """
    rows = []
    for ident, t in _flatten_templates(power):
        values = tuple(
            tuple(t[f]) if isinstance(t.get(f), list) else t.get(f)
            for f in TEMPLATE_VALUE_FIELDS)
        rows.append((ident, values))
    return tuple(sorted(rows, key=repr))


def _by_shared_power(powers: dict[str, dict]) -> dict[tuple, dict[str, dict]]:
    """`(powerset, power) -> {category: power}` — one archetype's copies grouped
    with the others'. The category is the only part of the key that carries the
    archetype, so dropping it is what makes the copies comparable."""
    out: dict[tuple, dict[str, dict]] = {}
    for key, power in powers.items():
        parts = key.split('/')
        if len(parts) != 3:
            continue
        category, powerset, name = parts
        out.setdefault((powerset, name), {})[category] = power
    return out


def find_cross_at_divergence(left: dict[str, dict], right: dict[str, dict]):
    """Powers whose archetype copies agreed on the left and disagree on the right.

    A powerset copied onto several archetypes ships the same power several times.
    When those copies are template-for-template identical on live and stop being
    identical on beta, an edit reached some copies and not the rest. That is
    either a deliberate per-AT split — in which case the notes say so — or an
    edit applied to the wrong copy, and the build disagreeing with itself is what
    makes it worth asking about with no note in hand.

    The comparison is per AGREEING SET, not per power. A power often ships an
    NPC or mission-maker copy beside its archetype ones that has always been
    different; asking whether *every* copy agrees would let that copy mask a
    split among the copies that did agree. Foot Stomp is exactly this case —
    `mission_maker_attacks` carries a third copy — and the whole-group form of
    this sweep silently misses the build's strongest finding.
    """
    L, R = _by_shared_power(left), _by_shared_power(right)
    hits = []
    agreeing_sets = 0
    for shared in sorted(L):
        left_copies, right_copies = L[shared], R.get(shared) or {}
        categories = sorted(set(left_copies) & set(right_copies))
        if len(categories) < 2:
            continue
        live_classes: dict[tuple, list[str]] = {}
        for c in categories:
            live_classes.setdefault(_template_signature(left_copies[c]), []).append(c)
        for agreed in live_classes.values():
            if len(agreed) < 2:
                continue
            agreeing_sets += 1
            groups: dict[tuple, list[str]] = {}
            for c in agreed:
                groups.setdefault(_template_signature(right_copies[c]), []).append(c)
            if len(groups) > 1:
                hits.append((shared, right_copies, groups))
    return hits, agreeing_sets


def sweep_cross_at_divergence(left: dict[str, dict], right: dict[str, dict],
                              limit: int = 25) -> None:
    """Report [`find_cross_at_divergence`], each hit carrying the template-level
    diff between one representative of each surviving behaviour."""
    hits, agreeing_sets = find_cross_at_divergence(left, right)
    print('\n=== Sweep: powers that agreed across archetypes and no longer do ===')
    print(f'  Sets of copies that agreed on the left: {agreeing_sets}')
    print(f'  Diverged: {len(hits)}')
    for (powerset_name, power_name), right_copies, groups in hits[:limit]:
        ordered = sorted(groups.values(), key=lambda cats: cats[0])
        print(f'\n  ~ {powerset_name}/{power_name} — '
              f'{len(ordered)} behaviours across {sum(len(c) for c in ordered)} archetypes')
        reference = ordered[0]
        print(f'      {", ".join(reference)}  (reference)')
        for cats in ordered[1:]:
            print(f'      {", ".join(cats)}')
            added, removed, changes = deep_diff_power(
                right_copies[reference[0]], right_copies[cats[0]])
            for ident in added:
                print(f'         + {_ident_str(ident)}')
            for ident in removed:
                print(f'         - {_ident_str(ident)}')
            for ident, f, old, new in changes:
                print(f'         ~ {_ident_str(ident)}: {f} {old!r} -> {new!r}')
    if len(hits) > limit:
        print(f'\n  ... and {len(hits) - limit} more')


def load_powerset_indexes(root: Path) -> dict[str, dict]:
    """Every powerset's `index.json`, keyed `category/powerset`."""
    indexes: dict[str, dict] = {}
    if not root.exists():
        return indexes
    for cat_dir in root.iterdir():
        if not cat_dir.is_dir():
            continue
        for ps_dir in cat_dir.iterdir():
            index_file = ps_dir / 'index.json'
            if not ps_dir.is_dir() or not index_file.exists():
                continue
            try:
                with open(index_file) as f:
                    indexes[f'{cat_dir.name}/{ps_dir.name}'] = json.load(f)
            except (json.JSONDecodeError, OSError) as e:
                print(f'  WARN: could not read {index_file}: {e}')
    return indexes


def _powerset_power_names(entry: dict) -> set:
    """A powerset's power DISPLAY names."""
    return set(entry.get('power_display_names') or [])


def find_powerset_losses(left: dict[str, dict], right: dict[str, dict]):
    """`(vanished, shrunk)` — powerset keys gone from the right, and
    `(key, lost, gained)` for those that kept existing but lost a power."""
    gone = sorted(set(left) - set(right))
    shrunk = []
    for key in sorted(set(left) & set(right)):
        lost = _powerset_power_names(left[key]) - _powerset_power_names(right[key])
        if lost:
            gained = _powerset_power_names(right[key]) - _powerset_power_names(left[key])
            shrunk.append((key, sorted(lost), sorted(gained)))
    return gone, shrunk


def sweep_powerset_losses(left: dict[str, dict], right: dict[str, dict],
                          limit: int = 40) -> None:
    """Powersets that lost a power, and powersets that vanished outright.

    Keyed by DISPLAY name rather than by file name, because a file name is not a
    stable identity across builds — Homecoming reuses power slots, and this beta
    alone ships seven files whose display name belongs to a different file.

    A powerset losing one of several powers is invisible in the top-level removed
    list, which is one flat line per removed key. Grouping the loss under the
    powerset that still exists is what turns a line among dozens into "this pet
    lost its Avoid while its siblings kept theirs".

    Each loss is printed with whatever that powerset GAINED, because keying on
    the display name means a rename reads as a loss. Shown side by side a rename
    is obvious (Adrenalin Boost out, Adrenaline Boost in) and a real deletion has
    nothing beside it — which is the whole distinction the reader needs.
    """
    gone, shrunk = find_powerset_losses(left, right)
    names = _powerset_power_names

    print('\n=== Sweep: powersets that lost powers ===')
    print(f'  Powersets: {len(left)} -> {len(right)}')
    print(f'  Vanished entirely: {len(gone)}')
    print(f'  Kept but lost a power: {len(shrunk)}')
    for key in gone[:limit]:
        print(f'  - {key}  ({", ".join(sorted(names(left[key]))) or "no powers"})')
    if len(gone) > limit:
        print(f'  ... and {len(gone) - limit} more')
    for key, lost, gained in shrunk[:limit]:
        trailer = f'  gained {gained}' if gained else '  gained nothing'
        print(f'  ~ {key}  lost {lost}{trailer}')
    if len(shrunk) > limit:
        print(f'  ... and {len(shrunk) - limit} more')


def summarize(left_name: str, left: dict[str, dict],
              right_name: str, right: dict[str, dict]) -> None:
    left_keys = set(left)
    right_keys = set(right)

    added = sorted(right_keys - left_keys)
    removed = sorted(left_keys - right_keys)
    common = left_keys & right_keys

    changed: list[tuple[str, list[str]]] = []
    for k in sorted(common):
        fields = shallow_diff(left[k], right[k])
        if fields:
            changed.append((k, fields))

    print(f'\n=== Diff: {left_name} -> {right_name} ===')
    print(f'  {left_name}: {len(left_keys)} powers')
    print(f'  {right_name}: {len(right_keys)} powers')
    print(f'  Added   in {right_name}: {len(added)}')
    print(f'  Removed in {right_name}: {len(removed)}')
    print(f'  Changed: {len(changed)}')

    if added:
        print(f'\n--- Added ({len(added)}) ---')
        for k in added[:50]:
            print(f'  + {k}')
        if len(added) > 50:
            print(f'  ... and {len(added) - 50} more')

    if removed:
        print(f'\n--- Removed ({len(removed)}) ---')
        for k in removed[:50]:
            print(f'  - {k}')
        if len(removed) > 50:
            print(f'  ... and {len(removed) - 50} more')

    if changed:
        print(f'\n--- Changed ({len(changed)}) ---')
        # Show first 30 with the fields that differ
        for k, fields in changed[:30]:
            print(f'  ~ {k}: {", ".join(fields)}')
        if len(changed) > 30:
            print(f'  ... and {len(changed) - 30} more')

        # Field-frequency histogram across all changed powers
        from collections import Counter
        field_counts = Counter(f for _, fields in changed for f in fields)
        print('\n--- Most-changed fields ---')
        for field, count in field_counts.most_common(15):
            print(f'  {count:5d}  {field}')


def main():
    ap = argparse.ArgumentParser(description='Export live + experimental and diff')
    ap.add_argument('--left', default='live', help='Left assets subdir name (default: live)')
    ap.add_argument('--right', default='experimental', help='Right assets subdir name (default: experimental)')
    ap.add_argument('--assets-root', default=None,
                    help='Root containing <left>/ and <right>/. Omit to use the '
                         'remembered root or a folder picker.')
    ap.add_argument('--pick', action='store_true',
                    help='Open a folder picker to choose/change the assets root')
    ap.add_argument('--output-root', default=str(DEFAULT_OUTPUT_ROOT),
                    help=f'Root for JSON outputs (default: {DEFAULT_OUTPUT_ROOT})')
    ap.add_argument('--skip-export', action='store_true',
                    help='Skip running exports; diff existing output dirs')
    ap.add_argument('--deep', action='store_true',
                    help='Also diff inside effects, pairing templates by '
                         'identity instead of array position')
    ap.add_argument('--no-sweeps', action='store_true',
                    help='Skip the two note-free consistency sweeps (cross-'
                         'archetype divergence, powerset losses). They run by '
                         'default: both are cheap, and both found things no '
                         'note-driven pass did.')
    ap.add_argument('--left-dir', default=None,
                    help='Diff an existing export tree directly (implies '
                         '--skip-export); pairs with --right-dir')
    ap.add_argument('--right-dir', default=None,
                    help='See --left-dir')
    args = ap.parse_args()

    # Direct tree-vs-tree mode: skips assets resolution entirely, so an
    # already-exported pair (or a committed tree) can be diffed as-is.
    if args.left_dir or args.right_dir:
        if not (args.left_dir and args.right_dir):
            sys.exit('ERROR: --left-dir and --right-dir must be given together')
        left = load_powers(Path(args.left_dir))
        right = load_powers(Path(args.right_dir))
        if not left or not right:
            sys.exit('ERROR: one or both sides are empty; check the paths.')
        summarize(args.left_dir, left, args.right_dir, right)
        if args.deep:
            summarize_deep(left, right)
        if not args.no_sweeps:
            sweep_cross_at_divergence(left, right)
            sweep_powerset_losses(load_powerset_indexes(Path(args.left_dir)),
                                  load_powerset_indexes(Path(args.right_dir)))
        return

    # The assets ROOT holds two assets dirs (<left>/ and <right>/), so validate
    # that both halves look like assets dirs rather than the root itself.
    def _valid_root(p) -> bool:
        from bin_crawler.assets_dir import is_assets_dir
        root = Path(p)
        return root.is_dir() and is_assets_dir(root / args.left) and is_assets_dir(root / args.right)

    assets_root = Path(resolve_dir(
        args.assets_root,
        key='assets_root',
        title=f'Select the assets ROOT (the folder containing {args.left}/ and {args.right}/)',
        pick=args.pick,
        validate=_valid_root,
    ))
    output_root = Path(args.output_root)
    left_out = output_root / args.left
    right_out = output_root / args.right

    if not args.skip_export:
        run_export(assets_root / args.left, left_out)
        run_export(assets_root / args.right, right_out)

    print(f'\nLoading {left_out}...', flush=True)
    left = load_powers(left_out)
    print(f'Loading {right_out}...', flush=True)
    right = load_powers(right_out)

    if not left or not right:
        sys.exit('ERROR: one or both sides are empty. Run without --skip-export or check paths.')

    summarize(args.left, left, args.right, right)
    if args.deep:
        summarize_deep(left, right)
    if not args.no_sweeps:
        sweep_cross_at_divergence(left, right)
        sweep_powerset_losses(load_powerset_indexes(left_out),
                              load_powerset_indexes(right_out))


if __name__ == '__main__':
    main()
