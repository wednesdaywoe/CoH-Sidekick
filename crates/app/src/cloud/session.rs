//! The session: where the token lives, when it is refreshed, and the one door every call goes
//! through so no call site has to remember either.
//!
//! # The pre-refresh is a rule, not a habit
//!
//! The beta calls `supabase.auth.refreshSession()` immediately before each privileged invoke —
//! five sites, four in `sharedBuilds.ts` and one in `profile.ts`, each with the same comment
//! ("Refresh auth session to avoid 401 from expired JWT") and the same rethrow. That is five
//! places that remembered, and a sixth function added later is a place that will not: six of the
//! eight edge functions read the JWT themselves through `auth.getUser(token)`, so the gateway is
//! refreshing nothing on anyone's behalf.
//!
//! **Ported as a property of the session rather than of the call.** [`SessionManager::client`] is
//! the only way to reach a [`Cloud`], and it refreshes when the session it holds is within
//! [`EXPIRY_MARGIN`] of expiring. Nothing keys on *which* function is being called, which matters
//! for the reason RB4j exists: a list of privileged function names is a population written down,
//! and a population written down stops being recounted. Expiry is a fact the session carries, so
//! a new call site inherits the rule by having no way around it.
//!
//! This is also strictly less traffic than the beta. `refreshSession()` round-trips unconditionally
//! — the belt to `autoRefreshToken`'s braces, which exists because a backgrounded tab has its
//! 30-second refresh timer throttled and can come back holding a dead JWT. Checking expiry at call
//! time covers exactly that case, and covers it whether or not any timer is running, which is why
//! there is no timer here.
//!
//! # A signed-out session is not a session-shaped hole
//!
//! With no session there is nothing to refresh and nothing to check: the request goes out bearing
//! the anon key, because `Cloud::bearer` never sends nothing (see the module doc above). So the
//! anonymous read paths — `get-build`, `backfill-preview` — pass through this door unchanged, and
//! there is no second door for them to take.
//!
//! # The storage key is a compatibility contract — on the web, and only there
//!
//! Everything in this section is a fact about the WASM target. The desktop
//! build is served from `dioxus://index.html` (`dioxus-desktop/src/protocol.rs:15-22`), a fresh
//! origin with no beta `localStorage` to inherit and no cut-over to survive, so none of the
//! reasoning below is load-bearing there. It is applied to both anyway, and that is deliberate
//! rather than an oversight: the key is derived from the project ref, so it is the same string on
//! both targets, and one derivation with no `cfg` in it is worth more than a desktop-only name
//! whose only argument is that it could be different. Stated because a reason that is true of one
//! target and written as though it were true of both is how a later reader talks themselves into
//! changing the wrong one.
//!
//! The WASM build replaces the beta on the same origin, so it inherits the beta's `localStorage`,
//! and `sb-<project-ref>-auth-token` is a third contract alongside the two RB4e names. Write the
//! session under a key of our own and every signed-in beta user is signed out at the cut-over, all
//! at once, with nothing in any log to find it by — and a rollback would not bring them back,
//! because the beta would still be reading the old key.
//!
//! So the key is derived the way `supabase-js` derives it (`supabase-js/dist/index.cjs:202`:
//! `sb-${hostname.split(".")[0]}-auth-token`), and **the stored value is the token endpoint's own
//! JSON, whole**. Only `expires_at` is added, and only when the server did not send it, which is
//! what `auth-js` does too (`lib/fetch.js:126`). Nothing is projected into a struct of ours on the
//! way through: a session we re-serialise from four known fields is a session that has silently
//! lost `user`, `provider_token` and whatever the next `auth-js` adds, and the beta would read the
//! remains as a logged-in user with no identity.
//!
//! # One store, both targets
//!
//! There is no `cfg` here, and that is not a concession to RB4a's grep — it is that the desktop
//! target is a webview whose `localStorage` persists to disk, which is where every other piece of
//! this app's state already lives ([`crate::build_store`], [`crate::theme`], [`crate::layout_store`]
//! and four more, none of them target-split). A config-dir file on desktop would make the session
//! the only state in the app that persists somewhere different from everything else.
//!
//! Writes go through [`crate::storage::commit`] rather than `document::eval` for the reason that
//! module documents at length: a raw `localStorage` write from a popped-out webview silently stops
//! the main window's later writes from ever landing. A session write is exactly the kind that
//! happens while a user has a panel popped out.

