#!/usr/bin/env python3
"""Dump the planner's explanatory text to a file you can edit, and write your edits back.

    python3 scripts/ui-strings.py --dump     # code  -> docs/ui-strings.toml
    python3 scripts/ui-strings.py --apply    # file  -> the .rs literals
    python3 scripts/ui-strings.py --self-test

WHAT IT TAKES. Four kinds, and they are the ones that EXPLAIN the UI rather than label it:
`title:` tooltips, `"aria-label":`, `placeholder:`, and text nodes inside an element whose class
names it a hint, an empty state, a notice or a caption. Button labels, headings and column names
are deliberately out: they are one or two words, they outnumber the prose four to one, and a file
carrying them buries the sentences somebody actually wants to reword.

WHAT THE FILE IS. A lens on the code, not a store beside it. Nothing at runtime reads it, the
Rust literals stay where they are, and deleting both the file and this script leaves the tree
exactly as it was. That is the whole reason to prefer it over a string table: no indirection, no
key to keep in step, and every edit lands as an ordinary reviewable diff.

`original` is the anchor and `text` is yours. `--apply` rewrites a site only when `text` differs
from `original` AND the literal in the file still decodes to `original` -- so an edit made in the
code after the dump is never silently overwritten; the entry is refused by name and the rest go
through. Keys are handles for reading, regenerated on each dump, and nothing matches on them.

ONE DIRECTION IS DESTRUCTIVE. `--dump` does not merge with the file on disk; it replaces every
`text` with what the code says. Any rewording typed in and not yet applied goes with it, and the
moment that is easiest to walk into is a refused entry, because starting over is the obvious
response to one. So `--dump` refuses while the file holds wording the code does not have yet,
names the entries, and points at `--apply`; `--force` is the way through on purpose. Wording
already written into the code does not count, or the guard would block the refresh that `--apply`
asks for next.

THREE THINGS THAT MAKE THIS MORE THAN A grep, all of them measured in this tree:

  * **Placeholders.** 457 of the app's user-facing strings carry `{...}`. An edit that drops one
    either fails the build or prints a literal brace at a user, so `--apply` compares the multiset
    of placeholders and refuses the entry rather than writing it.

  * **One sentence, several literals.** rsx concatenates adjacent strings, so a hint wider than
    the file's margin is written as a run of them -- `build_io.rs:608-610` is one sentence in
    three pieces. The run is fused into one entry to edit and re-wrapped on the way back, at the
    indent and width it was found with. A tool that edited the pieces would be asking somebody to
    reword a sentence they cannot see the end of.

  * **One string, several sites.** 97 texts appear at more than one call site (218 sites). They
    become ONE entry listing every site, so rewording it moves all of them -- the empty-state hint
    in `panels/powers.rs` is at two, and an edit that moved one would read as a bug in the other.

WHAT IT IS BLIND TO. It reads the source as text, not as a parsed rsx tree, so it sees a text node
by the shape of the line it sits on: a line holding nothing but a string literal, inside an
element whose class matched. A hint assembled by a function, pushed through a `format!`, or
returned from a `match` arm is not reachable this way and is not in the file. That is a floor on
coverage, not a correctness risk -- what it does not see, it does not touch.
"""

from __future__ import annotations

import argparse
import pathlib
import re
import sys
import tempfile
import tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent
SRC = ROOT / "crates" / "app" / "src"
DEFAULT_FILE = ROOT / "docs" / "ui-strings.toml"

# The margin the app's rsx is hand-wrapped to. A literal that still fits stays on one line; one
# that outgrows it is re-wrapped in whichever form its site allows -- see `emit_literal`.
WRAP = 100

# An element whose class says it is explanatory. Matched against the class VALUE, so
# `sb-save__hint` and `powers__empty` are in and `feedback__check` is not.
HINTISH = re.compile(r"(hint|empty|notice|note|caption|blurb|placeholder|__desc|__sub)")

ATTRS = {
    "title:": "tooltip",
    '"aria-label":': "aria-label",
    "placeholder:": "placeholder",
}


# --------------------------------------------------------------------------------------------
# Reading Rust string literals
#
# Written out rather than regexed because two forms in this tree defeat a regex: an escaped quote
# inside the text, and a literal continued across lines with a trailing backslash (which eats the
# newline AND the next line's indent -- `feedback.rs`'s bug-report placeholder is one).
# --------------------------------------------------------------------------------------------

ESCAPES = {"n": "\n", "t": "\t", "r": "\r", "0": "\0", '"': '"', "\\": "\\", "'": "'"}


