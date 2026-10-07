//! The shared-builds service — RB4d, the anon-reachable half of the cloud surface.
//!
//! Four calls, ported from the beta's `CoH-Sidekick/src/services/sharedBuilds.ts`:
//!
//! * [`search`] — the browse query, PostgREST against the `shared_builds_with_author` view with
//!   `visibility = 'public'`. This one *is* a table read, and that is not a contradiction of the
//!   row's ban on PostgREST shortcuts: the RLS policy grants anon SELECT on exactly the public
//!   rows, so a search over that grant can never enumerate an unlisted build. The single-row
//!   read is the one with no anon policy, and that is why it goes through the function.
//! * [`get_shared_build`] — `get-build`, the edge function that is the ONLY path a non-owner
//!   reads an unlisted build through. `get-build/index.ts:6-8` states why it exists: a looser
//!   anon read policy on `shared_builds` would let anyone holding the public anon key bulk-list
//!   every row. The function accepts one id and nothing else, so it cannot enumerate; the
//!   client mirrors that by never asking it anything but one exact id. There is no PostgREST
//!   shortcut here, on purpose.
//! * [`increment_views`] — the `increment_views` RPC. The function is `SECURITY DEFINER`, which
//!   is how an anon visitor may bump a public row's counter. Deliberately fire-and-forget at
//!   the call sites: the beta discards the promise (`sharedBuilds.ts:541`), and a failed view
//!   count is not worth a modal.
//! * [`submit_preview_backfill`] — `backfill-preview`, the anonymous-write half of the preview
//!   story: any visitor's browser may fill in a missing or stale social image, bounded server-
//!   side by a version gate and a shape check. Returns whether the server accepted the image,
//!   and mirrors the beta's never-throws contract at the call site (the caller reports the
//!   outcome to its parent either way). The *capture* half — rendering the 1200×880 card the
//!   server's shape check demands — is not built here; see the RB4d closure note.
//!
//! Every call below takes `&Cloud` rather than reaching for the manager itself, and every
//! call site in the UI goes through `account.prepared().await?` — RB4c's note says why it is
//! not `manager.client()`: the refresh has to be serialised under the lock, and the request is
//! then free to run. A service function that asked for a client directly would be a second
//! door around the one rule the session owns.
//!
//! The four above are RB4d's anon half and the file has since outgrown them: RB4e added the
//! owner acts ([`share_build`], [`update_build_visibility`], [`delete_build`], [`claim_builds`]),
//! and RB4k added [`my_builds`] — the vault read, the one call here scoped to an account rather
//! than to a build or to the public rows.

use super::{Cloud, CloudError};
use coh_data::DatasetId;
use serde::{Deserialize, Serialize};

/// The current social-preview template. Mirrors `CURRENT_PREVIEW_TEMPLATE_VERSION` in the
/// beta's `BuildPreviewCard` and in every edge function that writes the column (`backfill-
/// preview/index.ts:26`) — hand-kept duplicates, all three, and all three must be bumped
/// together whenever the card's look changes. The server is the gate that actually refuses a
/// write; this copy only decides whether the client tries.
pub const CURRENT_PREVIEW_TEMPLATE_VERSION: u32 = 6;

/// Default page size for [`search`], matching the beta's `DEFAULT_PAGE_SIZE`.
pub const DEFAULT_PAGE_SIZE: usize = 20;

/// The visibility of a shared build row. `'private'`: owner only. `'unlisted'`: readable by
/// anyone with the exact share link, never surfaced in search. `'public'`: readable by link
/// AND listed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BuildVisibility {
    Public,
    Unlisted,
    Private,
}

/// The columns a LIST read asks the view for — the whole of what a card draws, named one by
/// one rather than taken as `*`.
///
/// **`*` is what F19 was.** The view is `SELECT b.*, …`, so every list read pulled `build_json`
/// — the entire build document — for every row on the page, to draw a card that reads none of
/// it. Measured against the deployed project on 2026-09-18: a default page of twenty public
/// rows is 381,633 bytes under `*` and 13,377 under this list, so 96% of what a browse
/// downloaded was a document nothing on that screen could show. The cost is not only
/// bandwidth. One hostile row's document is decoded by every visitor's client on a page they
/// did not choose it from, which is the amplifier that made a Medium of a column nobody reads:
/// newest-first puts a fresh row at position one, and a document that fails to decode fails
/// the page rather than the row.
///
/// **`owner_token_hash` is the other reason to name columns.** `b.*` carries it, so the wide
/// read handed every visitor the sha256 of the ownership credential for every public row —
/// 1,575 of the 2,665 public rows carry one. It is not invertible (the token is a
/// `crypto.randomUUID`), which is what keeps it small, but nothing in this client has ever had
/// a use for the column and a projection that names its columns cannot carry it by accident.
/// **This does not close that exposure and must not be read as closing it**: the grant is the
/// server's, so anyone may still ask the view for the column directly, and `get-build` returns
/// it by id. That is F73, and it is fixed where it is granted, not here.
///
/// Spelled as one flat string because that is what PostgREST's `select` takes; the guard that
/// keeps it honest is in the tests, and it reads this constant rather than a copy of it.
pub const LIST_COLUMNS: &str = "id,name,description,archetype,archetype_name,primary_set,\
primary_name,secondary_set,secondary_name,level,author_name,server,tags,created_at,updated_at,\
views,user_id,visibility,preview_image_path,preview_template_version,author_handle,\
author_display_name,author_avatar_url";

