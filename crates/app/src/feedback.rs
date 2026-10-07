//! The feedback form, and the worker it posts to.
//!
//! The worker is a Cloudflare one at [`WORKER_URL`] (`feedback-worker/` in the beta, Resend-backed)
//! and this module changes nothing about it: it is live, it serves the beta, and a rebuild that
//! needed its own would be a second address for the same inbox. What that costs is an interface
//! this side has to meet exactly — the worker reads `type`, `description`, `globalName`, `userId`,
//! `userName`, `buildContext`, `buildSnapshot`, `diagnostics`, `userAgent` and `timestamp`, in
//! camelCase, and renders the ones it recognises into an email. [`Report`] is that interface
//! written down, and `the_wire_shape_is_the_one_the_worker_reads` is what keeps it written down
//! correctly.
//!
//! **Its origin allow-list is part of the interface too.** `index.ts:51` admits
//! `coh-sidekick.com`, `wednesdaywoe.github.io` and `http://localhost:3000`, by prefix, and
//! answers everything else `403 Forbidden` before it reads a byte of the body — so this form works
//! from the deployed origins and from a dev server on port 3000, and reports "server refused" from
//! any other.
//!
//! **The desktop has no origin to be on that list, which is F35.** The browser sets `Origin`;
//! native `reqwest` does not, so every desktop submission met that 403 from the day the app
//! shipped, and the RC's whole purpose is a return channel. The worker grew a second arm rather
//! than this side growing a forged `Origin`: it now also admits a request carrying
//! [`DESKTOP_CLIENT_HEADER`] whose value matches its `DESKTOP_CLIENT_TOKEN` secret. The browser
//! arm is untouched — a page still passes on its origin alone, and a page cannot send this header
//! across origins anyway, because the worker's `Access-Control-Allow-Headers` does not name it.
//!
//! # What this sends that the beta's does not, and what it leaves out
//!
//! The beta attaches a `diagnostics` blob of about twenty-five UI flags, because its planner keeps
//! them in a UI store that the build export knows nothing about — Level Up Mode, Combat Mode,
//! exemplar level, the proc settings, the per-AT mechanic toggles. Without them its reports could
//! not be reproduced.
//!
//! Nearly all of them are on [`CharacterState`] in this tree — `combat`, `incarnates`,
//! `disabled_proc_categories`, `proc_overrides`, `accolades` — so the build snapshot already
//! carries them, and a second copy in a diagnostics blob would be the one that goes stale. What is
//! genuinely outside the build is what [`Diagnostics`] carries and no more: the app version, which
//! target is running, the dataset, the page address, the user agent, the viewport and the UI
//! scale.

use crate::build_session::BuildSession;
use crate::modal::{Modal, ModalSize};
use crate::shell::{Db, UiScale};
use coh_data::CharacterState;
use dioxus::prelude::*;
use serde::Serialize;

/// The modal's open flag, held at the shell root. A bare flag like [`crate::help::HelpOpen`]: the
/// quickbar's Feedback action and the main menu's row both want the same one form.
#[derive(Clone, Copy)]
pub struct FeedbackOpen(pub Signal<bool>);

/// The worker. One address, and the beta's — see the module doc.
pub const WORKER_URL: &str = "https://coh-planner-feedback.wedswoe.workers.dev";

/// The header the desktop sends in place of the `Origin` a browser would have set, matched by the
/// worker against its `DESKTOP_CLIENT_TOKEN` secret (`feedback-worker/src/index.ts`).
pub const DESKTOP_CLIENT_HEADER: &str = "X-Sidekick-Desktop";

/// Where its value comes from: baked at build time, the way the Supabase project is
/// ([`crate::cloud::Config::from_build_env`] has the argument for `option_env!` over `env!`).
/// Named here so a build without it can say which variable was missing instead of reporting a
/// bare 403 that tells the user nothing and the maintainer less.
pub const DESKTOP_TOKEN_VAR: &str = "SIDEKICK_FEEDBACK_TOKEN";

/// How long a submit may take before the form gives up on it. Set per request because that is the
/// method both targets carry ([`crate::cloud`] has the note).
const SUBMIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// The worker truncates the description at 5,000 characters (`index.ts:288`) and refuses an empty
/// one; the beta's form asks for ten before it enables the button. Mirrored here so the counter
/// beside the field is the honest number, with the server still holding the real rule.
pub const DESCRIPTION_MIN: usize = 10;
pub const DESCRIPTION_MAX: usize = 5000;

