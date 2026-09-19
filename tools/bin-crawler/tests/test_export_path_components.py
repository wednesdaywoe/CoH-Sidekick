"""Guard for `safe_path_component` — the exporter's filesystem boundary (F78).

`_write_power_tree` builds `exported_powers/<category>/<powerset>/` out of two
strings parsed from powers.bin, and every gate downstream treats that tree as
ground truth. Nothing between the binary and the filesystem looked at those
strings: a category of `..` writes a directory up, and `exported_powers/` is a
trust root nothing hashes, so the write would not be visible afterwards either.

What this grades: that the refusal fires on the shapes that escape a directory,
and that it does NOT fire on any name the four committed forks actually ship —
a validator nobody can regen past gets deleted, and the hole comes back with it.

What it cannot grade: the exporter itself. `bin_crawler/path_safety.py` says why
the call sites are not there yet — editing `export_powers.py` declares all four
committed exports stale, and this check is provably a no-op on them. Until that
wiring lands, this sweep is the live half: a future export that writes a name
which is not a name goes red here.

It also cannot say whether the binary a future regen reads is the game's. That
is the other half of F78 and it is still open.

Reads the committed `exported_powers/` trees only — no .bin / .pigg needed.
"""

import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..'))
from bin_crawler.path_safety import safe_path_component

REPO_ROOT = os.path.abspath(
    os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..', '..'))
EXPORTED = os.path.join(REPO_ROOT, 'exported_powers')

_failures = []


def _check(condition, message):
    if not condition:
        _failures.append(message)


def _refuses(value, what='power category', **kwargs):
    try:
        safe_path_component(value, what, **kwargs)
    except ValueError:
        return True
    return False


def test_traversal_is_refused():
    for value in ('..', '.', '../etc', 'a/b', 'a\\b', '/abs', 'C:name'):
        _check(_refuses(value), f"{value!r} was accepted as a path component")


def test_the_unprintable_and_the_empty_are_refused():
    for value in ('', 'na\x00me', 'na\nme', 'tab\there', 42, None):
        _check(_refuses(value), f"{value!r} was accepted as a path component")


def test_windows_device_names_are_refused():
    # These open as devices whatever the extension, so the write goes to a port.
    for value in ('con', 'NUL', 'com1', 'lpt9.json'):
        _check(_refuses(value), f"{value!r} was accepted as a path component")


def test_edge_dots_are_refused_for_a_directory_and_tolerated_for_a_leaf():
    _check(_refuses(' spaced '), "' spaced ' was accepted as a directory name")
    _check(_refuses('trailing.'), "'trailing.' was accepted as a directory name")
    # `v_arachnos/explosive_drone/.json` ships in four forks, from a power whose
    # name parses empty. A leaf check that broke the export over it would be
    # reverted, so the tolerance is explicit and narrow.
    _check(
        not _refuses('.json', 'power filename', permit_edge_dots=True),
        "the committed '.json' leaf is refused; the export would break")
    _check(
        _refuses('../x.json', 'power filename', permit_edge_dots=True),
        "a traversal leaf is accepted under permit_edge_dots")


def test_an_ordinary_name_comes_back_unchanged():
    # A validator, not a rewriter: changing a name here would rename files in
    # every committed tree.
    for value in ('Blaster_Ranged', 'v_arachnos', 'Combat_Training_Defensive'):
        _check(safe_path_component(value, 'powerset') == value,
               f"{value!r} did not survive the check unchanged")


def test_every_committed_component_passes():
    if not os.path.isdir(EXPORTED):
        _failures.append(f"{EXPORTED} is missing; nothing was graded")
        return

    directories = 0
    leaves = 0
    for dirpath, dirnames, filenames in os.walk(EXPORTED):
        for name in dirnames:
            directories += 1
            try:
                safe_path_component(name, 'power category or powerset')
            except ValueError as exc:
                _failures.append(f"committed directory {dirpath}/{name}: {exc}")
        for name in filenames:
            leaves += 1
            try:
                safe_path_component(name, 'power filename', permit_edge_dots=True)
            except ValueError as exc:
                _failures.append(f"committed file {dirpath}/{name}: {exc}")

    _check(directories > 1000, f"only {directories} directories walked; wrong root?")
    _check(leaves > 1000, f"only {leaves} files walked; wrong root?")
    print(f"  walked {directories} directories and {leaves} files under exported_powers/")


if __name__ == "__main__":
    for name, fn in sorted(globals().items()):
        if name.startswith("test_") and callable(fn):
            fn()
    if _failures:
        for failure in _failures:
            print(f"FAIL: {failure}", file=sys.stderr)
        sys.exit(1)
    print("OK — every exported path component is one safe path segment.")
