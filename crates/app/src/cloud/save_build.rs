//! RB4e's surface: saving a build to the shared repository, and the one-click short link.
//!
//! Two acts, and they differ in more than how many clicks they take.
//!
//! * **Save** opens a surface, because a shared build is a post: it has a name, a description,
//!   tags and — decisively — a visibility, and none of those have a right answer the app could
//!   pick. It creates a row and keeps the owner token the server mints.
//! * **Copy short link** opens nothing. It is the act the forum export cannot offer: a URL, on
//!   the clipboard, in one click. It creates an `unlisted` row the first time and UPDATES that
//!   same row every time after, which is what the fingerprint cache in [`super::owner_store`] is
//!   for — an unchanged build re-copies with no network call at all.
//!
//! # What the build travels as
//!
//! [`coh_data::skif::encode`], which writes **v5**. That is this app's own format and the only
//! one it can write losslessly: v4 has no field for `origin`, `stances` or `powerState`, spells
//! modes as an array that cannot express an explicitly-off one, and packs booster and relative
//! level onto a single ambiguous `boost`. Writing v4 to stay readable by the beta would quietly
//! diminish the user's own build on a round trip, which is a worse bargain than it looks.
//!
//! The live beta reads v1–v4 and, until 2026-09-16, treated *everything else* as v1 — so a v5 row
//! rendered there as a mangled build rather than an error. That was closed at the other end
//! rather than worked around here: `buildStore.ts`'s import now refuses a version it does not
//! know, by name, instead of guessing. A v5 row opened in the beta during the overlap is a stated
//! refusal, and the fix arrives when RB4i cuts the origin over.
//!
//! # Where the preview comes from
//!
//! The same [`crate::export_image::render_preview_png`] RB4d built, so a card posted at share
//! time and a card backfilled later are the same card. Best-effort in the beta's sense — a
//! capture failure omits the field and the server keeps whatever image the row had — but never
//! mute: `share-build` shape-checks nothing about the image beyond its size, so a refusal here
//! would be the upload failing, and that is worth the console line.

use super::account::Account;
use super::owner_store::{self, QuickShare};
use super::shared_builds::{
    self, BuildVisibility, RateLimitAction, ShareError, ShareInput, Shared,
    CURRENT_PREVIEW_TEMPLATE_VERSION,
};
use super::tag_vocab;
use crate::build_session::BuildSession;
use crate::modal::{Modal, ModalSize};
use crate::panels::stats::BuildTotals;
use crate::shell::Db;
use coh_data::character::CharacterState;
use dioxus::prelude::*;

/// Raised by the main menu's save row; owned by the shell.
#[derive(Clone, Copy)]
pub struct SaveBuildOpen(pub Signal<bool>);

// ============================================================
// The pure half.
// ============================================================

/// The identity columns `share-build` files a row under.
///
/// These are the SEARCH columns — what the browser's archetype and powerset filters match on,
/// and what a card draws before anyone opens it. They are resolved from the loaded database
/// rather than read back out of the encoded build, because a build file carries ids and the
/// display name of an id is whatever the dataset calls it today.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BuildIdentity {
    pub archetype: String,
    pub archetype_name: String,
    pub primary_set: String,
    pub primary_name: String,
    pub secondary_set: String,
    pub secondary_name: String,
    pub level: u8,
}

/// Read a build's identity columns against the loaded database.
///
/// Every field falls back to the empty string rather than to a placeholder, because the server
/// validates three of them (`archetype`, `primary_set`, `secondary_set` must be non-empty) and an
/// invented value would turn "this build has no archetype yet" into a row filed under a
/// fabricated one. An empty string is refused loudly at the function; "Unknown" would not be.
pub fn identity_of(build: &CharacterState, database: &Db) -> BuildIdentity {
    BuildIdentity {
        archetype: build.archetype.id.clone().unwrap_or_default(),
        archetype_name: crate::naming::archetype_name(build, database).unwrap_or_default(),
        primary_set: build.primary.id.clone().unwrap_or_default(),
        primary_name: crate::naming::powerset_name(&build.primary, database).unwrap_or_default(),
        secondary_set: build.secondary.id.clone().unwrap_or_default(),
        secondary_name: crate::naming::powerset_name(&build.secondary, database)
            .unwrap_or_default(),
        level: build.level,
    }
}

