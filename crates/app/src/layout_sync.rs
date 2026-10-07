//! The local half of layout sync (docs/LAYOUT-SYNC-PLAN.md): when this browser's layout was
//! last edited, and its five stored keys as the one document the account holds.
//!
//! **What counts as an edit.** A write that changes a key's stored value, made after the person
//! has pressed a pointer or a key on this page. The app writes layout keys on its own while it
//! loads — the defaults on a first visit, an upgraded save format, a layout matched to the panel
//! roster — and none of those is a choice anyone made. Counting them would give a browser nobody
//! has touched a fresher time than the account's copy, and the untouched default would win.
//! Edits only happen through input, so input is the line.
//!
//! The stamp is the browser's clock in milliseconds, under [`KEY_EDITED`]. No stamp means the
//! layout here was never edited, and any account copy beats it.
//!
//! **Whose layout it is.** [`KEY_USER`] names the account this browser's layout was last synced
//! with, and sign-out clears it. Only a layout that is that account's competes with it on time.
//! One edited signed out, or under another account, loses to the account's copy at sign-in:
//! otherwise hiding panels while signed out, or a second person signing in on the same browser,
//! would overwrite the account with a newer layout that was never its own. Found 2026-10-01 in
//! the first signed-in test, where exactly the first of those happened.
//!
//! **The loop.** [`tick`] runs every [`PAUSE_MS`] from the shell, once the shell has restored the
//! layout. For a newly signed-in user it compares this browser's copy with the account's once
//! and the newer wins whole (decision 2026-10-01, user-chosen). After that it uploads an edit
//! once [`QUIET_MS`] have passed without another, so a burst of drags is one write. Running from
//! the shell, after restore, is what keeps an account copy from being overwritten by the
//! restore's own writes a moment later.
//!
//! **Failures are silent and retried.** The browser copy has already saved before any of this
//! runs, so a failed sync loses nothing here; it is tried again after [`RETRY_MS`]. Reporting it
//! would be a popup per drag while offline.

use crate::cloud::account::Account;
use crate::cloud::layouts;
use crate::{layout_store, stats_store};
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// When this browser's layout was last edited, in milliseconds since the epoch.
pub(crate) const KEY_EDITED: &str = "sk-layout-edited";

/// The account this browser's layout was last synced with. Absent when signed out.
const KEY_USER: &str = "sk-layout-user";

/// The keys that make up a layout. Nothing outside this list syncs (decision 2026-10-01,
/// user-chosen: layout only, not theme or other display preferences).
pub(crate) const KEYS: [&str; 5] = [
    stats_store::KEY_STATS,
    layout_store::KEY_DESKTOP,
    layout_store::KEY_MOBILE,
    layout_store::KEY_QUICKBAR,
    layout_store::KEY_POWERS,
];

/// Arm edit stamping on the first pointer or key press. Called once from the shell root; safe to
/// call again, since the page keeps the flag.
pub fn arm_edit_stamps() {
    dioxus::document::eval(
        "if (!window.__skEditsArmed) {\
            window.__skEditsArmed = true;\
            const arm = () => { window.__skEditsLive = true; };\
            addEventListener('pointerdown', arm, { capture: true, once: true });\
            addEventListener('keydown', arm, { capture: true, once: true });\
        }",
    );
}

/// Run `write` — a script that writes `key` — and stamp the edit time if it changed what was
/// stored and stamping is armed. Every write of a layout key goes through here.
///
/// Compared by content with object keys sorted, not as text. A layout that came from the
/// account went through the database's JSON, which reorders keys, and the app re-saving it in
/// its own order is not an edit.
pub fn commit_tracked(key: &str, write: String) {
    crate::storage::commit(format!(
        "{{\
            const canon = text => {{\
                const sort = v => Array.isArray(v) ? v.map(sort)\
                    : v && typeof v === 'object'\
                        ? Object.fromEntries(Object.keys(v).sort().map(k => [k, sort(v[k])]))\
                        : v;\
                try {{ return JSON.stringify(sort(JSON.parse(text))); }} catch (_) {{ return text; }}\
            }};\
            let before = null;\
            try {{ before = localStorage.getItem({key:?}); }} catch (_) {{}}\
            {write}\
            try {{\
                if (window.__skEditsLive && canon(localStorage.getItem({key:?})) !== canon(before)) {{\
                    localStorage.setItem({KEY_EDITED:?}, String(Date.now()));\
                }}\
            }} catch (_) {{}}\
        }}"
    ));
}

