//! Build persistence — the pure, DOM-free half (M3 execution plan step 10, D6).
//!
//! The planner keeps ONE working build per dataset, restored non-destructively when you
//! switch datasets (beta `per-server-builds.ts`). The persisted payload is the envelope
//! [`StoredBuilds`] `{ active_dataset, builds_by_dataset }`, versioned exactly like
//! `app::layout_store`'s layout blob so a future shape change migrates instead of
//! clobbering.
//!
//! **Why no separate slim codec here.** The beta's `build-serialization.ts` strips a
//! `Build` (which snapshots power/enhancement *definitions*) down to identity before
//! storage. The rebuild's [`CharacterState`] already stores identity only — the
//! identity/definition split is built into the model (see `character.rs`), and the calc
//! resolves defs lazily from the [`crate::PowerDatabase`], surfacing a pick the dataset
//! can't resolve through its fail-loud `CalcError` channel (never a silent repair). So
//! persistence stores the full [`CharacterState`] directly; there is nothing to slim and
//! nothing to re-attach on load. The export-slim codec (stripping enhancement icon/name
//! for share links / `.skif`) is a separate, display-stripping concern that lands with
//! M5's share format, not persistence.
//!
//! DOM I/O (the `localStorage` get/set and the boot pre-peek) lives in `app::build_store`
//! so this module stays pure and unit-testable, mirroring how `per-server-builds.ts` is
//! store-free.

use crate::character::CharacterState;
use crate::database::DatasetId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The persisted payload: the last-active dataset plus one working build per dataset.
/// `BTreeMap` (not `HashMap`) so the serialized key order is deterministic — the blob is
/// diffed in tests and written to `localStorage`, both of which want a stable string.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredBuilds {
    /// The dataset whose build was last being edited — chosen as the initial dataset on
    /// boot (beta `bootServerId`) so the right definitions are resident when the build is
    /// first calculated.
    pub active_dataset: DatasetId,
    /// One working build per dataset. A dataset absent from the map has no saved build yet;
    /// [`select_active`] hands back an empty one for it.
    pub builds_by_dataset: BTreeMap<DatasetId, CharacterState>,
}

impl StoredBuilds {
    /// A fresh envelope: Homecoming active, no builds saved.
    pub fn empty() -> Self {
        StoredBuilds {
            active_dataset: DatasetId::Homecoming,
            builds_by_dataset: BTreeMap::new(),
        }
    }
}

/// Versioned wrapper — the actual `localStorage` shape. A tagged enum so a later schema
/// change adds an arm and migrates rather than failing to parse (same pattern as
/// `app::layout_store::StoredDesktopLayout`).
///
/// V1 and V2 hold the same Rust type: what changed between them is the KEY SPACE of the two
/// per-power maps, not the shape, so a V1 blob deserializes and is then re-addressed by
/// [`migrate_v1`].
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "schema_version", content = "data")]
enum StoredEnvelope {
    #[serde(rename = "1")]
    V1(StoredBuilds),
    #[serde(rename = "2")]
    V2(StoredBuilds),
}

/// Serialize the envelope to the versioned JSON string written to `localStorage`.
pub fn encode(builds: &StoredBuilds) -> Result<String, serde_json::Error> {
    serde_json::to_string(&StoredEnvelope::V2(builds.clone()))
}

/// Parse a persisted string back to [`StoredBuilds`]. `None` for anything that does not
/// deserialize cleanly (missing/garbage/corrupt) — the caller keeps an empty workspace,
/// mirroring the layout store's corrupt-falls-back-to-default rule. A single unreadable
/// build fails the whole envelope by design: a typed [`CharacterState`] that will not
/// deserialize is corruption, not a build worth half-restoring.
pub fn decode(text: &str) -> Option<StoredBuilds> {
    match serde_json::from_str::<StoredEnvelope>(text) {
        Ok(StoredEnvelope::V2(builds)) => Some(builds),
        Ok(StoredEnvelope::V1(mut builds)) => {
            for build in builds.builds_by_dataset.values_mut() {
                migrate_v1(build);
            }
            Some(builds)
        }
        Err(_) => None,
    }
}

/// The `:buffpet` suffix a V1 blob's buff-pet opt-in was keyed with, before the opt-ins moved
/// to their own map. Spelled here rather than imported because `coh_math` sits ABOVE this
/// crate: the migration reads a key space that no longer exists, so it cannot ask the live
/// minter what it looks like.
const LEGACY_BUFF_PET_SUFFIX: &str = ":buffpet";

