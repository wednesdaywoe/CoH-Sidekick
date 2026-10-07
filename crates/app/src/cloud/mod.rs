//! The transport floor under every cloud call: four wire shapes, one HTTP client, no target.
//!
//! `@supabase/supabase-js` was handing the beta these four shapes for free. There is no Supabase
//! crate, so here they are ours to write:
//!
//!   * `POST /functions/v1/<name>` — the eight edge functions ([`Cloud::invoke`])
//!   * `POST /rest/v1/rpc/<name>` — `increment_views`, `search_authors`, `resolve_author` ([`Cloud::rpc`])
//!   * PostgREST on `/rest/v1/<table>` — the `favorites` table, the only one the beta touches
//!     directly ([`Cloud::select`], [`Cloud::upsert`], [`Cloud::delete`])
//!   * `/auth/v1/*` — the session endpoints RB4b and RB4c build on ([`Cloud::auth_post`])
//!
//! # Why this file names no target
//!
//! No CLIENT code under this directory names a target, and that is a rule about the shape of the
//! layer rather than about which platform ships first. A cloud client written behind
//! `cfg(target_arch = "wasm32")` and ported to desktop later is the slot-drag defect with a bigger
//! blast radius — a helper only one target could run, with a `None` twin standing in for the
//! platform that got nothing ([`crate::build_file`] carries the full note). It would be discovered
//! at the point desktop tries to sign in, which is after the web work is declared done.
//!
//! `npm run audit:cloud-target-arms` is the gate. It was a bare grep for `target_arch` printing 0
//! until 2026-09-18, when a native-only TEST arrived — `session.rs` binds a real `TcpListener`,
//! and the observable it grades IS a socket — and the grep could not tell a test from a client.
//! It reports the two populations apart: client arms must be zero, and a test arm has to carry a
//! line above it naming what makes it native, because a reason that lives in a stream file is a
//! reason nobody re-checks. It also fails on a missing or empty directory, since a scan that
//! matches nothing reads exactly like a scan that found nothing wrong.
//!
//! `reqwest` is what makes that affordable rather than principled: it is `fetch` on wasm and
//! hyper+rustls on native, and it gates every TLS and hyper dependency behind
//! `cfg(not(target_arch = "wasm32"))` in its own manifest, so `features = ["json", "rustls-tls"]`
//! is one unconditional line that means TLS on the desktop build and nothing at all on the web
//! one. Only the intersection of its two API surfaces is used here — the wasm `ClientBuilder` has
//! `default_headers` and no `timeout`, so the timeout is set per request, where both have it.
//!
//! # The bearer is never absent
//!
//! "No session" never meant "no `Authorization` header". `supabase-js` seeds its default headers
//! with `Authorization: Bearer <anon key>` and replaces it only when a session access token
//! exists (`supabase-js/dist/index.cjs:334` and `:111`), so every anonymous call in the beta
//! arrives bearing the public anon key — which is what satisfies gateway JWT verification on the
//! functions that are absent from `supabase/config.toml` and therefore run on its default.
//! `get-build` is one of those, and it is the anonymous read path.
//!
//! A client that sent no bearer when logged out would break it at the gateway, and the failure
//! would read as the function being broken rather than as a missing header. So the fallback lives
//! in [`Cloud::bearer`], once, and no call site is trusted to remember it.
//!
//! # Errors carry what the server said
//!
//! Supabase puts its message in the response body, not in the status line, which is why the beta
//! reaches through `error.context.json()` for `body.error` rather than reporting the status. A
//! status alone is a soft failure wearing a number: it tells the user something broke and the
//! maintainer nothing. [`server_message`] keeps the server's own words, and falls back to the raw
//! body rather than to a phrase of ours, because an unrecognised error shape is exactly the case
//! where inventing wording loses the only evidence there is.
//!
//! What the server said is kept whole on the error and bounded where it is *shown*: every caller
//! renders a [`CloudError`] by its `Display`, into a text node, so an answer that is a document
//! rather than a message would otherwise be pasted into the page. [`clipped`] cuts the rendered
//! line; the `body` fields still carry the full text for the caller that parses it.

