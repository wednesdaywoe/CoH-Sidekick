#!/usr/bin/env python3
"""Flag CoH proper nouns in comments that are ALMOST a real name.

The existing sweep is change-driven: it starts
from something you renamed, moved or deleted and hunts for comments still using
the old name. That catches a comment which BECAME wrong. It cannot catch one
that was never right, because nothing was renamed to trigger it.

"Organic Armor" was the suspected example and turned out NOT to be a fault:
it is a real Thunderspy-only powerset (exported_powers/thunderspy/*/organic_armor/,
"An advanced variant of Bio Armor"), distinct from Homecoming's Bio-Organic
Armor. This linter stayed correctly silent on it. That is the point -- the data
settles a name question that reading prose could not, and a reviewer who only
knows Homecoming will suspect a real name is wrong.

So this checks the other direction. Domain proper nouns are enumerable -- every
power, powerset and IO set name is in the data -- so a capitalised phrase in a
comment can be checked against them. It reports only NEAR MISSES: a phrase that
is not itself a name but is contained in one, or is one small edit away. An
exactly-correct name stays silent, and ordinary English stays silent, which is
what keeps this cheap enough to leave switched on.

    python3 scripts/lint-proper-nouns.py            # report
    python3 scripts/lint-proper-nouns.py --quiet    # exit 1 only, for CI
"""
import json, re, sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DATA = [ROOT / "exported_powers", ROOT / "contract"]
SRC = ROOT / "crates"

# Phrases that look like proper nouns and are not. Kept short on purpose: every
# entry here is a hole in the check, so add one only with a reason.
STOP = {
    "Max HP", "Data Gap", "Rule Of", "See Also", "Note That", "The Bag",
    "Schedule A", "Schedule B", "Schedule C", "Schedule D", "Schedule E",
    # Rarity tiers. Every incarnate component is "<Aspect> Very Rare", so the
    # bare tier looks like ten truncated names and is none of them.
    "Very Rare", "Super Rare", "Ultra Rare",
    # Attribute vocabulary. Enhancements are named "<Origin> <Aspect>" ("Generic
    # Endurance Discount", "Hamidon Buff Recharge"), so the bare aspect reads as
    # a truncation of dozens of them. It is what the aspect is CALLED.
    "Endurance Discount", "Buff Recharge", "Endurance Modification",
}

COMMENT = re.compile(r"^\s*(?://[/!]?|\*)\s?(.*)$")
# Two or more capitalised words, allowing internal hyphens: "Bio-Organic Armor".
PHRASE = re.compile(r"\b([A-Z][a-zA-Z]*(?:-[A-Z][a-zA-Z]*)*(?:\s+[A-Z][a-zA-Z]*(?:-[A-Z][a-zA-Z]*)*)+)\b")


def norm(s):
    """Compare on words only: 'Bio_Organic_Armor' == 'Bio-Organic Armor'."""
    return " ".join(re.split(r"[^a-z0-9]+", s.lower())).strip()


def build_index():
    """Every power, powerset and IO-set name the data knows."""
    names = {}
    for root in DATA:
        if not root.exists():
            continue
        for f in root.rglob("*.json"):
            try:
                d = json.loads(f.read_text(errors="replace"))
            except Exception:
                continue
            for item in (d if isinstance(d, list) else [d]):
                if not isinstance(item, dict):
                    continue
                for key in ("display_name", "name", "displayName"):
                    v = item.get(key)
                    if isinstance(v, str) and 2 < len(v) < 70:
                        names.setdefault(norm(v), v)
                # IO sets carry their pieces, and a piece name is an aspect label
                for piece in (item.get("pieces") or []):
                    if isinstance(piece, dict) and isinstance(piece.get("name"), str):
                        names.setdefault(norm(piece["name"]), piece["name"])
    return names


def near_misses(phrase, index):
    """A phrase that is not a name but is nearly one. Empty when it is fine.

    Deliberately narrow. The first cut matched any phrase CONTAINED in any name
    and produced 108 hits, nearly all noise: 'Set Bonuses' against 'Set Bonus',
    and 'Status Resistance' against every enhancement piece whose compound label
    happens to include those words. Neither is a misnamed entity.

    The real failure mode is a name with its leading word(s) dropped -- 'Organic
    Armor' for 'Bio-Organic Armor' -- so that is what is matched: the phrase must
    be a SUFFIX of a real name, short by no more than two words. Plurals are not
    misnames and are ignored.
    """
    n = norm(phrase)
    if not n or n in index:
        return []
    words = n.split()
    if len(words) < 2:
        return []
    if n.rstrip("s") in {k.rstrip("s") for k in index}:
        return []
    out = []
    for key, original in index.items():
        # Compound labels ("Impervious Skin: Status Resistance", "Aegis:
        # Psionic/Status Resistance") are a set name glued to an aspect list, not
        # an entity whose leading word someone dropped. Their tails match generic
        # vocabulary -- 'Status Resistance', 'Fast Snipe' -- and were the whole
        # of the second round of noise. Parenthesised ones ("Adrenal Modifier
        # (Endurance Modification)") are the same shape and were the third.
        # Internal keys, not display names: "Crafted_Endurance_Discount",
        # "DSync_Buff_Recharge". Their tails are ordinary attribute vocabulary
        # ("Endurance Discount", "Buff Recharge"), which is what an aspect is
        # CALLED, not an entity someone half-named.
        if "_" in original:
            continue
        if any(c in original for c in ":/()"):
            continue
        kw = key.split()
        gap = len(kw) - len(words)
        # "The Labyrinth" is not a truncation of "Conqueror of the Labyrinth":
        # the dropped words end in a preposition, so the phrase is a fragment of
        # a longer title rather than the title minus its qualifier.
        if not (1 <= gap <= 2) or kw[gap:] != words:
            continue
        if kw[gap - 1] in ("of", "the", "and", "for", "in", "to"):
            continue
        out.append(original)
    return sorted(set(out))[:3]


def main():
    quiet = "--quiet" in sys.argv
    index = build_index()
    if not index:
        print("no name index built -- is exported_powers/ present?", file=sys.stderr)
        return 2

    hits = []
    for f in sorted(SRC.rglob("*.rs")):
        for lineno, line in enumerate(f.read_text(errors="replace").split("\n"), 1):
            m = COMMENT.match(line)
            if not m:
                continue
            for phrase in PHRASE.findall(m.group(1)):
                if phrase in STOP:
                    continue
                sugg = near_misses(phrase, index)
                if sugg:
                    hits.append((f.relative_to(ROOT), lineno, phrase, sugg))

    if not quiet:
        if not hits:
            print(f"proper-noun lint: clean ({len(index)} names indexed)")
        else:
            print(f"proper-noun lint: {len(hits)} near-miss(es), {len(index)} names indexed\n")
            for path, lineno, phrase, sugg in hits:
                print(f"{path}:{lineno}")
                print(f"    wrote  {phrase!r}")
                print(f"    meant  {' | '.join(repr(s) for s in sugg)}?\n")
    return 1 if hits else 0


if __name__ == "__main__":
    sys.exit(main())