/// Whether a build carries what the function requires before any call is worth making.
///
/// `share-build/index.ts:139` refuses a build with no archetype, primary or secondary with a 400.
/// Checking here is not enforcement of a server rule — it is refusing to spend a rate-limit slot
/// on a request whose answer is already known, and being able to say which field is missing
/// rather than relaying "Build must have an archetype, primary, and secondary powerset".
pub fn missing_identity(identity: &BuildIdentity) -> Option<&'static str> {
    if identity.archetype.is_empty() {
        return Some("an archetype");
    }
    if identity.primary_set.is_empty() {
        return Some("a primary powerset");
    }
    if identity.secondary_set.is_empty() {
        return Some("a secondary powerset");
    }
    None
}

/// Split the tag field into the list the server stores.
///
/// Comma-separated, trimmed, empties dropped, deduplicated, and cut to ten — the server slices to
/// ten itself (`share-build/index.ts:235`), so cutting here only means the user sees what will
/// actually be kept rather than watching three tags vanish server-side.
///
/// A typed tag that IS a curated one is folded onto the curated spelling ([`tag_vocab::canonical`]),
/// so `perma hasten` filters alongside the chip. Anything else is kept exactly as typed — the
/// free-text field is there precisely for the tags nobody anticipated.
pub fn parse_tags(raw: &str) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    for tag in raw.split(',') {
        let tag = tag.trim();
        if tag.is_empty() || tags.iter().any(|kept| kept.eq_ignore_ascii_case(tag)) {
            continue;
        }
        tags.push(match tag_vocab::canonical(tag) {
            Some(curated) => curated.to_string(),
            None => tag.to_string(),
        });
        if tags.len() == tag_vocab::MAX_TAGS {
            break;
        }
    }
    tags
}

/// The tag list a save actually sends: the chips the author ticked, then whatever they typed
/// beside them.
///
/// Chips first because they are the ones the browser can filter on, and the cut at
/// [`tag_vocab::MAX_TAGS`] has to fall somewhere. Deduplicated across BOTH halves, since typing
/// `budget` into the free-text box while `Budget` is ticked is the obvious way to end up paying
/// twice for one tag.
pub fn combine_tags(chosen: &[String], raw: &str) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    for tag in chosen
        .iter()
        .map(String::as_str)
        .chain(parse_tags(raw).iter().map(String::as_str))
    {
        if tag.is_empty() || tags.iter().any(|kept| kept.eq_ignore_ascii_case(tag)) {
            continue;
        }
        tags.push(tag.to_string());
        if tags.len() == tag_vocab::MAX_TAGS {
            break;
        }
    }
    tags
}

