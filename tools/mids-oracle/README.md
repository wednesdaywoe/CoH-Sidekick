# mids-oracle — Mids Reborn structural oracle (DSH1 + DSH5)

A Linux-native reader for the Mids Reborn `I12.mhd` power database, used as a
**structural oracle** for the Deductive Schema Harness
([streams/DEDUCTIVE_SCHEMA_HARNESS.md](../../docs/DEDUCTIVE_SCHEMA_HARNESS.md)).
Mids models a power the same way the game and our bin parser do — a flat array of
atomic, single-attrib effect records at *template granularity* — so it is a clean
parser-to-parser structural diff, **no Mids calc engine required**.

Trust boundary (plan doc): **Mids for topology, bins for names and numbers.** TRUST
sub-effect count/identity, `EffectType`/`DamageType`/`MezType`, `PvMode`,
`Resistible`, `ModifierTable` selection, `Aspect`/`AttribType`. DISTRUST exact
`Scale`/`Mag`/`Duration` (Mids is ~5 weeks rebalance-stale). When Sidekick and Mids
disagree numerically, the raw `.pigg` bin is the tiebreaker.

## Files

- **`read_i12.py`** — ports the MidsReborn `BinaryReader` layout to Python. Seeks the
  `\x0cBEGIN:POWERS` marker, reads `count+1` Powers (each with inline `count+1`
  Effects), maps enums, and emits one JSON line per power at template granularity.
  Layout is transcribed verbatim from (do not reorder without re-checking these):
  - `MidsReborn-master/MidsReborn/Core/DatabaseAPI.cs` `LoadMainDatabase` (top level)
  - `.../Core/Base/Data_Classes/Power.cs:213` `Power(BinaryReader)`
  - `.../Core/Base/Data_Classes/Effect.cs:87` `Effect(BinaryReader)`
  - `.../Core/Requirement.cs:67` `Requirement(BinaryReader)`
  - `.../Core/Enums.cs` (enum ordinals)

  **Self-check (the one desync risk):** the format's `+1` array idiom (writes
  `Length-1`, reads `count+1`) means a single misread field desyncs the whole
  stream. After reading `count+1` powers the reader MUST land exactly on the
  `BEGIN:SUMMONS` string; it raises with the byte offset otherwise. Passing this on
  the full HC DB (11,002 powers / 73,431 effects, Mids 2026.5.1337) proves the layout
  is byte-correct. The database is the one in the Wine prefix `mids-wine.sh` drives —
  `read_i12.DEFAULT_MHD`, which every tool here resolves through.

- **`diff_oracle.py`** — PoC structural comparison of the oracle vs our parser export
  (`exported_powers/`), canonicalized to the bridge-free identity tuple
  `(modifier_table, aspect, pv_mode, resistible)`. `--normalize-pvp` applies the
  combat canonicalization (fold `pv`→Combat, strip the `pvp` table token) that
  reconciles Mids' explicit PvE/PvP record pairs with our Any-base + `_pvp*`-override
  encoding. This is a **PoC validator, not the production harness** (that is DSH5,
  sequenced after the DSH4 closed schema).

- **`emit_canonical.ts` + `diff_harness.py`** — the DSH5 production harness. The TS
  emitter canonicalizes the whole HC export via the **tested DSH4 bridge**
  (`ingestExportPower` from `src/data/core/atomic-effect.ts`), resolving redirect
  shells, so the app's schema and the oracle diff can never drift (the bridge is
  single-source; Python never re-ports it). `diff_harness.py` joins every Mids power
  to the canonical export by `full_name`, keys effects by the DSH4 identity, checks
  the structural invariants, runs a tiered classifier, and writes
  `oracle_divergence_rules.json` (the committable baseline) + a coverage manifest.
  See "DSH5 harness" below.

- **`read_enhdb.py`** — DSH9 bootstrap reader for `EnhDB.mhd` (enhancement + IO set DB).
  Ports `DatabaseAPI.LoadEnhancementDb` plus `Enhancement(BinaryReader)` and
  `EnhancementSet(BinaryReader)` verbatim. Emits either enhancements, sets, both,
  or a summary. For enhancement FX payloads it reuses `read_i12.py`'s
  `Effect(BinaryReader)` parser at template granularity.