/// The layout as one document: each key's stored value, parsed. What the account row holds.
///
/// Keyed by storage key and carried as plain JSON, so this module never interprets a layout. A
/// key a newer app version adds arrives here as an entry this one does not list, and is ignored
/// on the way in rather than failing the whole document.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LayoutDoc(pub BTreeMap<String, Value>);

/// This browser's layout, when it was last edited, and which account it was last synced with.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalLayout {
    pub doc: LayoutDoc,
    pub edited_ms: Option<i64>,
    pub user: Option<String>,
}

impl LayoutDoc {
    /// The document for these stored strings. Keys outside [`KEYS`] and values that are not
    /// JSON are left out: a corrupt key here is one the app would not load either.
    pub fn from_stored(stored: &BTreeMap<String, String>) -> LayoutDoc {
        LayoutDoc(
            KEYS.iter()
                .filter_map(|key| {
                    let value = serde_json::from_str(stored.get(*key)?).ok()?;
                    Some((key.to_string(), value))
                })
                .collect(),
        )
    }

    /// The writes that put this document into storage: one per key this build knows.
    pub fn to_stored(&self) -> Vec<(&'static str, String)> {
        KEYS.iter()
            .filter_map(|key| Some((*key, self.0.get(*key)?.to_string())))
            .collect()
    }
}

/// Read the answer of [`read_local`]'s script:
/// `{ edited: string | null, user: string | null, keys: { [key]: string } }`.
fn parse_local(answer: &Value) -> Option<LocalLayout> {
    let stored: BTreeMap<String, String> =
        serde_json::from_value(answer.get("keys")?.clone()).ok()?;
    let edited_ms = answer
        .get("edited")
        .and_then(Value::as_str)
        .and_then(|text| text.parse().ok());
    let user = answer
        .get("user")
        .and_then(Value::as_str)
        .map(str::to_string);
    Some(LocalLayout {
        doc: LayoutDoc::from_stored(&stored),
        edited_ms,
        user,
    })
}

/// This browser's layout, or `None` where storage cannot be read at all.
pub async fn read_local() -> Option<LocalLayout> {
    let keys = serde_json::to_string(&KEYS).ok()?;
    let js = format!(
        "try {{\
            const keys = {{}};\
            for (const key of {keys}) {{\
                const value = localStorage.getItem(key);\
                if (value !== null) keys[key] = value;\
            }}\
            return {{\
                edited: localStorage.getItem({KEY_EDITED:?}),\
                user: localStorage.getItem({KEY_USER:?}),\
                keys,\
            }};\
        }} catch (_) {{ return null; }}"
    );
    let answer = dioxus::document::eval(&js).await.ok()?;
    parse_local(&answer)
}

/// Replace this browser's layout with `user`'s `doc`, edited at `edited_ms`. Not an edit made
/// here, so it does not go through [`commit_tracked`]: the time is the copy's own.
pub fn write_local(doc: &LayoutDoc, edited_ms: i64, user: &str) {
    let mut js = String::from("try {");
    for (key, value) in doc.to_stored() {
        js.push_str(&format!("localStorage.setItem({key:?}, {value:?});"));
    }
    js.push_str(&format!(
        "localStorage.setItem({KEY_EDITED:?}, {:?}); }} catch (_) {{}}",
        edited_ms.to_string()
    ));
    crate::storage::commit(js);
    claim_for(user);
}

/// Record that this browser's layout is now `user`'s.
fn claim_for(user: &str) {
    crate::storage::commit(format!(
        "try {{ localStorage.setItem({KEY_USER:?}, {user:?}); }} catch (_) {{}}"
    ));
}

/// Forget whose layout this is, so edits from here on are nobody's until the next sign-in
/// compares them. Called on sign-out, and when a session ends without one.
pub fn forget_user() {
    crate::storage::commit(format!(
        "try {{ localStorage.removeItem({KEY_USER:?}); }} catch (_) {{}}"
    ));
}

