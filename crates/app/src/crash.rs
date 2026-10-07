//! Crash reporting: the panic hook, and the Sentry event it hands to the ingest API.
//!
//! The beta reports crashes through `@sentry/react`, with `@sentry/vite-plugin` uploading source
//! maps so a minified frame resolves back to a line of TypeScript. Neither has a
//! `wasm32-unknown-unknown` equivalent, which is the open question RB4h was written to close.
//! This module is the answer: **Sentry, without an SDK.**
//!
//! # Why there is no SDK here
//!
//! Sentry's ingest is an ordinary HTTPS endpoint that takes an envelope — a few newline-separated
//! JSON documents — and its DSN carries a public key that is meant to ship inside a browser. The
//! JavaScript SDK is a convenience over that endpoint, not a gatekeeper to it, so a client that
//! can POST can report. Three things make writing the envelope here the cheaper side of the trade:
//!
//! * `@sentry/wasm`, the integration that would parse WASM frames, is published to npm and has
//!   never been published as a CDN bundle. `crates/app` has no JavaScript bundler, so adopting it
//!   means adopting one.
//! * The JS SDK is a browser API. It would give the web build crash reporting and the desktop
//!   build nothing — the `None` twin standing in for the platform that got nothing, which is the
//!   defect [`crate::build_file`] and [`crate::cloud`] both carry the note about.
//! * An envelope is a string. Both targets can produce one, and the whole target split collapses
//!   to the last inch: how the bytes leave ([`deliver`]).
//!
//! # A panic hook cannot await, so the delivery is the design
//!
//! `wasm32-unknown-unknown` aborts on panic. The hook runs first, but everything after it is a
//! trap, so a future spawned here would be queued against an executor that never runs again —
//! reporting that compiles, passes review, and sends nothing. The web arm therefore hands the
//! envelope to `navigator.sendBeacon`, which is the browser API for exactly this: the request is
//! the browser's the moment it is handed over, and it completes whether or not the page (or the
//! module) survives. It also costs no CORS preflight — a beacon's `text/plain` body is a
//! safelisted content type, and Sentry accepts an envelope under it.
//!
//! The desktop arm unwinds rather than traps, so it can do the honest thing and wait: a blocking
//! POST on a thread of its own, joined, so the process cannot exit before the report leaves.
//!
//! # What a frame is worth here, and what it is not
//!
//! The panic's own `file:line:col` is exact and needs nothing to read it, so it is the frame the
//! event carries. The JavaScript stack is captured beside it as `extra.js_stack`, and on the web
//! build every frame in it is an opaque `wasm-function[1234]`: neither the debug nor the release
//! wasm carries a `name` section. Symbolicating those is reachable and not done here — the
//! release wasm ships full DWARF (`.debug_info`, `.debug_line`, `.debug_str`), which is the input
//! Sentry's WASM symbolication consumes, and what stands between it and readable frames is a
//! `build_id` custom section plus a `sentry-cli debug-files upload`, both of which belong to a
//! release pipeline this repo does not have yet. Capturing the stack now is what keeps that
//! upgrade a build step rather than a rewrite: a report sent without it could never be
//! symbolicated later.

use serde_json::json;
use std::panic::PanicHookInfo;
use std::sync::atomic::{AtomicBool, Ordering};

/// How long the desktop arm waits for the report to leave before giving up on it.
#[cfg(not(target_arch = "wasm32"))]
const DELIVERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// The project crash events go to.
///
/// Parsed from the DSN rather than assembled from parts because the DSN is the one string Sentry
/// hands you and the one a reader can compare against the project page. The key rides in the
/// query string rather than in an `X-Sentry-Auth` header for the web arm's sake: a beacon sets no
/// headers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dsn {
    envelope_url: String,
}

/// Why a build has no crash reporting. Both arms name the variable, for the reason
/// [`crate::cloud::CloudError`]'s twin does: "not configured" without one sends the reader to the
/// wrong file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DsnFault {
    /// The variable is unset. An ordinary local build, and not an error.
    NotConfigured(&'static str),
    /// The variable is set and is not a DSN. A build-env bug, and one that no runtime symptom
    /// would ever lead anyone to.
    Unusable(&'static str),
}

impl std::fmt::Display for DsnFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotConfigured(var) => write!(f, "{var} is unset"),
            Self::Unusable(var) => write!(f, "{var} is set and is not a Sentry DSN"),
        }
    }
}

