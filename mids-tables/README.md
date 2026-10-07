# `mids-tables/`: Mids Reborn's own answers, vendored

Two tables per dataset, read out of an installed Mids Reborn's `EnhDB.mhd` and
committed here because nothing in this repo can produce them.

| File | What it is |
| --- | --- |
| `<dataset>/mids-uids.json` | Every enhancement UID Mids knows, keyed by set and piece, with each set's attunement prefix and level range. The `.mbd` export path writes these strings. |
| `<dataset>/mids-enh-names.json` | Mids' enhancement array in order, each row `[set short name, piece short name, UID, kind]`. The legacy `.mxd` reader needs both the short code and the array position. |

## Why these are committed and nothing else is

Every other input the pipeline reads is re-derived from `exported_powers/`, which is itself
committed, so a clone with no game install rebuilds the whole of `pipeline/` by running the
converters. These two can't be rebuilt that way.

- Mids matches a slotted enhancement by its name and empties the slot on a miss, with no
  error and no log. Guessing a name will lose enhancements silently.
- The names are Mids' own: `Numinas_Convalesence`, `ToHit_DeBuff`. The attunement prefix
  is per-set (`Crafted_` / `Attuned_` / `Superior_Attuned_`). No rule recovers any of it from
  a display name. Mids is the only authority on Mids' namespace, the same way the game's
  `.pigg` files are the only authority on the game's.

The dependency is baked-in, so the only choice is whether to make it visible.
These files are named here instead of folded in with the derived ones, where they used to
be: they were `src/data/datasets/<dataset>/generated/mids-uids.ts` and `mids-enh-names.ts`
until 2026-09-24, looking like every other generated file.

## How they reach the contract

`scripts/emit-contract.cjs` reads each file here, through `midsJson` in
`collect-composed-powers.cjs`, so `npm run regen` works on a machine with no Mids install, and
only a re-vendoring needs one. A missing file stops the emit and names the script that rebuilds
it; the emit never treats it as empty.

They were copied to `pipeline/<dataset>/` first until 2026-09-25, by the since-deleted
`scripts/emit-pipeline-json.cjs`, for no reason but giving the emitter one input root. `hand-data/`
had already refused that mistake, for the same reason: `pipeline/` is gitignored output a clean
build deletes, so anything in there has to be something the build can remake. These two aren't.
The same reasoning moved `effect-registry.json` out of `contract/` and keeps `hand-data/` out of
`pipeline/`. Deleting that script is what moved the read here.

The two scripts under `tools/mids-oracle/` still describe the copy in their headers. That was
deliberate rather than missed, and is now merely stale. They were left alone because
`scripts/` and `tools/` were hand-copied to a canonical sibling, so any edit to a shared file owed
a re-adjudication in both. That cost went on 2026-09-25 with the cross-repo machinery, which had
been grading this repo against a manifest describing `coh-sidekick-1.0`. Fixing the two headers is
now free and nobody has done it, so this paragraph is still the one to believe.

## Regenerating

Needs Mids Reborn installed, at the path `tools/mids-oracle/read_i12.py` spells
(`~/Games/mids-reborn/drive_c/MidsReborn/Databases/`, a Wine prefix driven by
`tools/mids-oracle/mids-wine.sh`).

```
python3 tools/mids-oracle/emit_mids_uids.py --dataset all
python3 tools/mids-oracle/emit_mids_enh_names.py --dataset all
```

Then run `npm run regen` and read the diff. `contract/` is committed, so a re-vendoring that
changes a shipped value shows up there rather than passing quietly.

Neither table says which Mids database it came from. An
`EnhDB.mhd` header says only "Mids Reborn Enhancement Database", with no fork and no version,
so the `sourceSha256` each table carries grades freshness and is blind to which database the
bytes are. Brainstorm reads Homecoming's, and Thunderspy reads Mids' Generic database,
because Mids has never shipped a Thunderspy one. The run prints the full provenance for
each dataset; `DATASET_PROVENANCE` in `emit_mids_uids.py` is its written home. See DATA-GAP
MBDEXPORT-2.

## Format

Pretty-printed, two-space indent, with a set's piece UIDs and an enhancement's name fields
each kept on one line. These are files a person reads on a Mids version bump, and a
one-line JSON diff would say only that the line changed. Key order is load-bearing: it is the
order the contract's `mids-uids` and `mids-enh-names` sections ship in, and `contract/` is
compared byte for byte.

Related: `hand-data/`, for committed data that is authored rather than vendored.