/// A row of the `shared_builds_with_author` view as a LIST read returns it — one shared build
/// with its author's profile columns joined in, and without the build document itself. The
/// rebuild's domain model of a shared build is this wire shape, not a projection of it: the
/// fields the UI does not draw yet (author handle, avatar) belong to RB4f and are carried now
/// rather than dropped on the way through, which is the same instinct the parser mandate
/// applies one layer out.
///
/// **This is the type the three list surfaces hold** — the browse grid, the vault and the
/// favourites list — and it is a type that structurally cannot carry a build document, which
/// is a stronger statement than a query that happens not to ask for one. The document arrives
/// only through [`get_shared_build`], one build at a time, on the screen that loads it; there
/// it comes back as a [`SharedBuild`], which is this row plus that document.
///
/// **It deliberately does NOT `deny_unknown_fields`, and the reason is not the one it looks
/// like.** Denying them is the stricter shape and the first thing tried here: it turns a read
/// that went back to `select=*` into a loud refusal, which is the Rule 1 reading. The
/// suspicion was that it would break the detail view, because this type is also the flattened
/// half of [`SharedBuild`] and `get-build` answers `select('*')` under the service role —
/// twenty-five keys to an anonymous caller on 2026-09-18, `owner_token_hash` among them and
/// non-null. **That suspicion is wrong, and the mutation is what said so**: with the denial in
/// place a detail row carrying those extras still decodes, because serde documents
/// `deny_unknown_fields` as unsupported in combination with `flatten` and what it does there
/// is ignore it.
///
/// Which is the actual objection. The strict shape would buy a refusal on the list read while
/// the detail read kept whatever `flatten` happens to do this release, and a property that
/// holds by accident is one a patch version can take away — silently, on the path that reads a
/// stranger's document. So the type tolerates columns it did not ask for, that tolerance is
/// asserted rather than assumed
/// ([`the_list_type_ignores_columns_it_did_not_ask_for`]), and the projection is guarded where
/// it is actually spelled: the tests on [`LIST_COLUMNS`].
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct SharedBuildSummary {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub archetype: String,
    pub archetype_name: String,
    pub primary_set: String,
    pub primary_name: String,
    pub secondary_set: String,
    pub secondary_name: String,
    pub level: u32,
    #[serde(default)]
    pub author_name: String,
    #[serde(default)]
    pub server: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub views: u64,
    /// The owner's account id; `None` for an anonymous share (the beta's `user_id ?? null`).
    #[serde(default)]
    pub user_id: Option<String>,
    pub visibility: BuildVisibility,
    /// Object path in the `build-previews` bucket, or `None` if never rendered.
    #[serde(default)]
    pub preview_image_path: Option<String>,
    /// The template version the stored image was rendered under; a value older than
    /// [`CURRENT_PREVIEW_TEMPLATE_VERSION`] means the image is stale, not missing.
    #[serde(default)]
    pub preview_template_version: Option<u32>,
    #[serde(default)]
    pub author_handle: Option<String>,
    #[serde(default)]
    pub author_display_name: Option<String>,
    #[serde(default)]
    pub author_avatar_url: Option<String>,
}

/// One shared build WITH its document — the shape [`get_shared_build`] answers with, and the
/// only shape in this module that carries a build.
///
/// A detail row is a list row plus the payload, and it is spelled that way rather than as a
/// second field list so the two cannot drift: add a column to the card and both reads get it,
/// because there is only one place to add it. `flatten` is what makes the wire agree — the
/// edge function answers with one flat object, and the split is this client's, not the
/// server's.
///
/// **The separation is the point of F19's fix.** A build document is a document from a
/// stranger: unbounded in size, unbounded in nesting until a parser refuses it, and refusing
/// it fails whatever read carried it. Confining it to this type confines that to the one
/// screen that asked for one build by id.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct SharedBuild {
    #[serde(flatten)]
    pub summary: SharedBuildSummary,
    /// The beta's `BuildExport` envelope — the whole build, as JSON. Kept whole rather than
    /// projected: "Load into Planner" hands it to the import reader verbatim.
    pub build_json: serde_json::Value,
}

/// What search is being asked to filter and sort by. Mirrors the beta's `SearchFilters`
/// (`types/shared.ts`), minus the fields nothing anon can act on yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchFilters {
    pub archetype: Option<String>,
    pub primary_set: Option<String>,
    pub secondary_set: Option<String>,
    /// Free-text query over name/description/author. Empty means no text filter.
    pub query: String,
    /// The canonical author filter (a specific account). `author_name` only filters when this
    /// is absent — for anonymous build cards there is no id to match.
    pub author_id: Option<String>,
    /// Display filter when no `author_id` is available.
    pub author_name: Option<String>,
    /// Tags a row must carry ALL of. Empty means no tag filter. Conjunctive because that is what
    /// the picker's chips read as — ticking `AFK farming` and `Budget` asks for builds that are
    /// both, not for the union, which would be a longer list than no filter at all on one of them.
    pub tags: Vec<String>,
    pub sort_by: SortBy,
    pub page: usize,
    pub page_size: usize,
}