- **`diff_enh_oracle.py`** — minimal DSH9 coverage diff. Compares:
  - oracle set names (`EnhancementSet.display_name`) vs `src/data/datasets/<dataset>/io-sets-raw.ts`
  - oracle proc tuples (`set display name + enhancement name` where `is_proc`) vs
    `src/data/proc-data.ts` (`setName + ioName`)

  This is intentionally a bootstrap worklist generator, not yet the full value-level
  DSH9 gate.

  Two guards sit under `--value-diff`, both of which fail `--strict` **even behind a
  baseline** (PROV-5). They grade the comparator, not the data, so a baseline must not
  forgive them:
  - **stat vocabulary** — the two sides name the same 33 stats and nothing compared
    the lists. A repo-side rename or an unmapped effect type does not read as an
    error; it splits one matching row into a `missing` line *and* an `extra` line.
  - **unmapped oracle effect rows** — an effect this comparator has no name for is a
    gap in the mapper, not an absence in the data. Prints each shape and its count.

  `proc-data.ts` is one file for all three forks while the oracle DB is one fork's, so
  procs whose set is absent from the dataset's own `io-sets-raw.ts` are excluded and
  counted (19 Rebirth-only pairs on a Homecoming run).

- **`test_read_enhdb.py`** — smoke regression check for the new reader (alignment,
  count floor, and stable identity anchors).

- **`enh_oracle_residual_baseline.json`** — the DSH9 committed residual. Carries
  `read_i12.provenance` for **both** databases it grades (`oracle.enhdb`,
  `oracle.i12`) since PROV-5; before that it named neither.

- **`test_diff_harness.py`** — pure-function checks on the DSH5 comparator's record
  layer (the PROV-4 Enhancement fold and the complete-type-set fold). Needs no `.mhd`
  and no canonical cache, so unlike the harness itself it runs anywhere.

## Requirements

Local only — the `.mhd` databases live in a gitignored Wine prefix, so these tools do
not run in CI. `MidsReborn-master/` is the vendored Mids SOURCE, read by a human when
porting a binary layout; it is not where a database is read from. It held one until
PROV-3 (2026-09-13), which is how the DSH5 baseline came to name a database nobody
could produce — see the provenance note below. Wiring the harness into CI (with a
committed DB or a golden JSON export) is DSH5/DSH7. Python 3, stdlib only.

## Usage

```sh
python3 read_i12.py                              # HC I12.mhd -> stdout JSONL
python3 read_i12.py --grep Trick_Arrow --limit 5 # spot-check specific powers
python3 diff_oracle.py --normalize-pvp           # DSH1 gate-2/3 known-answer comparison
python3 diff_oracle.py --cohort trick_arrow --normalize-pvp

# DSH9 bootstrap (EnhDB oracle):
python3 read_enhdb.py --mode summary
python3 read_enhdb.py --mode sets --grep "gambler" --limit 5
python3 diff_enh_oracle.py --dataset homecoming --show 20
python3 diff_enh_oracle.py --dataset homecoming --value-diff --show 20
python3 diff_enh_oracle.py --dataset homecoming --value-diff --triage-top 20
python3 diff_enh_oracle.py --dataset homecoming --value-diff --triage-json tools/mids-oracle/enh_oracle_triage.json
python3 diff_enh_oracle.py --dataset homecoming --value-diff --baseline-out tools/mids-oracle/enh_oracle_residual_baseline.json
python3 diff_enh_oracle.py --dataset homecoming --value-diff --baseline tools/mids-oracle/enh_oracle_residual_baseline.json --strict
python3 diff_enh_oracle.py --dataset homecoming --value-diff --strict
python3 test_read_enhdb.py
python3 test_diff_enh_oracle.py
python3 test_diff_harness.py

# DSH5 production harness (auto-emits the canonical export on first run):
python3 diff_harness.py                           # full HC sweep, write rules, cohort gate
python3 diff_harness.py --baseline oracle_divergence_rules.json  # + regression gate
python3 diff_harness.py --emit --top 30           # force-refresh canonical, show 30 classes
```

## Provenance (PROV-3, 2026-09-13)

Every artefact here that is derived from a `.mhd` states which one, through the single
`read_i12.provenance()` — `database`, `version`, `sha256`, `powerCount`. The sha256 is
the identifying field: the header reads "Mids Reborn Powers Database" in all four forks
and a version string names none of them either (MBDEXPORT-2). A path is not a citation,
and `oracle_divergence_rules.json` carried only a path until this row.

## Gate results (Gate 1 re-run 2026-09-13; 2 and 3 as of 2026-07-05)

