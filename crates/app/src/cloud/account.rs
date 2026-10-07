//! The shell's handle on the session: who is signed in, the one door every cloud call takes, and
//! the browser half of the OAuth redirect.
//!
//! [`super::session::SessionManager`] owns the rule; this owns the fact that there is exactly one
//! of it, that Dioxus has to be told when it changes, and that reaching it from a component means
//! reaching across an `await`.
//!
//! # The `TOKEN_REFRESHED` swallow, re-derived
//!
//! The beta's comment is the bug report, not the patch (`services/auth.ts:47-58`): a same-user
//! token rotation hands back a brand-new `User` object, and that fresh identity alone retriggered
//! every effect keyed on `user` — one visibility toggle remounted the whole My Builds grid
//! mid-write, because `updateBuildVisibility` refreshed the session, the refresh re-set the store,
//! and `BuildsPage`'s effect saw a "new" user and refetched.
//!
//! **The same defect exists here and nothing about React carried it.** A write to a
//! `Signal<Option<AuthUser>>` re-runs every `use_effect` that reads it, whether or not the value
//! changed — Dioxus marks the signal dirty on `set`, not on difference. So the rule has to be
//! restated in this framework's terms, and the restating is what [`publishes`] is.
//!
//! The beta keys on the auth EVENT, and there are no events here — there are call sites. So the
//! reason is passed in: [`Account::prepared`] republishes as [`IdentityChange::Refreshed`]
//! because it cannot know whether the pre-refresh fired, and the swallow makes the common answer
//! a no-op. That is the same shape as the original, which also republished on every rotation and
//! dropped the same-id ones — and it keeps the half of the original that matters most: a refresh
//! that *did* change the answer, because it failed and dropped the session, still publishes, so
//! a user whose token died stops being shown as signed in.
//!
//! `USER_UPDATED` still propagates in the beta because it is the event that carries profile
//! changes. Its counterpart here is [`IdentityChange::UserUpdated`], which nothing raises yet —
//! it is RB4f's, and it exists now so that row inherits the distinction rather than rediscovering
//! why keying on the id alone was not enough.
//!
//! # One manager, behind a lock rather than a signal
//!
//! `SessionManager::client` takes `&mut self` and awaits inside, which is the shape that makes
//! the pre-refresh unskippable. A `Signal<SessionManager>` cannot hold that: the write guard is a
//! `RefCell` borrow, a second call while the first is in flight panics on it, and "signed out
//! because you double-clicked" is a failure mode worth designing out rather than catching.
//!
//! An async mutex holds it instead, and the serialisation it buys is not incidental — **a GoTrue
//! refresh token is single-use**, which [`super::session::refresh_failure`] already relies on to
//! decide that a refusal means the session is gone. Two calls refreshing in parallel would spend
//! it twice, and the loser's refusal reads as a dead session. Under the lock the second caller
//! re-checks expiry against the session the first one just stored and finds nothing to do.
//!
//! [`Account::prepared`] hands back an owned [`Cloud`] and releases the lock, so it is the
//! refresh that is serialised and not the request. That is the seam RB4d–RB4j call:
//! `account.prepared().await?.invoke(…)`, with no way to reach a client that skipped the rule.

use super::oauth::{self, AuthProvider, OAuthError};
use super::session::{Session, SessionManager};
use super::{Cloud, CloudError};
use dioxus::prelude::*;
use futures_util::lock::Mutex;
use serde_json::Value;
use std::rc::Rc;

// ============================================================
// Who is signed in.
// ============================================================

/// The identity, projected down to what this app draws with.
///
/// The beta's own projection (`Header.tsx:910-911`): `full_name`, then `name`, then a fallback —
/// except the fallback is not here. A user whose provider sent no name has no name, and deciding
/// what to draw instead is the drawing surface's job; baking "Account" in would put a fabricated
/// value where an absent one belongs, which is the habit [`crate::cloud`] exists to break.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthUser {
    pub id: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