impl Default for SearchFilters {
    fn default() -> Self {
        Self {
            archetype: None,
            primary_set: None,
            secondary_set: None,
            query: String::new(),
            author_id: None,
            author_name: None,
            tags: Vec::new(),
            sort_by: SortBy::Newest,
            page: 1,
            page_size: DEFAULT_PAGE_SIZE,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortBy {
    Newest,
    Views,
}

/// A page of search results, with what the page control needs to know it is not alone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchResult {
    pub builds: Vec<SharedBuildSummary>,
    pub total: u64,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
}

/// The `tags=cs.{…}` value for an array-containment filter.
///
/// Every member is double-quoted and escaped. What that buys, measured against the deployed
/// project on 2026-09-17 rather than reasoned about:
///
///   * **A comma separates members, so a tag containing one would split.** This is the live
///     reason to quote. The curated thirty contain no comma, but the save dialog keeps a
///     FREE-TEXT field beside the chips, so a user-typed `solo, no incarnates` reaches here.
///   * **Leading and trailing whitespace is trimmed on an unquoted member** and kept on a quoted
///     one — `cs.{ High End}` matched 2 rows, `cs.{" High End"}` matched 0.
///   * **A quote or backslash would end the member list early**, which is why both are escaped.
///
/// **What it does NOT buy: surviving an interior space.** This comment used to say PostgREST
/// splits an unquoted brace list on whitespace, so `cs.{Perma Hasten}` asked for `Perma` and
/// `Hasten` and matched nothing. That is false, and it was disproved on the wire: `cs.{High End}`
/// and `cs.{"High End"}` returned the SAME two rows, while `cs.{High}` and `cs.{End}` each
/// returned none — so no split occurred. A Postgres array literal separates on commas only.
///
/// The correction is recorded because the claim was load-bearing in the wrong direction: it made
/// quoting look mandatory for the curated vocabulary, when it is actually mandatory for the
/// free-text field nobody cited.
fn tag_containment(tags: &[String]) -> String {
    let members: Vec<String> = tags
        .iter()
        .map(|tag| format!("\"{}\"", tag.replace('\\', "\\\\").replace('"', "\\\"")))
        .collect();
    format!("cs.{{{}}}", members.join(","))
}

/// Build the PostgREST wire for a [`SearchFilters`]: the `eq.` filters, the websearch text
/// query, the order spec and the item slice. Kept apart from the call so the mapping is
/// gradeable without a runtime or a server — the filters are the whole of what search means,
/// and the transport's job is only to send them.
fn search_wire(
    filters: &SearchFilters,
) -> (Vec<(String, String)>, Option<&'static str>, (usize, usize)) {
    let mut params: Vec<(String, String)> = Vec::new();
    params.push(("visibility".to_string(), "eq.public".to_string()));
    if let Some(archetype) = filters.archetype.as_deref() {
        params.push(("archetype".to_string(), format!("eq.{archetype}")));
    }
    if let Some(primary) = filters.primary_set.as_deref() {
        params.push(("primary_set".to_string(), format!("eq.{primary}")));
    }
    if let Some(secondary) = filters.secondary_set.as_deref() {
        params.push(("secondary_set".to_string(), format!("eq.{secondary}")));
    }
    if let Some(author) = filters.author_id.as_deref() {
        params.push(("user_id".to_string(), format!("eq.{author}")));
    } else if let Some(author) = filters.author_name.as_deref() {
        params.push(("author_name".to_string(), format!("eq.{author}")));
    }
    if !filters.tags.is_empty() {
        params.push(("tags".to_string(), tag_containment(&filters.tags)));
    }
    let query = filters.query.trim();
    if !query.is_empty() {
        // `textSearch('name', q, { type: 'websearch' })` — postgrest-js encodes websearch as
        // `wfts`, and the terms are the query verbatim (URL-encoded by the transport).
        params.push(("name".to_string(), format!("wfts.{query}")));
    }
    let order = match filters.sort_by {
        SortBy::Newest => Some("created_at.desc"),
        SortBy::Views => Some("views.desc"),
    };
    let from = (filters.page.saturating_sub(1)) * filters.page_size;
    let range = (from, from + filters.page_size.saturating_sub(1));
    (params, order, range)
}

/// The view every list read reads, named once — the same one [`super::favorites`] reads its
/// starred rows back out of, because a favourite is a shared build and there is one shape for
/// that.
pub(super) const BUILDS_VIEW: &str = "shared_builds_with_author";

/// One page of list rows, with the total. **The one place a paged list read names its
/// columns.**
///
/// Split from [`search`] for the reason [`search_wire`] is split from it — but this half is the
/// projection rather than the filter, and after F19 that is the half worth confining. A
/// projection spelled at each call site only has to be widened at one of them to be wide
/// everywhere; spelled here, `select=*` is a one-word mutation in a line the tests below read.
async fn list_page(
    cloud: &Cloud,
    filters: &[(&str, &str)],
    order: Option<&str>,
    range: Option<(usize, usize)>,
) -> Result<(Vec<SharedBuildSummary>, u64), CloudError> {
    cloud
        .select_counted::<SharedBuildSummary>(BUILDS_VIEW, LIST_COLUMNS, filters, order, range)
        .await
}

/// Every list row matching a filter, unpaged — the vault ([`my_builds`]) and the favourites
/// list ([`super::favorites::favorite_builds`]). The other half of [`list_page`], and the other
/// place the projection is named.
pub(super) async fn list_rows(
    cloud: &Cloud,
    filters: &[(&str, &str)],
    order: Option<&str>,
) -> Result<Vec<SharedBuildSummary>, CloudError> {
    cloud
        .select::<SharedBuildSummary>(BUILDS_VIEW, LIST_COLUMNS, filters, order)
        .await
}

/// Search public shared builds, paginated. `filters.page` is 1-based, like the beta's.
pub async fn search(cloud: &Cloud, filters: &SearchFilters) -> Result<SearchResult, CloudError> {
    let (params, order, range) = search_wire(filters);
    let refs: Vec<(&str, &str)> = params
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let (builds, total) = list_page(cloud, &refs, order, Some(range)).await?;
    let total_pages = if total == 0 {
        0
    } else {
        total.div_ceil(filters.page_size as u64) as usize
    };
    Ok(SearchResult {
        builds,
        total,
        page: filters.page,
        page_size: filters.page_size,
        total_pages,
    })
}

/// The PostgREST wire for [`my_builds`]: one `eq.` on the owning account, newest edit first.
///
/// Split out for the reason [`search_wire`] is — the filter IS the meaning of "mine", and a
/// filter that silently widened would list somebody else's builds under the user's own tab.
/// Gradeable without a server.
fn my_builds_wire(user_id: &str) -> (Vec<(String, String)>, &'static str) {
    (
        vec![("user_id".to_string(), format!("eq.{user_id}"))],
        // `updated_at`, not `created_at`: this list is a workspace, so the build touched last is
        // the one wanted first. The beta orders the same way (`sharedBuilds.ts:698`), and it is
        // the opposite of what the public browse defaults to, which sorts by publication.
        "updated_at.desc",
    )
}

/// Every build filed under an account — the vault list (RB4k), and the beta's `getMyBuilds`.
///
/// A PostgREST read rather than an edge function, and that is not the shortcut the module doc
/// bans. The ban is on reading rows an anon policy does not grant; this read is scoped by the
/// caller's OWN account id and runs under their session's bearer, so RLS answers it with exactly
/// the rows the server already agrees are theirs. Unlike [`search`] it is unpaged and uncounted,
/// which mirrors the beta — a personal vault is tens of rows, not thousands, and a pager over it
/// would be furniture around an empty state.
///
/// **The beta's pre-migration tolerance is deliberately NOT ported.** `getMyBuilds` sniffs the
/// error text for `user_id` and `does not exist` and answers with an empty list, covering a
/// deployment whose `user_id` column had not been added yet. Two reasons it does not come across:
///
///   * The state is gone. `GET /rest/v1/shared_builds_with_author?select=id,user_id,updated_at`
///     against the deployed project on 2026-09-17 returned rows with a populated `user_id`, and
///     the contrast — asking for a column that genuinely is absent — answered `42703 / HTTP 400`,
///     which is the shape the sniff was written for. The migration is applied.
///   * It is a Rule 1 violation on the worst possible surface. "You have no builds" is a
///     complete, plausible answer, and it would be a lie told about the user's own saved work
///     while the server was refusing the question. A visible error is a bug report; an empty
///     vault is a support ticket about lost builds.
///
/// So a failure here is `Err`, whatever it was.
pub async fn my_builds(
    cloud: &Cloud,
    user_id: &str,
) -> Result<Vec<SharedBuildSummary>, CloudError> {
    let (filters, order) = my_builds_wire(user_id);
    let refs: Vec<(&str, &str)> = filters
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    list_rows(cloud, &refs, Some(order)).await
}

/// Read one shared build by exact id, through `get-build` — the only path a non-owner reads an
/// unlisted build through (see the module doc and `get-build/index.ts:6-8`).
///
/// `Ok(None)` means the server answered 404 — unknown id, or private and not the owner, which
/// the function deliberately answers the same way so a 403 can never confirm an id exists.
/// Every other failure (network, the gateway, a 500) is `Err`, because a soft "not found" would
/// burn the evidence that the read path itself is broken.
pub async fn get_shared_build(cloud: &Cloud, id: &str) -> Result<Option<SharedBuild>, CloudError> {
    let body = serde_json::json!({ "id": id });
    match cloud.invoke::<_, SharedBuild>("get-build", &body).await {
        Ok(build) => Ok(Some(build)),
        Err(error) => map_get_error(error),
    }
}

/// The one error mapping the service owns. A 404 is the function's normal answer for "unknown,
/// or private and not yours" — the function responds the same either way so a 403 can never
/// confirm an id exists — and reads as `Ok(None)`. Every other failure (network, the gateway, a
/// 500) is carried as `Err`, because a soft "not found" would burn the evidence that the read
/// path itself is broken.
fn map_get_error(error: CloudError) -> Result<Option<SharedBuild>, CloudError> {
    match error {
        CloudError::Refused { status: 404, .. } => Ok(None),
        other => Err(other),
    }
}

/// Bump a build's public view counter, fire-and-forget. `increment_views` is `SECURITY
/// DEFINER`, so an anonymous visitor may call it; the beta discards the result
/// (`sharedBuilds.ts:541`) and the call sites here do the same — a view count failing is
/// not worth a modal, and the row that opens a detail page owns the choice.
pub async fn increment_views(cloud: &Cloud, id: &str) -> Result<(), CloudError> {
    cloud
        .rpc_empty("increment_views", &serde_json::json!({ "build_id": id }))
        .await
}

/// The server's answer to a backfill write.
#[derive(Deserialize)]
struct BackfillResponse {
    #[serde(default)]
    success: bool,
}

/// Submit a preview image for `id`, mirroring the beta's `submitPreviewBackfill`
/// (`sharedBuilds.ts:563-571`): no auth of its own — any visitor's browser can be the one that
/// generates an image — and the server only accepts a write when the row's stored version is
/// missing or behind [`CURRENT_PREVIEW_TEMPLATE_VERSION`], then shape-checks the PNG
/// (exactly 1200×880, under the byte cap) before storing it.
///
/// Returns whether the server accepted the image. `skipped` (server answered "nothing to do")
/// counts as success, exactly as the beta reads `!error && !data?.error`. The error is carried
/// rather than collapsed into `Ok(false)` so a caller that does want to know why can; the
/// beta's never-throws UI contract is a property of whatever orchestrates the capture, not of
/// this function.
pub async fn submit_preview_backfill(
    cloud: &Cloud,
    id: &str,
    preview_image_base64: &str,
) -> Result<bool, CloudError> {
    let body = serde_json::json!({
        "id": id,
        "preview_image_base64": preview_image_base64,
    });
    let response = cloud
        .invoke::<_, BackfillResponse>("backfill-preview", &body)
        .await?;
    Ok(response.success)
}

/// Whether viewing this build should try to fill in / refresh its preview image — the beta's
/// `needsPreviewCapture` (`BuildDetailPage.tsx:36-42`): private builds never (their page is
/// not public, so there is no unfurl to serve); a build with no image always; a build whose
/// image predates the live template needs a re-render.
pub fn preview_needs_backfill(build: &SharedBuildSummary) -> bool {
    if build.visibility == BuildVisibility::Private {
        return false;
    }
    if build.preview_image_path.is_none() {
        return true;
    }
    build
        .preview_template_version
        .is_none_or(|version| version < CURRENT_PREVIEW_TEMPLATE_VERSION)
}

/// Whether this planner can render `build`'s own preview faithfully: only when the build names
/// the dataset this instance has actually loaded. The capture reads the build against the
/// loaded database, so asking it to draw a foreign-fork build would render the wrong data as
/// if it were the build — the exact soft-wrong-number failure Rule 1 exists to catch. A
/// foreign-fork build simply keeps no capture and stays in the missing-preview state, which is
/// the state that triggered the attempt.
/// Which fork a row names is [`coh_data::skif::probe_dataset`]'s question, not a second copy of
/// it here. The spelling depends on the version — v5 says `build.dataset`, the legacy tier says
/// `build.serverId` — and RB4d's first cut read only the legacy one. That was invisible while
/// every row in the table was beta-authored v4, and would have started silently refusing every
/// capture the moment RB4e began writing v5 rows: a build shared from this app, opened in this
/// app, declining to render its own preview for a reason nothing logged.
pub fn preview_capture_matches(build_json: &serde_json::Value, loaded: DatasetId) -> bool {
    let Ok(text) = serde_json::to_string(build_json) else {
        return false;
    };
    coh_data::skif::probe_dataset(&text) == Some(loaded)
}

// ============================================================
// RB4e — the owner-scoped half: save, share, delete.
// ============================================================

/// Hourly save/share limits, mirroring `share-build/index.ts`'s own constants.
///
/// **A hint, never a decision.** The server is authoritative and returns live counters on every
/// successful save; these exist only to warn *before* a call and to fill in a message when a 429
/// arrives in the older unstructured shape. Nothing here refuses a call — a client that enforced
/// its own copy of a server's limit would refuse saves the server would have accepted, and would
/// do it from a constant that drifts silently the day the function is redeployed.
///
/// The beta's third constant, `windowHours`, is not mirrored. It was, until RB4g deleted the
/// module-wide `allow(dead_code)` and it turned out nothing read it: both messages below say
/// "hour" in prose, so the copy was a mirror nobody looked in.
pub const SHARE_RATE_LIMIT: u32 = 10;
/// Saved (private library) builds per hour. `'private'` and `'unlisted'` share this bucket.
pub const VAULT_RATE_LIMIT: u32 = 50;

/// Which hourly bucket a save was metered against.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RateLimitAction {
    /// A public share.
    Share,
    /// A saved build — private, unlisted, or a visibility-preserving re-save.
    Vault,
}

impl RateLimitAction {
    /// The bucket the SERVER will meter a save against, from the visibility the client asked for.
    ///
    /// `share-build/index.ts:175` is the rule, verbatim: `meterAsPublic = visibilityProvided &&
    /// visibility === 'public'`. An omitted visibility is a visibility-preserving re-save and
    /// meters as a vault action.
    ///
    /// **This deliberately does not mirror the beta's client-side copy**, which reads
    /// `visibility === 'private' || 'unlisted' ? 'vault' : 'share'` (`sharedBuilds.ts:355-357`)
    /// and so answers `share` for the omitted case the server calls `vault`. That copy is only
    /// reachable when the server sends an unstructured 429, and its disagreement costs the user a
    /// message naming the wrong bucket and the wrong number — "you've hit the limit (10 public
    /// shares)" for a limit of 50 saved builds. Mirroring the function rather than the beta's
    /// mirror of it is the whole reason this is a function and not a literal.
    pub fn for_requested(visibility: Option<BuildVisibility>) -> Self {
        match visibility {
            Some(BuildVisibility::Public) => Self::Share,
            _ => Self::Vault,
        }
    }

