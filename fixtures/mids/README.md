# Mids Reborn `.mbd` corpus

Builds written by Mids Reborn itself. Eight files across two forks — the first files this repo's
`.mbd` reader has ever been pointed at that the reader's authors did not write.

That last part is the whole point, and it is worth stating plainly because the importer looked
well covered without it. `src/utils/mids-import/` carries ~3,500 lines and eight test files, and
every one of them builds its `.mbd` inline with `JSON.stringify({...})`. A hand-made input grades
the reader against the author's model of the format; these grade it against the format. The first
run found MBDIMPORT-5.

## The five Homecoming files

| file | App string | Mids | Mids DB | build | entries / slots / enhancements |
|---|---|---|---|---|---|
| `homecoming/blaster-assault-rifle-tactical-arrow-v3861.mbd` | `Mids Reborn` | 3.8.6.1 | 2026.8.1359 | Blaster, Assault Rifle / Tactical Arrow, 48 | 40 / 98 / 93 |
| `homecoming/stalker-martial-arts-willpower-v37521.mbd` | `Mids' Reborn` | 3.7.5.21 | 2025.2.963 | Stalker, Martial Arts / Willpower, 49 | 41 / 98 / 92 |
| `homecoming/stalker-martial-arts-willpower-v3860.mbd` | `Mids Reborn` | 3.8.6.0 | 2026.5.1337 | the same Stalker, re-saved, mostly unslotted | 42 / 98 / 25 |
| `homecoming/warshade-umbral-blast-umbral-aura-v3860.mbd` | `Mids Reborn` | 3.8.6.0 | 2026.5.1337 | Warshade, Umbral Blast / Umbral Aura, 50 | 46 / 110 / 86 |
| `homecoming/warshade-umbral-blast-umbral-aura-slots-only-v3860.mbd` | `Mids Reborn` | 3.8.6.0 | 2026.5.1337 | a second Warshade, every slot placed, nothing slotted | 45 / 110 / 0 |

**The App string is not a constant.** 3.7.5.21 writes `Mids' Reborn` with an apostrophe and the
3.8 builds write `Mids Reborn` without one. Nothing may branch on it; it is provenance, not a
discriminator.

The two Stalker files are the same character saved by two Mids versions fourteen months apart, so
they are a drift pair rather than two builds: what differs between them is the writer, not the
build. The later save also strips most of the slotting, which is what makes it grade the
empty-slot path (98 slots, 25 enhancements).

Original filenames, since they carry provenance the renames drop: `Blaster (Assault Rifle -
Tactical Arrow) Ranged.mbd`, `Stalker (Martial Arts - Willpower).mbd`, `mids-slot-test.mbd`, and
`Warshade (Umbral Blast - Umbral Aura).mbd` for both Warshades.

**The Warshade is the Kheldian file**, authored to order on 2026-09-09 with both forms taken and
both slotted unevenly — Nova 6/6/2/6, Dwarf 1/4/3/6/1/1 — so that a per-sub-power count is
checkable rather than only a total. It is the first file to reach the importer's
`slottableSubPowerParent` branch, and it settles that branch's founding assumption: Mids does
write all ten form powers as `Inherent.Inherent.<name>`, never in the form's powerset.

It ships with `warshade-umbral-blast-umbral-aura-v3860.mnu` beside it — Mids' own
"export to game popmenu" for the same build, which lists every slotted piece by **Mids' UID**.
That makes it a second, independent reading of the same slotting: the `.mbd` says what Mids
stored, the `.mnu` says what Mids would hand the game, and the UID table has to satisfy both.

**The slots-only Warshade is a control, authored to order on 2026-09-09**, and it is the one file
here whose *method* is attested rather than inferred: the same character re-picked from scratch
with every slot placed by hand and none removed-and-replaced, then saved with nothing slotted.
It answers two different questions.