/// Read the identity out of a session, or `None` if it carries none.
///
/// A user with no `id` is not a user: the id is what every owner-scoped row is keyed on, and one
/// missing is a session that would look signed in and own nothing.
pub fn identity(session: &Session) -> Option<AuthUser> {
    let user = session.user()?;
    let id = user.get("id").and_then(Value::as_str)?;
    let metadata = user.get("user_metadata");
    let field = |name: &str| {
        metadata
            .and_then(|m| m.get(name))
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    };
    Some(AuthUser {
        id: id.to_string(),
        display_name: field("full_name").or_else(|| field("name")),
        avatar_url: field("avatar_url"),
    })
}

/// Why the identity is being republished. The beta's auth events, minus the ones that have no
/// counterpart in a client with no subscription model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentityChange {
    /// A sign-in completed, or a stored session was restored at boot.
    SignedIn,
    /// A call went through [`Account::prepared`], which may or may not have rotated the token.
    Refreshed,
    /// The profile changed under the same id. Nothing raises this yet — RB4f does.
    UserUpdated,
    /// Signed out, here or on the server.
    SignedOut,
}

/// Whether this change is worth writing to the signal.
///
/// `if (event === 'TOKEN_REFRESHED' && user?.id === lastUserId) return;` — `auth.ts:69`, with the
/// held id read off the signal rather than kept in a closure variable, so there is one record of
/// who is signed in instead of two that can disagree.
///
/// Only a rotation is ever swallowed. A sign-in that lands on the id already held still publishes
/// (signing in as yourself is a completed act, and something is waiting on it), and so does a
/// sign-out.
pub fn publishes(change: IdentityChange, held: Option<&str>, next: Option<&str>) -> bool {
    !(change == IdentityChange::Refreshed && held == next)
}

// ============================================================
// What the user has to be told.
// ============================================================

/// A cloud act that did not complete. Same two fields as [`crate::build_io::BuildIoOutcome`]'s
/// refusal arm and for the same reason: the act in the user's terms, and the refusal in the words
/// of whatever refused it.
#[derive(Clone, PartialEq)]
pub struct AccountRefusal {
    pub action: String,
    pub reason: String,
}

// ============================================================
// The handle.
// ============================================================

/// The session, the identity signal, and the report — provided once by the shell.
///
/// `Clone` rather than `Copy` because of the `Rc`; every consumer holds the same manager and the
/// same two signals.
#[derive(Clone)]
pub struct Account {
    /// `Err` when the build carries no project. Held rather than discarded so every call reports
    /// it by the name of the variable that is missing, which is the whole of
    /// [`CloudError::NotConfigured`]'s reason for existing.
    manager: Rc<Mutex<Result<SessionManager, CloudError>>>,
    user: Signal<Option<AuthUser>>,
    report: Signal<Option<AccountRefusal>>,
}

impl Account {
    /// Built once, in the shell's body. Synchronous on purpose — see
    /// [`SessionManager::new`]; the storage read and the callback happen in [`Account::start`].
    pub fn new() -> Self {
        Self {
            manager: Rc::new(Mutex::new(
                Cloud::from_build_env().and_then(SessionManager::new),
            )),
            user: Signal::new(None),
            report: Signal::new(None),
        }
    }

    /// Who is signed in, for the UI. Reading this subscribes the caller, which is what the
    /// swallow above exists to keep quiet.
    pub fn user(&self) -> Signal<Option<AuthUser>> {
        self.user
    }

    pub fn report(&self) -> Signal<Option<AccountRefusal>> {
        self.report
    }

