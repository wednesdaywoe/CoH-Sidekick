//! SPIKE2 — the WASM boundary between the beta React planner and the rebuild engine.
//!
//! The whole boundary is JSON strings and an opaque handle:
//!   - [`load_dataset`] parses a dataset bundle into a [`DatasetHandle`] that holds the
//!     [`PowerDatabase`] entirely on the Rust side — the heavyweight definitions NEVER
//!     cross into JS.
//!   - [`DatasetHandle::recalculate`] takes a `CharacterState` JSON (the SPIKE4 adapter's
//!     output), runs [`coh_math::recalculate`], and returns the [`coh_math::CalculatedTotals`]
//!     as JSON.
//!   - [`DatasetHandle::project_power`] answers the one question the totals cannot: the
//!     per-power values for a power the build does NOT hold (PROD6C).
//!
//! The public `#[wasm_bindgen]` surface is a thin, wasm-only shell over the
//! target-independent core fns ([`load_database`], [`recalculate_json`],
//! [`project_power_json`]) so the round-trip can be unit-tested natively without a wasm
//! toolchain (wasm-bindgen is a wasm-target-only dependency — see `Cargo.toml`).
//!
//! Fail-loud: every failure edge — schema drift, a corrupt bundle, an
//! unparseable build — returns an error string that the wasm layer surfaces to JS as a
//! thrown `Error`, never a silent default.

use coh_data::PowerDatabase;

/// The contract's schema descriptor, embedded from the SAME regen the bundles come from —
/// the identical assertion the desktop/web app makes at every load edge
/// (`crates/app/src/data_source.rs`). A contract whose atom tuple order drifted from this
/// decoder must refuse to load rather than decode cleanly and ship wrong data as
/// authoritative.
const SCHEMA_VERSION_JSON: &str = include_str!("../../../contract/schema-version.json");

/// Core (target-independent) — parse a dataset bundle into a [`PowerDatabase`].
///
/// The bytes are gz-sniffed (`0x1f 0x8b`) because a static host may transparently gunzip a
/// `.gz` via `Content-Encoding` before the app sees it — the same sniff `data_source::load`
/// does. Returns the error as a `String` so the wasm layer can surface it to JS and native
/// tests can assert on it.
// Consumed by the wasm-only module and by tests; unreferenced only in the bare native lib
// build, which exists solely to host `cargo test`.
pub fn load_database(bytes: &[u8]) -> Result<PowerDatabase, String> {
    coh_data::database::assert_schema_version(SCHEMA_VERSION_JSON).map_err(|e| e.to_string())?;
    if bytes.starts_with(&[0x1f, 0x8b]) {
        PowerDatabase::from_gz_bytes(bytes).map_err(|e| e.to_string())
    } else {
        let json = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
        PowerDatabase::from_bundle_json(json).map_err(|e| e.to_string())
    }
}

/// Refuse a structurally impossible build before the engine reads it.
///
/// [`coh_data::CharacterState::validate`] existed, was tested, and had no production caller
/// anywhere in the workspace — the shape Rule 1 exists to prevent, a
/// guard present in the tree and absent from the flow. This boundary is the one that needed
/// it: `build_json` arrives from JS, on the public website, and `wasm32-unknown-unknown` is
/// `panic=abort`, so a panic deeper in is a dead tab rather than an error the caller can
/// surface.
///
/// It is a narrowing, not a seal. `validate` covers the character level range, the per-power
/// slot budget and the team size; it does not make `coh_math` total over hostile input, and
/// F74 stays open for exactly that.
#[cfg_attr(not(any(target_arch = "wasm32", test)), allow(dead_code))]
fn validated(state: &coh_data::CharacterState) -> Result<(), String> {
    state.validate().map_err(|errors| {
        let listed = errors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; ");
        format!("CharacterState invalid: {listed}")
    })
}

/// Core (target-independent) — deserialize a `CharacterState` JSON, run the engine, and
/// serialize the [`coh_math::CalculatedTotals`] back to JSON. The [`PowerDatabase`] stays in
/// Rust; only the two JSON strings cross the boundary.
pub fn recalculate_json(db: &PowerDatabase, build_json: &str) -> Result<String, String> {
    let state: coh_data::CharacterState =
        serde_json::from_str(build_json).map_err(|e| format!("CharacterState parse: {e}"))?;
    validated(&state)?;
    let totals = coh_math::recalculate(&state, db);
    serde_json::to_string(&totals).map_err(|e| format!("totals serialize: {e}"))
}

/// Core (target-independent) — project ONE power the build need not hold (PROD6C).
///
/// The per-power info surfaces render a power you are only hovering in the picker, which no
/// `all_selected()` walk reaches. This runs the same pipeline `recalculate_json` does — the
/// projection's "final" tier needs the finalized accumulator, so the totals must be computed
/// either way — and returns just that power's `PowerProjection` JSON, so a hover does not ship
/// a whole totals payload across the boundary. `null` when the dataset has no such power, which
/// the caller surfaces rather than papering over (Rule 1).
pub fn project_power_json(
    db: &PowerDatabase,
    build_json: &str,
    powerset: &str,
    internal_name: &str,
    targets_hit: Option<u32>,
) -> Result<String, String> {
    let state: coh_data::CharacterState =
        serde_json::from_str(build_json).map_err(|e| format!("CharacterState parse: {e}"))?;
    validated(&state)?;
    let request = coh_math::projection::PowerRef {
        powerset: powerset.to_string(),
        internal_name: internal_name.to_string(),
        targets_hit,
    };
    let totals = coh_math::recalculate_projecting(&state, db, std::slice::from_ref(&request));
    let projection = totals.power_projection.iter().find(|p| {
        p.power_set == request.powerset && p.power_internal_name == request.internal_name
    });
    serde_json::to_string(&projection).map_err(|e| format!("projection serialize: {e}"))
}