    pub fn limit(self) -> u32 {
        match self {
            Self::Share => SHARE_RATE_LIMIT,
            Self::Vault => VAULT_RATE_LIMIT,
        }
    }

    /// What this bucket is called in a sentence addressed to the user.
    pub fn label(self) -> &'static str {
        match self {
            Self::Share => "public shares",
            Self::Vault => "saved builds",
        }
    }
}

/// The live counters a successful save comes back with. The server's numbers, not ours.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub struct RateLimitInfo {
    pub action: RateLimitAction,
    pub limit: u32,
    pub remaining: u32,
}

/// A refused save, with enough to say when the limit lifts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RateLimited {
    pub action: RateLimitAction,
    pub limit: u32,
    /// Seconds until the oldest in-window request ages out. `0` when the server did not say —
    /// the older unstructured 429 — and the message then says "within the hour" rather than
    /// inventing a countdown.
    pub retry_after_seconds: u64,
    pub reset_at: Option<String>,
}

/// The structured fields `share-build` puts beside the message on a 429. Every one optional,
/// because the older deployed shape is a bare `{"error": "..."}` at status 429 and this has to
/// read that too.
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RefusalBody {
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    action: Option<RateLimitAction>,
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    retry_after_seconds: Option<u64>,
    #[serde(default)]
    reset_at: Option<String>,
}

