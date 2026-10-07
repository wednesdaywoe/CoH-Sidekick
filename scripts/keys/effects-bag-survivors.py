#!/usr/bin/env python3
"""The `effects` bag removal, writer side — a standing check with no row above it.

Run from the repo root:  python3 scripts/keys/effects-bag-survivors.py

NO TRACKER, AND THAT IS DELIBERATE. This was written as the key to a `REBUILD-PROGRESS` row.
That progress document and the whole issue-register system were removed ON PURPOSE when this
repository was set up as an experiment; they were never in its git history, so there is nothing
to recover. Every `XXXX-nn` id in this file — and the ~1,450 others across 246 files — is prose
context, not a pointer. Do not go looking for the row. The check is real regardless.

The claim it grades: "`effects` is gone from the contract schema and from every generator that
writes it, and nothing reads a slot that no longer exists." Every part is checked here against
the artifact it is about rather than re-derived from prose, because each one closes a door and
three have already been wrong once.

Four legs, and each is a different kind of claim:

  1. **The emitters.** "`convert-powerset.cjs` and the five sibling converters stop emitting
     `effects`" read as six live emitters, then one, then three, then two. It is now ZERO: the
     last two stopped on 2026-09-03. Five siblings compute `extractEffects` into a local and pass
     it to the Thunderspy guards without emitting it, and `convert-powerset.cjs`'s `entry.effects`
     is a conditionalEffects entry, not the power-level bag.

     The leg has two halves and it needed both. Roles are adjudicated once by reading each file,
     and that half breaks when the evidence line moves — but it CANNOT see a role that changes
     while its evidence sits still, which is exactly what happened here: both remaining emitters
     were pinned on their `extractEffects` merge line, the local build, which the retirement does
     not touch. They reported `emitter` after they had stopped emitting. So the deciding half is
     now mechanical and consults no table: a scan for a `power.effects =` assignment over every
     converter, which is the thing the row's Goal is actually about and which catches a NEW
     emitter by structure rather than by someone remembering to add a row.

  2. **The atom-less bags.** A bag on a power that carries atoms has somewhere to migrate to; a
     bag on an ATOM-LESS power does not, and deleting it deletes the only copy. Four survive, all
     four are `Domination`, and all four carry no `stats` object at all.

  3. **The Rust sites.** "The reader side is done, so the strip is safe" is false: retiring the
     typed `Bag` left the raw `extra["effects"]` object with production readers. But a census
     keyed on the NAME over-counts as badly as one keyed on it under-counts — three of the
     sixteen sites read a pet ability's `effects` ARRAY or a conditionalEffects entry's own bag,
     neither of which this retires. Six more came out on 2026-09-26, leaving
     ten: one `live` (the writer), four `inert`, two `guard`, three `not-the-bag`.

     Roles are adjudicated per site, and the roles that are claims about the DATA (`covered`,
     `inert`, `live`) are re-measured against the contract on every run, so a fallback nobody
     reads today fails the gate the moment it becomes the only source of a number.

     WHICH POWER A SITE RECEIVES DECIDES WHETHER IT CAN REACH A BAG AT ALL — `gather::
     resolve_power` output is the contract power and never carries one, the effective power may
     — BUT THAT IS ONLY HALF THE QUESTION, and item 29 stopped there. The other half is whether
     the KEY the site reads is one the runtime bag can hold, and for four of those sites it is
     not. Both halves are answered in the runtime-bag note below.

  5. **The pets.** `summon` was the fourth `strip-blocks` reader and the one that got out: the
     strip removed `effects.summon` while four sites still read it, and 400+ summoners per fork
     lost their pets outright — no rows on the display, and the buff-pet aura fold at zero on all
     four forks (ENT-22, closed 2026-08-29 by giving the pet parameters a top-level home). This
     leg counts what that closure owes the next reader: CASUALTIES, powers carrying an `EntCreate`
     atom and resolving no summon, never survivors. The survivor count is what hid the gap — it
     read 17 and was taken for the migration's size when it was the count of powers the census
     could still SEE. A survivor count cannot go down when a power vanishes; it just stops
     counting it.

  4. **The graders.** This leg demanded an ORACLE — an independent statement of the right
     answer — because when it was written the strip was a MIGRATION, and migrations move
     numbers. That stopped being the job on 2026-09-03, when the data went bag-free. What is
     left is removing readers that cannot fire, which must move NOTHING; and proving "nothing
     moved" is a change detector's work, not an oracle's. `crates/coh_math/tests/
     totals_baseline.rs` (2026-09-26) records 141 numbers per build, 20,163 over the corpus,
     per fork, and reds on any movement, with `totals_replay.rs`, `oracle.rs`, `proc_replay.rs`,
     `set_bonus_replay.rs` and `movement_replay.rs` beside it. The frozen corpus is still gone
     and still cannot be re-minted; this leg reports that and NO LONGER BLOCKS ON IT.

**THE BAG IS NOT ONLY A CONTRACT ARTIFACT, AND THAT STILL DOES NOT MAKE ITS READERS LIVE.**
`crates/coh_math/src/effective.rs`'s `with_active_conditionals` BUILDS a bag at runtime out of
the active conditionals' delta and stamps it on the working copy of the power every display
surface reads. Item 29 (2026-09-26) took that as far as calling five sites `live` and stating
that `granted.rs`'s arc fallback catches a conditional-supplied cone width in radians. THAT
CLAIM WAS WRONG, and it was wrong in the direction this file exists to catch: an argument about
the code, never measured against it.

MEASURED 2026-09-26, by running it rather than reading it. Every power in
every fork — 14,449 of them, each asked under three caster classes — with every conditional it
authors forced on. 1,962 effective powers came back carrying a bag, and the union of their keys
is 44 names, all of them EFFECT KINDS: `absorb`, `damageBuff`, `defenseDebuff`, `hold`,
`knockback`, `movement`, `recoveryBuff`, `slow`, `stun`, `summon`, `durations`, `buffDuration`,
`effectDuration` and the like. `arc` NEVER APPEARS. Neither does `damage`, `enduranceCost`,
`recharge`, `accuracy`, `range`, `castTime`, `radius` or `maxTargets`.

That is structural, not incidental. The delta's keys come from `conditional_effects`, which
projects the conditional's ATOMS through the bag mirror (`window_slots.rs`), and that mirror's
keys are `&'static str` literals naming effect kinds. A power's geometry and execution stats are
not in its vocabulary and cannot be routed into it. So the four reader rows below read a key
their source cannot contain, from either side: the contract carries no bag, and the runtime bag
cannot carry these names. They are `inert`, they are cheap, and they still STAY — an unreachable
read costs nothing and the enforcement below turns it red the moment a CONTRACT bag supplies
one. What does not survive is the claim that deleting one ships a wrong number today.

ONE row is `live` and it is the WRITER: the line in `effective.rs` that stamps the delta as the
effective power's `effects`. Everything else reads what it wrote, in the 44-name vocabulary
above.

Exit 0 = no bag survives in the contract, no converter writes one, no atom-less bag holds the
         only copy, every coverage claim still holds, and every remaining Rust site is the
         `live` writer, a deliberate `guard`, an `inert` read, or not the bag at all.
Exit 1 = at least one leg still stands, and the message names which. An exit 1 now means a
         REGRESSION rather than remaining work — most likely a converter that has started
         writing the bag again.

Mutation-tested in both directions: one added bag key on one pool power reds the coverage leg,
and reflowing an adjudicated site's source line reds it as STALE rather than silently
reclassifying the site.

Canonical-only: it reads `contract/` and `crates/`, which the beta repo does not carry.
"""
import gzip
import json
import os
import re
import sys

