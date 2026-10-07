//! The main menu — the ☰ at the head of the header, and the app's answer to "what does this
//! thing do, and how is it set up".
//!
//! It absorbs two surfaces that were the same kind of thing in two places: the Build menu, which
//! led the quickbar, and the Options popover, which sat mid-header. Neither is about the build on
//! screen. Everything here either acts on the build as a FILE — a thing you keep, hand over, or
//! replace — or is a preference set once and then forgotten, and both of those are the opposite
//! of the quickbar's contents, which are things you *do* to the build you are reading.
//!
//! **The order is by consequence, not by category**, and that is the whole design of this file.
//! A reader scanning down passes from "destroys work" to "harmless" exactly once: account, then
//! the acts that change which build you are looking at, then the acts that hand a copy to
//! someone else, then the preferences that change no build at all, then help. Grouping by
//! category instead — "files", "sharing", "settings" — puts New build (discards everything) two
//! rows from Open file (discards everything) and eleven rows from Theme, which is right, and
//! then puts Save (harmless) between them, which is not.
//!
//! **What is not built is drawn disabled with its reason**, the convention
//! [`crate::build_io`]'s own unbuilt entries already follow. A menu is read as "what can this
//! app do"; a silently missing row answers that question wrongly, and the user's next move is to
//! go looking for it somewhere else.
//!
//! The entries themselves live where their flows do — [`crate::build_io`] owns the file acts,
//! [`crate::export_image`] the image — and this module owns only the order they are in and the
//! preferences at the bottom. Composition, not implementation: a menu that reimplemented its own
//! entries would be a second place for each act to be subtly different.

use crate::shell::RuleOfFiveAlert;
use crate::theme;
use coh_data::DatasetId;
use dioxus::prelude::*;
// Not in `dioxus::prelude`, which re-exports `spawn` and stops there — so a reader who finds
// `spawn` here and `spawn_forever` in the sign-in row would otherwise have no way to tell they
// are siblings. `spawn_forever` runs its future on `ScopeId::ROOT`, which is the whole of F06's
// fix, and the reason the two account rows are the places in this file that reach for it.
use dioxus::core::spawn_forever;

/// One entry with no flow behind it yet.
///
/// A struct rather than a tuple for the reason [`crate::quickbar`]'s twin is one: both fields
/// are `&'static str` and sit next to each other, so naming them at every site is what makes
/// transposing them something you have to do on purpose.
///
/// `PartialEq` because [`UnbuiltGroup`] takes a slice of these as a prop, and Dioxus decides
/// whether to re-render a child by comparing its props.
#[derive(PartialEq)]
struct Unbuilt {
    label: &'static str,
    /// Why it is inert, as a whole sentence — it goes to `title` verbatim, because a disabled
    /// row says unavailable and does not say why.
    why: &'static str,
}

/// The account tier's other half. Signing in is built ([`AccountEntries`]); browsing what other
/// people have shared is RB4d and now lands through [`crate::cloud::browser`]'s host, so the
/// row below is real rather than the stub it replaces.
///
/// Handing the build over by reference rather than by copy.
///
/// **"Copy short link" left this table in RB4e**, which built exactly the thing its reason said
/// was missing — and left a second, disabled row of the same name beside the working one until
/// it was removed. Worth the note: an unbuilt row states a reason, the reason stops being true
/// the moment the row is built, and nothing fails when it rots. The menu is the only place that
/// shows it.
///
/// What remains needs something genuinely different. A LIVE link carries the build in the URL
/// fragment rather than in a row, so it resolves with no server at all — and nothing in this
/// repo writes a fragment yet (`coh_data::import_link` decodes only, and says so).
const LINKS: [Unbuilt; 1] = [Unbuilt {
    label: "Copy live link",
    why: "Not in the rebuild yet — a live link carries the whole build in the URL, and this \
          planner reads that format without yet writing it",
}];