/// What kind of report this is. The worker validates the wire spelling against exactly these three
/// (`index.ts:281`) and refuses anything else, so the enum is the allow-list rather than a
/// suggestion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Topic {
    Bug,
    Suggestion,
    Other,
}

impl Topic {
    /// Every topic, in the beta's order. Exhaustive `match`es below hang off this, so a fourth
    /// kind fails to compile here before it can be a button with no wire spelling.
    pub const ALL: [Topic; 3] = [Topic::Bug, Topic::Suggestion, Topic::Other];

    /// The spelling the worker checks. Not the label: the label is ours to reword, and this is
    /// the other end of a validation.
    pub fn wire(self) -> &'static str {
        match self {
            Topic::Bug => "bug",
            Topic::Suggestion => "suggestion",
            Topic::Other => "other",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Topic::Bug => "Bug report",
            Topic::Suggestion => "Suggestion",
            Topic::Other => "Other",
        }
    }

    /// What a useful report of this kind contains. In the placeholder rather than in a paragraph
    /// above the box, so it is where the user is already looking when they start typing.
    pub fn placeholder(self) -> &'static str {
        match self {
            Topic::Bug => {
                "What you were doing, what happened, and what you expected \
                           instead.\n\nThe build snapshot below includes the build itself. Any \
                           other information can go here."
            }
            Topic::Suggestion => "What problem would it solve, and how would you use it?",
            Topic::Other => "Anything you like.",
        }
    }
}

/// The build, in the terms the report's email header renders: names, counts, and nothing that
/// needs a dataset lookup to say.
///
/// Every field here is already on [`CharacterState`], which is what makes [`build_context`] a pure
/// function over the build rather than another caller of the definitions.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BuildContext {
    pub archetype: String,
    pub level: u8,
    pub primary: String,
    pub secondary: String,
    pub pools: Vec<String>,
    /// `null` rather than absent when there is no epic pool — the worker's own field is
    /// `string | null` and it renders the key either way.
    pub epic_pool: Option<String>,
    pub power_count: usize,
    pub slot_count: usize,
}

/// What an empty selection reads as in the email. The beta's word, kept: a blank cell in a report
/// reads as the reporting being broken rather than as the field being empty.
const UNSET: &str = "None";

/// The build as the report describes it.
///
/// **Inherents are counted**, unlike the beta's `getBuildContext`, which walks primary, secondary,
/// pools and the epic pool only. In this tree the fitness powers and the AT inherent are
/// [`CharacterState::inherents`] and they hold real slots, so a count that skipped them would
/// report fewer slots than the build has — and the number exists to be compared against what the
/// reporter says they slotted.
///
/// Which is why the walk is [`CharacterState::all_selected`] rather than one written here: that
/// method's own doc names the defect a hand-rolled four-of-five-bucket walk leaves behind, and a
/// second copy of it is how a bucket the model grows later reaches one of them and not the other.
pub fn build_context(build: &CharacterState) -> BuildContext {
    let mut power_count = 0;
    let mut slot_count = 0;
    for power in build.all_selected() {
        power_count += 1;
        slot_count += power.slots.iter().filter(|slot| slot.is_some()).count();
    }

    let named = |name: &str| match name.trim().is_empty() {
        true => UNSET.to_string(),
        false => name.to_string(),
    };

    BuildContext {
        archetype: named(&build.archetype.name),
        level: build.level,
        primary: named(&build.primary.name),
        secondary: named(&build.secondary.name),
        pools: build
            .pools
            .iter()
            .map(|pool| pool.name.clone())
            .filter(|name| !name.trim().is_empty())
            .collect(),
        epic_pool: build.epic_pool.as_ref().map(|pool| pool.name.clone()),
        power_count,
        slot_count,
    }
}