CONTRACT = 'contract'
FORKS = ('homecoming', 'rebirth', 'thunderspy', 'brainstorm')

# Slots with a second address on the power (`stats`, or top level). ENDSTAT-1 moved these;
# they are not at risk from the strip, so they do not make a bag load-bearing.
EXEC_SLOTS = {
    'accuracy', 'effectArea', 'endurance', 'enduranceCost', 'activationTime', 'castTime',
    'activatePeriod', 'range', 'damage', 'radius', 'maxTargets', 'arc', 'interruptTime',
    'recharge',
}

# The wire spells some slots differently from `stats`; a slot is not orphaned when its twin
# is present under the other name.
ALIAS_TO_STATS = {'endurance': 'enduranceCost', 'activationTime': 'castTime'}

# Atom tuple positions (contract/schema-version.json `atomTupleFields`).
ATOM_EFFECT_TYPE, ATOM_SUB_TYPE = 0, 1


def json_files(root):
    for base, _, names in os.walk(root):
        for n in sorted(names):
            if n.endswith('.json'):
                yield os.path.join(base, n)


def walk_bags(node, partition, out):
    """Yield every power-level `effects` bag: a named node with a non-array `effects` object."""
    if isinstance(node, list):
        for v in node:
            walk_bags(v, partition, out)
        return
    if not isinstance(node, dict):
        return
    bag = node.get('effects')
    if (isinstance(bag, dict) and 'name' in node
            and ('atoms' in node or 'powerType' in node)):
        atoms = node.get('atoms') if isinstance(node.get('atoms'), list) else []
        stats = node.get('stats') if isinstance(node.get('stats'), dict) else {}
        # A slot is ORPHANED when nothing else on the power carries it. Do NOT pre-filter
        # execution slots here: EXEC_SLOTS encodes "a second address exists", which is exactly
        # the premise that fails on a power with no `stats` object at all. Filtering first is
        # what hid `recharge: 200` on the Dominator `Domination` inherent, whose bag is the
        # only copy of all three of its keys.
        orphan = [k for k in bag
                  if k not in stats and ALIAS_TO_STATS.get(k, k) not in stats and k not in node]
        out.append({
            'partition': partition,
            'who': node.get('internalName') or node.get('name'),
            'atoms': atoms,
            'has_stats': bool(stats),
            'slots': list(bag.keys()),
            'orphan_slots': orphan,
            'value_slots': [k for k in bag if k not in EXEC_SLOTS],
            # Carried whole so the coverage census can re-measure each `covered` role against
            # the other source rather than against a summary of it.
            'bag': bag,
            'stats': stats,
            'node': node,
        })
    for v in node.values():
        walk_bags(v, partition, out)