/// Why a save, visibility change, delete or claim did not happen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShareError {
    /// The hourly limit, with the countdown. Split from [`Self::Cloud`] because it is the one
    /// refusal that is not a fault and that the user can act on by waiting.
    RateLimited(RateLimited),
    /// An update was asked for with neither an owner token for that build nor a session — there
    /// is no credential to prove ownership with, so the call is not worth making.
    NotOwned,
    /// The action is account-only and there is no session.
    SignInRequired,
    Cloud(CloudError),
}

impl From<CloudError> for ShareError {
    fn from(error: CloudError) -> Self {
        Self::Cloud(error)
    }
}

impl std::fmt::Display for ShareError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RateLimited(limit) => write!(f, "{}", rate_limit_message(limit)),
            Self::NotOwned => write!(
                f,
                "this browser holds no owner token for that build, and there is no session"
            ),
            Self::SignInRequired => write!(f, "sign in to do that"),
            Self::Cloud(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for ShareError {}

/// The sentence a hit limit gets, mirroring the beta's `formatRateLimitMessage`: a precise
/// countdown when the server reported one, and "within the hour" when it did not — never a
/// fabricated number standing in for a fact the server withheld.
pub fn rate_limit_message(limit: &RateLimited) -> String {
    let head = format!(
        "You've hit the hourly limit ({} {}).",
        limit.limit,
        limit.action.label()
    );
    if limit.retry_after_seconds == 0 {
        return format!("{head} Please try again within the hour.");
    }
    let minutes = limit.retry_after_seconds.div_ceil(60).max(1);
    let plural = if minutes == 1 { "" } else { "s" };
    format!("{head} Try again in ~{minutes} min{plural}.")
}

/// The proactive one-liner shown before any limit is hit (the beta's `rateLimitHint`).
pub fn rate_limit_hint(action: RateLimitAction) -> String {
    format!("Up to {} {} per hour.", action.limit(), action.label())
}

/// Read a refusal, and decide whether it is the hourly limit.
///
/// Three tells, any of which is enough, mirroring `sharedBuilds.ts:344`: the 429 status, the
/// structured `code: "rate_limited"`, or the message saying so in words. Three rather than one
/// because the deployed function and the current source do not answer identically, and a client
/// that recognised only the newest shape would report a rate limit as an unexplained failure
/// against the older one.
///
/// `requested` is the visibility the call asked for, used only to name the bucket when the
/// server's answer did not — see [`RateLimitAction::for_requested`].
fn classify_refusal(error: CloudError, requested: Option<BuildVisibility>) -> ShareError {
    let CloudError::Refused {
        status,
        ref message,
        ref body,
    } = error
    else {
        return ShareError::Cloud(error);
    };
    let parsed: RefusalBody = serde_json::from_str(body).unwrap_or_default();
    let says_so = parsed.code.as_deref() == Some("rate_limited")
        || parsed
            .error
            .as_deref()
            .unwrap_or(message)
            .to_lowercase()
            .contains("rate limit");
    if status != 429 && !says_so {
        return ShareError::Cloud(error);
    }
    let action = parsed
        .action
        .unwrap_or_else(|| RateLimitAction::for_requested(requested));
    ShareError::RateLimited(RateLimited {
        action,
        limit: parsed.limit.unwrap_or_else(|| action.limit()),
        retry_after_seconds: parsed.retry_after_seconds.unwrap_or(0),
        reset_at: parsed.reset_at,
    })
}

/// Everything `share-build` is told about a build being saved.
///
/// The identity columns are carried explicitly rather than re-read out of `build_json`, which is
/// what the beta does (`sharedBuilds.ts:280-296`). Two reasons, and the second is the real one:
/// v5 spells the archetype as a bare id where v4 spelled it `{ id, name }`, so a re-read would
/// need version-aware code to find what the caller already has — and the display NAMES are the
/// database's, not the file's. A build file carries ids; "Illusion Control" is what the loaded
/// dataset calls that id today. Reading the name back out of the build would ship whatever name
/// was current when the build was authored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShareInput {
    pub name: String,
    pub description: String,
    pub archetype: String,
    pub archetype_name: String,
    pub primary_set: String,
    pub primary_name: String,
    pub secondary_set: String,
    pub secondary_name: String,
    pub level: u8,
    pub author_name: String,
    pub server: String,
    pub tags: Vec<String>,
    /// The `{version, build}` envelope, as [`coh_data::skif::encode`] writes it.
    pub build_json: serde_json::Value,
    /// `None` means "do not send the field at all" — on an update that tells the server to
    /// PRESERVE the row's current visibility. It must not be defaulted to `Public` here: the
    /// omission is load-bearing, and defaulting it would silently re-publish a build the user
    /// had made private since sharing it (`sharedBuilds.ts:298-303`).
    pub visibility: Option<BuildVisibility>,
    /// Set to update an existing row rather than create one.
    pub existing_id: Option<String>,
    /// The social card, already rendered and base64'd. Best-effort: the field is simply absent
    /// when capture failed, and the server keeps whatever image the row already had rather than
    /// nulling it.
    pub preview_image_base64: Option<String>,
}