    /// A prepared client, with a token that is good when it is handed over. **The only door.**
    ///
    /// Goes through [`SessionManager::client`], so the pre-refresh cannot be skipped, and
    /// republishes the identity afterwards on both outcomes — a refresh that failed has already
    /// dropped the session, and a UI still showing the old name would be the one thing worse than
    /// the error.
    pub async fn prepared(&self) -> Result<Cloud, CloudError> {
        let mut guard = self.manager.lock().await;
        let manager = guard.as_mut().map_err(|e| e.clone())?;
        let outcome = manager.client().await.cloned();
        self.publish(IdentityChange::Refreshed, manager.session());
        outcome
    }

    /// Restore whatever session is stored, then finish a sign-in if this page load is one coming
    /// back. Called once, from the shell, after mount.
    ///
    /// The order is load-bearing in one direction only: a callback overwrites a restored session,
    /// which is right, and a restore can never overwrite a callback.
    pub async fn start(&self) {
        // The URL is read BEFORE the lock is taken, and that is not tidiness. The lock guards the
        // manager; a DOM read is not the manager's, and holding a lock across one means a user
        // who clicks Sign in during boot waits on a `document::eval` — or waits forever, if that
        // eval never answers. A lock whose hold time depends on the browser is a lock that can
        // deadlock the only door every cloud call takes.
        let callback = read_location()
            .await
            .map(|location| oauth::parse_parameters(&location.href))
            .filter(oauth::is_callback);
        if callback.is_some() {
            // Out of the address bar before anything else can go wrong. `auth-js` clears only on
            // success; a live access token left visible because the identity fetch failed is the
            // one place being faithful to the port costs something real.
            clear_fragment();
        }

        let mut guard = self.manager.lock().await;
        if let Ok(manager) = guard.as_mut() {
            manager.restore_stored().await;
            // A stored session that would not read is reported rather than swallowed. The beta
            // signs the user out in silence here; the difference is whether anyone ever finds out
            // that a blob on this origin is unreadable, and the error deliberately quotes none of
            // it (see `SessionError`).
            if let Some(discarded) = manager.discarded() {
                self.refuse("Restore your session", discarded.to_string());
            }
        }

        // After the restore, so a sign-in coming back overwrites a stale stored session and never
        // the other way round.
        if let Some(params) = callback {
            if let Err(e) = self.complete_callback(&mut guard, &params).await {
                self.refuse("Sign in", e.to_string());
            }
        }

        let session = guard.as_ref().ok().and_then(SessionManager::session);
        self.publish(IdentityChange::SignedIn, session);
        // The lock goes before the reconcile, not after: `sync_favorites` takes the one door
        // ([`Account::prepared`]) and that door takes this lock.
        drop(guard);
        self.sync_favorites().await;
    }

    /// Reconcile the local favourites list with the account's, once auth has resolved to a user.
    ///
    /// **Called from [`Account::start`] and nowhere else, because start is the only place auth
    /// resolves in this client.** The beta has to guard this with a `lastSyncedUserId`
    /// (`authStore.ts:33-41`): its listener re-fires on every token rotation with the same user,
    /// and an unguarded sync would run per tick. Here a sign-in is a page load — `sign_in`
    /// navigates to the provider and the answer comes back through `start` — so a hook that runs
    /// once at boot has nothing to guard against. RB5's desktop sign-in is what would change
    /// that, and it is the row that would move this call.
    ///
    /// Signed out, this does nothing: there is no account to reconcile against, and the local
    /// list is already the whole truth.
    ///
    /// The failure is reported, unlike the per-star mirror. A sync is the step that decides what
    /// this browser thinks the user's favourites ARE, so one that did not run is a list quietly
    /// missing whatever another device added — the opposite of a single star write, which the
    /// local list has already answered correctly.
    async fn sync_favorites(&self) {
        let Some(user_id) = self.user.peek().as_ref().map(|user| user.id.clone()) else {
            return;
        };
        let outcome = match self.prepared().await {
            Ok(cloud) => super::favorites::sync(&cloud, &user_id).await,
            Err(e) => Err(e),
        };
        if let Err(e) = outcome {
            self.refuse("Sync your favourites", e.to_string());
        }
    }

