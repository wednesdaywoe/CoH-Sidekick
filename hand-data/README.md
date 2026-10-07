# `hand-data/` — committed data nothing derives

Data that is **authored**, not extracted. It is read and never written, and it has no
upstream to be regenerated from: if a file here is lost, it is lost, and only git has it
back.

Everything else the pipeline reads comes from `exported_powers/` (committed, and itself
produced from the game's `.pigg` files by `tools/bin-crawler`). A converter's output is
reproducible — delete `pipeline/` and run the regen. These files are not.

Two kinds of consumer read this directory, and the difference matters when something moves.
Most of it is **pipeline** input, read at regen time by a script: the per-fork subdirectories and
the six shared files — `origins.json`, `purple-patch.json`, `levels.json` and the three `proc-*`
ones. Two others are
**compile-time** input — `effect-registry.json` and `set-bonus-stat-vocab.json`, which `coh_math`
`include_str!`s, so they are baked into the binary and a path typo is a rustc error, not a runtime
one. That also puts those two in the RC bundle's sparse checkout list (`/hand-data/*.json` in
`rc-bundle.yml`) — see the note under the table.

A pipeline reader reads this directory **directly**. Authored data is not copied into
`pipeline/` on its way to the emitter: `pipeline/` is gitignored output that a clean build
deletes, and data with no upstream does not belong in a directory named after output. That is
the same argument that moved `effect-registry.json` out of `contract/`, and
`collect-composed-powers.cjs`'s `handJson` is where the emitter acts on it.

The near neighbour is `mids-tables/`, which is committed for a related but different
reason: those tables are not authored, they are read out of an installed Mids Reborn, which
no clone has. Authored data goes here; data vendored from an external tool goes there. Those two
tables were copied into `pipeline/` until 2026-09-25, the one place the rule above was not
followed; `midsJson`, beside `handJson`, reads them where they sit now, and `pipeline/` holds
nothing but derived output.

## Why this directory exists

The hand data lived in `src/data/`, mixed in with the TypeScript oracle, and that is a
problem in two directions:

- **Python had to parse TypeScript to read it.** `extract-rebirth-io-sets-v2.py` carried a
  brace matcher and a trailing-comma stripper to get at a module body that is almost JSON
  and not quite. See the comment on `_load_hc_sets`.
- **`src/` is on its way out.** The oracle is meant to be a reference implementation the
  Rust engine is graded against, not a stage in the supply line that feeds the shipped app.
  Hand data sitting inside it makes `src/`
  load-bearing and blocks the removal.

## What is here