/// Encode the working build the way it travels, and hand back the parsed envelope.
///
/// Parsed rather than passed as text because `build_json` is a JSON *value* in the payload, not a
/// string containing JSON — a stringified build would store as a quoted blob and every reader of
/// the column, this app's included, would fail to find `version` in it.
pub fn build_envelope(build: &CharacterState, database: &Db) -> Result<serde_json::Value, String> {
    let text = coh_data::skif::encode(build, database).map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

/// Assemble the save, given everything already resolved.
///
/// Split from the click handler so the shape of a save is gradeable without a runtime: which
/// fields carry through from the form, and which `None` means "let the server decide".
#[allow(clippy::too_many_arguments)]
pub fn share_input(
    identity: &BuildIdentity,
    build_json: serde_json::Value,
    name: &str,
    description: &str,
    author_name: &str,
    server: &str,
    tags: Vec<String>,
    visibility: Option<BuildVisibility>,
    existing_id: Option<String>,
    preview_image_base64: Option<String>,
) -> ShareInput {
    ShareInput {
        name: name.trim().to_string(),
        description: description.trim().to_string(),
        archetype: identity.archetype.clone(),
        archetype_name: identity.archetype_name.clone(),
        primary_set: identity.primary_set.clone(),
        primary_name: identity.primary_name.clone(),
        secondary_set: identity.secondary_set.clone(),
        secondary_name: identity.secondary_name.clone(),
        level: identity.level,
        author_name: author_name.to_string(),
        server: server.to_string(),
        tags,
        build_json,
        visibility,
        existing_id,
        preview_image_base64,
    }
}

/// The metadata edit the beta calls `updateBuildMetadata` — a re-save of an existing row with new
/// name/description/tags and the SAME build.
///
/// It is `shareBuild` with an `existingId` and no `visibility`, which is why it has no edge
/// function of its own and why a grep for one would not find it. The omitted visibility is the
/// whole point: editing a description must not republish a build the user made private.
///
/// **No surface calls this yet, and the allow below is that fact written down.** RB4e ported the
/// call; the form that would send it is the beta's `?edit=true` deep link, which no row has
/// claimed. Scoped to this item on purpose: RB4g deleted the module-wide allow this used to hide
/// under, and found two unrelated items under it that nobody had meant to keep.
///
/// **Kept in the 2026-09-26 sweep that deleted five other uncalled items, and the reason for the
/// difference is that this one is a feature waiting for a screen rather than a leftover.** What it
/// does -- re-save an existing row with a new name, description and tags, and NO visibility -- is
/// wanted; nothing else in this repo does it. The line that stood here claimed "the test beneath
/// it" is what would go red if the omitted visibility were ever sent. There is no test beneath it,
/// here: that sentence came from `coh-sidekick-1.0`, where one test calls this function and
/// nothing else does. So the omitted `None` below is currently unguarded, and editing a
/// description must still never republish a build the user made private -- read the argument
/// above before touching it.
#[allow(dead_code)]
pub fn metadata_patch(
    existing: &shared_builds::SharedBuild,
    identity: &BuildIdentity,
    name: &str,
    description: &str,
    tags: Vec<String>,
) -> ShareInput {
    share_input(
        identity,
        existing.build_json.clone(),
        name,
        description,
        &existing.summary.author_name,
        &existing.summary.server,
        tags,
        // Never sent — see the doc above.
        None,
        Some(existing.summary.id.clone()),
        None,
    )
}

// ============================================================
// The two acts.
// ============================================================

/// Whether the viewer may edit this build — the beta's `isOwnedBuild`.
///
/// Two independent claims, and either is enough, because they are the two the SERVER accepts:
/// an owner token this browser holds for that exact row, or a session whose account id is the
/// row's `user_id`. A token-owned build stays editable after sign-out, which is the point of the
/// token; an account-owned one stays editable from any browser, which is the point of claiming.
///
/// `user_id.is_some()` is checked rather than implied. Without it, a signed-out viewer looking at
/// an anonymous build compares `None == None` and every visitor owns every anonymous row.
pub fn is_owned(
    build: &shared_builds::SharedBuildSummary,
    owner_token: Option<&str>,
    user_id: Option<&str>,
) -> bool {
    if owner_token.is_some() {
        return true;
    }
    user_id.is_some() && build.user_id.as_deref() == user_id
}

/// What a save produced, for the surface to draw.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveOutcome {
    pub id: String,
    pub updated: bool,
    /// The bucket the server metered this save against, and what is left in it.
    pub remaining: Option<(RateLimitAction, u32)>,
}

/// Render the build's social card, or `None` if it could not be drawn.
///
/// Best-effort by contract: the field is then simply absent from the payload, and `share-build`
/// leaves whatever image the row already had rather than nulling it (`share-build/index.ts:295`).
fn capture_preview(
    build: &CharacterState,
    database: &Db,
    totals: &coh_math::CalculatedTotals,
) -> Option<String> {
    match crate::export_image::render_preview_png(build, database, totals) {
        Ok(bytes) => Some(crate::clipboard::base64(&bytes)),
        Err(reason) => {
            // Not mute: the card is what every unfurl of this link will show, and a save that
            // silently shipped without one is the state RB4d's backfill exists to repair.
            let msg = format!("preview capture failed, sharing without one: {reason}");
            // JSON string literal rather than `{:?}` — F29, and the same rule as the sibling
            // site in `browser.rs`.
            if let Ok(literal) = serde_json::to_string(&msg) {
                let _ = document::eval(&format!("console.error({literal});"));
            }
            None
        }
    }
}

/// Keep the owner token a create minted. The only copy that will ever exist — the row stores a
/// SHA-256 of it, and the server never sends it twice.
async fn keep_owner_token(shared: &Shared) {
    if let Some(token) = &shared.owner_token {
        owner_store::remember_owner_token(&shared.id, token).await;
    }
}

/// Save the working build, creating a row or updating one.
pub async fn save(
    account: &Account,
    input: ShareInput,
    signed_in: bool,
) -> Result<SaveOutcome, ShareError> {
    let owner_token = match &input.existing_id {
        Some(id) => owner_store::owner_token(id).await,
        None => None,
    };
    let cloud = account.prepared().await?;
    let shared =
        shared_builds::share_build(&cloud, &input, owner_token.as_deref(), signed_in).await?;
    keep_owner_token(&shared).await;
    Ok(SaveOutcome {
        id: shared.id,
        updated: shared.updated,
        remaining: shared
            .rate_limit
            .map(|limit| (limit.action, limit.remaining)),
    })
}

