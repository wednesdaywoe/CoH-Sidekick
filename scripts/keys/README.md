# Keys

A **key** turns a door-closing claim in a tracker row.

> **THE TRACKERS ARE GONE, AND THAT IS DELIBERATE.** The `REBUILD-PROGRESS` progress document
> and the whole issue-register system were removed ON PURPOSE when this repository was set up as
> an experiment. They were never in its git history, so there is nothing to recover and nothing
> to restore. Every row id you will meet — `TSPY-12`, `MBDIMPORT-17`, `ENT-22`, `PROD6C-3k`,
> `SOURCE-1`, `F74` and roughly 270 others, cited about 1,450 times across 246 files — is
> **prose context, not a pointer**. Do not go looking for the row, and do not read a dangling id
> as rot to be chased.
>
> What that does NOT change: the keys themselves still run, still read real artifacts and still
> pass or fail on what they find. A check is worth keeping when the claim it grades is still
> true of the code, whatever happened to the paperwork above it. Read each one's docstring for
> what it actually measures; the id in its first line is history.

The trackers gave every open row a `Check` when the
row made a claim that stops the next session looking — *"a filter is not the fix"*, *"nothing is
granted through this channel"*. Claims that merely describe current state need no key: if they are
wrong, the work surfaces it. Door-closing claims are the ones that rot silently, because nobody
goes there to find out.

Most keys are a one-line `grep` written inline in the row. A key lives here instead when a grep
cannot state the claim honestly:

- **`tspy11-filter-cost.py`** — TSPY-11's *"a `reaches_caster` filter is not the fix"*. The claim
  is about a predicate (`reaches_caster` reads `toWho` AND the power's `targetsAffected` AND the
  atom's gate), so no text search expresses it. **Exits 1 today** — the row says so.
- **`tspy12-independent-schedule-read.py`** — TSPY-12's *"71 is what Thunderspy ships"*. The
  claim is about a decode, so a grep cannot state it and neither can our own decoder: this is a
  SECOND reader for `schedules.bin`, sharing no path with `bin_crawler.parser._schedules`. It
  assumes no Parse7 header size, no field offsets, and no size-prefixed struct — it brute-force
  scans for level arrays and finds the unique chain of seven that tiles the file, which is the
  property that makes it a read rather than a replay. Grades our export on all four datasets, and
  grades the pigg hop against a loose `schedules.bin` from another install. Canonical-only: it
  reads game installs.
- **`mbdexport18-slot-level-drift-census.cjs`** and
  **`mbdexport19-variablevalue-census.cjs`** — MBDEXPORT-18's *"the writer re-derives every slot's
  level"* and -19's *"the slider is written as a literal 0"*. Neither claim can be checked by our
  own round trip, which is the point of both rows: the reader shares the writer's convention, so
  the two agree while both diverge from the file the build came from. Each censuses Mids' own
  corpus files against ours and exits non-zero if the population moves, so a count that falls
  because the CORPUS changed is not mistaken for the defect being fixed.
- **`shapeshift-suppression.py`** — the Kheldian residual's *"nothing is granted through this
  channel, so no wrong number ships"*. Needs a walk of the export's tagged `EffectGroup`s and their
  `chance`, which is a 746 MB tree.
- **`effects-bag-survivors.py`** — the `effects` writer-side row's *"the reader side is done, so
  what is left is an emitter edit plus a regen"*. Distinguishes the bag from the three other
  things spelled `effects` in the contract, separates converters that EMIT it from those that only
  compute a `guardBag`, and separates the retired typed `Bag` from the raw `extra["effects"]`
  object Rust still reads. **Exits 1 today** on three legs — the row says so. Its converter roles
  are an adjudicated table with a per-file evidence line rather than a regex classification: two
  successive regexes got the count wrong in both directions, because the binding name in
  `convert-powerset.cjs` is the generic `effects` and any pattern loose enough to catch a real
  merge also catches local builder writes and unrelated spreads.

**Four census tools stood here until 2026-09-25 and are gone; what follows is their numbers, and
the numbers are now the whole of the record.** They were not keys but the census tooling a closed
row was measured with, and they were kept on the argument that a later session auditing the same
question should re-run them rather than re-derive. Every one of them read `src/data/datasets/`,
which was deleted on 2026-09-25, so that argument stopped holding: re-running was no longer
possible, and a tool that cannot run is not tooling. They are recoverable from history
(`git log --diff-filter=D -- scripts/keys/`) but would need a new input tree to say anything.
Each measurement below is transcribed as it was reported, and none of it can now be re-derived in
this repo:

- **`perma2-ts-census.cjs`** — ran the real `isPermaEligible` over every power of every fork,
  walking the bundle in the order `PowerDatabase::from_gz_bytes` does so the rows joined
  positionally against the Rust census (`cargo run -p coh_math --release --features census-probe
  --example perma_eligibility_census`, which is still here and now has nothing to join against).
- **`perma2-candidate-probe.cjs`** — graded a candidate atom-native rule against that census and
  printed every divergence by fork. PERMA-2's port was chosen on its output: **14,488 entries, one
  named divergence**.
- **`mbdexport8-separator-census.cjs`** — MBDEXPORT-8's *"how many names differ from Mids' only in
  their separators?"*. The row would not let a join be widened on one observed name, and this is
  the count that decided it: **1 across all four datasets, 0 ambiguous**. It took the powerset
  pairing from the generator itself (`convert-mids-name-map.cjs --dataset X --pairs`) rather than
  re-deriving it — a second copy of those three conditions is free to drift from the one the
  writer actually uses, and the number is only worth having if it is the writer's population.
  Then it checked `MIDS_NAME_REVERSE_LOOSE` carried a row for each, and **exited 1 if one was
  missing**, because a population count alone cannot tell a fork with no such names apart from a
  generator that stopped emitting them. Mutation-checked: renaming the one row's key redded it.
  The advice was to re-run after any HC rename sweep, since the mechanism that minted one row will
  mint more; nothing does that now, so a later sweep goes unmeasured.
- **`mbdexport9-powerset-pairing-census.cjs`** — MBDEXPORT-9's roster. The pairing runs three
  passes (exact `group.set`, the set segment, then the roster), and this is what survived all
  three, with the reason each one is unreachable rather than merely unreached: **40 across four
  datasets — 10 whose Mids counterpart is already another powerset of ours, 18 whose nearest Mids
  set holds a different number of powers, 9 Mids has never carried, 2 Mids splits per archetype
  and 1 two of ours both hold whole**. Same input as its sibling above and for the same reason.
  It **exited 1** on a set of ours whose exact roster is held by one *unpaired* Mids set — pass
  three's own population, so finding one there meant the pass stopped reaching it.
  Mutation-checked: disabling the pass redded it with 150 named sets. Its population was every
  `group.set` the dataset layer carries, which is wider than the roster a Build can hold —
  the loader-side gate measures that one and pins it at 1 / 6 / 386 / 10, and is still live.

## Writing one

State what result **breaks** the claim, not just what confirms it, and exit non-zero on the break.
Print the population either way — a key that says only "ok" cannot be audited.

Three traps, all hit while cutting these keys (2026-08-28):

- **`grep -i` on a mixed-case token.** `grep -i 'tEXt'` matches every *"text"*. That returned 118
  hits for a 4-hit question and produced a wrong tracker row.
- **A `timeout`-ed search reads exactly like a confirmed absence.** A cut-off scan of
  `exported_powers/` returned empty and briefly "disproved" a claim that was true.
- **A `| head -n` search reads exactly like a complete one.** `grep -rn extractEffects scripts/*.cjs
  | head -20` cut the list at 20 of 25 lines, and the five it dropped were three of the emitters.
  That produced a stated "only one converter still emits the bag" — wrong, and wrong in the
  reassuring direction. The pager belongs on the display, never on the census.
- **A filter whose premise fails on the population it is filtering.** The same key skipped
  execution slots when listing what an atom-less power would lose, on the ground that those have
  a second address in `stats`. The one power it most needed to report — the `Domination`
  archetype inherent — has no `stats` object at all, so the filter hid `recharge: 200` behind
  the very assumption that power breaks. Test an exclusion against the rows it excludes.

## Canonical-only

Declared `canonicalOnly` in `../sync-manifest.json` until that file was deleted on 2026-09-25:
these read `contract/` and `exported_powers/`, which the beta does not carry in this layout. The register is
mirrored to the beta, so a beta reader sees the `Check` lines and cannot run them — the same
already-stated condition as the [`docs/gaps/`](../../docs/gaps/) narratives its rows point at.
That condition used to be enforced by `lint-register-shape.cjs` rule 6; the script was deleted on
2026-09-26, because `docs/DATA-GAP-REGISTER.md` and `docs/gaps/` have never existed in this
checkout, so it threw ENOENT at its own line 122 before reaching any rule. Both the register the
rows cite and the guard over it are absent here; both were removed on 2026-09-26, and the
measurement they recorded is transcribed in this file.