/// How often the loop looks.
const PAUSE_MS: u32 = 2_000;
/// How long the layout must go without an edit before it is uploaded.
const QUIET_MS: i64 = 2_000;
/// How long after a failed request the loop tries again.
const RETRY_MS: i64 = 30_000;

/// What to do with two copies of a layout, given when each was last edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Winner {
    Account,
    Browser,
    Neither,
}

/// Which copy to keep. A browser copy that is not this account's (`ours` false) loses to any
/// account copy; between two copies of the same account's layout the newer wins. A browser copy
/// never edited loses to any account copy, and with neither edited there is nothing to store.
fn winner(browser_ms: Option<i64>, account_ms: Option<i64>, ours: bool) -> Winner {
    if !ours && account_ms.is_some() {
        return Winner::Account;
    }
    match (browser_ms, account_ms) {
        (None, None) => Winner::Neither,
        (Some(_), None) => Winner::Browser,
        (None, Some(_)) => Winner::Account,
        (Some(b), Some(a)) if a > b => Winner::Account,
        (Some(b), Some(a)) if b > a => Winner::Browser,
        _ => Winner::Neither,
    }
}

/// Whether an edit at `edited_ms` is due for upload: newer than what was last synced, and
/// [`QUIET_MS`] old.
fn upload_due(edited_ms: Option<i64>, synced_ms: Option<i64>, now_ms: i64) -> bool {
    edited_ms.is_some_and(|edited| Some(edited) > synced_ms && now_ms - edited >= QUIET_MS)
}

fn now_ms() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as i64)
}

/// Wait [`PAUSE_MS`]. Through the page's own timer, which both targets have.
pub async fn pause() {
    let _ = dioxus::document::eval(&format!(
        "await new Promise(done => setTimeout(done, {PAUSE_MS})); return true;"
    ))
    .await;
}

/// Where the loop is, for the user it is syncing.
#[derive(Debug, Default)]
pub struct SyncState {
    user: Option<String>,
    reconciled: bool,
    synced_ms: Option<i64>,
    retry_at: i64,
}

/// One pass of the loop. Answers whether the account's layout was just written into this
/// browser, which the caller must then load into the screen.
pub async fn tick(state: &mut SyncState, account: &Account) -> bool {
    let user = account.user().peek().as_ref().map(|user| user.id.clone());
    let Some(user) = user else {
        // A session that ended here without a sign-out (an expired refresh) still ends the
        // layout's claim, or edits made after it would count as the account's.
        if state.user.is_some() {
            forget_user();
        }
        *state = SyncState::default();
        return false;
    };
    if state.user.as_deref() != Some(user.as_str()) {
        *state = SyncState {
            user: Some(user.clone()),
            ..SyncState::default()
        };
    }
    let now = now_ms();
    if now < state.retry_at {
        return false;
    }
    let Some(local) = read_local().await else {
        return false;
    };
    if state.reconciled && !upload_due(local.edited_ms, state.synced_ms, now) {
        return false;
    }

    let outcome = match account.prepared().await {
        Ok(cloud) if state.reconciled => {
            let edited = local.edited_ms.unwrap_or(now);
            layouts::push(&cloud, &user, &local.doc, edited)
                .await
                .map(|()| (false, Some(edited)))
        }
        Ok(cloud) => reconcile(&cloud, &user, local).await,
        Err(e) => Err(e),
    };
    match outcome {
        Ok((arrived, synced)) => {
            state.reconciled = true;
            state.synced_ms = synced;
            arrived
        }
        Err(_) => {
            state.retry_at = now + RETRY_MS;
            false
        }
    }
}