    /// Send the browser off to the provider. Everything after this happens on the next page load.
    ///
    /// Web only in effect, and not in code: the URL is the whole of what desktop needs too, and
    /// RB5's sign-in is the same string handed to the OS browser. The caller is what decides —
    /// see [`crate::main_menu`], where the desktop rows are drawn disabled with their reason.
    pub async fn sign_in(&self, provider: AuthProvider) {
        let outcome = self.begin_sign_in(provider).await;
        if let Err(e) = outcome {
            self.refuse(format!("Sign in with {}", provider.label()), e.to_string());
        }
    }

    /// Draw a verifier, park it, and leave with its challenge (F01).
    ///
    /// The web arm used to send `None` here and take whatever the fragment came back with, which
    /// meant any URL on this origin could hand this client a session. It is PKCE now, the same
    /// flow the desktop has run since F62 and the same one the beta runs since `flowType: 'pkce'`
    /// — one origin, one flow, and the compatibility argument that once justified implicit now
    /// requires this.
    ///
    /// The draw is a browser read rather than a `getrandom` call, and that is the manifest's
    /// decision rather than a preference: `getrandom` is declared for `cfg(not(wasm32))` only
    /// (`Cargo.toml:117`), because on `wasm32-unknown-unknown` it compiles only when the build
    /// names a backend. `crypto.getRandomValues` is the CSPRNG this target does have, it is the
    /// one `auth-js` itself draws from (`helpers.js:239`), and reaching it through
    /// `document::eval` is how everything else in this app talks to the page.
    async fn begin_sign_in(&self, provider: AuthProvider) -> Result<(), OAuthError> {
        let cloud = self.prepared().await?;
        let location = read_location()
            .await
            .ok_or(CloudError::Unusable("window.location"))?;
        let verifier = draw_verifier().await.ok_or(OAuthError::NoVerifier)?;
        let challenge = oauth::pkce_challenge(&verifier);
        // Parked BEFORE the navigation, and the order is the whole of it: `navigate` is the last
        // thing this page does, so a verifier written after it is a verifier written by a document
        // that is already leaving. A failed park fails the sign-in here rather than at the
        // callback, where the only thing left to say would be that the secret is missing.
        {
            let guard = self.manager.lock().await;
            let manager = guard.as_ref().map_err(|e| e.clone())?;
            manager.remember_code_verifier(&verifier)?;
        }
        let url = oauth::authorize_url(&cloud, provider, &location.redirect_to, Some(&challenge));
        navigate(&url);
        Ok(())
    }

    /// The provider URL for a caller that owns the round trip itself — RB5's desktop sign-in.
    ///
    /// The web flow reads `redirect_to` off `window.location` because the page is the only thing
    /// that knows the deployed origin. A desktop sign-in has no such origin: the answer comes back
    /// to a loopback listener whose port did not exist a moment ago, so the address is the
    /// CALLER's to supply and this layer must not guess at one.
    ///
    /// The `challenge` is the caller's for a sharper reason (F62): it is the public half of a
    /// verifier that must never leave the process that will spend it, so this layer is handed the
    /// half that is safe to put in a URL and is never given the half that is not.
    ///
    /// Still no target named here ([`super`]'s rule): a redirect address is a string, a challenge
    /// is a string, and which strings they are belongs to whoever can receive the answer.
    pub async fn authorize_url(
        &self,
        provider: AuthProvider,
        redirect_to: &str,
        challenge: &str,
    ) -> Result<String, OAuthError> {
        let cloud = self.prepared().await?;
        Ok(oauth::authorize_url(
            &cloud,
            provider,
            redirect_to,
            Some(challenge),
        ))
    }