use super::{Cloud, CloudError};
use dioxus::prelude::*;
use serde_json::{Map, Value};
use std::time::Duration;

/// How close to expiry counts as expired. 90 seconds, which is `auth-js`'s own `EXPIRY_MARGIN_MS`
/// (`AUTO_REFRESH_TICK_THRESHOLD * AUTO_REFRESH_TICK_DURATION_MS`, `lib/constants.js:13`) and the
/// figure its `_loadSession` compares against. Matching it rather than picking one keeps this
/// client and any beta tab still open agreeing on when a token is spent.
pub const EXPIRY_MARGIN: Duration = Duration::from_secs(90);

/// Why a session could not be read.
///
/// Deliberately carries no copy of the text it failed on, unlike [`CloudError::Decode`]. That text
/// is a session, a session contains a refresh token, and an error string is a thing that gets
/// logged, rendered and pasted into a bug report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionError {
    /// A field a session cannot work without was absent or not a string.
    Missing(&'static str),
    /// Not JSON, or JSON that is not an object. Carries serde's message, never the input.
    Malformed(String),
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing(field) => write!(f, "session has no {field}"),
            Self::Malformed(detail) => write!(f, "session is not readable: {detail}"),
        }
    }
}

impl std::error::Error for SessionError {}

/// A session, kept as the JSON it arrived as.
///
/// The three fields this client acts on are read back out through the accessors; everything else
/// is carried untouched so what is written back is what `supabase-js` would have written. See the
/// module doc on why that is a contract rather than laziness.
#[derive(Clone, PartialEq)]
pub struct Session {
    raw: Map<String, Value>,
}

/// Redacted by hand, because every value in `raw` is a credential or sits beside one.
///
/// The derive printed the whole blob, so a single `{:?}` — a log line, a
/// panic message, an error type that came to carry a session — would have written a live
/// `access_token` and `refresh_token` wherever that went. Nothing formats a `Session` today, and
/// that is exactly what made this Info rather than a leak: it is a door left open in a room
/// nobody is in yet. Closing it now costs an impl; closing it after the first `{:?}` costs
/// finding the line that printed it.
///
/// **The field NAMES are printed and no value is.** The names are schema — the one thing this
/// type is ever debugged for is which fields the endpoint actually sent — and the values are the
/// secret. `expires_at` is the single exception, because a second count is not a credential and
/// it is the field a refresh bug is read through.
///
/// Discord's `provider_token` and `provider_refresh_token` cannot appear here at all:
/// [`Session::from_value`] strips them at the one door every session comes through. This impl
/// does not rely on that — it names nothing, so it cannot start leaking a field added later.
impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field(
                "fields",
                &self.raw.keys().map(String::as_str).collect::<Vec<_>>(),
            )
            .field("expires_at", &self.expires_at())
            .finish_non_exhaustive()
    }
}

impl Session {
    /// Read a session that was already stored — by us, or by the beta before the cut-over.
    pub fn from_stored(text: &str) -> Result<Self, SessionError> {
        Self::from_value(parse_object(text)?)
    }

    /// Read the token endpoint's answer, stamping `expires_at` when the server did not send one.
    ///
    /// `now` is seconds since the epoch. Passed in rather than read here so the stamping rule is
    /// gradeable without a clock.
    pub fn from_token_response(value: Value, now: u64) -> Result<Self, SessionError> {
        let Value::Object(mut raw) = value else {
            return Err(SessionError::Malformed("not a JSON object".to_string()));
        };
        if !raw.contains_key("expires_at") {
            let expires_in = raw
                .get("expires_in")
                .and_then(Value::as_u64)
                .ok_or(SessionError::Missing("expires_in or expires_at"))?;
            raw.insert(
                "expires_at".to_string(),
                Value::from(now.saturating_add(expires_in)),
            );
        }
        Self::from_value(raw)
    }

