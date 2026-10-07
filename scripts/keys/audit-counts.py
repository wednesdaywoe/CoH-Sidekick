#!/usr/bin/env python3
"""SECURITY_AUDIT.md's header count is derived from its table, or this fails.

**Why this exists.** The file's own "How to read a row" section says the Status column is the
only place a status lives, and that the header count is "derived from the table below by
counting it, which is now possible". It was not being counted. Measured across the five commits
before this one, the header read one Low too many every time -- 36/35, 35/34, 35/34, 34/33,
31/30 -- because each session decremented the previous header rather than recounting the table.
An inherited number looks exactly like a derived one.

**The off-by-one had a cause, and it is worth stating because it will recur.** F40 is Low and
PARTIAL. The header counted it among the open Lows, which is right -- a PARTIAL row has live
clauses -- and the trailing tally ALSO listed it as "1 partial" beside "50 fixed", which made
`30 open + 50 fixed + 1 partial + 5 refuted` come to 87 against a file of 86. Two defensible
conventions, mixed. So the header now states one: **PARTIAL counts as open**, and the tally
names only the two statuses that are not.

**What is checked**: the four severities, the open total, the partial count, fixed, refuted,
accepted, the row total, the three Reach buckets, and that they add up. Not the titles, not the
anchors -- this is a count, and a check that tried to be a review would be one nobody could keep
green.

**ACCEPTED, added 2026-09-23, is the one status that buys something with a structural rule.** It
means "real, understood, and deliberately not fixed", which four rows needed and no status could
say -- an upstream default nothing can invoke, a worker on a personal subdomain, a key that is
public by design with the RLS behind it measured. The danger is obvious and is the reason the
status did not exist before: every ACCEPTED row is a live mechanism that has stopped being
counted, so the bucket is a place open rows can go to become quiet. The price is that an ACCEPTED
row's section in `docs/security-audit-findings.md` must carry an `**Exit condition.**`, and this
refuses the row otherwise. It does not grade the answer -- "none owed, because X" is a legitimate
exit condition -- only that somebody was made to write one, which is the whole difference between
a decision and a row nobody looked at again.

**The Reach column, added 2026-09-21, is checked structurally as well as by count**, because its
failure mode is not a wrong number. An open row with no reach is a row the triage does not cover,
and that is what this column exists to prevent: the 2026-09-20 split lived in another file as a
sentence naming no ids, and by the next day it described 36 rows in a file of 29 open. A closed
row carrying a reach is the opposite defect and a worse one -- the reach then disagrees with the
Status column about whether the row is live, which is the two-places-for-one-status drift this
file was reshaped to end.

**How to grade it**: `--self-test` runs each rule against a table and header that disagree, and
the cases were chosen by mutating this file rather than by imagination. The first sweep left
**seven of thirteen mutants alive**, every one because the obvious input for a rule was already
being refused by a different rule -- `open + fixed + refuted == total` catches any single wrong
number, so deleting the `fixed` comparison and running "2 fixed where the table says 1" proves
nothing about the comparison. Four cases now isolate a rule by being the only thing that refuses
them: an unrecognised STATUS (in no bucket, so honest counts stop adding up), an unrecognised
SEVERITY (open is right and the four severities stop summing), a duplicated id with a header
updated to match it, and two errors that cancel (`2 fixed, 0 refuted` against `1 fixed,
1 refuted`).

**EVERY single mutation survives, and the sweep was pushed until it could say WHY rather than
leaving that as a number.** Deleting any one of the direct `open`, `total`, `fixed`, `refuted` or
`accepted` comparisons leaves the file green. Deleting them in pairs is what separates two
different reasons, and the full 5-single / 10-pair sweep was run rather than reasoned about:

- **`fixed`, `refuted` and `accepted` are jointly load-bearing, as a group of three.** Every pair
  drawn from them goes red, on that pair's cancelling case; no single one does. None is
  redundant -- each is simply covered by the other two, because the add-up identity leaves the
  closed buckets free to trade against one another and any one surviving comparison pins the
  rest. A one-at-a-time sweep structurally cannot show this, which is why it is not run that way.
- **`open` and `total` are genuinely redundant**, and stayed so when `accepted` was added: every
  pair containing either is green. The per-severity comparisons pin `open` and the add-up
  identity then pins `total`. No input exists that only those refuse.

All five are kept. The redundant two earn their place at the other end: when this fails, it fails
at a human who has to write a correct header, and "the header says 31 open, the table has 30" is
the sentence that tells them what to type. That is a reason to keep a check, not a claim that it
is graded -- and the difference is the whole point of writing this down.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
AUDIT = ROOT / "SECURITY_AUDIT.md"
FINDINGS = ROOT / "docs" / "security-audit-findings.md"

# What an ACCEPTED row has to carry in its findings section. The status means "real, understood,
# and deliberately not fixed", which is a useful thing to be able to say and a dangerous bucket to
# own: every row in it is a live mechanism that stopped being counted. The marker is the price.
# It does not grade the ANSWER -- "none owed, because X" is a legitimate exit condition -- only
# that somebody was made to write one down, which is the difference between a decision and a
# row that went quiet.
EXIT_MARKER = "**Exit condition"

SEVERITIES = ("High", "Medium", "Low", "Info")

# `| [F03](docs/...) | High | OPEN | machine | title | anchor |`
ROW = re.compile(
    r"^\|\s*\[(F\d+)\]\([^)]*\)\s*\|\s*(\w+)\s*\|\s*(\w+)\s*\|\s*([^|]*?)\s*\|"
)

# The Reach column, and the one value that means "this row is not open". A row carrying a reach
# it has no business having is the same drift as a status in two places: the column stops being
# derivable from the Status column and starts being a second opinion about it.
REACHES = ("machine", "cloud", "none")
NO_REACH = "\u2014"

# The two sentences under `## Status`. Spelled out rather than fuzzy-matched: a header this
# check cannot parse is a header that has drifted into a shape nobody agreed to, and saying so
# is more useful than guessing at it.
HEADER = re.compile(
    r"\*\*(?P<open>\d+) open of (?P<total>\d+)\*\* — "
    r"(?P<High>\d+) High, (?P<Medium>\d+) Medium, (?P<Low>\d+) Low, (?P<Info>\d+) Info; "
    # `(?P=open)` rather than `\d+`: "2 of those 15 are PARTIAL" beside "29 open" is a sentence
    # that parses and means nothing, and it is the same class of drift as the count itself.
    r"(?P<partial>\d+) of those (?P=open) are PARTIAL\.\s*\n"
    r"(?P<fixed>\d+) fixed, (?P<refuted>\d+) refuted, (?P<accepted>\d+) accepted\.\s*\n"
    # Added 2026-09-21. The triage it states was a sentence in another file naming no ids, and
    # it described 36 rows while this file held 29 open. A split that is not counted is prose.
    r"Reach: (?P<machine>\d+) machine, (?P<cloud>\d+) cloud, (?P<none>\d+) none\."
)

LIVE = ("OPEN", "PARTIAL")
# The three that are not live. ACCEPTED joined them 2026-09-23: four rows carried finished,
# evidenced decisions -- an upstream default nothing can invoke, a worker on a personal
# subdomain, a public-by-design key measured against production RLS -- and no status could say
# so, so they sat in the open count describing work nobody intended to do.
CLOSED = ("FIXED", "REFUTED", "ACCEPTED")


def rows_of(text: str) -> list[tuple[str, str, str]]:
    """Every findings row, as (id, severity, status, reach)."""
    return [match.groups() for line in text.splitlines() if (match := ROW.match(line))]


def repeated_ids(rows: list[tuple[str, str, str]]) -> list[str]:
    """Ids the table lists more than once. One row per finding is the file's own rule."""
    ids = [row[0] for row in rows]
    return sorted({found for found in ids if ids.count(found) > 1})


