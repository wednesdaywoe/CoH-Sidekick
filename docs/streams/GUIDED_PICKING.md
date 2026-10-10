---
project: coh-sidekick-next
kind: plan
area: crates
title: Guided free-form picking
id-prefix: GP
thesis: With Level Up mode off, each pick moves the character's level to the next empty pick slot, powers out of reach at that level are dimmed but still pickable, an empty pick slot can be clicked to target it, and a power held without its prerequisite at an earlier level is marked until the build is put back in order.
satisfied-when: GP1..GP7 all [x]
status-ext: [unchecked]
---

# Guided free-form picking

Free-form (Level Up mode off) is the default and stays free: nothing is refused for being early
or out of order. It guides instead, the way Mids' Reborn does outside its own level-up mode.
Level Up mode stays the strict option and is not changed by this work.

Decisions (2026-10-10, user-chosen):
- The level that follows the picks is the character's real `CharacterState::level`, not a
  separate marker. Totals, slot budget and inherents recompute as it moves.
  Rejected: a separate cursor that leaves the level at 50 (keeps totals still, but isn't what
  the user wants the planner to do).
- A pick above the current level lands in the earliest empty slot it can legally fill. This is
  already what `next_pick_level` does.
- Prerequisites come from the power's `requires` expression in the contract, the same data that
  gates picks today. No hand-written table.
- Only a missing or out-of-order prerequisite raises the warning. It shows in two places: on the
  power's card in the by-level grid, and as a red italic name in the power list (Mids shows only
  the latter).
- Clicking an empty pick slot sets the level to that slot's level (Mids' behaviour). After the
  next pick the level returns to the earliest empty slot.
- Level Up mode keeps refusing a pick whose prerequisite is missing.

## Preconditions

- `main` builds and `cargo nextest run --workspace` passes before work starts.
- The homecoming contract carries `requires` on pool powers — checked 2026-10-10:
  `Pool.Fighting.Tough` has `["Pool.Fighting.Boxing","Pool.Fighting.Kick","||"]`.

## Active

- [ ] **GP1** — an order-aware prerequisite check in `coh_data::pick_rules`: for a held power,
      the prerequisite counts only if it is held at a strictly lower pick level. Today
      `requires_met` counts ownership at any level.
      done when: unit tests show Tough at 20 with Boxing at 30 is flagged, Tough at 20 with Kick
      at 18 is clear, and Tough with neither is flagged. These guard the level comparison, which
      nobody would catch by looking at one build.
- [ ] **GP2** — with Level Up mode off, a power whose gate is `PickGate::Closed` is pickable and
      lands at `next_pick_level`. With Level Up mode on it is still refused.
      done when: `row_verdict` tests cover both modes for a closed gate.
- [ ] **GP3** — after a pick with Level Up mode off, the level is set to the earliest empty pick
      slot, or to the maximum level when every pick is filled, in the same commit as the pick
      (one undo step, `inherents::sync` and `granted_powers::sync` run). Removing a power and
      loading a build do not move the level.
      done when: a test of the level-choosing function covers "next empty slot", "a gap left
      behind by an out-of-level pick" (third pick is a level 32 power → level becomes 2), and
      "all picks filled → max level".
- [ ] **GP4** — power-list rows that unlock above the current level are dimmed as a whole row,
      not just the badge tint `is-locked` gives today, and stay clickable.  @unchecked
      done when: in the app at level 4, I can tell at a glance which Archery and pool powers are
      out of reach, and clicking one still takes it.
- [ ] **GP5** — an empty cell in the by-level grid (`PowersByLevel`) can be clicked. The click
      sets the level to that cell's level and highlights the cell. The next pick fills that cell
      if the power is legal there, otherwise the earliest legal empty slot.
      done when: a placement test shows a level-2 power goes into the clicked level-24 slot
      rather than the earliest empty one, and a level-32 power clicked into slot 24 goes to 32.
- [ ] **GP6** — a power held without its prerequisite at an earlier level (GP1) is marked on its
      grid card and shown red italic in the power list. The tooltip names what is missing
      ("needs Boxing or Kick at an earlier level"). Both clear once the build is back in order.  @unchecked
      done when: in the app, I take Tough before Boxing and see both marks; I add Kick at a lower
      level and both clear; I move Kick above Tough and they return.
- [ ] **GP7** — Q/A pass in the web build with screenshots (`qa-check` skill), Level Up mode off,
      covering: a level-1 build picked in order, an out-of-level pick, a slot click, the Tough
      case, and Level Up mode still refusing Tough.  @unchecked
      done when: the screenshots show each case behaving as above, and slotting enhancements
      while picking still works as the level moves down and up.

## Out of scope

- The by-powerset layout of the Powers panel: it has no empty slots to click. The level still
  follows picks there (GP3).
- Any change to Level Up mode beyond keeping its refusal (GP2).
- Fixing an out-of-order build automatically. The planner marks it; the user reorders.
- Moving the level when a power is removed or a build is loaded. Only a pick or a slot click
  moves it.
- Order checks on enhancement slot levels. This plan is about power picks only.
- Set-level gates (`PoolDef.buy_requires`, epic `minLevel`): they are level rules, which
  placement already honours.