/// The three groups the header's menus are built from, and the one menu the mobile nav still
/// carries — F36 found the switch, HM8 is why the menu was split.
///
/// **HM1 put all of this in one ☰ and ordered it by consequence rather than by category**:
/// account, then the acts that replace the build, then the acts that hand it to someone, then
/// preferences, then help — "a reader scanning down passes from 'destroys work' to 'harmless'
/// exactly once". That ordering was right and the menu still outgrew its panel:
/// `.popover-panel` is `max-height: 70vh`, HM1 moved the Build menu in from row 2 and the whole
/// Options popover in from row 1, and WebKitGTK draws overlay scrollbars that stay hidden until
/// a scroll begins. Nineteen rows in a 672px panel look exactly like a menu that ends, which is
/// how F36's crash-reporting switch came to be reported as missing when it was simply below the
/// fold.
///
/// **So the split is by category, deliberately, and HM1's principle survives inside each menu
/// rather than across them.** File still runs replace-the-build before hand-it-over; Options
/// still runs the preferences that change nothing; About is harmless throughout. What changed is
/// that the walk from "destroys work" to "harmless" now happens three times in three short
/// menus instead of once down a list nobody can see the end of.
///
/// The groups are components rather than inline markup because they are composed twice: three
/// popovers on the desktop header, and one ☰ in the mobile nav, where a row of three menus would
/// cost more than a scroll does.
#[allow(non_snake_case)]
#[component]
fn FileEntries(
    /// `None` while the bundle loads. The acts that need definitions — encoding a build,
    /// resolving an opened one, drawing an image of it — say so rather than pretending.
    database: Option<crate::shell::Db>,
    /// The fork on screen. Opening a file authored on another fork is a dataset switch before
    /// it is an import, so the file entries write this too.
    dataset: Signal<DatasetId>,
) -> Element {
    rsx! {
        AccountEntries {}
        BrowseBuildsEntry {}
        div { class: "main-menu__hair" }

        // The steepest step in the menu: both of the acts above this hairline replace
        // the build wholesale, and everything below it leaves the build alone.
        crate::build_io::BuildFileEntries { database: database.clone(), dataset }
        div { class: "main-menu__hair" }

        crate::build_io::BuildHandoffEntries { database }
        UnbuiltGroup { entries: LINKS.as_slice() }
    }
}

/// Set-and-forget choices. None of them changes a build, which is why they are not in File.
///
/// (Dataset left this group in HM2 — it decides which game the build is a build of, so it sits
/// in the identity panel.)
#[allow(non_snake_case)]
#[component]
fn PreferenceEntries() -> Element {
    rsx! {
        div { class: "options-group",
            span { class: "field-label", "Theme" }
            ThemeSwitcher {}
        }
        div { class: "options-group",
            span { class: "field-label", "Alerts" }
            RuleOfFiveAlertToggle {}
        }
        div { class: "options-group",
            span { class: "field-label", "Powers" }
            ShowSlotLevelsToggle {}
        }
        div { class: "options-group",
            span { class: "field-label", "UI scale" }
            UiScaleControl {}
        }
        div { class: "options-group",
            span { class: "field-label", "Privacy" }
            CrashReportingToggle {}
        }
    }
}

/// The app talking about itself. Neither a file act nor a preference, which is the whole reason
/// this is a third menu rather than a tail on one of the other two.
#[allow(non_snake_case)]
#[component]
fn AboutEntries() -> Element {
    rsx! {
        HelpEntry {}
        WhatsNewEntry {}
        FeedbackEntry {}
        ChangelogEntry {}
        AboutEntry {}
        DiscordEntry {}
        DonateEntry {}
    }
}

/// What the app does to a build as a *file*, plus the account that can hold one.
#[allow(non_snake_case)]
#[component]
pub fn FileMenu(database: Option<crate::shell::Db>, dataset: Signal<DatasetId>) -> Element {
    rsx! {
        crate::popover::Popover {
            label: "File".to_string(),
            title: "File".to_string(),
            modifier: "popover--menu".to_string(),
            div { class: "main-menu", FileEntries { database, dataset } }
        }
    }
}

/// Every preference that changes nothing about the build.
#[allow(non_snake_case)]
#[component]
pub fn OptionsMenu() -> Element {
    rsx! {
        crate::popover::Popover {
            label: "Options".to_string(),
            title: "Options".to_string(),
            modifier: "popover--menu-end".to_string(),
            div { class: "main-menu", PreferenceEntries {} }
        }
    }
}

/// Help, feedback, what changed, what this is, and how to support it.
#[allow(non_snake_case)]
#[component]
pub fn HelpMenu() -> Element {
    rsx! {
        crate::popover::Popover {
            label: "Help".to_string(),
            title: "Help".to_string(),
            modifier: "popover--menu-end".to_string(),
            div { class: "main-menu", AboutEntries {} }
        }
    }
}