    /// The provider's own credentials, which this app holds onto for nobody.
    ///
    /// `provider_token` is a Discord access token and `provider_refresh_token` renews it. GoTrue
    /// returns both from the OAuth code exchange, and the rule above this one - keep the token
    /// endpoint's JSON WHOLE - carried them into `localStorage` and kept them there. **Nothing in
    /// either client reads either field.** The Supabase session that signs every request is
    /// `access_token`; these are for calling Discord on the user's behalf, which neither client
    /// does.
    ///
    /// So they were a credential at rest for a call nobody makes - and one this app **cannot
    /// revoke**, because revoking a Discord token needs the OAuth client secret, which lives in
    /// Supabase's provider config and reaches no client. A credential that cannot be retired is
    /// one to not hold in the first place.
    const NOT_OURS_TO_KEEP: [&'static str; 2] = ["provider_token", "provider_refresh_token"];

    /// The one door every session comes through - a fresh exchange, a refresh, and a blob read
    /// back off disk - which is why the exclusion sits here rather than at the exchange.
    ///
    /// Putting it here means an already-stored session is cleaned the next time it is written,
    /// with no migration to run and nothing to remember.
    ///
    /// **This is an exclusion, not a projection, and the distinction is this module's whole
    /// rule.** Keeping the endpoint's JSON whole exists because re-serialising from the fields we
    /// happen to know about silently loses whatever `auth-js` adds next. Two fields named here,
    /// for a reason written down here, is the opposite of that: everything else still passes
    /// through untouched, and a reader diffing this against the response sees exactly what is
    /// missing and why. `the_exclusion_is_not_a_projection` is the test that keeps the two apart,
    /// because without it "drop two fields" and "keep only what we know" pass the same suite.
    fn from_value(mut raw: Map<String, Value>) -> Result<Self, SessionError> {
        for field in ["access_token", "refresh_token"] {
            if !raw.get(field).is_some_and(Value::is_string) {
                return Err(SessionError::Missing(field));
            }
        }
        for field in Self::NOT_OURS_TO_KEEP {
            raw.remove(field);
        }
        Ok(Self { raw })
    }

    pub fn access_token(&self) -> &str {
        self.raw
            .get("access_token")
            .and_then(Value::as_str)
            .unwrap_or_default()
    }

    pub fn refresh_token(&self) -> &str {
        self.raw
            .get("refresh_token")
            .and_then(Value::as_str)
            .unwrap_or_default()
    }

    /// When the access token dies, in seconds since the epoch, or `None` if the session never
    /// said. Absent is not defaulted to a far future here — see [`Session::expires_soon`].
    pub fn expires_at(&self) -> Option<u64> {
        self.raw.get("expires_at").and_then(Value::as_u64)
    }

    /// Whether this session's token is spent or nearly so.
    ///
    /// A session with no `expires_at` answers `true`: the alternative is treating an unknown
    /// expiry as a distant one, which is how a dead token gets sent to a function that reads it
    /// and the 401 comes back looking like the function is broken. Refreshing a token that did not
    /// need it costs one request; not refreshing one that did costs the call.
    pub fn expires_soon(&self, now: u64, margin: Duration) -> bool {
        match self.expires_at() {
            Some(at) => at <= now.saturating_add(margin.as_secs()),
            None => true,
        }
    }

    /// The identity the provider attached, as GoTrue sent it, or `None` if the session carries
    /// none.
    ///
    /// Absent is a real answer here rather than an empty object: the implicit redirect hands back
    /// tokens and no user, so a session without this one is a session whose identity has not been
    /// fetched yet — see [`crate::cloud::oauth`], which refuses to store one.
    pub fn user(&self) -> Option<&Value> {
        self.raw.get("user").filter(|user| user.is_object())
    }

    /// Attach the identity fetched after an implicit redirect.
    ///
    /// The one field this client ever adds to a session besides `expires_at`, and it is added for
    /// the same reason: the wire did not carry it and the stored blob is expected to have it.
    /// A non-object is refused rather than inserted — a `null` under this key is worse than an
    /// absent one, because every reader tests for presence.
    pub fn with_user(mut self, user: Value) -> Result<Self, SessionError> {
        if !user.is_object() {
            return Err(SessionError::Malformed(
                "the identity endpoint did not answer with a user".to_string(),
            ));
        }
        self.raw.insert("user".to_string(), user);
        Ok(self)
    }

    /// The JSON to store, which is the JSON that arrived.
    pub fn to_stored(&self) -> String {
        Value::Object(self.raw.clone()).to_string()
    }
}

fn parse_object(text: &str) -> Result<Map<String, Value>, SessionError> {
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(raw)) => Ok(raw),
        Ok(_) => Err(SessionError::Malformed("not a JSON object".to_string())),
        Err(e) => Err(SessionError::Malformed(e.to_string())),
    }
}