It is the empty-slot path at full scale — 110 slots, zero enhancements — where the 3.8.6.0
Stalker is only the partial (25 of 98). Every other file in the corpus couples the slot path to
the enhancement path, so a reader that counted a placed slot as an imported enhancement would
look right on all of them; on this one `slotsImported` is 101 while `enhancementsImported` is 0,
and the two claims finally come apart.

And it settles MBDIMPORT-8's slot levels. Its placed slots fall at exactly the levels the other
Warshade's do — level for level, including **2 at 47 and 1 at 49** — under a different power
selection and different pools. That is what took those two levels from "the author edited
something" to a property of the schedule Mids plans against: **Mids follows the RESPEC table**.
`RLevels.mhd` grants 3 slots at 47 and 3 at 49 for 73 total; `NLevels.mhd` grants none at either
for 67, and is byte-identical to this repo's exported schedule across all 27 grant levels. A
reader that validated `.mbd` slot levels against `assignable_boost` would have warned about
roughly ten placements on most real builds — a fail-loud channel crying wolf. The right behaviour
is to retain the level verbatim and validate nothing.

## The three Rebirth files

Mids Reborn 3.8.6.0, database 2023.7.445, all three **authored to order** on 2026-09-09 by the
user, in a Mids running under Wine — which is why they reach what the Homecoming arm cannot.

| file | build | entries / slots / enhancements | what only it reaches |
|---|---|---|---|
| `rebirth/guardian-dark-assault-atmospheric-composition-v3860.mbd` | Guardian, Dark Assault / Atmospheric Composition, 49 | 32 / 106 / 78 | an archetype Homecoming does not have; a category the name map cannot see (MBDIMPORT-7) |
| `rebirth/mastermind-mercenaries-trick-arrow-v3860.mbd` | Mastermind, Mercenaries / Trick Arrow, 48 | 36 / 107 / 79 | three `_H` pet shadow entries; the Superior Endless Nightmare set |
| `rebirth/veat-night-widow-teamwork-v3860.mbd` | Night Widow, 49 | 32 / 102 / 89 | a VEAT branch; **every origin tier at every relative level** |

**A `.mbd` cannot come from a third fork.** Mids ships Generic, Homecoming and Rebirth databases
and has never supported Thunderspy, so Rebirth is the only cross-fork file that can exist and
these three are it. The `Database` stamp routes all three to `serverId: rebirth`, which is the
`CrossForkChoice` half of the import row graded for the first time.

**The Widow is the file that mattered most, and it is worth saying why it exists.** Asked for a
levelling build with minus-level SOs, it carries 11 TrainingO, 31 DualO and 37 SingleO spread from
MinusThree to PlusTwo. Every other `.mbd` anywhere in this repo — real or hand-made — carries only
`Even`, `PlusThree` and `PlusFive`, and grades `None` or `SingleO`, where the only `SingleO` pieces
are Hamidons. One request, and the shape that had hidden MBDIMPORT-6 for as long as the importer
existed walked in the door.

## What is deliberately not here

Five more `.mbd` files exist alongside these, and all five say `"App": "CoH Planner"` — our own
exporter's output. They are not in the corpus, because a file this planner wrote can only
disagree with this planner where this planner disagrees with itself. They belong to the export
half's round trip, which is a different claim from reading what Mids writes.

## The format

Plain JSON, uncompressed, CRLF from Mids and LF from us. No decode step at all — the ~3,500 lines
of the importer are name and UID resolution, not parsing.

