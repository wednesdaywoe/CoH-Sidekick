# Architecture

How CoH Sidekick is put together: where things live in the repository, how game data becomes
the dataset the app loads, and how the app uses it. For building and running, see the
[README](../README.md).

---

## Repository layout

| Path | What it holds |
| --- | --- |
| `tools/bin-crawler/` | The parser. Python; reads a local install's `.pigg` archives and `.bin` files. |
| `exported_powers/` | The parser's output, committed. 77,139 JSON files, 694 MB, one per power. |
| `scripts/` | The converters, the audit gates and the contract emitter. Node, plus two Python steps. |
| `pipeline/` | Converter output, JSON. Gitignored — rebuilt, never committed. |
| `hand-data/` | Authored data nothing derives. Committed, read where it sits. |
| `refdata/` | Values extracted from the private Homecoming `.powers` defs, so a checkout without them (CI) regenerates the same data. Written by the regen whenever the defs are present. |
| `mids-tables/` | Two name tables emitted from an installed Mids Reborn by `tools/mids-oracle/`. Committed. |
| `contract/` | The dataset contract the Rust side consumes. 1,444 files, 154 MB, committed. |
| `crates/coh_data/` | The contract in Rust: wire decode, the power and character model, build file formats. |
| `crates/coh_math/` | Pure calculation. No UI, no IO. |
| `crates/coh_wasm/` | A `wasm-bindgen` boundary exposing the engine to JavaScript callers. |
| `crates/app/` | The planner UI. One Dioxus codebase for desktop and web; binary name `Sidekick`. |
| `vendor/dioxus-asset-resolver/` | Upstream 0.7.9 plus one patched function, marked `SIDEKICK PATCH`. |
| `fixtures/`, `baseline/` | Answer keys the Rust replay tests grade against; they move with each game patch. |
| `tools/*.py`, `scripts/audit-*.cjs`, `scripts/keys/` | Gates and census probes. |

---

## The data pipeline

```
a local City of Heroes install (.pigg archives)
  │
  ├─ tools/bin-crawler                  the parser (Python)
  │
  ├─ exported_powers/                   committed JSON, one file per power
  │
  ├─ scripts/ converters                orchestrated by regen-all.cjs, per dataset
  │     └─ pipeline/*.json              gitignored; the only thing a converter writes
  │
  ├─ hand-data/ · mids-tables/          committed, read in place
  │
  ├─ scripts/emit-contract.cjs
  │
  ├─ contract/<dataset>/bundle.json.gz  the runtime artifact
  └─ crates/app/assets/contract/        the same bundle, copied for the app to carry
```

**1. Parse.** `bin_crawler/assets_dir.py` resolves the install root, remembers it in
`~/.config/bin-crawler/config.json`, and falls back to a folder picker; `assets_sources.json`
records which root belongs to which fork on this machine. The decoders in
`bin_crawler/parser/` read powers, powersets, classes, boost sets, schedules, diminishing
returns, messages, recipes, salvage and entities. Only the 34 player-relevant categories of 204
are exported.

**2. Convert.** `node scripts/regen-all.cjs` rebuilds everything from the committed
`exported_powers/` tree — no `.pigg` archive and no game install needed. It runs two shared
producers once (the cross-fork proc table and the atom tuple field order), then a
dependency-ordered list of 23 steps per dataset, then a series of gates, then the contract
emitter. Each converter writes JSON into `pipeline/<dataset>/`.

**3. Emit.** `scripts/emit-contract.cjs` composes `pipeline/`, `hand-data/`, `mids-tables/` and
`exported_powers/` into `contract/`: a per-dataset `manifest.json`, one shard per powerset, a
dozen top-level section files, and `bundle.json.gz` — a single gzip of all of it, which is the
artifact the app loads. It also writes `contract/schema-version.json` and copies each bundle to
`crates/app/assets/contract/<dataset>/`.

**Determinism.** The converters embed no timestamps, `JSON.stringify` preserves source order, and
the gzip header's OS byte is pinned. A clean working tree before a regen must stay clean after it,
so `regen → emit → git diff --exit-code` is the guard that the committed contract matches what the
current converters produce.

### What each bundle carries

| Dataset | Powersets | Powers | Atoms |
| --- | --- | --- | --- |
| homecoming | 366 | 3,890 | 41,821 |
| rebirth | 307 | 3,299 | 34,131 |
| thunderspy | 307 | 3,289 | 32,664 |
| brainstorm | 375 | 3,971 | 44,386 |

---

## The dataset contract

A power owns its effects. Each is an `AtomicEffect` — an effect type, a sub-type, an aspect, an
attribute, a target, a scale, a stacking rule, a gate expression and so on — and every power
carries its own `Vec<AtomicEffect>`, never a pointer into a shared table.

On the wire an atom is a tuple, not an object. The field order is stated once in
`contract/schema-version.json`, which is embedded into both `crates/app` and `crates/coh_wasm` at
compile time and asserted at every load edge, so a contract whose tuple order drifted from the
decoder refuses to load instead of decoding cleanly.