/// What has to happen before the next request leaves.
///
/// Split out from [`SessionManager::client`] so the rule is gradeable without a server or a clock,
/// the way `require_filters` is — these are the two decisions in this module whose
/// failure is silent rather than visible.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreRequest {
    /// Signed out, or the token has life left in it.
    Nothing,
    /// The token is spent or nearly so.
    Refresh,
}

pub fn pre_request(session: Option<&Session>, now: u64) -> PreRequest {
    match session {
        Some(session) if session.expires_soon(now, EXPIRY_MARGIN) => PreRequest::Refresh,
        _ => PreRequest::Nothing,
    }
}

/// What a failed refresh means for the session being held.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefreshFailure {
    /// Nothing was learned about the token. Keep it.
    KeepSession,
    /// The refresh token will not work again. Drop it.
    DropSession,
}

/// Decide from the failure, not from the fact of failing.
///
/// This is the split `send` exists to make, used for the one decision that turns on it.
/// Signing a user out because their train went into a tunnel is the failure mode the beta has —
/// any `refreshSession()` error there becomes "Session expired — please log in again" — and it is
/// the wrong half of the split to act on.
pub fn refresh_failure(error: &CloudError) -> RefreshFailure {
    match error {
        // No answer. The token is whatever it was.
        CloudError::Transport(_) => RefreshFailure::KeepSession,
        // Never sent. Same. Both of these are build-env faults, not token faults.
        CloudError::NotConfigured(_) | CloudError::Unusable(_) => RefreshFailure::KeepSession,
        // Answered and refused: GoTrue rejected the refresh token, and it is single-use.
        CloudError::Refused { .. } => RefreshFailure::DropSession,
        // Answered 2xx with a body we could not read. The server rotated the refresh token on the
        // way, so the one still held is spent whether or not the new one was understood — keeping
        // it would mean retrying a token the server has already retired.
        CloudError::Decode { .. } => RefreshFailure::DropSession,
    }
}

/// Whether a refused sign-out is worth reporting.
///
/// `auth-js` swallows 401, 403 and 404 on logout (`GoTrueClient.js:1600-1605`) and it is right to:
/// a token the server will not accept, or a user it no longer has, is a session there was nothing
/// left to end. Reporting those would put an error in front of a user whose sign-out worked — and
/// the common case is signing out with a token that expired while the tab was closed, which is
/// every one of them.
pub fn logout_failure_matters(error: &CloudError) -> bool {
    !matches!(error, CloudError::Refused { status, .. } if matches!(status, 401 | 403 | 404))
}

/// The session, the client, and the rule between them.
///
/// Held once, by the shell. [`SessionManager::client`] is the only accessor that hands out a
/// [`Cloud`], so there is no way to issue a request that has skipped the refresh — which is the
/// difference between porting the beta's pre-refresh and porting its five call sites.
pub struct SessionManager {
    cloud: Cloud,
    session: Option<Session>,
    storage_key: String,
    discarded: Option<SessionError>,
}

