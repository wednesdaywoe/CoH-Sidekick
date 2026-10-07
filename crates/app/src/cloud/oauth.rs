//! Signing in: the redirect out, and the fragment that comes back.
//!
//! The beta's whole auth surface is twenty lines (`services/auth.ts:9-27`) because
//! `supabase-js` owns the flow behind them. Those twenty lines are the part that is ours to keep;
//! the flow underneath them is the part that has to be written out, and writing it out is where
//! the one decision in this file lives.
//!
//! # One flow now, and the argument that used to split them is the argument that joined them
//!
//! **Both builds run PKCE.** The web arm did not, until F01: `createClient` was called with no
//! auth options (`lib/supabase.ts:13`), so the beta ran on `DEFAULT_AUTH_OPTIONS` and its
//! `flowType: "implicit"` (`auth-js/GoTrueClient.js:24`), and this arm was written to match it.
//! An implicit redirect comes back with the tokens themselves in the URL **fragment** —
//! `#access_token=…&refresh_token=…` — with nothing to exchange and nothing to check, which means
//! any URL on this origin carrying that shape was a session this client adopted. The beta sets
//! `flowType: 'pkce'` now (`lib/supabase.ts`, F01), so the compatibility argument below has not
//! been set aside — it has changed sides, and it is what requires this arm to be PKCE too.
//!
//! What the compatibility contract covers is unchanged and is worth being exact about, because it
//! is what makes one origin serving two clients workable: [`super::session`] documents the STORED
//! BLOB as the contract, and PKCE does not touch its shape. What PKCE adds beside it is one more
//! key — `sb-<ref>-auth-token-code-verifier`, which `auth-js` writes and reads under exactly that
//! name — so a sign-in begun in one client and landed in the other finds its verifier where it
//! looks for it. See [`super::Config::pkce_verifier_key`].
//!
//! **The desktop build has run PKCE since F62**, and the two arms were never one problem in
//! different clothes.
//! The web callback lands on a page load inside the app's own origin; the desktop one lands on a
//! loopback listener whose address had to be handed to a child process to get the browser open —
//! so on desktop the redirect address is readable by anything on the machine that can read a
//! command line (`/proc/<pid>/cmdline` is world-readable), and under an implicit flow whoever
//! reads it gets a bounce page that will forward any fragment it is given. PKCE removes what
//! reading it is worth: the URL carries a `code_challenge`, which is public by design; the answer
//! comes back as `?code=` in a **query** rather than as tokens in a fragment; and that code cannot
//! be exchanged without the verifier this process never let out of its own memory. An injected
//! code fails the exchange loudly, because it was issued against the attacker's challenge and this
//! side presents its own verifier.
//!
//! **This was rejected once, and the reason it was rejected for has been measured and is false.**
//! The note that stood here said PKCE "means an allow-list edit and a dashboard edit that must
//! land before any of this works at all". Neither is true of what is deployed. `flowType` is a
//! supabase-js **client** setting with no server counterpart, and the GoTrue this project runs —
//! v2.197.0, read from its own `/auth/v1/health` — implements `code_challenge` on `authorize` and
//! `grant_type=pkce` on `token` whatever any client is configured with. Both were probed against
//! the live endpoint before this was written: a challenge on `authorize` answers 302, and
//! `grant_type=pkce` with a bogus code answers `flow_state_not_found` where an absent grant
//! answers `unsupported_grant_type`. The allow-list half went the same way —
//! `IsRedirectURLValid` returns `ip.IsLoopback()` **before** `URIAllowListMap` is consulted
//! (`internal/utilities/request.go`), so a loopback `redirect_to` needs no entry at any port or
//! any path.
//!
//! What the old note got right is that this is a change with a blast radius, so here is the edge
//! of it: **nothing in this tree can prove the round trip works**, because the provider hop is not
//! ours to drive. Every decision above is graded, the exchange is graded against RFC 7636's own
//! test vector, and the last step is still one live desktop sign-in.
//!
//! What implicit cost is kept written down, because it is the reason this arm moved and because
//! nothing stops a future client reaching for it again: for the length of one page load the access
//! token and the refresh token were in `window.location`, which means the address bar and the
//! session history — and, worse than either, it meant this client could not tell a callback it had
//! started from one it had been handed. [`super::account::Account::start`] still clears the URL on
//! the way out, on the failure path as well as the success one, and now takes the spent `code` off
//! the search as well; `auth-js` clears only on success (`GoTrueClient.js:1543`), and a live token
//! left in the address bar because the *user fetch* 500'd is the one case where being faithful to
//! the port cost something real.
//!
//! # No target is named here either
//!
//! No client code under this directory names a target, and PKCE did not cost it
//! (`npm run audit:cloud-target-arms`).
//! [`authorize_url`] is a string, [`pkce_challenge`] is a hash of a string, and [`exchange_code`]
//! is a POST — none of the three can tell which target it is on. The desktop-only halves are the
//! two this layer must not hold: drawing the verifier (a CSPRNG read) and catching the answer (a
//! TCP listener), and both live at the caller that already owns the split
//! (`crate::desktop_signin`).
//!
//! # Why the user is fetched, and why a failure to fetch it fails the sign-in
//!
//! The fragment carries tokens and no identity. `auth-js` fills that in with
//! `GET /auth/v1/user` before it assembles the session (`GoTrueClient.js:1529`), and the
//! assembled session — `user` and all — is what goes into `localStorage`. That is not
//! decoration: [`super::session`] documents the stored blob as a compatibility contract with the
//! beta on the same origin, and a stored session with no `user` is one the beta reads as a
//! signed-in user with no identity. So the fetch is part of signing in, and a sign-in that could
//! not complete it is a sign-in that failed — loudly, rather than a session that looks fine
//! until something reads its name.