def scan_literal(src: str, i: int):
    """Decode the Rust string literal starting at `src[i] == '"'`.

    Returns `(text, end)` with `end` one past the closing quote, or `None` when the literal is
    raw (`r"..."`), unterminated, or carries an escape this does not model. `None` means "leave
    this one alone", which is the only safe default: a literal decoded wrongly would be written
    back wrongly.
    """
    if i >= len(src) or src[i] != '"':
        return None
    if i > 0 and src[i - 1] in "r#":  # r"..." and r#"..."# -- not modelled, not touched
        return None
    out = []
    j = i + 1
    while j < len(src):
        c = src[j]
        if c == '"':
            return "".join(out), j + 1
        if c != "\\":
            out.append(c)
            j += 1
            continue
        j += 1
        if j >= len(src):
            return None
        e = src[j]
        if e == "\n":
            # A continuation: the newline and the whole of the next line's indent vanish.
            j += 1
            while j < len(src) and src[j] in " \t":
                j += 1
            continue
        if e in ESCAPES:
            out.append(ESCAPES[e])
            j += 1
            continue
        if e == "u" and j + 1 < len(src) and src[j + 1] == "{":
            close = src.find("}", j)
            if close < 0:
                return None
            out.append(chr(int(src[j + 2 : close], 16)))
            j = close + 1
            continue
        return None  # an escape we do not model
    return None


def encode_literal(text: str) -> str:
    """The Rust source for one literal, as a single `"..."` with no line breaks."""
    out = []
    for ch in text:
        if ch == "\\":
            out.append("\\\\")
        elif ch == '"':
            out.append('\\"')
        elif ch == "\n":
            out.append("\\n")
        elif ch == "\t":
            out.append("\\t")
        elif ch == "\r":
            out.append("\\r")
        else:
            out.append(ch)
    return '"' + "".join(out) + '"'


def wrap_literals(text: str, indent: int) -> list[str]:
    """`text` as a run of ADJACENT literals, one per line, wrapped to the margin.

    The space between two words lands at the END of a piece rather than the start of the next,
    which is what rsx's concatenation needs and what the hand-written runs in this tree already
    do. A word too long for the margin gets its own over-long line rather than being broken.

    **Only valid inside `rsx!`.** Adjacent literals concatenate there and nowhere else in Rust:
    `"a" "b"` in a `match` arm is a syntax error, not a string. The first cut of this tool used
    adjacency for every site and broke `Topic::placeholder()` the first time that entry was
    edited -- which is why the emit style is recorded per site at extraction rather than decided
    here. See `wrap_continuation` for the form the rest of the language takes.
    """
    budget = max(24, WRAP - indent - 2)
    words = text.split(" ")
    lines: list[str] = []
    cur = ""
    for w, word in enumerate(words):
        piece = word if w == len(words) - 1 else word + " "
        if cur and len(encode_literal(cur + piece)) > budget:
            lines.append(cur)
            cur = piece
        else:
            cur += piece
    if cur or not lines:
        lines.append(cur)
    return [encode_literal(line) for line in lines]


def wrap_continuation(text: str, indent: int) -> str:
    """`text` as ONE literal, carried across lines with trailing backslashes.

    The form every string outside `rsx!` has to take. A `\\` at end of line eats the newline and
    the next line's indent, so -- exactly as with adjacency -- the space between two words has to
    sit before the break, and the indent that follows is cosmetic. Splitting is done on the
    ESCAPED body so a break can never land inside a `\\n`.
    """
    body = encode_literal(text)[1:-1]
    budget = max(24, WRAP - indent - 2)
    words = body.split(" ")
    lines: list[str] = []
    cur = ""
    for w, word in enumerate(words):
        piece = word if w == len(words) - 1 else word + " "
        if cur and len(cur) + len(piece) > budget:
            lines.append(cur)
            cur = piece
        else:
            cur += piece
    if cur or not lines:
        lines.append(cur)
    pad = " " * (indent + 1)
    return '"' + ("\\\n" + pad).join(lines) + '"'


def emit_literal(text: str, indent: int, style: str, was_wrapped: bool) -> str:
    """The source form for one site: one literal, an adjacent run, or a continuation.

    Style is a property of WHERE the literal sits and of nothing else, so length decides only
    WHETHER to wrap, never HOW. An earlier cut asked it the other way round and let an over-long
    edit at an `adjacent` site fall through to a continuation. That parses, and it renders the
    right string, so nothing downstream complained -- what it broke was the way back. The hint
    finder recognises a text node by the shape of the line it sits on, and the first line of a
    continued literal is an unterminated one, so three hints silently left the file the moment
    they were reworded. A string you can edit once and then never find again is the failure this
    file exists to prevent, so the round trip is asserted on this function in --self-test rather
    than on the halves of it.
    """
    if not was_wrapped and len(encode_literal(text)) <= WRAP - indent:
        return encode_literal(text)
    if style == "adjacent":
        return ("\n" + " " * indent).join(wrap_literals(text, indent))
    return wrap_continuation(text, indent)


# --------------------------------------------------------------------------------------------
# Finding the strings
# --------------------------------------------------------------------------------------------

BARE = re.compile(r'^\s*("(?:[^"\\]|\\.)*")\s*,?\s*$')


def line_of(src: str, offset: int) -> int:
    return src.count("\n", 0, offset) + 1