```json
{ "BuiltWith": { "App": "Mids Reborn", "Version": "3.8.6.1",
                 "Database": "Homecoming", "DatabaseVersion": "2026.8.1359" },
  "Level": "48", "Class": "Class_Blaster", "Origin": "Technology", "Alignment": "Hero",
  "PowerSets": ["Blaster_Ranged.Assault_Rifle", "…", "", "Pool.Leaping", "…"],
  "LastPower": 24,
  "PowerEntries": [ { "PowerName": "Blaster_Ranged.Assault_Rifle.Tranquilizer_Dart",
                      "Level": 1, "StatInclude": true, "ProcInclude": false,
                      "VariableValue": 0, "InherentSlotsUsed": 0, "SubPowerEntries": [],
                      "SlotEntries": [ { "Level": 1, "IsInherent": false,
                                         "Enhancement": { "Uid": "Superior_Attuned_Superior_Defiant_Barrage_A",
                                                          "Grade": "None", "IoLevel": 49,
                                                          "RelativeLevel": "Even",
                                                          "Obtained": false },
                                         "FlippedEnhancement": null } ] } ] }
```

`BuiltWith.Database` is the fork stamp — the only thing in the file that says which game's data
the author was planning against. An empty string in `PowerSets` is a real entry (the unpicked
slot), not a hole. `VariableValue` is the per-power stack/targets slider. A power is named by its
INTERNAL name and nothing else, which is what makes the next section the load-bearing part.

## What the corpus proved, measured

**The internal-name rotation is not an edge case — it is on both builds.** HC reassigns internal
names under stable display names (MBDIMPORT-2), and both files walk into it:

| the `.mbd` says | this dataset's internal name | what a player sees |
|---|---|---|
| `Blaster_Support.Tactical_Arrow.Gymnastics` | `Quickness` | Gymnastics |
| `Blaster_Support.Tactical_Arrow.Oil_Slick_Arrow` | `Gymnastics` | Oil Slick Arrow |
| `Stalker_Defense.Willpower.Resurgence` | `Reconstruction` | Resurgence |
| `Stalker_Defense.Willpower.Reconstruction` | — no counterpart | — |

An importer matching on the internal name alone lands five enhancements on the wrong Tactical
Arrow power and reports nothing. The shipped reader gets all four rows right, through the
per-powerset map derived from Mids' own database (`src/data/datasets/*/generated/mids-name-map.ts`).
**Any port shares that map rather than matching names.**

The last row is the one with no answer: Mids still lists a Stalker Willpower `Reconstruction`, and
no power in HC's Stalker Willpower displays that name. The reader refuses it rather than falling
through to Regeneration's same-named power, which is right — and MBDIMPORT-5 was what happened
next, until the refusal started reporting what the power was holding.

## What it caught

- **MBDIMPORT-6** (fixed) — Mids grades an origin enhancement `TrainingO`/`DualO`/`SingleO` and
  the importer tested for `'SO'`/`'DO'`/`'TO'`. The Widow imported **10 of its 89 enhancements**
  and warned about the other 79. Nothing had noticed because the only origin-graded pieces in the
  Homecoming arm are three Hamidons, which a different branch owns, and because the test that
  verified MBDIMPORT-4 passed `'SO'` by hand.
- **MBDEXPORT-2's Endless Nightmare clause** (fixed) — the Mastermind slots a set this repo said
  Mids did not have. Mids has it, spelled `Superior _Endless_Nightmare`; our generated key kept
  the stray space and matched nothing in either direction.
- **MBDIMPORT-7** — 13 Guardian secondaries have no name-map rows, because our export calls the
  category `Guardian_Comp` and Mids calls it `Guardian_Composition`.
- **MBDIMPORT-5** (fixed) — the refused power took its slotted enhancements with it, and the
  summary reported `enhancementsFailed: 0`. Three instances when it was found: the 3.7.5.21 Stalker
  six, the 3.8.6.0 Stalker one, the Guardian six — the Guardian's went to MBDIMPORT-7, which is why
  its power stopped being refused at all. Same shape as MBDIMPORT-1, one door over.
- **MBDEXPORT-3** — found by pointing Mids at a file *we* wrote rather than by reading one it
  wrote. The export applies no reverse name rotation, so `Disrupting_Torrent` came up in Mids as a
  blank row still holding six enhancements; the same file with that one name spelled Mids' way
  bound all of it.
