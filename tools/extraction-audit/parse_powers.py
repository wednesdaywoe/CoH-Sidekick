"""
Parser for the City of Heroes `.powers` source-definition format (the HC dev raw
defs under `raw defs/`). These are the authoritative, complete definitions — the
oracle for auditing what our bin parser -> exported_powers extraction drops.

Format (brace-nested, repeatable blocks):

    Power Category.Powerset.PowerName
    {
        Name        "Energy_Torrent"
        Type        kClick
        AttackTypes kAOE_Attack, kEnergy_Attack
        Effect
        {
            Requires enttype target> critter eq
            AttribMod
            {
                Attrib   kSmashing
                Aspect   kAbs
                Table    "Ranged_Damage"
                Scale    0.3000
                Messages { DisplayAttackerHit "P119576033" }
            }
            AttribMod { ... }
        }
        Effect { ... }
    }

parse_powers_file(path) -> dict with:
    - 'fullname': str (from the `Power <fullname>` header)
    - scalar fields: { key: value-or-[values] }   (repeated keys -> list)
    - 'Effect': [ effect-dict, ... ]   where each effect-dict has its own scalars
      plus 'AttribMod': [ attribmod-dict, ... ]

Block names (Effect / AttribMod / Messages / etc.) become keys whose value is a
list of child dicts (always a list, even for one, so callers don't special-case).
Scalars: key = first token, value = rest of the line (quotes stripped; comma
lists split into a list).
"""

from __future__ import annotations
import re
from pathlib import Path


def _coerce(value: str):
    """Strip quotes; split comma-lists; leave everything else as a string."""
    v = value.strip()
    if not v:
        return ""
    if "," in v:
        return [_coerce(part) for part in v.split(",")]
    if len(v) >= 2 and v[0] == '"' and v[-1] == '"':
        return v[1:-1]
    return v


def parse_powers_text(text: str) -> dict:
    lines = text.splitlines()
    # Strip // and ; line comments, blank lines.
    cleaned = []
    for raw in lines:
        s = raw.strip()
        if not s:
            continue
        if s.startswith("//") or s.startswith("#"):
            continue
        cleaned.append(s)

    root: dict = {}
    fullname = None
    # stack of (container_dict, pending_block_name)
    stack: list[dict] = []
    pending: list[str | None] = []
    cur = root
    pend = None

    i = 0
    n = len(cleaned)
    while i < n:
        line = cleaned[i]

        if line == "{":
            # Open a block under `pend` in `cur`.
            child: dict = {}
            name = pend or "_anon"
            cur.setdefault(name, [])
            if not isinstance(cur[name], list):
                cur[name] = [cur[name]]
            cur[name].append(child)
            stack.append(cur)
            pending.append(pend)
            cur = child
            pend = None
            i += 1
            continue

        if line == "}":
            if stack:
                cur = stack.pop()
                pend = pending.pop()
            i += 1
            continue

        # Header line: `Power <fullname>` then `{`
        m = re.match(r"^Power\s+(\S+)", line)
        if m and cur is root and fullname is None:
            fullname = m.group(1)
            pend = "Power"  # the next `{` opens the power body into root['Power']
            i += 1
            continue

        # Is this a block header (single token, next line is `{`)?
        nxt = cleaned[i + 1] if i + 1 < n else None
        tokens = line.split(None, 1)
        key = tokens[0]
        rest = tokens[1] if len(tokens) > 1 else ""

        if nxt == "{" and rest == "":
            # bare keyword block header
            pend = key
            i += 1
            continue
        if nxt == "{" and rest != "":
            # e.g. some headers carry an inline arg; treat key as block name,
            # stash the arg under '_arg'.
            pend = key
            # consume the '{' next iteration; remember the arg via a temp
            cleaned[i] = key  # collapse to bare header
            # attach arg after block opens — simplest: skip, rarely needed
            i += 1
            continue

        # Scalar `key value...`
        val = _coerce(rest)
        if key in cur:
            if not isinstance(cur[key], list):
                cur[key] = [cur[key]]
            cur[key].append(val)
        else:
            cur[key] = val
        i += 1

    body = {}
    if isinstance(root.get("Power"), list) and root["Power"]:
        body = root["Power"][0]
    body["fullname"] = fullname
    return body


def parse_powers_file(path: str | Path) -> dict:
    return parse_powers_text(Path(path).read_text(encoding="utf-8", errors="replace"))


def iter_attribmods(power: dict):
    """Yield (effect_index, attribmod_dict) for every AttribMod in every Effect."""
    for ei, eff in enumerate(power.get("Effect", []) or []):
        if not isinstance(eff, dict):
            continue
        for am in eff.get("AttribMod", []) or []:
            if isinstance(am, dict):
                yield ei, am


if __name__ == "__main__":
    import sys
    p = parse_powers_file(sys.argv[1])
    effects = p.get("Effect", []) or []
    print(f"fullname: {p.get('fullname')}")
    print(f"scalar fields: {sorted(k for k, v in p.items() if not isinstance(v, list) or (v and not isinstance(v[0], dict)))}")
    print(f"Effect blocks: {len(effects)}")
    total_am = sum(1 for _ in iter_attribmods(p))
    print(f"AttribMods total: {total_am}")
    for ei, am in iter_attribmods(p):
        print(f"  E{ei}: Attrib={am.get('Attrib')} Aspect={am.get('Aspect')} Table={am.get('Table')} Scale={am.get('Scale')} Target={am.get('Target')}")