def find_attrs(src: str):
    """`title:`, `"aria-label":` and `placeholder:` literals, wherever they sit."""
    for marker, kind in ATTRS.items():
        start = 0
        while True:
            at = src.find(marker, start)
            if at < 0:
                break
            start = at + len(marker)
            q = start
            while q < len(src) and src[q] in " \t":
                q += 1
            got = scan_literal(src, q)
            if got:
                text, end = got
                if text.strip():
                    yield kind, text, q, end, "single"


def element_span(src: str, class_at: int):
    """The source range of the element whose `class:` attribute is at `class_at`.

    Found by walking back to the `{` that opened it and forward to the matching `}`. Crude, and
    deliberately so: it does not need to understand rsx, only to bound the lines a text node of
    this element could be on.
    """
    open_at = src.rfind("{", 0, class_at)
    if open_at < 0:
        return None
    depth = 0
    for k in range(open_at, len(src)):
        if src[k] == "{":
            depth += 1
        elif src[k] == "}":
            depth -= 1
            if depth == 0:
                return open_at, k
    return None


def find_hint_runs(src: str):
    """Runs of adjacent text-node literals inside hint-ish elements.

    A run is consecutive lines each holding nothing but a string literal. Consecutive is what
    fuses `build_io.rs:608-610` into one sentence; anything between them -- a brace, an `if`, a
    blank line -- ends the run, which is what keeps the two arms of a hint that reads one way
    ticked and another way unticked from being welded into one nonsense string.
    """
    lines = src.split("\n")
    offsets = []
    pos = 0
    for line in lines:
        offsets.append(pos)
        pos += len(line) + 1

    seen: set[tuple[int, int]] = set()
    for m in re.finditer(r'class:\s*"([^"]*)"', src):
        if not HINTISH.search(m.group(1)):
            continue
        span = element_span(src, m.start())
        if not span:
            continue
        open_at, close_at = span
        first, last = line_of(src, open_at), min(line_of(src, close_at), len(lines))

        run: list[tuple[int, int, str]] = []
        runs: list[list[tuple[int, int, str]]] = []
        for n in range(first, last + 1):
            hit = BARE.match(lines[n - 1])
            got = None
            if hit:
                q = offsets[n - 1] + hit.start(1)
                got = scan_literal(src, q)
            if not hit or not got or not got[0].strip():
                if run:
                    runs.append(run)
                    run = []
                continue
            text, end = got
            run.append((offsets[n - 1] + hit.start(1), end, text))
        if run:
            runs.append(run)

        for r in runs:
            # Nested hint-ish elements would otherwise yield the same run twice, once per
            # enclosing class. The span is the identity.
            ident = (r[0][0], r[-1][1])
            if ident in seen:
                continue
            seen.add(ident)
            yield "hint", "".join(p[2] for p in r), r[0][0], r[-1][1], "adjacent"


# A function whose NAME says its return value explains something, and whose return type says the
# value is a literal. `Topic::placeholder()` is the one that made this worth having: the feedback
# form's bug-report prompt is the longest hint in the app and reaches the DOM as
# `placeholder: "{topic().placeholder()}"`, so attribute scanning sees the call and never the
# words. `label()` is deliberately not here -- a label is the naming this file's scope excludes.
HINT_FN = re.compile(r"fn\s+(title|hint|note|placeholder)\s*\([^)]*\)\s*->\s*&'static str\s*\{")
FN_KIND = {"title": "tooltip", "hint": "hint", "note": "hint", "placeholder": "placeholder"}

# Only a literal that is plainly the value being RETURNED. A prose string compared against, rather
# than handed back, would be logic -- and an editable entry that silently rewrites a comparison is
# the one way this tool could break something instead of merely missing it.
RETURNED = re.compile(r"(=>|return\s|^\s*)$")


def find_hint_fns(src: str):
    """Prose returned from the small set of functions named for explaining something."""
    for m in HINT_FN.finditer(src):
        depth, body_end = 0, len(src)
        for k in range(m.end() - 1, len(src)):
            if src[k] == "{":
                depth += 1
            elif src[k] == "}":
                depth -= 1
                if depth == 0:
                    body_end = k
                    break
        i = m.end()
        while True:
            q = src.find('"', i)
            if q < 0 or q >= body_end:
                break
            got = scan_literal(src, q)
            if not got:
                i = q + 1
                continue
            text, end = got
            line_start = src.rfind("\n", 0, q) + 1
            before = src[line_start:q].rstrip()
            if len(text) >= 20 and " " in text and RETURNED.search(before):
                yield FN_KIND[m.group(1)], text, q, end, "single"
            i = end


def placeholders(text: str) -> list[str]:
    """The `{...}` forms in a string, with `{{` and `}}` (an escaped brace) left out."""
    cleaned = text.replace("{{", "\0").replace("}}", "\0")
    return sorted(re.findall(r"\{[^{}]*\}", cleaned))


def slug(text: str) -> str:
    words = re.findall(r"[a-z0-9]+", text.lower())
    out = "_".join(words[:6])
    return out[:48] or "text"