    /// Finish a sign-in whose callback arrived **outside a page load** — RB5's desktop flow.
    ///
    /// [`Account::start`] is the web shape: the answer IS the next page load, so the callback is
    /// read off the address bar once at boot and the restore runs beside it. Nothing about that
    /// applies here. The app never navigated, there is nothing to restore, and what was caught is
    /// handed in by whoever caught it.
    ///
    /// **It takes a code and a verifier, and that is the whole of F62's fix at this layer.** The
    /// version of this method that stood here took a href and read tokens off it, which made this
    /// the one entry point in the client that would turn bytes off a loopback socket into a stored
    /// session. There is now no such entry point: the only thing the listener can hand up is an
    /// authorization code, and a code is worth nothing without the verifier this process drew and
    /// never published. An injected code fails the exchange at GoTrue rather than being believed
    /// here.
    ///
    /// What it keeps from `start`, because these are properties of signing in rather than of a
    /// page load: the identity is published so the menu redraws, and the favourites are
    /// reconciled. [`Account::sync_favorites`]'s own doc names this row as the one that would
    /// move that call — a desktop sign-in is the first one in this client that is not a boot, so
    /// a reconcile hooked to boot would never run for it.
    pub async fn finish_sign_in(&self, code: &str, verifier: &str) -> Result<(), OAuthError> {
        let mut guard = self.manager.lock().await;
        let outcome = self.exchange_code(&mut guard, code, verifier).await;
        let session = guard.as_ref().ok().and_then(SessionManager::session);
        self.publish(IdentityChange::SignedIn, session);
        // The lock goes before the reconcile, for `start`'s reason: `sync_favorites` takes the
        // one door and that door takes this lock.
        drop(guard);
        if outcome.is_ok() {
            self.sync_favorites().await;
        }
        outcome
    }

    /// End the session, here and on the server.
    ///
    /// The short-link cache goes with it (`authStore.ts:76`). That cache names a row and claims
    /// this browser may update it, so leaving it behind would hand the NEXT user on this browser
    /// a "Copy short link" that silently re-writes the previous user's unlisted build. The
    /// favourites cache goes for the sibling reason (`authStore.ts:81`) and costs nothing: they
    /// live on the account and [`super::favorites::sync`] pulls them back down at the next login,
    /// whereas leaving them would show the next user someone else's stars and then push them up
    /// to their account. The owner TOKEN map deliberately survives both: those are anonymous
    /// builds belonging to the browser rather than to any account, and clearing them would orphan
    /// every one of them. The layout's claim to this account goes too, so a layout edited after
    /// sign-out cannot overwrite the account's copy at the next sign-in ([`crate::layout_sync`]).
    pub async fn sign_out(&self) {
        let mut guard = self.manager.lock().await;
        let outcome = match guard.as_mut() {
            Ok(manager) => {
                let outcome = manager.sign_out().await;
                self.publish(IdentityChange::SignedOut, manager.session());
                outcome
            }
            Err(e) => Err(e.clone()),
        };
        super::owner_store::clear_quick_share();
        super::favorites::clear_cache();
        crate::layout_sync::forget_user();
        if let Err(e) = outcome {
            self.refuse("Sign out", e.to_string());
        }
    }

    async fn complete_callback(
        &self,
        guard: &mut Result<SessionManager, CloudError>,
        params: &std::collections::BTreeMap<String, String>,
    ) -> Result<(), OAuthError> {
        let manager = guard.as_mut().map_err(|e| e.clone())?;
        // Signed out at this point, so this is the anon-bearing client and no refresh happens —
        // it is still taken through the one door rather than around it.
        let cloud = manager.client().await?.clone();
        // Taken before the exchange and cleared either way, so a verifier cannot outlive the one
        // callback it was drawn for — see [`SessionManager::take_code_verifier`].
        let verifier = manager.take_code_verifier().await;
        let session = oauth::complete(&cloud, params, verifier.as_deref(), now_unix()).await?;
        manager.sign_in(session);
        Ok(())
    }

