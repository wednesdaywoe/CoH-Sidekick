//! Dataset loading, one strategy per target:
//!   desktop — the bundles are embedded (`include_bytes!`), the app works offline;
//!   web     — the selected dataset's bundle is fetched lazily as an asset.
//! Both decode through the same `coh_data` path; the bytes are gz-sniffed because a
//! static host may transparently gunzip a `.gz` (Content-Encoding) before we see it.

use coh_data::{DatasetId, PowerDatabase};
use std::sync::Arc;

/// The contract's schema descriptor, embedded from the SAME regen the bundles come
/// from. Asserted at every load edge (not just the roundtrip test): a contract whose
/// atom tuple order drifted from this build's decoder must refuse to load — a
/// tuple-tail reorder of the string-typed fields would otherwise decode cleanly and
/// ship wrong data as authoritative.
const SCHEMA_VERSION_JSON: &str = include_str!("../../../contract/schema-version.json");

pub async fn load(id: DatasetId) -> Result<Arc<PowerDatabase>, String> {
    coh_data::database::assert_schema_version(SCHEMA_VERSION_JSON).map_err(|e| e.to_string())?;
    let bytes = bundle_bytes(id).await?;
    let db = if bytes.starts_with(&[0x1f, 0x8b]) {
        PowerDatabase::from_gz_bytes(&bytes)
    } else {
        PowerDatabase::from_bundle_json(std::str::from_utf8(&bytes).map_err(|e| e.to_string())?)
    }
    .map_err(|e| e.to_string())?;
    Ok(Arc::new(db))
}

#[cfg(not(target_arch = "wasm32"))]
async fn bundle_bytes(id: DatasetId) -> Result<Vec<u8>, String> {
    Ok(match id {
        DatasetId::Homecoming => {
            include_bytes!("../assets/contract/homecoming/bundle.json.gz").to_vec()
        }
        DatasetId::Rebirth => include_bytes!("../assets/contract/rebirth/bundle.json.gz").to_vec(),
        DatasetId::Thunderspy => {
            include_bytes!("../assets/contract/thunderspy/bundle.json.gz").to_vec()
        }
        DatasetId::Brainstorm => {
            include_bytes!("../assets/contract/brainstorm/bundle.json.gz").to_vec()
        }
    })
}

#[cfg(target_arch = "wasm32")]
async fn bundle_bytes(id: DatasetId) -> Result<Vec<u8>, String> {
    use dioxus::prelude::*;
    static HC: Asset = asset!("/assets/contract/homecoming/bundle.json.gz");
    static RB: Asset = asset!("/assets/contract/rebirth/bundle.json.gz");
    static TS: Asset = asset!("/assets/contract/thunderspy/bundle.json.gz");
    static BS: Asset = asset!("/assets/contract/brainstorm/bundle.json.gz");
    let url = match id {
        DatasetId::Homecoming => HC.to_string(),
        DatasetId::Rebirth => RB.to_string(),
        DatasetId::Thunderspy => TS.to_string(),
        DatasetId::Brainstorm => BS.to_string(),
    };
    let resp = gloo_net::http::Request::get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(format!("fetch {url}: HTTP {}", resp.status()));
    }
    resp.binary().await.map_err(|e| e.to_string())
}