| File | What it is | Who reads it |
| --- | --- | --- |
| `homecoming/io-sets-raw.json` | Homecoming's IO set library: 227 sets with their piece aspect lists and bonus tiers, hand-curated where the binary cannot reproduce them. | `scripts/extract-rebirth-io-sets-v2.py`, which overlays each fork's own binary values and writes `pipeline/<id>/io-sets-raw.json`. Rebirth and Thunderspy reuse the entry for any set they share with Homecoming. |
| `<id>/archetypes.json` | Per fork: the authored half of each archetype — display name, side, description, inherent blurb, the five hand-curated scalars, the primary/secondary roster and the VEAT branches. The binary-derived stats are deliberately absent; see below. | `scripts/convert-archetype-registry.cjs`, which merges in `pipeline/<id>/archetype-stats.json` and writes `pipeline/<id>/archetypes.json`. |
| `effect-registry.json` | 19 KB. How each power-effect key is interpreted for display and resolution — display unit, which enhancement aspect scales it, which table path resolves it. App glue over the exported data, not derived from it. Its own `_comment` field is the field-by-field spec. | `coh_math/src/effect_registry.rs` (`include_str!`), plus two read-only surveys in `scripts/`. |
| `set-bonus-stat-vocab.json` | 4.5 KB. The raw→internal stat vocabulary for set bonuses: which engine stat each raw bonus string resolves to, and which stats are paired. | `coh_math/src/set_bonuses.rs` (`include_str!`), and `scripts/emit-set-bonus-fixtures.ts` to name the stat behind a raw string its own oracle drops. |
| `origins.json` | The five character origins. Shared by every fork, and the whole of what the pipeline ever read off `src/data/enhancements.ts`. | `scripts/emit-contract.cjs`, through `handJson`. It rides the contract's `enhancements` section as `origins`. |
| `purple-patch.json` | Combat level-difference scaling: five tables of numbers and one constant. Shared by every fork, all four of which hold the same values today. Its `_comment` is the spec — the signed-`levelDiff` convention, the table lengths, and what a fork that retunes them does. | `scripts/emit-contract.cjs`, through `handJson`. It IS the contract's `purple-patch` section, minus the `_comment`. |
| `levels.json` | Level-progression rules the export does not carry — the level cap, the slot cap, the enhancement-type availability table, the epic-tier requirements, the incarnate level and slot table — plus the authored half of the four inherent Fitness powers. Shared by every fork, because only Homecoming ever authored a `levels.ts`. Its `_comment` is the spec. | `scripts/emit-contract.cjs`, through `handJson`. It is most of the contract's `levels` section; the emitter splices each Fitness power's atoms off the fork's own `fitness` pool and adds the two inherent lists and the seven schedule constants. |
| `proc-data.json` | The authored proc/global IO table: 184 enhancement pieces with their PPM, mechanics prose, level range and rarity. A BASE, not a section — six derived side tables and the two below are stapled onto it. Shared by every fork. Its `_comment` is the spec, `_groups` records the authored grouping and `_notes` the per-entry provenance. Key AND field order are deliberately unsorted; see Format. | `scripts/emit-contract.cjs`, through `handJson`, whose `procDatabase()` performs the eight-way merge. Also `scripts/extract-proc-data.py`, which reads its key list and its `mechanics`/`setName`/`ppm` as the oracle it resolves each derived table against. |
| `proc-residual-effects.json` | Structured effects for the 43 procs the generator cannot reach: Rebirth-only sets, pet summons, PBAoE ally buffs, self meters. Each is a faithful transcription of the proc's `mechanics` string. | `scripts/emit-contract.cjs`, seventh of the eight merges, REPLACING an entry's whole `effects` array. |
| `proc-variable-controls.json` | Three procs whose contribution is not a single literal: two self-stacking "By the Slotted Power" buffs and one HP-scaling floor. Patches `maxStacks`, `valueMax` and `scaleTable` only. | `scripts/emit-contract.cjs`, LAST of the eight merges, and the only additive one. |

`effect-registry.json` and `set-bonus-stat-vocab.json` came out of `contract/`, which is becoming build output a clean build can delete —
authored data must not live in a directory named after output. Moving them is also why
`rc-bundle.yml` carries `/hand-data/*.json`: that workflow checks out a sparse subset of a 939 MB
tree, and the first dispatch died 614 seconds in, as a rustc error, on these exact two files being
outside the list. `scripts/keys/rc-sparse-checkout-covers-includes.py` derives the embed set from
the source and fails in under a second if it happens again.

## What belongs here, and it has all arrived

Nothing is owed. The rehoming finished on 2026-09-25 with `proc-data.ts`, and no pipeline script
reads authored data out of `src/data/` any more, each move recorded in the commit that made it.
`enhancements.ts` gave up `ORIGINS` and `purple-patch.ts` gave up its
tables, the two straight moves; `levels.ts` and `proc-data.ts` split along the seam where authored
data was glued to derived data by a merge that only resolved when the module was imported.

The duty this paragraph used to record is discharged, and both halves of it went on the same day.
It said three pairs of copies had to be kept in step until `src/` went — `src/data/proc-data.ts`,
`proc-residual-effects.ts` and `proc-variable-controls.ts` against the three rows above — and that
no header said so in the modules themselves because a shared-file edit owed a re-adjudication in
the canonical sibling. `src/` went on 2026-09-25, so there is no second copy to keep in step; the
adjudication went the same day, once the manifest was shown to describe `coh-sidekick-1.0` rather
than this repo. The modules are recoverable from `6caffef34^`, the commit before the one that
deleted them, if the rows above are ever doubted.

## How `archetypes.json` was split, and why it is split that way