/// Core (target-independent) — the target-rank vocabulary this dataset's gates distinguish,
/// as JSON `[{segment, classes}]` sorted by segment ([`coh_data::target_ranks`]).
///
/// A per-power damage projection needs a target before its rank-forked components resolve
/// (the Scrapper crit rows gate on `arch target> Class_…`), so the beta has to name one —
/// and this vocabulary is where it names one FROM, the same derived list the rebuild's
/// combat panel offers. Without it the beta's only options are a hand-written class token
/// (a game proper noun, Rule 0) or a TS re-scan of the gates (a twin free to drift).
// LIVE on wasm32 -- the `#[wasm_bindgen]` shell below is its caller. The `test` arm was
// dropped from this cfg_attr on 2026-09-26: it was there so a NATIVE test run would warn if no
// test exercised this, which is a good tripwire in a repo with tests and a permanent false
// warning in one without. This repo has never had a crates/*/tests/ directory, so
// `cargo check --all-targets` turned the tripwire into noise. Restore `, test` to the
// `any(...)` when the suite is ported.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn target_ranks_json(db: &PowerDatabase) -> Result<String, String> {
    let ranks = coh_data::target_ranks(db)?;
    serde_json::to_string(&ranks).map_err(|e| format!("target ranks serialize: {e}"))
}

/// The wasm-bindgen public surface. Gated to the wasm target so the native build/test path
/// never links wasm-bindgen; both entry points delegate to the core fns above.
#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::{
        load_database, project_power_json, recalculate_json, target_ranks_json, PowerDatabase,
    };
    use wasm_bindgen::prelude::*;

    /// An opaque handle to a loaded dataset. Owns the [`PowerDatabase`] on the Rust side of
    /// the boundary — the definitions never cross into JS. The JS caller holds the handle
    /// for the app's lifetime (loaded once at boot) and passes it back into `recalculate`.
    #[wasm_bindgen]
    pub struct DatasetHandle {
        db: PowerDatabase,
    }

    /// The stat names a what-if TEAM-BUFF entry may use, as a JSON array of strings.
    ///
    /// Dataset-independent, so it hangs off the module rather than a [`DatasetHandle`]: the
    /// vocabulary is a property of the ACCUMULATOR (which fields it routes and which of those
    /// are accumulations rather than baselines), not of any fork's data.
    ///
    /// Exported so the beta's what-if modal derives its controls from the same answer the
    /// engine's own injection uses. A hand-kept list on the JS side would be a second stat
    /// vocabulary free to drift — the exact shape PROD6A killed.
    #[wasm_bindgen]
    pub fn what_if_vocabulary() -> String {
        serde_json::to_string(&coh_math::what_if::vocabulary())
            .expect("a Vec<&str> always serializes")
    }

    /// Load a dataset bundle (gz or raw JSON bytes) into an opaque [`DatasetHandle`].
    #[wasm_bindgen]
    pub fn load_dataset(bytes: &[u8]) -> Result<DatasetHandle, JsError> {
        load_database(bytes)
            .map(|db| DatasetHandle { db })
            .map_err(|e| JsError::new(&e))
    }

    #[wasm_bindgen]
    impl DatasetHandle {
        /// Recalculate totals for a build. `build_json` is the SPIKE4 adapter's
        /// `CharacterState` JSON; returns the `CalculatedTotals` as JSON. Throws a JS
        /// `Error` on a parse failure rather than returning garbage.
        #[wasm_bindgen]
        pub fn recalculate(&self, build_json: &str) -> Result<String, JsError> {
            recalculate_json(&self.db, build_json).map_err(|e| JsError::new(&e))
        }

        /// Project one power against this build — including a power the build does not hold,
        /// which is what the info tooltip renders while you hover the picker (PROD6C).
        /// Returns the `PowerProjection` JSON, or `"null"` for a ref this dataset has no
        /// power for. `targets_hit` is that power's stacking-slider value (PROD6C-3b); the
        /// surfaces keep it by name, so an unheld power can carry one too.
        #[wasm_bindgen]
        pub fn project_power(
            &self,
            build_json: &str,
            powerset: &str,
            internal_name: &str,
            targets_hit: Option<u32>,
        ) -> Result<String, JsError> {
            project_power_json(&self.db, build_json, powerset, internal_name, targets_hit)
                .map_err(|e| JsError::new(&e))
        }

        /// The target ranks this dataset's gates distinguish, as JSON `[{segment, classes}]`
        /// — the vocabulary a caller picks a `combat.target_class` token from before asking
        /// [`Self::project_power`] for target-resolved damage. Throws when the archetype
        /// catalogue will not parse rather than offering a guessed list (Rule 1).
        #[wasm_bindgen]
        pub fn target_ranks(&self) -> Result<String, JsError> {
            target_ranks_json(&self.db).map_err(|e| JsError::new(&e))
        }
    }
}