impl SessionManager {
    /// A manager for this client, signed out, without touching storage.
    ///
    /// Split from [`SessionManager::restore_stored`] because the storage read is asynchronous and
    /// the shell needs the manager at the render that PROVIDES it as context, which cannot await.
    /// A context built inside an effect is a context that is absent for the first frame, and
    /// every consumer would need an arm standing in for "not yet" — the `None` twin again, one
    /// layer up from the one [`crate::build_file`] describes.
    pub fn new(cloud: Cloud) -> Result<Self, CloudError> {
        let storage_key = cloud.config().storage_key()?;
        Ok(Self {
            cloud,
            session: None,
            storage_key,
            discarded: None,
        })
    }

    /// Park the in-flight PKCE verifier where the next page load will find it.
    ///
    /// The web sign-in is two page loads with a provider in between, so the verifier cannot stay
    /// in memory the way the desktop one does — there is no process left to hold it. Storage is
    /// what survives the hop, and it is the same storage the session itself uses, under the key
    /// [`Config::pkce_verifier_key`] explains.
    ///
    /// **What this does and does not buy, written down because the two are easy to run together.**
    /// A verifier in `localStorage` is readable by anything running on this origin, so it is not a
    /// secret from a script that is already executing here — and against that attacker the session
    /// blob sitting beside it was already the bigger prize. What it IS secret from is the other
    /// side of the network and anyone handing this browser a URL, which is the whole of F01: a
    /// stranger cannot produce the verifier that matches a challenge this browser drew, so a
    /// callback they compose cannot be exchanged.
    ///
    /// [`Config::pkce_verifier_key`]: crate::cloud::Config::pkce_verifier_key
    ///
    /// **Stored as JSON, encoded here.** [`write_stored`] writes the text it is given verbatim — it
    /// was built for the session blob, which is JSON already — so a bare verifier handed to it
    /// landed as bare hex, and [`Self::take_code_verifier`] then failed to decode it and reported
    /// the secret missing. Every web sign-in ended on "this browser no longer holds the secret"
    /// until 2026-09-30, when the first real one was tried on next.coh-sidekick.com; the desktop
    /// never stores a verifier, which is why nothing had seen it.
    pub fn remember_code_verifier(&self, verifier: &str) -> Result<(), CloudError> {
        write_stored(
            &self.cloud.config().pkce_verifier_key()?,
            &encode_verifier(verifier),
        );
        Ok(())
    }

    /// Read the parked verifier and clear it in the same breath.
    ///
    /// **Taken, not read**, and the clear is unconditional: a verifier is spent by one exchange,
    /// and one left behind is one a later callback could be matched against. `auth-js` removes it
    /// on both outcomes for the same reason (`GoTrueClient.js:794` and `:812`).
    ///
    /// The stored value is JSON — `setItemAsync` writes `JSON.stringify(verifier)`
    /// (`auth-js/lib/helpers.js:125`) and [`Self::remember_code_verifier`] does the same — so it is
    /// decoded rather than used raw. A value that will not decode answers `None`, which fails the sign-in loudly
    /// at the exchange rather than sending the provider a verifier with quotes around it.
    pub async fn take_code_verifier(&self) -> Option<String> {
        let key = self.cloud.config().pkce_verifier_key().ok()?;
        let text = read_stored(&key).await;
        clear_stored(&key);
        decode_verifier(&text?)
    }

    /// Adopt whatever session is in storage. Called once, after mount.
    ///
    /// A stored session that will not read is dropped and the key cleared rather than carried:
    /// the user is signed out, which is a state the UI already shows. The reason is kept on
    /// [`SessionManager::discarded`] so it is reportable instead of merely true.
    ///
    /// Answers nothing, because there is nothing here a caller could act on differently: a
    /// missing session and an unreadable one both mean signed out, and the second is on
    /// `discarded` for whoever wants to say so.
    pub async fn restore_stored(&mut self) {
        let Some(text) = read_stored(&self.storage_key).await else {
            return;
        };
        match Session::from_stored(&text) {
            // Not `adopt`: that persists, and this session came *from* storage. Writing it back
            // at every launch is a write that can only lose — it cannot add anything, and it is
            // a chance to round-trip the beta's blob through a bug of ours.
            //
            // The one exception is the write below, and it is an exception because it is the
            // only thing in this program that ever takes the provider's credentials off DISK.
            Ok(session) => {
                // F24 strips `provider_token` and `provider_refresh_token` on the way in, which
                // is what stopped this app holding them or sending them anywhere. It did not
                // touch what was already written: a session stored before F24 landed keeps them
                // at rest until some later refresh happens to overwrite the whole blob, and in
                // one of the data directories this app will never open again (F49) that is
                // never. Measured on this machine — one orphaned store, 2026-09-18, still
                // holding both.
                //
                // Guarded rather than unconditional, so the paragraph above stays true of every
                // other launch: this fires only when the bytes on disk still carry a field
                // `from_value` refuses, and the only difference it can make is their removal.
                if stored_session_carries_refused_fields(&text) {
                    write_stored(&self.storage_key, &session.to_stored());
                }
                self.cloud
                    .set_access_token(Some(session.access_token().to_string()));
                self.session = Some(session);
            }
            Err(e) => {
                self.discarded = Some(e);
                clear_stored(&self.storage_key);
            }
        }
    }