/// The shared-builds browser's door (RB4d). Anonymous-reachable: opening the list needs no
/// session, and the cloud layer answers with the anon key when there is none — the whole
/// point of `get-build` working logged out. Not target-gated: the desktop build reaches
/// Supabase too (decided 2026-09-16), and an unconfigured build fails visibly with the
/// variable's name rather than drawing a dead row.
#[component]
fn BrowseBuildsEntry() -> Element {
    let mut browser = use_context::<crate::cloud::browser::BrowserOpen>().0;
    let mut menu = use_context::<crate::popover::PopoverOpen>().0;

    rsx! {
        button {
            class: "main-menu__item",
            r#type: "button",
            onclick: move |_| {
                browser.set(Some(crate::cloud::browser::BrowserTarget::Browse));
                menu.set(false);
            },
            span { class: "main-menu__label", "Browse builds…" }
            span { class: "main-menu__hint", "Search builds the community has shared" }
        }
    }
}

/// Sign in, or the name of whoever already is.
///
/// One component for both states because they are one row's worth of the same question, and the
/// signed-out state is two rows rather than one for the reason the beta's menu is
/// (`Header.tsx:1061-1065`): the provider is the choice, and a "Sign in…" that then asks which
/// is a dialog standing in for two buttons.
///
/// **It reads [`crate::cloud::account::Account`]'s user signal**, which is the signal the
/// `TOKEN_REFRESHED` swallow exists to keep quiet — a menu that remounted on every token rotation
/// is the harmless end of the same defect that remounted the beta's build grid mid-write.
#[component]
fn AccountEntries() -> Element {
    let account = use_context::<crate::cloud::account::Account>();
    let mut menu = use_context::<crate::popover::PopoverOpen>().0;
    let user = account.user();

    if let Some(signed_in) = user.read().clone() {
        // The last fallback the beta's header applies (`Header.tsx:910`), applied here because
        // this is the drawing surface — `identity` leaves an absent name absent.
        let name = signed_in
            .display_name
            .unwrap_or_else(|| "Account".to_string());
        return rsx! {
            button {
                class: "main-menu__item",
                r#type: "button",
                onclick: {
                    let account = account.clone();
                    move |_| {
                        let account = account.clone();
                        // **F72, which is F06's defect in the row that stands in for it.**
                        // This handed its future back to the row's scope, and the popover unmounts
                        // that scope on any click outside it — so a dismissal during the GoTrue
                        // round trip dropped the sign-out where it stood. The window is one
                        // request rather than `DEADLINE`, which is the whole of why this was
                        // rated below F06 and not a reason to leave it: a logout against a
                        // hung or unreachable server widens it to the timeout.
                        //
                        // What a cancel costs is everything `sign_out` does AFTER the request,
                        // and that is all of it — `SignedOut` unpublished, the quick-share
                        // tokens and the favourites cache unemptied, and
                        // `SessionManager::forget` never reached, so the token stays on the
                        // client and the stored session stays in `localStorage` (session.rs:358).
                        // The last one outlives the process: the user is signed in again at the
                        // next launch, having signed out and watched the menu close on it.
                        spawn_forever(async move {
                            account.sign_out().await;
                        });
                        // FIRST, and for the sign-in row's reason: what made the order matter
                        // was ownership. Handed back to this scope, closing first
                        // cancelled the work — measured 2026-09-16, when closing first made
                        // every sign-in and sign-out a click that did nothing, silently, with
                        // the menu shutting as though it had worked. On the root scope the
                        // close is free, and a row the user has finished with should go.
                        menu.set(false);
                    }
                },
                span { class: "main-menu__label", "Sign out" }
                span { class: "main-menu__hint", "Signed in as {name}" }
            }
            ProfileEntry {}
            ClaimBuildsEntry {}
        };
    }

    rsx! {
        for provider in crate::cloud::oauth::PROVIDERS {
            button {
                key: "{provider.id()}",
                class: "main-menu__item",
                r#type: "button",
                onclick: {
                    let account = account.clone();
                    move |_| {
                        let account = account.clone();
                        // **The root scope owns this, not the row, and that is F06's fix.** A
                        // Dioxus scope cancels the futures its handlers return when it unmounts,
                        // and this row sits inside a popover the user can dismiss with a click
                        // anywhere outside it. The desktop flow waits on a loopback listener for
                        // up to `DEADLINE`, so the whole five minutes was cancellable by the one
                        // click a user is most likely to make in them: the click back onto the
                        // app after the browser has taken the foreground. It lands on
                        // `popover-backdrop`, and the listener went with the scope.
                        //
                        // Reproduced live on 2026-09-17 rather than read: the listener bound on
                        // 127.0.0.1:44709, the authorize URL in the opener's argv named 44709,
                        // and 20 seconds later the port refused connections — with Brave still
                        // pointed at it. The timing is the part a reading could not supply. It
                        // had already survived `open_external`, the browser taking the
                        // foreground and the window losing focus, so the cancel belongs to the
                        // dismissal and to nothing upstream of it.
                        //
                        // It failed silently, and that is structural rather than bad luck:
                        // `desktop_signin::sign_in` reports refusals through `Account::refuse`,
                        // and that call is inside the future being dropped. A cancelled sign-in
                        // has nothing left to report with.
                        spawn_forever(async move {
                            // The one target split this menu still makes, and RB5 is what turned
                            // it from a disabled row into a fork. The web build's sign-in IS a
                            // page load — it navigates away and the answer comes back through
                            // `Account::start`. The desktop build cannot do that: the window is a
                            // webview at `dioxus://index.html`, so navigating it would send the
                            // app itself to the provider and the answer would land wherever the
                            // OS browser is. `crate::desktop_signin` catches it on a loopback port
                            // instead. `cfg!` rather than `#[cfg]`, so both arms stay type-checked
                            // together and neither can rot behind the other.
                            if cfg!(target_arch = "wasm32") {
                                account.sign_in(provider).await;
                            } else {
                                crate::desktop_signin::sign_in(&account, provider).await;
                            }
                        });
                        // FIRST, and what changed is ownership rather than the order's reason.
                        // A row that hands its future back to this scope must close LAST or the
                        // close cancels the work — the 2026-09-16 measurement the sign-out row
                        // records, and still the rule for the one row here that awaits on its
                        // own scope (`ClaimBuildsEntry`, which closes not at all). This one is
                        // the root's and no longer cares, which frees the menu to do what a row
                        // that sends the user to a browser window should do: get out of the way
                        // at once, rather than stand over a five-minute wait.
                        menu.set(false);
                    }
                },
                span { class: "main-menu__label", "Sign in with {provider.label()}" }
                span { class: "main-menu__hint", "{provider.hint()}" }
            }
        }
    }
}