/// Compare this browser's copy with the account's and keep the newer. Answers whether the
/// account's copy was written here, and the edit time both now agree on.
async fn reconcile(
    cloud: &crate::cloud::Cloud,
    user: &str,
    local: LocalLayout,
) -> Result<(bool, Option<i64>), crate::cloud::CloudError> {
    let account = layouts::fetch(cloud, user).await?;
    let account_ms = account.as_ref().map(|(_, ms)| *ms);
    let ours = local.user.as_deref() == Some(user);
    match winner(local.edited_ms, account_ms, ours) {
        Winner::Account => {
            let (doc, ms) = account.expect("the account wins only when it has a copy");
            write_local(&doc, ms, user);
            Ok((true, Some(ms)))
        }
        Winner::Browser => {
            let ms = local
                .edited_ms
                .expect("the browser wins only when it was edited");
            layouts::push(cloud, user, &local.doc, ms).await?;
            claim_for(user);
            Ok((false, Some(ms)))
        }
        Winner::Neither => {
            claim_for(user);
            Ok((false, local.edited_ms.max(account_ms)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn stored(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn all_five_keys_round_trip_through_the_document() {
        let before = stored(&[
            (
                "sk-stats-config",
                r#"{"schema_version":"2","data":{"panels":[],"next_id":2}}"#,
            ),
            ("sk-layout", r#"{"schema_version":"4","data":{"12":[]}}"#),
            (
                "sk-mobile-order",
                r#"{"schema_version":"1","data":["Powers"]}"#,
            ),
            ("sk-quickbar", r#"{"schema_version":"1","data":[]}"#),
            ("sk-powers-layout", r#""ByLevel""#),
            ("sk-build", r#"{"not":"layout"}"#),
        ]);

        let doc = LayoutDoc::from_stored(&before);
        let after: BTreeMap<String, String> = doc
            .to_stored()
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();

        assert_eq!(after.len(), 5);
        for (key, value) in &after {
            let original: Value = serde_json::from_str(&before[key]).unwrap();
            assert_eq!(
                serde_json::from_str::<Value>(value).unwrap(),
                original,
                "{key}"
            );
        }
        assert!(!after.contains_key("sk-build"));
    }

    #[test]
    fn a_layout_never_edited_here_has_no_time() {
        let answer = json!({ "edited": null, "keys": { "sk-quickbar": "[]" } });
        let local = parse_local(&answer).unwrap();
        assert_eq!(local.edited_ms, None);
        assert_eq!(local.doc.0.len(), 1);

        let answer = json!({ "edited": "1759320000000", "keys": {} });
        assert_eq!(
            parse_local(&answer).unwrap().edited_ms,
            Some(1_759_320_000_000)
        );
    }

    #[test]
    fn a_newer_document_applies_the_keys_this_build_knows() {
        let doc: LayoutDoc = serde_json::from_value(json!({
            "sk-quickbar": { "schema_version": "1", "data": [] },
            "sk-theme-from-the-future": "dark",
        }))
        .unwrap();

        let writes = doc.to_stored();
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].0, "sk-quickbar");
    }

    #[test]
    fn between_copies_of_one_account_the_newer_wins() {
        assert_eq!(winner(None, None, true), Winner::Neither);
        assert_eq!(winner(Some(5), None, true), Winner::Browser);
        assert_eq!(winner(None, Some(5), true), Winner::Account);
        assert_eq!(winner(Some(5), Some(9), true), Winner::Account);
        assert_eq!(winner(Some(9), Some(5), true), Winner::Browser);
        assert_eq!(winner(Some(5), Some(5), true), Winner::Neither);
    }

    #[test]
    fn a_layout_edited_signed_out_loses_to_the_account_however_new() {
        // The 2026-10-01 test: panels hidden signed out, then signing back in.
        assert_eq!(winner(Some(9), Some(5), false), Winner::Account);
        assert_eq!(winner(None, Some(5), false), Winner::Account);
    }

    #[test]
    fn an_account_with_no_layout_yet_takes_the_browsers() {
        assert_eq!(winner(Some(5), None, false), Winner::Browser);
        assert_eq!(winner(None, None, false), Winner::Neither);
    }

    #[test]
    fn an_edit_uploads_once_it_has_gone_quiet_and_only_once() {
        assert!(!upload_due(None, None, 10_000));
        assert!(
            !upload_due(Some(9_000), None, 10_000),
            "still inside the quiet window"
        );
        assert!(upload_due(Some(8_000), None, 10_000));
        assert!(upload_due(Some(8_000), Some(7_000), 10_000));
        assert!(
            !upload_due(Some(8_000), Some(8_000), 10_000),
            "already sent"
        );
    }

    #[test]
    fn a_corrupt_key_is_left_out_rather_than_failing_the_document() {
        let doc = LayoutDoc::from_stored(&stored(&[
            ("sk-quickbar", "not json"),
            ("sk-powers-layout", r#""ByLevel""#),
        ]));
        assert_eq!(doc.0.keys().collect::<Vec<_>>(), ["sk-powers-layout"]);
    }
}