pub mod account;
pub mod auction;
pub mod avatar;
pub mod browser;
pub mod favorites;
pub mod layouts;
pub mod oauth;
pub mod owner_store;
pub mod profile;
pub mod save_build;
pub mod session;
pub mod shared_builds;
pub mod tag_vocab;

use serde::de::DeserializeOwned;
use serde::Serialize;
use std::time::Duration;

/// How long any one request may take. Set per request rather than on the client because the wasm
/// `ClientBuilder` has no `timeout` and the native one does — the per-request method is the one
/// both targets carry.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// The project the client talks to. Both values are baked at compile time because that is the one
/// mechanism both targets share: there is no process environment in a browser tab to read at
/// runtime.
///
/// The anon key being in the binary is not a leak — it is the public key the beta already ships
/// in its JavaScript, and row-level security is what stands between it and the data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// The project origin, with no trailing slash.
    base_url: String,
    anon_key: String,
}

impl Config {
    /// Read the project out of the build environment, or name the variable that was missing.
    ///
    /// A build without these compiles and runs; every cloud call then fails visibly with the
    /// variable's name in it. That is the beta's own shape — `supabase` is null when unconfigured
    /// and each service throws "Sharing is not configured" — and it beats a build-time `env!`,
    /// which would make an offline desktop build of the planner impossible to produce.
    pub fn from_build_env() -> Result<Self, CloudError> {
        let base_url = option_env!("SIDEKICK_SUPABASE_URL")
            .ok_or(CloudError::NotConfigured("SIDEKICK_SUPABASE_URL"))?;
        let anon_key = option_env!("SIDEKICK_SUPABASE_ANON_KEY")
            .ok_or(CloudError::NotConfigured("SIDEKICK_SUPABASE_ANON_KEY"))?;
        Ok(Self::new(base_url, anon_key))
    }

    /// The project as given. Trailing slashes are trimmed here so every URL built below can join
    /// with a `/` and never produce a `//` path that PostgREST reads as an empty table name.
    pub fn new(base_url: &str, anon_key: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            anon_key: anon_key.to_string(),
        }
    }

    pub fn anon_key(&self) -> &str {
        &self.anon_key
    }

    /// The `localStorage` key the session lives under, derived the way `supabase-js` derives it:
    /// `sb-${hostname.split(".")[0]}-auth-token` (`supabase-js/dist/index.cjs:202`).
    ///
    /// Derived rather than configured, and refused rather than guessed at, because this key is a
    /// compatibility contract with the beta it replaces on the same origin — [`session`] has the
    /// argument. A key computed from a URL this could not parse would be a key no session is
    /// stored under, and the symptom is every user appearing to have never signed in.
    pub fn storage_key(&self) -> Result<String, CloudError> {
        let host = reqwest::Url::parse(&self.base_url)
            .ok()
            .and_then(|url| url.host_str().map(str::to_string))
            .ok_or(CloudError::Unusable("SIDEKICK_SUPABASE_URL"))?;
        let project = host.split('.').next().unwrap_or_default();
        if project.is_empty() {
            return Err(CloudError::Unusable("SIDEKICK_SUPABASE_URL"));
        }
        Ok(format!("sb-{project}-auth-token"))
    }

    /// Where the in-flight PKCE verifier waits out the provider hop, on the web arm.
    ///
    /// `supabase-js`'s own key, not one of ours: `getCodeChallengeAndMethod` writes
    /// `${storageKey}-code-verifier` (`auth-js/lib/helpers.js:269`) and `_exchangeCodeForSession`
    /// reads it back from the same place. [`storage_key`] is already a compatibility contract with
    /// the beta on this origin, and a sign-in that begins in one client and lands in the other is
    /// exactly what one origin serving two clients means — so the verifier has to be findable
    /// under the name the other client would look for it under, or the exchange fails on a
    /// verifier that is sitting right there under a different name.
    ///
    /// [`storage_key`]: Self::storage_key
    pub fn pkce_verifier_key(&self) -> Result<String, CloudError> {
        Ok(format!("{}-code-verifier", self.storage_key()?))
    }
}

