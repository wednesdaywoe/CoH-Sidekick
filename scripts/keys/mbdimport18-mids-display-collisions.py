#!/usr/bin/env python3
"""Key for DATA-GAP MBDIMPORT-18: the display join refuses OUR ambiguity, not MIDS'.

Run from the repo root:  python3 scripts/keys/mbdimport18-mids-display-collisions.py

`convert-mids-name-map.cjs` joins Mids' powers to ours on the display name, and refuses when the
display names two powers HERE — "picking either arm would be a coin flip dressed as a decode," in
its own words. The same sentence read the other way was not enforced. Rebirth's Savage Melee pet
sets give both `Rending_Flurry_Normal` and `Rending_Flurry_Large` the display "Rending Flurry";
ours separates them as "Rending Flurry" and "Frenzied Rending Flurry"; so Mids' `Large` reached
our `Normal`, past an internal-name identity, and the reverse row sent the writer the same way.

So this breaks on TWO outcomes, and upward on a population:

  no row rests on a colliding  read out of `contract/<fork>/mids-names.json`, which is what the
  Mids display                 reader parses — not the generator's stderr, so a generator that
                               stops applying the guard reds here even though its own report
                               would still look clean.

  every refused name is        `MidsNames::is_rotated_away` DECLINES a pick whose Mids name our
  genuinely absent             side has given to a different power. That is right only while the
                               name Mids means has no counterpart here at all; if one of our
                               powers displays what Mids displays, the refusal is dropping a pick
                               the map should have carried. 15 builds in the 2,178-file corpus
                               turn on this — every `Stalker_Defense.Willpower.Reconstruction` —
                               and all 42 of that sweep's failed enhancements hang off them.

  the refused population is     a bounded refusal can be adjudicated name by name, which is what
  small                         closed this. A larger one is a policy question again.

Our side is read from `exported_powers/`, not from `contract/<fork>/powersets/`: pet and summon
sets are not selectable powersets and have no file there, and those are exactly the sets Mids
collides displays in. Reading a partition that cannot hold the case would make every check on it
vacuously green — which is how the first pass of this census read "no counterpart" off a lookup
that had found nothing at all.
"""
import json, os, re, sys

FORKS = ("homecoming", "rebirth", "thunderspy", "brainstorm")
ROOTS = {
    "homecoming": "exported_powers",
    "rebirth": "exported_powers/rebirth",
    "thunderspy": "exported_powers/thunderspy",
    "brainstorm": "exported_powers/brainstorm",
}
NESTED = ("rebirth", "thunderspy", "brainstorm")
# Two rotated-away names survive the fix, both adjudicated: Homecoming's Stalker Willpower, where
# Mids carries a `Reconstruction` the set does not have, and Thunderspy's Darkness Control, where
# Mids' own display for the name is "Shadowy Binds (invalid)". Left at the adjudicated number
# rather than at the four this was found with — the four included the two the fix removed.
MAX_ROTATED_AWAY = 2


def normalize(s):
    """The generator's `normalizeDisplay`: separator runs to one space, case folded."""
    return re.sub(r"[^a-z0-9]+", " ", str(s or "").lower()).strip()


def export_sets(fork):
    """Our powers per `group.set`, read the way the generator reads them."""
    root = ROOTS[fork]
    out = {}
    for dirpath, dirnames, filenames in os.walk(root):
        rel = os.path.relpath(dirpath, root).split(os.sep)
        if fork == "homecoming" and rel[0] in NESTED:
            dirnames[:] = []
            continue
        if "index.json" not in filenames or len(rel) < 2:
            continue
        powers = []
        for name in filenames:
            if name == "index.json" or not name.endswith(".json"):
                continue
            try:
                power = json.load(open(os.path.join(dirpath, name), encoding="utf-8"))
            except Exception:
                continue
            if power.get("name"):
                powers.append((power["name"], power.get("display_name")))
        if powers:
            out.setdefault(f"{rel[-2]}.{rel[-1]}".lower(), []).extend(powers)
    return out