- **MBDEXPORT-4** (fixed) — the Warshade's round trip, opened in a real Mids. Our exporter wrote
  `Grade: enh.tier`, so a `SingleO` read in came back out as `SO` — the very token MBDIMPORT-6
  had just proved Mids never uses. `Enum.Parse` throws inside `LoadBuild`, and **the entire build
  failed to open**: an error dialog reading *Requested value 'SO' was not found*, and Mids falling
  back to an empty default. Three origin pieces in 86 were enough. The import half read Mids'
  grade names correctly and the export half wrote ours; nobody had run the two in sequence. Both
  now read one table, and the re-export opens: 50 of 86 enhancements bind, up from none.
- **MBDEXPORT-5** — with the grades hand-patched so the file would load at all, the same round
  trip showed the other half. Our exporter writes the ten form sub-powers at their powerset path
  (`Warshade_Offensive.Umbral_Blast.Dark_Nova_Blast`) where Mids wrote and expects
  `Inherent.Inherent.Dark_Nova_Blast`. All ten came back as blank rows at levels 6/8/10/12 and
  32/35/38/41/44/47, holding 6/6/2/6 and 1/4/3/6/1/1 empty slots — the file's own counts, with
  **36 of 86 enhancements gone** and `warnings: []`.

## What this corpus cannot see

- **Two forks, and no third is possible.** The Rebirth arm landed the same day the Homecoming one
  did, so `CrossForkChoice` is graded — but **there is no third file to go looking for**: Mids
  Reborn ships Generic, Homecoming and Rebirth databases and has never supported Thunderspy (user,
  2026-09-09), so no Thunderspy `.mbd` has ever been written by anyone. Brainstorm rides
  Homecoming's stamp. What the same fact does to the *export* half is DATA-GAP MBDEXPORT-2.
- **No Peacebringer, and no Kheldian on Rebirth.** The Warshade covers the branch; what it cannot
  say is whether Rebirth's *renamed* form attacks survive the same path, which is where the
  `Kheldian_Pets.*` traversal and the rename collision both live.
- **`SubPowerEntries` and `FlippedEnhancement` are still dark, and the Kheldian ruled out the
  likeliest producer of the first.** `SubPowerEntries` is non-empty in none of the eight files —
  the Warshade writes `[]` on all 46 entries — is typed `unknown[]`, and is read by nothing; our
  exporter writes `[]` at all three of its sites. `FlippedEnhancement` is `null` on all 829 slots
  and likewise read by nothing. Whatever Mids feature produces either, no build here uses it.
- **Nothing with a slotted Afterburner.** Mids writes it `Inherent.Inherent.Afterburner`; the HC
  `Fly` group in `granted-powers.ts` calls its granted child `Fly_Boost`, so the entry matches
  neither branch and is dropped. The Warshade's carries zero slots, so the drop costs nothing and
  proves nothing — a build that slots it would be MBDIMPORT-5's shape at a third door.
- **No accolades or incarnates on the Rebirth arm** — all three report 0, where the Homecoming
  files carry four accolades and three incarnates each.
- **The `.mxd` arm is six files and a different question** — see `mxd/` below. It grades the
  legacy reader, and nothing it holds is graded by anything above: the two corpora share no file
  and no format.
- **Nothing about totals.** The corpus grades what arrives, not what it then computes.

## `ours/` — the same eight builds, in our own writer's spelling

Eight more files, and none of them is a corpus file: they are what OUR exporter writes when it is
handed each corpus build, produced by
[`scripts/write-mbd-export-fixtures.ts`](../../scripts/write-mbd-export-fixtures.ts) and
committed beside the originals.

They exist because everything above grades the READER, and the two directions now share the UID
table and the per-powerset name map. A table edit that suits the writer and rots the reader leaves
every gate above green — and so does a writer that simply does not write something, which is what
the first run found: six differences, MBDEXPORT-11 through -16, all writer-side.
The writer round-trip gate reads both
spellings of each build against one database and compares the results, pinning each known
difference by its population.