/// Edit the public profile behind the name every shared build is signed with (RB4f).
///
/// Signed-in only, and absent rather than disabled when signed out — for [`ClaimBuildsEntry`]'s
/// reason inverted: a signed-out user *can* act on this, by signing in, and the rows that offer
/// exactly that are the ones this replaces. Two doors to the same place, one of them inert,
/// would only say the menu had a profile the visitor could not have.
#[component]
fn ProfileEntry() -> Element {
    let mut open = use_context::<crate::cloud::profile::ProfileOpen>().0;
    let mut menu = use_context::<crate::popover::PopoverOpen>().0;
    rsx! {
        button {
            class: "main-menu__item",
            r#type: "button",
            onclick: move |_| {
                open.set(true);
                menu.set(false);
            },
            span { class: "main-menu__label", "Public profile…" }
            span { class: "main-menu__hint", "Your handle, display name and bio" }
        }
    }
}

/// Attach the builds this browser shared anonymously to the account now signed in (RB4e).
///
/// **A deliberate act, not something sign-in does on its own** — the beta puts it behind a button
/// in General Settings (`GeneralSettings.tsx:44`) and this keeps that shape. Claiming rewrites
/// who owns a row, and a sign-in that silently reassigned every anonymous build on a shared
/// browser would be a surprise with no undo.
///
/// The row is absent, not disabled, when this browser holds no owner tokens: a disabled row
/// states an option the user could take if something changed, and there is nothing they could do
/// to make this one apply. Every other unavailable row in this menu is disabled-with-a-reason
/// precisely because the reason is actionable.
#[component]
fn ClaimBuildsEntry() -> Element {
    let account = use_context::<crate::cloud::account::Account>();
    let mut working = use_signal(|| false);
    let mut claimed = use_signal(|| None::<usize>);

    // The token map is read once per menu open. A resource rather than a signal because the read
    // goes through the document, which is asynchronous.
    let tokens = use_resource(|| async { crate::cloud::owner_store::owner_tokens().await });

    let held = match &*tokens.read_unchecked() {
        Some(tokens) => tokens.clone(),
        None => return rsx! {},
    };
    if held.is_empty() {
        return rsx! {};
    }
    let count = held.len();

    rsx! {
        button {
            class: "main-menu__item",
            r#type: "button",
            disabled: working() || claimed().is_some(),
            onclick: {
                let account = account.clone();
                move |_| {
                    let account = account.clone();
                    let held = held.clone();
                    async move {
                        working.set(true);
                        let outcome = match account.prepared().await {
                            Err(e) => Err(crate::cloud::shared_builds::ShareError::Cloud(e)),
                            Ok(cloud) => {
                                crate::cloud::shared_builds::claim_builds(&cloud, &held, true).await
                            }
                        };
                        match outcome {
                            Ok(result) => claimed.set(Some(result.claimed.len())),
                            Err(reason) => {
                                account.refuse("Claim your anonymous builds", reason.to_string())
                            }
                        }
                        working.set(false);
                        // The menu is NOT closed here, and this is the one row still owed
                        // that care: closing unmounts this scope and cancels whatever it is
                        // still awaiting (the 2026-09-16 measurement the sign-out row records).
                        // The sign-in and sign-out rows escaped it by moving to the root scope;
                        // this one cannot follow them, because the future writes `working` and
                        // `claimed`, which are this scope's signals. Leaving it open also puts
                        // the result where the user is looking.
                    }
                }
            },
            span { class: "main-menu__label",
                if working() {
                    "Claiming…"
                } else if let Some(done) = claimed() {
                    "Claimed {done}"
                } else if count == 1 {
                    "Claim 1 anonymous build"
                } else {
                    "Claim {count} anonymous builds"
                }
            }
            span { class: "main-menu__hint", "Attach builds shared from this browser to your account" }
        }
    }
}