/// What the server said about a save.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Shared {
    pub id: String,
    /// Minted by the server on a CREATE only, and the only copy of it that will ever exist —
    /// the row stores a SHA-256. [`super::owner_store`] is where it has to land.
    #[serde(default)]
    pub owner_token: Option<String>,
    #[serde(default)]
    pub updated: bool,
    /// `rateLimit` on the wire while `owner_token` beside it is snake_case — the function
    /// spells its own response both ways in one object (`share-build/index.ts:319` and `:349`),
    /// so this rename is mirroring the server rather than tidying it.
    #[serde(default, rename = "rateLimit")]
    pub rate_limit: Option<RateLimitInfo>,
}

/// The `share-build` request body.
///
/// Split from [`share_build`] so the omission rules are gradeable without a server: which fields
/// are present is the whole contract here, and two of them (`visibility`, `owner_token`) mean
/// something different by being absent than by being null.
fn share_payload(input: &ShareInput, owner_token: Option<&str>) -> serde_json::Value {
    let mut payload = serde_json::json!({
        "name": input.name,
        "description": input.description,
        "archetype": input.archetype,
        "archetype_name": input.archetype_name,
        "primary_set": input.primary_set,
        "primary_name": input.primary_name,
        "secondary_set": input.secondary_set,
        "secondary_name": input.secondary_name,
        "level": input.level,
        "author_name": input.author_name,
        "server": input.server,
        "tags": input.tags,
        "build_json": input.build_json,
    });
    let map = payload.as_object_mut().expect("a json! object");
    // Only when explicitly set — see `ShareInput::visibility`.
    if let Some(visibility) = input.visibility {
        map.insert(
            "visibility".to_string(),
            serde_json::to_value(visibility).expect("a unit enum"),
        );
    }
    if let Some(id) = &input.existing_id {
        map.insert(
            "existing_id".to_string(),
            serde_json::Value::from(id.clone()),
        );
    }
    if let Some(token) = owner_token {
        map.insert("owner_token".to_string(), serde_json::Value::from(token));
    }
    // Absent rather than null when capture failed: `uploadPreviewImage` leaves the column
    // untouched on a non-string, so a stale-but-present image beats no image.
    if let Some(image) = &input.preview_image_base64 {
        map.insert(
            "preview_image_base64".to_string(),
            serde_json::Value::from(image.clone()),
        );
    }
    payload
}