/// The runtime facts the build cannot carry (see the module doc for why this is short).
#[derive(Serialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct Diagnostics {
    pub app: DiagnosticsApp,
    pub env: DiagnosticsEnv,
    /// The worker renders every key it finds here, and only the keys it already knows about under
    /// `env` (`index.ts:100`) — so anything this tree has and the beta did not goes in `ui` or it
    /// goes nowhere.
    pub ui: DiagnosticsUi,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct DiagnosticsApp {
    pub version: String,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsEnv {
    pub user_agent: String,
    pub viewport: Viewport,
    pub dataset_id: String,
    pub url: String,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsUi {
    /// `web` or `desktop`. The one fact that decides which half of every renderer difference the
    /// reporter was looking at, and the email has nowhere else to learn it — both targets are a
    /// webview, so the user agent does not say.
    pub target: String,
    pub ui_scale_pct: u32,
}

/// The page's own view of itself, read through the document because both targets have one.
///
/// `document::eval` rather than `web-sys`, which is the rule [`crate::clipboard`] states and the
/// slot drag paid for: the desktop build is a webview, so `navigator` and `window` answer there
/// too, and a `cfg(target_arch = "wasm32")` reader would be this block of the report deleted on
/// desktop rather than filled.
const READ_ENV: &str = "\
try {\
  return JSON.stringify({\
    userAgent: navigator.userAgent,\
    url: window.location.href,\
    width: window.innerWidth,\
    height: window.innerHeight,\
  });\
} catch (_) { return null; }";

async fn read_env() -> Option<(String, String, Viewport)> {
    let value = document::eval(READ_ENV).await.ok()?;
    let value: serde_json::Value = serde_json::from_str(value.as_str()?).ok()?;
    Some((
        value["userAgent"].as_str().unwrap_or_default().to_string(),
        value["url"].as_str().unwrap_or_default().to_string(),
        Viewport {
            width: value["width"].as_u64().unwrap_or_default() as u32,
            height: value["height"].as_u64().unwrap_or_default() as u32,
        },
    ))
}

/// One submission, as the worker reads it.
///
/// Absent optional fields are omitted rather than sent as `null`, which is what the beta's
/// `undefined` does to them in `JSON.stringify` — the worker's `payload.globalName` check is a
/// truthiness test either way, and matching the bytes keeps the two clients comparable in a
/// network log.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub r#type: &'static str,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub global_name: Option<String>,
    /// The signed-in account, attached automatically. Absent when signed out, which is most of
    /// the reports the beta receives.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_name: Option<String>,
    pub build_context: BuildContext,
    /// The whole build, as `.skif` text. The worker parses it, splices the diagnostics into it and
    /// attaches it as a `.json` file, so this is a string containing JSON rather than JSON.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_snapshot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostics: Option<Diagnostics>,
    pub user_agent: String,
    pub timestamp: String,
}

/// What a report carries, in the words the form shows, keyed to the wire fields each line covers.
///
/// **The form renders this table and the guard below grades it against [`Report`]'s own
/// serialization**, so a field added to the payload with no line here reds rather than quietly
/// joining the set the user was never told about. That set is what F37 found: the form disclosed
/// the one field with a control beside it, and the account, the user agent and every diagnostic
/// travelled unmentioned next to it.
///
/// A hand-written list would have drifted back the first time [`Diagnostics`] grew a field, and
/// nothing would have said so — a disclosure has no user who notices it going stale, which is
/// why this is a table the render reads rather than prose beside one.
///
/// Keys are dotted paths into the serialized report, matched by whole components, and each names
/// either a leaf or the object directly above the leaves one sentence covers.
///
/// **What the guard cannot check is the depth.** `diagnostics` alone would cover every leaf
/// beneath it and pass forever while telling the user nothing, which is the understatement the
/// row is about. Where a path sits is a reading; that it still covers something, and that
/// nothing travels uncovered, is the check.
const DISCLOSED: &[(&[&str], &str)] = &[
    (
        &["type", "description", "globalName"],
        "What you typed: the kind of report, the description, and the global name if you gave one.",
    ),
    (
        &["userId", "userName"],
        "Your username (when you are signed in) and its id and your display name.",
    ),
    (&["buildContext"], "The build summary above."),
    (
        &["buildSnapshot"],
        "The whole build; powers, slots, enhancements — while \"Attach this build\" is ticked.",
    ),
    (
        &[
            "diagnostics.app",
            "diagnostics.ui.target",
            "diagnostics.env.datasetId",
        ],
        "Which Sidekick this is: the version, desktop or web, and the dataset it loaded.",
    ),
    (
        &[
            "userAgent",
            "diagnostics.env.userAgent",
            "diagnostics.env.url",
            "diagnostics.env.viewport",
            "diagnostics.ui.uiScalePct",
        ],
        "Your user-agent string, the page address, the window size and the UI scale.",
    ),
    (&["timestamp"], "When you sent it."),
];

/// Who is reporting, when the report carries an account.
///
/// A named pair rather than a tuple, which is the rule for two same-typed values travelling
/// together: `(id, name)` transposed is a report filed under a display name, and nothing in the
/// types or the worker would notice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reporter {
    pub id: String,
    pub display_name: Option<String>,
}