- **Gate 1 (reader byte-correct):** PASS — parses all 11,002 HC powers / 73,431
  effects and lands exactly on `BEGIN:SUMMONS`.
- **Gate 2 (structural agreement):** the oracle corresponds to our parser export on
  known-answer powers (Single Shot, Flash Arrow, Poison Gas Arrow match *exactly*
  under canonicalization). Every divergence buckets into a modeling class below —
  **none is a Sidekick/parser defect.**
- **Gate 3 (reproduces the collapse):** the resistible/unresistable-twin cohort
  (Flash Arrow, Poison Gas Arrow — the powers the 2026-07-05 converter fix repaired)
  matches the oracle exactly. Mids independently shows Flash Arrow's resistible +
  `IgnoreResistance` ToHit twin; the pre-fix generated output (commit `d94431fe0d^`)
  carried **0** `unresistable` markers, the post-fix output **1** — the oracle
  corroborates precisely the effect the collapse had dropped.

## DSH9 bootstrap results (2026-07-06)

- `read_enhdb.py` parses local Homecoming `EnhDB.mhd` with end-of-file alignment OK.
  Current DB summary: 1,345 enhancements, 227 sets, 128 proc enhancements.
- `test_read_enhdb.py` passes against the local Homecoming DB.
- `diff_enh_oracle.py` reports meaningful coverage deltas:
  - set names are now 227 vs 227 with a small typo-only mismatch class
    (`Ascendency`/`Cacophany`/`Convalesence` vs normalized spelling)
  - proc tuple residuals are surfaced as a concrete DSH9 worklist for extractor
    reconciliation.

### `diff_enh_oracle.py` modes

- Default mode: identity coverage only (set names + proc tuples).
- `--value-diff`: enables value-aware residuals:
  - oracle set-bonus links (`EnhDB` -> `I12` linked powers) projected to
    planner-like `(stat,value)` and compared against `io-sets-raw` bonus tiers.
  - coarse oracle proc effect categories compared against structured categories in
    `proc-data.ts` (where present).
- `--strict`: exits non-zero when residuals exist. In `--value-diff` mode this
  includes value/category mismatches, not just missing identities.
- `--triage-top N`: prints highest-impact value mismatches by absolute delta,
  plus top missing/extra stat families.
- `--triage-json PATH`: writes summary + signatures + top triage buckets and the
  auto/explicit proc alias maps for artifact/CI consumption. Also includes a
  heuristic classification of repo-only proc rows into:
  - `likely_non_proc_global_or_passive`
  - `likely_mapping_gap`
  - `unknown`
- `--baseline-out PATH`: writes the current residual signatures (identity +
  value-aware buckets) to a JSON baseline.
- `--baseline PATH`: compares current signatures against baseline and reports only
  NEW residual signatures. With `--strict`, only new signatures fail the run.

### Proc name normalization

`diff_enh_oracle.py` now auto-normalizes proc name drift using a conservative rule:

- for a given set, if there is exactly one missing oracle proc name and exactly one
  extra repo proc name, they are treated as a name alias for identity comparison.

This reduces false identity residuals while keeping proc category comparisons on
raw exact-name intersections.