def extract():
    """Every entry, keyed by (kind, text) so repeated strings become one entry with many sites."""
    merged: dict[tuple[str, str], dict] = {}
    for path in sorted(SRC.rglob("*.rs")):
        src = path.read_text(encoding="utf-8")
        cut = src.find("#[cfg(test)]")
        limit = len(src) if cut < 0 else cut
        rel = str(path.relative_to(SRC))
        found = list(find_attrs(src)) + list(find_hint_runs(src)) + list(find_hint_fns(src))
        for kind, text, start, end, style in found:
            if start >= limit:
                continue
            key = (kind, text)
            entry = merged.setdefault(
                key, {"kind": kind, "original": text, "sites": [], "placeholders": placeholders(text)}
            )
            entry["sites"].append(
                {"file": rel, "start": start, "end": end, "line": line_of(src, start), "style": style}
            )
    # Stable, readable ordering: by the first file a string appears in, then by line.
    out = sorted(merged.values(), key=lambda e: (e["sites"][0]["file"], e["sites"][0]["line"]))
    seen: dict[str, int] = {}
    for e in out:
        base = f"{pathlib.Path(e['sites'][0]['file']).stem}.{slug(e['original'])}"
        seen[base] = seen.get(base, 0) + 1
        e["key"] = base if seen[base] == 1 else f"{base}_{seen[base]}"
    return out


# --------------------------------------------------------------------------------------------
# The file
# --------------------------------------------------------------------------------------------

HEADER = """\
# The planner's explanatory text -- every tooltip, aria-label, placeholder and panel hint.
#
# GENERATED by `python3 scripts/ui-strings.py --dump`. Edit the `text` of any entry and run
# `python3 scripts/ui-strings.py --apply` to write it back into the Rust. Then re-dump: the
# keys are derived from the text and move when you reword it, which is fine, because nothing
# matches on them.
#
#   original      what the code says right now. The anchor: --apply refuses an entry whose
#                 site no longer holds this, rather than overwriting an edit made in the code.
#   text          what you want it to say. Equal to `original` until you change it.
#   sites         every call site this one string is rendered from. Editing moves all of them.
#   placeholders  the {...} forms the text must keep. --apply refuses an edit that loses one.
#
# --apply first, THEN re-dump. A dump rewrites every `text` from the code, so it discards any
# rewording still waiting here -- it refuses while there is one, and --force overrides it.
#
# Not read at runtime: this file is a lens on the code, not a string table beside it.
"""


def toml_string(text: str) -> str:
    if "\n" in text:
        body = text.replace("\\", "\\\\").replace('"""', '\\"\\"\\"')
        return '"""\n' + body + '"""'
    body = (
        text.replace("\\", "\\\\")
        .replace('"', '\\"')
        .replace("\t", "\\t")
        .replace("\r", "\\r")
    )
    return '"' + body + '"'


def write_file(entries, dest: pathlib.Path) -> None:
    out = [HEADER]
    for e in entries:
        sites = ", ".join(f'"{s["file"]}:{s["line"]}"' for s in e["sites"])
        out.append(f"\n[{e['key']}]")
        out.append(f'kind = "{e["kind"]}"')
        out.append(f"sites = [{sites}]")
        if e["placeholders"]:
            out.append("placeholders = [" + ", ".join(toml_string(p) for p in e["placeholders"]) + "]")
        out.append("original = " + toml_string(e["original"]))
        out.append("text = " + toml_string(e["original"]))
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_text("\n".join(out) + "\n", encoding="utf-8")


def read_file(path: pathlib.Path) -> dict[str, dict]:
    """Every entry in the file, flattened to `key -> row`.

    The keys are `<file stem>.<slug>`, so TOML reads them as a table per file with the entries
    under it -- the grouping a reader wants, and two levels the code has to walk back down.
    """
    raw = tomllib.loads(path.read_text(encoding="utf-8"))
    data: dict[str, dict] = {}
    for group, body in raw.items():
        if not isinstance(body, dict):
            continue
        if "original" in body:  # an entry whose key carried no dot
            data[group] = body
            continue
        for name, row in body.items():
            if isinstance(row, dict):
                data[f"{group}.{name}"] = row
    return data


def pending_edits(path: pathlib.Path) -> list[str]:
    """The keys whose `text` has been reworded and is not in the code yet.

    `text != original` is NOT the test, because it stays true after a successful --apply: the
    code has moved on and the file's anchor has not, until the next dump. Testing it that way
    made the guard refuse the one command --apply tells you to run next, which is a guard that
    trains you to reach for --force. So the question asked here is the one that matters -- is
    this wording in the code? -- and an entry counts as landed only when the code has stopped
    saying `original` and started saying `text`. An edit that was refused still reads as
    pending, which is the case the guard exists for.
    """
    live = {(e["kind"], e["original"]) for e in extract()}
    waiting = []
    for key, row in read_file(path).items():
        original, text, kind = row.get("original"), row.get("text"), row.get("kind")
        if original is None or text is None or kind is None or text == original:
            continue
        landed = (kind, original) not in live and (kind, text) in live
        if not landed:
            waiting.append(key)
    return waiting


# --------------------------------------------------------------------------------------------
# Writing edits back
# --------------------------------------------------------------------------------------------