impl Dsn {
    /// The DSN baked into this build, or the reason there is none.
    ///
    /// `option_env!` rather than `env!` for the reason [`crate::cloud::Config::from_build_env`]
    /// gives: a build without it must still compile and run, because most builds of this planner
    /// are somebody's local one.
    pub fn from_build_env() -> Result<Self, DsnFault> {
        let raw = option_env!("SIDEKICK_SENTRY_DSN")
            .ok_or(DsnFault::NotConfigured("SIDEKICK_SENTRY_DSN"))?;
        Self::parse(raw).ok_or(DsnFault::Unusable("SIDEKICK_SENTRY_DSN"))
    }

    /// `https://<public key>@<host>/<project id>` — the shape Sentry's project page hands out.
    ///
    /// Everything it needs is required to be present: a DSN missing its key or its project id
    /// would build a URL that 400s on every crash, which is a failure nobody is there to read.
    pub fn parse(raw: &str) -> Option<Self> {
        let url = reqwest::Url::parse(raw.trim()).ok()?;
        let key = url.username();
        let host = url.host_str().unwrap_or_default();
        let project = url.path().trim_matches('/');
        if key.is_empty() || host.is_empty() || project.is_empty() || project.contains('/') {
            return None;
        }
        Some(Self {
            envelope_url: format!(
                "{}://{host}/api/{project}/envelope/?sentry_key={key}&sentry_version=7",
                url.scheme()
            ),
        })
    }

    pub fn envelope_url(&self) -> &str {
        &self.envelope_url
    }
}

/// What the hook learned about a panic, in the terms the event is built from.
///
/// Borrowed from the hook's own `PanicHookInfo` rather than owned, so building the event allocates
/// only what it sends. The struct exists so that everything above the four-line hook adapter is
/// testable without a panic: a `PanicHookInfo` cannot be constructed outside the runtime.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Crash<'a> {
    /// The panic message. `None` when the payload is neither `&str` nor `String`, which is a
    /// panic raised by something other than the `panic!` family.
    pub message: Option<&'a str>,
    /// Source location, absent only when the panic carries none.
    pub file: Option<&'a str>,
    pub line: u32,
    pub column: u32,
    /// The JavaScript stack at hook time, on the target that has one.
    pub stack: Option<String>,
}

/// The panic as Sentry stores it: the type line every event of this kind shares.
const EXCEPTION_TYPE: &str = "panic";

/// What a panic with an unreadable payload says. Named rather than inlined so the grouping key it
/// becomes is one string rather than one per call.
const UNREADABLE_PAYLOAD: &str = "panic with a payload that is neither &str nor String";

/// Whether this panic gets sent: consent first, then the one-per-process rule.
///
/// **The order is the whole of it, and it is why this is a function rather than two lines in
/// [`report`].** Asking [`REPORTED`] first would burn the one-shot on a crash that was never
/// going to be sent, so a user who opted out, crashed, then opted back in would have spent
/// their only report on the crash nobody saw. Consent is asked first, and the one-shot is only
/// claimed by a report that is actually going.
///
/// Asked per crash rather than around [`install`], so the switch takes effect on the next crash
/// instead of the next launch — a setting you must restart the app to apply is one people
/// reasonably read as not having worked. It also leaves the hook installed and still chained to
/// the previous one, so opting out of *reporting* does not also opt out of the console dump.
fn should_report() -> bool {
    if !reporting_enabled() {
        return false;
    }
    !REPORTED.swap(true, Ordering::SeqCst)
}