/// Save a build to the shared repository — create, or update in place.
///
/// **On the session refresh the beta does here and this does not.** `shareBuild` calls
/// `supabase.auth.refreshSession()` unconditionally before the invoke, to dodge a 401 from an
/// expired JWT. That job is already done, once, upstream: every call site reaches this through
/// `account.prepared().await?`, which refreshes inside the lock when the token is within its
/// expiry margin (RB4b). Mirroring the unconditional refresh would not be belt-and-braces but a
/// second door around that rule — and an expensive one, because **a GoTrue refresh token is
/// single-use**: a forced refresh on every save spends a token the margin rule did not need to
/// spend, and `refresh_failure` reads a refusal as a dead session.
///
/// `signed_in` is whether there is a session. It decides only whether an update may be attempted
/// without an owner token, exactly as the beta's `!ownerToken && !user` check does — the server
/// re-checks both credentials and is the one that actually decides.
pub async fn share_build(
    cloud: &Cloud,
    input: &ShareInput,
    owner_token: Option<&str>,
    signed_in: bool,
) -> Result<Shared, ShareError> {
    if input.existing_id.is_some() && owner_token.is_none() && !signed_in {
        return Err(ShareError::NotOwned);
    }
    let payload = share_payload(input, owner_token);
    cloud
        .invoke::<_, Shared>("share-build", &payload)
        .await
        .map_err(|error| classify_refusal(error, input.visibility))
}