/// The one-click unlisted short link.
///
/// Mirrors the beta's `quickShareBuild` including the order of its fallbacks, which is the part
/// worth reading twice:
///
/// 1. An unchanged build whose cached row was rendered under the current card template returns
///    the cached URL with **no network call**. That is what makes the button instant on a second
///    press, and what stops a re-copy burning a rate-limit slot.
/// 2. Otherwise the cached row is UPDATED in place — with `visibility` omitted, so a build the
///    user has since made public through the visibility toggle is not silently reverted to
///    unlisted by a re-copy.
/// 3. If that update fails, a fresh row is created — the cached row may have been deleted, or
///    ownership lost. **Except on a rate limit**: retrying as a create would spend a second slot
///    to fail the same way, so that one propagates instead of falling through.
pub async fn quick_share(
    account: &Account,
    build: &CharacterState,
    database: &Db,
    totals: &coh_math::CalculatedTotals,
    author_name: &str,
    signed_in: bool,
) -> Result<String, ShareError> {
    if !signed_in {
        return Err(ShareError::SignInRequired);
    }
    let identity = identity_of(build, database);
    if let Some(missing) = missing_identity(&identity) {
        return Err(ShareError::Cloud(super::CloudError::Refused {
            status: 0,
            message: format!("this build needs {missing} before it can be shared"),
            body: String::new(),
        }));
    }
    let envelope = build_envelope(build, database).map_err(|reason| {
        ShareError::Cloud(super::CloudError::Refused {
            status: 0,
            message: format!("this build could not be encoded: {reason}"),
            body: String::new(),
        })
    })?;
    let fingerprint = shared_builds::fingerprint_build(&envelope);

    let cached = owner_store::quick_share().await;
    if let Some(cache) = &cached {
        if owner_store::quick_share_hits(cache, &fingerprint, CURRENT_PREVIEW_TEMPLATE_VERSION) {
            return Ok(cache.share_id.clone());
        }
    }

    let name = shared_builds::quick_share_name(
        &build.name,
        &identity.archetype_name,
        &identity.primary_name,
        &identity.secondary_name,
    );
    let preview = capture_preview(build, database, totals);

    // The update attempt, when there is a row to update.
    if let Some(cache) = &cached {
        let input = share_input(
            &identity,
            envelope.clone(),
            &name,
            "",
            author_name,
            build.dataset.as_str(),
            Vec::new(),
            // Omitted on purpose — see step 2 above.
            None,
            Some(cache.share_id.clone()),
            preview.clone(),
        );
        match save(account, input, signed_in).await {
            Ok(outcome) => {
                owner_store::remember_quick_share(&QuickShare {
                    share_id: outcome.id.clone(),
                    fingerprint,
                    template_version: Some(CURRENT_PREVIEW_TEMPLATE_VERSION),
                });
                return Ok(outcome.id);
            }
            // A second slot would buy the same refusal.
            Err(ShareError::RateLimited(limited)) => return Err(ShareError::RateLimited(limited)),
            Err(_) => (),
        }
    }

    let input = share_input(
        &identity,
        envelope,
        &name,
        "",
        author_name,
        build.dataset.as_str(),
        Vec::new(),
        Some(BuildVisibility::Unlisted),
        None,
        preview,
    );
    let outcome = save(account, input, signed_in).await?;
    owner_store::remember_quick_share(&QuickShare {
        share_id: outcome.id.clone(),
        fingerprint,
        template_version: Some(CURRENT_PREVIEW_TEMPLATE_VERSION),
    });
    Ok(outcome.id)
}

// ============================================================
// The surface.
// ============================================================

/// Mounted by the shell above both layout roots. Renders nothing while closed.
#[component]
pub fn SaveBuildHost(database: Option<Db>) -> Element {
    let mut open = use_context::<SaveBuildOpen>().0;
    if !open() {
        return rsx! {};
    }
    rsx! {
        Modal {
            title: "Save to the cloud".to_string(),
            size: ModalSize::Md,
            on_close: move |_| open.set(false),
            SaveBuildBody { database }
        }
    }
}

/// One chip as the picker draws it. `blocked` is not `!on` — it is the separate fact that the
/// ten-tag budget is spent, which is why a chip can be off and still untickable.
struct Chip {
    tag: &'static str,
    on: bool,
    blocked: bool,
}