/// The event this crash reports as, framed as an envelope ready to POST.
///
/// `at_secs` is passed in rather than read, because a function that reads the clock is a function
/// whose output cannot be asserted on.
pub fn envelope(event_id: &str, crash: &Crash, at_secs: f64) -> String {
    let mut event = json!({
        "event_id": event_id,
        "timestamp": at_secs,
        // Not "javascript": the frames are Rust source locations, and the platform decides which
        // symbolication Sentry attempts on them.
        "platform": "other",
        "level": "fatal",
        "logger": "panic",
        "release": format!("sidekick@{}", crate::app_info::APP_VERSION),
        "environment": if cfg!(debug_assertions) { "development" } else { "production" },
        "tags": {
            // `cfg!` rather than `#[cfg]` so both spellings are compiled on both targets and
            // neither can rot behind the other.
            "target": if cfg!(target_arch = "wasm32") { "wasm32" } else { "native" },
        },
        "exception": {
            "values": [{
                "type": EXCEPTION_TYPE,
                "value": crash.message.unwrap_or(UNREADABLE_PAYLOAD),
            }],
        },
    });

    // The frame is attached only when the panic named a place. A frame with no filename groups
    // every crash in the app together, which is worse than no frame at all.
    if let Some(file) = crash.file {
        event["exception"]["values"][0]["stacktrace"] = json!({
            "frames": [{
                "filename": file,
                "lineno": crash.line,
                "colno": crash.column,
                "in_app": true,
            }],
        });
    }
    if let Some(stack) = &crash.stack {
        event["extra"] = json!({ "js_stack": stack });
    }

    let payload = event.to_string();
    let header = json!({ "event_id": event_id }).to_string();
    let item = json!({
        "type": "event",
        "content_type": "application/json",
        "length": payload.len(),
    })
    .to_string();
    format!("{header}\n{item}\n{payload}\n")
}

/// An event id: 32 lowercase hex characters, which is what Sentry means by a UUID here.
///
/// Derived from the crash and the clock through the SHA-256 the quick-share fingerprint already
/// pulls in, rather than from a random source. Two reasons, and neither is tidiness: a browser and
/// a desktop process have different random sources, and a derived id is one an assertion can name.
/// Identity is what it is for — Sentry drops a second event bearing an id it has already stored —
/// so two distinct crashes must not collide, and the timestamp is what separates two panics that
/// are otherwise the same line twice.
pub fn event_id(crash: &Crash, at_secs: f64) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(crash.message.unwrap_or(UNREADABLE_PAYLOAD).as_bytes());
    hasher.update(crash.file.unwrap_or_default().as_bytes());
    hasher.update(crash.line.to_le_bytes());
    hasher.update(crash.column.to_le_bytes());
    hasher.update(at_secs.to_le_bytes());
    hasher
        .finalize()
        .iter()
        .take(16)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Seconds since the epoch, from the clock both targets have.
///
/// `web_time` rather than `std::time`, for the reason `Cargo.toml` states beside it:
/// `SystemTime::now()` panics on `wasm32-unknown-unknown`, and a crash reporter that panics while
/// reporting a crash is the one bug this module must not have.
fn now_secs() -> f64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|since| since.as_secs_f64())
        .unwrap_or_default()
}

/// One report per process, for the whole life of the process.
///
/// On the web that is not a policy, it is the arithmetic: the target aborts, so there is no second
/// panic to describe. On desktop it is a policy, and the reason is that a second panic arriving
/// after a fatal one is most often this hook's own wreckage rather than news.
static REPORTED: AtomicBool = AtomicBool::new(false);

/// The `localStorage` key the answer is kept under, on the same channel as every other
/// preference about the reader rather than the build ([`crate::ui_scale`], [`crate::theme`]).
pub const KEY_CRASH_REPORTING: &str = "sk-crash-reporting";

/// Whether a crash report may leave this machine.
///
/// **Starts `false`, and that is the whole design.** A panic hook cannot await, so this has to
/// be readable without blocking, which means the stored answer arrives after boot — and until
/// it does, the honest state is "nobody has said yes yet". Defaulting to `true` would report
/// every crash in the window before the preference loads, which is exactly the window a
/// crash-at-startup lands in, and reporting against a stored opt-out is the failure this row is
/// about. The cost is that a panic in the first moments of a launch goes unreported; the
/// alternative is a consent switch with a hole in it at the one moment it is most likely to
/// matter.
///
/// [`load_consent`] flips it once the stored value is read. Absent means yes — see
/// [`REPORTING_DEFAULT`].
static CONSENTED: AtomicBool = AtomicBool::new(CONSENT_BEFORE_LOAD);