    /// [`Account::complete_callback`]'s desktop twin: the same door and the same lock, with the
    /// exchange in place of the fragment read.
    async fn exchange_code(
        &self,
        guard: &mut Result<SessionManager, CloudError>,
        code: &str,
        verifier: &str,
    ) -> Result<(), OAuthError> {
        let manager = guard.as_mut().map_err(|e| e.clone())?;
        let cloud = manager.client().await?.clone();
        let session = oauth::exchange_code(&cloud, code, verifier, now_unix()).await?;
        manager.sign_in(session);
        Ok(())
    }

    /// Republish the identity because the profile behind it changed — the beta's `USER_UPDATED`.
    ///
    /// **Nothing about the session moved, and that is the point.** A profile edit rewrites a
    /// `profiles` row; the access token, the id and the JWT metadata this app projects an
    /// [`AuthUser`] out of are byte-identical on both sides of it. So there is no refresh to
    /// piggyback on and no value here to compare — [`publishes`] lets this change through
    /// unconditionally, which is what re-runs the effects keyed on the user signal and redraws
    /// whatever was showing the old name.
    ///
    /// The lock is taken to read the session, not to change it. [`super::profile::update`] is the
    /// one caller, and it calls this after [`Account::prepared`] has already released.
    pub async fn user_updated(&self) {
        let guard = self.manager.lock().await;
        let session = guard.as_ref().ok().and_then(SessionManager::session);
        self.publish(IdentityChange::UserUpdated, session);
    }

    fn publish(&self, change: IdentityChange, session: Option<&Session>) {
        let next = session.and_then(identity);
        let mut user = self.user;
        let write = {
            let held = user.peek();
            publishes(
                change,
                held.as_ref().map(|held| held.id.as_str()),
                next.as_ref().map(|next| next.id.as_str()),
            )
        };
        if write {
            user.set(next);
        }
    }

    /// Report a cloud act that did not complete. `pub(crate)` so the surfaces RB4e added can use
    /// the one refusal channel the account already owns rather than growing a second one.
    pub(crate) fn refuse(&self, action: impl Into<String>, reason: String) {
        let mut report = self.report;
        report.set(Some(AccountRefusal {
            action: action.into(),
            reason,
        }));
    }
}

impl Default for Account {
    fn default() -> Self {
        Self::new()
    }
}

/// Seconds since the epoch. Same clock and same bargain as [`super::session`]'s — `web-time` is
/// `Date.now()` on wasm, where `std`'s panics.
fn now_unix() -> u64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0)
}

// ============================================================
// The browser half.
// ============================================================

/// Where the page is, and where a provider should send it back to.
struct PageLocation {
    href: String,
    redirect_to: String,
}

/// `window.location.origin + BASE_URL` — `auth.ts:10`, where `BASE_URL` is `'/'`
/// (`vite.config.ts:162`). The origin is read rather than configured because it is the one thing
/// only the running page knows, and it is also what the provider's allow-list is written against:
/// deploying the successor under a sub-path would change this string and the allow-list together,
/// which is a deployment decision and RB4i's.
/// Answered as a JSON *string* rather than as an object, matching the one shape this app already
/// proves resolves ([`crate::build_io`]'s fragment read and [`super::session`]'s storage read both
/// return strings). An `Eval` that never answers is indistinguishable from one that answered
/// `None`, and the cost of guessing wrong is paid by whatever is waiting on the caller.
const READ_LOCATION: &str = "\
try {\
  return JSON.stringify({ href: window.location.href, origin: window.location.origin });\
} catch (_) { return null; }";