def census_contract():
    bags = []
    for fork in FORKS:
        root = os.path.join(CONTRACT, fork)
        if not os.path.isdir(root):
            continue
        for path in json_files(root):
            rel = os.path.relpath(path, root)
            partition = f'{fork}/powersets' if rel.startswith('powersets' + os.sep) else f'{fork}/{rel}'
            try:
                doc = json.load(open(path, encoding='utf-8'))
            except Exception:
                continue
            walk_bags(doc, partition, bags)
    return bags


# `convert-pet-entities.cjs` defines its OWN one-arg `extractEffects(powerData)` (line 1089)
# that builds a pet ability's `effects` ARRAY of `{type, scale, table}` records. That is a
# different artifact from the named-slot bag — it is not a projection of atoms into slots, and
# it is not what this row retires. Excluded by name so the leg does not report it either way.
NOT_THE_BAG = {'convert-pet-entities.cjs'}


# The adjudicated role of every `extractEffects` caller, with the evidence that decides it.
# This is a table rather than a regex classification on purpose: the binding name in
# `convert-powerset.cjs` is the generic `effects`, so any pattern loose enough to catch a merge
# also catches unrelated spreads and local builder writes. The first cut of this key tried that
# and reported `convert-powerset.cjs` as an emitter twice over. A key turns a claim; it does not
# re-derive a classification. So each role is adjudicated once, by reading the file, and the key
# fails when the evidence line moves.
#
#   emitter    — the projection is merged into the object that reaches the wire
#   guard      — bound to a local and passed to the Thunderspy trap guards, never emitted
#   internal   — consumed inside the converter (a conditionalEffects entry, or a local ref);
#                the power-level bag is NOT written from it
EMITTER_ADJUDICATION = {
    'convert-inherents.cjs': ('guard', 'guardThunderspyOnesBuffs(power, rawJson.targets_affected, effects);'),
    'convert-pool-powers.cjs': ('guard', 'guardThunderspyOnesBuffs(power, rawJson.targets_affected, effects);'),
    'convert-accolades.cjs': ('guard', 'const guardBag = extractEffects('),
    'convert-basic-inherents.cjs': ('guard', 'const guardBag = extractEffects('),
    'convert-epic-pools.cjs': ('guard', 'const guardBag = extractEffects('),
    'convert-powerset.cjs': ('internal', 'if (hasEffects) entry.effects = effects;'),
}

# The assignment that MAKES a converter an emitter. The role table above is a hand adjudication
# and was blind in one direction until 2026-09-03: both of the last two emitters were pinned on
# their `extractEffects` merge line, which is the local build and survives the retirement, so
# when they stopped writing `power.effects` the table went on reporting `emitter` and the leg
# stayed green on a false classification. A role can change while its evidence sits still.
#
# This is the mechanical half, and it is the one that decides the row's Goal ("`effects` is gone
# ... from every generator that writes it"): no converter may assign the power-level bag. It is
# checked over every converter, adjudicated or not, so a NEW emitter is caught by structure
# rather than by remembering to add a row.
BAG_ASSIGNMENT = re.compile(r'^\s*power\.effects\s*=', re.M)