/// What [`CONSENTED`] holds before [`load_consent`] has answered. Named rather than written
/// into the `AtomicBool` directly because a static's initial value cannot be asserted on once
/// any test has stored to it — a mutation that opened this window passed the whole suite until
/// it had a name of its own.
const CONSENT_BEFORE_LOAD: bool = false;

/// What an unanswered preference means. Sent unless the user has said otherwise: a crash
/// reporter nobody opts into reports nothing, and the crashes worth fixing are the ones on
/// machines whose owners never open a settings menu. The opt-out is one click, it takes effect
/// on the next crash rather than the next launch, and it is stated in plain words beside the
/// switch. Flipping this is the whole of the change, if the trade is judged differently later.
pub const REPORTING_DEFAULT: bool = true;

/// What the notice beside the switch says, kept here so the words and the rule cannot drift.
///
/// A notice that cannot say what is sent is not a notice. What [`envelope`] builds is the
/// exception type and message, the source location, the release and whether this is a debug or
/// release build — no account, no session, no build data, nothing typed. Sentry's own server
/// sees the request's IP, as any server does.
pub const REPORTING_NOTICE: &str =
    "Sends the error message, where in the code it happened, and the app version when Sidekick \
     crashes. Never your builds, your account, or anything you typed.";

/// Whether a report may be sent right now.
pub fn reporting_enabled() -> bool {
    CONSENTED.load(Ordering::SeqCst)
}

/// Record the answer and remember it.
///
/// Both values are written rather than only the opt-out, so "never asked" and "asked for the
/// default" stay distinguishable — which matters the day [`REPORTING_DEFAULT`] changes.
pub fn set_reporting(on: bool) {
    CONSENTED.store(on, Ordering::SeqCst);
    let json = if on { "\"true\"" } else { "\"false\"" };
    crate::storage::commit(format!(
        "try {{ localStorage.setItem({KEY_CRASH_REPORTING:?}, {json}); }} catch (_) {{}}"
    ));
}

/// Read the stored answer at boot and open the gate if it says so.
///
/// Anything unreadable is treated as unanswered, which is [`REPORTING_DEFAULT`]: a corrupt
/// preference should not silence a reporter, and it should not force one either.
pub async fn load_consent() {
    let js = format!(
        "try {{ return localStorage.getItem({KEY_CRASH_REPORTING:?}); }} catch (_) {{ return null; }}"
    );
    let stored = dioxus::document::eval(&js).await.ok();
    let answer = stored
        .as_ref()
        .and_then(|value| value.as_str())
        .map(|text| text.trim_matches('"') == "true")
        .unwrap_or(REPORTING_DEFAULT);
    CONSENTED.store(answer, Ordering::SeqCst);
}

/// Guards against a second [`install`] chaining a second copy of the hook onto the first.
static INSTALLED: AtomicBool = AtomicBool::new(false);

/// Install the panic hook, or say why crash reporting is off.
///
/// **Called from the first render rather than from `main`**, and that is load-bearing:
/// `dioxus-web` installs a panic hook of its own inside `run()` (`devtools.rs:35`, under
/// `cfg(all(feature = "devtools", debug_assertions))`), so a hook set before launch is the one
/// that gets replaced. Installing after the first render puts this hook on top of that one and
/// chains it, which is what keeps the dev build's console dump and toast working — and what makes
/// the reporting path testable in `dx serve` instead of only in a release build nobody can drive.
///
/// A release build chains the default hook instead, which is the whole reason this row exists: a
/// release web build installs no hook at all, so a panic there is a bare wasm trap with no
/// message, no location and nowhere for it to go.
pub fn install() {
    if INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }
    match Dsn::from_build_env() {
        Ok(dsn) => {
            let previous = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                report(&dsn, info);
                previous(info);
            }));
        }
        // Not silent, and not fatal. A build with no DSN is the normal local one; a build whose
        // DSN is unusable is a deployment bug whose only symptom would otherwise be crash reports
        // that never arrive — which looks exactly like an app that stopped crashing.
        Err(fault) => warn(&format!("crash reporting is off: {fault}")),
    }
}