use super::session::{Session, SessionError};
use super::{Cloud, CloudError};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// The two providers the project has configured.
///
/// An enum rather than the beta's string union so a third provider is a compile error at every
/// site that has to say something about it, which is the [`crate::cloud`] rule applied to a
/// two-member set: the ids are what GoTrue matches on, and a typo in one of them is a redirect
/// to a provider that does not exist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthProvider {
    Discord,
    SimpleLogin,
}

pub const PROVIDERS: [AuthProvider; 2] = [AuthProvider::Discord, AuthProvider::SimpleLogin];

impl AuthProvider {
    /// The id GoTrue knows it by. `custom:simplelogin` is a custom OIDC provider, and the colon
    /// is part of the id rather than a separator this code should be splitting on.
    pub fn id(self) -> &'static str {
        match self {
            Self::Discord => "discord",
            Self::SimpleLogin => "custom:simplelogin",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Discord => "Discord",
            Self::SimpleLogin => "SimpleLogin",
        }
    }

    /// The beta's own `title` on each row, carried over because it is the only place the app says
    /// what signing in is *for*.
    pub fn hint(self) -> &'static str {
        match self {
            Self::Discord => "Share builds, claim a public author handle, and create short links",
            Self::SimpleLogin => "Email-based, privacy-focused",
        }
    }
}

/// Why a sign-in did not happen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OAuthError {
    /// The provider or GoTrue refused, and said so on the redirect. Carries the server's own
    /// words for the same reason the transport's `server_message` does.
    Refused { code: String, description: String },
    /// The identity fetch, or a client that could not be built at all.
    Cloud(CloudError),
    /// The tokens arrived and would not assemble into a session.
    Session(SessionError),
    /// A URL offered this browser a session it never asked for (F01).
    ///
    /// Named for what it is rather than reported as a callback that came back short, because the
    /// one way to reach it is to have followed somebody's link, and the user-facing wording says
    /// so. There used to be an `Incomplete(field)` variant beside this one for a callback of ours
    /// that arrived missing a field; it belonged to the implicit flow and went with it — see the
    /// note at [`complete`].
    Unsolicited,
    /// A code came back and the verifier that would spend it is gone.
    ///
    /// Storage cleared mid-flow, a second tab that spent it first, or a sign-in begun on a
    /// different device. Not a security event on its own — it is the state that FAILS the
    /// exchange — but it is the one the user has to be told about, because the remedy is to
    /// sign in again from this browser.
    NoVerifier,
}

impl std::fmt::Display for OAuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Refused { code, description } => {
                // Both fields come off the address bar, so both are bounded the way
                // `CloudError`'s server text is (F22). A refusal is reached by following a link,
                // and the one thing a stranger controls about this modal is how much of it is
                // their text — `clipped` marks the cut and the real size so a clipped string
                // cannot read as a whole one.
                write!(
                    f,
                    "sign-in was refused ({}): {}",
                    super::clipped(code),
                    super::clipped(description)
                )
            }
            Self::Cloud(e) => write!(f, "{e}"),
            Self::Session(e) => write!(f, "{e}"),
            Self::Unsolicited => write!(
                f,
                "that link offered a sign-in this browser never started, so it was refused"
            ),
            Self::NoVerifier => write!(
                f,
                "this browser no longer holds the secret that finishes this sign-in — start it again"
            ),
        }
    }
}