An additional explicit alias layer handles known edge cases not captured by the
auto pass (currently including Stupefy's "chance of knockback" wording drift).

## Divergence taxonomy → DSH5 canonicalizer worklist

Modeling-convention differences the diff surfaces (NOT defects); each becomes a
typed rule in DSH5's tiered classifier:

| class | what | reconciliation |
| --- | --- | --- |
| `PVP_MODELING` | Mids: base table + explicit PvE/PvP records. Ours: Any base + `_pvp*` override table. | fold `pv`→Combat + strip `pvp` token (done in `--normalize-pvp`) |
| `MULTI_TYPE_GRANULARITY` | Mids emits one record per damage type; our export keeps one multi-attrib record (e.g. Build Up: 7 `melee_buff_dmg` vs 1). | expand our multi-attrib templates one-per-attrib — **needs the DSH4 attrib→type bridge** |
| `MEZ_PVP_RESIDUAL` | our `_pvpmez` maps to Mids' `_ones`/`_special`, not `_mez`. | explicit table alias map |
| `INHERENT_EXTRA` | export carries `_inherentdamage` records Mids omits (the inherent bonus `damage.ts` deliberately filters). | skip inherent tables |
| `REDIRECT` | redirect shells (`effects: []` + `redirect[]`, e.g. Arachnos Burst→Crab/Wolf_Burst). Mids inlines the target; our export defers to the pointer. | resolve the redirect chain before diffing |
| `OTHER` | small residual: `_level` table ranged/melee-prefix differences + conditional-variant count imbalances in rich powers. | triage per-case in DSH5 |

## DSH5 harness — invariants, canonicalization, gate results (2026-07-05)

`diff_harness.py` sweeps **all 5,668 joined HC powers** (not a 14-power cohort) and
keys effects by the real DSH4 `(effectType, subType, resistible)` identity. The
build-out surfaced — and the harness now canonicalizes — the systematic *modeling*
differences that would otherwise masquerade as defects (each was verified against a
concrete power before being folded, never assumed):

| canonicalization | why | where |
| --- | --- | --- |
| **complete-type-set fold** | an all-damage/all-position effect: Mids collapses to one `damage_type=None` record; our export lists every type (the bridge splits per-attrib). Fold a complete set → one `All` on both sides. Twin (R/U) folded independently, stays distinct, and the damage and position sets fold independently too — one group can hold both. | Poison Gas Arrow (Mids 1 vs export 8) |
| **Enhancement fold** (PROV-4) | Mids spells a strength buff `EffectType.Enhancement` and names the enhanced attribute in a second field, `et_modifies`; our export spells the attribute itself, keeping `Enhancement` only for mez and defense strength (the two families with no effectType of their own). Read `et_modifies` and re-spell in the export's vocabulary. Where it routes is in the rules file under `enhancement_fold`. | Power Boost (SpeedRunning, SpeedFlying and Defense were one key) |
| **ResEffect fold** | Mids' catch-all for "resistance to a secondary-attribute debuff"; our bridge keeps the affected attrib at `aspect=Res` (the DSH4/DSH6 boundary). Bucket both → `ResEffect`. | Acid Arrow (−regen/−rec/−end…) |
| **set-not-multiset INV1** | Mids enumerates conditional/DoT/combo scale-tiers as separate records on the *same* key. Collapse drops a whole *distinct* sibling key — never duplicate copies — so compare distinct-key **presence**; count deltas on a shared key are advisory `MULTIPLICITY`. | Claw Swipe (Mids 28 vs export 6 Lethal) |
| **INV4 resistibility-flip** | a residual key whose base `(effectType\|subType)` exists on the other side with the opposite resistible bit — the twin-collapse axis. Split into its own `RESISTIBILITY_FLIP` class (kept in the worklist; not gated wholesale — Mids self-buff records carry an unresistable convention). | Power Surge (mez-protection U-twins) |

**Gate** (the teeth, false-positive-free): the known-answer cohort must match — Flash
Arrow + Poison Gas Arrow **twin-exact**, plus Single Shot / Acid Arrow / Build Up with
zero UNCLASSIFIED — and `--baseline` fails on any *new* UNCLASSIFIED signature. Both
green. Everything numeric/table-name against the ~5-week-stale, typo-carrying oracle is
**advisory only** (INV5 table-name agreement 97.0%; MULTIPLICITY 1,917) — gating on it
would be the one thing these guards must never do (a false positive).

**Result:** 2,576 / 5,668 powers carry a structural residual → `oracle_divergence_rules.json`
(schema `dsh5-oracle-divergence-rules/1`): BY_DESIGN 1,157 (redirect/inherent/pvp-only),
RELABEL 547 (type-granularity), **UNCLASSIFIED 6,245 = the DSH6 triage worklist.** The
UNCLASSIFIED bucket is genuine signal, not noise — spot-verified to be Mids semantic
relabeling (Enhancement = the `aspect=Str` boundary), content scope (incarnate/epic), the
289 resistibility-flips, and ~160 scattered "Mids has a damage type we lack" powers (e.g.
Mace Beam Blast Smashing+Energy vs our Energy — a concrete candidate finding for the bin
tiebreaker, DSH7). This is **local-only** (the `.mhd` is gitignored → not in CI yet, DSH7);
`.oracle_cache/` (the canonical dump) is gitignored, the distilled rules file is committable.

**Re-measured 2026-09-13** on the database PROV-3 re-pointed to, with PROV-4's Enhancement
fold in: matched 9,198 (`boosts/` joined in July), 2,333 carrying a structural residual,
**UNCLASSIFIED 5,679**, BY_DESIGN 1,158, RELABEL 404, INV5 89.6%, MULTIPLICITY 1,847. The
numbers above are the 2026-07-05 snapshot and are kept as the shape of the first sweep.