/// The guide. Raises the same flag the footer's Help button does
/// ([`crate::help::HelpOpen`]), because one guide reached two ways is the whole point of the
/// flag living at the shell root — the alternative, a signal owned by whichever surface was
/// built first, leaves the other one raising something nothing reads.
#[component]
fn HelpEntry() -> Element {
    let mut help = use_context::<crate::help::HelpOpen>().0;
    let mut menu = use_context::<crate::popover::PopoverOpen>().0;

    rsx! {
        button {
            class: "main-menu__item",
            r#type: "button",
            onclick: move |_| {
                help.set(true);
                menu.set(false);
            },
            span { class: "main-menu__label", "Help" }
            span { class: "main-menu__hint", "What each part of the planner is for" }
        }
    }
}

/// The release notes the first-visit welcome carries. The quickbar's ✦ raises the same flag;
/// this row is its door on a phone, where the quickbar is not drawn.
#[component]
fn WhatsNewEntry() -> Element {
    let mut whats_new = use_context::<crate::app_info::WelcomeOpen>().0;
    let mut menu = use_context::<crate::popover::PopoverOpen>().0;

    rsx! {
        button {
            class: "main-menu__item",
            r#type: "button",
            onclick: move |_| {
                whats_new.set(true);
                menu.set(false);
            },
            span { class: "main-menu__label", "What's new" }
            span { class: "main-menu__hint", "The latest changes, as the welcome shows them" }
        }
    }
}

/// The report form (RB4h). Same shape as [`HelpEntry`] above and for the same reason: the
/// quickbar's Feedback button raises this flag too, so neither door owns it.
///
/// It sits beside Help rather than under the file acts because that is what it is — a thing the
/// app does about itself, next to the other one. A reader who has just failed to find what they
/// wanted in the guide is one row away from saying so.
#[component]
fn FeedbackEntry() -> Element {
    let mut feedback = use_context::<crate::feedback::FeedbackOpen>().0;
    let mut menu = use_context::<crate::popover::PopoverOpen>().0;

    rsx! {
        button {
            class: "main-menu__item",
            r#type: "button",
            onclick: move |_| {
                feedback.set(true);
                menu.set(false);
            },
            span { class: "main-menu__label", "Send feedback…" }
            span { class: "main-menu__hint", "Report a bug, or ask for something" }
        }
    }
}