def census_emitters():
    """Report each converter's adjudicated role, and break when the evidence no longer holds.

    Returns (roles, stale, emitters). `stale` names any file whose adjudicated evidence line is
    gone, or any `extractEffects` caller with no adjudication at all — either means the table
    describes a file that has since changed, and the classification must be re-read rather than
    trusted. `emitters` is the structural answer: every converter that assigns `power.effects`,
    found without consulting the table, so the row's Goal is graded by the source and not by the
    adjudication's memory of it.
    """
    roles, stale, emitters = {}, [], []
    seen = set()
    for name in sorted(os.listdir('scripts')):
        if not name.startswith('convert-') or not name.endswith('.cjs'):
            continue
        if name in NOT_THE_BAG:
            continue
        src = open(os.path.join('scripts', name), encoding='utf-8').read()
        if BAG_ASSIGNMENT.search(src):
            emitters.append(name)
        if 'extractEffects(' not in src:
            continue
        seen.add(name)
        if name not in EMITTER_ADJUDICATION:
            stale.append(f'{name}: calls extractEffects but has no adjudicated role')
            continue
        role, evidence = EMITTER_ADJUDICATION[name]
        roles.setdefault(role, []).append(name)
        if evidence not in src:
            stale.append(f'{name}: adjudicated {role!r} on evidence that is no longer present '
                         f'-- {evidence[:60]!r}')
    for name in EMITTER_ADJUDICATION:
        if name not in seen:
            stale.append(f'{name}: adjudicated here but no longer calls extractEffects')
    for name in sorted(set(emitters)):
        role = EMITTER_ADJUDICATION.get(name, (None,))[0]
        if role != 'emitter':
            stale.append(f'{name}: assigns `power.effects` but is adjudicated {role!r} — '
                         f'the bag is being written again')
    return roles, stale, emitters


# The adjudicated role of every Rust site that names `effects`, with the evidence that decides
# it. A table rather than a name match, for the same reason the emitter table is one: three of
# these sites read an `effects` that is NOT the power-level bag, and a census keyed on the name
# reports them as migration work that does not exist. The evidence is the site line plus the
# preceding non-blank line, because the three pet/conditional sites are all bare `.get("effects")`
# and only their receiver tells them apart.
#
#   strip-blocks — the strip changes this site's answer; it must migrate first
#   live         — WRITES the bag `with_active_conditionals` synthesises at runtime. One site,
#                  and the only one that is live in any sense. See the module docstring.
#   guard        — inert on purpose and must stay: deleting it converts a loud failure into a
#                  silent one. Both live in `coh_data::database`.
#   covered      — a fallback whose other source answers for EVERY power in the corpus
#   inert        — nothing reaches this read. Either no power in the corpus does, or — the
#                  four arc/damage/truthy_stat/enduranceCost rows, measured 2026-09-26 — the
#                  key is not one the runtime bag can hold, so neither source can supply it.
#
# The third slot names the population `census_source_coverage` measures for that site. On a
# `covered` or `inert` row it is a CLAIM and the gate fails when it stops holding: those two
# roles are statements about the DATA, and one power gaining a bag key its `stats` twin lacks
# turns a fallback nobody reads into the only source of a number. On a `strip-blocks` row it is
# just the size of the migration, printed and not enforced — that site already fails the gate.
#   no-op        — a write, removal or refusal whose input disappears with the bag, changing
#                  nothing
#   not-the-bag  — a different `effects` artifact: a pet ability's effects ARRAY, or a
#                  conditionalEffects entry's own bag. Neither is what this row retires.
RUST_SITE_ADJUDICATION = {
    ('// this errors on the empty set today and exists to stay that way.',
     'if named && !is_power && map.get("effects").is_some_and(Value::is_object) {'):
        ('guard', 'is_power stopped counting the bag 2026-09-03; what reads it now is the Rule 1 '
                  'refusal that replaced the arm, so a bag-only power reds the load instead of '
                  'silently not existing. With no bags the condition is never true and the '
                  'loader behaves identically. Guarded by power_shape_requires_atoms', None),
    ('.is_some_and(|kind| kind.eq_ignore_ascii_case("toggle"));',
     'let Some(Value::Object(effects)) = power.get_mut("effects") else {'):
        ('guard', 'normalize_legacy_power adapts a legacy pool/epic shape and CANNOT create a '
                  'bag — it returns early unless one is already there. Kept for an older '
                  'contract; against a current one the second half never runs', None),
    ('for effect in ability', '.get("effects")'):
        ('not-the-bag', "a pet ability's effects ARRAY", None),
    ('if let Some(effects) = merged_effects(&delta) {',
     'set(&mut merged, "effects", Some(Value::Object(effects)));'):
        ('live', 'THE WRITER, and the only live site. This is what puts a bag on a power now — '
                 'the conditional delta, stamped on the effective power. Its base half came off '
                 '2026-09-26; the delta half is the mechanism. What it writes is 44 effect-kind '
                 'keys measured over all four forks, which is why every reader below is inert',
         None),
    (') -> Option<Vec<(String, Value)>> {',
     'let authored = conditional.get("effects").and_then(Value::as_object);'):
        ('not-the-bag', "a conditionalEffects entry's own bag, which the strip does not touch", None),
    ('let raw_arc = object_number(extra_object(power, "stats"), "arc")',
     '.or_else(|| object_number(extra_object(power, "effects"), "arc"));'):
        ('inert', 'granted.rs display arc. Reads the EFFECTIVE power, which item 29 took as '
                  'proof a conditional-supplied arc arrives here. MEASURED: it cannot. `arc` is '
                  'not one of the 44 keys the runtime bag can hold, because the bag mirror '
                  'routes effect kinds and not geometry. Unreachable from both sides', 'arc'),
    ('.get("damage")',
     '.or_else(|| extra_object(power, "effects").and_then(|e| e.get("damage")));'):
        ('inert', 'granted.rs display damage. MEASURED: `damage` is not in the runtime bag\'s '
                  '44-key vocabulary — a conditional\'s damage reaches the effective power '
                  'through `merged_damage` as the TOP-LEVEL array, never through the bag',
         'damage'),
    ('ability', '.get("effects")'):
        ('not-the-bag', "a pet ability's effects ARRAY", None),
    ('let stats = extra_object(power, "stats");', 'let effects = extra_object(power, "effects");'):
        ('inert', 'truthy_stat over recharge/accuracy/range/castTime/radius/maxTargets, on the '
                  'EFFECTIVE power (`shown` in projection.rs). MEASURED: not one of those six '
                  'names is in the runtime bag\'s 44-key vocabulary. `recharge`, `accuracy` and '
                  '`range` exist in the mirror only as SUB-keys of `debuffResistance`',
         'truthy_stat'),
    ('}', 'object_number(extra_object(power, "effects"), "enduranceCost").filter(|value| *value != 0.0)'):
        ('inert', 'base_endurance_cost on the EFFECTIVE power. MEASURED: `enduranceCost` is not '
                  'in the runtime bag\'s 44-key vocabulary. Its only writer is the legacy '
                  'adapter above, which cannot run without a contract bag to adapt',
         'base_endurance_cost'),
}