    /// The client, with a token that is good when it is handed over. **The only door.**
    ///
    /// Refreshes first when the session is within [`EXPIRY_MARGIN`] of expiry; does nothing at all
    /// when signed out, because a request with no session still bears the anon key.
    pub async fn client(&mut self) -> Result<&Cloud, CloudError> {
        if pre_request(self.session.as_ref(), now_unix()) == PreRequest::Refresh {
            self.refresh().await?;
        }
        Ok(&self.cloud)
    }

    /// Adopt a session that arrived some other way — the OAuth callback (RB4c), or a sign-in.
    pub fn sign_in(&mut self, session: Session) {
        self.adopt(session);
    }

    /// End the session here and on the server.
    ///
    /// The local half happens whichever way the server call goes. `auth-js` swallows 401, 403 and
    /// 404 on logout for the same reason (`GoTrueClient.js:1600-1605`): a token the server will not
    /// accept is a token there is no point keeping, and a sign-out that leaves the user signed in
    /// because the network was down is the one outcome nobody would call correct.
    pub async fn sign_out(&mut self) -> Result<(), CloudError> {
        let outcome = match self.session {
            Some(_) => self.cloud.auth_post_empty("logout?scope=global").await,
            None => Ok(()),
        };
        self.forget();
        match outcome {
            Err(e) if !logout_failure_matters(&e) => Ok(()),
            outcome => outcome,
        }
    }

    /// The session, for the UI asking whether anyone is signed in.
    pub fn session(&self) -> Option<&Session> {
        self.session.as_ref()
    }

    /// Why the stored session was thrown away at start-up, if it was.
    pub fn discarded(&self) -> Option<&SessionError> {
        self.discarded.as_ref()
    }

    async fn refresh(&mut self) -> Result<(), CloudError> {
        let Some(current) = self.session.as_ref() else {
            return Ok(());
        };
        let refresh_token = current.refresh_token().to_string();
        let body = serde_json::json!({ "refresh_token": refresh_token });
        let answer: Value = self
            .cloud
            .auth_post("token?grant_type=refresh_token", &body)
            .await
            .inspect_err(|e| {
                if refresh_failure(e) == RefreshFailure::DropSession {
                    // `forget`, not just the in-memory half: a dead session left in storage is
                    // restored at the next launch, refreshed, refused, and dropped in memory
                    // again — a user who looks signed in at every boot and is signed in at none.
                    self.forget();
                }
            })?;
        match Session::from_token_response(answer, now_unix()) {
            Ok(session) => {
                self.adopt(session);
                Ok(())
            }
            Err(e) => {
                // A 2xx whose body is not a session: the old refresh token is spent either way.
                self.forget();
                Err(CloudError::Decode {
                    detail: e.to_string(),
                    // Never the body — it is a session. See [`SessionError`]. The same string
                    // `decode_auth` uses, so the two withholding sites cannot drift apart.
                    body: super::WITHHELD_BODY.to_string(),
                })
            }
        }
    }

    fn adopt(&mut self, session: Session) {
        self.cloud
            .set_access_token(Some(session.access_token().to_string()));
        write_stored(&self.storage_key, &session.to_stored());
        self.session = Some(session);
    }