**These cannot be regenerated here any more.** The command was:

```
npx tsx --import ./scripts/env-register.mjs scripts/write-mbd-export-fixtures.ts
```

Every part of that line is gone: `write-mbd-export-fixtures.ts` and `env-register.mjs` were
deleted on 2026-09-25, and the `mids-export.ts` and `src/data/*` modules it read went with `src/`
before them. So these eight files are a frozen record of what our writer produced, on the same
footing as the corpora above rather than as derived output — the argument the 48 files under
`fixtures/{oracle,enhancement,totals,procs,movement,set-bonuses}` are kept on.

Nothing walks this directory looking for a roster — the pairing is by file name against the two
fork directories, and a corpus file with no twin panics rather than being skipped.

**What these cannot say** is whether a real Mids parses what we write. That measurement is Wine
(`tools/mids-oracle/mids-wine.sh`), and it is what MBDEXPORT-4 and -5 took.

Four gates read the eight files Mids wrote: the importer's, the round trip's, the export
names', and the Rust reader's. **Each one asserts its
roster against this directory**, because all four list the files by hand — they have to, since
each entry carries a fork or a set of pinned counts a directory walk cannot supply — and a file
added here without being listed would otherwise be graded by nothing while every assertion stayed
green.

The importer's runs one arm per fork. It pins each file's counts, and since MBDIMPORT-5 closed it
asserts the reconciliation those pins were standing in for: what the file holds is what the two
tallies add to, on every file. Both twins were run against the Homecoming arm on 2026-09-09 and returned
identical summaries, so the verdict here speaks for the beta's copy as well as canonical's.

## `mxd/` — six posted files, one per thing they disagree about

Six legacy `.mxd` builds, taken from the posted corpus the `.mxd` reader was cut against (1,929
files). They are not a sample: each is here for a property no other one has, and
the `.mxd` gate pins the roster by name so a file
swapped out is a property that stopped being graded.

| file | what only it has |
|---|---|
| `brute-war-mace-shield-defense-fmt101.mxd` | the 1.01 record shape, an HTML post with `&nbsp;` gutters, `:50` on every code, a Hamidon, five empty slots, an incarnate, and six codes this Mids database has renamed since — the only fixture that exercises the residual pass |
| `blaster-assault-rifle-gadgets-fmt31.mxd` | the 3.1 shape, and a tab-separated post |
| `sentinel-assault-rifle-bio-armor-fmt32.mxd` | the 3.2 shape — a two-byte slot level — and a post that states no IO level at all |
| `corruptor-fire-blast-time-manipulation-fmt101-unflipped.mxd` | a header that says 1.01 over records with no `FlippedEnhancement`, which is why the shape is measured rather than looked up; and the only file with no post half, so it grades the unpaired read |
| `bane-spider-soldier-fmt101-flipped.mxd` | the only spelling that USES the flipped slot, and a VEAT whose file names its branch sets and not the sets under them |
| `tanker-invulnerability-energy-melee-fmt101-accolades.mxd` | four accolades, which Mids files at level `-1` and the post writes as `Level 0:` |

## What the `.mxd` arm cannot see

- **One fork.** A `.mxd` has no field for a fork — the format predates them — so every file here
  reads against whatever dataset is loaded, and all six are Homecoming builds. There is no such
  thing as a Rebirth `.mxd` to find.
- **No origin enhancements, and no TO/DO/SO anywhere.** Every slotted piece across all six is an
  invention, a Hamidon or an empty slot, so the branch that reads a piece's GRADE is graded only
  on its Hamidon arm. The tier mapping for a training/dual/single origin piece is structurally
  identical and measured on nothing.
- **Nothing from a Mids between 1.01 and 3.1.** Whatever versions sat in that gap wrote shapes
  nobody here has seen; the reader refuses a stream that fits none rather than reading it.