/// What went wrong, in the terms the caller can act on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CloudError {
    /// The build carries no project. Names the variable, because "not configured" without one
    /// sends the reader to the wrong file.
    NotConfigured(&'static str),
    /// The build carries a project, and it cannot be used. Separate from `NotConfigured` because
    /// "unset" and "unreadable" send the reader to different places, and both are build-env bugs
    /// nobody would find from a runtime symptom.
    Unusable(&'static str),
    /// The request never completed — no network, DNS, CORS, TLS.
    Transport(String),
    /// The server answered and refused. `message` is the server's own words (see [`server_message`]).
    ///
    /// `body` is that answer as it arrived, kept because a refusal is sometimes STRUCTURED and
    /// the message is only one field of it: `share-build` answers a 429 with `action`, `limit`,
    /// `retryAfterSeconds` and `resetAt` beside the error string, and a client that kept only the
    /// string could tell the user it had hit a limit but never when the limit lifts. The beta
    /// reaches for the same thing through `error.context.json()`. Carried on every refusal rather
    /// than parsed here, because which fields matter is the calling row's question, not the
    /// transport's.
    Refused {
        status: u16,
        message: String,
        body: String,
    },
    /// The server answered and the body was not what this call expected. Carries the body, since
    /// a decode error without the text it failed on cannot be diagnosed from a log line.
    Decode { detail: String, body: String },
}

impl std::fmt::Display for CloudError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotConfigured(var) => write!(f, "cloud is not configured: {var} is unset"),
            Self::Unusable(var) => write!(f, "cloud is not configured: {var} cannot be read"),
            Self::Transport(detail) => write!(f, "could not reach the server: {detail}"),
            Self::Refused {
                status, message, ..
            } => write!(f, "server refused ({status}): {}", clipped(message)),
            Self::Decode { detail, body } => {
                write!(f, "unexpected response ({detail}): {}", clipped(body))
            }
        }
    }
}

impl std::error::Error for CloudError {}

/// The Supabase client. Cheap to clone — `reqwest::Client` is a handle to a shared pool.
#[derive(Clone)]
pub struct Cloud {
    http: reqwest::Client,
    config: Config,
    /// The current session's access token, or `None` when signed out. RB4b owns keeping this
    /// fresh; every request below reads it through [`Cloud::bearer`] rather than directly, so
    /// there is one place the anon-key fallback can be got wrong.
    access_token: Option<String>,
}

/// Redacted by hand: `access_token` is a live bearer token.
///
/// The same reasoning as [`session::Session`]'s impl. Whether a token is
/// held is the debuggable fact — signed in or not is what a request-path bug turns on — and the
/// token itself never is.
///
/// `base_url` is printed whole: it is the project URL, which ships in the binary and in the
/// beta's JavaScript. The anon key is not printed even though F58 establishes it is public by
/// design, because a redaction guard that makes an exception is one whose next reader has to
/// re-derive the exception.
impl std::fmt::Debug for Cloud {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cloud")
            .field("base_url", &self.config.base_url)
            .field(
                "access_token",
                &self.access_token.as_ref().map(|_| "<held>"),
            )
            .finish_non_exhaustive()
    }
}

impl Cloud {
    /// Build a client for the project in the build environment.
    pub fn from_build_env() -> Result<Self, CloudError> {
        Self::new(Config::from_build_env()?)
    }