/// The rebuild's own history — what 1.0.0 shipped. Its content is curated in
/// [`crate::app_info`], written against this code, never the beta's git log.
#[component]
fn ChangelogEntry() -> Element {
    let mut changelog = use_context::<crate::app_info::ChangelogOpen>().0;
    let mut menu = use_context::<crate::popover::PopoverOpen>().0;

    rsx! {
        button {
            class: "main-menu__item",
            r#type: "button",
            onclick: move |_| {
                changelog.set(true);
                menu.set(false);
            },
            span { class: "main-menu__label", "Changelog" }
            span { class: "main-menu__hint", "What 1.0.0 shipped" }
        }
    }
}

/// What this rebuild is and where its data comes from — the beta's About was the
/// original author's personal note, so the row answers a different question here.
#[component]
fn AboutEntry() -> Element {
    let mut about = use_context::<crate::app_info::AboutOpen>().0;
    let mut menu = use_context::<crate::popover::PopoverOpen>().0;

    rsx! {
        button {
            class: "main-menu__item",
            r#type: "button",
            onclick: move |_| {
                about.set(true);
                menu.set(false);
            },
            span { class: "main-menu__label", "About" }
            span { class: "main-menu__hint", "What this rebuild is, and where its data comes from" }
        }
    }
}

/// The community invite, opened outside the app. The quickbar's Discord mark is the other door;
/// on a phone this is the only one.
#[component]
fn DiscordEntry() -> Element {
    let mut menu = use_context::<crate::popover::PopoverOpen>().0;

    rsx! {
        button {
            class: "main-menu__item",
            r#type: "button",
            onclick: move |_| {
                crate::app_actions::open_external(crate::app_actions::DISCORD_INVITE_URL);
                menu.set(false);
            },
            span { class: "main-menu__label", "Join the Discord" }
            span { class: "main-menu__hint", "Questions, builds, and what is coming next" }
        }
    }
}

/// The in-app donation flow. The beta's sat in the footer's floating actions; those are
/// their own unbuilt item, so the entry sits where the app's other app-level acts do.
#[component]
fn DonateEntry() -> Element {
    let mut donate = use_context::<crate::app_info::DonateOpen>().0;
    let mut menu = use_context::<crate::popover::PopoverOpen>().0;

    rsx! {
        button {
            class: "main-menu__item",
            r#type: "button",
            onclick: move |_| {
                donate.set(true);
                menu.set(false);
            },
            span { class: "main-menu__label", "Support Sidekick" }
            span { class: "main-menu__hint", "Buy the planner's author a coffee" }
        }
    }
}

/// A run of not-yet-built rows, drawn exactly like the built ones beside them but inert.
///
/// Same classes and same shape on purpose: a row that looked different as well as being disabled
/// would read as a different KIND of thing, when the only difference is that it does not work
/// yet. Disabled plus a hover reason is the whole of what needs saying.
#[component]
fn UnbuiltGroup(entries: &'static [Unbuilt]) -> Element {
    rsx! {
        for entry in entries {
            button {
                key: "{entry.label}",
                class: "main-menu__item",
                r#type: "button",
                disabled: true,
                title: "{entry.why}",
                span { class: "main-menu__label", "{entry.label}" }
            }
        }
    }
}

#[component]
fn ThemeSwitcher() -> Element {
    let manifest = match theme::manifest() {
        Ok(m) => m,
        Err(e) => {
            return rsx! {
                nav { class: "theme-switcher",
                    span { class: "load-state error", "{e}" }
                }
            }
        }
    };
    // Which theme is on. The theme lives on `<html data-theme>` and nowhere in Rust state, so the
    // switcher reads it back from the document when it mounts and tracks it locally from then
    // on. With ten themes a row of buttons that does not say which one is on is a guessing game.
    let mut current = use_signal(|| None::<String>);
    use_future(move || async move {
        if let Ok(value) =
            document::eval("return document.documentElement.dataset.theme || null;").await
        {
            if let Some(id) = value.as_str() {
                current.set(Some(id.to_string()));
            }
        }
    });
    rsx! {
        nav { class: "theme-switcher",
            for t in &manifest.themes {
                button {
                    class: if current.read().as_deref() == Some(t.id.as_str()) { "seg active" } else { "seg" },
                    "aria-pressed": current.read().as_deref() == Some(t.id.as_str()),
                    title: "{t.tagline}",
                    onclick: {
                        let id = t.id.clone();
                        move |_| {
                            theme::apply_theme(&id);
                            current.set(Some(id.clone()));
                        }
                    },
                    span {
                        class: "swatch",
                        style: "background: {t.swatch.accent};",
                    }
                    "{t.label}"
                }
            }
        }
    }
}