`archetypes.ts` was the awkward one and was deliberately last. It was not one thing: each
archetype's `stats` block was a TypeScript spread, `{ ...ARCHETYPE_BINARY_STATS[at],
baseEndurance: 100, ... }`, which only resolved when the module was imported. That is why
moving the file would not have moved `generate-archetypes.cjs` with it, and why it outlived
every other entry in the oracle-reading dump's per-dataset list (that script, then named
`emit-pipeline-json.cjs`, was itself deleted on 2026-09-25).

The fix was to cut along the seam the spread already marked, rather than to freeze the
resolved result:

- **Authored, so it lives here.** Name, side, description, inherent text, the five scalars
  (`baseEndurance`, `baseRecovery`, `damageModifier`, `buffDebuffModifier`, `defenseCap`),
  the roster and the branches. The roster is authored despite having once been derived:
  Homecoming orders its set lists for display rather than alphabetically, and the
  Kheldian/VEAT sets are filed under `epic`, where a primary/secondary lookup finds nothing.
- **Derived, so it stays out.** The HP and HP-cap curves, resistance cap, threat, damage
  cap, absorb ceiling, the recharge and endurance clamp bounds, and the movement and
  attribute ceilings — roughly two thirds of the resolved file by size. These come from
  `classes.bin` via `convert-archetypes.cjs`, which exists precisely because a hand-typed
  HP table used to diverge from the live game in silence. Snapshotting them into this
  directory would have re-introduced that.

`convert-archetype-registry.cjs` performs the merge at regen time and refuses to let an
authored field shadow a binary one — the spread allowed that quietly, and none does today
on any fork. Its output is byte-identical to what the TypeScript produced, on all four.

`generate-archetypes.cjs` survives as what it always claimed to be: a bootstrap that writes
a fork's **first draft** of this file, to be read and corrected by a person. It is not part
of the regen. With the data in JSON it no longer parses TypeScript, and it now names any
archetype a fork has that Homecoming cannot supply metadata for (Rebirth's Guardian) rather
than dropping it.

## Format

Plain JSON, two-space indent, keys sorted, one trailing newline — a person has to read and
edit these, and a diff has to be reviewable. Sorting stops where the order is itself data:
`<id>/archetypes.json` sorts its archetype ids, and leaves field order inside an archetype
alone, because `branches` lists Night Widow before Fortunata for a reason and a sort would
quietly lose that. `levels.json` is the same at one remove — field order inside a Fitness power
is the order those keys land in the contract, and `emit-contract.cjs` writes with
`JSON.stringify(value, null, 1)`, so sorting them would move shipped bytes. `proc-data.json` is
that case at full size and in both directions: its 184 keys and the nine fields inside each are
exactly the order the contract's `proc-data` section ships in, so neither is sorted, and the
authored order is also the grouping its `_groups` records. `proc-residual-effects.json` keeps its
order for the grouping alone — the merge it feeds writes each key onto a different entry, so
sorting would be harmless to the bytes and would still lose the buckets. Where an order is
neither sortable nor recoverable it gets its own field — that file's `archetypeOrder` is the
registry order, which the two id lists do not imply.

This is deliberately **not** the format of `pipeline/*.json`, which is machine output
written by `JSON.stringify` with no whitespace.

A file here holds the data itself, with no wrapper naming the TypeScript export it used to
be. The export name belongs to the module it came out of, not to the data.

Two exceptions, both deliberate. A file may carry a `_comment` key holding its own field-by-field
spec — `effect-registry.json` does, and that text is the only place the meaning of its fields is
written down, so it travels with the data rather than in a header comment the format cannot hold.
The three proc files extend that to two more underscore keys, for prose that was per-entry rather
than per-file: `_groups`, an ordered list of the headings that sat above runs of entries in the
module they came out of, each with the key its run starts at, and `_notes`, keyed by entry, holding
the provenance for a hand-curated value. Both exist because the TypeScript carried that text in
comments and a migration does not get to delete it. No proc key starts with an underscore, and
`emit-contract.cjs` drops every key that does.
And a file may have several named top-level tables (`effects`, `statNameMap`, `pairedStats`) when it
genuinely holds more than one; that is not the same thing as a wrapper around a single table.