def apply_edits(path: pathlib.Path, dry_run: bool = False):
    """Returns `(rewrites, refusals)`. Nothing is written when anything is refused."""
    data = read_file(path)
    live = {(e["kind"], e["original"]): e for e in extract()}

    rewrites: list[dict] = []
    refusals: list[str] = []

    for key, row in data.items():
        original, text, kind = row.get("original"), row.get("text"), row.get("kind")
        if original is None or text is None or kind is None:
            refusals.append(f"{key}: entry is missing kind, original or text")
            continue
        if text == original:
            continue
        if not text.strip():
            refusals.append(f"{key}: the new text is empty, which would leave a blank in the UI")
            continue
        want, got = placeholders(original), placeholders(text)
        if want != got:
            lost = [p for p in want if p not in got]
            added = [p for p in got if p not in want]
            detail = []
            if lost:
                detail.append("drops " + ", ".join(lost))
            if added:
                detail.append("adds " + ", ".join(added))
            refusals.append(f"{key}: the edit {' and '.join(detail)} -- placeholders must carry through")
            continue
        entry = live.get((kind, original))
        if entry is None:
            refusals.append(
                f"{key}: nothing in the code says {original[:48]!r} any more -- this entry stopped "
                f"anchoring to the code, because one of the two was edited since the dump. Correct "
                f"`original` to what the code says now and keep your wording in `text`. Do NOT "
                f"re-dump to fix it while other edits are pending: a dump rewrites every `text` "
                f"from the code and would throw them all away."
            )
            continue
        rewrites.append({"key": key, "entry": entry, "text": text})

    if dry_run:
        return rewrites, refusals

    # A refusal is about ONE entry, and the entries are independent literals, so a typo in one
    # does not hold back the other thirty-nine. The exit code still says not everything landed,
    # and every rewrite is an ordinary diff to look at before it goes anywhere.

    # Per file, splice from the back so an earlier rewrite never moves a later site's offsets.
    by_file: dict[str, list[tuple[dict, str]]] = {}
    for r in rewrites:
        for site in r["entry"]["sites"]:
            by_file.setdefault(site["file"], []).append((site, r["text"]))

    for rel, sites in by_file.items():
        full = SRC / rel
        src = full.read_text(encoding="utf-8")
        for site, text in sorted(sites, key=lambda s: -s[0]["start"]):
            start, end = site["start"], site["end"]
            line_start = src.rfind("\n", 0, start) + 1
            indent = start - line_start
            was_wrapped = "\n" in src[start:end]
            joined = emit_literal(text, indent, site["style"], was_wrapped)
            src = src[:start] + joined + src[end:]
        full.write_text(src, encoding="utf-8")

    return rewrites, refusals


# --------------------------------------------------------------------------------------------
# Self-test
# --------------------------------------------------------------------------------------------