def tally(rows: list[tuple[str, str, str]]) -> dict[str, int]:
    """The numbers the header is supposed to state."""
    live = [row for row in rows if row[2] in LIVE]
    counts = {"total": len(rows), "open": len(live)}
    for severity in SEVERITIES:
        counts[severity] = sum(1 for row in live if row[1] == severity)
    counts["partial"] = sum(1 for row in rows if row[2] == "PARTIAL")
    counts["fixed"] = sum(1 for row in rows if row[2] == "FIXED")
    counts["refuted"] = sum(1 for row in rows if row[2] == "REFUTED")
    counts["accepted"] = sum(1 for row in rows if row[2] == "ACCEPTED")
    for reach in REACHES:
        counts[reach] = sum(1 for row in live if row[3] == reach)
    return counts


def sections_of(findings: str) -> dict[str, str]:
    """Each finding's section body in `docs/security-audit-findings.md`, keyed by id."""
    parts = re.split(r"^### (F\d+) — .*$", findings, flags=re.M)
    return {parts[i]: parts[i + 1] for i in range(1, len(parts) - 1, 2)}


def disagreements(text: str, findings: str = "") -> list[str]:
    """Every number in the header that the table does not support."""
    rows = rows_of(text)
    if not rows:
        return ["no findings rows were parsed at all, so this check proved nothing"]
    if duplicated := repeated_ids(rows):
        return [f"{', '.join(duplicated)} is listed more than once; one row per finding"]

    header = HEADER.search(text)
    if header is None:
        return [
            (
                "the `## Status` header is not in the shape this check reads. It must be "
                "`**N open of M** — a High, b Medium, c Low, d Info; p of those N are "
                "PARTIAL.` then `f fixed, r refuted, k accepted.` then "
                "`Reach: x machine, y cloud, z none.`"
            )
        ]

    # A reach is a claim about a LIVE row. On a closed one it is a second opinion about the
    # Status column, which is the one thing this file's shape forbids; on an open one its
    # absence is the 2026-09-20 triage all over again, a split that names no ids.
    if misreached := [
        f"{row[0]} is {row[2]} and its Reach is {row[3]!r}; an open row needs one of "
        f"{', '.join(REACHES)}"
        for row in rows
        if row[2] in LIVE and row[3] not in REACHES
    ] + [
        f"{row[0]} is {row[2]} and carries Reach {row[3]!r}; a row that is not open has no live "
        f"reach and takes {NO_REACH!r}"
        for row in rows
        if row[2] not in LIVE and row[3] != NO_REACH
    ]:
        return misreached

    # An ACCEPTED row is a live mechanism that has stopped being counted, so the one thing it
    # must not be is silent about how it stops being accepted.
    bodies = sections_of(findings)
    if unconditioned := [
        (
            f"{row[0]} is ACCEPTED and its section in docs/security-audit-findings.md carries no "
            f"{EXIT_MARKER}.** -- an accepted row states what would end the acceptance, even if "
            f"that is 'none owed', or it is just an open row that went quiet"
        )
        if row[0] in bodies
        else f"{row[0]} is ACCEPTED and has no section in docs/security-audit-findings.md"
        for row in rows
        if row[2] == "ACCEPTED" and EXIT_MARKER not in bodies.get(row[0], "")
    ]:
        return unconditioned

    counted = tally(rows)
    stated = {key: int(value) for key, value in header.groupdict().items()}

    problems = [
        f"the header says {stated[key]} {label}, the table has {counted[key]}"
        for key, label in (
            ("open", "open"),
            ("total", "findings in total"),
            ("partial", "PARTIAL"),
            ("fixed", "fixed"),
            ("refuted", "refuted"),
            ("accepted", "accepted"),
            *((severity, f"open {severity}") for severity in SEVERITIES),
            *((reach, f"open rows reaching {reach}") for reach in REACHES),
        )
        if stated[key] != counted[key]
    ]

    by_severity = sum(stated[severity] for severity in SEVERITIES)
    if by_severity != stated["open"]:
        problems.append(
            f"the header's severities come to {by_severity} and it claims {stated['open']} open"
        )
    by_reach = sum(stated[reach] for reach in REACHES)
    if by_reach != stated["open"]:
        problems.append(
            f"the header's reaches come to {by_reach} and it claims {stated['open']} open"
        )
    whole = stated["open"] + stated["fixed"] + stated["refuted"] + stated["accepted"]
    if whole != stated["total"]:
        problems.append(
            f"open + fixed + refuted + accepted is {whole}, not the {stated['total']} findings "
            f"claimed. "
            f"PARTIAL counts as open here and must not be added a second time"
        )
    return problems