impl std::error::Error for OAuthError {}

impl From<CloudError> for OAuthError {
    fn from(e: CloudError) -> Self {
        Self::Cloud(e)
    }
}

impl From<SessionError> for OAuthError {
    fn from(e: SessionError) -> Self {
        Self::Session(e)
    }
}

// ============================================================
// Out: the redirect.
// ============================================================

/// Where to send the browser to sign in.
///
/// `auth-js` builds this by hand out of `encodeURIComponent`, not out of `URLSearchParams`
/// (`GoTrueClient.js:2324-2348`), and the two disagree on a handful of bytes — so this builds it
/// by hand too, with [`encode_uri_component`]. The `redirect_to` is the caller's: it is the
/// deployed origin, which only the page knows, or the loopback address, which only the listener
/// knows.
///
/// **`challenge` is what picks the flow**, and it is an `Option` rather than two functions so that
/// every call site has to say which one it is starting. `Some` is PKCE and the answer comes back
/// as `?code=`; `None` is implicit and the answer comes back as tokens on the fragment. The module
/// doc has which build takes which and why.
pub fn authorize_url(
    cloud: &Cloud,
    provider: AuthProvider,
    redirect_to: &str,
    challenge: Option<&str>,
) -> String {
    let mut url = cloud.auth_url("authorize");
    let _ = write!(
        url,
        "?provider={}&redirect_to={}",
        encode_uri_component(provider.id()),
        encode_uri_component(redirect_to),
    );
    if let Some(challenge) = challenge {
        // Lower-case `s256`, which is what supabase-js sends and what this deployment was probed
        // with. GoTrue lower-cases the method before matching on it
        // (`internal/security/pkce.go`), so the case is a matter of being the same string the
        // other client sends rather than of being accepted.
        let _ = write!(
            url,
            "&code_challenge={}&code_challenge_method=s256",
            encode_uri_component(challenge),
        );
    }
    url
}

/// The `code_challenge` for a verifier: `BASE64URL(SHA256(verifier))`, unpadded.
///
/// RFC 7636 §4.2, and GoTrue verifies it with exactly that — `base64.RawURLEncoding` over a
/// `sha256.Sum256`, compared in constant time (`internal/security/pkce.go`). Unpadded is not a
/// stylistic choice: GoTrue's own validator refuses a challenge containing anything outside
/// `[a-zA-Z._~0-9-]` (`internal/api/pkce.go`), which rules out both `=` and the standard
/// alphabet's `+` and `/`.
///
/// The verifier itself is **not** drawn here. It is the one value in this flow that must never
/// leave the process, and drawing it needs a CSPRNG, which is a platform read this directory does
/// not make (see the module doc's rule). [`crate::desktop_signin`] draws it and hands this
/// function the part that is safe to publish.
pub fn pkce_challenge(verifier: &str) -> String {
    use base64::Engine as _;
    use sha2::{Digest, Sha256};
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// Trade the code the redirect brought back for a session.
///
/// `POST /auth/v1/token?grant_type=pkce` with `{auth_code, code_verifier}` — the shape GoTrue's
/// `PKCEGrantParams` names (`internal/api/token.go`). **This is the step that makes the code worth
/// nothing to anyone who intercepted it**: the flow state GoTrue finds by that code carries the
/// challenge it was issued against, and a verifier that does not hash to it is a 400 rather than a
/// session.
///
/// Unlike the implicit arm, the answer is a full `AccessTokenResponse` and normally carries the
/// user already, so the identity fetch is conditional rather than unconditional. It is still there
/// for the case where it does not: a session stored without a `user` is one the beta reads as a
/// signed-in user with no identity, which [`super::session`] documents as the thing not to store.
pub async fn exchange_code(
    cloud: &Cloud,
    code: &str,
    verifier: &str,
    now: u64,
) -> Result<Session, OAuthError> {
    let mut body = Map::new();
    body.insert("auth_code".to_string(), Value::from(code));
    body.insert("code_verifier".to_string(), Value::from(verifier));
    let answer: Value = cloud
        .auth_post("token?grant_type=pkce", &Value::Object(body))
        .await?;
    let session = Session::from_token_response(answer, now)?;
    if session.user().is_some() {
        return Ok(session);
    }
    let user = fetch_user(cloud, session.access_token()).await?;
    Ok(session.with_user(user)?)
}

/// JavaScript's `encodeURIComponent`, byte for byte.
///
/// Written out rather than reached for, because the obvious substitutes are not the same
/// function: form encoding turns a space into `+` and leaves `~`, `!`, `'`, `(` and `)` to the
/// implementation, and a `redirect_to` that round-trips differently is a redirect the provider's
/// allow-list does not recognise. The unreserved set is the one the spec names —
/// `A-Z a-z 0-9 - _ . ! ~ * ' ( )` — and everything else is its UTF-8 bytes in upper-case hex.
pub fn encode_uri_component(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' => out.push(byte as char),
            b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')' => out.push(byte as char),
            _ => {
                let _ = write!(out, "%{byte:02X}");
            }
        }
    }
    out
}