    fn forget(&mut self) {
        self.session = None;
        self.cloud.set_access_token(None);
        clear_stored(&self.storage_key);
    }
}

/// Seconds since the epoch, on both targets.
///
/// `std::time::SystemTime::now()` panics on `wasm32-unknown-unknown` — there is no clock in that
/// target's std. `web-time` is `Date.now()` there and `std` everywhere else, behind one
/// unconditional import, which is the same bargain `reqwest` makes for the transport: the target
/// split lives in a dependency's manifest instead of in this crate's source.
///
/// A clock that reads before 1970 yields 0, which makes every session look expired and costs a
/// refresh. The other rounding — treating an unreadable clock as "plenty of time left" — costs the
/// call instead.
fn now_unix() -> u64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0)
}

/// Read the stored session, from the document the caller is in.
///
/// Whether the bytes on disk still carry a field [`Session::from_value`] refuses to keep.
///
/// Reads the stored text rather than the parsed `Session`, and that is the whole point: by the
/// time there is a `Session` the fields are already gone, so asking it would always answer no.
/// A blob this cannot parse answers no as well — `from_stored` will have refused it and the
/// caller clears the key, which removes the credentials by a shorter route.
fn stored_session_carries_refused_fields(text: &str) -> bool {
    serde_json::from_str::<Map<String, Value>>(text)
        .map(|raw| {
            Session::NOT_OURS_TO_KEEP
                .iter()
                .any(|field| raw.contains_key(*field))
        })
        .unwrap_or(false)
}

/// Reads stay local by [`crate::storage`]'s rule; only writes are routed. The session is read once,
/// by the shell, in the main window — a read from a popped-out webview would be that window's
/// snapshot, and nothing in this app loads persisted state from one.
/// A verifier as it is parked: a JSON string, the shape `auth-js` writes (`JSON.stringify`).
fn encode_verifier(verifier: &str) -> String {
    serde_json::Value::String(verifier.to_string()).to_string()
}

/// The parked verifier back out, or `None` when what is stored is not a JSON string.
fn decode_verifier(stored: &str) -> Option<String> {
    serde_json::from_str(stored).ok()
}

async fn read_stored(key: &str) -> Option<String> {
    let key = serde_json::to_string(key).ok()?;
    let js = format!("try {{ return localStorage.getItem({key}); }} catch (_) {{ return null; }}");
    let value = document::eval(&js).await.ok()?;
    let text = value.as_str()?.to_string();
    (!text.is_empty()).then_some(text)
}

/// Write the session, through the window that owns persistence.
///
/// Both arguments reach JS as JSON string literals rather than through `{:?}`, for
/// [`crate::build_file`]'s reason: Rust's debug escaping is close enough to JS's to be tempting and
/// is not the same language. Here the payload carries a whole `user` object of the provider's
/// making — display names, avatar URLs — which is arbitrary text from outside this program.
fn write_stored(key: &str, json: &str) {
    let (Ok(key), Ok(json)) = (serde_json::to_string(key), serde_json::to_string(json)) else {
        return;
    };
    crate::storage::commit(format!(
        "try {{ localStorage.setItem({key}, {json}); }} catch (_) {{}}"
    ));
}

fn clear_stored(key: &str) {
    let Ok(key) = serde_json::to_string(key) else {
        return;
    };
    crate::storage::commit(format!(
        "try {{ localStorage.removeItem({key}); }} catch (_) {{}}"
    ));
}

#[cfg(test)]
mod tests {
    use super::{decode_verifier, encode_verifier};

    /// The web sign-in parks its verifier across a page load and reads it back on the callback.
    /// The two halves disagreed on the format once, and every web sign-in failed as a result.
    #[test]
    fn a_parked_verifier_reads_back_as_itself() {
        let verifier = "b14b5d810e1ac8edd7907a58dd4beb3c";
        assert_eq!(
            decode_verifier(&encode_verifier(verifier)).as_deref(),
            Some(verifier)
        );
        assert_eq!(encode_verifier(verifier), format!("\"{verifier}\""));
    }
}