BLOCKING_ROLES = ('strip-blocks',)


def _prev_line(lines, ln):
    for line in reversed(lines[:ln - 1]):
        if line.strip():
            return line.strip()
    return ''


def census_rust_readers():
    """Every production (pre-`#[cfg(test)]`) site naming `effects`, with its adjudicated role.

    Returns (sites, stale). A site with no adjudication is stale, and so is an adjudication
    whose site is gone — either means the table describes code that has since moved, and the
    classification must be re-read rather than trusted.
    """
    sites, seen = [], set()
    for crate in ('coh_data', 'coh_math', 'app'):
        root = os.path.join('crates', crate, 'src')
        if not os.path.isdir(root):
            continue
        for base, _, names in os.walk(root):
            for n in sorted(names):
                if not n.endswith('.rs'):
                    continue
                path = os.path.join(base, n)
                lines = open(path, encoding='utf-8').read().splitlines()
                cutoff = len(lines) + 1
                for i, line in enumerate(lines, 1):
                    if re.match(r'\s*#\[cfg\(test\)\]', line):
                        cutoff = i
                        break
                for i, line in enumerate(lines[:cutoff - 1], 1):
                    if '"effects"' not in line:
                        continue
                    if 'incarnate_effects' in line or 'conditionalEffects' in line:
                        continue
                    key = (_prev_line(lines, i), line.strip())
                    seen.add(key)
                    sites.append((path, i, line.strip(), key))
    stale = [f'{p}:{i}: {t[:70]!r} has no adjudicated role'
             for p, i, t, k in sites if k not in RUST_SITE_ADJUDICATION]
    for key in RUST_SITE_ADJUDICATION:
        if key not in seen:
            stale.append(f'adjudicated site is gone -- {key[1][:70]!r}')
    return sites, stale