Decode is strict in both directions. Every field is `Option`, because the wire trims trailing
nulls and absence must stay absence rather than become a default. Every discriminator is exhaustive:
an unrecognised enum string is a load error.

`crates/coh_data` is the only place the bundle is decoded.

---

## The crates

**`coh_data`** — the contract in Rust, plus the model everything else works against:
`PowerDatabase` (a loaded dataset), `CharacterState` (a build), `AtomicEffect`, archetype tables,
enhancement curves, the boost index, incarnate catalogues, slotting and pick rules. It also owns
every build file format the planner reads or writes.

**`coh_math`** — calculation only. The entry point is
`recalculate(&CharacterState, &PowerDatabase) -> CalculatedTotals`, which runs in passes:

1. Gather the active, mode-resolved power list.
2. Collect `+Strength` self-buffs.
3. Apply each active power's effects through the per-family appliers in `appliers/` — damage,
   defense, resistance, recharge, absorb, mez protection and resistance, stealth, movement,
   accuracy, to-hit, range, endurance discount, and the rest.
4. Add the additive archetype inherents (Vigilance, Fury, …) from the live combat context.
5. Add incarnate stat bonuses and the incarnate level shift.
6. Finalize: the purple-patch combat projection, then caps and combine-by-max into the
   `CharacterStats` the UI reads.

The same projection answers per-power questions, including for powers the build does not hold.
Set bonuses, procs, enhancement diversification, attack chains and the what-if team-buff layer all
live here.

**`coh_wasm`** — a thin `wasm-bindgen` shell for JavaScript callers. `load_dataset` parses a bundle
into a handle that keeps the `PowerDatabase` entirely on the Rust side; `recalculate` and
`project_power` take and return JSON strings. The core functions are target-independent, so they
can be tested natively without a wasm toolchain.

**`app`** — the UI. 100 Rust files, Dioxus components, with `desktop` and `web` as cargo features.

---

## How the app works

**State.** One `CharacterState` holds the build — archetype, powerset selections, picked powers,
slots, enhancements, accolades, incarnates, and the state of what is running (toggles, stances,
forms, proc switches). Edits go through it; `history.rs` gives undo and redo, bound to Ctrl/Cmd+Z
and Ctrl/Cmd+Y at the document level. Reading conditions — team size, current HP, enemy level,
exemplar level, content mode — live on a `CombatContext` alongside it.

**Layout.** Header (main menu, identity, build settings, level, undo/redo, tools, display), then
the quickbar of whatever the user pinned, then one of two layouts. Above 900px it is a free 2D grid
of absolutely positioned panels with collision-push and compaction; below 900px it is a flat
ordered list with a reorder overlay. Both persist, independently of each other and of the dataset.

**Panels.** Powers, powers by level, powerset select, pool and epic pickers, per-power info, stats
and user-defined stat dashboards, detailed totals, set bonus totals and finder, slotted procs and
proc settings, combat and build settings, adjusters, accolade and incarnate pickers, incarnate
crafting, the attack-chain builder, and the what-if team-buff simulator. Panels can be popped out
into their own desktop window.

**Persistence.** `localStorage` on both targets, with every write funnelled through `storage.rs`
so only the window that owns persistence performs one.

**Cloud (optional).** `cloud/` speaks to a Supabase backend over four wire shapes: edge functions,
PostgREST on the `favorites` table, RPC, and the auth endpoints. On top of them: a browser for
public shared builds with search and filters, saving and short-linking a build, favourites that
work signed out and mirror to the account when signed in, author profiles and avatars, and auction
price lookups. No code under `cloud/` names a target — `reqwest` is `fetch` on web and
hyper+rustls on desktop — and desktop sign-in completes through a loopback listener with a PKCE
challenge.

**Files in and out.**

| Format | Direction | Notes |
| --- | --- | --- |
| `.skif` v5 | read + write | The native build file. Structural errors are refused; unknown vocabulary (a newer dataset's ids) is preserved and reported, never dropped or applied. Legacy v2–v4 also read. |
| `.mbd` | read | Mids Reborn's current file. Plain JSON; the work is name and UID resolution. |
| `.mxd` | read | Mids' legacy file — a forum post plus a zlib `\|MxDz;…\|` binary block. Both halves are read together. |
| `/buildsave` | read | The game's own in-client export. Enhancement records are resolved through the boost index, never parsed out of the name. |
| PNG | write | The build as a poster, drawn to a pixel buffer in Rust with no DOM or rasterization step. |
| Forum post | write | Mids-style plain text for a thread, subreddit or Discord message. |

A file authored against another fork is detected before it is decoded: the planner parks the text,
asks whether to switch forks or read it against the loaded one, and waits for the target dataset's
restore to finish before adopting it.

**Crash reporting.** A panic hook delivers the report before the process goes down — blocking
`reqwest` on a dedicated thread on desktop, `navigator.sendBeacon` on web.