/// Assemble the submission, given everything already resolved.
///
/// Split from the click handler for the reason [`crate::cloud::save_build::share_input`] is: the
/// shape of a report is gradeable without a runtime, and which fields carry through from the form
/// is the part that can be got wrong silently.
#[allow(clippy::too_many_arguments)]
pub fn report(
    topic: Topic,
    description: &str,
    global_name: &str,
    user: Option<Reporter>,
    build: &CharacterState,
    snapshot: Option<String>,
    diagnostics: Option<Diagnostics>,
    at_ms: i64,
) -> Report {
    let (user_id, user_name) = match user {
        Some(who) => (Some(who.id), who.display_name),
        None => (None, None),
    };
    let user_agent = diagnostics
        .as_ref()
        .map(|d| d.env.user_agent.clone())
        .unwrap_or_default();
    Report {
        r#type: topic.wire(),
        description: description.trim().chars().take(DESCRIPTION_MAX).collect(),
        global_name: Some(global_name.trim().to_string()).filter(|name| !name.is_empty()),
        user_id,
        user_name,
        build_context: build_context(build),
        build_snapshot: snapshot,
        diagnostics,
        user_agent,
        timestamp: iso_utc(at_ms),
    }
}

/// `2026-09-17T11:42:31.000+00:00` — the beta sends `new Date().toISOString()`, and the worker
/// prints it rather than parsing it.
///
/// Formatted from a millisecond count rather than read from the clock here, because a function
/// that reads the clock is one no assertion can name — and because `chrono`'s own `now` panics on
/// `wasm32-unknown-unknown` (`crates/app/Cargo.toml` states it beside `web-time`).
fn iso_utc(at_ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(at_ms)
        .map(|at| at.to_rfc3339())
        .unwrap_or_default()
}

/// Say which client this is, in the terms the worker's gate reads.
///
/// Two targets, two answers. **In the browser** there is nothing to do: the page's `Origin` is set
/// by the browser, it cannot be set from here, and the worker's allow-list is already the gate
/// this arm passes. **On the desktop** there is no origin at all, so the request carries
/// [`DESKTOP_CLIENT_HEADER`] instead.
///
/// A build without the token refuses here rather than posting. The alternative is a 403 from the
/// worker that reads as "the server is down" — which is what F35 looked like for as long as it
/// went unnoticed. The message names [`DESKTOP_TOKEN_VAR`], so the answer to a tester reporting it
/// is a build flag rather than an investigation.
#[cfg(target_arch = "wasm32")]
fn identify(request: reqwest::RequestBuilder) -> Result<reqwest::RequestBuilder, String> {
    Ok(request)
}

#[cfg(not(target_arch = "wasm32"))]
fn identify(request: reqwest::RequestBuilder) -> Result<reqwest::RequestBuilder, String> {
    let token = option_env!("SIDEKICK_FEEDBACK_TOKEN")
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .ok_or_else(|| {
            format!(
                "This build cannot send feedback: it was built without {DESKTOP_TOKEN_VAR}, and                  the worker refuses a desktop report that does not carry it. Please send it in                  Discord instead."
            )
        })?;
    Ok(request.header(DESKTOP_CLIENT_HEADER, token))
}

/// POST it, and keep the worker's own words on a refusal.
///
/// The worker answers `{"error": "…"}` on every path it refuses, and reporting the status instead
/// would tell the user something broke and the maintainer nothing — the argument
/// [`crate::cloud::server_message`] carries for the Supabase client, applied to the one call that
/// does not go through it.
pub async fn send(report: &Report) -> Result<(), String> {
    let request = reqwest::Client::new()
        .post(WORKER_URL)
        .json(report)
        .timeout(SUBMIT_TIMEOUT);
    let response = identify(request)?
        .send()
        .await
        .map_err(|error| format!("The report never left: {error}"))?;

    if response.status().is_success() {
        return Ok(());
    }
    let status = response.status().as_u16();
    let body = response.text().await.unwrap_or_default();
    let said = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|body| body["error"].as_str().map(str::to_string))
        .unwrap_or(body);
    match said.trim().is_empty() {
        true => Err(format!("The server refused the report ({status}).")),
        false => Err(format!("The server refused the report ({status}): {said}")),
    }
}