/// The permanent off switch for the shell's Rule-of-5 banner — the beta's "Bonus Cap Alert"
/// setting. Distinct from the banner's own ✕, which is a dismissal until reload; a preference
/// that could only be expressed by dismissing would have to be re-expressed every session.
#[component]
fn RuleOfFiveAlertToggle() -> Element {
    let alert = use_context::<RuleOfFiveAlert>();
    let mut enabled = alert.enabled;
    rsx! {
        label { class: "options-check",
            input {
                r#type: "checkbox",
                checked: enabled(),
                onchange: move |evt| enabled.set(evt.checked()),
            }
            "Warn when a set bonus is over the Rule of 5"
        }
    }
}

/// Whether each slot shows the level it was added at. The levels are kept either way; this
/// only decides whether the powers panel draws them.
#[component]
fn ShowSlotLevelsToggle() -> Element {
    let mut shown = use_context::<crate::panels::powers::ShowSlotLevels>().0;
    rsx! {
        label { class: "options-check",
            input {
                r#type: "checkbox",
                checked: shown(),
                onchange: move |evt| shown.set(evt.checked()),
            }
            "Show the level each slot is added at"
        }
    }
}

/// The crash reporter's off switch, and the sentence that says what it sends.
///
/// The row's three clauses are one feature: reporting ran whenever a DSN was compiled in, with
/// nothing telling anyone it was happening and no way to stop it. The notice matters as much as
/// the switch, which is why [`crate::crash::REPORTING_NOTICE`] lives beside the rule it
/// describes rather than being written out here — a sentence kept somewhere else is one that
/// goes on saying what used to be true.
///
/// Reads [`crate::crash::reporting_enabled`] once per render rather than holding a signal: the
/// authority is an `AtomicBool` the panic hook can read without awaiting, and a second copy of
/// the answer is a second thing to keep in step.
#[component]
fn CrashReportingToggle() -> Element {
    let mut on = use_signal(crate::crash::reporting_enabled);
    rsx! {
        label { class: "options-check",
            input {
                r#type: "checkbox",
                checked: on(),
                onchange: move |evt| {
                    crate::crash::set_reporting(evt.checked());
                    on.set(evt.checked());
                },
            }
            "Send crash reports"
        }
        p { class: "options-note", "{crate::crash::REPORTING_NOTICE}" }
    }
}

/// Pure app chrome — nothing in the calc reads this. Steps [`crate::ui_scale::MIN_PCT`]..=
/// [`crate::ui_scale::MAX_PCT`] by [`crate::ui_scale::STEP_PCT`]; the `.stepper`/`.step`
/// classes are the same shared chrome the header's build-level control and the Combat panel's
/// steppers already use.
#[component]
fn UiScaleControl() -> Element {
    let ui_scale = use_context::<crate::shell::UiScale>();
    let mut pct = ui_scale.pct;
    let current = pct();
    rsx! {
        div { class: "stepper",
            button {
                class: "step",
                r#type: "button",
                "aria-label": "Shrink the app",
                disabled: current <= crate::ui_scale::MIN_PCT,
                onclick: move |_| pct.set(current.saturating_sub(crate::ui_scale::STEP_PCT).max(crate::ui_scale::MIN_PCT)),
                "−"
            }
            span { class: "ui-scale__value mono", "{current}%" }
            button {
                class: "step",
                r#type: "button",
                "aria-label": "Grow the app",
                disabled: current >= crate::ui_scale::MAX_PCT,
                onclick: move |_| pct.set((current + crate::ui_scale::STEP_PCT).min(crate::ui_scale::MAX_PCT)),
                "+"
            }
        }
    }
}