/// Build the event and hand it to the transport. Never panics: every step of it runs inside a
/// panic that has already happened.
fn report(dsn: &Dsn, info: &PanicHookInfo) {
    if !should_report() {
        return;
    }
    let location = info.location();
    let crash = Crash {
        message: info.payload_as_str(),
        file: location.map(|at| at.file()),
        line: location.map_or(0, |at| at.line()),
        column: location.map_or(0, |at| at.column()),
        stack: js_stack(),
    };
    let at = now_secs();
    deliver(
        dsn.envelope_url(),
        envelope(&event_id(&crash, at), &crash, at),
    );
}

/// The JavaScript stack, on the target that has one.
///
/// Read the way `console_error_panic_hook` reads it — a fresh `Error` carries the stack of
/// wherever it was constructed, which inside the hook is the panic path. On the desktop build
/// there is no such thing, and `RUST_BACKTRACE` reaches the terminal through the chained default
/// hook rather than through this event.
///
/// Through `Reflect` rather than a binding, because `Error.prototype.stack` is not standard and
/// `js_sys::Error` accordingly exposes no getter for it. An absent or non-string `stack` is a
/// browser that does not offer one, not a failure worth reporting from inside a crash.
#[cfg(target_arch = "wasm32")]
fn js_stack() -> Option<String> {
    use js_sys::wasm_bindgen::JsValue;
    let error = js_sys::Error::new("");
    let stack = js_sys::Reflect::get(&error, &JsValue::from_str("stack"))
        .ok()?
        .as_string()?;
    (!stack.is_empty()).then_some(stack)
}

#[cfg(not(target_arch = "wasm32"))]
fn js_stack() -> Option<String> {
    None
}

/// Hand the envelope to the browser and let go of it.
///
/// `sendBeacon` rather than `fetch`: the module is about to trap, so the request must be complete
/// from this side the instant it is made. The browser owns it from there, including across the
/// page unload that a crashed planner usually ends in.
#[cfg(target_arch = "wasm32")]
fn deliver(url: &str, body: String) {
    let Some(navigator) = web_sys::window().map(|window| window.navigator()) else {
        return;
    };
    if navigator.send_beacon_with_opt_str(url, Some(&body)) != Ok(true) {
        warn("crash report was refused by the browser before it was sent");
    }
}

/// Send it and wait, on the target that can.
///
/// A thread of its own because `reqwest::blocking` refuses to run inside a runtime context, and
/// joined because the alternative is a detached thread racing a process that is already unwinding
/// — a crash report that arrives only when the crash was slow enough.
#[cfg(not(target_arch = "wasm32"))]
fn deliver(url: &str, body: String) {
    let url = url.to_string();
    let sent = std::thread::spawn(move || {
        reqwest::blocking::Client::new()
            .post(url)
            .header("content-type", "application/x-sentry-envelope")
            .timeout(DELIVERY_TIMEOUT)
            .body(body)
            .send()
    })
    .join();
    match sent {
        Ok(Ok(response)) if response.status().is_success() => {}
        Ok(Ok(response)) => warn(&format!("crash report refused: {}", response.status())),
        Ok(Err(error)) => warn(&format!("crash report never left: {error}")),
        Err(_) => warn("crash report thread panicked"),
    }
}

/// Where this module says a thing went wrong, on each target's own channel.
///
/// The twin of `granted_powers`'s, and separate from it on purpose: that one routes a gate the
/// evaluator could not read, and merging them would make one module's console channel the other's
/// dependency for no shared behaviour.
#[cfg(target_arch = "wasm32")]
fn warn(message: &str) {
    web_sys::console::warn_1(&message.into());
}

#[cfg(not(target_arch = "wasm32"))]
fn warn(message: &str) {
    eprintln!("{message}");
}
