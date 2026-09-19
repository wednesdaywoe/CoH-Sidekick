---
project: coh-sidekick
kind: plan
title: Homecoming Parser
relates:
  - REBIRTH_PARSER.md
  - THUNDERSPY_PARSER.md
  - THUNDERSPY_SUPPORT_PROGRESS.md
---

# Homecoming Parser

> **Canonical (`coh-sidekick-1.0`) is authoritative for this log**, at
> `docs/streams/HOMECOMING_PARSER.md`. CoH-Sidekick (beta) carries a synced copy at
> `streams/HOMECOMING_PARSER.md`, because 15 beta-owned files cite it and a beta-only clone must
> still resolve them. The flow is one-way, the same as `tools/bin-crawler`: edit it in canonical,
> then run beta's `scripts/sync-bin-crawler.sh`. The file is mirrored byte-for-byte, which is why
> this header names the repos instead of saying "this one".
>
> Paths below are written **relative to the repo root**, so they read as paths rather than
> resolving as markdown links from the directory this file sits in. Leave them that way — the two
> copies sit at different depths, so any `../` prefix correct in one repo is wrong in the other.
> 59 of the 68 link targets exist in both repos from the root.
>
> These entries predate canonical's `docs/DATA-GAP-REGISTER.md`, which starts 2026-07-16 and does
> not backfill. For anything before that date this log is the only record — `cast_through` and
> `toggle_ignore` appear nowhere else in canonical. New findings belong in the register, with its
> ids and severities; this log is closed history, kept because the code still points at it.

Running log of bugs and gaps in the binary parser → JSON conversion pipeline
(`tools/bin-crawler/` + `scripts/convert-powerset.cjs` + `scripts/convert-epic-pools.cjs`), with diagnoses and recommended fixes. Newest entries at top as `- [ ]` items under `## Active`. When completed, move the entry into `## Resolved` (checkbox `- [x]`) with the fix details.

> **Before you make any edits** Be sure to read `GAME-DATA-PRINCIPLES.md`

> **Rebirth-specific gaps** are indexed in [REBIRTH_PARSER.md](REBIRTH_PARSER.md)
> (the front door for the Rebirth dataset — most trace to one root: Parse6 drops AttribMod
> tail/condition fields that Parse7 decodes). Rebirth findings still get their detailed
> write-up *here*; that doc links back to them.

> **Thunderspy-specific gaps** are logged in [THUNDERSPY_PARSER.md](THUNDERSPY_PARSER.md)
> (Parse7 framing + Parse6-derived 50-level schema; home of the custom **Primalist** AT).
> Resolved there: `Class_Primalist` empty-categories (`.texture` icon anchor) + export
> filter; class `attribs` (HP/caps/threat/dmg-cap) via a new Parse7+50-level layout. No
> Thunderspy issues currently open.

>Agent note: One small flag for your separate task: when you pick up the parser work, the verification tooling I built is at c:\tmp\ (oracle-verify.mjs, override-audit.mjs, etc.) — handy if you end up retiring the Discharge overrides after the parser fix, to confirm the audit goes 9 → 7.

## Active

*(Sibling game-data finding from the I28P3 audit — NOT a parser issue, logged here only for
cross-reference: Sentinel `Blinding_Powder` Confuse **scale is 10**, but the patch notes
and the Scrapper/Stalker versions say **12**. The parser reads it correctly — Scr/Stk parse
as 12 — so the binary genuinely contains 10 on the Sentinel record. Flagged to the dev team;
data pushed as-is.)*

- [~] **Resolve `Set_Mode` indices → mode stat-multiplier definitions** — RE complete;
  mode-**name** resolution SHIPPED (HC); the Domination ×2/×1.5 is confirmed engine-side
  (hardcode stays). Deferred: the attrib-118 mag=1 misdecode fix + Rebirth/Thunderspy tables.

