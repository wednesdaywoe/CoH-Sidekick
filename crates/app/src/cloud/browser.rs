//! The shared-builds browser — RB4d's UI half.
//!
//! Three modal surfaces and one boot-time door:
//!
//! * [`BrowserModal`] — search/browse over public builds (the beta's "All Builds" tab): text
//!   search, archetype / primary / secondary filters, newest-or-most-viewed, pagination.
//! * [`BuildDetailModal`] — one build by id (the beta's `BuildDetailPage`): the read-only
//!   view (name, sets, description, tags, powers), copy link, load-into-planner, and the view
//!   count. This is the surface `get-build` exists for — the *only* path a non-owner reads an
//!   unlisted build through — and the modal never asks it anything but one exact id.
//! * [`AuthorModal`] — one author's page (the beta's `AuthorPage`), reached from the author
//!   filter below or from `/author/<handle>` at boot. Two requests in order: `resolve_author`
//!   turns the handle into an account id, and only then can the build list be asked for.
//! * the `/builds/<id>` and `/author/<handle>` paths at boot — the deep links every share and
//!   every author link point at. The URL is the
//!   currency of the whole feature: a looser anon read policy on `shared_builds` would have
//!   made every build bulk-listable by anyone holding the public anon key
//!   (`get-build/index.ts:6-8`), which is why the read goes through the function, and it is
//!   also why the path reader here only ever opens exactly one id and never clears the URL.
//!
//! Both surfaces are anonymous-reachable: there is no gating on a session, and `prepared()`
//! passes through the same door as every other call — a signed-out visit finds the anon key
//! as the bearer (RB4a, the whole reason `get-build` works with no session at all).
//!
//! The favourite star landed in RB4g, on [`BuildCard`], so it is on both grids — the browse one
//! and the author page's — from one place. The "Favourites" view beside the search is one of the
//! beta's three tabs; the third, My Builds, landed in RB4k as [`VaultPanel`] and is the one list
//! here that can hold a private row. The author box landed in RB4f and commits on
//! Enter rather than as you type — [`AuthorFilter`] carries the reason, which is that a
//! debounce needs a timer neither target shares without a `cfg` this directory forbids. The
//! social-preview *capture* is here — the 1200×880 card is rendered in-process by
//! [`crate::export_image::render_preview_png`] and pushed to `backfill-preview` fire-and-forget
//! from the detail modal, once per open, only for builds whose own dataset is loaded (the beta
//! hid the same step in a second wasm boot behind `?previewCapture=`; this instance IS the
//! renderer, so there is no second boot and no empty-build race to hide from).

use crate::build_io::{BuildIoChoice, BuildIoPending, BuildIoReport};
use crate::build_session::BuildSession;
use crate::cloud::account::Account;
use crate::cloud::avatar::avatar_src;
use crate::cloud::favorites;
use crate::cloud::profile;
use crate::cloud::shared_builds::{
    self, BuildVisibility, SearchFilters, SharedBuild, SharedBuildSummary, SortBy,
};
use crate::cloud::tag_vocab;
use crate::modal::{Modal, ModalSize};
use crate::shell::Db;
use coh_data::{DatasetId, Powerset};
use dioxus::prelude::*;

/// What the browser is showing right now. `Browse` is the search/list surface; `Build(id)` the
/// single-build view. `None` is the browser being closed. Held in one signal so the two
/// surfaces can never both be open, and because a deep link arriving at boot has to be able to
/// open the detail without first owning the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BrowserTarget {
    Browse,
    Build(String),
    /// One author's page, by handle (RB4f). A third surface rather than a preset of `Browse`
    /// because it opens on a handle and `Browse` filters on a `user_id` — the resolve is what
    /// turns one into the other, and it can fail in a way a filter cannot: nobody holds it.
    Author(String),
}

/// Provided by the shell; raised by the main menu row and by the boot-time path reader.
#[derive(Clone, Copy)]
pub struct BrowserOpen(pub Signal<Option<BrowserTarget>>);

/// The host, mounted at the shell root like the other overlays. Owns the boot-time deep link
/// and renders whichever surface the signal names.
#[component]
pub fn BrowserHost(database: Option<Db>, dataset: Signal<DatasetId>) -> Element {
    let mut open = use_context::<BrowserOpen>().0;

    // The one deep link the app honors: `pathname == /builds/<id>`, exactly. Read once, at
    // boot, and never cleared — the path belongs to the URL and a reload must reopen the same
    // build (bookmarkability is the whole point of share links). The fragment reader
    // ([`crate::build_io`]) owns the `#…` axis and clears only what it reads; this reads a
    // different axis and clears nothing, which keeps the two from ever racing.
    use_future(move || async move {
        let Some(pathname) = read_pathname().await else {
            return;
        };
        let Some(target) = boot_target(&pathname) else {
            return;
        };
        if open.peek().is_none() {
            open.set(Some(target));
        }
    });

    let target = match open.read().clone() {
        None => return rsx! {},
        Some(target) => target,
    };

    rsx! {
        match target {
            BrowserTarget::Browse => rsx! {
                BrowserModal { database: database }
            },
            BrowserTarget::Build(id) => rsx! {
                // Keyed on the id so reopening a different build remounts the modal — the
                // fetch, the view count and the copy state are all per-build.
                BuildDetailModal { key: "{id}", id, database, dataset }
            },
            BrowserTarget::Author(handle) => rsx! {
                // Keyed for the same reason: the resolve and the build list are per-handle.
                AuthorModal { key: "{handle}", handle }
            },
        }
    }
}