/// Change a build's visibility. Account-only: the function refuses a token-owned row outright
/// (`update-build-visibility/index.ts:88`), because visibility is the one control an anonymous
/// link-holder must not have over a row they can otherwise edit.
pub async fn update_build_visibility(
    cloud: &Cloud,
    id: &str,
    visibility: BuildVisibility,
    signed_in: bool,
) -> Result<(), ShareError> {
    if !signed_in {
        return Err(ShareError::SignInRequired);
    }
    let body = serde_json::json!({ "id": id, "visibility": visibility });
    cloud
        .invoke::<_, serde_json::Value>("update-build-visibility", &body)
        .await?;
    Ok(())
}

/// Delete a shared build. Ownership is proven by the owner token, the session, or both; the
/// server checks each and refuses 403 if neither matches.
///
/// Returns nothing on success — the caller is responsible for dropping the owner token, because
/// the token store is the UI layer's to touch and a service that reached into `localStorage`
/// would be doing it from whichever window happened to make the call.
pub async fn delete_build(
    cloud: &Cloud,
    id: &str,
    owner_token: Option<&str>,
    signed_in: bool,
) -> Result<(), ShareError> {
    if owner_token.is_none() && !signed_in {
        return Err(ShareError::NotOwned);
    }
    let mut body = serde_json::json!({ "id": id });
    if let Some(token) = owner_token {
        body.as_object_mut()
            .expect("a json! object")
            .insert("owner_token".to_string(), serde_json::Value::from(token));
    }
    cloud
        .invoke::<_, serde_json::Value>("delete-build", &body)
        .await?;
    Ok(())
}

/// What `claim-builds` did with the tokens it was handed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct Claimed {
    #[serde(default)]
    pub claimed: Vec<String>,
    #[serde(default)]
    pub failed: Vec<String>,
}

/// Attach this browser's token-owned builds to the signed-in account.
///
/// Called after a sign-in: builds shared anonymously before the user had an account become
/// theirs, so they survive a browser change. An empty token map is `Ok` with nothing claimed and
/// no call made — the beta's early return, and it matters because the alternative is a request
/// on every sign-in for the majority of users who have no anonymous builds at all.
///
/// The server caps a single call at 50 (`claim-builds/index.ts:68`) and this does not pre-check
/// it, for the same reason nothing here pre-checks a rate limit: the cap is the server's, and a
/// client copy of it would refuse a call the server might accept.
pub async fn claim_builds(
    cloud: &Cloud,
    owner_tokens: &[(String, String)],
    signed_in: bool,
) -> Result<Claimed, ShareError> {
    if !signed_in {
        return Err(ShareError::SignInRequired);
    }
    if owner_tokens.is_empty() {
        return Ok(Claimed::default());
    }
    let map: serde_json::Map<String, serde_json::Value> = owner_tokens
        .iter()
        .map(|(id, token)| (id.clone(), serde_json::Value::from(token.clone())))
        .collect();
    let body = serde_json::json!({ "owner_tokens": map });
    Ok(cloud.invoke::<_, Claimed>("claim-builds", &body).await?)
}

// ---- quick share: the one-click unlisted link ----------------------------------------------

/// Fingerprint a build, so an unchanged one can be re-copied without a network call.
///
/// SHA-256 of the build object, truncated to 64 bits — the beta's `fingerprintBuild`
/// (`sharedBuilds.ts:451-470`), same hash and same truncation. 64 bits is plenty against the
/// only collision that matters here: two builds by one user, in one browser, back to back.
///
/// **It will not agree with a fingerprint the beta wrote, and that is expected.** The two apps
/// hash different payloads — v4's slim shape there, v5's here — so at the cut-over every cached
/// entry reads as a miss. A miss costs exactly one `share-build` call, which updates the cached
/// row in place and rewrites the entry in this app's terms. Nothing is orphaned, which is why
/// this cache is not the compatibility contract the owner-token map is.
pub fn fingerprint_build(build_json: &serde_json::Value) -> String {
    use sha2::{Digest, Sha256};
    let canonical = build_json
        .get("build")
        .map(|build| build.to_string())
        .unwrap_or_default();
    let digest = Sha256::digest(canonical.as_bytes());
    digest[..8].iter().map(|b| format!("{b:02x}")).collect()
}

/// The name a quick-shared build is filed under when the user never gave it one.
///
/// Mirrors the beta's fallback (`sharedBuilds.ts:484-491`): an untitled build becomes
/// "Generic <Archetype> - <Primary> - <Secondary>" so a vault of one-click shares is not a page
/// of identical rows. A name the user DID choose is never touched.
pub fn quick_share_name(name: &str, archetype: &str, primary: &str, secondary: &str) -> String {
    let trimmed = name.trim();
    let untitled = trimmed.is_empty() || trimmed.eq_ignore_ascii_case("untitled build");
    if !untitled {
        return trimmed.to_string();
    }
    let generic = format!("Generic {archetype}");
    let parts: Vec<&str> = [generic.trim(), primary.trim(), secondary.trim()]
        .into_iter()
        .filter(|part| !part.is_empty() && *part != "Generic")
        .collect();
    match parts.is_empty() {
        true => "Shared Build".to_string(),
        false => parts.join(" - "),
    }
}