The parser captures `Set_Mode` templates but leaves their magnitude values (mode
indices, e.g. Domination's 45/46) **opaque** — the "what does mode N actually do"
mapping is never extracted (see the note at
[_powers.py](tools/bin-crawler/bin_crawler/parser/_powers.py) ~L1254:
`Set_Mode/Rage/Taunt/Create_Entity` values index into a table we don't resolve).

**Motivating case: Dominator Domination's ×2 mez magnitude / ×1.5 duration.**
Investigated exhaustively (2026-07-04) and CONFIRMED these multipliers are **not in
the exported power data**: not in any of the six `domination_*.json` sub-powers (they
are a `Rage`-meter system + `Set_Mode` + mez/KB protection + endurance/ToHit — no mez
magnitude/duration buff), not on any of the 118 Dominator control powers (no
mode-conditional template, no redirect field), and not reachable via the redirects
category. Positive proof the mechanism *could* be data-driven but isn't: "+mez
magnitude" IS representable — Power Boost encodes it as a `Strength`-aspect buff on the
mez attribs (`Held/Stunned/… scale 0.66`) — yet Domination carries no such template.
So the ×2/×1.5 are applied by the engine's **mode 45/46 handler**, whose multipliers
live in the mode/attrib-mod definitions the engine keys off `Set_Mode`, outside the
player-power JSON. Currently hardcoded as documented constants in
[getDominationInfo()](src/utils/calculations/inherents.ts) (`magnitudeMultiplier: 2.0`,
`durationMultiplier: 1.5`) with a provenance note; `activeDuration`/`rechargeTime` are
already data-backed from the inherent metadata.

**Task.** Investigate whether the mode definitions (index → attribute strength/duration
modifiers) exist as parseable data in the bins (a modes/attribmod table distinct from
`powers.bin`) and, if so, resolve `Set_Mode` index values into their stat effects so
the Domination boost (and any other mode-gated mechanic — Bio Armor adaptations,
Dual-Pistols ammo, Wind Control weather modes) can be derived rather than hardcoded.
**Genuinely uncertain to pay off** — the multipliers may be hardcoded in the game
client's C code rather than stored as data, in which case there is nothing to extract.
Scope this as exploratory RE, not a guaranteed win. Cross-ref: the
`ModesDisallowed` (3,475 powers) clean-field capture in Next steps is a related but
distinct mode surface (which powers are *disabled* in a mode, not what a mode *does*).

**RE COMPLETE (2026-07-04) — findings, all oracle-verified.** The investigation
resolved the mechanism end-to-end. Two questions, two answers:

*(1) Do mode NAME definitions exist as parseable data? **YES.*** The global mode-name
registry lives in **`attrib_names.bin`** (which the parser currently **does not read at
all** — `ATTRIB_NAME` is a hardcoded dict). It is the `ppchMode` sub-array of the
`AttribNames` struct: a table of 16-byte records `{u4 flag=0, u4 type=12, str name,
str display}` — 213 modes on the current HC bin. A `Set_Mode` template's **`magnitude`
field *is* the 1-based mode index**: `mode = modeTable[magnitude − 1]`. Confirmed against
the HC `.powers` oracle, where `kSetMode`'s `Magnitude` field is literally the mode
token (`Magnitude kRestedAdaptation`, `Magnitude kGranite_Mode`, …). **Broad oracle
validation: 183 MATCH / 1 MISMATCH** over all HC `Set_Mode` templates with mag≥2 — the
lone "mismatch" is Shifting_Tides (Seismic Blast) mag186 → `ShiftingTides`, correct in
the current bin but postdating the 4,943-power oracle snapshot (stale oracle, not a
mapping error). Sample anchors: Domination mags 45/46 → `Domination` + `Domination_Active`;
Bio Efficient/Defensive/Offensive → `RestedAdaptation`(153)/`DefensiveAdaptation`(154)/
`OffensiveAdaptation`(155) — note **there is no `EfficientAdaptation` mode; Efficient sets
`RestedAdaptation`**, verified in the oracle, not a bug; DP ammo 89–92 →
`Lethal/Ice/Fire/ToxicAmmo`; Granite → `Granite_Mode`(6) + `Suppress_{Fly,Run,Jump}Toggles`
(35/36/37). Mode indices are **per-server** (each server's `attrib_names.bin` reorders
them); HC is Parse7. Rebirth (Parse6) / Thunderspy need their own read — Rebirth's pigg
was not mounted at investigation time.

*(2) Do mode STAT EFFECTS (the "what does mode N do" multipliers) exist as data? **NO —
modes are pure flags.*** A mode carries only `name` + `display` — no magnitude/duration
modifiers. But the effects that happen "in a mode" ARE data-driven, via one of three
mechanisms: (a) separate `requires`-gated AttribMods on other powers (Bio/DP/etc.), (b)
**`Tag`-gated nested effect groups baked into the affected powers** — the engine's
universal AT-mechanic system — or (c) engine C-code.

**CORRECTION (2026-07-04) — Domination is mechanism (b), NOT C-code. The earlier
"C-code / hardcode is correct" conclusion below was WRONG.** It was reached by checking
whether the control `.powers` reference `kDomination` / the `Domination_Active` mode / a
magnitude expression — they don't — but it never checked the **`Tag` field** on effect
groups, which is where the boost actually lives. Every Dominator control power carries a
**nested `Effect { Tag "Domination" … }`** group that duplicates the control with extra
mag/duration; the engine enables `Tag "Domination"` groups while Domination is active.
Concretely, `Dominator_Control.Mind_Control.Dominate` has a base `kHeld mag 3.0 scale 12`
group **plus** a child `Tag "Domination"` group `kHeld mag 3.0 scale 18` — they stack, so
under Domination the target eats mag 6 (×2) and the added piece runs scale-18 vs scale-12
(×1.5). Verified against the live measurement (Dazzle 3.0/17.88s → +3.0/26.82s; 26.82/17.88
= 1.500). Scope: **205 `Domination`-tagged groups across 106 Dominator powers**; the values
are **already in the export** (`export_powers.py` emits `tags`, and `dominate.json` carries
the tag). The pattern is *mostly* ×1.5 duration / ×2 mag but **not uniform** — Cryo Freeze
Ray is ×1.8, PvP variants add only +1 mag, and some powers tag a *different* mez than their
base — so the flat `getDominationInfo()` 2.0/1.5 hardcode is an approximation, not exact.

This is the same `Tag` mechanism behind every AT gimmick (tag histogram, HC 2026-06):
`ScrapperCrit_ST` 387 · `SentCrit` 410 · `StealthCrit` 277 · `Containment` 264 ·
`Overpower` 215 (Controller bonus-mag) · `FieryEmbrace` 605 · `Defiance` 128 ·
`Domination` 205. Lesson for future "where does mode/AT effect N come from" hunts: **check
`EffectGroup.tags` before concluding C-code** — a search over `requires`/`.Mode?`/attrib
names will miss tag-gated effects entirely.

**SHIPPED (2026-07-04) — per-effect Domination wiring.** Replaced the blanket ×2/×1.5
(gated on `category === 'control'`) with the real per-power data. Converter
([convert-powerset.cjs](scripts/convert-powerset.cjs)) stamps `Tag "Domination"` templates
(`_tagDomination` in `collectTemplatesDeep`) and routes them to a `MezEffect.domination`
`{mag,scale,table}` sub-field (PvE only) instead of letting the base one-per-type mez
discard them; type in [power.ts](src/types/power.ts); both Dominator categories regenerated
(**80 powers** gained the field — clean diff, only additive). Display
([SharedPowerComponents.tsx](src/components/info/SharedPowerComponents.tsx)) now shows
`mag = base + domination.mag` and duration from `domination.scale` when Domination is active,
per-power — so it boosts tagged **assault** mez the category gate missed (Total Focus Stun),
honours outliers (Cryo Freeze Ray ×1.8 dur), and leaves untagged control effects alone
(**0** epic/patron holds carry the tag — a Widow's FRT Dominate is not boosted). Guard:
[domination-per-effect.test.ts](src/data/domination-per-effect.test.ts) (4 cases incl. the
untagged-not-boosted regression). `calculateDominationMagnitude`/`Duration` are now unused by
the display; the summary badge still shows getDominationInfo's typical ×2/×1.5. Verified:
`tsc` clean, 761 tests. Committed export already carried the tags, so this needed no
attrib-118-style re-export — only the Dominator powerset regen.

<details><summary>Original (WRONG) conclusion, kept for history</summary>

> **Domination's ×2/×1.5 is the C-code case, now confirmed at the oracle level**: `Dominate`
> is a flat `kHeld Mag 3.0` PvE / `4.0` PvP with no Domination-mode gate and no magnitude
> expression, and zero Dominator control `.powers` reference `kDomination`/`Domination_Active`,
> so the `getDominationInfo()` hardcode is correct and should stay. — This missed the `Tag`
> field; see the correction above.

</details>

**BONUS BUG surfaced — attrib-index-118 collision (separate from modes).** ~957 of 2,402
`Set_Mode` templates carry **mag=1**, and mag=1 → mode #1 = `Peacebringer_Blaster_Mode`,
which **0** real player powers set (oracle). These are **misdecodes**: attrib index 118 is
a shared "special" slot (CLAUDE.md's own note on 118: "CoD2 also: Set_Costume,
Debt_Protection"), so `kXPDebtProtection` / `kSetCostume` decode to `Set_Mode` with a
placeholder mag=1. Proof: `Empathy/Resurrect` exports `attribs:['Set_Mode'] mag=1` in the
exact slot the oracle has `kXPDebtProtection` (its `kHeal`/`kEndurance`/`kStealthRadius`
neighbours decode fine). Net effect: XP-debt-protection is currently invisible/mislabeled
on ~190 HC player powers, and any consumer treating mag=1 `Set_Mode` as a real mode gets
a phantom "Bright Nova". Root fix = disambiguate index 118 via the main `attrib_names.bin`
attrib array (out of scope for mode resolution; the practical mode-feature guard is
**mag≥2**).

**SHIPPED (2026-07-04) — mode-NAME resolution, HC.** New parser
[_attrib_names.py](tools/bin-crawler/bin_crawler/parser/_attrib_names.py)
(`parse_mode_table`) reads the `ppchMode` array (the parser's first-ever read of
`attrib_names.bin`); the exporter resolves a `Set_Mode` template's `magnitude` to a
`mode_name` (guard **mag≥2**; the magnitude is a **direct index** — `ServerTrayOverride`
is index 0, so `Domination`=45, `Domination_Active`=46, `RestedAdaptation`=153).
Wired into [export_powers.py](tools/bin-crawler/bin_crawler/export_powers.py)
(`power_to_dict(..., mode_table=…)`). **De-risked per §6:** a with-modes vs no-modes
re-export from the same bin differed in **exactly** the added `mode_name` keys (0 non-mode
drift). Focused apply to committed HC data → **394 files, +463 `mode_name` keys, purely
additive** (each key adds one trailing-comma flip; no content change). Guard:
[set-mode-resolution.test.ts](src/data/set-mode-resolution.test.ts) (5 cases incl. the
mag==1-not-resolved regression). `tsc` clean, 751 tests. Downstream consumers ignore the
new key (converter reads specific fields); wiring the converter to *use* `mode_name` (match
Bio/DP conditional effects by name; resolve `modes_required`/`modes_disallowed`) is
follow-on, not part of this slice.

**SHIPPED (2026-07-04) — power-level mode gates, HC** (closes the `ModesDisallowed`
Next-step). The `.def` **ModesRequired / ModesDisallowed / ModesSuspended** u4-arrays
(powers.bin fields 76–78, previously read-and-discarded) are now captured on
`PowerRecord` (both the Parse7/HC and Parse6/Rebirth+Thunderspy paths) and resolved to
names at export via the **same `mode_table`** the `Set_Mode` magnitudes use — verified the
index base is identical: **all 79 distinct indices across 26,313 HC powers resolve with 0
unresolved**, and the reference `domination.json` cross-checks exactly (`Domination`
inherent → `modes_required: ["Domination"]`, index 45). Unlike Set_Mode magnitudes these
are dedicated arrays with **no attrib-118 collision, so every index resolves incl. mode 1**
(no mag≥2 gate). Semantics confirmed: Titan `FastMode`/Momentum, DP `LethalAmmo` swap gate,
Granite auto-suppress of Stone toggles, Kheldian/Primalist form gates, Domination meter
plumbing. Emitted **faithfully and sparse** — including the engine's generic system gates
(`Disable_All` on ~5,300 attacks, `Disable_Epic`/`Incarnate`/`Pool`/`Alpha`, Kheldian
`*_Mode` disallows); no editorializing prefix-filter (user decision 2026-07-04).
**De-risked per §6:** a structural HEAD-vs-working deep-diff over all 5,916 changed files
proved the cumulative diff is **exactly** the added top-level `modes_*` keys (5,837 files)
plus the prior slice's template `mode_name` keys (463) — **0 other value changes**; the
unrelated pre-existing `magnitude_expression` drift in a fresh re-export (516 files, snipe
ToHit / foresight HP scaling — committed data is stale on that field) was **deliberately
not swept in** (surgical apply kept committed values). Guard:
[set-mode-resolution.test.ts](src/data/set-mode-resolution.test.ts) +6 cases (11 total);
`tsc` clean; `audit_stack_alignment` parses all 26,313 powers (byte-identical consumption —
the change only captures return values of reads already made).

**SHIPPED (2026-07-04) — attrib-118 misdecode ROOT FIX (whole special region).** The
deferred "disambiguate index 118 via `attrib_names.bin`" theory was **wrong** —
`attrib_names.bin` holds only categorised sub-arrays (Damage/Defense/Boost/Group/Mode),
no per-attrib master list, and (verified from raw bytes) index 118 is **not** a real
collision to break: `kXPDebtProtection`, `kSetMode`, `kSetCostume` are stored as **three
distinct raw u4 values 472/473/474** and the parser's `v // 4` was **truncating the low 2
bits**. The whole "special/meta-scripting" region (collapsed indices **117–128**, raw
468–515) is **byte-granular sub-indexed** — `raw % 4` is a real sub-index — so `// 4`
merged 2–4 unrelated engine attribs into each index. *This is the actual cause of the
CLAUDE.md "these indices are multi-purpose / CoD2 relabels by context" note: CoD2 never
needed context, we discarded the discriminating bits.* Fix: new `resolve_attrib()` +
`SPECIAL_ATTRIB_BY_RAW` in [_enums.py](tools/bin-crawler/bin_crawler/parser/_enums.py)
resolve the special window by raw value before the `// 4` lookup; wired at
[_powers.py](tools/bin-crawler/bin_crawler/parser/_powers.py) `_parse_effect_template`
(HC/Parse7 + Veracity path). **Authoritative map cross-referenced against the `.powers`
oracle (1,905 matched powers):** 468 Translucency · 469 Create_Entity · 470 Clear_Damagers ·
471 Silent_Kill · **472 XPDebtProtection · 473 Set_Mode · 474 Set_Costume** · 476 Null ·
477 Avoid · 478 Reward · 480 Drop_Toggles · 481 Grant_Power · **482 Revoke_Power** (the
*opposite* of 481, formerly both `Grant_Power`) · 483 Unset_Mode · 484 Global_Chance_Mod ·
486 Grant_Boosted_Power · 487 View_Attributes · 489 Reward_Source_Team · 491 Combat_Phase ·
492 Combat_Mod_Shift · 493 Recharge_Power · 494 Vision_Phase · 495 Ninja_Run · 498
Steam_Jump · 499 Designer_Status · 500 Exclusive_Vision_Phase · 502 Set_Script_Value · 503
Add_Behavior · 505 Token_Add · 506 Token_Set · 508 Script_Notify · 509 Force_Move · 511
Cancel_Mods (was mislabeled `Cancel_Effects`) · 512 Execute_Power. Five rare raw values
(475/479/485/490/501, all count≤10 NPC/prestige/proc) resolve to honest `Special(<raw>)`
rather than borrow a sibling's name. **Verified against fresh binary (26,313 powers, no
parse errors):** old collapsed `Set_Mode` 2306 → **XPDebtProtection 251 + Set_Mode 1365 +
Set_Costume 689** (+1 Special); old `Grant_Power` → **2413 grant + 1910 Revoke_Power**;
Empathy/Resurrect now `['XPDebtProtection']`, Granite RockSuit now `['Set_Costume']` (no
phantom mode), and **Bright Nova mag=1 → `Peacebringer_Blaster_Mode`** — the *one* genuine
player mode-1 set (the oracle's 4,943-power sample omits Peacebringer_Offensive, but the
binary has it). Because `Set_Mode` is now **only** raw 473, the mag==1 impostors are gone,
so the exporter's `mag≥2` mode-resolution guard was relaxed to **`≥1`**
([export_powers.py](tools/bin-crawler/bin_crawler/export_powers.py); skips only mode 0
`ServerTrayOverride`), `_extract_params`' index-117 collision guard is now moot (comment
updated), and `_POWER_PARAM_ATTRIBS` picked up `revoke_power`/`cancel_mods`. Guard:
[test_special_attribs.py](tools/bin-crawler/tests/test_special_attribs.py) (7 pure-function
cases — no bin needed). `tsc` clean; the 11-case
[set-mode-resolution.test.ts](src/data/set-mode-resolution.test.ts) still green (its comments
updated to flag the mag==1 case as pre-regen data that flips on re-export).

**Deferred (regen, not re-swept per user's defer-regen decision 2026-07-04):** the committed
`exported_powers/` + `src/data` still carry the pre-fix labels. A re-export (de-risk per §6)
will relabel ~hundreds of player files (Set_Mode 692 / Grant_Power 1316 / Create_Entity 1383
files touched, only the mislabeled sub-slots changing) and **should be paired with deleting
the downstream collision workarounds this fix obviates** —
[convert-pet-entities.cjs](scripts/convert-pet-entities.cjs) ("Silent_Kill labeled
Create_Entity"), [convert-incarnate-effects.cjs](scripts/convert-incarnate-effects.cjs)
("index 123 collapses Recharge/Vision/Ninja"), and the `extract-proc-data.py` special-attrib
exclusion lists — plus verifying converters that key on `Grant_Power` don't silently swallow
the newly-split `Revoke_Power`. Same batch as the `magnitude_expression` re-export ((4) below).

**Deferred follow-ups.** (1) *DONE — see the SHIPPED root-fix above.* (2) **Rebirth (Parse6) /
Thunderspy** mode tables — the parser now *captures* the mode-gate arrays on those paths,
but resolution still needs their own `attrib_names.bin` mode table (Rebirth's pigg ships
one; the Parse7 `parse_mode_table` heuristic may need a Parse6 variant). (3) Converter
consumption of the new `mode_name` + `modes_*` fields (match Bio/DP conditional effects by
name; gate powers by caster mode). (4) The **`magnitude_expression` staleness** surfaced by
this slice's re-export — committed HC data lacks snipe/foresight scaling expressions a fresh
export now produces; a full re-export is due (separate, user-reviewed change).

- [~] `.powers` extraction-completeness audit

Decision (2026-06-04): stop omitting *mechanically-relevant* data for file size. The
naïve early choice to skip "build-irrelevant" fields keeps surprising us (knockback,
foe -KB, brute mods, Kheldian effects). New rule: **capture everything that affects what
a power does; skip only asset references** (`VisualFX`/`.PFX` paths, animation `include`s,
combat-text message IDs, icon internals).

Oracle: the HC dev's authoritative `.powers` source defs (`raw defs/`, 4,943 powers,
same category structure, gitignored). Confirmed OK to use (public game data, anon source).

**Framework built + one-time sweep done (2026-06-04):** `tools/extraction-audit/` —
`parse_powers.py` (parses the brace-nested `.powers` format) + `audit.py` (Phase-1
comparator: `.powers` vs `exported_powers`). Sweep covered 4,943 raw defs — **3,686
audited**, 1,257 skipped (no export; NPC/Temporary_Powers categories we don't extract).
Report snapshot: `tools/extraction-audit/gap-report.json`.

### Knockback + Knockup protection erroneously rolled into the same value
**STATUS: Resolved**
Root cause
In CoH, knockback protection is a single physical stat that always applies equally to Knockback and Knockup. The binary stores it as one template with attribs: ["Knockup", "Knockback"], and the converter faithfully splits that into two effect fields. But the calculation layer then summed both into the same accumulator:

In `character-totals.ts`, the `mezProtTypes` loop ran `global.protKnockback` += mag for both knockback and knockup. The author's own comment ("Both feed the same protKnockback stat") shows the pairing was known — but += on both doubles it.

The systemic sweep (12 agents, adversarially verified) found the KB doubling was the sole confirmed add/duplicate flaw:

The converter's += Math.abs(scale) accumulation is correct — it sums the aspect-paired Resistance(+5)/Current(−15) templates, which is the intended KB-protection encoding (that's where the 20 comes from).
Display/export layers read protKnockback once — no downstream re-summing.

The fix: Both KB paths now fold the Knockback/Knockup pair into one per-power contribution via `Math.max` (they're always equal), added once — while different powers still stack (Unyielding + Bo Ryaku + Acrobatics all add up). I also hardened the parallel `effects.protection` path (latent — no data reaches it today, but same shape). The converter stays untouched: emitting both fields is a faithful representation of the binary; aggregation semantics is where the fix belongs, and this avoids regenerating ~197 data files.



### Sweep findings — Phase 1 (parser/export gaps)

**Power-level fields genuinely absent from our export (clean signal — distinct names,
no normalization ambiguity). Several are mechanically relevant:**

| `.powers` field | # powers | relevance |
|---|---|---|
| `ModesDisallowed` | 3,475 | power disabled in certain modes (mez'd/phased/etc.) |
| `TimeToRoot` | 2,340 | animation root/lock time — affects DPS & rotation |
| `StrengthsDisallowed` | 951 | which enhancement aspects can't affect the power |
| `BuyRequires` | 631 | power prerequisites — build legality |
| `ToggleIgnoreHold/Sleep/Stun` | 524 | **toggle persists through mez** |
| `IgnoreStrength` | 438 | effect unaffected by enh strength — **currently sourced from CoD2**; capturing natively closes that dependency |
| `CastThroughHold/Sleep/Stun/Terrorize` | ~48+ | **power usable while mez'd** |

**Attrib comparison is NOT yet trustworthy** — its top entries are *normalization
artifacts*, not gaps. Verified: `.powers` `kDefense` = our `Base_Defense`,
`kSpeedFlying/Running/Jumping` = `FlyingSpeed`/`RunningSpeed`/`JumpingSpeed`,
`kEntCreate` = `Create_Entity`, the damage types = `*_Dmg`. `audit.py`'s `norm_attrib`
needs a real `.powers`↔export attrib name-map before its numbers mean anything. The
*real* attrib gaps underneath match the known unmapped-exotic class (CLAUDE.md ~7%):
`*_Elusivity` (`*elude`), `revoke_power`, `grant_power`/`grant_boosted_power`,
`silent_kill`, `cancel_mods`, `set_costume`, `jump_pack`, `xp_debt_protection`,
`null_bool`. (Note: Energy Torrent is faithful at the effect-group level — 5 vs 5,
not the earlier "6 vs 5", which was an `rg` false match on `EffectArea`.)

### Progress
- [x] **`cast_through` (Blaster Defiance — "Cast While Mez'd")** surfaced
  (`418f3ec82`). Was already in the export, just unused — a Phase-2 captured-but-
  unused gap. Converter→model→display, 177 powers. No parser/re-export needed.
- [x] **`toggle_ignore` ("Stays On While Mez'd")** captured (`542c9baea`). DID need
  parser work: `_parse_cast_flags` read `cast_through` then skipped the next 6
  bools — now reads the 3 `toggle_ignore_*` ones (alignment preserved). Re-exported
  both datasets (de-risked: `toggle_ignore`-only, no drift; sole incidental change
  was Rest gaining `allowed_set_categories ['Rest Buff']`). 784 powers. This proved
  the full parser→export→converter→model→display path and the re-export workflow.
- [x] **`IgnoreStrength` — CONFIRMED CALC GAP (not "moot" — that earlier call was
  wrong).** [Settled 2026-08-13; see the Next-steps closure note below for the leftover that was later closed.] The data is captured (`template.flags`), so nothing is lost; the gap is
  that the **converter only honors `IgnoreStrength` for regeneration** (the
  `resType === 'regeneration'` branch → `regenBuffUnenhanced`/`effects.n`). Every
  other real-stat effect that ignores strength is mishandled. After filtering the
  traps (the `aspect=Strength, scale=0` damage **meta-template**; `aspect=Resistance`
  -Res/debuff-resistance templates that merely use `*_Dmg`/`Base_Defense` attribs;
  procs; pets), **288 player-powerset effects** remain with a genuine
  `aspect=Current/Absolute` `IgnoreStrength`: Endurance 120, Recovery 90, ToHit 50,
  Base_Defense 10, RechargeTime 8, Heal 7, Absorb 3. Two failure modes:
  - **Over-enhance** (main `effects`): emitted as a normal enhanceable effect.
    *Confirmed:* Bio Armor **Environmental Adaptation**'s +ToHit (0.75, `IgnoreStrength`
    in game) → generated `tohitBuff: 0.75`, and `tohitBuff` has
    `enhancementAspect: 'tohit'`, so the calc boosts it with ToHit IOs / global +ToHit.
  - **Dropped entirely** (`activation_effects` toggles/autos): the converter drops
    non-regen `IgnoreStrength` templates there (`isDropForActivationEffects`) — the
    very "missing data" pattern we keep getting bitten by.
  **Fix:** generalize the unenhanceable handling beyond regen — route the effect to a
  `…Unenhanced` key and add it to its global total WITHOUT the enhancement multiplier.
  Calc-affecting; needs care (the meta-template / resistance traps above show how a
  naïve `flags.includes('IgnoreStrength')` over-fires). Scope with the
  `aspect ∈ {Current,Absolute,Magnitude}` + non-proc discriminator validated here.
  The discipline: the data was never the problem — our *use* of it was.

  **Status (2026-06-04):**
  - [x] **Recovery + ToHit FIXED** (`940d89dbb`): `recoveryBuffUnenhanced` /
    `tohitBuffUnenhanced` keys; 65 powers reclassified; confirmed Env Adaptation +ToHit
    no longer enhanced. tsc + 84 tests.
  - [~] **Refinement** — the original "288" over-counted on two fronts:
    - `recharge`, `absorb`, `endurance` (`enduranceGain`) carry `IgnoreStrength` but
      are **not enhanced in the calc** (a +recharge buff isn't boosted by Recharge IOs,
      etc.) → **no over-enhance bug**; correctly left as-is.
    - [x] **Defense is NOT a bug either** (verified by inspecting all 10): every
      `Base_Defense` `IgnoreStrength` case is a **debuff or self-penalty**, not a buff
      — Eye of the Storm is a foe `-Def` (`Melee_Debuff_Def` table); Rage (−0.2), Rest
      (−1000), Vulnerability (−0.1125) are negative-scale self-crashes already routed
      to `defenseDebuff`. No genuine defense **buff** carries `IgnoreStrength`. The
      earlier "Defense (10)" count was debuffs caught by a too-loose filter.
  - [x] **Heal FIXED** (single-portion heals). Heal flows `extractDamage` → a
    `type:'Heal'` damage entry → re-extracted into a `healing` effect, which was
    duplicated in three places. Consolidated into one helper
    (`src/utils/calculations/healing.ts` `extractHealingFromDamage`, replacing the
    copies in `InfoPanel.tsx`, `PowerInfoTooltip.tsx`, `CompareSlottingModal.tsx`),
    then threaded `IgnoreStrength`: the converter tags `Heal` entries with
    `ignoreStrength`, the helper carries it, and `power-stats.ts` skips Healing enh /
    global +Heal for it. Fixed the genuine single-IgnoreStrength-heal powers —
    **Inner Will, Restore Essence (7.5), Energy Transfer** — no longer over-enhanced.
  - [x] **Both-portions heal SUMMED** (`extractHealingFromDamage` now combines same-table
    `type:'Heal'` entries into `scale` + `unenhancedScale`; `power-stats` enhances only
    `scale − unenhancedScale`). This unified single-portion / fully-unenhanced /
    both-portions handling and **fixed Inner Will** (it actually pairs a 0.075
    IgnoreStrength heal with a 0.075 enhanceable one — the old "take first entry" logic
    showed only 0.075; now 0.15 with enh on half). Runtime-only (the entries were
    already in `generated`), tsc + 84 tests.
  - [x] **FIXED — per-target HP-state leech effects (DNA Siphon etc.) were dropped.**
    Distinct from IgnoreStrength: leech powers gate effects on the *target's* HP state
    — `Cur.kHitPoints target> 0 >` (per **living** foe) / `... ==` (per **defeated**
    foe). The converter's conditional-gate filter treated these as positive-state
    gates and dropped the effects, so DNA Siphon showed **no heal/+End/+Regen/
    +Recovery**, Phoenix Awakening no Heal-over-Time, Glitz no damage, Soul Absorption
    no -ToHit. Per-case analysis showed they're the powers' **advertised base effects**
    (DNA Siphon's "Self +HP, +End, +Special"), NOT toggle conditions. Fix: add the
    `Cur.kHitPoints target> 0 (>|==)` clause to `_stripIgnoredClauses`, so pure HP-state
    gates fold into the base display (with `perTarget` preserving the per-foe scaling)
    while a trailing **mode** clause (`kDefensiveAdaptation Source.Mode?`) survives and
    keeps that portion an Adaptation conditional. Self-rez (`kHitPoints == 0`, no
    `target>`) untouched. 15 generated files (the ~9 leech/state powers × datasets);
    tsc + 84 tests. (The Warshade dead-enemy mechanic the user flagged — Stygian Circle
    — uses a *different* gate and isn't affected here.)
    - *Minor remaining:* DNA Siphon's mode-gated **heal** bonus (Defensive +0.375),
      being a heal-via-`damage` entry, doesn't surface in the Adaptation
      `conditionalEffects` (the heal-from-damage / mode-conditional paths don't meet).
      Base heal is correct; only the mode bonus is under-shown.
  - [x] **`activation_effects` drop — verified harmless (no action).** Audited every
    non-regen `IgnoreStrength` self template the filter drops: **0** are real
    enhanceable buffs (aspect=Current/Absolute on recovery/tohit/heal/defense/etc.).
    The non-duplicate drops are all meta-templates (the all-damage-types strength row,
    `Set_Mode`, `Grant_Power`, `Create_Entity`, `Global_Chance_Mod` — correctly
    dropped / skipped by `extractEffects` anyway) or protection (mez/KB/debuff-
    resistance) that is **also captured from the power's main `effects`** (confirmed:
    Entropy Shield's `mezResistance` etc. are present). So the drop loses nothing real
    — the earlier "duplicate-vs-genuine" worry didn't survive verification.

### Next steps
- [x] **`IgnoreStrength` DONE**; **per-target HP-state leech effects DONE** (DNA Siphon /
   Phoenix Awakening / Glitz / Soul Absorption now surface their advertised effects).
   Minor leftover: DNA Siphon's mode-gated *heal* bonus (heal-via-damage + mode) doesn't
   reach the Adaptation conditionalEffects — needs the heal/mode-conditional paths joined.
- [x] **`IgnoreStrength` DONE**; **per-target HP-state leech effects DONE** (DNA Siphon / Phoenix Awakening / Glitz / Soul Absorption now surface their advertised effects).
   Minor leftover: DNA Siphon's mode-gated *heal* bonus (heal-via-damage + mode) doesn't reach the Adaptation conditionalEffects — needs the heal/mode-conditional paths joined.
   **2026-09-02 — leftover CLOSED.** DNA Siphon's Defensive mode bonus now emits as a `conditionalEffects` entry (`defensiveadaptation`) carrying `Heal` + `ignoreStrength: true`, and `adaptation-modes.test.ts` grades all three mode bonuses (5/5 green). The heal/mode-conditional path is joined; nothing left. The tracker heading above this carried a stale `~` — the work settled 2026-08-13. (same pattern as the mez fields):
  `TimeToRoot` (2,340 — animation lock, affects DPS/rotation), ~~`ModesDisallowed`
  (3,475)~~ **DONE 2026-07-04** (+`ModesRequired`/`ModesSuspended`, resolved to names —
  see the power-level mode-gates SHIPPED note above), `StrengthsDisallowed` (951),
  `BuyRequires` (631). Remaining are genuinely absent, distinct names. Need parser reads
  + re-export (the toggle_ignore workflow).
- [ ] **Add the `.powers`↔export attrib name-map** to `audit.py` → produce a clean
  attrib-gap list → then close the genuinely-dropped exotic attribs.
- [ ] **Phase 2 — converter completeness**: diff `exported_powers` vs `generated` (the
  class the knockback bug belonged to — parser captured it, converter dropped it);
  ensure every mechanically-relevant template/field incl. `requires_expression`
  gating is emitted. Also fold in the template-tail field the converter still
  ignores: `suppress_events` (parsed into `EffectTemplate.suppress_events` but not
  yet consumed). Note: `flags` is already parsed **and** honored by the converter
  (`IgnoreStrength`/`IgnoreResistance` via `flags_raw`/`flags2_raw`); only `fx`
  (cosmetic) remains genuinely unparsed.
- [ ] **Later**: a `.powers ⊆ extraction` guard, once the sweep backlog is worked down.

## Resolved

- [x] **`stack_key` string-offset misread → real StackKeys registry names (TravelBuff, StealthToggle, …)** — 2026-07-12

**Bug (user travel-speed report).** Travel powers weren't respecting mutual suppression:
Combat Jumping + Super Jump stacked jump height, Super Speed's momentum stacked with Super
Jump, etc. Root cause was three-layer, but the *data* root was here in the parser.

**Root cause.** The Parse7 template parser read `stack_key` with `read_string()` — but the
field is a **u4 ID into the global StackKeys registry** (attrib_names.bin), NOT a string
offset. Dereferencing the small ID (28/29/31…) into `powers.bin`'s string-table head
returned garbage *suffixes of that table's first entry* (`"5thColumn.5thColumnEndgame.
NictusFX"`): ID 29 → `"ictusFX"`, 28 → `"NictusFX"`, 31 → `"tusFX"`. These happened to be
STABLE per-ID (same offset every time), so the stealth-suppression grouping still worked by
accident (all stealth powers → the same `"NictusFX"` garbage string) — which is why it went
unnoticed. But the travel group (`TravelBuff`) was invisible: its garbage string didn't match
anything the calc keyed on, and the converter dropped the movement-slot key entirely.

**Fix.** The StackKeys sub-array lives in `attrib_names.bin` right after the Mode/Elusivity
sub-arrays — same 16-byte record shape the mode table uses. Added `parse_stack_key_table` to
[_attrib_names.py](tools/bin-crawler/bin_crawler/parser/_attrib_names.py) (Parse7 anchored by
`TestStack`; Parse6/Rebirth variant walks the sub-arrays to the same anchor), refactored the
mode-table scanner into a shared `_scan_record_runs_parse7`. The template parsers now capture
`stack_key_id` (u4; -1 = no key, normalized to 0); [export_powers.py](tools/bin-crawler/bin_crawler/export_powers.py)
resolves ID → name. **Direct 0-based registry index on every format** (verified 10/10 vs the
CoD2 `.powers` oracle: kStealthToggle 28, kTravelBuff 29, kTravelMaxBuff 31 …; 351/351 agree
across all HC travel/stealth/mez-prot powers). Rebirth resolves its i25-era registry
(HybridSupport/HybridDefense on Support_Genome, structurally verified); Thunderspy's
heuristic tail-scan template parser doesn't decode stack fields at all → no keys, unchanged.

**Downstream (converter + calc + UI):** the converter now keeps the suppress-group key on
movement buffs and splits aspect=Maximum speed templates into a new `movementCapBump` slot
(fixing the bag collapse where Super Speed showed `1.938 × Melee_Ones` — the cap raise —
instead of `1.0 × Melee_SpeedRunning`, the real +350% buff); the calc resolves TravelBuff
groups strongest-wins (`resolveMovementTotals`, mirroring stealth) and gates suppressible
buffs on combat mode; the UI derives movement caps from `movementCapBump` data instead of
the hardcoded `TRAVEL_CAP_BUMPS` table. Also fixed: inherent Hurdle's dropped JumpingSpeed
(FITNESS_POWER_EFFECTS listed only jumpHeight) and Fly's double-counted kFly mode grant.
Full re-export (all 3 datasets, stack_key values change; else byte-identical), regen, gate
green. Guard: [travel-speed-fixes.test.ts](src/utils/calculations/travel-speed-fixes.test.ts)
(15 tests). 990 tests green.

- [x] Chain / max-targets expression fields — ChainTarget + MaxTargetsExpr recovered & verified against HC oracle — 2026-07-01

**Ask (user):** the export was missing the `ChainTarget` (next-target selection) and
`MaxTargetsExpr` (target-cap) RPN expressions that CoD surfaces.

**Resolved against HC/Parse7 data** (`/home/jiiwii/.wine/drive_c/Games/Homecoming/assets/live`,
`bin_powers.pigg` 2026-06-18 — the data was on THIS Linux box all along, not "PC/Laptop" as
the prior blocker assumed). Probe: `tools/bin-crawler/probe_chain_fields.py`. Final mapping:

| Field | Slot in `_parse_power` (Parse7) | Exported key | Verification |
|---|---|---|---|
| **ChainEff** | field 42 string_array | `chain_eff_expression` | `@ChainJump…minmax`, 77 powers (already shipped) |
| **MaxTargetsExpr** | field 38 string_array (HC-only) | `max_targets_expression` | `GauntletTargetCap` resolves here, 59 powers |
| **ChainTarget** | **field 43b** — was read as a discarded `u4_array` | `chain_target_expression` | circuits match `.powers` oracle exactly, 55 powers |

**The key discovery:** `ChainTarget` was NOT field 43 (the earlier hypothesis). Field 43 is
an FX/`ChainIntoPower` array. The real `ChainTarget` lived in **field 43b**, which the parser
read as `r.read_u4_array()` and threw away. A `string_array` and a `u4_array` are
**byte-identical** in Parse7 (`count` + N×u4), so reading 43b as a `u4_array` discarded the
string resolution WITHOUT desyncing the stream — that's why `prevdistance`/`maintarget>` were
present in the raw bytes but landed in none of the captured string fields. Changing 43b's read
to `read_string_array()` recovers the RPN with zero alignment risk. Located via a monkeypatch
diagnostic that resolved every `u4_array` element as a string offset and found the tokens at
the 2nd top-level `read_u4_array` call (= line 1050 = field 43b).

Oracle matches (HC `Rejuvenating_Circuit` / `Energizing_Circuit` / `Empowering_Circuit`):
```
ChainTarget = 101 kHitPoints%  target> - enttype maintarget> enttype target> eq 99 * 1 + * 1 prevdistance / +   (Rejuvenating — HP-priority)
ChainTarget = 101 kEndurance% target> - enttype maintarget> enttype target> eq 99 * 1 + * 1 prevdistance / +   (Energizing — End-priority)
ChainTarget =                            enttype maintarget> enttype target> eq 99 *     1 + 1 prevdistance / +   (Empowering — proximity-only)
```
The damage chains (`Chain_Induction`, `Jolting_Chain`, `Synaptic_Overload`) correctly have NO
ChainTarget — they use ChainEff + ChainFork, not proximity-weighted selection.

**Code (`tools/bin-crawler/`):** `parser/_dataclasses.py` promotes the two probe-scratch
fields to real `chain_target_expression` / `max_targets_expression`; `parser/_powers.py`
`_parse_power` captures field 43b as `chain_target_expr` and field 38 as `max_targets_expr`;
`export_powers.py` `power_to_dict` emits both (sparse, only when present, like `redirect`).
Parse6 (Rebirth/Thunderspy/Veracity) has neither field → both stay empty there.

**Promoted to committed data (focused, §6):** re-exported to a scratch dir, then copied ONLY
the 122 JSONs that gained a field into `exported_powers/` (the 6 that also carried unrelated
`duration_expression`/`magnitude_expression` drift were reverted and re-injected with just the
3 keys) → a purely additive **122 files / +178 / -0** diff. Then regenerated the 51 affected
player powersets via `convert-powerset.cjs` — **83 `generated/` `.ts` files, +99 / -0**, zero
drift (composed layer imports `base`, so generated-only is enough).

**Wired into the planner:** `convert-powerset.cjs` carries `chain_target_expression` /
`max_targets_expression` onto the power object; `src/types/power.ts` adds
`chainTargetExpression` / `maxTargetsExpression`; the Info panel (`PowerInfoBlocks.tsx`
`GeneralStatsBlock`) shows humanized **Chain Target** / **Target Cap** rows (raw RPN on hover)
via pure helpers in `src/utils/chain-expressions.ts`. Focused test:
`src/data/chain-target-expressions.test.ts` (7 asserts — circuit oracle values + humanizer
mapping). `tsc` clean; full suite 655/655 green.

Not covered (internal, non-Info-facing): the pets/redirects/inherent powers that also gained
fields in the export use other converters (`convert-pet-entities`, `reconvert-redirect-powersets`,
…) — regenerate those if the fields are ever needed there.

- [x] Summon-shell "ground patch" powers now surface the pseudo-pet's AoE footprint (Burn et al.) — display-only, bin untouched — 2026-06-24

**Was.** Burn showed as **Single Target** with no radius/cap; Location patches (Tar Patch,
Rain of Fire, Caltrops, Ice Storm, …) showed a bare "Location AoE" with the radius/cap hidden —
even though the real footprint (Burn → `FieryBurn` 15ft/10, Tar Patch 25ft/16) was already
resolved into `effects.summon.resolvedEntities[].abilities[]`. The headline read the shell's
`effect_area`/`radius`/`max_targets` (Self/SingleTarget/0), so the AoE numbers never surfaced.

**Fix (UI display-only — convert/bin untouched, as the literal `effect_area` is accurate and
downstream calc/targeting depends on it).** New `deriveSummonPatchArea(power, effects)` in
[PowerInfoBlocks.tsx](src/components/info/PowerInfoBlocks.tsx) — the single source that feeds
`GeneralStatsBlock` (used by both the Info Panel and the power tooltip):
- Fires only when the power has `summon.resolvedEntities` **and** no headline radius of its own
  (so real AoEs and MM/PET_ENTITIES summons are untouched). Keyed on `resolvedEntities`
  presence, **not** the `isPseudoPet` flag — that flag is `false` for Burn-style
  `PL_StaticObject` shells.
- Picks the **widest-footprint** resolved ability (self-buff abilities carry no radius and are
  skipped), borrows its `radius`/`maxTargets`. `255` (uncapped sentinel) is omitted rather than
  rendered as "255 max".
- Preserves the power's own area when it already conveys an AoE shape (Location patches stay
  **"Location AoE, 25ft, 16 max (patch)"**); only the misleading Self/SingleTarget shells
  borrow the pet's area (Burn → **"AoE, 15ft, 10 max (patch)"**). The `(patch)` suffix flags
  that the footprint comes from the summoned entity, not a direct hit.
- Also extended `formatEffectArea`'s `Location` case to render radius/cap (it previously dropped
  them), which additionally benefits any real Location AoE that carries a headline radius.

Types: `ResolvedPseudoPet`/`ResolvedPseudoPetAbility` re-exported from `@/types`. `tsc` clean;
44 pseudo-pet data/calc tests still green (display change touches no calc path).

- [x] Burn / Lightning Rod showed ZERO damage — a gated FieryEmbrace bonus beside the base DoT flipped the whole ability to `conditionalDamage` — 2026-06-24

**Symptom.** Burn displayed **no damage at all** (and Lightning Rod likewise). The summon
shell's resolved pseudo-pet carried the right scales, but the Damage block was empty.

**Root (`resolveSummonRedirects` in [convert-powerset.cjs](scripts/convert-powerset.cjs)).**
`FieryBurn` has two damage groups: an always-on `chance=1` Fire DoT (scale 0.14) **and** a
`chance=0, tags=["FieryEmbrace"]` bonus (scale 0.063) — the persistent Fiery-Embrace component.
A `chance:0`-with-payload group is a mode-gate sentinel, so the 0.063 leaf is marked `gated`.
The old rate classifier did `if (anyGated) conditionalDamage = true` — **one** gated bonus leaf
flagged the **entire** ability conditional, and the runtime keeps `conditionalDamage` abilities
OUT of the headline DoT. With both of Burn's abilities (FieryBurn + FieryBurn_FrontDamage) so
flagged, the guaranteed 0.14 / 1.0+0.44 base damage was suppressed → zero. Same shape on
Lightning Rod (base Energy 2.27 + a gated FE Fire 1.02) → also zero.

**Fix (one-spot, surgical).** `conditionalDamage` now requires that there be **no guaranteed
(non-gated) damage at all** — i.e. it's reserved for redirects whose damage is *entirely*
mode-gated (a storm High-Winds-only branch, the original Storm-Cell `1908` guard). A gated bonus
sitting **beside** an always-on hit no longer suppresses the headline; the bonus stays in the
`damage` array (the armor Burns deliberately surface the FE-active variant `0.14 + 0.063`, per
the existing `pseudopet-redirect` test + `_filterFieryEmbraceBonus` fire-set rationale). The
proc-rate (`damageChance`) derivation now reads guaranteed leaves only, so a gated bonus can't
drag the headline to a proc rate either.

**Scope.** Regen touched exactly the 9 affected powers — Burn ×5 (Tanker/Brute/Scrapper/
Sentinel/Stalker Fiery Aura) and Lightning Rod ×4 (Electric Melee) — each diff a pure
`conditionalDamage: true` removal, all damage scales intact. Storm Cell / Category Five
**unchanged** (their base aura is a non-gated `IncreaseStormStrength` accumulator, so they were
never hitting the all-gated branch). `tsc` clean; full suite **577/577** green (the
`pseudopet-redirect` Burn FE-variant test now passes for the right reason).

- [x] `Grant_Power → Temporary_Powers` DoT procs resolved end-to-end — Molten Embrace / Hidden Flame DoT now visible + enhanceable-aware (full 3-layer fix, 2026-06-24)

**Gap (from the I28P3 audit).** The note "Molten Embrace [Sentinels] — the DoT damage proc
is now affected by damage enhancements and buffs" was unverifiable from `exported_powers/`:
the passive holds **two `Grant_Power` templates** → `Temporary_Powers.Temporary_Powers.
Molten_Embrace_Proc`, and the *actual* player-facing damage (Fire ~0.0493/tick, period 1.0s,
duration 2.1s, **no `IgnoreStrength`** = enhanceable) lives in that granted power.
`Temporary_Powers` isn't an exported category, so the proc's damage **and** its
enhanceable-ness were invisible to the whole pipeline. **Scoped first (verify-don't-assume):**
confirmed the planner showed *nothing* of this proc (generated Molten Embrace = only the
`+Damage` toggle; empty override; not in `proc-data.ts`, which is IO-set procs only). A direct
`parse_powers` over `Temporary_Powers` showed the real scope is **narrow**: of 67 distinct
`Temporary_Powers` grant targets, 14 carry damage — 13 of those are all-8-types
`IgnoreStrength` **+Damage *buff*** definitions (Power Siphon, Reach for the Limit, Perfection
of Body …, aspect=Strength), already represented elsewhere; the genuinely-missing
**damage-DEALING** single-type procs are just Molten Embrace, Stalker Hidden Flame (same
proc), Toxins, Envenomed Blades.

**Fix (3 layers, all data-driven — no curated power lists).**
1. **Export** ([export_powers.py](tools/bin-crawler/bin_crawler/export_powers.py)) — new
   `_collect_grant_targets` + a referenced-target inclusion pass: after filtering to player
   categories, pull in exactly the `Temporary_Powers` powers a player power **grants** via
   `Grant_Power` (67 files), instead of the 1,247-power whole category (travel/accolade/event
   junk). Mirrors how `Pets`/`*_Aux` host `Execute_Power`/redirect damage, but scoped to
   references. `convert-all-powersets` never turns `Temporary_Powers` into a powerset (not in
   its category map), so these are pure resolver data. Gated off `--categories`.
2. **Convert** ([convert-powerset.cjs](scripts/convert-powerset.cjs)) — new
   `resolveGrantedDamageProcs` / `_buildGrantedDamageProc`: walks the grant, loads the target
   via the existing `resolveRedirectPath`, and attaches `grantedDamageProcs`. Three traps
   handled: (a) the proc's damage lives in nested `child_groups` with engine-guard
   `requires_expression`s (`activateperiod==0`, Judgement exclusions, `enttype critter eq`) that
   `collectTemplatesWithChance` reads as conditional gates and would skip — so a dedicated
   **lenient PvE walker** collects through them, dropping only the `enttype target> player eq`
   PvP variant; (b) the damage genuinely sits on a `Melee_PvPDamage`-NAMED table, so the redirect
   resolver's `/pvp/i`-table drop is wrong here (the **requires**, not the table name, is the
   PvE/PvP discriminator); (c) **multi-grant guard** — a power granting >1 distinct proc is a
   mutually-exclusive MODE system (Bio Armor Offensive/Defensive/Efficient adaptations grant all
   three procs with no group-requires to tell stances apart), so it's left to the existing
   conditionalEffects/Mechanic-Adjuster surface rather than flat-attached. `extractDamage` already
   drops the aspect=Strength +Dmg buffs and we additionally exclude `Heal` (Bio's defensive proc
   is a self-heal). `enhanceable = no kept damage template has IgnoreStrength`.
3. **Display** ([InfoPanel.tsx](src/components/info/InfoPanel.tsx) + `GrantedDamageProc` type in
   [power.ts](src/types/power.ts)) — a "Granted Damage Proc" block computing per-tick × ticks
   over duration via `calculateDamageWithATTable` (the AT-table helper that resolves
   `Melee_PvPDamage`; `calculateEffectValue` does NOT — §13 silent-fallback trap, now guarded by a
   test). Enhanceable procs scale with the power's slotted damage + global damage strength;
   IgnoreStrength ones show flat. Surfaced **informationally** (it procs off the player's own
   attacks at `tickChance`), not summed into the power's own DPS.

**Result.** Exactly 4 generated powers gain `grantedDamageProcs`: Molten Embrace + Stalker Hide
(Fire 0.0493, **enhanceable:true**, 80%/tick, 2.1s — exactly the I28P3 claim), Toxins +
Envenomed Blades (Toxic, enhanceable:false). Sentinel per-tick resolves to **Fire 3.80 ×3 ≈
11.4** over 2.1s. **De-risked per §6:** scratch export vs committed = **0 files differ**, the only
delta is the new `temporary_powers/` dir (+67 files). Regen is idempotent (only the 4 files).
Guard: [granted-damage-procs.test.ts](src/data/granted-damage-procs.test.ts) (5 cases incl. the
Bio-not-attached regression + the table-resolves guard). tsc clean, **577 tests**.

**Follow-up — DONE (2026-08-25, calc work in the 1.0 app, not a data gap).** The Bio Armor
adaptation toxic proc + the +Damage-buff grants (Power Siphon, Reach for the Limit, Perfection
of Body) remain on the Mechanic-Adjuster surface, but the granted DoT now ALSO folds into
per-attack DPS: the Attack Chain Builder adds each proc's expected per-cast contribution
(`tickChance × per-tick × ticks`) to every attack (`grantedDoTPerAttack` in
`attack-chain-powers.ts`), gated on `hasDamage` (a pure Build Up gains no pseudo-attack damage)
and on the stance (`activeSubPower`) for stance-scoped grants like Bio Offensive Adaptation.
Guard: `attack-chain-granted-dot.test.ts`.

**Audit-harness note (context, not a pipeline bug):** the production exporter DOES capture
`child_effects` (raw `child_groups`) and caster-on-cast buffs under `activation_effects` (e.g.
Blinding Powder's "+30% crit chance for 10s" `Global_Chance_Mod`) — any ad-hoc tooling reading
these JSONs must walk both or it under-reads.

- [x] Thunderspy Primalist orphan-power LEAK into the HC export — source-aware category gating (2026-06-24)

**Bug (found during the I28P3 export regen).** The HC export contained three
Thunderspy-only Primalist categories — `feral_might` (14), `primal_gifts` (12),
`primalist_misc` (37) = **63 orphan powers**. Primalist is strictly a Thunderspy custom AT;
it must not appear in HC at all. `primalist_misc` had been leaking into committed HC
exports for a while; `feral_might`/`primal_gifts` newly surfaced with the current HC bin.

**Root cause.** `PLAYER_CATEGORIES` in
[export_powers.py](tools/bin-crawler/bin_crawler/export_powers.py) is **one shared whitelist
applied to every source**, and it lists `Feral_Might`/`Primal_Gifts`/`Primalist_Misc` for
Thunderspy's sake. The old comment asserted *"HC/Rebirth have no equivalent (these
categories simply won't exist in those datasets)"* — **false.** HC's `powers.bin` carries 63
orphan Primalist powers under those exact name-prefixes with **0 backing powersets** (verified
`parse_powersets` → 0), **0 powercats** (`powercats.bin` has 203 cats, none Primalist), and
**no Primalist class** (HC `classes.bin` = the 15 standard ATs). The exporter assigns each
power's category from its dotted-name prefix and filters by `pw.category in categories`, so
the orphans sailed straight through. The Tspy-support whitelist entries quietly leaked HC's
orphan copies. (Why HC's bin even has these powers: shared CoX codebase lineage; they're
unwired dev/legacy data, not playable HC content.)

**Severity was low** — export-tree-only. The convert step never produced HC-side Primalist
data (no HC class/powerset to attach to), so the planner was unaffected; the legit Primalist
in the app comes from `src/data/datasets/thunderspy/`. Still wrong, fixed at root.

**Fix.** New `_source_has_primalist_class(resolver)` helper + `PRIMALIST_CATEGORIES`
gate in `main()`: drop `Feral_Might`/`Primal_Gifts`/`Primalist_Misc` from the category set
unless the source defines a **Primalist class** (checks `classes.bin` +
`villain_classes.bin` for a class name containing `primalist`). Data-driven, no per-dataset
flag; skipped when the user passes explicit `--categories` (advanced override). Updated the
misleading whitelist comment to document the orphan-leak gotcha.

**Validated.** HC: `_source_has_primalist_class` → **False** → re-export drops all 3
categories → **63 cats / 10,644 powers** (was 66 / 10,707; exactly the 63 orphans removed).
Thunderspy (`coxg` AND `tspy` sources): → **True** (matches `Class_Primalist`) → Primalist
categories **retained** — the no-op guarantee for Tspy holds. Other cross-server whitelist
entries (Rebirth `Guardian_Assault`/`Guardian_Comp`) do **not** leak into HC (absent from
its bin); no other phantom categories found. Committed-tree cleanup: the stale leaked
`exported_powers/primalist_misc/` removed as part of the HC re-export apply.

**SECOND VECTOR — `export_entities.py` (worse: it reached the planner).** The powers leak
was export-tree-only/inert, but `export_entities.py` exports every villaindef entity whose
name starts with `pets_`/etc. with **no class/category gate**, so HC's 13 orphan Primalist
pet entities (`pets_primalwolf`, the per-attack `*_heal` lifesteal pseudo-pets) leaked — and
these DID flow through `convert-pet-entities` into `src/data/datasets/homecoming/
pet-entities.ts` (26 refs). Fixed with the SAME gate, reference-based: import
`PRIMALIST_CATEGORIES` + `_source_has_primalist_class` from `export_powers` and skip any
entity referencing a power in a Primalist category (robust — the NPC false-match
`pets_minisignature_achlysgeneral_hunter`, refs `NPC_Pets`/`Pets` only, is correctly kept).
HC re-export: **13 dropped, 731 written**, fresh-vs-committed diff = exactly those 13
deletions (0 content drift on the 731 shared). After `convert-pet-entities` (note: it's
`generated:false` in regen-all, so `regen:generated` does NOT refresh it — must run the full
`regen` or the script directly), HC `pet-entities.ts` → **0 Primalist refs**; Thunderspy/
Rebirth pet-entities unchanged. Exhaustive post-sweep of all HC surfaces (exported_powers,
datasets/homecoming, generated, entities, tables) = clean; remaining "primal" hits are the
Kheldian **Primal Energy** inherent and the Sorcery **Enflame** pet (not the Primalist AT).

- [x] HC P-hash "entity_def" FIXED — it was the EntCreate float/combat-text message, now resolved to the real entity at the parser — 2026-06-12

**The premise was wrong.** This was filed as "an opaque entity-def reference needing a
separate villain/entity-def bin's name table (new parser, high effort)." It is none of
that. The opaque `P<number>` is a **message-table key for the EntCreate's float /
combat-text DisplayMessage** (the summon/detonation announcement) — resolvable via the
**existing** `clientmessages-en.bin`, no new parser. Verified: **all 49** distinct P-hash
`entity_def` values across the HC export resolve to a message string, and every one is
float text:
- `P1985334123` (×25) → *"You call forth a {PowerName}!"* (Rain of Fire, Sleet, Auto Turret…)
- `P174413786` (×16) → *"You turn the ground into a sticky {PowerName}…"* (Tar Patch)
- `P1757360070` → *"You call forth Three {PowerName} of dancing flame!"* (Fire Imps)
- `P1648600109` → *"An Enraged Spectre emerges!"* (hence it "maps to 2 names" — same float,
  two pet tiers; it was never a name hash, which is why CRC32/mul33 found 0 matches)
- `P1090583630` → *"Zero!"* (the bomb detonation float — the 3 "unresolved" Self_Destructs)
- `P4164625865`/`P2437733030` → *"You take {Damage} points of … damage from the explosion."* (Briefcase)

**Why the parser mislabels it.** `_extract_params`'s heuristic
([_powers.py](tools/bin-crawler/bin_crawler/parser/_powers.py)) picks `entity_def` as the
first tail string passing `_looks_like_entity_def`. A message key `P\d+` passes that test
(leading uppercase, all alnum) **and** the DisplayMessage leads the Params block before the
EntityDef — so the float-message key is grabbed as `entity_def`, and the **real** entity
name falls through to `priority_list`. Proven by the dual Self_Destruct cases:
`Underground_Bomb` has the real `Objects_Underground_DemoCharge` *first* → grabbed
correctly (P-hash is a 2nd EntCreate's float); `Proximity_Bomb` has *only* `P1090583630`
("Zero!") → P-hash grabbed, no `priority_list` (it's a pure detonation, summons no pet).

**Live impact: none.** The 44 referenced pseudo-pets resolve correctly — `priority_list`
*is* the real entity source (Rain of Fire→`Pets_RainofFire`, Tar Patch→`Pets_TarPatch`,
Fire Imps→`Pets_FireImp_Controller`), so the "workaround" was actually reading the right
field all along. The ~5 P-hash-only powers (Proximity_Bomb.Self_Destruct, Briefcase.Detonate,
Crey.Clone_Subjugator.Consume, Council.Robot_Jets.Launch) are unreferenced object/NPC float
effects.

**Fix (applied 2026-06-12).** New `_is_message_key(s)` helper (`P`+digits) in
[_powers.py](tools/bin-crawler/bin_crawler/parser/_powers.py); both `_extract_params`
(Parse7) and `_extract_params_parse6` now drop message keys from the EntCreate string pool
*before* picking entity_def/priority_list. Result: the off-by-one is corrected —
`entity_def` lands on the real EntityDef and `priority_list` on the real AI list. The
invariant that de-risks it: **new `entity_def` == old `effectiveEntity`** (the convert
ternary `/^P\d+$/.test(ed) ? priority_list : ed`), so `convert-powerset` resolves
identically. Pure-float detonations with no real entity (Proximity_Bomb.Self_Destruct =
"Zero!") now correctly emit **no** EntCreate.

**Verified (HC re-export + generated diff).** Scratch re-export → committed diff was
**156 files, every change confined to `effects` (0 non-effects drift); all 156 had a
committed P-hash entity_def** (no collateral). Applied + regenerated HC powersets (26 files):
- P-hash → real name everywhere (Rain of Fire→`Pets_RainofFire`, Tar Patch→`Pets_TarPatch`).
- **Bonus fixes:** powers that previously showed a *garbage* opaque entity now resolve —
  `P4176979480`→`Pets_Volcanicgas`, `P1734497484`→`Pets_Spirit_Tree`, `P2416771117`→
  `Pets_Fallout`, `P2874174058`→`Pets_Phoenix`; Black Hole's bogus `4ZHT…`-hash summon
  (an encoded-unresolved-P-hash placeholder; Black Hole is a no-damage intangibility field,
  its mez lives in `effects`) is dropped.
- **Counts/splits preserved:** Fire Imps ×3, Gremlins, Decoy ×3, Gang War, Soul Extraction
  (mutuallyExclusive), Rain of Arrows still split (`Pets_RainofArrows_Visual` ≠
  `Pets_RainofArrows`). Dominator Shadow Field improved: `entityCount:2`-of-one → the real
  base + `_Domination` variant (the P-hash EntCreate was the Domination variant, masked by
  its base priority_list).
- `resolvePhashSiblings` is now effectively a no-op (no P-hashes survive) but left in place
  as a harmless safety net. **Rebirth unaffected** — its Parse6 already emits real `Pets_*`
  entity_defs (verified **0** P-hash EntCreate params), so the shared filter is a no-op there.

Guards: `multipet-summon-count.test.ts` (Rain-of-Arrows assertion updated: P-hash resolved →
assert the two distinct resolved names) + the pseudopet suites. tsc clean, 476 tests.

- [x] Rebirth Parse6 AttribMod flags decoded — IgnoreStrength/Resistance/CombatMods (bundled full re-export) (2026-06-12)

**Gap.** Rebirth template `flags` were always `[]` — the calc never saw `IgnoreStrength`,
so 81 player powers (Bio Armor, Stone Armor toggles, Unstoppable, Meltdown, Power Surge,
Resurgence, Mutation, Elixir of Life…) **over-enhanced** their `+Recovery`/`+ToHit` buffs
(applied EndMod/ToHit enhancement where the game says ignore).

**Root cause + fix.** NOT a missing field — Parse6 stores these as the **inverse `Allow*`
9-bool block** (NearGround, AllowStrength, AllowResistance, …, AllowCombatMods) that
`_parse_effect_template_parse6` read into `_bool_block` and **discarded**, plus a
`cancel_on_miss` bool. Now decoded → HC-shaped `flags`: `IgnoreStrength` (not AllowStrength),
`IgnoreResistance`, `IgnoreCombatMods`, `NearGround`, `CancelOnMiss`. The calc is
dataset-agnostic, so the existing `IgnoreStrength` handling (split enhanced buffs to the
`*Unenhanced` bucket, §4; meta-template exclusion) now fires on Rebirth too. **Validated**
against the HC binary: precision/recall **0.9992/0.9954** (IgnoreStrength), **0.9842/0.9981**
(IgnoreResistance), **0.9715/0.9484** (IgnoreCombatMods) across 14,644 aligned templates.
**Still deferred:** `CopyBoosts`/`PseudoPet` live in the not-yet-decoded post-magnitude tail,
NOT this bool block — separate RE.

**Bundled the full Rebirth re-export** (closing the long-standing staleness item). De-risked
per §6: scratch export → a leaf-level diff PROVED the entire committed-vs-fresh delta was
**only `{flags, tags}` across 7,823 files** — zero game drift, no scale/table/effects-structure
changes. Applied + regenerated: 69 generated files change (all legit IgnoreStrength→Unenhanced
splits / `ignoreStrength:true`), **HC untouched**, `tags: null→[]` refreshed, `entities/`
already current (separate `export_entities.py` path, 0 changes). tsc clean, 480 tests.

- [x] Rebirth stealth suppression (NictusFX) — fix BUILT then REVERTED to additive (deliberate, 2026-06-12)

> **OUTCOME: Rebirth stealth is kept ADDITIVE.** The cross-server-oracle fix below was built,
> validated, and then **deliberately reverted** (user call) — per the Jounin lesson (Rebirth
> genuinely diverges from HC, only in-client observation settles it), additive is the **safer
> inference**: its sole failure mode is an inflated *display* stealth radius, and it never
> affects what's slottable. Re-apply ONLY if live Rebirth is observed to be max-wins. The
> membership (12 leaves) + mechanism are kept below for that re-application.

**Gap.** Rebirth stealth radius was **pure-additive** (e.g. Stealth 55 + Super Speed 35 →
90 instead of max 55), over-counting any build with 2+ suppress-group stealth powers. Filed
as "Parse6 drops the stack fields" — but that was wrong.

**Root cause (reframed via verify-don't-assume).** It is NOT a droppable field. HC's Parse7
`stack_key` is a **string** that can name a *global, cross-power* group ("NictusFX", shared
by 30 stealth powers). Rebirth's older **Parse6 `stack_key` is a per-power integer** (e.g.
Support_Genome groups its own damage types as 1/2; Guardians_Gift uses 6–10 for its mez
types) — it structurally **cannot express** a global string key. Proof: "NictusFX" appears
in Rebirth's `powers.bin` only **once**, on an unrelated NPC FX power; Super Speed's stealth
template doesn't contain it at all. (The same multiverse install confirms the format split:
Thunderspy's `coxg/bin.pigg` is **Parse7** and carries NictusFX; Rebirth's `z_rebirth_bin.pigg`
is **Parse6** and can't. Rebirth's `rebirth/` is the correct, current source — `piggs/` is the
stock base with no Rebirth content.) So the suppression is CoH **engine behavior** that only
Parse7 happens to serialize.

**Fix (cross-server oracle).** `STEALTH_SUPPRESS_LEAVES` in
[convert-powerset.cjs](scripts/convert-powerset.cjs) — the binary-derived HC membership by
power **leaf name** (12 leaves: Stealth, Invisibility, Super_Speed, Cloak_of_Darkness,
Energy_Cloak, Arctic_Fog, Shadow_Fall, Steamy_Mist, Shadow_Cloak, Cloaking_Device,
Shinobi-Iri, Kyokan). When the export has no native `Suppress` key (Rebirth) and the leaf
matches, emit `stackKey: "NictusFX"`. **No-op on HC** (native key already set) — verified: HC
regen = 0 changes. Leaf-match is safe because `extractEffects(…, powerName)` is fed only
player powerset + pool powers (convert-powerset / convert-pool-powers); the 73 HC NPC/pet
powers sharing these leaves (additive) use a separate extractEffects / aren't converted.

**Membership is binary-authoritative, and the double-check paid off:** the prior prose listed
**Mask Presence** as a suppress member — the binary says `Replace`/null (additive). Corrected
in the entry below + memory. **Result:** 26 Rebirth player stealth powers (incl. the 3 pool
powers) gain `stackKey: "NictusFX"`; Hide / Grant Invis / Mask Presence / IO procs stay
additive. Guard: [rebirth-stealth-suppress.test.ts](src/data/rebirth-stealth-suppress.test.ts).
tsc clean, 480 tests.

- [x] Rebirth "Return From The Grave" resurrection set — mislabeled "Brute Archetype Sets" → new "Resurrection" category (2026-06-12)

**Bug (Rebirth).** Return From The Grave / Superior (Rebirth's first-ever Rez IO
set, Halloween event) showed up as a **Brute Archetype Set**. Its 60-power pool
is exclusively resurrection powers (Revive, Rise of the Phoenix, Soul Transfer,
Resurgence, Howling Twilight, Resurrect, Rebirth, Mutation, Stygian Return,
Restore Essence, Resuscitate, Power of the Phoenix, Phoenix, Conduit of Pain…),
so it leaked "Brute Archetype Sets" onto every non-Brute rez power (Controller
Resurrect, Pool Resuscitate, Scrapper/Stalker Revive, Kheldian self-rezzes…).

**Root cause.** No common-rarity set shares the rez pool, so pool-matching
fails; the rarity is `ECHalloween`, which triggers `_infer_ato_category`, and
that scanned the first power (`Brute_Defense.Dark_Armor.Soul_Transfer`) → bogus
`ECBrute`. It was the ONE `ECHalloween` set without a curated override (Witchcraft/
Haunting/Nightmare all have one that wins first).

**Fix.** New bespoke **"Resurrection"** category (the Witchcraft→"Universal Debuff"
precedent). (1) `_CHALLENGE_SET_OVERRIDES_BY_NAME` → "Resurrection" for both names
in [_boostsets.py](tools/bin-crawler/bin_crawler/parser/_boostsets.py); guarded the
`_infer_ato_category` call to skip BY_NAME-overridden sets so the raw category isn't
left as a misleading `ECBrute`. (2) Plumbed the category through `IOSetCategory`
(common.ts), `IO_SET_TYPE_TO_CATEGORY` (io-sets.ts), `SET_CATEGORY_TO_ENHANCEMENT`
+ `CATEGORY_PRIORITY` (enhancement-registry.ts), the 4 event-tier special-cases in
EnhancementPicker.tsx, and the set `type` in rebirth `io-sets-raw.ts`. (3) **Surgical
re-apply** (§6): recomputed `build_power_category_index` on the fixed boostsets and
copied only `allowed_set_categories` into the 59 affected committed
`exported_powers/rebirth` JSONs, then regenerated (44 powerset files + power/epic
pools). Non-Brute rez powers DROP the spurious "Brute Archetype Sets" and gain
"Resurrection"; genuine Brute rez powers (Soul Transfer / Rise of the Phoenix in
`Brute_Defense.*`) KEEP "Brute Archetype Sets" (real Brute ATOs slot every Brute
power) AND gain "Resurrection". Guard:
[rebirth-resurrection.test.ts](src/data/rebirth-resurrection.test.ts). tsc clean, 473
tests.

**Same boostsets sweep, two side-findings.** (a) The old `_boostsets.py` TODO list
flagged Vampire's Bite (ECMelee), Imperial Might (ECKnockback), Liberty's Belt
(ECResist) as "mis-tagged" — VERIFIED CORRECT against the binary: their pools are
byte-identical to the standard Melee(511)/Knockback(608)/Resist(314) pools, so
pool-matching assigns them right. Replaced the false TODOs with the verified note.
(b) The Call Jounin "missing Accurate Defense Debuff" question is now PROVEN a
genuine Rebirth client `boostsets.bin` omission (not a dropped field/parser gap) —
see [REBIRTH_PARSER.md](REBIRTH_PARSER.md) §1 and
`memory/io-set-category-plumbing.md`; parked pending live-client confirmation.

- [x] SYSTEMIC parser misalignment FIXED — 1133 powers' dropped effects restored (incl. Trip Mine) — 2026-06-11

**One-line root fix** in [_powers.py](tools/bin-crawler/bin_crawler/parser/_powers.py):
the field before the redirect block was read as a lone `read_u4()` ("redirect
pre-field, always 0 in samples") but is actually a **`u4_array`** (a mode/recharge-group
list, e.g. `['kPostDeath']` on pet/entity powers). Reading it as a single u4 was
byte-identical ONLY when the array was empty (count=0); when non-empty it left the
element values in place, shifting the redirect + effects reads → `_parse_effects` read a
garbage `eff_count` (2360) → the `try/except pass` silently produced `effects: []`.
Changed `read_u4()` → `read_u4_array()`: identical for the empty case (zero regression),
correct otherwise.

**Verified (CoD2 oracle + scratch-export diff).** Re-export changed **1133 HC powers
0→effects, 0 lost, 0 changed, 0 spurious** (no power gained effects CoD2 says shouldn't
exist). 265 are CoD2-confirmed player-relevant (MM henchman abilities, Kheldian Energy
Drone `Impact`, Lore/Signature pet attacks, Beast wolf `Wild_Charge`/`Howl`); the other
868 are NPC/critter powers that feed Lore-pet mimicry (Cabal, Arachnos…). `Pets.Mine.
Trip_Mine` now carries `Fire 2.0 + Lethal 1.0 + Fire 1.0 + KB` — matching CoD2 — so Trip
Mine / Time Bomb (Devices/Traps/Arsenal) finally show explosion damage via the pet path.
Materialized: `exported_powers/` (1134 files) + regen → `pet-entities.ts` (+3495) + Bane
Spider Placate (a player VEAT power that was empty) + epic-pools. Also added a **fail-loud
`_warn_dropped`** so a future silent drop announces itself (the swallowed `try/except pass`
hid this for months). tsc clean, 460 tests (+ `parser-effect-alignment.test.ts`).

**Stragglers / follow-ups (not this fix):**
- **`Incarnate_I20.Airstrike.Main`** — the 1 of 265 still empty, but a DIFFERENT bug:
  `eff_count=1` (not garbage), its single group's *templates* fail to parse (template-
  level, one Incarnate Judgement). Separate investigation.
- **Rebirth Parse6** — `_parse_power_parse6` was NOT touched; 109/1361 Rebirth pet powers
  (8%) have 0 effects, possibly the same misread in the Parse6 layout. Needs its own
  byte-level check + a Rebirth re-export. Tracked for the Rebirth data pass.
- The "ours >> CoD2" counts on Assassin `_Quick`/`_Stealth` + StormCell variants are
  PRE-EXISTING (identical in the committed export) — CoD2 de-dups redirect/PvP-paired
  templates; not introduced here.

<details><summary>Original investigation notes (kept for context)</summary>

Started as "Trip Mine shows no damage," root-caused into a **systemic binary-parser bug**.
The data IS in `powers.bin` — CoD2 reads the same file and gets the effects — **our parser
misaligns on a class of pet/summon entity powers and silently drops their entire effects
array.**

**Smoking gun.** Instrumenting `_parse_effects` for `Pets.Mine.Trip_Mine`: it reads
`eff_count = 2360` (0x938) — **garbage**. The reader is misaligned by the time it reaches
the effects struct_array, so it reads a junk count; every subsequent group read fails and
is swallowed by the `try/except pass` at [_powers.py](tools/bin-crawler/bin_crawler/parser/_powers.py)
~788 → the power ends up with `effects: []`, **no crash, no warning**. A sibling in the
same powerset (`Pets.Mine.Self_Destruct`) aligns fine, so it's a **field-size misread
triggered by something in these powers' record shape** (a variable-length field —
attack_types / boosts_allowed / a string-array / the redirect pre-field — sized wrong for
this layout, cascading into the effects offset). The format auto-detect (`has_field_45b` /
`has_field_41b`) is the prime suspect.

**Scope — 265 powers** where CoD2 has effects and our deep-walked export has **zero**
(`raw_data_homecoming-20251209` vs `exported_powers`, child_effects counted on both sides
to avoid the false positive that Dual Pistols / Electrical Melee — which merely *nest* in
`child_effects` — initially produced):

| category | # | player-relevant? |
|---|---|---|
| Mastermind_Pets | 64 | **yes** — henchman abilities (Beast wolf `Wild_Charge`/`Howl` = dmg + ToHit debuff) |
| Objects | 47 | Trip Mine / Time Bomb detonations, mission objects |
| Pets | 44 | `Pets.Mine.Trip_Mine` (Fire 2.0 + Lethal 1.0 + 2×KB), location pets |
| Incarnate | 43 | **yes** — Lore pet attacks/buffs (`Mind_Link`, `Soothe`, `Frigid_Burst`) |
| Villain_Pets | 22 | Mu `EM_Pulse`, Arachnobot `Web`, Lore-mimic sources |
| Signature_Summon | 16 | **yes** — Signature Lore pets (`Soul_Storm`, `Psychic_Wail`) |
| Mastermind_Summon | 14 | **yes** — `Grant_Power` upgrade powers (Tame/Train Beasts, Abyssal Empowerment) |
| Kheldian_Pets | 5 | **yes** — PB Energy Drone `Impact` (Energy dmg + KB) |
| Redirects / NPC_Pets / GenericVillains / Epic | 9 | mixed |

Some victims are travel/FX (`Super_Leap`, `Fly_fx` — low value), but many are **real pet
attacks/buffs/upgrades** players care about. **Caveat to confirm:** whether each dropped
power actually feeds the planner's displayed pet damage (some pets' main attacks may parse
fine and only secondary abilities are dropped) — scope the player-facing delta before/after
a fix. This supersedes the earlier (wrong) "explosion entity is unnamed" note — the entity
red herring was `Self_Destruct`; the damage is on `Pets.Mine.Trip_Mine`, which we misparse.

**Fix = find the misaligned field in the entity-power record layout** (compare byte layout
of a correctly-parsed pet power vs a victim; the divergence point is the bad field). Proper
binary-sourced root fix — restores ~265 powers, kills the Trip Mine gap, and needs NO new
bin.

</details>

- [x] Remote Bomb shows damage — `*_Info` display-power resolution (4 powers) — 2026-06-11

Surfaced right after the Trip Mine fix (user: "Trip Mine shows damage, Remote Bomb does
not"). **Different root** — not the parser, and not a pet: **Remote Bomb** (Blaster
Devices + Traps Controller/Corruptor/Defender; the reworked "Time_Bomb") is a pure
**mode-conditional redirect shell**. The player power has no effects; its Self/Target/
detonation redirects carry only a scale-0 placeholder + the bomb-pet summon
(`Pets_Bomb_Temporal`, a control bubble). The game keeps the player-facing damage on a
dedicated `<name>_Info` / `_Blaster_Info` display power (`show_in_info`, condition `'0'`
— never fires mechanically, exists to show the number): `Remote_Bomb_Blaster_Info` =
Fire 2.0 + Lethal 3.0 + KB.

**Fix** ([convert-powerset.cjs](scripts/convert-powerset.cjs)
`collectInfoRedirectTemplates`): when a redirect-shell power's mechanical redirect yields
no damage, follow the `*_Info` display power paired in its own redirect list. Three
gotchas handled: (1) strip the redundant `arch source> Class_<AT> eq` selector (the info
power is already AT-specific, else `collectTemplatesDeep` drops the whole group);
(2) drop the PvP `enttype target> player eq` KB variant (prefer PvE — else KB doubles
4+4=8); (3) bypass `_filterFieryEmbraceBonus` for info-sourced damage (it wrongly stripped
Remote Bomb's genuine base Fire as an FE bonus). Result: Blaster → Fire 2.0/Lethal 3.0/KB
4; Traps → the "Temporal Bomb" Fire 1.182/Lethal 0.818. Exactly **4 generated files**;
gated to the redirect-shell+`_Info` shape so nothing else is touched. Guard:
[redirect-info-damage.test.ts](src/data/redirect-info-damage.test.ts). tsc clean, 462
tests. *(Traps damage values worth an in-game spot-check — they come from the generic
`Remote_Bomb_Info`/"Temporal Bomb" display power.)*

- [x] Rebirth Tar Patch no longer shows "To Hit Debuff" — Witchcraft recategorized "Universal Debuff" (2026-06-11)

**Bug (user, Rebirth):** Tar Patch's slotting offered "To Hit Debuff" sets, but in-game
it only takes Slow, **Universal Debuff**, Range, Endurance, Recharge.

**Root cause.** `allowed_set_categories` is computed from boostsets.bin's authoritative
per-set power lists. The only non-Slow set covering Tar Patch is **Witchcraft /
Superior Witchcraft** (Rebirth Halloween event sets). Their pieces span **Defense + Slow
+ ToHit Debuff** — genuine *multi-aspect "Universal Debuff"* sets, slottable in any of
those power types (Tar Patch qualifies via its **Slow**). But the binary tags them
`ECToHitDeBuff` (one of their aspects) → the exporter labeled every power that can slot
them "To Hit Debuff", wrongly implying Tar Patch (which has NO to-hit debuff) takes
ToHit-debuff sets. (HC is unaffected — it has no Witchcraft; HC Tar Patch = `['Slow
Movement']`.)

**Fix.** Recategorized Witchcraft + Superior Witchcraft → "Universal Debuff" via
`_CHALLENGE_SET_OVERRIDES_BY_NAME` in
[_boostsets.py](tools/bin-crawler/bin_crawler/parser/_boostsets.py) (same mechanism as
ForcedIndoctrination → "Universal Control Duration"). Real single-aspect ToHit sets
(Cloud Senses, Dark Watcher's Despair, Siphon Insight) stay "To Hit Debuff".
**Surgical re-export** (per the de-risk workflow): exported Rebirth to scratch and applied
**only** the `allowed_set_categories` field to the committed `exported_powers/rebirth`
(**922 power files** — the real scope of Witchcraft's slottable powers; verified each diff
is allowed_set_categories-only, no stale-export drift). Plumbed the new category through
the planner: `IOSetCategory` ([common.ts](src/types/common.ts)),
`IO_SET_TYPE_TO_CATEGORY` + `CATEGORY_TO_ASPECTS`/`CATEGORY_PRIORITY`
([io-sets.ts](src/data/io-sets.ts), [enhancement-registry.ts](src/data/enhancement-registry.ts)),
Witchcraft `type` in [io-sets-raw.ts](src/data/datasets/rebirth/io-sets-raw.ts), and the
EnhancementPicker event-tier-standard surfacing.

**Verified.** Tar Patch → `['Slow Movement', 'Universal Debuff']` (no To Hit Debuff);
genuine -ToHit powers (Darkest Night, Fearsome Stare, Twilight Grasp) KEEP "To Hit Debuff"
AND gain "Universal Debuff" — the discriminator is exact. Regen changed Rebirth powersets
(+ pools/epics). Guard:
[rebirth-universal-debuff.test.ts](src/data/rebirth-universal-debuff.test.ts). tsc clean,
466 tests. See [[epic-pool-availability-class-gate]] for the sibling Rebirth-slotting work.

- [x] "Low-value leftovers" were two real bugs + one near-miss (skeptic pass, 2026-06-11)

A skeptic re-investigation of the three items the log had filed as "cosmetic / low
priority / can't fix." Two were genuinely-wrong player-facing bugs with clean fixes;
the third was already correct AND verifying it caught a bug I nearly introduced.

**① Fire Imps / Gremlins P-hash display — FIXED (broad).** The log claimed the 1+1
P-hash shape was "indistinguishable from a rain (Rain of Arrows), where merging would
double-count" → unmergeable. **Wrong** — the discriminator was in the data the whole
time: each P-hash carries its OWN `priority_list`. Fire Imps' `P1757360070.priority_
list == "Pets_FireImp_Controller"` (its named siblings) → provably the same pet →
merge. Rain of Arrows' `P4047293352.priority_list == "Pets_RainofArrows_Visual"` ≠ its
sibling `Pets_RainofArrows` (visual vs static object) → no match → untouched.
New `resolvePhashSiblings` ([convert-powerset.cjs](scripts/convert-powerset.cjs), runs
after `normalizeSummonEntities`) merges a P-hash into a named sibling ONLY when its
`priority_list` exactly equals that sibling's `entity_def`. Result: Fire Imps →
`Pets_FireImp_Controller ×3`, Gremlins → `×2` (Controller `Pets_Gremlin_Controller`,
Dominator `Pets_Gremlin`) — the garbage `P…` entity gone for **every Fire/Electric
Controller + Dominator**. Exactly 3 generated files; Decoy (already merged by the FX
path) and Rain of Arrows byte-identical. Guard: 3 new cases in
[multipet-summon-count.test.ts](src/data/multipet-summon-count.test.ts) incl. a
Rain-of-Arrows-stays-split regression.

**② Soul Extraction — FIXED (was showing NO pet).** Summons ONE Ghost whose tier
matches the sacrificed Undead henchman; the binary lists all three tiers as separate
EntCreates, each gated by a henchman-identity `requires` (HC: `MastermindPets_Lich
target.VillainName>`; Rebirth Parse6: `arch target> Class_Boss_Henchman eq`). The
converter's blanket `.VillainName>`/NPC-gate drop discarded all three → the power
rendered no summon at all. New `rebuildTierConditionalSummon` detects the
player-henchman tier-gate shape (both server encodings) and rebuilds the summon as
**mutually-exclusive variants** — a new `SummonEffect.mutuallyExclusive` flag
([power.ts](src/types/power.ts)); the 3 tier ghosts at count 1 each. Displays
([InfoPanel.tsx](src/components/info/InfoPanel.tsx),
[PowerInfoTooltip.tsx](src/components/info/PowerInfoTooltip.tsx)) render
"Summons 1 of (tier matches henchman)" and SUPPRESS the summed "Total … DPS" (which
would imply you get all three). Both servers, 2 generated files. Guard in the multipet
test (3 tiers + `mutuallyExclusive`, no inflated `entityCount`).

**③ Inexhaustibility — VERIFIED CORRECT; near-miss bug avoided.** Mid-investigation I
thought the set's `bonuses: []` was a *missing* always-on Heal/End/Regen bonus (the
`Set_Bonus.Challenge_Set_Bonus.Inexhaustibility` power grants Heal 2.0 / +End 0.10 /
+Regen 2.0). Verifying before emitting (GAME-DATA-PRINCIPLES §12) showed that power has
`activate_period=10`, `chance=0.5` — it's a **periodic proc, not an always-on bonus**.
Emitting those as a static set bonus would have over-counted a +2.0 Regen that only
ticks 50%/10s. So `bonuses: []` is **correct**, `proc:true` is **correct**, and the
proc is already captured ([proc-data.ts](src/data/proc-data.ts) +
[proc-residual-effects.ts](src/data/proc-residual-effects.ts), category `Special` like
every other bespoke Rebirth self-proc). The piece's binary `display_name` is itself an
unresolvable P-hash message ID (`P3179408089`), so the curated `REBIRTH_PIECE_PATCHES`
name is the only source — the "recognise the Set_Mode shape" idea can't name it either.
**No change to the data; the curated patch is the right call.** Rationale recorded in
[extract-rebirth-io-sets-v2.py](scripts/extract-rebirth-io-sets-v2.py) so it isn't
"fixed" into a bug later. (The old ⬜ "Set_Mode special-piece not recognised" entry is
closed by this.)

tsc clean; 458/458 tests (was 455 + 3 multipet). See [[pseudo-pet-resolution]],
[[proc-piece-name-misresolution]].

- [x] regen-all now refreshes pools/epics — orchestrator dry-run fixed, stranded converter drift landed (2026-06-11)

**Root cause (as diagnosed).** `scripts/regen-all.cjs` ran `convert-pool-powers.cjs`
and `convert-epic-pools.cjs` with `args: []`, but both DRY-RUN unless given `--apply`.
So `npm run regen:generated` refreshed powersets but **never**
`generated/power-pools.ts` / `epic-pools.ts`, and CI's `regen-diff` guard inherited the
blind spot — the documented root of every "pool/epic output is stale, hand-applied /
deferred" note in this log (CopyBoosts, stealth `stackKey`, specialBuff, accuracy…).

**Fix.** Added `--apply` to those two STEPS in [regen-all.cjs](scripts/regen-all.cjs)
(+ a comment noting *why* they need it where the others don't). The orchestrator now
genuinely regenerates the pool/epic layers, so CI's regenerate-and-diff will catch
pool/epic drift from here on.

**The stranded drift, reviewed and landed.** The first real apply surfaced **2264
insertions / 320 deletions** across the 4 generated files (HC + Rebirth × pool + epic;
epic-heavy as predicted). Bucketed the entire diff by JSON key and confirmed **every**
change maps to a converter improvement already shipped + validated on powersets,
values preserved — no regressions:
- **`slow` movement-extraction** (+164 `slow`, + `fly`/`runSpeed`/`jumpHeight`/
  `jumpSpeed`/`movementControl`/`movementFriction`): immobilizes/holds carrying a
  `Ranged_Slow` table now split their movement-suppression `slow` out (previously
  dropped) while **keeping** `rechargeDebuff` at the same scale/table/duration — the
  pre-`slow`-extraction `rechargeDebuff → slow` restructuring the old entry warned of,
  confirmed additive not destructive.
- **Offensive knockback** (+105 `knockback`, +14 `knockup`, +6 `repel`): the
  2026-06-04 "KB dropped from all attacks" fix reaching pool/epic attacks (Boxing 2.68,
  Spring Attack knockup 4, epic blasts 1.34…).
- **`copyBoosts`** (+51): the 2026-06-11 second-flags-word fix on pool/epic summons.
- **Pseudo-pet entity resolution** (`entity` +7/−7): P-hash → named
  (`P2832274689 → Pets_Enflame_Pet`, etc.).
- **IgnoreStrength re-keys** (`recoveryBuff`/`tohitBuff` → `…Unenhanced`, +8/−8 with
  values identical): the 940d89dbb generalization. These + the unchanged
  `defenseDebuff`/`tohitDebuff`/`specialDebuff`/`maxTargets` values (whose containers
  merely gained a new `slow`/`knockback` sibling) are why the per-key add/remove
  tallies net to zero — renames and sibling-additions, not value changes.

**Verified.** A second `regen-all --generated-only` run is **idempotent** (identical
2264-line diff, zero further churn) and confirms the *other* generated layers
(powersets/incarnate/salvage/archetypes) were already current — only the 4 pool/epic
files change. tsc clean; **455/455 tests** incl. `converter-invariants` (no new
`*_PvPMez`/`0xFFFFFFFF` leaks from the regen).

**Follow-up (not done here — belongs to the override campaign).** Now that the
pool/epic generated layer is current, any `overrides/power-pools.ts` /
`overrides/epic-pools.ts` entries that were pinning a value the converter now produces
natively are candidate **dead pins** — worth an MSOT-4-style audit pass, but left
untouched to keep this diff attributable. See [[adversarial-remediation-campaign]].

- [x] Stealth radius binary-sourced stacking groups — `stack_key` carried, suppress-group max model (2026-06-11)

**Symptom.** Celerity / Unbounded Leap +Stealth IO procs didn't count toward the
stealth-radius dashboard stat (DP/NIN Sentinel report). The proximate cause was a calc
gap (no proc `Stealth` handler in `applySingleProcEffect`), but fixing it surfaced the
deeper model question: stealth radius was aggregated **max-wins**, then (interim)
**pure-additive**, and *neither* matches the game.

**The binary rule (verify-don't-assume, §12).** Every StealthRadius template carries a
`stack` mode + `stack_key`. Across all 103 HC player stealth templates there are exactly
**two** behaviors (`exported_powers/live/**`):
- `stack="Suppress"` + `stack_key="NictusFX"` (**30 powers**, 12 leaf names: Pool Stealth/
  Super Speed/Invisibility, Cloak of Darkness, Energy Cloak, Shinobi-Iri, Shadow Cloak,
  Steamy Mist, Arctic Fog, Cloaking Device, Kyokan) — one mutual-suppression group, only
  the **largest** radius applies.
- `stack="Replace"` + `stack_key=null` (Stalker Hide, Grant Invisibility, Smoke Flash, pet
  stealth — AND the IO procs, a separate group) — **additive**.

  **Correction (verified 2026-06-12):** **Mask Presence** (Night Widow / Fortunata) was
  previously listed here as a NictusFX suppress member — it is NOT. Every Mask Presence
  template is `stack="Replace"`, `stack_key=null` in the binary → it stacks **additively**,
  like Hide. (The generated *data* was always correct — the converter only carries
  `stackKey` on `Suppress` templates — only this prose was wrong; count was 32, is 30.) The
  authoritative membership is the binary's 12 leaves above; trust the binary, not the list.

This reproduces the game exactly: pool Stealth (55) alone is invis-capped; Super Speed
(35) + pool Stealth (55) → max **55** (don't stack, both NictusFX); Super Speed (35) +
a Celerity IO → **65** (the famous combo — the IO is a separate group, so it adds).

**Fix.**
- **Converter** ([convert-powerset.cjs](scripts/convert-powerset.cjs), shared by the
  pool/epic converters): carry `template.stack_key` onto `effects.stealth.stackKey`, but
  ONLY when `stack==='Suppress'` AND the key is resolved (≠ `4294967295` 0xFFFFFFFF
  sentinel, ≠ `0`).
- **Calc** ([character-totals.ts](src/utils/calculations/character-totals.ts)):
  `StealthContribution` + `resolveStealthRadius` — active powers and procs *collect*
  contributions tagged by `stackKey`; the commit (one **max** per keyed group + **sum**
  of null-key) runs once after procs are gathered.
- **Type:** `StealthEffects.stackKey` ([power.ts](src/types/power.ts)).

**Materialized.** HC regen = 29 powerset files + 3 pool powers gain `stackKey:
"NictusFX"`, nothing else (regen-idempotent). The 3 pool keys (Pool Stealth/Super Speed/
Invisibility) were **hand-injected** into `generated/power-pools.ts` because regen-all
dry-runs pools — see the ⬜ regen-all entry above — to avoid bundling ~2400 lines of
unrelated stale-pool drift.

**Rebirth Parse6 gap (cross-server, deferred).** Parse6 resolves **neither** the stack
mode nor the key for stealth — it reports `stack="Replace"`, `stack_key="4294967295"`
for even pool Stealth. So the converter's `Suppress`+resolved-key guard correctly falls
Rebirth through to **pure additive** (no `stackKey` emitted): a documented limitation —
Rebirth over-counts builds running 2+ suppress-group stealth powers. See
[[rebirth-assets-and-parse6]], [[stealth-stacking-model]].

> **RESOLVED 2026-06-12 — see the dedicated entry at the top of this log.** This was
> NOT a parser-droppable field: Parse6's `stack_key` is a *per-power integer* and
> structurally cannot express HC's *global* string key "NictusFX" (confirmed — the
> string exists only on one unrelated NPC FX power in Rebirth's binary; Thunderspy's
> Parse7 `coxg/bin.pigg` carries it, Rebirth's Parse6 can't). Fixed by re-applying the
> HC-oracle membership (12 leaf names) in the converter — a no-op on HC.

**Guard:** [stealth-procs.test.ts](src/utils/calculations/stealth-procs.test.ts) — 9
tests incl. "Shinobi-Iri (35.5) + Super Speed (35) = 35.5, not 70.5" (suppress group)
and "+ Celerity = 65.5" (IO adds on top). tsc clean.

- [x] Multi-pet summon counts — Phantom Army (6→3) + Gang War (dropped→9) fixed (2026-06-11)

`normalizeSummonEntities` ([convert-powerset.cjs](scripts/convert-powerset.cjs),
runs after the effect loop) corrects the two genuine count bugs the flat
per-template EntCreate handler got wrong:

- **Phantom Army** (internal "Decoy", Controller+Dominator) carried its 3 staggered
  decoys in TWO effect groups with **complementary `@CustomFX Mirror` requires**
  (`… ||` vs `… || !` — a visual branch, only one fires). The handler counted both
  → `P998401764 ×2 + Pets_Decoy ×4` (6). Now FX-deduped to one branch + the P-hash
  first-decoy merged into the named pet → **`Pets_Decoy ×3`** (Dominator:
  `Pets_Decoy_Dominator ×3`).
- **Gang War** is 13 `Pets_Thug_Pose_01..09` (one Thug, cosmetic poses) firing at
  `chance 1.0×6, .75×2, .5×2, .25, .10×2`. On HC its EntCreates sit in
  `activation_effects` with `IgnoreStrength`, so the buff-drop filter discarded the
  whole summon (it showed **no pets at all**). Now rebuilt from powerJson, poses
  collapsed to `Pets_Thug_Pose_01`, count = chance-weighted expected value →
  **9** (Rebirth 10; per-server pose chances + 120s duration). Expected-value
  matches the planner's proc convention; *worth an in-game spot-check.*

**Strictly scoped to be rain-safe and regression-safe.** The normalizer is a no-op
unless complementary FX-variant groups OR `_Pose_NN` variants are present, and only
rebuilds a *dropped* summon when poses let it collapse correctly. So:
- Rains/location pseudo-pets (Rain of Arrows, Whirlpool — a single rain as
  P-hash+named) have none of these patterns → untouched (no double-count).
- Level/tier-gated MM henchmen (Battle Drones, Soul Extraction, Call Thugs, Zombie
  Horde, …) are left EXACTLY as before — recounting them would mis-model their
  level-gated/tier-conditional counts (Soul Extraction would wrongly show 3 tiers).
- The 1+1 P-hash shapes (Fire Imps, Gremlins) are NOT merged (their counts were
  already correct; the P-hash is a cosmetic display split — see the ⬜ leftover).
- Attacks with an incidental single EntCreate (Necromancy Dark Blast → a specter,
  Thugs Pistols → a pose) are below the 2-instance threshold → still dropped.

Verified: a full HC powerset regen changes exactly 3 files (Decoy ×2 ATs +
Gang War); Rebirth changes Gang War only (Phantom Army's Parse6 shape lacks the FX
pattern → no-op). Guard: [multipet-summon-count.test.ts](src/data/multipet-summon-count.test.ts)
(incl. a Soul-Extraction-not-inflated regression assertion). tsc clean, 446 tests.

- [x] Pseudo-pet `summon.powers` redirect chains resolved (~32 powers, path C) — log reconciled 2026-06-11

This was completed across seven follow-ups (Storm Cell / Category Five reference
cases first, then full generalization to both datasets) — the `⬜` entry here was
just never flipped. Full implementation history, worked examples, in-game-verified
numbers, and the remaining smaller gaps live in
**PSEUDO-PET-POWER-RESOLUTION.md** (no longer in the tree).

Summary of what shipped (verified present + committed 2026-06-11):
- **Resolver** `resolveSummonRedirects` + `collectTemplatesWithChance` +
  `attachResolvedPseudoPets` in [convert-powerset.cjs](scripts/convert-powerset.cjs):
  walks the redirect graph (`Execute_Power` + cycle-guarded `Create_Entity` hops),
  reuses `collectTemplatesDeep` (AT-conditional dedup, PvP exclusion, storm-mode
  de-double-count), and synthesizes `summon.resolvedEntities` at convert time so the
  runtime display path is unchanged. Scoped to a vetted set of location-shell
  entity_defs (`PSEUDOPET_SHELL_ENTITIES`) that are absent from `PET_ENTITIES` →
  double-count-safe; collapse bug fixed (Category Five keeps BOTH pseudo-pets by
  entity_def + redirect-list signature).
- **Runtime** `calculateResolvedPseudoPetDamage` (pet-damage.ts) scales off the
  summoner's AT table; `synthesizePseudoPetEffects` honors the IgnoreStrength
  enhanceable/display-only split; mode swaps (Storm Cell "High Winds" toggle →
  WindSpeed debuffs + Strong lightning; Oil Slick "Ignited" toggle) wired through.
- **Coverage:** ~90 HC generated files carry `resolvedEntities` (Storm Cell,
  Category Five, Tar Patch, Faraday Cage, Carrion Creepers, the Trick-Arrow patches,
  Static Field, Tesla Coil, Tide Pool, Meteor, Vines, Geode, Freezing Rain, Burn,
  Voltaic Sentinel, Lightning Rod, …). The P-hash→`priority_list`/`Pets_`-fallback
  family (Rain of Fire, Glue Arrow, Trip Mine, Ice Storm, Sleet, Liquefy, Bonfire)
  keeps the existing pet-entity path. **Cross-server:** HC Parse7 uses the
  shell+redirect pattern (resolver handles it); Rebirth Parse6 inverts it
  (`entity_def=Pets_*` real pet, shell in `priority_list`) so the existing pet path
  already covers it → Rebirth regen = 0 changes.
- **Guard:** `pseudopet-redirect.test.ts` (in-game-verified scales) +
  `pseudopet-effects.test.ts` — both green (38 cases).

Both follow-ups from that list were closed. Burn's Fiery-Embrace variant split is
covered: the base patch (`Fire 0.14`) and the FE-active `fieryburn` (
0.14 + 0.063) are pinned in `pseudopet-redirect.test.ts`, and the
chance-0 FE duplicate no longer bumps the count. Voltaic Sentinel's "secondary
bolt component" was disproven: the real pet's two extra 0.213 strikes are
gated on a critter/PvE RPN (`enttype target> critter/player eq`) AI-side
effect, correctly dropped — the player-facing 0.56 bolt matches CoD2. The
base-aura face-value AoE fuzziness was a display-truth, not a bug.

- [x] Rebirth `is_pvp` — Phalanx Fighting ally scaling restored; Parse6 ally-buff vs PvP-split disambiguated (2026-06-11)

**Root cause (confirmed against the Rebirth `.pigg`, now on the machine).** Parse6
has **no explicit `is_pvp` flag** — the parser *synthesizes* it from each
AttribMod's RPN `requires` (`_parse_effects_parse6`). The old heuristic mapped any
`target> player eq` → `PVP_ONLY`. Phalanx's per-ally +Def increment (`scale 0.3`,
`Stack`, **target=Self**) carries
`entref target> entref source> eq ! enttype target> player eq &&` =
**"target ≠ self AND target is a player"** — i.e. *count nearby OTHER players*
(the ally-scaling clause), **not** a PvP combat gate. So convert-powerset dropped
it from the PvE planner and Rebirth Phalanx generated a flat +Def with no ally
scaling. (HC reads an explicit `is_pvp=EITHER` for the same template, which is why
HC was fine.)

**The discriminator (verify-don't-assume).** The genuine PvE/PvP split is
**foe-targeted** (`target=AnyAffected`) and ships as a **pair**: a `critter eq`
copy (PvE) + a `player eq` copy (PvP) of the same effect — Force Bubble's
Repel/Knockback is the canonical example, and it must stay split. The ally-buff is
**Self-targeted** with the **self-exclusion clause** and has **no `critter`
sibling**. Probing the whole Rebirth binary: of the `PVP_ONLY` `player eq`
templates carrying the self-exclusion clause, exactly **9 are `target=Self`** — the
3 Phalanx Fightings (Brute/Scrapper/Tanker × melee/ranged/area) — vs 26
`AnyAffected` (the legitimate foe-repel splits). No `target=Self` + self-exclusion
+ `critter` template exists, so widening the pattern is safe.

**Scope correction.** The original "~165 powers" was a loose estimate (any
`PVP_ONLY` self-`Stack` increment lacking a PvE copy). The *precise* mis-classified
pattern — Self-targeted ally-counting self-buff — is **only Phalanx Fighting**
(3 ATs). The other `PVP_ONLY` self-`Stack` increments without PvE copies are
genuine offensive/foe effects, correctly classified.

**Fix.** [_powers.py](tools/bin-crawler/bin_crawler/parser/_powers.py)
`_parse_effects_parse6`: a Self-targeted AttribMod whose `requires` contains the
self-exclusion clause `entref target> entref source> eq !` is a proximity/ally
self-buff → classify **EITHER** (the `player`/`critter` sub-clause is an ally-type
filter, not the combat split). Flips exactly the 9 Phalanx increments; Force Bubble
et al. untouched (`PVP_ONLY` 7599→7590).

**Materialized.** A fresh Rebirth re-export diff was **`is_pvp`-only on exactly the
3 Phalanx files** (after ignoring a pre-existing stale-export `tags: None → []`
drift — see below), so applied the change surgically: `PVP_ONLY → EITHER` in the 3
committed `exported_powers/rebirth/.../phalanx_fighting.json` files (9 lines),
regenerated the 3 shield-defense powersets. Phalanx now generates `perTarget: 0.3`
ally scaling (**matching HC's `perTarget 0.3`**) + `maxStacks: 2` from the binary
`stack_limit`. **HC vs Rebirth encode this differently:** HC has no `Stack`
template (it bakes `perTarget` into the base, no `maxStacks`); Rebirth uses a
separate `Stack` increment with `stack_limit=2`, faithfully rendered as
`maxStacks: 2` — a real per-server data difference, not an artifact.

**Deferred (separate):** the committed `exported_powers/rebirth` tree is **stale
vs the current parser** — it carries `tags: None` where a fresh export emits
`tags: []` (~7800 files would churn). That's the `tags` parser change never having
been re-exported for Rebirth; unrelated to this fix. Left for a dedicated
"re-export Rebirth to current parser" pass; applied only the 3 Phalanx files here
to keep the diff attributable.

**Guard:** [rebirth-phalanx-ally-scaling.test.ts](src/data/rebirth-phalanx-ally-scaling.test.ts).
tsc clean, 442/442 tests pass. See [[rebirth-assets-and-parse6]].

- [x] Summon `copy_boosts` binary-sourced — second AttribMod flags word decoded; Discharge overrides retired (2026-06-11)

**Root cause.** AttribMod flags are stored across **two** consecutive u4 words.
`flags_raw` (the first) was decoded by `_FLAG_BITS`; the **second** word — holding
the copy/pet keywords (`CopyBoosts`, `PseudoPet`, `CopyCreatorMods`) — was
swallowed whole into the opaque Params tail (`_extract_params` heuristic scan).
So `template.flags` never contained `CopyBoosts`, `summon.copyBoosts` was always
`undefined`, and the converter's `template.flags.includes('CopyBoosts')` at
[convert-powerset.cjs:2682](scripts/convert-powerset.cjs) had nothing to read.
(The earlier guess that CopyBoosts was a missing `_FLAG_BITS` bit was wrong —
Omega_Maneuver's `flags_raw=0x70` decodes its three Ignore flags correctly and
has no CopyBoosts bit; the flag is in the *next* word.)

**Decode + validation (verify-don't-assume, §12).** Probes in `c:\tmp\copyboosts-probe*.py`
matched ~14.5k binary templates to their `.powers` `Flags` keyword sets via a
`(table, scale, magnitude)` signature, then ran per-bit precision/recall on the
second word. Clean bits (perfect recall, fn=0):
- **`0x4` CopyBoosts** (tp=408; also **415/415** on an independent entity_def-keyed
  cross-check; 688 EntCreate templates lack the bit, so it's a real discriminator)
- `0x20` PseudoPet (tp=68, fp=0, fn=0)
- `0x8` CopyCreatorMods (tp=33, fn=0)
The first word's decode reproduced `_FLAG_BITS` exactly (IgnoreStrength=0x10,
IgnoreResistance=0x20, IgnoreCombatMods=0x40, CancelOnMiss=0x4, NearGround=0x8;
fp=0) — confirming the field alignment is right.

**Fix (Parse7/HC).** [_powers.py](tools/bin-crawler/bin_crawler/parser/_powers.py):
read `flags2_raw = r.read_u4()` right after `flags_raw`, decode via new
`_FLAG2_BITS`/`_decode_flags2`, merge into `flags`; store the raw word on
`EffectTemplate.flags2_raw` (new dataclass field) so the rest of the word can be
decoded later without a re-export. Consuming the word causes **zero**
param-extraction drift (`c:\tmp\verify-tail-shift.py`: 0/2472 EntCreate templates
change entity_def/redirects/priority_list — the param scan never depended on it).
Only `CopyBoosts` is emitted for now: **`PseudoPet` is validated but deferred** —
emitting it flips `summon.isPseudoPet` on ~68 powers, changing Power-Info display
(`🐾 Summons` → `⚡ Creates`); that belongs with the pseudo-pet resolution work,
not this fix. `CopyCreatorMods` is unconsumed downstream. Both bits are documented
in `_FLAG2_BITS` (commented out) for a trivial future enable. Lower bits (0x1/0x2)
didn't map cleanly to any tested keyword; left undecoded.

**Materialized (HC).** Re-exported, applied the **CopyBoosts-only** delta to the
committed `exported_powers/` (1045 files; semantic diff verified copyBoosts-only,
0 other changes, 0 missing — no data drift). Regenerated HC **powersets**
(`convert-all-powersets --force`): 285 generated files, every diff is
`copyBoosts: true` (+ the trailing comma on the preceding line), nothing else.
**Deferred:** `epic-pools`/`power-pools` regen — their committed output predates
the foe movement-`slow` extraction fix, so regenerating them surfaces unrelated
stale-converter drift; left for a dedicated "bring pool layers current" pass (the
Discharge deliverable is in powersets, which was already current).

**Override retirement.** With `copyBoosts` now generated, the controller +
corruptor **Electrical Affinity Discharge** overrides went `DIVERGENT → DEAD_PIN`;
retired via the campaign's `override-retire.mjs` (dropped the dead `effects` pin,
kept the text fields). Audit tally moved **9 → 7 DIVERGENT** exactly as predicted
(the 2 Discharges are now TEXT_ONLY). See [[adversarial-remediation-campaign]].

**Parse6 (Rebirth) not done — and not needed.** The Parse6 template parser doesn't
decode `flags` at all (always `[]`), and no Rebirth override depends on
`copyBoosts`, so Rebirth pets still don't get the flag. Tracked here; do it if a
Rebirth summon's pet DPS is reported wrong.

**Guard:** [summon-copyboosts.test.ts](src/data/summon-copyboosts.test.ts).
tsc clean, 439/439 tests pass.

- [x] Archetype defs binary-sourced from classes.bin — HP curves/caps + baseThreat + damageCap, both servers (2026-06-06)

Campaign leg #2, done in three phases. The `classes.bin` parser (`_classes.py`) now
reads the class struct's CharacterAttributes region (not just `named_tables`),
anchoring on the `hit_points` curve and reading siblings at fixed byte-deltas
per format (`_ATTRIB_LAYOUT["parse7"|"parse6"]`).

- **Phase 1:** per-level HP curve, HP-cap curve, baseHP/maxHP, resistanceCap.
  Caught a stale HC Brute HP table (1499 → binary 1606.3451; HC buffed it,
  Rebirth keeps 1499).
- **Phase 2:** `baseThreat` — a class-**header** float at `hit_points_anchor−4040`
  (Parse7) / `−4004` (Parse6). Caught the hand-port's wrong Rebirth **Guardian**
  threat (1.0 → binary 2.0; header alignment confirmed identical to every other
  AT, so not a misread).
- **Phase 3:** `damageCap` — L50 of the first damage-type **StrengthMax** curve
  (`dmg_cap_delta` 74872 Parse7 / 30944 Parse6). Caught the hand-port
  under-capping **Scrapper/Tanker/Sentinel/Corruptor/Stalker at 400% when HC has
  them at 500%** (and Rebirth Brute at 7.0 when it's 7.75). Verified vs the HC
  2020-01-23 Tanker/Brute patch notes (Tanker 400→500%, Brute 775→700%) + forum
  (Scrapper 500%). **Per-server authoritative:** HC Tanker 500/Brute 700, Rebirth
  Tanker 400/Brute 775. NB the cap has a *second, stale* pre-2020 copy in the HC
  record (≈delta 82092) — the parser reads the live block, not that one. The
  initial "damageCap not in classes.bin" read was a false negative (searched the
  stale hand vector; the cap is stored as a rising per-level curve, not a flat
  array).

The remaining scalars (`damageModifier`, `buffDebuffModifier`) aren't single
binary quantities — they're abstractions over the per-category `named_tables`,
which ARE binary-sourced and current and which the calc already prefers (these
scalars are table-less *fallbacks* only). See [[at-modifiers-are-binary-tables]].

Pipeline: parser `attribs` → `export_classes.py` → `convert-archetypes.cjs` →
`archetype-stats.generated.ts` → spread into each `archetypes.ts`. Guarded by
`src/data/archetype-stats.test.ts` (60 tests, both datasets, incl. baseThreat +
damageCap). `threat_delta` is a *negative* delta (header sits before the anchor),
so a future HC header-field insertion would shift it — the sane-range guards + CI
test catch a gross misread. Full write-up: **ARCHETYPE-DEFS-BINARY-SOURCING.md** (no longer in the tree).

- [x] `perception` / `knockback_strength` set bonuses modeled (2026-06-06)

`perception` set bonuses were emitted by the source but dropped by the calc (absent from
`STAT_NAME_MAP`). Now mapped (`perception → perceptionradius`), routed through
`character-totals.ts` to the `perceptionRadius` global, and given a Set-Bonus-popup group
(`set-bonus-groups.ts`, "General"). `knockback_strength` is explicitly mapped to `null`
(intentionally ignored — KB *magnitude* bonuses aren't a tracked player stat; KB
protection/resistance already are). Merged via `fix/io-set-bonus-followups`.

- [x] Pseudo-pet summon entities resolve from priority_list (Glue Arrow et al.) (2026-06-06)

Location / rain / patch powers (Glue Arrow, Rain of Fire, Blizzard, Caltrops, Sleet,
Tornado, Lightning Storm, Ice Patch, Jack Frost, Phantasm, …) deal their damage + debuffs
through a summoned pseudo-pet. The HC binary stored the EntCreate `entity_def` as an opaque
P-hash, so `PET_ENTITIES[summon.entity]` never resolved and these powers showed no Damage
block / no debuffs (the Glue Arrow report — DoT(Toxic) / -spd / -rech / -fly / -jump flags
present but no effects). convert-powerset.cjs now resolves the P-hash to the `priority_list`
pet name (Pets_StickyArrow_Blaster, Pets_RainofFire, …), but ONLY for single-entity summons
(post-effect-loop) so multi-pet template-count summons are untouched (0 entityCount changes).
83 HC powers fixed; diff is purely the `summon.entity` line. Rebirth's export already
resolves entity_defs (no-op there). Guard: `pseudopet-summon-entity.test.ts`. Residual gaps
(HC exporter root cause, Phantom Army count) tracked in the unresolved section above.

- [x] IO sets are binary-sourced for both servers; bonuses fixed (2026-06-05)

`io-sets-raw.ts` (HC + Rebirth) now generates from `boostsets.bin` + `powers.bin` via
`extract-rebirth-io-sets-v2.py --dataset <id>`; dead `convert-io-sets.js` +
`extract-rebirth-io-sets.cjs` retired. Subsumes the old "generators drop Absorb" gap —
the Heal/Absorb attribs flow from the binary correctly.

Verifying the data (GAME-DATA-PRINCIPLES §2) caught two bug classes the prior trial missed
(it compared only aspect *sets* + bonus tier *counts*):
- **Bonus stat keys / value scaling.** The binary's `cold_resistance` / `maxhp` /
  per-type-mez keys aren't in `normalizeStatName` → the calc silently dropped them (≈196
  entries on Rebirth-only sets). Rewrote to canonical keys + per-attrib value multiplier
  (damage ×250, max HP ×10, max end ×1, else ×100; flat ×100 was 10× off on max HP),
  paired-stat de-dup, family collapses.
- **Piece effective-aspect count.** Recovered from the enhancement scale
  (`= multiAspectModifier(count) × rarity`) instead of name/aspect-list counting — fixes
  LotG +Recharge (2 aspects) and ATO "#6" proc pieces (4). Proc detection via chance/ppm;
  negative-scale Strength templates excluded as proc debuffs (dropped spurious Slow/KB).

Validated tier-by-tier vs the trusted HC hand-data: only diffs are 20 damage display-
rounding fixes + 1 Aegis psi/tox de-dup, 0 aspect-count regressions. Durable mechanics in
[GAME-DATA-PRINCIPLES.md](../GAME-DATA-PRINCIPLES.md) §11; guarded by `io-sets-bonus-keys.test.ts`
+ `io-sets-heal-absorb.test.ts`. tsc clean, 110/110 tests.

- [x] Offensive knockback was dropped from ALL attacks (FIXED 2026-06-04)

Root cause of the kheldian "drops knockback" symptom: in `extractEffects`
([convert-powerset.cjs](scripts/convert-powerset.cjs)) the knockback handler had
`if (!isSelfTargeting) continue;` — which dropped **every foe-targeted knockback**,
i.e. the offensive KB that attacks apply (Energy Blast, Storm, Nova/Dwarf, all
knockdowns). The regen-diff guard stayed green because every powerset regenerated
*consistently* without it; the long-stale `kheldian-form-variants.ts` was the lone
canary that still carried the correct (old-converter) knockback.

Fix: in the foe-targeted branch, **emit offensive KB** (positive-magnitude `Current`)
and **skip only protection/reduction** — KB applied to *protect* the foe from being
knocked (immobilize -KB: Stone Cages / Freeze Ray, encoded as `aspect=Resistance` +100
paired with `aspect=Current` −100 on `*_Ones`). The discriminator is principled, not a
threshold: offensive KB is positive Current; protection is resistance-aspect **or**
negative scale. Restored **~1030 knockback/knockup/repel effects across 888 generated
files** (verified: power-burst 4, Wormhole 14, Tremor knockdown 1.34, Bright Nova Blast
1 == committed; zero mag-100 leaks; Stone Cages clean). Powers whose conditional effect
*was* knockback (Storm Blast In-Storm-Cell, etc.) also correctly materialize that
conditional and get descriptive labels. tsc clean, 84/84 tests.

**Remaining (deferred to the `.powers` extraction audit):**
- **Foe -KB protection** (immobilize "can't be knocked") is now correctly *excluded*
  from offensive KB but **not modeled** — there's no `PowerEffects` field for foe-applied
  KB protection, and the game folds it into "Immobilize". Per the completeness decision
  (2026-06-04) it should be modeled as its own effect; doing so after the audit.
- **`kheldian-form-variants.ts` itself** was left reverted (not regenerated): a regen
  carries additional *unvetted* accumulated converter deltas (a `tohitBuff` 0.5 removed,
  a `rechargeDebuff`/`Ranged_Slow` 0.2 added) from being long-stale. These need
  source-verification — exactly the audit's job — before the file is regenerated.
- `homecoming/kheldian-form-variants.ts` is dead output (`InfoPanel` imports the
  **rebirth** map unconditionally); make the lookup dataset-aware when modeling resumes.

- [x] Rebirth Blaster ToHit-buff AT modifiers were stale (FIXED 2026-06-03)

`rebirth/at-tables.ts` carried Homecoming's Blaster ToHit-buff base modifiers (0.10)
instead of Rebirth's rebalanced `Melee_Buff_ToHit` 0.075 / `Ranged_Buff_ToHit` 0.07
(verified against `exported_powers/rebirth/tables/blaster.json`; HC source is 0.10, so
it's a genuine Rebirth divergence, not a parse bug). The planner was overstating Rebirth
Blaster ToHit buffs (Aim/Tactics/Build Up). Regenerated; no other AT table drifted.
Surfaced by the full `npm run regen` while validating the regen-diff guard.

- [x] Powerset/pool deep regen — converter `specialBuff` regression fixed, layers brought current (FIXED 2026-06-03)

Brought the stale `generated/powersets` + `generated/power-pools` layers current for
both datasets, materializing the accuracy + mez-PvE fixes and every other accumulated
converter improvement — after root-causing and fixing the converter regression that
blocked it.

**The blocker (now fixed):** a full regen emitted a **malformed bare `specialBuff`**
(`{ scale, table, perTarget }` instead of the keyed `{ <statKey>: … }`) for a handful
of powers (entropy-shield, beta-decay, Dual Blades combos, geode), breaking `tsc` and
threatening to drop those powers' buffs. Root cause: **`classifyTemplateForStacking`**
(stacking/perTarget patch keys) routed **`RechargeTime aspect=Strength`** to
`specialBuff`, but `extractEffects` keeps recharge on the flat `rechargeBuff` key — so
the perTarget patch was mis-keyed onto the `specialBuff` container and corrupted it
(and would have lost e.g. Entropy Shield's real +recharge-per-foe buff). Fix in
[convert-powerset.cjs](scripts/convert-powerset.cjs): (1) exclude `RechargeTime` from
the blanket `strength → specialBuff` in `classifyTemplateForStacking` (mirrors the
existing damage/accuracy exclusions, so it falls through to `rechargeBuff` and matches
`extractEffects`); (2) defensive guard in `mergeStackingPatches` — never apply a flat
patch to a keyed container (`specialBuff`/`specialDebuff`); the dropped `perTarget` is
calc-irrelevant there (`collectStrengthBuffs` uses `stacksLinear`/`maxStacks`, not
`perTarget` on specialBuff values). Plus the `accuracyDebuff` type gap (Geode's self
`-Accuracy`, raw `-999`) — added to `PowerEffects` + effect-registry.

**What shipped (verified):**
- **HC powersets:** 100 generated files — 54 `specialBuff` consolidation (strength
  buffs into the keyed container the calc consumes), 22 sentinel `4294967295 → -1`,
  5 `accuracyBuff` (Combat Training: Offensive, Eagle Eye, Terra Firma, Beryl Crystals,
  Targeting Drone…), the PvE-mez fixes, etc.
- **Rebirth powersets:** 52 files (sentinel + accuracy, mostly).
- **Pools:** Rebirth `power-pools` gained the accuracy fix; HC pools already current.
- **Safety checks:** `tsc` clean, **0** bare-`specialBuff`, **0** silently-lost buffs
  (scripted check: no buff key removed without a `specialBuff` replacement),
  entropy-shield & beta-decay restored to exact HEAD (recharge preserved), 79/79 tests.

**✅ Bio-armor root-fixed (the recurring naming problem) — module identifiers now
derive from the internal name.** The persistent bio-armor breakage was a symptom of a
latent fragility affecting ~1,234 powers whose internal name ≠ display name: the
generated **export const was derived from the mutable *display* name** (`power.name`),
while the file is named from the stable *internal* name. For bio-armor the two are
*crossed within the set* (internal "Adaptation" displays "Evolving Armor", and vice-
versa) **and** its display name keeps getting corrected (pigg/override history) — so
each correction flipped the export const, and `convert-powerset` scaffolded the
composed per-power files *only-if-missing*, leaving their imports stranded.

Fix ([convert-powerset.cjs](scripts/convert-powerset.cjs)): (1) derive the export const
**and** the index imports from `power.internalName` (= the file name) instead of the
display name — so file name === export name, stable forever, regardless of display-name
changes; (2) always-regenerate the composed per-power files (verified 100% mechanical,
0 hand-edits across 6,170 files — hand-edits live in the parallel override files), so a
rename can never strand them again. The display name now lives only in the power's
`name` data field, freely overridable without touching module structure.

This was a one-time re-derivation of export consts across the whole powerset layer
(~1,200 crossed powers × generated/composed/index, both datasets). **Audit:** zero
hand-written imports of power export consts exist (only the auto-regenerated
composed/index/kheldian layers consume them), so nothing external broke. **Verified:**
`tsc` clean, 79/79 tests, 0 bare-`specialBuff`, 0 lost buffs, non-crossed powers'
composed files byte-unchanged, bio-armor now `adaptation.ts → export const Adaptation`
(file = export) with "Evolving Armor" in `.name`. Bio-armor is included in this regen.

---

- [x] Focused Accuracy missing its +Accuracy self-buff (FIXED 2026-06-03)

**✅ RESOLUTION (2026-06-03).** Root cause confirmed exactly as diagnosed: the raw
template `Accuracy / aspect=Strength / Melee_Ones / scale 0.2` (= +20%) was
dropped by the converter and unhandled by the calc. Key correction to the
original plan: Accuracy is inherently a **Strength-aspect stat** (all 40 in-game
Accuracy templates are aspect=Strength — there is no Current variant), so for
Accuracy `aspect=Strength` IS the normal +Accuracy buff, NOT a Power-Boost-style
amplifier. It therefore routes to `accuracyBuff` → `global.accuracy`, **not**
`specialBuff` (the plan's suggested strength→specialBuff routing would have been
wrong). Fix, three layers + override:
1. **Converter** ([convert-powerset.cjs](scripts/convert-powerset.cjs)) — added
   `'accuracy': 'accuracy'` to `COMBAT_MODIFIERS`, an `else if (modType ===
   'accuracy')` emit branch (resistance → `debuffResistance.accuracy`, neg →
   `accuracyDebuff`, else → `accuracyBuff`), and an accuracy carve-out in the
   stacking-metadata helper so it maps to `accuracyBuff` not the specialBuff
   strength container.
2. **Calc** ([character-totals.ts](src/utils/calculations/character-totals.ts)) —
   added an `effects.accuracyBuff` handler right after `tohitBuff` that adds the
   scaled value × 100 into `global.accuracy` (additive with set bonuses). **No**
   enhancement multiplier — accuracy enhancements boost attack-roll accuracy, not
   a buff power's own +Accuracy. Effect-type decls added to `power.ts`, `stats.ts`,
   and the `ActivePowerEffect` interface; display entry added to
   [effect-registry.ts](src/data/core/effect-registry.ts) (`+Accuracy`, no
   `enhancementAspect`).
3. **Data** — materialized via a **deliberate epic-pools regen of both datasets**
   (`node scripts/convert-epic-pools.cjs --dataset {homecoming,rebirth} --apply`).
   The full diff was classified per-power: HC = 22 changed (10 `accuracyBuff`
   additions + 12 PvE-mez fixes — see the ✅ mez note below), Rebirth = 12 (all
   `accuracyBuff`). No other categories changed, no regressions. The earlier
   stop-gap `accuracyBuff` overrides were **removed** — the generated layer now
   owns the data (covers Focused Accuracy, Targeting Drone, Personal Force Field).

**Verified:** [focused-accuracy.test.ts](src/utils/calculations/focused-accuracy.test.ts)
asserts FA toggled on adds exactly +20% to `globalBonuses.accuracy` (0 when off),
now resolved through the generated layer. tsc clean, 79/79 tests pass.
Alpha-incarnate accuracy path is unaffected (special-cased).

**Not done:** powerset/pool accuracy powers (Combat Training: Offensive, Eagle Eye,
Terra Firma, …) — see the ⚠️ entry at top.

- [x] Epic-pools PvE-mez staleness (FIXED 2026-06-03, via the regen above)

The committed `epic-pools.ts` was built before the converter's "prefer the PvE mez
table over PvP" fix (`convert-powerset.cjs` ~line 2244: `*_PvPMez` tables have no
PvE AT-table entry, so a hold/sleep/stun whose duration is `scale × table` silently
showed **no duration** when the old "higher-magnitude-wins" tiebreaker picked the
PvP template). **Verified correct:** each affected power's raw carries *both* a PvE
template (e.g. `Ranged_Sleep`, scale 12) and a PvP one (`Ranged_PvPMez`, mag 4) —
`collectTemplatesDeep` finds both and the fix now keeps the PvE one. Materialized by
the regen above (12 HC powers: Flash Freeze→Sleep, Stalagmites→Stun, Netherworld
Grasp→Immobilize, etc.). Rebirth was already current (Parse6, 0 PvPMez powersets).

---

<details><summary>Original diagnosis (2026-06-03, pre-fix)</summary>

**Reported:** User in-game build showed Focused Accuracy granting +ToHit but
no +Accuracy (expected ~+20%). General/Offense tab Accuracy total only
reflected the Hecatomb 4pc set bonus (+15%).

**Symptom in data:** Every Focused Accuracy variant in
`src/data/datasets/<dataset>/generated/epic-pools.ts` has a `tohitBuff` effect
but no `accuracyBuff`. Affected powers (HC):

- `Epic.Body_Mastery.Focused_Accuracy` (Scrapper)
- `Epic.Body_Mastery_Stalker.Focused_Accuracy`
- `Epic.Brute_Mace_Mastery.Focused_Accuracy`
- `Epic.Corruptor_Mace_Mastery.Focused_Accuracy`
- `Epic.Energy_Mastery.Focused_Accuracy` (Tanker)
- `Epic.Energy_Mastery_Brute.Focused_Accuracy`
- (likely more — grep `"name": "Focused Accuracy"`)

Note: the `"accuracy": 1` field on each is the power's own to-hit multiplier
for its attack roll (neutral), not a self-buff — a different concept.

**Root cause — three layers:**

1. **Converter drops the attrib.** `scripts/convert-powerset.cjs:876-884`
   defines `COMBAT_MODIFIERS` (`tohit → toHit`, `range → range`, etc.) but has
   no entry for `accuracy`. When the bin parser emits a template with attrib
   `Accuracy` (HC index 84, Rebirth index 85 — see
   `tools/bin-crawler/bin_crawler/parser/_enums.py:242` and `:308`), the
   converter has no branch for it and silently discards the effect.

2. **Calc engine has no handler.** `src/utils/calculations/character-totals.ts:889`
   reads `effects.tohitBuff` and adds to `global.toHit`. There is no parallel
   `effects.accuracyBuff` branch feeding `global.accuracy`. Set bonuses
   already populate `global.accuracy` via `STAT_TO_GLOBAL` at line 327, which
   is why Hecatomb's +15% shows up but no power-driven contribution can.

3. **Data is regenerable but currently empty for this effect.** Even after
   fixing the converter, existing `generated/epic-pools.ts` files won't pick
   up the change until the user re-runs `node scripts/convert-epic-pools.cjs
   --apply` against the .pigg archives.

**Likely also affected:** Other powers that grant flat +Accuracy as a
self-buff (not +ToHit) — check Targeting Drone (`Pool.Devices.Targeting_Drone`
or similar), any incarnate Alpha slot that emits an `accuracy` attrib
template (Alpha is special-cased at `character-totals.ts:2463` — that path
works), and any boost-style accuracy buffs in Epic pools.

**Recommended path forward:**

1. **Converter fix** (`scripts/convert-powerset.cjs`):
   - Add `'accuracy': 'accuracy'` to the `COMBAT_MODIFIERS` map (line ~877).
   - Add an `else if (modType === 'accuracy')` branch in the combat-modifiers
     section (~line 2479) that emits `effects.accuracyBuff = makeEffect();
     recordDuration('accuracyBuff');`. Mirror the `tohitBuff` branch shape;
     handle `aspect === 'resistance'` → `debuffResistance.accuracy` and
     `aspect === 'strength'` → `specialBuff.accuracy` for parity.

2. **Calc engine fix** (`src/utils/calculations/character-totals.ts`):
   - Right after the `tohitBuff` block at line 889, add a parallel
     `effects.accuracyBuff` block that resolves the scaled value, multiplies
     by `100`, applies any relevant enhancement multiplier (the in-game
     allowedEnhancements on FA is `ToHit` only — accuracy enhancements don't
     slot the toggle's buff, so likely no enh multiplier; double-check
     against Mids' behavior before deciding), and adds to `global.accuracy`
     with a breakdown entry.
   - Also extend the effect type declaration in
     `src/utils/calculations/power-stats.ts` and any Power type that lists
     buff-effect keys.

3. **Data regen / override:**
   - Preferred: re-run `node scripts/convert-epic-pools.cjs --apply` against
     a current `.pigg` set to repopulate `generated/epic-pools.ts` for both
     `homecoming/` and `rebirth/` datasets. This picks up the converter fix
     for every affected power in one pass.
   - Fallback if raw data is unavailable on the current machine: add entries
     to `src/data/datasets/<dataset>/overrides/epic-pools.ts` keyed by
     `fullName` that merge in an `accuracyBuff` effect. Approximate values
     from the in-game tooltip: scale ~0.2 on `Melee_Buff_ToHit` (verify
     against CoD2 or in-game numbers; FA is roughly +20% Accuracy /
     +7% ToHit at base for Tanker/Scrapper). Verify duration / aspect match
     the existing `tohitBuff` shape.

4. **Verification checklist:**
   - Toggle FA on a Tanker / Scrapper / Brute / Corruptor build and confirm
     the Acc total in the Offense panel and General/totals tab gains the
     expected +20% (or AT-modulated value).
   - Confirm the Power Info popup for FA now lists both ToHit and Accuracy.
   - Spot-check Targeting Drone and any Alpha-incarnate accuracy contribution
     for regressions.
   - Add a regression test under `src/utils/calculations/__tests__/` (mirror
     existing tohit-buff tests) that asserts `accuracyBuff` flows into
     `globalBonuses.accuracy`.

**Files involved:**

- `scripts/convert-powerset.cjs:876` (COMBAT_MODIFIERS map)
- `scripts/convert-powerset.cjs:2450` (combat-modifier emit block)
- `src/utils/calculations/character-totals.ts:889` (tohitBuff handler — add
  accuracyBuff sibling)
- `src/utils/calculations/character-totals.ts:327` (STAT_TO_GLOBAL — already
  maps `accuracy`, no change needed)
- `src/data/datasets/homecoming/generated/epic-pools.ts` (regenerate)
- `src/data/datasets/rebirth/generated/epic-pools.ts` (regenerate)
- `tools/bin-crawler/bin_crawler/parser/_enums.py:242,308` (attrib mapping —
  already correct, FYI)

</details>

---


---OLD ISSUES---

- [x] Rebirth Inexhaustibility — no-rarity `boostsets.bin` record variant (FIXED 2026-06-02)

**Symptom.** Inexhaustibility (Secret Master 5th Column TF/SF reward, single-piece special that slots only into the Rest inherent) shipped with empty `pieces: []` and `bonuses: []`, so it couldn't be slotted. The other three Rebirth Challenge Enhancements (Liberty's Belt, Imperial Might, Forced Indoctrination) all use the standard layout and extract cleanly.

**Root cause (decoded from the binary).** Record #152 (rec_len=316) is a **no-rarity variant**: it omits both the rarity AND category strings, so the `power_count` u4 sits directly in the rarity slot. The parser read those bytes as a bogus empty pascal string, desynced, then read the first power string as `power_count` (garbage > rec_len) and the `> rec_len` guard bailed → empty record. Decoded contents:
- allowed power: `Inherent.Inherent.Rest`
- piece: `Boosts.Attuned_Inexhaustibility_A.Attuned_Inexhaustibility_A` — a `Set_Mode`/aspect=Strength special with no enhancement stat
- bonus: min 1 / max 6 → auto-power `Set_Bonus.Challenge_Set_Bonus.Inexhaustibility`, gated by an embedded `PowerBoostsSlotted 1 >=` requires expression
- effect (`powers.bin`): a Rest-only proc — 3 effect groups @ 50% chance: Heal (2.0×Melee_Heal), +Endurance (Current 0.10), +Regeneration (Current 2.0 / 10.25s). Proc-like and tied to Rest; does **not** fit the planner's flat `bonuses[].effects[]` (stat+value) model.

**Fix (two parts).**
1. **Parser fidelity** — [_boostsets.py](tools/bin-crawler/bin_crawler/parser/_boostsets.py): added `_rarity_is_present()` (rarity tags are always alphabetic; the no-rarity variant has a u4 whose content byte is `0x00`). When absent, skip the rarity+category strings and read `power_count` from that slot. Generalizes to any future no-rarity record. Verified safe: of 233 Rebirth sets, exactly 1 (Inexhaustibility) is no-rarity; all 232 normal records parse identically (Jaunt still 28 powers / 3 boostlists / 2 bonuses). Inexhaustibility now extracts its allowed power + piece + bonus min/max. (The non-standard bonus-block tail still leaves `auto_powers` empty — acceptable, since the effect is proc-like and isn't modeled as a set bonus.)
2. **Data** — [io-sets-raw.ts](src/data/datasets/rebirth/io-sets-raw.ts): hand-added the single proc-special piece (`aspects: []`, `proc: true`, `unique: true`) so Inexhaustibility is **slottable into Rest**. Left `bonuses: []` — the Rest-buff is a power-effect proc, not a flat set bonus, so fabricating a stat+value bonus would mislead the SetBonusDisplay (which renders `{stat}: +{value}%`). Zero drift to other sets.

**Not done:** surfacing the Rest-proc effect numerically. The planner doesn't model "while resting" procs; the set is now functional (slottable) and labeled `Rest Buff`.

- [x] Non-kExpression template `delay` field offset (FIXED 2026-05-21)

**Symptom.** Pet/pseudopet summon powers shipped with no `summon.duration` because the bin parser was reading `delay = 0` for every Silent_Kill template (the AttribMod that despawns the pet after a set time). Affected 11 player Click powers with EntCreate templates whose own Duration is 0 (Haunt, Hell on Earth, Dark Extraction, etc.). Without `summon.duration`, the perma tracker eligibility check skipped these powers, and Power Info couldn't show the pet's stay-alive time.

**Root cause.** `_parse_effect_template` in [tools/bin-crawler/bin_crawler/parser/_powers.py](tools/bin-crawler/bin_crawler/parser/_powers.py) had two parallel layouts for the post-magnitude field block:

- `kExpression` (typ_raw==3): `dur_expr_tokens (u4_array), mag_expr_tokens (u4_array), delay (f4)` — correct
- everything else: `delay (f4), dur_expr (string), mag_expr (string)` — **wrong**

Both branches actually use the kExpression layout in the binary. The non-kExpression branch happened to look right for templates with no Delay AND no expressions — the dur_expr/mag_expr token-array counts were 0, the parser read those 4 bytes as delay (got 0.0), then read the next two 4-byte slots as string offsets (got empty strings). The moment a template had a real Delay value (227 Silent_Kill templates across HC), the parser interpreted the dur_expr count (0) as the delay and the actual delay bytes as a stray string offset, silently dropping the value.

This is the same kind of bug the kExpression branch had originally; that one was fixed by reordering tokens-before-delay.

**Fix.** Merged both branches into one path that always reads tokens-then-delay. Verified against 227 Silent_Kill templates — all now match wiki-known pet lifespans (Pets_Shade 60s, Pets_Living_Hellfire 90s, Pets_Mastermind_Ghosts 300s, Pets_Warshade_Extraction 200s, etc.).

**Downstream wiring.**
- [scripts/convert-pet-entities.cjs](scripts/convert-pet-entities.cjs) extracts the Silent_Kill delay per pet entity by scanning its bundled `Self_Destruct` power (signature: target=Self, stack=Stack, table=Melee_Ones, no EntCreate params — needed because the binary's enum index 117 collision labels Silent_Kill as `Create_Entity`). Surfaces it on `PetEntity` as `lifespan?: number` and writes a CommonJS-friendly sidecar at `src/data/datasets/<id>/pet-lifespans.json` (33 HC entries).
- [scripts/convert-powerset.cjs](scripts/convert-powerset.cjs) loads the sidecar and falls back to `PET_LIFESPANS[entity_def]` when the summoning power's EntCreate AttribMod has `Duration=0`.

**Broader impact.** This bug affected every non-kExpression template that had a non-zero Delay. 227 templates fixed across HC. Worth a broader re-conversion sweep — anything that consumed `delay` (proc PPM activation gating, delayed-debuff effects, etc.) may have been silently reading 0 where it should have read a real value.

**Parse6 (Rebirth) is unaffected** — its template layout is `table, scale, app, type, delay, ...` (delay read directly after type, no expressions in between). Different parser path, never had this bug.

---

- [x] Pseudopet lifespans (PL_StaticObject / Vines patches) (FIXED 2026-05-21)

**Symptom.** After the Silent_Kill fix above, four player Click powers still shipped without `summon.duration`:

| Power | entity_def | Lifespan |
|---|---|---|
| Gravity Distortion Field (Cont/Dom) | PL_StaticObject | 60s |
| Paralyzing Blast (Cont/Dom) | PL_StaticObject | 60s |
| Vines (Plant Control) | Vines pseudo-pet | 60s |
| Glue Arrow | P4234428342 (P-hash patch) | 30s |

**Why not solved by Silent_Kill fix.** These pseudopets aren't represented as pet entity files (`pets_*.json` / `mastermindpets_*.json`) — they're engine-side primitives that don't have their own bundled power list keyed on the entity record. So the entity-keyed lookup in `pet-lifespans.json` had no entry to consult.

**Root mechanism (turns out it's the same Silent_Kill data, just routed differently).** Each pseudopet's summoning AttribMod populates `params.redirects` with the redirect powers the pseudopet runs — and one of those redirects is a `*.Self_Destruct` power carrying the Silent_Kill delay. Examples:
- Gravity Distortion Field → `Redirects.Gravity_Control.Self_Destruct` (delay 60)
- Paralyzing Blast → `Redirects.Electric_Control.Self_Destruct` (delay 60)
- Vines → `Villain_Pets.Vines.Self_Destruct` (delay 60)
- Glue Arrow → no Self_Destruct in redirects, but `priority_list: "Pets_StickyArrow_Blaster"` names a real pet entity that's in the existing entity-keyed sidecar (delay 30)

**Fix.**
- [scripts/convert-pet-entities.cjs](scripts/convert-pet-entities.cjs) now also produces `src/data/datasets/<id>/self-destruct-delays.json` — a map of fully-qualified Self_Destruct power names to their Silent_Kill delay. Built by a recursive walk over the bin export for every `self_destruct.json`. 43 HC entries.
- [scripts/convert-powerset.cjs](scripts/convert-powerset.cjs) wraps the lookup in a three-stage `resolvePetLifespan` cascade: (1) entity_def in pet-lifespans, (2) any `*.Self_Destruct` in params.redirects → self-destruct-delays, (3) priority_list in pet-lifespans. First hit wins; returns 0 if nothing matches (matches pre-fix behavior).

**Verified live.** All four powers now ship with `summon.duration` set, and the perma-tracker eligibility check includes them.

- [x] Dual Pistols Swap Ammo — per-ammo secondary effects (FIXED 2026-06-04)

**Symptom.** Every Dual Pistols attack showed all three ammo secondary effects at once, always-on in base `effects`: -Def (Standard), -Recharge (Cryo), -Damage (Chemical). In-game these are **mutually exclusive** — the loaded ammo selects exactly one (Standard -Def by default; Incendiary's is a fire DoT, already a damage entry).

**Root mechanism.** Unlike Bio Armor / Staff (`Source.Mode?` gates), Dual Pistols ammo uses **`Effect.Tag` + global-chance-mod**: each attack carries `Lethal`/`ColdDamage`/`ToxicDamage`/`FireDamage` tag groups (the non-Standard ones at `chance 0`), and the ammo toggle flips the tags' fire-chance. There is **no `requires_expression`** — so the conditional-gate path never saw them, and the base collector folded them all in.

**Blocker + parser fix.** The export **dropped the `Tag` field** (the parser read it only to advance the binary, then discarded it). Captured it in `_dataclasses.py` / `_powers.py` / `export_powers.py` and re-exported homecoming (purely additive: +41,222 lines, 0 deletions). Bonus: this activated the converter's dormant tag-aware stacking (`collectTemplatesWithMeta`), correctly separating the **Defiance** inherent (`aspect=Strength`, tagged `Defiance`) from 5 Blaster powers' per-target buffs (Soul Drain, etc.).

**Converter fix (key-based — works on BOTH binary formats).** `extractDualPistolsAmmo` is **key-based, not tag-based**: the ammo secondary lands in base as a fixed effect key per ammo (Standard -Def = `defenseDebuff`, Cryo -Recharge = `rechargeDebuff`, Chemical -Damage = `damageDebuff`), so it moves exactly those keys out of base into mutually-exclusive `swap-ammo` conditionals (`lethalammo` is `defaultActive`). Core effects keep their own keys and **stay in base** (knockback, the self +Def/+Dmg on Hail of Bullets, etc.). Driven by the build-scoped `Swap_Ammo.activeSubPower` via the stance-group system (with a new `defaultOptionId` so Standard applies when no ammo is loaded — even with no Swap Ammo power).

Why key-based and not tag-based: **HC/Parse7 tags the ammo groups, but Rebirth/Thunderspy uses Parse6** (`_parse_effects_parse6`), which has **no `EffectGroup` wrapper at all** — it stores flat AttribMods (each wrapped in a synthetic single-template group), so there's no group-level `Tag`. Both formats land the same effect KEYS in base, so attributing by key fixes **both servers** with no Rebirth re-export and no Parse6 parser work. (The HC `Tag` capture still earns its keep — the dormant tag-aware stacking + the Defiance fix above.)

**Follow-up — RESOLVED (2026-06-05): foe movement-slow now extracted.** The
converter's MOVEMENT block used to `continue` (drop) any non-self movement effect
("enemy slows like Time's Juncture"), so a Slow surfaced only its `-Recharge` half
(`rechargeDebuff`) and the `-Movement` half was lost. Now a foe movement mod on a
`*_Slow` table (or negative/debuff) is emitted as a foe `slow` (`MovementByType`,
**no `selfPenalty`** → the calc treats it as a foe debuff, not a self-slow; the
effect-registry already renders `slow` as "-Speed"). Broad fix — `slow` went from
94 → 1536 across ~676 powers (Ice/Cold/Time/Arachnos/Widow/…) plus Cryo ammo, which
now carries both halves of its slow in the `cryoammunition` conditional. Purely
additive (every other effect key net-preserved; self-penalty Granite-style slows
unchanged).

<!-- Summon `copy_boosts` (found + RESOLVED 2026-06-11) — see the ✅ entry at the
     top of the RESOLVED section. The second AttribMod flags word is now decoded;
     CopyBoosts is binary-sourced and the Discharge overrides are retired. -->

## Deferred

- [ ] Commit the converter input so CI can regenerate + byte-diff

**Shipped now (the lightweight half):** [converter-invariants.test.ts](src/data/converter-invariants.test.ts)
— a structural invariant scan over the **committed `generated/`** data that runs in CI
with **zero raw data**. It locks in the converter-regression classes that have repeatedly
bitten us: export const === `PascalCase(internalName)` (the bio-armor naming saga), no
malformed bare `specialBuff` (the RechargeTime/Strength stacking regression), no unsigned
`0xFFFFFFFF` sentinels, and no NEW `*_PvPMez` tables (prefer-PvE mez fix; a 5-entry
allowlist grandfathers the genuinely-PvP-only powers — scramble-thoughts ×3, arctic-air,
and `Epic.Field_Mastery.Repulsion_Bomb`). Cheap, no repo-footprint cost, catches the
*known* failure shapes at PR time.

**The deferred end-state (the heavy half):** commit the **converter input** —
`exported_powers/` for both datasets (**~233 MB / ~25k files**, vs the 30 MB / 6,176-file
committed `generated/` output) — so CI can run the converters **end-to-end** and byte-diff
the regenerated `generated/` against what's committed. That catches **any** converter drift,
not just the four known classes, and closes two gaps the current setup has:

- **Reproducibility:** today a fresh clone / CI **cannot** rebuild `generated/` — the
  converter input is gitignored, so the strongest possible guard (regenerate-and-diff) is
  impossible. The invariant scan is a proxy for it.
- **Two-machine fragility:** the raw source lives only on the PC + laptop local copies
  (see CLAUDE.md "Source Data"); there is no canonical, versioned input.

**Why it's the full ~233 MB, not a prunable subset.** The exporter is **not** a blind dump
— [`export_powers.py`](tools/bin-crawler/bin_crawler/export_powers.py) already filters
204 → ~60 categories. The 34 player AT/pool/epic categories are the core; the other ~25 are
a *documented, genuinely-consumed dependency closure* the converters dereference:
`*_Aux` (leap/charge hit-data via `Execute_Power`), `Pets`/`Villain_Pets`/`Mastermind_Pets`/
`Kheldian_Pets` (snipe & redirect targets, henchman powers), and the **villain-group block**
(Rularuu, Council, Crey, Rikti, DevouringEarth, CircleOfThorns…) that **Lore incarnate pets
"mimic"**. That last block is why the export is full of enemy types — and it lands in the
committed `incarnate-effects.ts` (both datasets) via `convert-pet-entities.cjs` /
`convert-incarnate-effects.cjs`. So a CI regen needs the **whole closure**, not the 34 AT
categories. (The real over-export is *granularity within* those categories — a whole 302-file
`council/` is pulled for ~3-4 Lore-referenced attacks — but tightening that needs the full
transitive entity/redirect closure, fragile and not worth it.)

**Decision (2026-06-03): commit the full ~233 MB as-is.** Acceptable because GitHub allows
it — no single file approaches the 100 MB hard limit (every power JSON is KB-scale; biggest
is ~44 KB), the ~233 MB total is well under GitHub's ~1 GB soft recommendation and the 2 GB
per-push limit, and our files are tiny JSON, not large blobs. The cost is permanent git-history
weight + slower clones + a ~25k-file working tree — tolerable for the reproducibility win.
*Optional* later shrink, if history weight bites: drop the ~10 unread template fields
(`duration_expression`, `magnitude_expression`, `flags_raw`, `tick_*`…) at export time — the
only prune that preserves regen, but bounded by the "fully replace CoD2" goal that wants some
of them (`suppress_events`, `flags`). git LFS is **not** needed (no large individual files).