/// A section of chips under its group heading.
struct ChipGroup {
    name: &'static str,
    chips: Vec<Chip>,
}

#[component]
fn SaveBuildBody(database: Option<Db>) -> Element {
    let account = use_context::<Account>();
    let session = use_context::<BuildSession>();
    let totals = use_context::<BuildTotals>().0;
    let mut open = use_context::<SaveBuildOpen>().0;

    let user = account.user();
    let signed_in = user.read().is_some();

    let mut name = use_signal(|| session.build.peek().name.clone());
    let mut description = use_signal(String::new);
    let mut tags = use_signal(String::new);
    // The ticked chips, kept apart from the free-text draft so a chip cannot be half-typed and
    // the counter can be exact before anything is parsed.
    let mut chosen = use_signal(Vec::<String>::new);
    // Anonymous saves are forced public by the server — there is no persistent identity for an
    // anonymous user to reclaim a private link with — so the default only offers the choice it
    // can honour.
    let mut visibility = use_signal(|| match signed_in {
        true => BuildVisibility::Unlisted,
        false => BuildVisibility::Public,
    });
    // The last choice made here, restored once the async read lands — unless the user has
    // already clicked one, which must not be overwritten by a read that arrived late.
    let mut picked_visibility = use_signal(|| false);
    use_hook(move || {
        if signed_in {
            spawn(async move {
                if let Some(remembered) = owner_store::save_visibility().await {
                    if !picked_visibility() {
                        visibility.set(remembered);
                    }
                }
            });
        }
    });
    let mut saving = use_signal(|| false);
    let mut outcome = use_signal(|| None::<SaveOutcome>);
    let mut refusal = use_signal(|| None::<String>);
    let mut copied = use_signal(|| false);

    let Some(database) = database else {
        return rsx! {
            div { class: "load-state", "Loading the dataset…" }
        };
    };

    let identity = identity_of(&session.build.read(), &database);
    let blocked = missing_identity(&identity);

    // What the save would actually send, computed here so the counter and the chips agree with
    // it rather than each counting their own half. A tag TYPED into the free-text box lights its
    // chip through this, which is the whole reason the combined list is what gets asked.
    let picked = combine_tags(&chosen.read(), &tags.read());
    let used = picked.len();
    let at_cap = used >= tag_vocab::MAX_TAGS;
    let chip_rows: Vec<ChipGroup> = tag_vocab::GROUPS
        .iter()
        .map(|group| ChipGroup {
            name: group.name,
            chips: group
                .tags
                .iter()
                .copied()
                .map(|tag| {
                    let on = picked.iter().any(|kept| kept.eq_ignore_ascii_case(tag));
                    Chip {
                        tag,
                        on,
                        blocked: at_cap && !on,
                    }
                })
                .collect(),
        })
        .collect();
    let mut toggle = move |tag: &'static str| {
        let mut list = chosen.write();
        match list.iter().position(|kept| kept.eq_ignore_ascii_case(tag)) {
            Some(at) => {
                list.remove(at);
            }
            None => list.push(tag.to_string()),
        }
    };

    let hint = shared_builds::rate_limit_hint(RateLimitAction::for_requested(Some(visibility())));

    let save_now = {
        let database = database.clone();
        let account = account.clone();
        let identity = identity.clone();
        move |_| {
            let database = database.clone();
            let account = account.clone();
            let identity = identity.clone();
            async move {
                if saving() {
                    return;
                }
                saving.set(true);
                refusal.set(None);
                let build = session.build.peek().clone();
                let computed = totals.peek().clone();
                let envelope = match build_envelope(&build, &database) {
                    Ok(envelope) => envelope,
                    Err(reason) => {
                        refusal.set(Some(format!("This build could not be encoded: {reason}")));
                        saving.set(false);
                        return;
                    }
                };
                let preview = capture_preview(&build, &database, &computed);
                let author = user
                    .peek()
                    .as_ref()
                    .and_then(|u| u.display_name.clone())
                    .unwrap_or_default();
                let input = share_input(
                    &identity,
                    envelope,
                    &name.peek().clone(),
                    &description.peek().clone(),
                    &author,
                    build.dataset.as_str(),
                    combine_tags(&chosen.peek().clone(), &tags.peek().clone()),
                    Some(visibility()),
                    None,
                    preview,
                );
                match save(&account, input, signed_in).await {
                    Ok(saved) => outcome.set(Some(saved)),
                    Err(error) => refusal.set(Some(error.to_string())),
                }
                saving.set(false);
            }
        }
    };

    // The saved state replaces the form: the act is done, and what the user wants now is the
    // link. Re-saving would create a second row, so the surface does not offer it.
    if let Some(saved) = outcome() {
        let id = saved.id.clone();
        return rsx! {
            div { class: "sb-save",
                p { class: "sb-save__done",
                    if saved.updated { "Your build was updated." } else { "Your build is saved." }
                }
                if let Some((action, remaining)) = saved.remaining {
                    p { class: "sb-save__quota",
                        "{remaining} more {action.label()} this hour."
                    }
                }
                div { class: "sb-detail__actions",
                    button {
                        class: "seg is-primary",
                        r#type: "button",
                        onclick: {
                            let id = id.clone();
                            move |_| {
                                let id = id.clone();
                                async move {
                                    match super::browser::copy_url(&id).await {
                                        Ok(()) => copied.set(true),
                                        Err(reason) => refusal.set(Some(reason)),
                                    }
                                }
                            }
                        },
                        if copied() { "Copied!" } else { "Copy link" }
                    }
                    button {
                        class: "seg",
                        r#type: "button",
                        onclick: move |_| open.set(false),
                        "Done"
                    }
                }
                if let Some(reason) = refusal() {
                    p { class: "sb-save__error", "{reason}" }
                }
            }
        };
    }

    rsx! {
        div { class: "sb-save",
            if let Some(missing) = blocked {
                p { class: "sb-save__error",
                    "This build needs {missing} before it can be shared."
                }
            }
            label { class: "field-label", "Name" }
            input {
                class: "sb-save__input",
                value: "{name}",
                oninput: move |e| name.set(e.value()),
                placeholder: "What should this build be called?",
            }
            label { class: "field-label", "Description" }
            textarea {
                class: "sb-save__input sb-save__input--area",
                value: "{description}",
                oninput: move |e| description.set(e.value()),
                placeholder: "What is it for? How is it played?",
            }
            label { class: "field-label", "Tags" }
            div { class: "sb-tagpick",
                for group in chip_rows {
                    div { key: "{group.name}", class: "sb-tagpick__group",
                        span { class: "sb-tagpick__name", "{group.name}" }
                        div { class: "sb-tagpick__chips",
                            for chip in group.chips {
                                button {
                                    key: "{chip.tag}",
                                    class: if chip.on { "sb-chip is-on" } else { "sb-chip" },
                                    r#type: "button",
                                    // A chip that silently did nothing at the cap would read as
                                    // broken, so the refusal is drawn and says why.
                                    disabled: chip.blocked,
                                    title: if chip.blocked {
                                        "Ten tags is all the server keeps — untick one first"
                                    } else { "" },
                                    onclick: move |_| toggle(chip.tag),
                                    "{chip.tag}"
                                }
                            }
                        }
                    }
                }
            }
            input {
                class: "sb-save__input",
                value: "{tags}",
                oninput: move |e| tags.set(e.value()),
                placeholder: "Anything else you want, separated with commas.",
            }
            p { class: "sb-save__hint",
                "{used} of {tag_vocab::MAX_TAGS} tags."
                if at_cap { " Untick one to add another." }
            }

            label { class: "field-label", "Who can see it" }
            div { class: "sb-save__choice",
                for (option, label, why) in [
                    (BuildVisibility::Public, "Public", "Listed in the build browser and readable by anyone."),
                    (BuildVisibility::Unlisted, "Unlisted", "Readable by anyone with the link; never listed."),
                    (BuildVisibility::Private, "Private", "Only you, on this account."),
                ] {
                    button {
                        key: "{label}",
                        class: if visibility() == option { "seg active" } else { "seg" },
                        r#type: "button",
                        // The server forces an anonymous save public, so the choice is shown
                        // disabled with its reason rather than offered and then overridden.
                        disabled: !signed_in && option != BuildVisibility::Public,
                        title: if !signed_in && option != BuildVisibility::Public {
                            "Sign in to save a build that is not public"
                        } else { why },
                        onclick: move |_| {
                            visibility.set(option);
                            picked_visibility.set(true);
                            owner_store::remember_save_visibility(option);
                        },
                        "{label}"
                    }
                }
            }
            p { class: "sb-save__hint", "{hint}" }

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
                    disabled: saving() || blocked.is_some(),
                    onclick: save_now,
                    if saving() { "Saving…" } else { "Save" }
                }
            }
        }
    }
}