def self_test() -> int:
    """Grades the reader, the writer and the refusals on inputs written to break each one."""
    fails: list[str] = []

    def check(name, got, want):
        if got != want:
            fails.append(f"{name}: got {got!r}, wanted {want!r}")

    # The literal reader, against every form this tree actually contains.
    check("plain", scan_literal('"hello there"', 0), ("hello there", 13))
    check("escaped quote", scan_literal(r'"say \"hi\""', 0)[0], 'say "hi"')
    check("newline escape", scan_literal(r'"a\nb"', 0)[0], "a\nb")
    check("backslash", scan_literal(r'"a\\b"', 0)[0], "a\\b")
    # The continuation eats the newline and the next line's indent, so the space has to be
    # BEFORE it -- get this wrong and every wrapped hint loses a word boundary.
    check("continuation", scan_literal('"one \\\n       two"', 0)[0], "one two")
    check("raw refused", scan_literal('r"a b"', 1), None)
    check("unterminated refused", scan_literal('"a b', 0), None)
    check("unknown escape refused", scan_literal(r'"a\qb"', 0), None)

    # The writer.
    check("encode plain", encode_literal("a b"), '"a b"')
    check("encode quote", encode_literal('a "b"'), '"a \\"b\\""')
    check("encode newline", encode_literal("a\nb"), '"a\\nb"')
    # Round trip is the property that matters, not either half of it.
    for sample in ["plain", 'has "quotes"', "has\nnewline", "back\\slash", "tab\there", "a" * 300]:
        got = scan_literal(encode_literal(sample), 0)
        check(f"round trip {sample[:12]!r}", got and got[0], sample)

    # Wrapping: every piece but the last ends with a space, and concatenation is the identity.
    text = "A share link, the part of one after the hash, or a whole build's text, read the same way."
    pieces = wrap_literals(text, 20)
    rebuilt = "".join(scan_literal(p, 0)[0] for p in pieces)
    check("wrap concatenates back", rebuilt, text)
    if len(pieces) < 2:
        fails.append("wrap: expected the sample to need more than one line")
    if any(len(p) > WRAP - 20 for p in pieces[:-1]):
        fails.append(f"wrap: a piece exceeded the margin: {pieces}")
    # One unbreakable word is emitted over-long rather than broken mid-word.
    check("wrap long word", "".join(scan_literal(p, 0)[0] for p in wrap_literals("x" * 200, 20)), "x" * 200)

    # Placeholders, including the escaped-brace form that must NOT count.
    check("placeholders", placeholders("a {x} b {y}"), ["{x}", "{y}"])
    check("escaped braces", placeholders("a {{literal}} b"), [])
    check("placeholders in order", placeholders("{b} {a}"), ["{a}", "{b}"])

    # The hint finder, on a fixture carrying the three shapes that matter.
    fixture = '''
        p { class: "sb-save__hint",
            "One line only."
        }
        p { class: "powers__empty",
            "A sentence too wide for the margin, so it was "
            "written as two pieces."
        }
        p { class: "feedback__check",
            "Not a hint: the class does not say so."
        }
        p { class: "sb-save__hint",
            if x() { "Ticked." } else { "Unticked." }
        }
    '''
    found = [(k, t, st) for k, t, _, _, st in find_hint_runs(fixture)]
    texts = [t for _, t, _ in found]
    if "One line only." not in texts:
        fails.append(f"hint finder missed the single-line case: {texts}")
    if "A sentence too wide for the margin, so it was written as two pieces." not in texts:
        fails.append(f"hint finder did not fuse the two-piece run: {texts}")
    if "Not a hint: the class does not say so." in texts:
        fails.append("hint finder took a string from a non-hint class")
    # Both arms of the if/else sit on one line, so neither is a bare-literal line: the finder
    # does not reach them. Recorded rather than papered over -- it is the coverage floor the
    # module docstring names, and a wrong fusion here would be worse than a miss.
    if "Ticked.Unticked." in texts:
        fails.append("hint finder fused two branches of an if/else into one string")

    # The continuation wrapper, which is the ONLY legal way to wrap a literal outside rsx!.
    long_text = (
        "What you were doing, what happened, and what you expected instead.\n\n"
        "The build snapshot below carries the build itself, and the part worth typing is the "
        "part it cannot show."
    )
    wrapped = wrap_continuation(long_text, 26)
    check("continuation round trip", scan_literal(wrapped, 0)[0], long_text)
    if "\n" not in wrapped:
        fails.append("continuation: expected the sample to wrap across lines")
    # The property is not "no adjacency lookalike in the text" -- that was the first assertion
    # here and a mutation that re-introduced adjacency walked straight past it. It is that the
    # whole thing is ONE literal: the scanner must consume every character of it.
    consumed = scan_literal(wrapped, 0)
    if not consumed or consumed[1] != len(wrapped):
        fails.append(
            f"continuation did not emit a single literal -- the scanner stopped at "
            f"{consumed and consumed[1]} of {len(wrapped)}. Adjacency only concatenates in rsx!."
        )
    # A break must never land inside an escape sequence.
    if "\\\n" in wrapped.replace("\\\\\n", ""):
        pass
    for piece in wrapped.split("\\\n"):
        if piece.rstrip().endswith("\\"):
            fails.append(f"continuation broke inside an escape: {piece!r}")

    # Where a string sits decides how it may be re-emitted, and getting this wrong is not a
    # cosmetic slip: `"a" "b"` concatenates in rsx! and fails to parse in a match arm. The first
    # cut of this tool used adjacency everywhere and broke `Topic::placeholder()` on first edit.
    fn_fixture = """
        pub fn placeholder(self) -> &'static str {
            match self {
                Topic::Bug => "What you were doing, and what you expected instead of it.",
            }
        }
    """
    fn_found = list(find_hint_fns(fn_fixture))
    if not fn_found:
        fails.append("hint-fn finder missed a prose return from placeholder()")
    elif fn_found[0][4] != "single":
        fails.append(f"a match-arm return was marked {fn_found[0][4]!r}, and adjacency is illegal there")
    if found and any(st != "adjacent" for _, _, st in found):
        fails.append("an rsx text run was not marked adjacent, so it will be re-emitted wrongly")

    # EMIT STYLE, which is the round trip and not a formatting preference. A site marked
    # `adjacent` that outgrows the margin has to come back as a run of bare-literal LINES,
    # because that line shape is the only thing the hint finder recognises. A continuation there
    # parses and renders correctly and then cannot be found again, which is how three hints left
    # the file. The same text at a `single` site has to be ONE literal, where adjacency would
    # not parse at all. Both directions are graded, because each is the other's failure.
    outgrown = "A hint that has outgrown the margin, " * 3
    run = emit_literal(outgrown, 20, "adjacent", was_wrapped=False)
    if "\n" not in run:
        fails.append("emit: the sample was supposed to need wrapping")
    unreadable = [line for line in run.split("\n") if not BARE.match(line)]
    for line in unreadable:
        fails.append(f"emit: an adjacent site produced a line the hint finder cannot read: {line!r}")
    if not unreadable:  # only then is scanning each line piecewise meaningful
        check(
            "emit adjacent round trip",
            "".join(scan_literal(line.strip(), 0)[0] for line in run.split("\n")),
            outgrown,
        )
    one = emit_literal(outgrown, 20, "single", was_wrapped=False)
    consumed_one = scan_literal(one, 0)
    if not consumed_one or consumed_one[1] != len(one):
        fails.append("emit: a single site did not produce one literal, and adjacency is illegal there")
    check("emit single round trip", consumed_one and consumed_one[0], outgrown)
    # Short enough to fit: neither form, just the literal.
    check("emit short stays one line", emit_literal("Short.", 12, "adjacent", False), '"Short."')

    # THE WHOLE WAY ROUND on a fixture: find a hint, reword it past the margin, emit it where it
    # was, and find it again. This is the assertion the continuation slip walked past -- every
    # piece of it was green while the composition was broken.
    round_fixture = '        p { class: "sb-save__hint",\n            "Short."\n        }\n'
    sites = list(find_hint_runs(round_fixture))
    if len(sites) != 1:
        fails.append(f"round trip fixture: expected one hint, found {len(sites)}")
    else:
        _, _, start, end, style = sites[0]
        indent = start - (round_fixture.rfind("\n", 0, start) + 1)
        # Long enough that emitting it MUST wrap at this indent -- a sample that still fits on
        # one line would take the easy branch and never exercise the form that broke.
        reworded = (
            "Short, but not any more: this one is comfortably longer than the margin allows, so "
            "emitting it has to break the literal across source lines, which is the whole point "
            "of the exercise."
        )
        emitted = emit_literal(reworded, indent, style, "\n" in round_fixture[start:end])
        if "\n" not in emitted:
            fails.append("round trip fixture: the rewording was supposed to need wrapping")
        rebuilt = round_fixture[:start] + emitted + round_fixture[end:]
        found_again = [t for _, t, _, _, _ in find_hint_runs(rebuilt)]
        if found_again != [reworded]:
            fails.append(f"a reworded hint could not be found again: {found_again!r}")

    # THE DUMP GUARD. A dump replaces every `text` from the code, so it silently discards pending
    # edits -- and the refusal a person is most likely to hit tells them to re-dump.
    with tempfile.TemporaryDirectory() as tmp:
        probe = pathlib.Path(tmp) / "probe.toml"
        write_file(extract()[:3], probe)
        if pending_edits(probe):
            fails.append("dump guard: a fresh dump was reported as holding pending edits")
        body = probe.read_text()
        first = body.index("text = ")
        probe.write_text(body[:first] + 'text = "reworded"' + body[body.index("\n", first):])
        if not pending_edits(probe):
            fails.append("dump guard: a reworded `text` was not seen as a pending edit")

        # An edit ALREADY written into the code is not pending -- `text != original` is still
        # true there, because --apply moves the code and leaves the anchor behind until the next
        # dump. A guard that fires on this blocks the exact command --apply tells you to run
        # next, and a guard you have to --force past on the happy path is not a guard.
        real = next(e for e in extract() if "\n" not in e["original"])
        landed = pathlib.Path(tmp) / "landed.toml"
        landed.write_text(
            "[probe.landed]\n"
            f'kind = "{real["kind"]}"\n'
            'sites = ["probe.rs:1"]\n'
            'original = "a wording this tree has never held"\n'
            "text = " + toml_string(real["original"]) + "\n"
        )
        if pending_edits(landed):
            fails.append("dump guard: an edit already written into the code was called pending")

    for f in fails:
        print(f"FAIL  {f}")
    print(f"self-test: {len(fails)} failed" if fails else "self-test: all checks passed")
    return 1 if fails else 0