    pub fn new(config: Config) -> Result<Self, CloudError> {
        // `Client::new()` panics when the backend will not start; the builder returns it. On the
        // web a panic aborts the whole app, so one unbuildable client would blank the planner
        // instead of failing the one thing that needed the network.
        let http = reqwest::Client::builder()
            .build()
            .map_err(|e| CloudError::Transport(e.to_string()))?;
        Ok(Self {
            http,
            config,
            access_token: None,
        })
    }

    /// Adopt a session's access token, or drop it on sign-out.
    pub fn set_access_token(&mut self, token: Option<String>) {
        self.access_token = token;
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    /// The token every request bears: the session's when there is one, the public anon key when
    /// there is not. Never nothing — see the module doc.
    fn bearer(&self) -> &str {
        self.access_token
            .as_deref()
            .unwrap_or_else(|| self.config.anon_key())
    }

    /// A request with the two headers Supabase wants on everything, and the timeout both targets
    /// support. Every shape below starts here so none of them can be built without them.
    fn request(&self, method: reqwest::Method, url: String) -> reqwest::RequestBuilder {
        self.http
            .request(method, url)
            .header("apikey", self.config.anon_key())
            .bearer_auth(self.bearer())
            .timeout(REQUEST_TIMEOUT)
    }

    // -- shape 1: edge functions ------------------------------------------------------------

    pub fn function_url(&self, function: &str) -> String {
        format!("{}/functions/v1/{function}", self.config.base_url)
    }

    /// `POST /functions/v1/<name>` with a JSON body.
    pub async fn invoke<B: Serialize + ?Sized, R: DeserializeOwned>(
        &self,
        function: &str,
        body: &B,
    ) -> Result<R, CloudError> {
        let request = self
            .request(reqwest::Method::POST, self.function_url(function))
            .json(body);
        decode(send(request).await?)
    }

    // -- shape 2: RPC -----------------------------------------------------------------------

    pub fn rpc_url(&self, function: &str) -> String {
        format!("{}/rest/v1/rpc/{function}", self.config.base_url)
    }

    /// `POST /rest/v1/rpc/<name>` with the arguments as a JSON object.
    pub async fn rpc<B: Serialize + ?Sized, R: DeserializeOwned>(
        &self,
        function: &str,
        args: &B,
    ) -> Result<R, CloudError> {
        let request = self
            .request(reqwest::Method::POST, self.rpc_url(function))
            .json(args);
        decode(send(request).await?)
    }

    /// `POST /rest/v1/rpc/<name>` for the functions that answer with nothing. `increment_views`
    /// is the one this client calls: it returns `void`, so PostgREST answers 204 with no body,
    /// and decoding the empty body as JSON would turn a successful increment into an error.
    pub async fn rpc_empty<B: Serialize + ?Sized>(
        &self,
        function: &str,
        args: &B,
    ) -> Result<(), CloudError> {
        let request = self
            .request(reqwest::Method::POST, self.rpc_url(function))
            .json(args);
        accept(send(request).await?)
    }

    // -- shape 3: PostgREST -----------------------------------------------------------------

    pub fn table_url(&self, table: &str) -> String {
        format!("{}/rest/v1/{table}", self.config.base_url)
    }

    /// `GET /rest/v1/<table>` with PostgREST filters, e.g. `[("user_id", "eq.abc")]`.
    ///
    /// `columns` is passed through as PostgREST's `select`; `"*"` for every column. The filters
    /// are the caller's, verbatim — PostgREST's operator vocabulary (`eq.`, `in.`, `gt.`) belongs
    /// to the row that knows what it is asking for, not to the transport. `order` is the caller's
    /// verbatim order spec (`updated_at.desc`), spelled the same way [`Cloud::select_counted`]
    /// takes it — one unpaged read needs an order (the favourites list) and sorting its rows here
    /// instead would be this client deciding what "newest" means from a string it did not parse.
    pub async fn select<R: DeserializeOwned>(
        &self,
        table: &str,
        columns: &str,
        filters: &[(&str, &str)],
        order: Option<&str>,
    ) -> Result<Vec<R>, CloudError> {
        let mut query: Vec<(&str, &str)> = Vec::with_capacity(filters.len() + 2);
        query.push(("select", columns));
        query.extend_from_slice(filters);
        if let Some(order) = order {
            query.push(("order", order));
        }
        let request = self
            .request(reqwest::Method::GET, self.table_url(table))
            .query(&query);
        decode(send(request).await?)
    }

    /// `GET /rest/v1/<table>` with an exact row count and a slice request — PostgREST's answer
    /// to "how many rows match, give me rows `from`..`to` (0-based, inclusive)".
    ///
    /// The count comes back in the `Content-Range` response header, not in the body: `count=exact`
    /// (`Prefer`) makes PostgREST return `Content-Range: a-b/total`, and that total is the whole
    /// reason this method exists — an unbounded page control that guesses "there is a next page"
    /// is the silent pager bug. `order` is the caller's verbatim PostgREST order spec
    /// (`created_at.desc`), `range` the item slice, both mirroring what `supabase-js`'s `.order`
    /// and `.range(from, to)` send.
    pub async fn select_counted<R: DeserializeOwned>(
        &self,
        table: &str,
        columns: &str,
        filters: &[(&str, &str)],
        order: Option<&str>,
        range: Option<(usize, usize)>,
    ) -> Result<(Vec<R>, u64), CloudError> {
        let mut query: Vec<(&str, &str)> = Vec::with_capacity(filters.len() + 2);
        query.push(("select", columns));
        query.extend_from_slice(filters);
        if let Some(order) = order {
            query.push(("order", order));
        }
        let mut request = self
            .request(reqwest::Method::GET, self.table_url(table))
            .query(&query)
            .header("Prefer", "count=exact");
        if let Some((from, to)) = range {
            request = request.header("Range", format!("{from}-{to}"));
        }
        let answer = send(request).await?;
        let total = answer
            .headers
            .get(reqwest::header::CONTENT_RANGE)
            .and_then(|value| value.to_str().ok())
            .and_then(content_range_total)
            .ok_or_else(|| CloudError::Decode {
                detail: "no parseable Content-Range on a count=exact response".to_string(),
                body: answer.body.clone(),
            })?;
        let rows = decode::<Vec<R>>(answer)?;
        Ok((rows, total))
    }

    /// `POST /rest/v1/<table>` with `resolution=merge-duplicates`, which is what makes it an
    /// upsert rather than an insert that fails on a row the user already has.
    ///
    /// `return=minimal` asks for no body back: the caller already holds the rows it sent, and a
    /// representation it would throw away is a payload per favourite toggle.
    pub async fn upsert<B: Serialize + ?Sized>(
        &self,
        table: &str,
        rows: &B,
    ) -> Result<(), CloudError> {
        let request = self
            .request(reqwest::Method::POST, self.table_url(table))
            .header("Prefer", "resolution=merge-duplicates,return=minimal")
            .json(rows);
        accept(send(request).await?)
    }

    /// `DELETE /rest/v1/<table>` under the given filters.
    ///
    /// An unfiltered delete is refused here rather than sent. PostgREST reads no filter as "every
    /// row", and this client's one table is shared by every user — the request that empties it is
    /// indistinguishable on the wire from one whose filter was accidentally empty, so the
    /// distinction has to be made before it leaves.
    pub async fn delete(&self, table: &str, filters: &[(&str, &str)]) -> Result<(), CloudError> {
        require_filters(table, filters)?;
        let request = self
            .request(reqwest::Method::DELETE, self.table_url(table))
            .header("Prefer", "return=minimal")
            .query(filters);
        accept(send(request).await?)
    }

    // -- shape 4: auth ----------------------------------------------------------------------

    pub fn auth_url(&self, path: &str) -> String {
        format!(
            "{}/auth/v1/{}",
            self.config.base_url,
            path.trim_start_matches('/')
        )
    }

    /// `POST /auth/v1/<path>` — the token, refresh and sign-out endpoints RB4b and RB4c drive.
    /// The path stays the caller's because the grant types are theirs to choose, not the
    /// transport's to enumerate before anything uses them.
    pub async fn auth_post<B: Serialize + ?Sized, R: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<R, CloudError> {
        let request = self
            .request(reqwest::Method::POST, self.auth_url(path))
            .json(body);
        decode_auth(send(request).await?)
    }

    /// `POST /auth/v1/<path>` for the endpoints that answer with nothing. `logout` is the one:
    /// `auth-js` sends it with `noResolveJson` (`GoTrueAdminApi.js:49`), and decoding the empty
    /// 204 body as JSON would turn a successful sign-out into an error.
    pub async fn auth_post_empty(&self, path: &str) -> Result<(), CloudError> {
        let request = self.request(reqwest::Method::POST, self.auth_url(path));
        accept(send(request).await?)
    }

    /// `GET /auth/v1/<path>` with query parameters — `user`, and the provider metadata reads.
    pub async fn auth_get<R: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<R, CloudError> {
        let request = self
            .request(reqwest::Method::GET, self.auth_url(path))
            .query(query);
        decode_auth(send(request).await?)
    }
}

/// Refuse a delete that names no row.
///
/// Kept apart from [`Cloud::delete`] so the decision is gradeable without a runtime or a server —
/// it is the one guard in this module whose failure mode is destructive rather than visible.
fn require_filters(table: &str, filters: &[(&str, &str)]) -> Result<(), CloudError> {
    if filters.is_empty() {
        return Err(CloudError::Refused {
            status: 0,
            message: format!("refusing to DELETE every row of {table}: no filter given"),
            body: String::new(),
        });
    }
    Ok(())
}

/// Send, and separate "never got an answer" from "got an answer that says no".
///
/// The split matters to the caller: a transport failure is worth retrying and a refusal is not,
/// and collapsing them into one error is how a client comes to retry a 400 forever.
async fn send(request: reqwest::RequestBuilder) -> Result<Answer, CloudError> {
    let response = request
        .send()
        .await
        .map_err(|e| CloudError::Transport(e.to_string()))?;
    let status = response.status().as_u16();
    // Headers first, because reading the body consumes the response and `select_counted`
    // reads the count out of `Content-Range` after the body is gone.
    let headers = response.headers().clone();
    // The body is read once, as text, before anything decides what it is. `json()` would consume
    // the response and leave nothing to put in the error when it fails.
    let body = response
        .text()
        .await
        .map_err(|e| CloudError::Transport(e.to_string()))?;
    if !(200..300).contains(&status) {
        return Err(CloudError::Refused {
            status,
            message: server_message(&body),
            body,
        });
    }
    Ok(Answer { body, headers })
}

/// A 2xx, its body and its headers, already read.
///
/// The headers are only there for [`Cloud::select_counted`], which reads the count out of
/// `Content-Range`; every other shape reads the body and ignores them.
struct Answer {
    body: String,
    headers: reqwest::header::HeaderMap,
}

/// Decode a successful answer into what the call expected.
fn decode<R: DeserializeOwned>(answer: Answer) -> Result<R, CloudError> {
    serde_json::from_str(&answer.body).map_err(|e| CloudError::Decode {
        detail: e.to_string(),
        body: answer.body,
    })
}

/// What stands in for a body this client will not quote. One string, so the two sites that
/// withhold cannot drift apart: [`decode_auth`] and the refresh arm in [`super::session`].
pub(super) const WITHHELD_BODY: &str = "(auth response withheld)";

/// Decode an answer from `/auth/v1/*`, whose body is never quotable.
///
/// [`decode`] keeps the body because a PostgREST refusal is the only evidence the report will
/// ever have. Here the body IS the thing being protected: the token endpoints answer with
/// `access_token` and `refresh_token`, and `user` answers with the identity behind them.
/// [`SessionError`] already refuses to carry that text for the reason that outlives the decode —
/// an error string is a thing that gets logged, rendered and pasted into a bug report.
///
/// The refresh arm withholds the body for a 2xx that is JSON but not a session. That guard
/// cannot reach the case this one covers — a 2xx that is not JSON at all fails here, before any
/// session is attempted, and [`decode`] would quote the whole body into the modal (F25).
/// `detail` is serde's message and is kept: it says where the parse died without reproducing
/// what died.
///
/// [`SessionError`]: super::session::SessionError
fn decode_auth<R: DeserializeOwned>(answer: Answer) -> Result<R, CloudError> {
    serde_json::from_str(&answer.body).map_err(|e| CloudError::Decode {
        detail: e.to_string(),
        body: WITHHELD_BODY.to_string(),
    })
}

/// Take a successful answer that is not expected to carry anything. `Prefer: return=minimal`
/// makes the body empty by request, so there is nothing here to check that `send` has not.
fn accept(_answer: Answer) -> Result<(), CloudError> {
    Ok(())
}

/// Parse the total out of a PostgREST `Content-Range` header: `0-19/123` → 123, `*/0` → 0.
///
/// The `*` form is what PostgREST answers when no `Range` was asked for — start unknown, total
/// known — and reading it is what keeps `select_counted` honest when a caller forgets the slice.
/// Any other shape fails closed: a countless page is a page that cannot paginate.
fn content_range_total(header: &str) -> Option<u64> {
    header.rsplit('/').next()?.parse().ok()
}

/// How much server text `Display` will render before cutting it off.
///
/// Wide enough for the refusals that are meant to be read — a PostgREST constraint violation runs
/// to about eighty characters, a rate-limit message to forty — and narrow enough that a body which
/// is not a message cannot take the page with it.
const DISPLAY_TEXT_LIMIT: usize = 200;

/// Server text cut to [`DISPLAY_TEXT_LIMIT`] for rendering, with the elision and the real size
/// marked so nobody reads a clipped body as a whole one.
///
/// The cut is here rather than at the render sites because there are nine of them and they are
/// all one `"{error}"` — `browser.rs:462` and seven siblings put the `Display` string in a text
/// node, `main_menu.rs:533` does the same. The error *value* keeps the body whole: `Refused.body`
/// is still the structured refusal `share-build` answers a 429 with, and `Decode.body` is still
/// the text the decode failed on. Only the line a human reads is bounded.
fn clipped(text: &str) -> std::borrow::Cow<'_, str> {
    if text.len() <= DISPLAY_TEXT_LIMIT {
        return std::borrow::Cow::Borrowed(text);
    }
    // Back off to a char boundary before slicing. Cutting a UTF-8 sequence in half by byte offset
    // is what panics the loopback request parser, and a panic here would be reached by any server
    // answer long enough to clip.
    let mut end = DISPLAY_TEXT_LIMIT;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    std::borrow::Cow::Owned(format!("{}… ({} bytes)", &text[..end], text.len()))
}

/// Pull the server's own words out of an error body.
///
/// Supabase speaks two error shapes — the edge functions answer `{"error": "..."}` and PostgREST
/// answers `{"message": "...", "hint": ..., "code": ...}` — and anything else that comes back is
/// returned as it arrived. The fallback is the raw body rather than wording of ours on purpose:
/// an unrecognised shape is the case where a phrase like "request failed" would throw away the
/// only evidence the report will ever have.
fn server_message(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return "(empty response body)".to_string();
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) else {
        return trimmed.to_string();
    };
    for key in ["error", "message", "error_description", "msg"] {
        if let Some(text) = value.get(key).and_then(serde_json::Value::as_str) {
            if !text.is_empty() {
                return text.to_string();
            }
        }
    }
    trimmed.to_string()
}