/// Take the auth parameters off the address bar, on both arms.
///
/// `replaceState` rather than `auth-js`'s `window.location.hash = ''`, for the reason
/// [`crate::build_io`]'s own fragment-taker gives: assigning the hash pushes a history entry, and
/// Back would then return to a URL still carrying a refresh token.
///
/// **The search is now pruned rather than kept whole**, and the rule that kept it whole is the
/// rule that prunes it: the query is not ours, so what comes off it is exactly the four parameters
/// that ARE — `code` and the error trio GoTrue writes. `previewCapture` and anything else on it
/// still belong to whoever put them there and still survive. A spent `code` left in the address
/// bar is one a reload re-presents and a history entry keeps, which is the shape F04 was about,
/// one arm over; `auth-js` deletes the same parameter for the same reason
/// (`GoTrueClient.js:1501-1503`).
const CLEAR_FRAGMENT: &str = "\
try {\
  var url = new URL(window.location.href);\
  ['code', 'error', 'error_code', 'error_description'].forEach(function (p) {\
    url.searchParams.delete(p);\
  });\
  window.history.replaceState(null, '', url.pathname + url.search);\
} catch (_) {}";

/// 56 bytes from the browser's CSPRNG, as 112 lower-case hex characters.
///
/// The length and the alphabet are `auth-js`'s, not a choice: `generatePKCEVerifier` draws 56
/// values and renders each one's low byte as two hex digits (`helpers.js:228-241`), which lands
/// inside RFC 7636 §4.1's 43–128 unreserved characters. Matching it matters for the same reason
/// [`crate::cloud::Config::pkce_verifier_key`] matters — one origin, two clients, and a verifier
/// one of them parked is one the other may have to spend.
///
/// Answers `None` rather than falling back to anything. `auth-js` falls back to `Math.random()`
/// when `crypto` is absent (`helpers.js:231-238`); this does not, because a verifier drawn from a
/// predictable source is the F65 defect exactly, and a browser with no `crypto` is one this app
/// cannot sign in from. Failing here says so.
const DRAW_VERIFIER: &str = "\
try {\
  var bytes = new Uint8Array(56);\
  crypto.getRandomValues(bytes);\
  return Array.from(bytes, function (b) { return ('0' + b.toString(16)).slice(-2); }).join('');\
} catch (_) { return null; }";

async fn draw_verifier() -> Option<String> {
    let value = document::eval(DRAW_VERIFIER).await.ok()?;
    let text = value.as_str()?.to_string();
    (text.len() == 112 && text.bytes().all(|b| b.is_ascii_hexdigit())).then_some(text)
}

async fn read_location() -> Option<PageLocation> {
    let value = document::eval(READ_LOCATION).await.ok()?;
    let value: Value = serde_json::from_str(value.as_str()?).ok()?;
    let href = value.get("href")?.as_str()?.to_string();
    let origin = value.get("origin")?.as_str()?;
    Some(PageLocation {
        redirect_to: format!("{origin}/"),
        href,
    })
}

fn clear_fragment() {
    document::eval(CLEAR_FRAGMENT);
}

/// Leave for the provider.
///
/// The URL reaches JS as a JSON string literal rather than through `{:?}`, for
/// [`crate::build_file`]'s reason: Rust's debug escaping is close enough to JS's to be tempting
/// and is not the same language.
fn navigate(url: &str) {
    let Ok(url) = serde_json::to_string(url) else {
        return;
    };
    document::eval(&format!(
        "try {{ window.location.assign({url}); }} catch (_) {{}}"
    ));
}

// ============================================================
// The receipt.
// ============================================================

/// What the last cloud act has to say for itself, when it could not complete.
///
/// Mounted at the shell root beside [`crate::build_io::BuildIoReportHost`], and for the same
/// containment reason. Only refusals reach it: a sign-in that worked is announced by the menu
/// showing a name, and a modal confirming what the user just watched happen is noise.
#[component]
pub fn AccountReportHost() -> Element {
    let mut report = use_context::<Account>().report();
    let Some(refusal) = report.read().clone() else {
        return rsx! {};
    };

    rsx! {
        crate::modal::Modal {
            title: "That didn't work".to_string(),
            size: crate::modal::ModalSize::Md,
            on_close: move |_| report.set(None),
            div { class: "build-report",
                p { class: "build-report__line", "{refusal.action} could not complete." }
                div { class: "load-state error", "{refusal.reason}" }
            }
        }
    }
}