def main():
    if not os.path.isdir("contract"):
        print("run from the repo root (no ./contract)", file=sys.stderr)
        return 2
    seated, stranded, rotated, vacuous = [], [], [], []
    colliding_names = 0
    forks_read = 0
    for fork in FORKS:
        oracle = f"tools/mids-oracle/mids-power-names.{fork}.json"
        names = f"contract/{fork}/mids-names.json"
        if not os.path.exists(oracle) or not os.path.exists(names):
            continue
        forks_read += 1
        mids = json.load(open(oracle, encoding="utf-8"))["powersets"]
        contract = json.load(open(names, encoding="utf-8"))
        forward, reverse = contract["nameMap"], contract["nameReverse"]
        ours = export_sets(fork)

        for powerset, entries in mids.items():
            key = powerset.lower()
            rows = forward.get(key, {})
            back = reverse.get(key, {})
            entries = [e for e in entries if isinstance(e, list) and len(e) >= 3]

            # Which Mids displays name more than one power in this set.
            share = {}
            for _internal, display, _level in entries:
                if normalize(display):
                    share[normalize(display)] = share.get(normalize(display), 0) + 1
            colliding = {
                str(i) for i, d, _l in entries if share.get(normalize(d), 0) > 1
            }
            colliding_names += len(colliding)

            for internal, display, level in entries:
                where = f"[{fork}] {powerset}: Mids {internal!r} ({display!r} @L{level})"
                if str(internal).lower() in rows and str(internal) in colliding:
                    seated.append(
                        f"{where} shares its display with "
                        f"{share[normalize(display)] - 1} sibling(s), yet carries a row to "
                        f"{rows[str(internal).lower()]!r}")
                # The reverse table is keyed by OUR name and valued by Mids'; a colliding Mids
                # name on the value side is the same coin flip pointed at the writer.
                for our_name, mids_name in back.items():
                    if str(mids_name) in colliding and str(mids_name) == str(internal):
                        seated.append(
                            f"{where} shares its display, yet the writer sends our "
                            f"{our_name!r} to it")

                # Rotated away: the map does not carry this name, and one of ours has taken it.
                if str(internal).lower() in rows or not rows:
                    continue
                if not any(str(internal).lower() == v.lower() for v in rows.values()):
                    continue
                rotated.append(where)
                # A lookup that finds nothing is not a power that is not there. The first cut of
                # this census read "no counterpart" off `contract/<fork>/powersets/`, which holds
                # no pet sets at all, and called two wrong rows adjudicated on the strength of an
                # empty list. An empty roster here is a broken claim, not a clean one.
                if not ours.get(key):
                    vacuous.append(
                        f"{where} is refused, and nothing under {key!r} was read from "
                        f"{ROOTS[fork]!r} — the check that its name is absent cannot run")
                here = [
                    i for i, d in ours.get(key, [])
                    if normalize(d) == normalize(display)
                ]
                if here:
                    stranded.append(
                        f"{where} is refused, but our {here} display{'s' if len(here) == 1 else ''}"
                        f" the same name — the pick had a counterpart after all")

    if forks_read == 0:
        print("no fork carries both a Mids names dump and a contract — nothing to check",
              file=sys.stderr)
        return 2

    print(f"forks read: {forks_read}")
    print(f"Mids names sharing a display inside their own set: {colliding_names}")
    print(f"rotated-away Mids names (refused picks): {len(rotated)}")
    for line in rotated:
        print(f"  {line}")
    print()
    for line in sorted(set(seated)):
        print(f"CLAIM BROKEN: {line}")
    for line in stranded:
        print(f"CLAIM BROKEN: {line}")
    for line in vacuous:
        print(f"CLAIM BROKEN: {line}")
    if len(rotated) > MAX_ROTATED_AWAY:
        print(f"CLAIM BROKEN: {len(rotated)} rotated-away names, adjudicated at "
              f"{MAX_ROTATED_AWAY}. Each one silently declines every pick that names it, so a "
              "grown population is a re-adjudication, not a bigger number.")
    if colliding_names == 0:
        print("CLAIM BROKEN: no Mids display collides with a sibling anywhere, so the row check "
              "above passed on an empty population — the names dump or its reader has changed.")
        return 1
    if seated or stranded or vacuous or len(rotated) > MAX_ROTATED_AWAY:
        return 1
    print(f"ok: no name-map row, forward or reverse, rests on a display Mids gives to more than "
          f"one power in its set; and each of the {len(rotated)} names the reader declines has no "
          f"counterpart here under that display.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