/// Re-address a V1 build's two per-power maps onto [`crate::power_address`] keys.
///
/// **Buff-pet opt-ins migrate, and the build itself is what makes that possible.** A V1 key is
/// `<internalName>:buffpet`; the qualified key needs the owning set, and the build's own
/// selections carry it. Where exactly one selection answers to that internal name the opt-in
/// moves; where none or several do it is dropped, because several IS the collision the
/// qualified key exists to fix and picking one would be the silent mis-binding.
///
/// **Legacy proc overrides are dropped, deliberately.** V1 keyed them on the power's DISPLAY
/// name, which no build record carries — re-addressing needs the dataset, which this module
/// cannot see (it is the pure half; see the module doc). Recovering them by fuzzy-matching a
/// display name is exactly the mis-binding the re-key exists to prevent, and the cost of
/// dropping is bounded: an absent override means enabled + auto, the runtime DEFAULT, so a
/// reverted switch reads as an un-set control rather than as a wrong number.
fn migrate_v1(build: &mut CharacterState) {
    build.proc_overrides.clear();

    let legacy: Vec<String> = build
        .combat
        .per_power_conditionals
        .keys()
        .filter(|key| key.ends_with(LEGACY_BUFF_PET_SUFFIX))
        .cloned()
        .collect();

    for key in legacy {
        let Some(on) = build.combat.per_power_conditionals.remove(&key) else {
            continue;
        };
        let internal_name = &key[..key.len() - LEGACY_BUFF_PET_SUFFIX.len()];
        let mut owners = build
            .all_selected()
            .filter(|selection| selection.internal_name == internal_name)
            .map(|selection| selection.address());
        let addressed = (owners.next(), owners.next());
        drop(owners);
        let (Some(address), None) = addressed else {
            continue;
        };
        build
            .combat
            .power_state
            .insert(format!("{address}{LEGACY_BUFF_PET_SUFFIX}"), on);
    }
}

/// The working build for `dataset`: the saved one if present, else a fresh empty build.
/// The returned build's [`CharacterState::dataset`] is stamped to `dataset` so a build's
/// dataset always agrees with the one actually loaded (beta `selectActiveBuild`, which
/// stamps `activeServerId`) — the guard against a `?serverId=` deeplink landing a build
/// whose stored dataset disagrees with the resident data.
pub fn select_active(builds: &StoredBuilds, dataset: DatasetId) -> CharacterState {
    let mut build = builds
        .builds_by_dataset
        .get(&dataset)
        .cloned()
        .unwrap_or_else(|| CharacterState::empty(dataset));
    build.dataset = dataset;
    // Belt to [`compose`]'s braces: an envelope written by an older build (or a foreign one)
    // could still hold a what-if layer, and a build must never LOAD already simulating.
    build.combat.what_if_buffs.clear();
    // Same reason, other axis: an envelope written before the booster floor landed can hold
    // a +5 on a sub-50 IO — a combine the game refuses — and a build must never LOAD showing
    // a number the game cannot reach.
    for power in build.all_selected_mut() {
        for piece in power.slots.iter_mut().flatten() {
            if piece.holds_refused_booster() {
                piece.boost = 0;
            }
        }
    }
    build
}

/// Fold the current working build back into the envelope under its own dataset, leaving
/// every other dataset's saved build untouched (beta `composePersistedState`). The
/// returned envelope's `active_dataset` becomes the build's dataset — the build being
/// edited is by definition the active one.
pub fn compose(active: &CharacterState, builds: &StoredBuilds) -> StoredBuilds {
    let mut by_dataset = builds.builds_by_dataset.clone();
    let mut stored = active.clone();
    // The what-if team-buff layer is a preview input, and this is the one place a build turns
    // into bytes someone else can open. Dropping it HERE — rather than clearing it on load —
    // is what makes "a shared build cannot carry a hidden +damage" structural: the number
    // never gets written, so there is nothing for a reader to inherit or a banner to warn
    // about (decision 2026-08-01; rejected: persist-with-warning-banner, because a banner is
    // dismissible and the number is not).
    stored.combat.what_if_buffs.clear();
    by_dataset.insert(active.dataset, stored);
    StoredBuilds {
        active_dataset: active.dataset,
        builds_by_dataset: by_dataset,
    }
}