def self_test() -> int:
    """Grade every rule against a header its table refuses. A check nobody has seen fail is a wish."""
    failures = []
    table = (
        "| [F01](docs/x.md#a) | High | OPEN | machine | t | a |\n"
        "| [F02](docs/x.md#b) | Low | PARTIAL | cloud | t | a |\n"
        "| [F03](docs/x.md#c) | Medium | FIXED | — | t | a |\n"
        "| [F04](docs/x.md#d) | Info | REFUTED | — | t | a |\n"
        "| [F05](docs/x.md#e) | Info | ACCEPTED | — | t | a |\n"
    )
    honest = (
        "**2 open of 5** — 1 High, 0 Medium, 1 Low, 0 Info; 1 of those 2 are PARTIAL.\n"
        "1 fixed, 1 refuted, 1 accepted.\n"
        "Reach: 1 machine, 1 cloud, 0 none.\n"
    )
    # The findings file the exit-condition rule reads. Only the ACCEPTED row needs a section.
    bodies = "### F05 — t\n\n**Exit condition.** None owed; the mechanism is the decision.\n"

    def refused(text: str, findings: str = bodies) -> list[str]:
        return disagreements(text, findings)

    if problems := refused(honest + table):
        failures.append(f"a header the table supports was reported: {problems}")

    # Each of these is one number changed, and each must be caught on its own.
    drifts = {
        "an open total one too high": ("**2 open of 5**", "**3 open of 5**"),
        "a severity the table does not support": ("1 Low, 0 Info", "2 Low, 0 Info"),
        "a PARTIAL count the table does not support": ("1 of those 2", "2 of those 2"),
        "a fixed count the table does not support": ("1 fixed", "2 fixed"),
        "a refuted count the table does not support": ("1 refuted", "2 refuted"),
        "an accepted count the table does not support": ("1 accepted", "2 accepted"),
        # The exact drift this file carried: the PARTIAL row counted as open AND added again
        # beside fixed, so the total ran one ahead of the table.
        "a total that counts the PARTIAL row twice": ("open of 5", "open of 6"),
        # ISOLATES the reach comparison. Trading one bucket for another keeps the reaches
        # summing to `open`, so the sum rule is blind to it and only the direct comparison
        # refuses it -- the same trick as the cancelling cases below.
        "a reach split that trades machine for cloud": (
            "Reach: 1 machine, 1 cloud",
            "Reach: 0 machine, 2 cloud",
        ),
    }
    for why, (before, after) in drifts.items():
        assert before in honest, why
        if not refused(honest.replace(before, after, 1) + table):
            failures.append(f"{why} was accepted")

    if not refused("## Status\n\nthirty-odd open, give or take.\n" + table):
        failures.append("a header in no recognised shape was accepted")

    # THE CASES BELOW ISOLATE A RULE. A first sweep of this file left seven mutants alive, all
    # because the obvious case for a rule was already being caught by a different rule -- "2
    # fixed" where the table says 1 also breaks the add-up identity, so deleting the fixed
    # comparison changed nothing. A rule is only graded by an input that ONLY it refuses.

    # An unrecognised status belongs to no bucket, so the four counts can all be honest and
    # still not add up. Only the add-up rule sees this.
    invented = table + "| [F06](docs/x.md#f) | Low | MOSTLY | — | t | a |\n"
    invented_header = (
        "**2 open of 6** — 1 High, 0 Medium, 1 Low, 0 Info; 1 of those 2 are PARTIAL.\n"
        "1 fixed, 1 refuted, 1 accepted.\n"
        "Reach: 1 machine, 1 cloud, 0 none.\n"
    )
    if not refused(invented_header + invented):
        failures.append("a row whose status is in no bucket was absorbed; the counts stop adding up")

    # An unrecognised severity is the same trick one column over: the row is open, so the open
    # total is right, and it lands in none of the four severities. Only the severities-sum rule
    # sees this.
    odd_severity = table + "| [F06](docs/x.md#f) | Critical | OPEN | machine | t | a |\n"
    odd_header = (
        "**3 open of 6** — 1 High, 0 Medium, 1 Low, 0 Info; 1 of those 3 are PARTIAL.\n"
        "1 fixed, 1 refuted, 1 accepted.\n"
        "Reach: 2 machine, 1 cloud, 0 none.\n"
    )
    if not refused(odd_header + odd_severity):
        failures.append("a severity outside the four was absorbed; the severities stop summing")

    # A duplicated id with a header that has been updated to match it. Every count agrees and
    # the file still lists one finding twice, which is the thing the Status column relies on.
    twice = table + "| [F01](docs/x.md#a) | High | OPEN | machine | t | a |\n"
    twice_header = (
        "**3 open of 6** — 2 High, 0 Medium, 1 Low, 0 Info; 1 of those 3 are PARTIAL.\n"
        "1 fixed, 1 refuted, 1 accepted.\n"
        "Reach: 2 machine, 1 cloud, 0 none.\n"
    )
    if not refused(twice_header + twice):
        failures.append("a finding listed twice was accepted once the header was made to agree")

    # ERRORS THAT CANCEL. The add-up identity is what catches a single wrong number, so a single
    # wrong number grades nothing about the direct comparisons. Move one row's worth between two
    # closed buckets and the identity still holds while both numbers are wrong. All three pairs
    # are run because the three closed comparisons are load-bearing as a GROUP and not
    # individually: no single one of them can be isolated, and each pair's case is what reds when
    # that pair is deleted. See the module docstring's sweep.
    for why, line in (
        ("fixed and refuted", "2 fixed, 0 refuted, 1 accepted."),
        ("fixed and accepted", "2 fixed, 1 refuted, 0 accepted."),
        ("refuted and accepted", "1 fixed, 2 refuted, 0 accepted."),
    ):
        cancelling = honest.replace("1 fixed, 1 refuted, 1 accepted.", line, 1)
        if not refused(cancelling + table):
            failures.append(f"a wrong {why} that cancel each other were accepted")

    # A header of zeroes over no table at all. Every comparison passes and nothing was checked.
    zeroed = (
        "**0 open of 0** — 0 High, 0 Medium, 0 Low, 0 Info; 0 of those 0 are PARTIAL.\n"
        "0 fixed, 0 refuted, 0 accepted.\n"
        "Reach: 0 machine, 0 cloud, 0 none.\n"
    )
    if not refused(zeroed):
        failures.append("a zeroed header over an empty table was accepted; that proves nothing")

    # AN OPEN ROW WITH NO REACH. Every count in the header can be honest -- it is open, it is
    # High, it is neither fixed nor refuted -- and the triage still does not cover it. This is
    # the 2026-09-20 split in miniature, and only the structural rule refuses it.
    unreached = table.replace("| High | OPEN | machine |", "| High | OPEN | — |", 1)
    unreached_header = (
        "**2 open of 5** — 1 High, 0 Medium, 1 Low, 0 Info; 1 of those 2 are PARTIAL.\n"
        "1 fixed, 1 refuted, 1 accepted.\n"
        "Reach: 0 machine, 1 cloud, 0 none.\n"
    )
    if not refused(unreached_header + unreached):
        failures.append("an open row with no reach was accepted; the triage does not cover it")

    # A CLOSED ROW CARRYING A REACH. The reach then disagrees with the Status column about
    # whether the row is live, which is exactly the two-places-for-one-status drift that this
    # file was reshaped to end. Graded on ACCEPTED as well as FIXED, because a newly closed
    # status is exactly the one whose rows still carry the reach they had while open.
    for before, after in (
        ("| Medium | FIXED | — |", "| Medium | FIXED | cloud |"),
        ("| Info | ACCEPTED | — |", "| Info | ACCEPTED | machine |"),
    ):
        if not refused(honest + table.replace(before, after, 1)):
            failures.append(f"a closed row carrying a live reach was accepted: {after}")

    # AN ACCEPTED ROW THAT NAMES NOTHING THAT WOULD END THE ACCEPTANCE. Every number is honest;
    # the row is simply an open one that went quiet, which is the failure this status invites
    # and the only reason it is affordable to have.
    if not refused(honest + table, findings="### F05 — t\n\nAccepted because it is fine.\n"):
        failures.append("an ACCEPTED row with no exit condition was accepted")
    if not refused(honest + table, findings=""):
        failures.append("an ACCEPTED row with no findings section at all was accepted")

    for failure in failures:
        print(f"SELF-TEST FAILED: {failure}", file=sys.stderr)
    if failures:
        return 1
    print("self-test: every number in the header is checked against the table")
    return 0


def main() -> int:
    if "--self-test" in sys.argv:
        return self_test()

    text = AUDIT.read_text(encoding="utf-8")
    findings = FINDINGS.read_text(encoding="utf-8") if FINDINGS.exists() else ""
    problems = disagreements(text, findings)
    for problem in problems:
        print(f"::error::SECURITY_AUDIT.md: {problem}", file=sys.stderr)
    if problems:
        print(
            "::error::the header count is meant to be derived from the table by counting it. "
            "Recount it; do not decrement the number that was there.",
            file=sys.stderr,
        )
        return 1

    counted = tally(rows_of(text))
    print(
        f"SECURITY_AUDIT.md: {counted['open']} open of {counted['total']} "
        f"({counted['partial']} PARTIAL), {counted['fixed']} fixed, {counted['refuted']} refuted, "
        f"{counted['accepted']} accepted; "
        f"reach {counted['machine']} machine / {counted['cloud']} cloud / {counted['none']} none "
        f"— header agrees with the table"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