// ============================================================
// Back: the fragment.
// ============================================================

/// Everything the redirect put on the URL, hash and query together.
///
/// `auth-js`'s `parseParametersFromURL` (`lib/helpers.js:87-106`), including the precedence:
/// the hash is read first and the query overwrites it. A `BTreeMap` rather than a `HashMap` so
/// a test that prints the whole thing prints it the same way twice.
pub fn parse_parameters(href: &str) -> BTreeMap<String, String> {
    let (before_hash, fragment) = href.split_once('#').unwrap_or((href, ""));
    let query = before_hash.split_once('?').map_or("", |(_, query)| query);

    let mut params = BTreeMap::new();
    for source in [fragment, query] {
        for pair in source.split('&').filter(|pair| !pair.is_empty()) {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            params.insert(decode_uri_component(key), decode_uri_component(value));
        }
    }
    params
}

/// `URLSearchParams`'s decoding: `%XX` to a byte, `+` to a space.
///
/// `+` is the asymmetry worth noticing — `encodeURIComponent` never writes one, and
/// `URLSearchParams` still reads one as a space, because the fragment GoTrue builds is
/// form-encoded on the server side. Reading it the way the browser reads it is the point.
///
/// Bytes that do not assemble into UTF-8 become U+FFFD, which is what `URLSearchParams` does
/// too. Nothing in a callback is expected to be non-ASCII; a token that arrived mangled fails as
/// a token, which is where it should fail.
pub fn decode_uri_component(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                match u8::from_str_radix(&text[i + 1..i + 3], 16) {
                    Ok(byte) => {
                        out.push(byte);
                        i += 3;
                    }
                    // A stray `%` is a literal `%`, as it is in the browser.
                    Err(_) => {
                        out.push(b'%');
                        i += 1;
                    }
                }
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Whether this page load is a sign-in coming back.
///
/// `auth-js`'s `_isImplicitGrantCallback` (`GoTrueClient.js:1560-1566`):
/// `access_token || error_description`, and **not** `error` or `error_code` on their own. Ported
/// as it stands rather than widened, because widening it is not free — the address bar is not
/// this app's alone, and a bare `?error=` from anything else on the origin would start raising a
/// sign-in failure over someone else's parameter. GoTrue sends `error_description` alongside
/// `error` on every redirect it refuses, so the narrow test catches every refusal that is ours.
///
/// **`code` joins the test now that the web arm is PKCE too** (F01). A PKCE callback carries no
/// tokens at all — the answer is `?code=`, and without this arm the page load that IS the sign-in
/// would not be recognised as one. `access_token` stays in the test, and staying is not
/// leftover tolerance: [`complete`] refuses it, and it has to be RECOGNISED to be refused. Drop it
/// here and a token-bearing URL stops being a callback, which means it is never reported and the
/// fragment is never cleared — the failure would be silent, which is the one shape Rule 1 forbids.
pub fn is_callback(params: &BTreeMap<String, String>) -> bool {
    params.contains_key("code")
        || params.contains_key("access_token")
        || params.contains_key("error_description")
}

// THE IMPLICIT FLOW'S CALLBACK READER STOOD HERE, and it was deleted on 2026-09-26 rather than
// annotated again. `session_from_callback` read `access_token`, `refresh_token`, `token_type` and
// `expires_in` straight out of the address the provider redirected to, and assembled a session
// from them; `OAuthError::Incomplete(field)` was its way of naming which of the four was absent,
// and it had no other constructor. Both are recoverable from
// `git show 71a2b04f0:crates/app/src/cloud/oauth.rs`.
//
// That shape is the one [`complete`] now REFUSES on purpose, as `Unsolicited`: a session offered
// in a URL is a session nothing ties to a sign-in this browser began, which is login CSRF (F01).
// So the deleted function was not a check this file is missing -- it was the previous flow's
// implementation, kept beside the flow that replaced it. Every session, however it arrives, still
// goes through the one door that validates it, [`Session::from_token_response`], which is where
// the four-field rule actually lives.
//
// It survived because in `coh-sidekick-1.0` it has no production caller either -- eight tests
// exercise it, one of them named `a_callback_missing_any_of_the_four_required_fields_is_refused_by
// _name`, so the compiler there never reports it. This repository has no tests, which is what made
// it visible. The same dead-code census cleared four other sites the same day; see the commit
// that deleted them (`git log --diff-filter=D --grep dead`).

/// `GET /auth/v1/user`, bearing the token that just arrived rather than the one the client holds.
///
/// The clone is the point: at this moment `cloud` is still signed out and its bearer is the anon
/// key, which would answer for nobody. `auth-js` passes the token explicitly for the same reason
/// (`GoTrueClient.js:1529` → `_getUser(access_token)`), and the alternative — adopting the
/// session first and fetching afterwards — would store a session this client has not yet
/// established is real.
pub async fn fetch_user(cloud: &Cloud, access_token: &str) -> Result<Value, CloudError> {
    let mut authed = cloud.clone();
    authed.set_access_token(Some(access_token.to_string()));
    authed.auth_get("user", &[]).await
}

/// Assemble the session the redirect brought back, identity included.
///
/// The whole of the callback except reading the URL and clearing it, which are the browser's
/// half and live in [`super::account`]. Kept apart so every decision above is gradeable without
/// a document — the one thing that is not is the fetch, and a sign-in that cannot reach the
/// server is a sign-in that failed either way.
/// **A session offered in the URL is refused** (F01). Until this arm was written, any URL on this
/// origin carrying `#access_token=…&refresh_token=…` was read, verified against `/auth/v1/user`
/// and adopted — no `state`, and nothing tying those tokens to a sign-in this browser began. That
/// is login CSRF: the victim keeps using the site as somebody else, and every build they save
/// lands in the attacker's account. The refusal is what closes it, and the refusal has to be here
/// rather than at the caller, because this is the only place that knows the difference between a
/// callback this browser started and one it was handed.
///
/// `auth-js` refuses the same shape the same way once it is on PKCE — `_getSessionFromURL` throws
/// "Not a valid PKCE flow url." before any network call (`GoTrueClient.js:1482-1485`) — which is
/// what keeps this arm and the beta's the same client on one origin.
///
/// **The refusal check below is this arm's own, not a delegation.** It used to be described as a
/// reach for `session_from_callback`, the implicit flow's reader; that was false when the compiler
/// reported that function unused on 2026-09-26, and the function is now gone — the note where it
/// stood says why. The point the sentence was making stands: a provider that refuses answers
/// `?error=&error_description=` on the PKCE arm exactly as it did on the implicit one, and that
/// refusal is the user's to see.
pub async fn complete(
    cloud: &Cloud,
    params: &BTreeMap<String, String>,
    verifier: Option<&str>,
    now: u64,
) -> Result<Session, OAuthError> {
    if let Some(description) = params.get("error_description").or(params.get("error")) {
        return Err(OAuthError::Refused {
            code: params
                .get("error_code")
                .or(params.get("error"))
                .cloned()
                .unwrap_or_else(|| "unspecified_code".to_string()),
            description: description.clone(),
        });
    }

    let Some(code) = params.get("code") else {
        // Everything that is not an error and not a code. Named for what it is rather than
        // reported as "incomplete", because the one way to arrive here is a URL offering a
        // session, and a user who sees this was handed a link.
        return Err(OAuthError::Unsolicited);
    };
    let verifier = verifier.ok_or(OAuthError::NoVerifier)?;
    exchange_code(cloud, code, verifier, now).await
}