/// The modal's door. Mounted below both layout roots like every other modal host, so its fixed
/// backdrop is not contained by a `transform`ed grid surface ([`crate::modal`] has the note).
#[component]
pub fn FeedbackHost(database: Option<Db>) -> Element {
    let mut open = use_context::<FeedbackOpen>().0;
    if !open() {
        return rsx! {};
    }
    rsx! {
        Modal {
            title: "Send feedback".to_string(),
            size: ModalSize::Lg,
            on_close: move |_| open.set(false),
            FeedbackBody { database }
        }
    }
}

#[component]
fn FeedbackBody(database: Option<Db>) -> Element {
    let account = use_context::<crate::cloud::account::Account>();
    let session = use_context::<BuildSession>();
    let scale = use_context::<UiScale>().pct;
    let mut open = use_context::<FeedbackOpen>().0;

    let user = account.user();

    let mut topic = use_signal(|| Topic::Bug);
    let mut description = use_signal(String::new);
    let mut global_name = use_signal(String::new);
    let mut include_snapshot = use_signal(|| true);
    let mut sending = use_signal(|| false);
    let mut sent = use_signal(|| false);
    let mut refusal = use_signal(|| None::<String>);
    let mut copied = use_signal(|| false);

    // A memo rather than a line in the body: the body re-runs on every keystroke in the
    // description, and the summary is derived from the build, which no keystroke touches.
    let context = use_memo(move || build_context(&session.build.read()));
    let typed = description.read().chars().count();
    let can_send = (DESCRIPTION_MIN..=DESCRIPTION_MAX).contains(&typed) && !sending();

    // The snapshot is encoded where it is needed rather than kept in a signal: it is the build,
    // and the build is already a signal. Encoding it twice — once to attach, once to copy — costs
    // nothing next to a round trip and removes the state that could disagree with the build.
    let encode = {
        let database = database.clone();
        move || -> Result<String, String> {
            let database = database.clone().ok_or("The dataset is still loading.")?;
            coh_data::skif::encode(&session.build.peek(), &database.0).map_err(|e| e.to_string())
        }
    };

    let submit = {
        let encode = encode.clone();
        move |_| {
            let encode = encode.clone();
            async move {
                if sending() {
                    return;
                }
                sending.set(true);
                refusal.set(None);

                let snapshot = match include_snapshot() {
                    // A snapshot that would not encode is reported without one rather than not at
                    // all: the description is the part only the user has.
                    true => encode().ok(),
                    false => None,
                };
                let diagnostics = read_env()
                    .await
                    .map(|(user_agent, url, viewport)| Diagnostics {
                        app: DiagnosticsApp {
                            version: crate::app_info::APP_VERSION.to_string(),
                        },
                        env: DiagnosticsEnv {
                            user_agent,
                            viewport,
                            dataset_id: session.build.peek().dataset.as_str().to_string(),
                            url,
                        },
                        ui: DiagnosticsUi {
                            target: match cfg!(target_arch = "wasm32") {
                                true => "web".to_string(),
                                false => "desktop".to_string(),
                            },
                            ui_scale_pct: scale.peek().to_owned(),
                        },
                    });
                let who = user.peek().as_ref().map(|signed_in| Reporter {
                    id: signed_in.id.clone(),
                    display_name: signed_in.display_name.clone(),
                });

                let report = report(
                    topic(),
                    &description.peek().clone(),
                    &global_name.peek().clone(),
                    who,
                    &session.build.peek().clone(),
                    snapshot,
                    diagnostics,
                    crate::cloud::profile::now_unix_ms(),
                );
                match send(&report).await {
                    Ok(()) => sent.set(true),
                    Err(reason) => refusal.set(Some(reason)),
                }
                sending.set(false);
            }
        }
    };

    // The sent state replaces the form rather than closing over it on a timer, which is what the
    // beta does (`FeedbackModal.tsx:130`). A dialog that shuts itself two seconds later takes the
    // confirmation with it, and the one thing a reporter wants to know is that it went.
    if sent() {
        return rsx! {
            div { class: "feedback",
                p { class: "feedback__done", "Thank you — your report has been sent." }
                p { class: "feedback__note",
                    "Reports cannot be answered here. For a reply, reach WW on Discord."
                }
                div { class: "sb-detail__actions",
                    button {
                        class: "seg is-primary",
                        r#type: "button",
                        onclick: move |_| open.set(false),
                        "Done"
                    }
                }
            }
        };
    }

    rsx! {
        div { class: "feedback",
            p { class: "feedback__notice",
                "This form does not include a reply address. If you want a reply, reach "
                a {
                    class: "feedback__link",
                    href: crate::app_actions::DISCORD_FEEDBACK_DM_URL,
                    target: "_blank",
                    rel: "noopener noreferrer",
                    "WW on Discord"
                }
                " or join the "
                a {
                    class: "feedback__link",
                    href: crate::app_actions::DISCORD_INVITE_URL,
                    target: "_blank",
                    rel: "noopener noreferrer",
                    "Sidekick Discord"
                }
                " instead."
            }

            label { class: "field-label", "What is this?" }
            div { class: "sb-save__choice",
                for kind in Topic::ALL {
                    button {
                        key: "{kind.wire()}",
                        class: if topic() == kind { "seg active" } else { "seg" },
                        r#type: "button",
                        onclick: move |_| topic.set(kind),
                        "{kind.label()}"
                    }
                }
            }

            label { class: "field-label", "Description" }
            textarea {
                class: "sb-save__input sb-save__input--area feedback__description",
                value: "{description}",
                maxlength: "{DESCRIPTION_MAX}",
                placeholder: "{topic().placeholder()}",
                oninput: move |e| description.set(e.value()),
            }
            p { class: "sb-save__hint",
                if typed < DESCRIPTION_MIN {
                    "{DESCRIPTION_MIN - typed} more characters needed."
                } else {
                    "{typed}/{DESCRIPTION_MAX}"
                }
            }

            label { class: "field-label", "Your global name (optional)" }
            input {
                class: "sb-save__input",
                value: "{global_name}",
                placeholder: "@YourName",
                oninput: move |e| global_name.set(e.value()),
            }

            div { class: "feedback__snapshot",
                label { class: "feedback__check",
                    input {
                        r#type: "checkbox",
                        checked: include_snapshot(),
                        onchange: move |e| include_snapshot.set(e.checked()),
                    }
                    span { "Attach this build" }
                }
                // "Only the summary below travels" was true of the BUILD and false of the
                // report, which is F37 one box up: the account and the browser details go either
                // way, and the fold below now says so. Scoped to the build, the sentence is true
                // again — and a claim that contradicts the disclosure under it is worse than the
                // silence the disclosure replaced.
                p { class: "sb-save__hint",
                    if include_snapshot() {
                        "The whole build is included report; powers, slots, enhancements."
                    } else {
                        "Of the build, only the summary below travels."
                    }
                }
                p { class: "feedback__summary",
                    "{context().archetype} · {context().primary} / {context().secondary} · "
                    "level {context().level} · {context().power_count} powers, {context().slot_count} slots"
                }
                button {
                    class: "seg",
                    r#type: "button",
                    onclick: {
                        let encode = encode.clone();
                        move |_| {
                            let encode = encode.clone();
                            async move {
                                match encode() {
                                    Ok(text) => match crate::clipboard::copy(&text).await {
                                        Ok(()) => copied.set(true),
                                        Err(reason) => refusal.set(Some(reason)),
                                    },
                                    Err(reason) => refusal.set(Some(reason)),
                                }
                            }
                        }
                    },
                    if copied() { "Copied!" } else { "Copy the build instead" }
                }
            }

            // F37. Below the snapshot box and above Send, which is the one moment the answer is
            // wanted: the box above is a control, and a reader who has just decided whether to
            // attach their build is the reader asking what else goes with it.
            //
            // Folded, with the categories in the summary line, so an unexpanded fold still names
            // the account and the browser rather than hiding them behind a neutral title. The
            // list is [`DISCLOSED`] rather than written here, because a second copy of it is the
            // one that would go stale — and the guard grades the table, so a copy would keep
            // passing while the form understated again.
            details { class: "feedback__carries",
                summary { class: "feedback__carries-title",
                    "What this report includes — your username, your build, and app and browser details"
                }
                ul { class: "feedback__carries-list",
                    for (_, line) in DISCLOSED {
                        li { key: "{line}", "{line}" }
                    }
                }
            }

            if let Some(reason) = refusal() {
                p { class: "sb-save__error", "{reason}" }
            }

            div { class: "sb-detail__actions",
                button {
                    class: "seg",
                    r#type: "button",
                    onclick: move |_| open.set(false),
                    "Cancel"
                }
                button {
                    class: "seg is-primary",
                    r#type: "button",
                    disabled: !can_send,
                    onclick: submit,
                    if sending() { "Sending…" } else { "Send" }
                }
            }
        }
    }
}