/// The search surface. Everything anon can do with the public rows: filter, sort, page, and
/// open a build. The database feed the archetype and powerset dropdowns; the cloud feeds the
/// rows; the two never meet, which is the point of the service layer.
#[component]
fn BrowserModal(database: Option<Db>) -> Element {
    let account = use_context::<Account>();
    let mut open = use_context::<BrowserOpen>().0;
    let filters = use_signal(SearchFilters::default);
    // The search box's draft text, committed to `filters` on submit — the beta's `BuildFilters`
    // keeps them apart for the same reason: a keystroke is not a search, and every keystroke
    // being one would fire a request per character.
    let mut query_input = use_signal(String::new);
    // Who the author filter is pinned to, for the chip. Held beside `filters.author_id` rather
    // than derived from it because the id is what the query needs and a name is what the user
    // recognises, and the rows the list comes back with are the wrong place to look one up: a
    // filter that matched nothing would leave the chip with no name to draw.
    let mut pinned = use_signal(|| None::<profile::AuthorSearchResult>);
    // Which of the three views is showing. A signal rather than a `SearchFilters` field because
    // neither of the other two is a filter over the public rows — each is a different list, and
    // both can hold a build the search never returns: a favourite may have gone unlisted, and a
    // vault row may be private, which is a visibility the public query is defined to exclude.
    let mut view = use_signal(|| BrowserView::All);
    // Whether the tag chips are showing. Collapsed by default: thirty chips above the results
    // would push the builds themselves off the first screen, and most searches do not filter on
    // a tag at all. The ACTIVE ones stay visible when collapsed, so the disclosure never hides
    // state — only the menu of choices.
    let mut tags_open = use_signal(|| false);

    // The favourites grid. Keyed on the view flag, so entering the tab re-reads the local list
    // and re-fetches; a star toggled WHILE the tab is open deliberately does not re-run it, and
    // the card stays where it is with a hollow star — the beta's behaviour, and the one that does
    // not pull a row out from under the cursor that just clicked it.
    let favorites_account = account.clone();
    let favorites_list = use_resource(move || {
        let account = favorites_account.clone();
        let showing = view() == BrowserView::Favourites;
        async move {
            if !showing {
                return Ok(Vec::new());
            }
            let ids = favorites::ids().await;
            let cloud = account.prepared().await?;
            favorites::favorite_builds(&cloud, &ids).await
        }
    });

    let result = use_resource(move || {
        let account = account.clone();
        let filters = filters.read().clone();
        async move {
            let cloud = account.prepared().await?;
            shared_builds::search(&cloud, &filters).await
        }
    });

    let archetypes = database
        .as_ref()
        .and_then(|db| db.archetypes().ok())
        .map(|list| list.all().to_vec())
        .unwrap_or_default();

    let current = filters.read();
    let archetype = current.archetype.clone();
    let archname = archetype.as_deref();
    let mut primaries = arch_sets(&database, archname, ArchSide::Primary);
    let mut secondaries = arch_sets(&database, archname, ArchSide::Secondary);
    primaries.sort_by(|a, b| a.name.cmp(&b.name));
    secondaries.sort_by(|a, b| a.name.cmp(&b.name));
    let archetype_set = current.archetype.is_some();
    let primary_set = current.primary_set.clone();
    let secondary_set = current.secondary_set.clone();
    let active_tags = current.tags.clone();
    let is_views = current.sort_by == SortBy::Views;

    // `evt.prevent_default()` is not decoration. A `form` whose submit is not cancelled does
    // what a form does — a native GET to the current URL — and the page RELOADS: the modal
    // closes, the filters reset and the search the user just asked for is gone, with the only
    // trace a `?` appended to the address bar. It shipped that way in RB4d and was measured in
    // the running build 2026-09-17, by RB4f copying this shape and failing the same way.
    let submit = move |evt: Event<FormData>| {
        evt.prevent_default();
        let mut filters = filters;
        let query = query_input.read().trim().to_string();
        let mut next = filters.write();
        next.query = query;
        next.page = 1;
    };
    let set_archetype = move |value: String| {
        let mut filters = filters;
        let mut next = filters.write();
        next.archetype = (!value.is_empty()).then_some(value);
        next.primary_set = None;
        next.secondary_set = None;
        next.page = 1;
    };
    let set_picker = move |filter: Picker, value: String| {
        let mut filters = filters;
        let mut next = filters.write();
        match filter {
            Picker::Primary => next.primary_set = (!value.is_empty()).then_some(value),
            Picker::Secondary => next.secondary_set = (!value.is_empty()).then_some(value),
            Picker::Sort => {
                next.sort_by = if value == "views" {
                    SortBy::Views
                } else {
                    SortBy::Newest
                };
            }
        }
        next.page = 1;
    };
    let turn_page = move |delta: isize| {
        let mut filters = filters;
        let mut next = filters.write();
        next.page = next.page.saturating_add_signed(delta).max(1);
    };
    // Tags accumulate rather than replace: each one narrows the list further (the filter is
    // conjunctive — see `SearchFilters::tags`), so the control cannot be the same shape as the
    // archetype dropdown, which holds exactly one.
    //
    // Chips rather than a `select`, and that was measured rather than chosen. A select here is a
    // COMMAND — picking adds a tag and the control must then fall back to its placeholder — and
    // a Dioxus `select` cannot be made to do that: with a constant `value: ""` the attribute is
    // identical between renders so the diff writes nothing and the DOM keeps displaying the tag
    // that was picked, naming one of several filters as if it were all of them. A `key` does not
    // remount it, and a placeholder whose value changes per pick desynchronises the other way —
    // the select lands on `selectedIndex: -1` and draws blank. All three were built and watched
    // in a browser. A chip has no such state to lose: it is on or it is off, and it is the same
    // control the save dialog's picker already uses.
    let toggle_tag = move |tag: &'static str| {
        let mut filters = filters;
        let mut next = filters.write();
        match next.tags.iter().position(|kept| kept == tag) {
            Some(at) => {
                next.tags.remove(at);
            }
            None => next.tags.push(tag.to_string()),
        }
        next.page = 1;
    };

    let select_class = "sb-select";

    rsx! {
        Modal {
            title: "Shared Builds".to_string(),
            size: ModalSize::Full,
            on_close: move |_| open.set(None),
            div { class: "sb-browser",
                p { class: "sb-browser__intro",
                    "Builds shared by the community. Open one to load it into the planner."
                }
                // The three views: the public search (RB4d), favourites (RB4g), and the vault
                // (RB4k). One `.seg` strip, so "which list am I looking at" reads the same as
                // every other segmented choice in the app.
                div { class: "sb-tabs",
                    for (option, label) in [
                        (BrowserView::All, "All builds"),
                        (BrowserView::Favourites, "★ Favourites"),
                        (BrowserView::Mine, "My builds"),
                    ] {
                        button {
                            key: "{label}",
                            class: if view() == option { "seg active" } else { "seg" },
                            r#type: "button",
                            onclick: move |_| view.set(option),
                            "{label}"
                        }
                    }
                }
                if view() == BrowserView::All {
                    form {
                        class: "sb-search",
                        onsubmit: submit,
                        input {
                            class: "sb-search__input",
                            placeholder: "Search builds…",
                            value: query_input(),
                            oninput: move |evt| query_input.set(evt.value()),
                            "aria-label": "Search builds"
                        }
                        button { class: "seg is-primary", r#type: "submit", "Search" }
                    }
                    div { class: "sb-filters",
                        select {
                            class: select_class,
                            value: archetype.clone().unwrap_or_default(),
                            onchange: move |evt| set_archetype(evt.value()),
                            "aria-label": "Archetype",
                            option { value: "", "All Archetypes" }
                            for arch in &archetypes {
                                option { value: "{arch.id}",
                                    "{arch.name}" }
                            }
                        }
                        select {
                            class: select_class,
                            value: primary_set.clone().unwrap_or_default(),
                            disabled: !archetype_set,
                            onchange: move |evt| set_picker(Picker::Primary, evt.value()),
                            "aria-label": "Primary powerset",
                            option { value: "", "All Primaries" }
                            for set in &primaries {
                                option { key: "{set.id}", value: "{set.id}", "{set.name}" }
                            }
                        }
                        select {
                            class: select_class,
                            value: secondary_set.clone().unwrap_or_default(),
                            disabled: !archetype_set,
                            onchange: move |evt| set_picker(Picker::Secondary, evt.value()),
                            "aria-label": "Secondary powerset",
                            option { value: "", "All Secondaries" }
                            for set in &secondaries {
                                option { key: "{set.id}", value: "{set.id}", "{set.name}" }
                            }
                        }
                        select {
                            class: select_class,
                            value: if is_views { "views" } else { "newest" },
                            onchange: move |evt| set_picker(Picker::Sort, evt.value()),
                            "aria-label": "Sort",
                            option { value: "newest", "Newest" }
                            option { value: "views", "Most Viewed" }
                        }
                        button {
                            class: if tags_open() || !active_tags.is_empty() { "seg active" } else { "seg" },
                            r#type: "button",
                            onclick: move |_| tags_open.toggle(),
                            if active_tags.is_empty() {
                                "Tags ▾"
                            } else {
                                "Tags · {active_tags.len()} ▾"
                            }
                        }
                    }
                    // The active tags, always drawn — collapsing the picker must not hide which
                    // filters are on, or an empty result list has an invisible cause.
                    if !active_tags.is_empty() {
                        div { class: "sb-tagfilter",
                            for tag in active_tags.clone() {
                                button {
                                    key: "{tag}",
                                    class: "sb-chip is-on",
                                    r#type: "button",
                                    title: "Stop filtering on this tag",
                                    onclick: {
                                        let tag = tag.clone();
                                        move |_| {
                                            // Matched back to the curated spelling so the click
                                            // removes the same string the chip added. A tag that
                                            // is somehow not in the vocabulary is left alone
                                            // rather than silently dropped from the filter.
                                            if let Some(curated) = tag_vocab::canonical(&tag) {
                                                toggle_tag(curated);
                                            }
                                        }
                                    },
                                    "{tag} ✕"
                                }
                            }
                        }
                    }
                    if tags_open() {
                        div { class: "sb-tagpick",
                            for group in tag_vocab::GROUPS {
                                div { key: "{group.name}", class: "sb-tagpick__group",
                                    span { class: "sb-tagpick__name", "{group.name}" }
                                    div { class: "sb-tagpick__chips",
                                        for tag in group.tags.iter().copied() {
                                            button {
                                                key: "{tag}",
                                                class: if active_tags.iter().any(|kept| kept == tag) {
                                                    "sb-chip is-on"
                                                } else {
                                                    "sb-chip"
                                                },
                                                r#type: "button",
                                                onclick: move |_| toggle_tag(tag),
                                                "{tag}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // The author axis (RB4f). Below the dropdown row rather than in it, because a
                    // pick opens a result list and a select does not.
                    match pinned() {
                        None => rsx! {
                            AuthorFilter {
                                on_pick: move |author: profile::AuthorSearchResult| {
                                    let mut filters = filters;
                                    let mut next = filters.write();
                                    next.author_id = Some(author.user_id.clone());
                                    // Left absent on purpose: `author_name` only filters when there
                                    // is no id (`shared_builds.rs:177`), and setting both would put
                                    // a display string in the query where an account key belongs.
                                    next.author_name = None;
                                    next.page = 1;
                                    drop(next);
                                    pinned.set(Some(author));
                                },
                            }
                        },
                        Some(author) => {
                            let label = if author.display_name.is_empty() {
                                match author.handle.as_deref() {
                                    Some(handle) => format!("@{handle}"),
                                    None => "Unnamed author".to_string(),
                                }
                            } else {
                                author.display_name.clone()
                            };
                            let handle = author.handle.clone();
                            rsx! {
                                div { class: "sb-authorpin",
                                    span { class: "sb-authorpin__label", "Builds by {label}" }
                                    if let Some(handle) = handle {
                                        button {
                                            class: "seg",
                                            r#type: "button",
                                            onclick: move |_| {
                                                open.set(Some(BrowserTarget::Author(handle.clone())))
                                            },
                                            "Author page"
                                        }
                                    }
                                    button {
                                        class: "seg",
                                        r#type: "button",
                                        onclick: move |_| {
                                            let mut filters = filters;
                                            let mut next = filters.write();
                                            next.author_id = None;
                                            next.page = 1;
                                            drop(next);
                                            pinned.set(None);
                                        },
                                        "Clear"
                                    }
                                }
                            }
                        }
                    }
                    match &*result.read() {
                        None => rsx! { div { class: "load-state", "Searching builds…" } },
                        Some(Err(error)) => rsx! {
                            div { class: "load-state error", "{error}" }
                        },
                        Some(Ok(found)) => {
                            if found.builds.is_empty() {
                                rsx! {
                                    div { class: "empty-state",
                                        "No builds found."
                                        span { class: "hint", "Try adjusting your filters or search terms." }
                                    }
                                }
                            } else {
                                let noun = if found.total == 1 { "build" } else { "builds" };
                                rsx! {
                                    p { class: "sb-count",
                                        "{found.total} {noun}"
                                    }
                                    div { class: "sb-cards",
                                        for build in &found.builds {
                                            BuildCard { key: "{build.id}", shared: build.clone() }
                                        }
                                    }
                                    if found.total_pages > 1 {
                                        // Pagination. The count came from the server, so this control
                                        // never has to guess whether a next page exists.
                                        div { class: "sb-pagination",
                                            button {
                                                class: "seg",
                                                disabled: found.page <= 1,
                                                onclick: move |_| turn_page(-1),
                                                "Previous"
                                            }
                                            span { class: "sb-pagination__where",
                                                "Page {found.page} of {found.total_pages}"
                                            }
                                            button {
                                                class: "seg",
                                                disabled: found.page >= found.total_pages,
                                                onclick: move |_| turn_page(1),
                                                "Next"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else if view() == BrowserView::Favourites {
                    match &*favorites_list.read() {
                        None => rsx! { div { class: "load-state", "Loading favourites…" } },
                        Some(Err(error)) => rsx! {
                            div { class: "load-state error", "{error}" }
                        },
                        Some(Ok(builds)) => {
                            if builds.is_empty() {
                                rsx! {
                                    div { class: "empty-state",
                                        "No favourites yet."
                                        span { class: "hint",
                                            "Star a build to keep it here."
                                        }
                                    }
                                }
                            } else {
                                let noun = if builds.len() == 1 { "build" } else { "builds" };
                                rsx! {
                                    p { class: "sb-count", "{builds.len()} {noun}" }
                                    div { class: "sb-cards",
                                        for build in builds {
                                            BuildCard { key: "{build.id}", shared: build.clone() }
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else {
                    VaultPanel {}
                }
            }
        }
    }
}

/// Which of the browser's three lists is showing.
///
/// Three genuinely different queries, not three filters over one. [`BrowserView::All`] is the
/// public search; [`BrowserView::Favourites`] is a list of ids this browser stored, resolved one
/// by one; [`BrowserView::Mine`] is a read scoped to the signed-in account. Only the first is
/// constrained to `visibility = 'public'`, and that is why they cannot be collapsed: the other
/// two are each defined to contain rows the first is defined to exclude.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BrowserView {
    All,
    Favourites,
    Mine,
}

/// The vault — every build filed under the signed-in account (RB4k), and the one place a build
/// owned by nothing but a token can be got back.
///
/// **Why the bulk acts live here and the single ones live on the detail page.** RB4e put delete
/// and visibility on a build's own page, which is the right home for acting on the build in front
/// of you and the wrong one for tidying up twelve: twelve builds is twelve navigations, and the
/// user who wants to unpublish a year of drafts gives up. Neither surface replaces the other, and
/// they call the same two service functions, so there is no second rule about who may do what.
///
/// **Signed out, this is not an empty list.** An empty grid would say "you have no builds", which
/// is a claim about the account rather than about the session, and it is exactly the lie the
/// service's own doc refuses to tell. Signed out there is no account to make a claim about, so
/// the panel says so — and still draws [`ReclaimBox`], because reclaiming touches no network and
/// no session at all: it is the recovery path for somebody who has no account by choice.
#[component]
fn VaultPanel() -> Element {
    let account = use_context::<Account>();
    let user = account.user();

    let mut selected = use_signal(std::collections::BTreeSet::<String>::new);
    let mut working = use_signal(|| false);
    let mut refusal = use_signal(|| None::<String>);
    let mut confirming = use_signal(|| false);

    // The account id is read INSIDE the resource, not captured outside it, so the list re-runs
    // when a sign-in lands while the panel is open. Captured outside it, the closure would hold
    // the `None` it was built with and the tab would stay empty behind a signed-in header.
    let list_account = account.clone();
    let mut builds = use_resource(move || {
        let account = list_account.clone();
        let id = user.read().as_ref().map(|u| u.id.clone());
        async move {
            let Some(id) = id else {
                return Ok(Vec::new());
            };
            let cloud = account.prepared().await?;
            shared_builds::my_builds(&cloud, &id).await
        }
    });

    let signed_in = user.read().is_some();
    if !signed_in {
        return rsx! {
            div { class: "sb-vault",
                div { class: "load-state", "Sign in to see the builds saved to your account." }
                ReclaimBox {}
            }
        };
    }

    let picked = selected.read().len();

    // One handler for all four bulk buttons, as a `Callback` because a plain closure is not
    // `Copy` and five buttons need five copies. The selection is read INSIDE it rather than
    // captured: a list captured at render time would be the selection as it stood when the bar
    // was drawn, and the bar is drawn before the last checkbox is ticked.
    //
    // `working` gates the buttons, `refusal` carries whatever stopped it, and the list is re-read
    // afterwards either way — after a partial run the rows on screen are the only record of what
    // actually changed, and a stale grid would hide it.
    let run = use_callback(move |act: BulkAct| {
        let account = account.clone();
        let ids: Vec<String> = selected.read().iter().cloned().collect();
        spawn(async move {
            working.set(true);
            refusal.set(None);
            let outcome = apply_bulk(&account, act, &ids).await;
            if let Err(reason) = outcome {
                refusal.set(Some(reason));
            }
            selected.write().clear();
            confirming.set(false);
            working.set(false);
            builds.restart();
        });
    });

    rsx! {
        div { class: "sb-vault",
            ReclaimBox {}
            match &*builds.read() {
                None => rsx! { div { class: "load-state", "Loading your builds…" } },
                Some(Err(error)) => rsx! {
                    // Never an empty list. A failed vault read says it failed — see
                    // `shared_builds::my_builds` for why the beta's empty answer is not ported.
                    div { class: "load-state error", "{error}" }
                },
                Some(Ok(rows)) => {
                    if rows.is_empty() {
                        rsx! {
                            div { class: "empty-state",
                                "No builds saved to your account yet."
                                span { class: "hint",
                                    "Share or save a build while signed in and it lands here, private unless you publish it."
                                }
                            }
                        }
                    } else {
                        let noun = if rows.len() == 1 { "build" } else { "builds" };
                        rsx! {
                            div { class: "sb-vault__bar",
                                p { class: "sb-count", "{rows.len()} {noun}" }
                                if picked > 0 {
                                    span { class: "sb-owner__note", "{picked} selected" }
                                    for (option, label) in [
                                        (BuildVisibility::Public, "Public"),
                                        (BuildVisibility::Unlisted, "Unlisted"),
                                        (BuildVisibility::Private, "Private"),
                                    ] {
                                        button {
                                            key: "{label}",
                                            class: "seg",
                                            r#type: "button",
                                            disabled: working(),
                                            onclick: move |_| run.call(BulkAct::SetVisibility(option)),
                                            "{label}"
                                        }
                                    }
                                    if confirming() {
                                        span { class: "sb-owner__note",
                                            "Delete {picked} {noun} for everyone?"
                                        }
                                        button {
                                            class: "seg",
                                            r#type: "button",
                                            onclick: move |_| confirming.set(false),
                                            "Keep them"
                                        }
                                        button {
                                            class: "seg is-destructive",
                                            r#type: "button",
                                            disabled: working(),
                                            onclick: move |_| run.call(BulkAct::Delete),
                                            "Delete"
                                        }
                                    } else {
                                        button {
                                            class: "seg is-destructive",
                                            r#type: "button",
                                            disabled: working(),
                                            onclick: move |_| confirming.set(true),
                                            "Delete…"
                                        }
                                    }
                                    button {
                                        class: "seg",
                                        r#type: "button",
                                        disabled: working(),
                                        onclick: move |_| {
                                            selected.write().clear();
                                            confirming.set(false);
                                        },
                                        "Clear selection"
                                    }
                                }
                            }
                            if let Some(reason) = refusal() {
                                div { class: "load-state error", "{reason}" }
                            }
                            div { class: "sb-cards",
                                for row in rows {
                                    div { key: "{row.id}", class: "sb-vault__row",
                                        label { class: "sb-vault__pick",
                                            input {
                                                r#type: "checkbox",
                                                checked: selected.read().contains(&row.id),
                                                disabled: working(),
                                                onchange: {
                                                    let id = row.id.clone();
                                                    move |evt: Event<FormData>| {
                                                        let mut picks = selected.write();
                                                        if evt.checked() {
                                                            picks.insert(id.clone());
                                                        } else {
                                                            picks.remove(&id);
                                                        }
                                                    }
                                                },
                                            }
                                            span { class: "sb-vault__badge",
                                                "{visibility_label(row.visibility)}"
                                            }
                                        }
                                        BuildCard { shared: row.clone() }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// What a bulk button does to every selected row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BulkAct {
    SetVisibility(BuildVisibility),
    Delete,
}

/// Apply one act to every selected build, in order, **stopping at the first refusal and saying
/// where it stopped**.
///
/// Pressing on past a failure and reporting only the last one would leave the user with a list
/// that had partly changed and nothing to tell them which half — and the half that did not change
/// is the half that still needs doing. Stopping keeps the answer to "what happened" a single
/// number: the first `done` builds, in the order the list showed them.
///
/// `signed_in: true` is not an assumption. [`VaultPanel`] returns before this is reachable when
/// there is no session, and the rows themselves came back from a query keyed on the account id,
/// so a row here without a session is not a state this can be in. The owner token is passed as
/// `None` for the same reason: these rows are account-owned by definition, which is the
/// credential the server checks. Any token this browser happens to hold for one is dropped after
/// a successful delete, because a token for a row that no longer exists would be offered to
/// `claim-builds` forever as a build that failed.
async fn apply_bulk(account: &Account, act: BulkAct, ids: &[String]) -> Result<(), String> {
    let cloud = account.prepared().await.map_err(|e| e.to_string())?;
    for (done, id) in ids.iter().enumerate() {
        let outcome = match act {
            BulkAct::SetVisibility(visibility) => {
                shared_builds::update_build_visibility(&cloud, id, visibility, true).await
            }
            BulkAct::Delete => shared_builds::delete_build(&cloud, id, None, true).await,
        };
        if let Err(reason) = outcome {
            return Err(format!("Stopped after {done} of {}: {reason}", ids.len()));
        }
        if act == BulkAct::Delete {
            crate::cloud::owner_store::forget_owner_token(id).await;
        }
    }
    Ok(())
}

/// The word shown on a vault row's badge. A label, not a branch — the enum is the state.
fn visibility_label(visibility: BuildVisibility) -> &'static str {
    match visibility {
        BuildVisibility::Public => "Public",
        BuildVisibility::Unlisted => "Unlisted",
        BuildVisibility::Private => "Private",
    }
}

/// Recover a build whose only proof of ownership is an owner token (RB4k, the beta's
/// `reclaimBuild`).
///
/// A build shared without an account is owned by a secret the browser that made it stored
/// locally. Clear that storage, or move to another machine, and the build is still there and
/// still yours — but nothing here knows it. Pasting the token back is the whole recovery path,
/// and it is why the token is shown once at share time.
///
/// **The id is checked; the token cannot be.** `get_shared_build` refuses a typo'd id before
/// anything is stored, which is the common mistake and the one that would otherwise leave a dead
/// entry in the token map pointing at nothing. There is no endpoint that validates a token
/// without acting on the row, so a WRONG token is stored happily and is proven false at the first
/// delete or re-share — the same place the beta proves it, because it is the only place the
/// server will answer the question.
///
/// The `prevent_default` is the RB4d lesson, measured twice: a form whose submit is not cancelled
/// does a native GET to the current URL, the page reloads, and the modal and everything typed
/// into it are gone.
#[component]
fn ReclaimBox() -> Element {
    let account = use_context::<Account>();
    let mut open = use_context::<BrowserOpen>().0;
    let mut showing = use_signal(|| false);
    let mut id_input = use_signal(String::new);
    let mut token_input = use_signal(String::new);
    let mut working = use_signal(|| false);
    let mut refusal = use_signal(|| None::<String>);

    if !showing() {
        return rsx! {
            div { class: "sb-vault__reclaim",
                button {
                    class: "seg",
                    r#type: "button",
                    onclick: move |_| showing.set(true),
                    "Recover a build with its owner token…"
                }
            }
        };
    }

    let submit = move |evt: Event<FormData>| {
        evt.prevent_default();
        let account = account.clone();
        let id = id_input.read().trim().to_string();
        let token = token_input.read().trim().to_string();
        async move {
            if id.is_empty() || token.is_empty() {
                refusal.set(Some(
                    "Paste both the build id and its owner token.".to_string(),
                ));
                return;
            }
            working.set(true);
            refusal.set(None);
            let found = match account.prepared().await {
                Err(error) => Err(error.to_string()),
                Ok(cloud) => shared_builds::get_shared_build(&cloud, &id)
                    .await
                    .map_err(|error| error.to_string()),
            };
            match found {
                Err(reason) => {
                    refusal.set(Some(reason));
                    working.set(false);
                }
                Ok(None) => {
                    refusal.set(Some(format!("No build with the id “{id}”.")));
                    working.set(false);
                }
                Ok(Some(_)) => {
                    crate::cloud::owner_store::remember_owner_token(&id, &token).await;
                    // LAST: this unmounts the scope, which cancels anything still awaited.
                    open.set(Some(BrowserTarget::Build(id)));
                }
            }
        }
    };

    rsx! {
        form { class: "sb-vault__reclaim", onsubmit: submit,
            input {
                class: "sb-search__input",
                placeholder: "Build id",
                value: id_input(),
                disabled: working(),
                oninput: move |evt| id_input.set(evt.value()),
                "aria-label": "Build id"
            }
            input {
                class: "sb-search__input",
                placeholder: "Owner token",
                value: token_input(),
                disabled: working(),
                oninput: move |evt| token_input.set(evt.value()),
                "aria-label": "Owner token"
            }
            button {
                class: "seg is-primary",
                r#type: "submit",
                disabled: working(),
                "Recover"
            }
            button {
                class: "seg",
                r#type: "button",
                disabled: working(),
                onclick: move |_| {
                    showing.set(false);
                    refusal.set(None);
                },
                "Cancel"
            }
            if let Some(reason) = refusal() {
                span { class: "load-state error", "{reason}" }
            }
        }
    }
}

/// One author's page: who they are, and every public build they have shared (RB4f).
///
/// **Two requests, in order, and the order is the whole surface.** `resolve_author` turns the
/// handle in the URL into a `user_id`; only then can the build list be asked for, because
/// [`SearchFilters::author_id`] is the canonical author filter and a handle is not one. A handle
/// nobody holds stops at the first request, and that is a page that says so rather than an empty
/// grid that reads as an author with no builds.
#[component]
fn AuthorModal(handle: String) -> Element {
    let account = use_context::<Account>();
    let mut open = use_context::<BrowserOpen>().0;
    let page = use_signal(|| 1usize);

    let resolved = use_resource({
        let account = account.clone();
        let handle = handle.clone();
        move || {
            let account = account.clone();
            let handle = handle.clone();
            async move {
                let cloud = account.prepared().await?;
                profile::resolve_author(&cloud, &handle).await
            }
        }
    });

    // **Three states, not two.** The outer `Option` is whether the resolve has landed and the
    // inner one is whether anybody holds the handle, and collapsing them is a bug this surface
    // shipped for one build: `Ok(None)` and "still waiting" both read as `None`, so a handle
    // nobody holds drew "Looking up this author…" and never stopped. A 404 wearing a spinner is
    // the blank failure this row was written against, found by opening `/author/nobodyholdsthis`
    // in the running build 2026-09-17.
    let resolved_now: Option<Option<profile::PublicAuthor>> = match &*resolved.read() {
        None => None,
        Some(Err(error)) => {
            let error = error.to_string();
            return rsx! {
                Modal {
                    title: "Author".to_string(),
                    size: ModalSize::Full,
                    on_close: move |_| open.set(None),
                    div { class: "load-state error", "{error}" }
                }
            };
        }
        Some(Ok(found)) => Some(found.clone()),
    };
    let author = resolved_now.clone().flatten();

    let builds = use_resource({
        let account = account.clone();
        move || {
            let account = account.clone();
            let page = page();
            // **Read INSIDE the closure, and that is the whole of why this works.**
            // `use_resource` re-runs on the signals its closure reads, and a value read before it
            // is a value captured once. Resolving is a request, so at mount this is `None` — a
            // `user_id` lifted out of `resolved` above the closure would be that `None` forever,
            // and the page would draw an author with no builds. Which is exactly what it did,
            // found by opening `/author/<handle>` in the running build 2026-09-17.
            let user_id = resolved
                .read()
                .as_ref()
                .and_then(|outcome| outcome.as_ref().ok())
                .and_then(|found| found.as_ref())
                .map(|author| author.user_id.clone());
            async move {
                // Nothing to ask for until the resolve lands. A `None` here is the pending
                // state, not an empty result — the two must not draw the same.
                let Some(user_id) = user_id else {
                    return Ok(None);
                };
                let cloud = account.prepared().await?;
                let filters = SearchFilters {
                    author_id: Some(user_id),
                    page,
                    ..SearchFilters::default()
                };
                shared_builds::search(&cloud, &filters).await.map(Some)
            }
        }
    });

    let turn_page = move |delta: isize| {
        let mut page = page;
        let next = page.peek().saturating_add_signed(delta).max(1);
        page.set(next);
    };

    let title = match &author {
        Some(author) if !author.display_name.is_empty() => author.display_name.clone(),
        Some(author) => format!("@{}", author.handle),
        None => "Author".to_string(),
    };

    rsx! {
        Modal {
            title,
            size: ModalSize::Full,
            on_close: move |_| open.set(None),
            div { class: "sb-browser",
                match &resolved_now {
                    None => rsx! { div { class: "load-state", "Looking up this author…" } },
                    Some(None) => rsx! {
                        div { class: "empty-state",
                            "No author holds @{handle}."
                            span { class: "hint",
                                "The handle may have been changed, or the link may be mistyped."
                            }
                        }
                    },
                    Some(Some(author)) => {
                        let handle = author.handle.clone();
                        let bio = author.bio.clone();
                        // F81: only an https url on a host we serve avatars from
                        // reaches an `img src`; anything else renders the
                        // placeholder (`cloud::avatar`).
                        let avatar = author
                            .avatar_url
                            .as_deref()
                            .and_then(avatar_src)
                            .map(str::to_string);
                        rsx! {
                            div { class: "sb-author",
                                if let Some(avatar) = avatar {
                                    img { class: "sb-author__avatar", src: "{avatar}", alt: "" }
                                } else {
                                    div { class: "sb-author__avatar sb-author__avatar--none" }
                                }
                                div { class: "sb-author__who",
                                    p { class: "sb-author__handle", "@{handle}" }
                                    if !bio.is_empty() {
                                        p { class: "sb-author__bio", "{bio}" }
                                    }
                                }
                            }
                        }
                    }
                }

                // Absent rather than empty while the resolve is still out: a "no builds" line
                // under a name that has not loaded is a claim this surface cannot make yet.
                if author.is_some() {
                    match &*builds.read() {
                        None => rsx! { div { class: "load-state", "Loading builds…" } },
                        Some(Err(error)) => rsx! { div { class: "load-state error", "{error}" } },
                        Some(Ok(None)) => rsx! {},
                        Some(Ok(Some(found))) => {
                            if found.builds.is_empty() {
                                rsx! {
                                    div { class: "empty-state",
                                        "No public builds."
                                        span { class: "hint", "This author has not shared anything publicly yet." }
                                    }
                                }
                            } else {
                                let noun = if found.total == 1 { "build" } else { "builds" };
                                rsx! {
                                    p { class: "sb-count", "{found.total} {noun}" }
                                    div { class: "sb-cards",
                                        for build in &found.builds {
                                            BuildCard { key: "{build.id}", shared: build.clone() }
                                        }
                                    }
                                    if found.total_pages > 1 {
                                        div { class: "sb-pagination",
                                            button {
                                                class: "seg",
                                                disabled: found.page <= 1,
                                                onclick: move |_| turn_page(-1),
                                                "Previous"
                                            }
                                            span { class: "sb-pagination__where",
                                                "Page {found.page} of {found.total_pages}"
                                            }
                                            button {
                                                class: "seg",
                                                disabled: found.page >= found.total_pages,
                                                onclick: move |_| turn_page(1),
                                                "Next"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                div { class: "sb-detail__actions",
                    button {
                        class: "seg",
                        r#type: "button",
                        onclick: move |_| open.set(Some(BrowserTarget::Browse)),
                        "All builds"
                    }
                }
            }
        }
    }
}

/// The browser's author box (RB4f): find an author, then filter the list to them.
///
/// **It commits on Enter rather than as you type, and that is a constraint before it is a
/// choice.** The beta debounces `searchAuthors` by 250 ms (`BuildFilters.tsx:186`). There is no
/// timer in this crate both targets can reach — `tokio::time` is declared under
/// `cfg(not(target_arch = "wasm32"))`, and adding a wasm twin beside it would put the first
/// `cfg(target_arch = "wasm32")` into [`super`]'s client code, which its module doc makes a rule
/// against and `audit:cloud-target-arms` gates. Firing per keystroke instead is the option this
/// same file already argues against, twenty lines up, for the build search box.
///
/// So the box behaves like the search box beside it, which is at least the convention a reader
/// arrives with. A cross-target debounce would let this become live without changing the call,
/// and the note is here so that whoever adds one knows where it goes.
#[component]
fn AuthorFilter(on_pick: EventHandler<profile::AuthorSearchResult>) -> Element {
    let account = use_context::<Account>();
    let mut typed = use_signal(String::new);
    let mut committed = use_signal(String::new);

    let found = use_resource({
        let account = account.clone();
        move || {
            let account = account.clone();
            let query = committed();
            async move {
                if query.is_empty() {
                    return Ok(Vec::new());
                }
                let cloud = account.prepared().await?;
                profile::search_authors(&cloud, &query, profile::AUTHOR_SEARCH_LIMIT).await
            }
        }
    });

    let searching = !committed.read().is_empty();
    let too_short = {
        let typed = typed.read();
        let length = typed.trim().chars().count();
        length > 0 && length < profile::AUTHOR_SEARCH_MIN
    };

    rsx! {
        div { class: "sb-authorfilter",
            form {
                class: "sb-authorfilter__form",
                // Cancelled for the reason `BrowserModal`'s `submit` carries: an uncancelled
                // form submit reloads the page and takes the modal with it.
                onsubmit: move |evt: Event<FormData>| {
                    evt.prevent_default();
                    let query = typed.read().trim().to_string();
                    committed.set(query);
                },
                input {
                    class: "sb-search__input",
                    placeholder: "Find an author…",
                    value: typed(),
                    oninput: move |e| typed.set(e.value()),
                    "aria-label": "Find an author"
                }
                button { class: "seg", r#type: "submit", "Find" }
            }
            if too_short {
                p { class: "sb-authorfilter__hint",
                    "Type at least {profile::AUTHOR_SEARCH_MIN} characters."
                }
            }
            if searching {
                match &*found.read() {
                    None => rsx! { p { class: "sb-authorfilter__hint", "Searching…" } },
                    Some(Err(error)) => rsx! { p { class: "sb-authorfilter__hint error", "{error}" } },
                    Some(Ok(authors)) if authors.is_empty() => rsx! {
                        p { class: "sb-authorfilter__hint", "No authors found." }
                    },
                    Some(Ok(authors)) => rsx! {
                        div { class: "sb-authorfilter__list",
                            for author in authors.iter().cloned() {
                                button {
                                    key: "{author.user_id}",
                                    class: "sb-authorfilter__row",
                                    r#type: "button",
                                    onclick: {
                                        let author = author.clone();
                                        move |_| {
                                            on_pick.call(author.clone());
                                            typed.set(String::new());
                                            committed.set(String::new());
                                        }
                                    },
                                    if let Some(avatar) = author.avatar_url.as_deref().and_then(avatar_src) {
                                        img { class: "sb-authorfilter__avatar", src: "{avatar}", alt: "" }
                                    } else {
                                        div { class: "sb-authorfilter__avatar sb-authorfilter__avatar--none" }
                                    }
                                    span { class: "sb-authorfilter__name",
                                        if author.display_name.is_empty() {
                                            match author.handle.as_deref() {
                                                Some(handle) => rsx! { "@{handle}" },
                                                // An account with neither is still a row the RPC
                                                // returned; drawing nothing would make it
                                                // unclickable for no stated reason.
                                                None => rsx! { "Unnamed author" },
                                            }
                                        } else {
                                            "{author.display_name}"
                                        }
                                    }
                                    if let Some(handle) = author.handle.as_deref() {
                                        span { class: "sb-authorfilter__at", "@{handle}" }
                                    }
                                    span { class: "sb-authorfilter__count",
                                        "{author.build_count} {build_word(author.build_count)}"
                                    }
                                }
                            }
                        }
                    },
                }
            }
        }
    }
}

fn build_word(count: i64) -> &'static str {
    if count == 1 {
        "build"
    } else {
        "builds"
    }
}

/// The sort pickers are three selects over one write path; naming them keeps the arms from
/// being a tuple nobody can read.
#[derive(Clone, Copy)]
enum Picker {
    Primary,
    Secondary,
    Sort,
}

/// The archetype's own primary/secondary powerset lists, resolved to sets — the beta's
/// `getPowersetsForArchetype` read from the export's owned relationship rather than from a
/// category scan. The ids are the export's; the names come from resolving them.
fn arch_sets<'a>(
    database: &'a Option<Db>,
    archetype: Option<&str>,
    side: ArchSide,
) -> Vec<&'a Powerset> {
    let Some(archetype) = archetype else {
        return Vec::new();
    };
    let Some(db) = database else {
        return Vec::new();
    };
    let Ok(list) = db.archetypes() else {
        return Vec::new();
    };
    let Some(at) = list.get(archetype) else {
        return Vec::new();
    };
    let ids = match side {
        ArchSide::Primary => &at.primary_sets,
        ArchSide::Secondary => &at.secondary_sets,
    };
    ids.iter().filter_map(|id| db.find_powerset(id)).collect()
}

#[derive(Clone, Copy)]
enum ArchSide {
    Primary,
    Secondary,
}

/// The author half of a build's byline, on the card and on the detail view.
///
/// One component rather than two blocks of markup, because what it carries is a *rendering*
/// invariant: the `@` is written here, in front of a handle the row's profile join proved, and
/// nowhere else in this file. Two copies of that rule would be two places for it to stop being
/// true, and the free-text half of a byline reading as a claimed handle is the whole of
/// [`profile::author_identity`] is where the decision is made and graded.
///
/// **It draws text and never a link.** Linking is the enclosing surface's business because the
/// card cannot do it: `.sb-card` is a `button` element, a `button` inside a `button` is not
/// nestable HTML and the parser closes the card — the same constraint that put the favourite
/// star outside the card as a positioned sibling. The detail view is a `div`, so [`DetailAuthor`]
/// wraps this in a button of its own.
#[component]
fn AuthorLabel(identity: profile::AuthorIdentity) -> Element {
    match identity {
        profile::AuthorIdentity::Anonymous => rsx! { "Anonymous" },
        profile::AuthorIdentity::Unverified { display } => rsx! { "{display}" },
        profile::AuthorIdentity::Verified { display, handle } => rsx! {
            if !display.is_empty() {
                "{display} "
            }
            span {
                class: "sb-author-handle",
                title: "A handle this account holds",
                "@{handle}"
            }
        },
    }
}

/// The detail view's byline: the same label, plus the one thing the card cannot carry — a proved
/// handle is a link to that author's page.
///
/// The link is on that arm alone because it is the only arm with somewhere to send a click.
/// `Unverified` names nobody: `resolve_author` would answer not-found for it, and a not-found
/// page reached by clicking a name reads as a broken app rather than as an unclaimed name.
#[component]
fn DetailAuthor(identity: profile::AuthorIdentity) -> Element {
    let mut open = use_context::<BrowserOpen>().0;
    if matches!(identity, profile::AuthorIdentity::Anonymous) {
        return rsx! {};
    }
    let link = match &identity {
        profile::AuthorIdentity::Verified { handle, .. } => Some(handle.clone()),
        _ => None,
    };
    rsx! {
        span {
            "By "
            match link {
                Some(handle) => rsx! {
                    button {
                        class: "sb-detail__author",
                        r#type: "button",
                        title: "Open this author's page",
                        onclick: move |_| open.set(Some(BrowserTarget::Author(handle.clone()))),
                        AuthorLabel { identity }
                    }
                },
                None => rsx! { AuthorLabel { identity } },
            }
        }
    }
}

/// One build on the browse grid (the beta's `BuildCard`, minus what RB4e's owner controls own),
/// with RB4g's favourite star. Clicking the card opens the detail view.
///
/// **The star is a sibling of the card button, not a child of it.** A `button` inside a `button`
/// is not nestable HTML — the parser closes the outer one and the card stops being one control —
/// which is why the beta draws its star as a `span role="button"` instead. That span cannot be
/// tabbed to, so this is the same star placed differently: a real button, positioned over the
/// card's corner, reachable by keyboard, with the card still the one click target the grid's own
/// CSS note promises.
///
/// **The state is read per card rather than from a list the grid holds.** One `localStorage` read
/// per card is a parse of a short array and buys the thing a shared snapshot would have to keep
/// correct by hand: the star always shows what storage says, including right after a sign-out
/// cleared it. `None` until that read lands — a control that has not been told yet draws nothing
/// rather than guessing at hollow.
#[component]
fn BuildCard(shared: SharedBuildSummary) -> Element {
    let account = use_context::<Account>();
    let mut open = use_context::<BrowserOpen>().0;
    let created = share_date(&shared.created_at);
    let author = profile::author_identity(&shared.author_name, shared.author_handle.as_deref());

    let mut starred = use_signal(|| None::<bool>);
    let read_id = shared.id.clone();
    use_future(move || {
        let id = read_id.clone();
        async move { starred.set(Some(favorites::is_favorite(&id).await)) }
    });

    let star_id = shared.id.clone();
    let open_id = shared.id.clone();

    rsx! {
        div { class: "sb-card__wrap",
            button {
                class: "sb-card",
                r#type: "button",
                onclick: move |_| open.set(Some(BrowserTarget::Build(open_id.clone()))),
                div { class: "sb-card__head",
                    h3 { class: "sb-card__name", "{shared.name}" }
                    span { class: "sb-card__level", "Lv {shared.level}" }
                }
                div { class: "sb-card__sets",
                    span { class: "sb-card__archetype", "{shared.archetype_name}" }
                    span { "{shared.primary_name} / {shared.secondary_name}" }
                }
                if !shared.description.is_empty() {
                    p { class: "sb-card__desc", "{shared.description}" }
                }
                if !shared.tags.is_empty() {
                    div { class: "sb-tags",
                        for tag in shared.tags.iter().take(4) {
                            span { class: "sb-tag", "{tag}" }
                        }
                        if shared.tags.len() > 4 {
                            span { class: "sb-tag sb-tag--more", "+{shared.tags.len() - 4}" }
                        }
                    }
                }
                div { class: "sb-card__foot",
                    span {
                        AuthorLabel { identity: author }
                        if !shared.server.is_empty() { " · {shared.server}" }
                    }
                    span { "{shared.views} views · {created}" }
                }
            }
            if let Some(on) = starred() {
                button {
                    class: if on { "sb-star is-on" } else { "sb-star" },
                    r#type: "button",
                    title: if on { "Remove from favourites" } else { "Add to favourites" },
                    "aria-pressed": if on { "true" } else { "false" },
                    "aria-label": "Favourite",
                    onclick: {
                        let account = account.clone();
                        let id = star_id.clone();
                        move |_| {
                            let account = account.clone();
                            let id = id.clone();
                            async move {
                                // Moved before the storage round trip, because the local list is
                                // the truth and it is about to say this. The authoritative answer
                                // replaces it a microtask later and is almost always the same
                                // value; when it is not, storage changed under this card and the
                                // second write is the one to believe.
                                starred.set(Some(!on));
                                let now = favorites::toggle(&id).await;
                                starred.set(Some(now));
                                // Mirrored to the account, fire-and-forget. The local list is
                                // already correct, so a failure here costs the user nothing they
                                // can see, and `favorites::sync` reconciles at the next login —
                                // which is the whole bargain the union merge exists to keep.
                                let Some(user_id) =
                                    account.user().peek().as_ref().map(|user| user.id.clone())
                                else {
                                    return;
                                };
                                if let Ok(cloud) = account.prepared().await {
                                    let _ = favorites::mirror(&cloud, &user_id, &id, now).await;
                                }
                            }
                        }
                    },
                    if on { "★" } else { "☆" }
                }
            }
        }
    }
}

// ============================================================
// The single-build view.
// ============================================================

/// One build by id — the surface `get-build` exists for. Fetch, count the view, and render the
/// read-only view; the owner-scoped controls (delete, visibility, metadata editing) are RB4e.
#[component]
fn BuildDetailModal(id: String, database: Option<Db>, dataset: Signal<DatasetId>) -> Element {
    let account = use_context::<Account>();
    let mut open = use_context::<BrowserOpen>().0;
    let session = use_context::<BuildSession>();
    let pending = use_context::<BuildIoPending>().0;
    let choice = use_context::<BuildIoChoice>().0;
    let report = use_context::<BuildIoReport>().0;
    let copied = use_signal(|| false);
    let copy_error = use_signal(|| None::<String>);
    let load_refusal = use_signal(|| None::<String>);

    // One resource for the whole page: the fetch and the view count are one act, exactly as
    // the beta performs them (`BuildDetailPage.tsx:104-110`) — count only what was actually
    // shown. The count is fire-and-forget: a failed RPC is silently ignored, per the service
    // contract, and cannot fail the page.
    let id_for_fetch = id.clone();
    let build = use_resource(move || {
        let account = account.clone();
        let id = id_for_fetch.clone();
        async move {
            let cloud = account.prepared().await?;
            let found = shared_builds::get_shared_build(&cloud, &id).await;
            if let Ok(Some(_)) = &found {
                let _ = shared_builds::increment_views(&cloud, &id).await;
            }
            found
        }
    });

    rsx! {
        Modal {
            title: "Shared Build".to_string(),
            size: ModalSize::Xl,
            on_close: move |_| open.set(None),
            div { class: "sb-detail",
                match &*build.read() {
                    None => rsx! { div { class: "load-state", "Loading build…" } },
                    Some(Err(error)) => rsx! {
                        div { class: "load-state error", "{error}" }
                        div { class: "sb-detail__actions",
                            button {
                                class: "seg",
                                onclick: move |_| open.set(Some(BrowserTarget::Browse)),
                                "← Back to builds"
                            }
                        }
                    },
                    Some(Ok(None)) => rsx! {
                        div { class: "sb-detail__missing",
                            h3 { "Build Not Found" }
                            p { "This build does not exist, has been removed, or is private." }
                        }
                        div { class: "sb-detail__actions",
                            button {
                                class: "seg",
                                onclick: move |_| open.set(Some(BrowserTarget::Browse)),
                                "← Back to builds"
                            }
                        }
                    },
                    Some(Ok(Some(build))) => rsx! {
                        DetailBody {
                            shared: build.clone(),
                            id: id.clone(),
                            database,
                            dataset,
                            session,
                            pending,
                            choice,
                            report,
                            copied,
                            copy_error,
                            load_refusal,
                        }
                    },
                }
            }
        }
    }
}

/// The build's own content, split from the fetch so the states above stay short.
#[component]
fn DetailBody(
    shared: SharedBuild,
    id: String,
    database: Option<Db>,
    dataset: Signal<DatasetId>,
    session: BuildSession,
    pending: Signal<Option<crate::build_io::PendingImport>>,
    choice: Signal<Option<crate::build_io::CrossForkChoice>>,
    report: Signal<Option<crate::build_io::BuildIoOutcome>>,
    mut copied: Signal<bool>,
    mut copy_error: Signal<Option<String>>,
    mut load_refusal: Signal<Option<String>>,
) -> Element {
    let account = use_context::<Account>();
    let mut open = use_context::<BrowserOpen>().0;

    // The document is unpacked from the row here and nowhere else. Everything below this line
    // draws card columns, and `shared` is the card-column half from here on — so the two
    // places that hand a stranger's document to a reader (the preview capture, and "Load into
    // Planner") are the two that name `build_json`, and a grep for it finds both (F19).
    let SharedBuild {
        summary: shared,
        build_json,
    } = shared;

    // Preview backfill (RB4d): once per open of a build that wants a fresh preview — missing
    // image, stale template, and not private — draw the 1200×880 card and push it to
    // `backfill-preview`. Fire-and-forget, exactly as the beta's hidden capture iframe was:
    // a capture that fails must not fail the page, because the state it fails INTO (no
    // preview image) is the state that triggered the attempt, and the server re-checks the
    // version gate and the 1200×880 shape on every write anyway. The captures are bound
    // OUTSIDE the resource, so Dioxus sees no tracked dependency inside it and the resource
    // runs once per modal open — the beta's `previewCaptureAttempted` guard, structurally.
    // A build whose own dataset is not loaded is refused before anything is rendered, so the
    // card can never draw the wrong fork's data as though it were the build's
    // ([`shared_builds::preview_capture_matches`]).
    let preview_for = shared.clone();
    let preview_json = build_json.clone();
    let preview_database = database.clone();
    let preview_loaded = dataset();
    // `use_resource` returns a `Resource` handle, which the UI framework makes awaitable; this
    // call site wants the side effect and not the handle, which is the documented way to use it.
    // The lint reads the discarded handle as a dropped future and is wrong here.
    #[allow(clippy::let_underscore_future)]
    let _ = use_resource(move || {
        let shared = preview_for.clone();
        let build_json = preview_json.clone();
        let database = preview_database.clone();
        let loaded = preview_loaded;
        let account = account.clone();
        async move {
            if !shared_builds::preview_needs_backfill(&shared) {
                return;
            }
            if !shared_builds::preview_capture_matches(&build_json, loaded) {
                return;
            }
            match database {
                None => (),
                Some(database) => {
                    let Ok(text) = serde_json::to_string(&build_json) else {
                        return;
                    };
                    let Ok(decoded) = crate::build_file::decode_import(&text, &database, loaded)
                    else {
                        return;
                    };
                    let totals = coh_math::recalculate(&decoded.build, &database);
                    let Ok(bytes) =
                        crate::export_image::render_preview_png(&decoded.build, &database, &totals)
                    else {
                        return;
                    };
                    let payload = crate::clipboard::base64(&bytes);
                    if let Ok(cloud) = account.prepared().await {
                        match shared_builds::submit_preview_backfill(&cloud, &shared.id, &payload)
                            .await
                        {
                            Ok(_) => (),
                            Err(reason) => {
                                // Best-effort does not mean mute: a refusal at the gate means
                                // the server kept the missing preview, and the reason is the
                                // only evidence there is (the beta logs the same way —
                                // `SharePreviewCapture.tsx`'s `console.error`).
                                let msg = format!("preview backfill refused: {reason}");
                                // The message reaches JS as a JSON string literal, not through
                                // `{:?}` (F29). `reason` is the server's own words, and Rust's
                                // debug escaping is a debug format with no stability guarantee —
                                // close enough to JS's to be tempting and not the same language.
                                // The rule `crate::build_file` states and `session::write_stored`
                                // follows.
                                if let Ok(literal) = serde_json::to_string(&msg) {
                                    let _ = document::eval(&format!("console.error({literal});"));
                                }
                            }
                        }
                    }
                }
            }
        }
    });
    let visibility_note = match shared.visibility {
        BuildVisibility::Private => Some("This build is private. Only you can see it."),
        BuildVisibility::Unlisted => Some(
            "This build is unlisted. Anyone with this link can see it, but it won't appear in search.",
        ),
        BuildVisibility::Public => None,
    };
    let date = share_date(&shared.created_at);
    let author = profile::author_identity(&shared.author_name, shared.author_handle.as_deref());
    let chips = power_chips(&build_json);

    rsx! {
        button {
            class: "sb-detail__back",
            r#type: "button",
            onclick: move |_| open.set(Some(BrowserTarget::Browse)),
            "← Back to builds"
        }
        div { class: "sb-detail__head",
            h3 { class: "sb-detail__name", "{shared.name}" }
            p { class: "sb-detail__archetype", "{shared.archetype_name} — Level {shared.level}" }
            div { class: "sb-detail__meta",
                DetailAuthor { identity: author }
                if !shared.server.is_empty() { span { "{shared.server}" } }
                span { "{date}" }
                span { "{shared.views} views" }
            }
        }
        if let Some(note) = visibility_note {
            div { class: "sb-detail__visibility", "{note}" }
        }
        div { class: "sb-detail__actions",
            button {
                class: "seg",
                r#type: "button",
                onclick: {
                    let id = id.clone();
                    move |_| {
                        let id = id.clone();
                        async move {
                            match copy_url(&id).await {
                                Ok(()) => copied.set(true),
                                Err(reason) => copy_error.set(Some(reason)),
                            }
                        }
                    }
                },
                if copied() { "Copied!" } else { "Copy Link" }
            }
            button {
                class: "seg is-primary",
                r#type: "button",
                onclick: {
                    let shared = shared.clone();
                    let build_json = build_json.clone();
                    move |_| {
                        let Ok(text) = serde_json::to_string(&build_json) else {
                            load_refusal.set(Some("the shared build could not be read as text".to_string()));
                            return;
                        };
                        crate::build_io::take_build(
                            text,
                            format!("Shared build — {}", shared.name),
                            database.clone(),
                            dataset,
                            session,
                            pending,
                            choice,
                            report,
                        );
                        // LAST: the planner now holds the build (or is about to ask about a
                        // different fork), and the modal is done either way.
                        open.set(None);
                    }
                },
                "Load into Planner"
            }
            if let Some(reason) = copy_error() {
                span { class: "load-state error", "{reason}" }
            }
        }
        OwnerActions { shared: shared.clone() }
        div { class: "sb-detail__sets",
            div { class: "sb-detail__set", span { class: "sb-detail__set-label", "Primary" } span { "{shared.primary_name}" } }
            div { class: "sb-detail__set", span { class: "sb-detail__set-label", "Secondary" } span { "{shared.secondary_name}" } }
        }
        if !shared.description.is_empty() {
            div { class: "sb-detail__block",
                h4 { "Description" }
                p { class: "sb-detail__desc", "{shared.description}" }
            }
        }
        if !shared.tags.is_empty() {
            div { class: "sb-tags",
                for tag in &shared.tags { span { class: "sb-tag", "{tag}" } }
            }
        }
        div { class: "sb-detail__block",
            h4 { "Powers" }
            div { class: "sb-powers",
                for group in &chips {
                    div { class: "sb-powers__group",
                        p { class: "sb-powers__label",
                            "{group.label}: " span { class: "sb-powers__name", "{group.name}" }
                        }
                        div { class: "sb-powers__chips",
                            for chip in &group.powers {
                                span {
                                    class: "sb-powers__chip",
                                    title: format!("Level {} · {} {}", chip.level, chip.slots, slot_word(chip.slots)),
                                    "{chip.name}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The beta's `PowersetSummary`, read from the shared build's own `build_json` — rendered
/// defensively because a build is an unvalidated document from a server, not a guarantee.
#[derive(Clone, Debug, PartialEq)]
struct PowerGroup {
    label: &'static str,
    name: String,
    powers: Vec<PowerChip>,
}

#[derive(Clone, Debug, PartialEq)]
struct PowerChip {
    name: String,
    level: u64,
    slots: usize,
}

/// Walk `build_json.build.{primary,secondary,pools[*],epicPool}` for the power lists, in the
/// order the beta's detail page draws them. Anything unparseable yields nothing rather than an
/// error — a foreign build is shown as far as it reads, not blanked or slaughtered.
fn power_chips(json: &serde_json::Value) -> Vec<PowerGroup> {
    let Some(build) = json.get("build") else {
        return Vec::new();
    };
    let mut groups = Vec::new();

    let mut add_set = |key: &str, label: &'static str| {
        let Some(set) = build.get(key) else {
            return;
        };
        let name = set
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let powers = power_chips_of(set.get("powers"));
        if powers.is_empty() {
            return;
        }
        groups.push(PowerGroup {
            label,
            name: name.to_string(),
            powers,
        });
    };
    add_set("primary", "Primary");
    add_set("secondary", "Secondary");

    if let Some(pools) = build.get("pools").and_then(serde_json::Value::as_array) {
        for pool in pools {
            let name = pool
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            let powers = power_chips_of(pool.get("powers"));
            if powers.is_empty() {
                continue;
            }
            groups.push(PowerGroup {
                label: "Pool",
                name: name.to_string(),
                powers,
            });
        }
    }
    if let Some(epic) = build.get("epicPool") {
        let name = epic
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let powers = power_chips_of(epic.get("powers"));
        if !powers.is_empty() {
            groups.push(PowerGroup {
                label: "Epic",
                name: name.to_string(),
                powers,
            });
        }
    }
    groups
}

fn power_chips_of(powers: Option<&serde_json::Value>) -> Vec<PowerChip> {
    let Some(powers) = powers.and_then(serde_json::Value::as_array) else {
        return Vec::new();
    };
    powers
        .iter()
        .filter_map(|power| {
            let name = power.get("name").and_then(serde_json::Value::as_str)?;
            let level = power
                .get("level")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            let slots = power
                .get("slots")
                .and_then(serde_json::Value::as_array)
                .map_or(0, Vec::len);
            Some(PowerChip {
                name: name.to_string(),
                level,
                slots,
            })
        })
        .collect()
}

/// What the owner of a build can do to it from its own page (RB4e): change who can see it, and
/// delete it.
///
/// **Here rather than only on the vault list**, because the detail page is the one surface that
/// already knows which build is in front of the user, and the two acts are about THIS build. The
/// bulk home landed in RB4k ([`VaultPanel`]) and calls the same two service functions, so there
/// is one rule about who may do what and two places to ask it from.
///
/// Ownership is asked once, asynchronously, because half the answer lives in `localStorage`
/// ([`crate::cloud::save_build::is_owned`]). Until it answers, nothing is drawn: a delete button
/// that appears a moment late is better than one that appears for a build the viewer does not own
/// and then vanishes.
///
/// The two acts differ in who may perform them, and that is the server's rule rather than a
/// choice here. A DELETE takes either credential. A VISIBILITY change is account-only
/// (`update-build-visibility/index.ts:88`) — an anonymous link-holder may edit a row they hold a
/// token for but must not be able to publish it — so that half is drawn only for the signed-in
/// owner of the row.
#[component]
fn OwnerActions(shared: SharedBuildSummary) -> Element {
    let account = use_context::<Account>();
    let mut open = use_context::<BrowserOpen>().0;
    let user = account.user();

    let mut confirming = use_signal(|| false);
    let mut working = use_signal(|| false);
    let mut refusal = use_signal(|| None::<String>);
    let mut visibility = use_signal(|| shared.visibility);
    let mut changed = use_signal(|| false);

    let id = shared.id.clone();
    let token_for = id.clone();
    let token = use_resource(move || {
        let id = token_for.clone();
        async move { crate::cloud::owner_store::owner_token(&id).await }
    });

    let held = match &*token.read_unchecked() {
        Some(token) => token.clone(),
        // Still reading storage — draw nothing rather than guessing at ownership.
        None => return rsx! {},
    };
    let account_id = user.read().as_ref().map(|u| u.id.clone());
    if !crate::cloud::save_build::is_owned(&shared, held.as_deref(), account_id.as_deref()) {
        return rsx! {};
    }
    // Only the ACCOUNT owner may publish or unpublish; a token holder may not.
    let may_set_visibility =
        account_id.is_some() && shared.user_id.as_deref() == account_id.as_deref();
    let signed_in = account_id.is_some();

    rsx! {
        div { class: "sb-owner",
            span { class: "sb-owner__label", "You own this build" }
            if may_set_visibility {
                div { class: "sb-save__choice",
                    for (option, label) in [
                        (shared_builds::BuildVisibility::Public, "Public"),
                        (shared_builds::BuildVisibility::Unlisted, "Unlisted"),
                        (shared_builds::BuildVisibility::Private, "Private"),
                    ] {
                        button {
                            key: "{label}",
                            class: if visibility() == option { "seg active" } else { "seg" },
                            r#type: "button",
                            disabled: working(),
                            onclick: {
                                let account = account.clone();
                                let id = id.clone();
                                move |_| {
                                    let account = account.clone();
                                    let id = id.clone();
                                    async move {
                                        working.set(true);
                                        refusal.set(None);
                                        let outcome = match account.prepared().await {
                                            Err(e) => Err(shared_builds::ShareError::Cloud(e)),
                                            Ok(cloud) => {
                                                shared_builds::update_build_visibility(
                                                    &cloud, &id, option, true,
                                                )
                                                .await
                                            }
                                        };
                                        match outcome {
                                            // The signal follows the SERVER having accepted it,
                                            // never the click — an optimistic flip would show a
                                            // visibility the row does not have.
                                            Ok(()) => {
                                                visibility.set(option);
                                                changed.set(true);
                                            }
                                            Err(reason) => refusal.set(Some(reason.to_string())),
                                        }
                                        working.set(false);
                                    }
                                }
                            },
                            "{label}"
                        }
                    }
                }
                if changed() {
                    span { class: "sb-owner__note", "Saved." }
                }
            }
            div { class: "sb-detail__actions",
                if confirming() {
                    span { class: "sb-owner__note", "Delete this build for everyone?" }
                    button {
                        class: "seg",
                        r#type: "button",
                        onclick: move |_| confirming.set(false),
                        "Keep it"
                    }
                    button {
                        class: "seg is-destructive",
                        r#type: "button",
                        disabled: working(),
                        onclick: {
                            let account = account.clone();
                            let id = id.clone();
                            let held = held.clone();
                            move |_| {
                                let account = account.clone();
                                let id = id.clone();
                                let held = held.clone();
                                async move {
                                    working.set(true);
                                    refusal.set(None);
                                    let outcome = match account.prepared().await {
                                        Err(e) => Err(shared_builds::ShareError::Cloud(e)),
                                        Ok(cloud) => {
                                            shared_builds::delete_build(
                                                &cloud, &id, held.as_deref(), signed_in,
                                            )
                                            .await
                                        }
                                    };
                                    match outcome {
                                        Ok(()) => {
                                            // The row is gone, so the token that opened it is
                                            // dead weight — left behind it would be offered to
                                            // `claim-builds` forever as a build that failed.
                                            crate::cloud::owner_store::forget_owner_token(&id)
                                                .await;
                                            // LAST: this closes the detail and unmounts the
                                            // scope, which cancels anything still awaited.
                                            open.set(Some(BrowserTarget::Browse));
                                        }
                                        Err(reason) => {
                                            refusal.set(Some(reason.to_string()));
                                            working.set(false);
                                        }
                                    }
                                }
                            }
                        },
                        "Delete"
                    }
                } else {
                    button {
                        class: "seg is-destructive",
                        r#type: "button",
                        disabled: working(),
                        onclick: move |_| confirming.set(true),
                        "Delete build…"
                    }
                }
            }
            if let Some(reason) = refusal() {
                span { class: "load-state error", "{reason}" }
            }
        }
    }
}

/// The share URL for a build: `origin/builds/<id>`, mirroring how the beta's cards navigate
/// and what any share link points at. The origin is read from the running page because only
/// the page knows it (`auth.ts:10` does the same for the OAuth redirect target).
/// The origin a share link is built on: the page's own when a browser could fetch it, the public
/// site otherwise.
///
/// [`copy_url`] pasted `window.location.origin` in front of the path, which
/// is right on the web and wrong in the desktop app. The shell there is served from a custom
/// scheme, so the origin is `dioxus://index.html` — measured rather than assumed, by
/// `examples/pop3_storage_origin.rs`, which read `location.origin` on six runs — and Copy Link
/// handed the tester `dioxus://index.html/builds/<id>`. That resolves inside the webview that
/// produced it and nowhere else, so the one action whose entire purpose is to leave the app
/// produced a string that could not.
///
/// **The test is the scheme, not the platform.** An origin a browser could not fetch is one no
/// recipient can open, whatever shell produced it; a `cfg(feature = "desktop")` would answer only
/// for the shell we have now and would have to be remembered by the next one that is not a
/// browser. `http` is allowed alongside `https` so a dev build on localhost still shares the link
/// it is actually serving.
///
/// A failed read takes the same door. There is no hardcode being reached for here — the public
/// site IS the right answer for any shell that is not a page, so falling back to it is the
/// correct link rather than a stand-in for one.
pub(crate) fn share_origin(page_origin: Option<&str>) -> &str {
    match page_origin {
        Some(origin) if origin.starts_with("https://") || origin.starts_with("http://") => origin,
        _ => crate::app_actions::SITE_ORIGIN,
    }
}

pub(crate) async fn copy_url(id: &str) -> Result<(), String> {
    let page_origin = read_origin().await;
    let origin = share_origin(page_origin.as_deref());
    crate::clipboard::copy(&format!("{origin}/builds/{id}")).await
}

/// The calendar day a build was published, cut from its RFC3339 timestamp
/// ("2026-09-16"). The planner never reads the wall clock on wasm — wasm32 has
/// no `SystemTime`, and `chrono::Local::now()` panics there — so a relative
/// "3h ago" is impossible in the web build. Cards and detail agree on the
/// absolute date instead.
fn share_date(iso: &str) -> String {
    iso.chars().take(10).collect::<String>()
}

// The beta chips the card's date as a locale string; the rebuild renders the ISO date part.
// Both are display-only; neither deserves a dependency.

/// The noun for a slot count, for the power chip's hover (`1 slot`, `2 slots`).
fn slot_word(slots: usize) -> &'static str {
    if slots == 1 {
        "slot"
    } else {
        "slots"
    }
}

// ============================================================
// The boot-time path reader.
// ============================================================

const READ_PATHNAME: &str = "\
try { return JSON.stringify(window.location.pathname); } catch (_) { return null; }";

const READ_ORIGIN: &str = "\
try { return JSON.stringify(window.location.origin); } catch (_) { return null; }";

async fn read_pathname() -> Option<String> {
    let value = document::eval(READ_PATHNAME).await.ok()?;
    let value: serde_json::Value = serde_json::from_str(value.as_str()?).ok()?;
    value.as_str().map(str::to_string)
}

async fn read_origin() -> Option<String> {
    let value = document::eval(READ_ORIGIN).await.ok()?;
    let value: serde_json::Value = serde_json::from_str(value.as_str()?).ok()?;
    value.as_str().map(str::to_string)
}

/// What the path at boot asks this app to open, or `None` for a path it does not own.
///
/// Exactly two shapes, each exactly two segments — `/builds/<id>` and `/author/<handle>`. A
/// longer path on either prefix is nobody's page, and this reader must not guess: the fallthrough
/// is the planner, which is what a URL nobody claimed should open.
///
/// The handle is normalised on the way in ([`super::profile::normalize_handle`]) because a URL
/// is typed by hand and pasted by people — `/author/@TigerEyes` is a plausible thing to arrive
/// on, and the RPC matches a CITEXT column against the bare lowercase value.
fn boot_target(pathname: &str) -> Option<BrowserTarget> {
    let segments: Vec<&str> = pathname
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    match segments.as_slice() {
        ["builds", id] if !id.is_empty() => Some(BrowserTarget::Build(id.to_string())),
        ["author", handle] => super::profile::normalize_handle(handle).map(BrowserTarget::Author),
        _ => None,
    }
}