# Time Bomb carries six ungated `EntCreate` atoms and resolves no summon, on Homecoming and
# Brainstorm, four archetype records each. Measured against the frozen corpus rather than assumed:
# it resolved none there either, so it predates the strip and is NOT an ENT-22 casualty. Named
# here so the residue below is a known population instead of a number that drifts.
SUMMONLESS_ADJUDICATED = {'Time_Bomb'}


def census_summon_casualties():
    """Powers with an `EntCreate` atom and no summon — the CASUALTY count, per fork.

    The question this leg exists to ask is the one the survivor count cannot: which powers create
    an entity and state nothing about it? A power whose summon went missing keeps its atoms, so it
    stays in the numerator here and drops out of any count of powers that HAVE a summon. That
    asymmetry is the whole point (ENT-22).

    A gated `EntCreate` is not a casualty. ENT-15 added `activation_effects` as a third atom
    source precisely so those rows ride the wire, and they land `gated: true` because the bag's
    collector did not take them — so `extractSummon` never saw them and correctly resolves
    nothing. Gang War is the case that settled it. Only an UNGATED create-entity atom with no
    summon is a power that says it makes something and will not say what.

    Returns {fork: (casualties, gated_only)}.
    """
    schema = json.load(open(os.path.join(CONTRACT, 'schema-version.json'), encoding='utf-8'))
    gated_at = schema['atomTupleFields'].index('gated')
    out = {}
    for fork in FORKS:
        path = os.path.join(CONTRACT, fork, 'bundle.json.gz')
        if not os.path.isfile(path):
            continue
        with gzip.open(path) as fh:
            bundle = json.load(fh)
        casualties, gated_only = [], []
        stack = [bundle.get(section) for section in
                 ('powersets', 'power-pools', 'epic-pools', 'levels', 'incarnate')]
        while stack:
            node = stack.pop()
            if isinstance(node, dict):
                atoms = node.get('atoms')
                if isinstance(node.get('name'), str) and isinstance(atoms, list):
                    created = [a for a in atoms
                               if isinstance(a, list) and a and a[0] == 'EntCreate']
                    if created and 'summon' not in node:
                        who = node.get('fullName') or node.get('internalName') or node['name']
                        ungated = [a for a in created
                                   if len(a) <= gated_at or a[gated_at] is not True]
                        if not ungated:
                            gated_only.append(who)
                        elif who.split('.')[-1] not in SUMMONLESS_ADJUDICATED:
                            casualties.append(who)
                stack.extend(v for v in node.values() if isinstance(v, (dict, list)))
            elif isinstance(node, list):
                stack.extend(v for v in node if isinstance(v, (dict, list)))
        out[fork] = (casualties, gated_only)
    return out


def census_source_coverage(bags, atomless_ids):
    """Re-measure every `covered`, `inert` and `live` role against the contract.

    A role is a claim about the DATA, not about the code, so it cannot be trusted from a table:
    one power gaining a bag key its `stats` twin lacks turns a covered fallback into a live
    dependency.

    Returns {claim: (new, also_atomless)}. The split is reported and never filtered: the four
    atom-less Domination records break `truthy_stat` for the same reason
    the leg above already fails on them — no `stats` object at all — and counting them twice
    would read as two findings where there is one. They are still listed, because a filter whose
    premise fails on the very rows it excludes is how `recharge: 200` stayed hidden on the one
    power with nowhere else to put it (scripts/keys/README.md, trap 4).
    """
    def truthy(v):
        try:
            return v is not None and float(v) != 0.0
        except (TypeError, ValueError):
            return False

    broken = {}
    for b in bags:
        fx, st, node = b['bag'], b['stats'], b['node']
        who = f"{b['partition']}:{b['who']}"

        bucket = 1 if who in atomless_ids else 0

        # noqa B023: `who` is read at call time, and every call is inside this same
        # iteration — `note` is never stored or deferred, so there is no late binding to
        # get wrong. (`bucket` is bound as a default for the same reason, defensively.)
        def note(claim, bucket=bucket):
            broken.setdefault(claim, ([], []))[bucket].append(who)  # noqa: B023

        # projection.rs `truthy_stat`: stats[k] truthy, else effects[k] truthy.
        for k in ('recharge', 'accuracy', 'range', 'castTime', 'radius', 'maxTargets'):
            if truthy(fx.get(k)) and not truthy(st.get(k)):
                note('truthy_stat')
        # projection.rs `base_endurance_cost`: stats.endurance truthy, else effects.enduranceCost.
        if truthy(fx.get('enduranceCost')) and not truthy(st.get('endurance')):
            note('base_endurance_cost')
        # The `stat_or_effect` claim stood here and went with its site on 2026-09-26:
        # `procs.rs` reads only `stats` now, over a power that comes from
        # `gather::resolve_power` and never carries a bag. A claim measured against no site is
        # the shape this file exists to catch, so it is not kept warm for a reader that is gone.
        if fx.get('arc') is not None and st.get('arc') is None:
            note('arc')
        if fx.get('damage') is not None and node.get('damage') is None:
            note('damage')
        # No `summon` claim here any more, and its absence is the finding this key got wrong
        # once. Measuring `effects.summon` against the contract counted the bags the strip had
        # LEFT — 17 of them — and that number was read as the size of the migration when it was
        # the size of what remained visible. The pets moved to `Power::summon` (ENT-22); the
        # population that matters is casualties, and `census_summon_casualties` counts those.
        # The `per_target` claim went the same way, and its site had already gone STALE before
        # that: `stacking.rs` no longer reads the bag for `perTarget` at all.
    return broken


