# `/buildsave` corpus

Builds written by the Homecoming game client itself, in response to the in-game `/buildsave`
command. Seven characters, levels 28–50, six archetypes.

This is the second oracle in the repo that the graded code did not write (the first is
`fixtures/skif/v4`, and it found four defects). It is stronger than that one in a way worth
stating: an exported `.skif` was authored by the beta planner, so it can only disagree with this
planner where the two planners disagree. These files were authored by the game.

## The format

CRLF line endings. Three sections:

```
Tzarina: Level 28 Mutation Class_Corruptor      ← name, level, origin, the binary class token

Character Profile:
------------------
Level 1: Corruptor_Ranged Water_Blast Hydro_Blast    ← level, category, powerset, power
	Mutation_Accuracy (15)                            ← one tab-indented line per slot
	Mutation_Damage (15)
	EMPTY                                             ← an unfilled slot
------------------
Badges Earned:
------------------
Tourist
DVDEdition
```

Enhancement lines carry the boost's binary name and, for crafted pieces, the level and any
booster: `Crafted_Armageddon_A (50+5)`. Attuned pieces print `(1)`, which is a count and not a
level. A character name may contain spaces (`Maiden Fury`).

`Level 0:` marks an auto-granted sub-power (`Fly_Boost`), matching the export's `available: -1`.

## What the corpus resolves against, measured

Every name these files print is a binary name the export already ships, so nothing here needs a
display-name match or an alias table:

| | resolved |
|---|---|
| enhancement UIDs vs `exported_powers/boosts/` | 279 / 279 |
| power triples vs `exported_powers/<cat>/<set>/<power>.json` | 137 / 137 |
| `Category Powerset` vs the contract's `setPath` index | 26 / 28 |

The two set paths with no contract entry are the ones that are not picks, which is the
distinction an importer has to get right rather than a gap to close:

- **`Inherent.Fitness`** (28 lines) — Homecoming grants these four, and ships them as their own
  set beside the `Pool.Fitness` the picker offers. The build models them as
  `InherentCategory::Fitness` grants, so an importer that read them as picks would spend four
  picks and a pool slot the character never spent.
- **`Redirects.Inherents`** (1 line, `Gauntlet_Proc`) — a redirect target, ownable by nobody.

**Grants carry slotting.** Six of the seven builds slot their granted inherents — Health with
three pieces in all six, plus Swift and Sprint — so dropping the `Inherent.*` lines would drop
some of the most load-bearing globals a build has.

## What it has caught

- **The Synthetic Hamidon family, modeled nowhere** (DATA-GAP BOOST-1). Eleven pieces on all
  three forks; `convert-special-enhancements.cjs` scanned `hamidon_*`, which
  `synthetic_hamidon_*` does not match. One build here slots a Synthetic Endoplasm, which is
  the whole argument for an oracle the planner did not write: nothing about the pipeline was
  internally inconsistent, so no gate over it could have noticed.

The gate that reads these files uses a reader of its own rather than the importer's — a gate that shares its
subject's parser cannot see that parser being wrong.