def check_current(path: pathlib.Path) -> int:
    """Is the file still a true picture of the code?

    Compares the SET of entries -- their kind and their `original` -- and deliberately ignores
    every `text`. An edit typed into the file and not yet applied is a pending edit, not drift,
    and reddening on it would mean the guard fires hardest at the one moment the file is being
    used for its purpose. What it does catch is the other direction: a string reworded in the
    .rs without a re-dump, which leaves the file quietly describing an app that no longer says
    that -- and a stale record read as a current one is worse than no record.
    """
    if not path.exists():
        print(f"missing {path.relative_to(ROOT)} -- run: python3 scripts/ui-strings.py --dump", file=sys.stderr)
        return 1
    on_file = {
        (row["kind"], row["original"])
        for row in read_file(path).values()
        if "kind" in row and "original" in row
    }
    in_code = {(e["kind"], e["original"]) for e in extract()}

    missing = sorted(in_code - on_file)
    extra = sorted(on_file - in_code)
    for kind, text in missing[:10]:
        print(f"NOT IN THE FILE  {kind}: {text[:72]!r}", file=sys.stderr)
    for kind, text in extra[:10]:
        print(f"NOT IN THE CODE  {kind}: {text[:72]!r}", file=sys.stderr)
    if missing or extra:
        print(
            f"\n{path.relative_to(ROOT)} is stale: {len(missing)} string(s) in the code it does not "
            f"carry, {len(extra)} it carries that the code no longer has.\n"
            f"Re-dump it: python3 scripts/ui-strings.py --dump",
            file=sys.stderr,
        )
        return 1
    # Every real string in the tree, put through `emit_literal` -- the same function --apply
    # calls -- and read back. Far stronger than the fixtures in --self-test, which are cases
    # somebody thought of: this is every escape, every dash, every placeholder and every
    # hand-wrapped run the app actually contains. Byte identity is NOT the property, since the
    # wrapper may break lines where it likes. Two things are: the text survives, and an
    # `adjacent` site comes back as lines the hint finder can still read. The second was the one
    # missing, and it was missing because this loop modelled the emit rule instead of calling
    # it -- so it graded a round trip that --apply was not performing.
    broken = []
    for e in extract():
        for site in e["sites"]:
            indent, text = 4, e["original"]
            form = emit_literal(text, indent, site["style"], was_wrapped=True)
            if site["style"] == "adjacent":
                pieces = form.split("\n")
                findable = all(BARE.match(piece) for piece in pieces)
                back = "".join(scan_literal(piece.strip(), 0)[0] for piece in pieces)
            else:
                consumed = scan_literal(form, 0)
                findable = bool(consumed) and consumed[1] == len(form)
                back = consumed[0] if consumed else None
            if back != text:
                broken.append((site["file"], site["line"], text[:60], "does not survive a rewrite"))
            elif not findable:
                broken.append((site["file"], site["line"], text[:60], "could not be found again"))
    for f, n, t, why in broken[:10]:
        print(f"ROUND TRIP  {f}:{n}: {t!r} {why}", file=sys.stderr)
    if broken:
        print(f"\n{len(broken)} site(s) would be corrupted by an edit.", file=sys.stderr)
        return 1

    print(
        f"{path.relative_to(ROOT)}: current -- {len(in_code)} entries, each one in the code, "
        f"and every one survives a rewrite"
    )
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--dump", action="store_true", help="write the file from the code")
    ap.add_argument("--apply", action="store_true", help="write the file's edits back into the code")
    ap.add_argument("--self-test", action="store_true", help="grade this script against its own fixtures")
    ap.add_argument("--check", action="store_true", help="is the file still a true picture of the code?")
    ap.add_argument("--file", type=pathlib.Path, default=DEFAULT_FILE)
    ap.add_argument(
        "--force", action="store_true", help="let --dump discard unapplied edits in the file"
    )
    args = ap.parse_args()

    if args.self_test:
        return self_test()

    if args.check:
        return check_current(args.file)

    if args.dump:
        # A dump writes `text = original` for every entry, so it does not merge with the file on
        # disk -- it replaces it, and any rewording typed in and not yet applied goes with it.
        # That is silent, total, and easiest to trigger at exactly the wrong moment, because the
        # natural response to a refused entry is to re-dump and start over. The guard costs one
        # command (--apply first) and the door out is explicit.
        if args.file.exists() and not args.force:
            waiting = pending_edits(args.file)
            if waiting:
                shown = "\n".join(f"  {k}" for k in waiting[:10])
                more = f"\n  ... and {len(waiting) - 10} more" if len(waiting) > 10 else ""
                print(
                    f"{args.file.relative_to(ROOT)} holds {len(waiting)} unapplied edit(s):\n"
                    f"{shown}{more}\n\n"
                    f"A dump rewrites every `text` from the code, so it would discard all of them.\n"
                    f"  write them into the code first:  python3 scripts/ui-strings.py --apply\n"
                    f"  or throw them away on purpose:   python3 scripts/ui-strings.py --dump --force",
                    file=sys.stderr,
                )
                return 1
        entries = extract()
        write_file(entries, args.file)
        sites = sum(len(e["sites"]) for e in entries)
        kinds: dict[str, int] = {}
        for e in entries:
            kinds[e["kind"]] = kinds.get(e["kind"], 0) + 1
        breakdown = ", ".join(f"{v} {k}" for k, v in sorted(kinds.items()))
        print(f"wrote {args.file.relative_to(ROOT)}: {len(entries)} entries ({breakdown}) over {sites} sites")
        return 0

    if args.apply:
        if not args.file.exists():
            print(f"no {args.file.relative_to(ROOT)} -- run --dump first", file=sys.stderr)
            return 2
        rewrites, refusals = apply_edits(args.file)
        for r in rewrites:
            where = ", ".join(f'{s["file"]}:{s["line"]}' for s in r["entry"]["sites"])
            print(f"rewrote {r['key']}  ({where})")
        for r in refusals:
            print(f"REFUSED  {r}", file=sys.stderr)
        if not rewrites and not refusals:
            print("no edits to apply -- every `text` still matches its `original`")
            return 0
        if rewrites:
            print(f"\n{len(rewrites)} rewritten. Re-run --dump to refresh the file, and read `git diff`.")
        if refusals:
            print(
                f"{len(refusals)} refused and left alone. Fix those entries and run --apply again.",
                file=sys.stderr,
            )
            return 1
        return 0

    ap.print_help()
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