def census_taunt(bags):
    """The stream's claim that HC taunt rows have no atom to migrate onto."""
    backed, orphan = [], []
    for b in bags:
        if 'taunt' not in b['slots']:
            continue
        has = any(
            isinstance(a, list) and len(a) > ATOM_SUB_TYPE and a[ATOM_SUB_TYPE] == 'Taunt'
            for a in b['atoms']
        )
        (backed if has else orphan).append(f"{b['partition']}:{b['who']}")
    return backed, orphan


def main():
    if not os.path.isdir(CONTRACT):
        print('contract/ not found — run from the repo root (canonical only).')
        return 2

    bags = census_contract()
    atomless = [b for b in bags if not b['atoms'] and b['orphan_slots']]
    roles, stale_roles, bag_emitters = census_emitters()
    rust, stale_sites = census_rust_readers()
    atomless_ids = {f"{b['partition']}:{b['who']}" for b in atomless}
    coverage_broken = census_source_coverage(bags, atomless_ids)
    taunt_backed, taunt_orphan = census_taunt(bags)
    summon_casualties = census_summon_casualties()
    blocking = [r for r in rust if RUST_SITE_ADJUDICATION.get(r[3], ('', '', None))[0]
                in BLOCKING_ROLES]
    # `covered`/`inert`/`live` rows make a claim the gate enforces; see RUST_SITE_ADJUDICATION.
    # `live` is in the set because those sites read the RUNTIME bag legitimately — a CONTRACT bag
    # appearing under one of them is still a new dependency and still a regression.
    enforced = {claim for _, _, _, key in rust
                for role, _, claim in [RUST_SITE_ADJUDICATION.get(key, ('', '', None))]
                if role in ('covered', 'inert', 'live') and claim}
    claims_broken = {k: v for k, v in coverage_broken.items() if k in enforced and v[0]}

    by_partition = {}
    for b in bags:
        by_partition[b['partition']] = by_partition.get(b['partition'], 0) + 1

    print(f'power-level `effects` bags surviving in contract/: {len(bags)}')
    for part, n in sorted(by_partition.items(), key=lambda kv: -kv[1]):
        print(f'  {n:>5}  {part}')

    print('\nconverters assigning `power.effects` (structural, table not consulted): '
          f'{len(bag_emitters)}' + (f" -- {', '.join(sorted(set(bag_emitters)))}" if bag_emitters else ' -- none'))
    print('\nconverter roles (adjudicated; see EMITTER_ADJUDICATION):')
    for role in ('emitter', 'guard', 'internal'):
        names = roles.get(role, [])
        print(f'  {role:<9} ({len(names)}): {", ".join(names) or "none"}')
    if stale_roles:
        print('  STALE — the adjudication no longer matches the file:')
        for w in stale_roles:
            print(f'    {w}')

    print(f'\nbags on ATOM-LESS powers (bag is the only copy): {len(atomless)}')
    for b in atomless:
        stats_note = '' if b['has_stats'] else '  [no stats object]'
        print(f"  {b['partition']:<28} {b['who']:<24} {','.join(b['orphan_slots'])}{stats_note}")

    by_role = {}
    for path, line, text, key in rust:
        role, why, claim = RUST_SITE_ADJUDICATION.get(key, ('UNADJUDICATED', '', None))
        by_role.setdefault(role, []).append((path, line, text, why, claim))
    print(f'\nRust sites naming `effects` (adjudicated; see RUST_SITE_ADJUDICATION): {len(rust)}')
    for role in ('strip-blocks', 'live', 'guard', 'covered', 'inert', 'no-op', 'not-the-bag',
                 'UNADJUDICATED'):
        rows = by_role.get(role, [])
        if not rows:
            continue
        print(f'  {role} ({len(rows)}):')
        for path, line, _text, why, claim in rows:
            enforce = role in ('covered', 'inert', 'live')
            new, seen_already = coverage_broken.get(claim, ([], [])) if claim else ([], [])
            total = len(new) + len(seen_already)
            flag = ('  <-- CLAIM BROKEN' if enforce and new
                    else f'  [{total} power(s)]' if total else '')
            print(f'    {path}:{line}{flag}')
            print(f'        {why}')
    if stale_sites:
        print('  STALE — the adjudication no longer matches the source:')
        for w in stale_sites:
            print(f'    {w}')

    print('\npopulations re-measured against the contract:')
    if not coverage_broken:
        print('  no power depends on the bag for any adjudicated read')
    for claim, (new, seen_already) in sorted(coverage_broken.items()):
        if claim in enforced:
            kind = 'CLAIM BROKEN' if new else 'holds — all of them are the atom-less rows above'
        else:
            kind = 'migration size'
        who = (new + seen_already)[0]
        extra = f', {len(seen_already)} of them atom-less' if seen_already else ''
        print(f'  {claim:<22} {len(new) + len(seen_already):>4} power(s)  [{kind}]{extra}'
              f'  e.g. {who}')

    print('\nsummon CASUALTIES — an ungated `EntCreate` atom and no summon (ENT-22):')
    for fork, (casualties, gated_only) in sorted(summon_casualties.items()):
        note = f'{len(gated_only)} gated-only record(s) correctly resolve none'
        print(f'  {fork:<12} {len(casualties):>4} casualty(ies)   [{note}]')
        for who in casualties[:5]:
            print(f'      {who}')

    print(f'\ntaunt bag slots WITH a Mez/Taunt atom: {len(taunt_backed)}')
    print(f'taunt bag slots with NO Mez/Taunt atom: {len(taunt_orphan)}')
    for w in taunt_orphan:
        print(f'  {w}')

    corpus = os.path.join('crates', 'coh_math', 'tests', 'frozen-corpus')
    answers = os.path.join('crates', 'coh_math', 'tests', 'frozen-answers')
    corpus_present = os.path.isdir(corpus) and os.path.isdir(answers)
    print(f'\nfrozen corpus present: {corpus_present}   [reported, NOT enforced]')
    if not corpus_present:
        print('  Gone, in 54093d10a2, and not re-mintable: minting needed `Bag`, deleted in the')
        print('  same commit. Recover a fork with:')
        print('  git show 54093d10a2^:crates/coh_math/tests/frozen-corpus/<fork>/bundle.json.gz')
        print('  This stopped being a blocker on 2026-09-26. It was demanded when the strip was')
        print('  a MIGRATION; what is left removes readers that cannot fire, which must move')
        print('  nothing — and crates/coh_math/tests/totals_baseline.rs grades exactly that,')
        print('  20,163 numbers per fork, red on any movement. See leg 4 of the docstring.')


    failed = []
    if atomless:
        failed.append(f'{len(atomless)} bag(s) on atom-less powers hold the only copy of a value')
    if blocking:
        failed.append(f'{len(blocking)} Rust site(s) whose answer the strip would change')
    if claims_broken:
        failed.append(f'{len(claims_broken)} adjudicated coverage claim(s) no longer hold — a '
                      f'`covered`, `inert` or `live` site has gained a CONTRACT-bag dependency: '
                      f'{", ".join(sorted(claims_broken))}')
    stranded = {fork: c for fork, (c, _) in summon_casualties.items() if c}
    if stranded:
        failed.append(
            f'{sum(len(c) for c in stranded.values())} power(s) create an entity and state no '
            f'summon: ' + ', '.join(f'{fork} {len(c)}' for fork, c in sorted(stranded.items())))
    if stale_roles:
        failed.append(f'{len(stale_roles)} converter role adjudication(s) no longer match the source')
    if stale_sites:
        failed.append(f'{len(stale_sites)} Rust site adjudication(s) no longer match the source')

    if failed:
        print('\nEXIT 1 — the strip is NOT safe to land yet:')
        for f in failed:
            print(f'  - {f}')
        return 1

    print('\nEXIT 0 — no bag survives in the contract, no atom-less bag holds the only copy,\n'
          'every coverage claim holds, and every Rust site left is the `live` writer, a\n'
          'deliberate `guard`, an `inert` read, or not the bag at all.')
    return 0


if __name__ == '__main__':
    sys.exit(main())
