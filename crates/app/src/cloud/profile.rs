//! RB4f's surface: the public profile, and the two RPCs that turn one into a place.
//!
//! Three acts, and they are three because the server splits them that way:
//!
//! * **Edit** — `update-profile`, the edge function. Handle, display name, bio; everything else
//!   on the row is derived from the JWT on the way through and is read-only here.
//! * **Search** — `search_authors`, an RPC, behind the browser's author box.
//! * **Resolve** — `resolve_author`, an RPC, which is what makes `/author/<handle>` a URL that
//!   opens something rather than a string in a profile page.
//!
//! # The rules are mirrored for the message and never for the verdict
//!
//! `^[a-z0-9][a-z0-9_-]{2,29}$`, a 30-day change cooldown, display name ≤ 30, bio ≤ 280 — those
//! four live in three places at once: the `profiles` CHECK constraints, `update-profile`'s own
//! guards, and [`profile.ts:28-31`] in the beta. This module is the fourth, and the distinction
//! that keeps a fourth copy honest is which way it is allowed to be wrong.
//!
//! [`handle_fault`] may **refuse to spend a call** whose answer is already known — the same
//! bargain [`super::save_build::missing_identity`] makes, and for the same reason: relaying
//! "Handle must be 3-30 characters…" back from a round trip tells the user less than saying which
//! character was the problem, and burns a request to say it. What it must never do is decide that
//! a handle is *acceptable*. Reserved, already taken, still in cooldown — those are three
//! refusals this client cannot compute, and two of them read against tables it cannot see. So
//! [`update`] validates nothing at all, and every refusal the user reads is the server's own
//! words arriving through [`super::CloudError::Refused`].
//!
//! The asymmetry is what bounds the drift risk. If the server loosened its handle rule tomorrow,
//! this copy would block a handle the server would have taken — a visible, reportable wrong. If
//! it tightened one, this copy would wave through a handle the server refuses, and the user reads
//! the real reason. Neither outcome is a fabricated success, which is the only outcome Rule 1
//! actually forbids.
//!
//! # The cooldown is computed here because the server only reports it on refusal
//!
//! `update-profile` answers a handle change inside the window with a 429 carrying the days
//! remaining in the message string (`update-profile/index.ts:131`). That is the whole of the
//! server's cooldown reporting: there is no field, and no way to ask. A client that only relayed
//! it would have to let the user type a new handle, press Save, and be told *then* — which is the
//! "failing blankly" this row was written against.
//!
//! So `handle_changed_at` comes back on the row, and [`handle_cooldown`] reads the window off it.
//! It is a **third** copy of the 30 days, and it answers a question the server would answer the
//! same way. It is also the one piece of this module that can be silently wrong, so it does not
//! get to be silent: an unparseable timestamp is [`CooldownUnreadable`], not `Free`. The beta
//! computes `NaN` there and renders "in NaN days" — which locks the field with no way out and
//! reads as a bug in the cooldown rather than in the value it was given.

use super::account::Account;
use super::avatar::avatar_src;
use super::{Cloud, CloudError};
use crate::modal::{Modal, ModalSize};
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

// ============================================================
// The rules, as constants.
// ============================================================

/// Shortest handle the format admits: one leading character plus the two the `{2,29}` requires.
pub const HANDLE_MIN: usize = 3;
/// Longest handle the format admits: the leading character plus 29.
pub const HANDLE_MAX: usize = 30;
/// `profiles.display_name_length`, and `update-profile/index.ts:97`.
pub const DISPLAY_NAME_MAX: usize = 30;
/// `profiles.bio_length`, and `update-profile/index.ts:106`.
pub const BIO_MAX: usize = 280;
/// `HANDLE_COOLDOWN_DAYS` — `update-profile/index.ts:22`.
pub const HANDLE_COOLDOWN_DAYS: u32 = 30;

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

// ============================================================
// The rows.
// ============================================================

/// A `profiles` row, as PostgREST hands it over.
///
/// `handle` is `Option` because the column is nullable until claimed, and that nullability is
/// load-bearing rather than incidental: a profile with no handle has no author page, and the
/// first claim is the one handle change that is never gated by the cooldown.
///
/// The Discord pair and the avatar are read-only here by contract — `update-profile` refreshes
/// them from the JWT on every call and this client has no way to write them. They are carried
/// because the edit surface draws them, not because anything may set them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct Profile {
    pub user_id: String,
    #[serde(default)]
    pub handle: Option<String>,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub discord_id: Option<String>,
    #[serde(default)]
    pub discord_username: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub bio: String,
    /// When the handle last changed. `None` before the first claim — which is why the cooldown
    /// reads `Free` on a profile that has never had one.
    #[serde(default)]
    pub handle_changed_at: Option<String>,
}

/// The patch `update-profile` takes. **Every field is omitted when absent, never sent as null.**
///
/// The function reads `body.display_name !== undefined` to decide whether to touch a column
/// (`update-profile/index.ts:93`), so an explicit `null` is not "leave it alone" — it is a value,
/// and `String(null).trim()` is the four-character string `"null"`. A serializer that emitted
/// nulls for the untouched fields would set a user's display name to `null` and their bio to
/// `null` every time they claimed a handle, and the write would succeed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ProfileUpdate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bio: Option<String>,
}

impl ProfileUpdate {
    /// Whether this patch would touch anything. An empty patch is answered by the server with the
    /// profile unchanged and a 200 (`update-profile/index.ts:166`), so sending one is legal and
    /// pointless; the surface uses this to keep Save inert rather than to refuse.
    pub fn is_empty(&self) -> bool {
        self.handle.is_none() && self.display_name.is_none() && self.bio.is_none()
    }
}

/// What `resolve_author` returns: the public face of a profile that has claimed a handle.
///
/// `handle` is not `Option` here and that is the RPC's doing rather than a convenience — its
/// `WHERE handle = h::citext` cannot match a null, so every row it can produce has one.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct PublicAuthor {
    pub user_id: String,
    pub handle: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub bio: String,
}

/// One row of `search_authors`.
///
/// `handle` IS `Option` here, unlike [`PublicAuthor`]'s: this RPC selects from `profiles` with no
/// handle predicate, so it finds accounts that have never claimed one. Such an author has builds
/// and a display name and no author page — the dropdown filters by `user_id`, which every row
/// has, so the missing handle costs the entry nothing but its `@` line.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct AuthorSearchResult {
    pub user_id: String,
    #[serde(default)]
    pub handle: Option<String>,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
    /// Public builds only — the RPC's `FILTER (WHERE b.visibility = 'public')`. A private build
    /// is not a thing the dropdown may count, because the count is shown to everyone.
    #[serde(default)]
    pub build_count: i64,
    /// The similarity the RPC ranked on. Carried because it is in the row, not drawn.
    #[serde(default)]
    pub sim: f32,
}

// ============================================================
// The handle format.
// ============================================================

/// Which rule a handle breaks, or `None` if it breaks none of the four this client can check.
///
/// Named per rule rather than returned as one sentence because the point of checking here at all
/// is to say something the round trip would not — `update-profile` answers every one of these
/// with the same 47-word string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandleFault {
    TooShort,
    TooLong,
    /// The first character is `-` or `_`. The format's leading `[a-z0-9]` exists to keep a handle
    /// from opening with punctuation; it is the one position with a narrower character class.
    LeadingPunctuation,
    /// A character outside `[a-z0-9_-]`, carried so the message can name it. Uppercase is not a
    /// fault the user ever sees: [`normalize_handle`] lowercases before this runs, matching the
    /// function, which lowercases before it tests (`update-profile/index.ts:112`).
    BadCharacter(char),
}

impl HandleFault {
    /// The fault in the user's terms, for an inline hint under the field.
    pub fn message(self) -> String {
        match self {
            Self::TooShort => format!("Handles are at least {HANDLE_MIN} characters."),
            Self::TooLong => format!("Handles are at most {HANDLE_MAX} characters."),
            Self::LeadingPunctuation => {
                "A handle has to start with a letter or a digit.".to_string()
            }
            Self::BadCharacter(found) => {
                format!("“{found}” is not allowed — use letters, digits, “_” or “-”.")
            }
        }
    }
}

/// Test a handle against `^[a-z0-9][a-z0-9_-]{2,29}$`, decomposed.
///
/// Character counts rather than byte lengths: the regex quantifies characters, and a handle typed
/// with an accented letter is one bad character rather than two.
pub fn handle_fault(handle: &str) -> Option<HandleFault> {
    let mut characters = handle.chars();
    let first = characters.next()?;
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        // A leading `-` or `_` is the format's own carve-out; anything else is just a bad
        // character that happens to be in position one, and saying so is more use.
        return Some(match first {
            '-' | '_' => HandleFault::LeadingPunctuation,
            other => HandleFault::BadCharacter(other),
        });
    }
    // The charset is checked before the length so a handle that is both too long and full of
    // spaces reports the spaces — the fixable thing, and the one the user did on purpose.
    for found in characters.clone() {
        if !found.is_ascii_lowercase() && !found.is_ascii_digit() && found != '_' && found != '-' {
            return Some(HandleFault::BadCharacter(found));
        }
    }
    match handle.chars().count() {
        length if length < HANDLE_MIN => Some(HandleFault::TooShort),
        length if length > HANDLE_MAX => Some(HandleFault::TooLong),
        _ => None,
    }
}

/// A handle as the server will read it: trimmed, stripped of a display `@`, lowercased.
///
/// The `@` is a UI convention and never part of the value — the beta says so at
/// `profile.ts:133`, the URL carries the bare handle, and `resolve_author` matches the column.
/// Tolerated on the way in because a user who reads `@tigereyes` on a card and types it into a
/// search box has done nothing wrong.
///
/// `None` for a string with nothing left in it, so an empty box and a box holding `"  @ "` are
/// the same absence rather than two.
pub fn normalize_handle(raw: &str) -> Option<String> {
    let normalized = raw.trim().trim_start_matches('@').trim().to_lowercase();
    (!normalized.is_empty()).then_some(normalized)
}

// ============================================================
// The author line.
// ============================================================

/// What a shared build's author line is allowed to say — the client half of F69.
///
/// `author_name` is free text the sharer typed into the share form. `author_handle` rides the
/// same row because `shared_builds_with_author` joins the owner's profile onto it, and it is the
/// only part of an author line this client can prove. Drawing the two as one string is what made
/// the free-text half an identity claim rather than a label: a card reading `@tigereyes` was
/// indistinguishable from a card belonging to the account that actually holds that handle, and
/// nothing on the card said which one it was.
///
/// **The invariant is one line: an `@` on an author line always came from a handle the join
/// proved, never from `author_name`.** It is a type rather than care taken at each call site
/// because the two call sites today will not be the last, and the next one gets written by
/// somebody who has not read this.
///
/// **The server-side normaliser does not make this redundant, and the database is what says so.**
/// `_shared/author-name.ts` strips the sigil on the way in, but it shipped on 2026-09-18 and the
/// rows already stored were never rewritten. Measured against production the same day, across all
/// 5,007 rows: 64 public rows open with `@` — 45 naming a handle nobody holds, 18 naming one the
/// row's own account does hold, and one anonymous share (`user_id IS NULL`) naming a handle that
/// exists. **No row has a signed-in account claiming a different account's handle**, and that
/// count is zero rather than unmeasured.
///
/// The one anonymous row is the argument, not an incident. Its likeliest explanation is dull —
/// the person who holds that handle shared a build while signed out and typed their own name —
/// and **nothing in the data distinguishes that from the other reading**, which is precisely why
/// the client may not render it as a handle. A surface that cannot tell the two apart must not
/// draw the one that would be a claim. No server-side fix reaches these rows either way.
///
/// The measurement is re-runnable, and a non-zero `hidden_sigil` is the one result that would
/// break what [`author_identity`] claims to cover. Written with Postgres `E''` escapes rather
/// than the characters themselves, because a zero-width character pasted into a source file is
/// invisible to the next reader of it:
///
/// ```text
/// supabase db query --linked "
///   select count(*) filter (where author_name ~ '^[@[:space:]]') as open_sigil,
///          count(*) filter (where author_name ~ E'[\u200b-\u200f\u202a-\u202e\u2066-\u2069\ufeff]')
///            as hidden_sigil
///   from public.shared_builds;"
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthorIdentity {
    /// The share named no author, and no account holds the row.
    Anonymous,
    /// A display name with nothing behind it. It carries no sigil and is not a link — a click
    /// has nowhere honest to go when there is no account at the other end.
    Unverified { display: String },
    /// A handle the join proved, and whatever free-text name the share carried beside it.
    ///
    /// `display` is empty when the share named no author. The handle then stands alone rather
    /// than the line reading "Anonymous", because this row demonstrably has an owner and saying
    /// otherwise would be the same class of lie in the other direction. Not a hypothetical arm:
    /// of the 1,328 public rows whose author holds a handle, **101 carry no `author_name`**, and
    /// every one of them drew "Anonymous" before this (measured 2026-09-18).
    Verified { display: String, handle: String },
}

/// Whether a display name may open with this character: does it draw ink?
///
/// The positive half of F69's rule, and the mirror of `OPENER` in the beta's
/// `_shared/author-name.ts` and `utils/author-identity.ts`. Three spellings of one rule is a
/// cost; the alternative was a fourth leading-run enumeration, which is what was being paid for
/// already.
///
/// Ink is the five general-category groups that put marks on a page — letters, numbers, marks,
/// symbols, punctuation — minus the two kinds of blank those groups let through:
///
///   * **Default-ignorable code points**, the property Unicode maintains for exactly this
///     question. It is what catches the HANGUL FILLER family (U+3164, U+115F, U+1160, U+FFA0),
///     all of them category `Lo` and all of them blank, and it catches the next one without this
///     function being edited, which is the whole reason for reaching a property rather than a
///     list.
///   * **U+2800 BRAILLE PATTERN BLANK**, an empty braille cell — category `So`, *not*
///     default-ignorable, and one column of nothing. It has to be named, and naming exactly one
///     codepoint is the residual enumeration this rule could not get rid of.
///
/// Whitespace needs no arm: none of the five groups contains it, so a space fails the test by
/// construction rather than by a rule that has to remember spaces exist.
///
/// **This goes stale too, and the direction is the point.** A blank glyph Unicode adds tomorrow
/// that is neither default-ignorable nor U+2800 is admitted. What changed is the cost of being
/// wrong: an unfamiliar non-ink character now loses a leading character off a display name,
/// where the rules this replaced let a sigil through to the card.
fn opens_a_name(c: char) -> bool {
    use icu_properties::props::{DefaultIgnorableCodePoint, GeneralCategory, GeneralCategoryGroup};
    use icu_properties::{CodePointMapData, CodePointSetData};

    if c == '\u{2800}' || CodePointSetData::new::<DefaultIgnorableCodePoint>().contains(c) {
        return false;
    }
    let category = CodePointMapData::<GeneralCategory>::new().get(c);
    GeneralCategoryGroup::Letter.contains(category)
        || GeneralCategoryGroup::Number.contains(category)
        || GeneralCategoryGroup::Mark.contains(category)
        || GeneralCategoryGroup::Symbol.contains(category)
        || GeneralCategoryGroup::Punctuation.contains(category)
}

/// Decide what an author line says, from the two columns that carry it.
///
/// The sigil comes off `author_name` here rather than in the markup, because a stripped name is
/// what the whole app should see: `author_name` is the label, and the label was never allowed to
/// be a handle. Case survives, unlike [`normalize_handle`]: a display name is not a lookup key,
/// and `Savant` is not `savant`.
///
/// **The rule says what may END the leading run, not what may be in it, and the two rules that
/// said the other thing were both bypassed.** `sanitizeAuthorName` first stripped `^@+` once
/// after trimming, so `@@savant` came out clean and `@ @savant` came out as `@savant` — still
/// opening with the sigil. Widening the run to `@` AND whitespace fixed that and was bypassed in
/// turn by `\u{3164}@admin`: U+3164 HANGUL FILLER is general category `Lo`, a **letter** that
/// renders as a blank, so it is neither a sigil nor whitespace and the trim stopped dead in
/// front of it. Both failures are one failure — a leading-run predicate has to list what may be
/// skipped, and the list is never finished.
///
/// So [`opens_a_name`] states the complement: a name may open with anything that draws ink and
/// nothing that draws none, and everything before the first such character comes off, sigil or
/// blank, in any order. `@\u{3164}@admin` reduces for the same reason `@ @savant` does.
///
/// What it still deliberately does not mirror is the rest of that normaliser — the bidi
/// overrides, the zero-width family, the `\p{Zs}` impostors — *away from the front of the
/// string*. Those are how one name is made to render as another rather than how it claims a
/// namespace, and no stored row carries one: measured across all 5,007 rows on 2026-09-18, the
/// count was zero. **That measurement is also what failed here**, and the way it failed is worth
/// keeping: it counted three named classes, U+3164 is in none of them, and the query therefore
/// could not have found the thing that broke the bound it was supporting. The residual is now a
/// row hiding a sigil behind a blank-renderer *after* the first ink character, where it claims
/// nothing, rather than in front of it, where it claimed everything.
pub fn author_identity(author_name: &str, author_handle: Option<&str>) -> AuthorIdentity {
    let display = author_name
        .trim_start_matches(|c: char| c == '@' || !opens_a_name(c))
        .trim_end()
        .to_string();
    match author_handle.and_then(normalize_handle) {
        Some(handle) => AuthorIdentity::Verified { display, handle },
        None if display.is_empty() => AuthorIdentity::Anonymous,
        None => AuthorIdentity::Unverified { display },
    }
}

// ============================================================
// The cooldown.
// ============================================================

/// Whether the handle may be changed right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandleCooldown {
    /// No change has been recorded, or the window has passed.
    Free,
    /// A change landed inside the window. `days_left` is always ≥ 1 — a window with under a day
    /// left rounds up, matching `Math.ceil` on both the server and the beta, because "0 days
    /// left" beside a locked field reads as a stuck control.
    Waiting { days_left: u32 },
}

/// A `handle_changed_at` that could not be read. Carries the text, because a timestamp this
/// client cannot parse is a schema or serialisation change and the only evidence of it is the
/// string itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CooldownUnreadable(pub String);

impl std::fmt::Display for CooldownUnreadable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "could not read when the handle last changed: {}", self.0)
    }
}

impl std::error::Error for CooldownUnreadable {}

/// Days remaining on the 30-day window, given the row's `handle_changed_at` and the wall clock.
///
/// `now_ms` is passed rather than read so this is gradeable without a clock — every boundary in
/// the window is a test below rather than a thing that is true on some days.
///
/// An absent timestamp is [`HandleCooldown::Free`] and an unreadable one is an error, which is
/// the distinction the beta collapses: `new Date(null)` and `new Date("nonsense")` both leave
/// its arithmetic holding `NaN`, and only one of them means "never changed".
pub fn handle_cooldown(
    handle_changed_at: Option<&str>,
    now_ms: i64,
) -> Result<HandleCooldown, CooldownUnreadable> {
    let Some(stamp) = handle_changed_at.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(HandleCooldown::Free);
    };
    let changed_ms =
        parse_timestamp_ms(stamp).ok_or_else(|| CooldownUnreadable(stamp.to_string()))?;
    let expires_ms = changed_ms + i64::from(HANDLE_COOLDOWN_DAYS) * DAY_MS;
    let left_ms = expires_ms - now_ms;
    if left_ms <= 0 {
        return Ok(HandleCooldown::Free);
    }
    // Ceiling division on positives; `left_ms` is > 0 here, so this is ≥ 1 by construction and
    // the `days_left` doc's guarantee holds without a clamp.
    let days_left = (left_ms + DAY_MS - 1) / DAY_MS;
    Ok(HandleCooldown::Waiting {
        days_left: days_left as u32,
    })
}

/// An ISO-8601 instant to milliseconds since the epoch.
///
/// Two spellings reach this from one column, and RFC 3339 covers both. `update-profile` writes
/// `new Date().toISOString()` — `2026-09-16T12:27:00.000Z` — and PostgREST renders the same
/// `timestamptz` back as `2026-09-16T12:27:00+00:00`, which is the spelling a read of the row
/// arrives in. Nothing else is accepted: the Postgres console's `2026-09-16 12:27:00+00` is not
/// a shape any wire this client reads produces, and a lenient path for it would be a repair
/// with no caller, tested against a string the server never sends.
///
/// `chrono` is used for parsing only. The workspace builds it with `clock` but without
/// `wasmbind`, so `Utc::now()` would reach `std::time::SystemTime::now()` and panic on
/// `wasm32-unknown-unknown`; the clock in this file is [`now_unix_ms`], which is `web-time`, for
/// the reason [`super::session`] gives.
fn parse_timestamp_ms(stamp: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(stamp)
        .ok()
        .map(|parsed| parsed.timestamp_millis())
}

/// Milliseconds since the epoch. `web-time` is `Date.now()` on wasm and `std` everywhere else —
/// the same bargain [`super::session`] and [`super::account`] make, and for the same reason.
pub fn now_unix_ms() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|since| since.as_millis() as i64)
        .unwrap_or(0)
}

// ============================================================
// The patch.
// ============================================================

/// The edit form's three fields, as typed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProfileForm {
    pub handle: String,
    pub display_name: String,
    pub bio: String,
}

impl ProfileForm {
    /// The form as it opens on an existing profile.
    pub fn of(profile: &Profile) -> Self {
        Self {
            handle: profile.handle.clone().unwrap_or_default(),
            display_name: profile.display_name.clone(),
            bio: profile.bio.clone(),
        }
    }
}

/// The patch this form would send against this profile: the changed fields, and only those.
///
/// Three rules, all of them the beta's (`ProfileSettingsPage.tsx:110-115`), and the first is the
/// one worth stating:
///
/// * **An emptied handle is not a change.** There is no way to un-claim a handle — the function
///   has no branch that writes `null` — so a cleared field must not be sent as `""`, which the
///   format check would refuse and which would read to the user as the app breaking on a blank.
///   Clearing the box simply patches nothing, and the handle they have stays theirs.
/// * An emptied display name or bio IS a change; both columns are `NOT NULL DEFAULT ''` and both
///   are meant to be clearable.
/// * Values are compared trimmed, because trimmed is what the server stores — sending a name
///   that differs from the stored one only by a trailing space would be a write that changes
///   nothing and still re-stamps `updated_at`.
pub fn profile_patch(form: &ProfileForm, profile: &Profile) -> ProfileUpdate {
    let handle = normalize_handle(&form.handle)
        .filter(|next| Some(next.as_str()) != profile.handle.as_deref());
    let display_name = Some(form.display_name.trim().to_string())
        .filter(|next| next.as_str() != profile.display_name.trim());
    let bio = Some(form.bio.trim().to_string()).filter(|next| next.as_str() != profile.bio.trim());
    ProfileUpdate {
        handle,
        display_name,
        bio,
    }
}

/// The path an author's page lives at. One place, because the browser's route reader and every
/// link that points at it have to agree, and a `format!` at each call site is two strings that
/// can drift.
pub fn author_path(handle: &str) -> String {
    format!("/author/{handle}")
}

// ============================================================
// The calls.
// ============================================================

/// Read one profile by account id.
///
/// PostgREST rather than a function: `profiles` carries `USING (TRUE)` for SELECT — reads are
/// public by policy — so there is nothing for an edge function to guard here. `maybeSingle` in
/// the beta; a zero-row answer is `None` rather than an error, because a signed-in user whose
/// trigger-created row is missing is a real state and the surface says so in those terms.
pub async fn load(cloud: &Cloud, user_id: &str) -> Result<Option<Profile>, CloudError> {
    let filter = format!("eq.{user_id}");
    let rows: Vec<Profile> = cloud
        .select("profiles", "*", &[("user_id", filter.as_str())], None)
        .await?;
    Ok(rows.into_iter().next())
}

/// What `update-profile` answers with: the whole row, under one key.
#[derive(Debug, Deserialize)]
struct ProfileEnvelope {
    profile: Profile,
}

/// Write the patch, and republish the identity.
///
/// **The republish is the reason this takes an [`Account`] and not a [`Cloud`].** A profile edit
/// is the one write in this app that changes what other surfaces draw about the user without
/// changing the session they read it from — the access token is byte-identical before and after.
/// [`super::account::IdentityChange::UserUpdated`] exists for exactly this, and was put there by
/// RB4b with this row named; raising it re-runs every effect keyed on the user signal, which is
/// how a card drawn with the old display name redraws with the new one.
///
/// Nothing is validated here. See the module note: the server is the judge, and its refusal
/// arrives with its own words attached.
pub async fn update(account: &Account, patch: &ProfileUpdate) -> Result<Profile, CloudError> {
    let cloud = account.prepared().await?;
    let envelope: ProfileEnvelope = cloud.invoke("update-profile", patch).await?;
    account.user_updated().await;
    Ok(envelope.profile)
}

/// The shortest query worth sending. `profile.ts:117` — below this every author in the table is a
/// trigram match and the dropdown is a list of everyone.
pub const AUTHOR_SEARCH_MIN: usize = 2;

/// How many rows the dropdown asks for. The beta passes 8 at its one call site
/// (`BuildFilters.tsx:189`) against an RPC that defaults to 10; the call site's number is the one
/// that has been looked at.
pub const AUTHOR_SEARCH_LIMIT: usize = 8;

/// Fuzzy author lookup for the browser's author box.
///
/// A query under [`AUTHOR_SEARCH_MIN`] characters returns empty **without a call** rather than
/// returning empty from one — the RPC would answer, and the answer would be most of the table.
pub async fn search_authors(
    cloud: &Cloud,
    query: &str,
    limit: usize,
) -> Result<Vec<AuthorSearchResult>, CloudError> {
    let query = query.trim();
    if query.chars().count() < AUTHOR_SEARCH_MIN {
        return Ok(Vec::new());
    }
    cloud
        .rpc(
            "search_authors",
            &serde_json::json!({ "q": query, "lim": limit }),
        )
        .await
}

/// Resolve a handle to the author page behind it, or `None` if nobody holds it.
///
/// The RPC `RETURNS TABLE`, so PostgREST answers with an array — of one row or of none. An empty
/// array is a handle nobody has claimed, which is a 404 on the surface and not an error here.
pub async fn resolve_author(
    cloud: &Cloud,
    handle: &str,
) -> Result<Option<PublicAuthor>, CloudError> {
    let Some(handle) = normalize_handle(handle) else {
        return Ok(None);
    };
    let rows: Vec<PublicAuthor> = cloud
        .rpc("resolve_author", &serde_json::json!({ "h": handle }))
        .await?;
    Ok(rows.into_iter().next())
}

// ============================================================
// The surface.
// ============================================================

/// Raised by the main menu's profile row; owned by the shell.
#[derive(Clone, Copy)]
pub struct ProfileOpen(pub Signal<bool>);

/// Mounted by the shell above both layout roots. Renders nothing while closed.
#[component]
pub fn ProfileHost() -> Element {
    let mut open = use_context::<ProfileOpen>().0;
    if !open() {
        return rsx! {};
    }
    rsx! {
        Modal {
            title: "Your public profile".to_string(),
            size: ModalSize::Md,
            on_close: move |_| open.set(false),
            ProfileBody {}
        }
    }
}

/// The edit form.
///
/// The row is loaded here rather than passed in because it is the only surface that needs it and
/// because it must be re-read after a save — `update-profile` answers with the updated row, and
/// taking that answer as the new truth is what keeps `handle_changed_at` (and therefore the
/// cooldown) correct without a second request.
#[component]
fn ProfileBody() -> Element {
    let account = use_context::<Account>();
    let mut open = use_context::<ProfileOpen>().0;
    let user = account.user();

    // The loaded row, and the form over it. `None` until the load lands.
    let mut profile = use_signal(|| None::<Profile>);
    let mut form = use_signal(ProfileForm::default);
    let mut saving = use_signal(|| false);
    let mut refusal = use_signal(|| None::<String>);
    let mut saved = use_signal(|| false);

    let loaded = use_resource({
        let account = account.clone();
        move || {
            let account = account.clone();
            // Read inside, for the reason [`super::browser::AuthorModal`]'s twin carries: a value
            // lifted out above the closure is captured once, and this one would be `None` for any
            // mount that beats the session restore.
            let user_id = user.read().as_ref().map(|u| u.id.clone());
            async move {
                let Some(user_id) = user_id else {
                    return Ok(None);
                };
                let cloud = account.prepared().await?;
                let found = load(&cloud, &user_id).await?;
                if let Some(row) = &found {
                    form.set(ProfileForm::of(row));
                    profile.set(Some(row.clone()));
                }
                Ok::<_, CloudError>(found)
            }
        }
    });

    if user.read().is_none() {
        return rsx! {
            div { class: "load-state", "Sign in to set up a public profile." }
        };
    }

    // The signal is the truth once the load has landed. A save writes it from the envelope, and
    // the `UserUpdated` republish that same save raises re-runs the resource above — its closure
    // reads `user`, and that is the signal `publish` sets. So `loaded` goes back to pending on
    // every successful save, and reading it here would flash "Loading your profile…" over a row
    // already in hand. Only the signal's absence sends this to the resource, and then only for a
    // reason worth drawing.
    //
    // Both writers land the same values today — the re-read answers with the row the envelope
    // already carried — so the later one winning is invisible. It is a race all the same: a
    // server that normalised a field the envelope did not, or a user typing inside the window,
    // would see the re-read overwrite the form.
    let Some(current) = profile.read().clone() else {
        return match &*loaded.read() {
            None => rsx! { div { class: "load-state", "Loading your profile…" } },
            Some(Err(error)) => rsx! { div { class: "load-state error", "{error}" } },
            // The `handle_new_user` trigger creates this row at sign-up, so its absence is a real
            // account in a state this client cannot repair — said plainly rather than papered
            // over with an empty form that would fail on save.
            Some(Ok(_)) => rsx! {
                div { class: "load-state error",
                    "This account has no profile row. Signing out and back in recreates it."
                }
            },
        };
    };

    let draft = form.read().clone();
    let patch = profile_patch(&draft, &current);
    let dirty = !patch.is_empty();

    // Read off the patch rather than off the box, which is what scopes the check to what was
    // actually typed: `profile_patch` has already dropped an empty box and a handle that matches
    // the stored one. A fault on either would be this client calling the server's own accepted
    // value invalid, under a field nobody has touched.
    let fault = patch.handle.as_deref().and_then(handle_fault);

    let cooldown = handle_cooldown(current.handle_changed_at.as_deref(), now_unix_ms());
    // The first claim is never gated — `update-profile` only consults the window when there is a
    // handle to change (`update-profile/index.ts:125`), so a profile with none is always free.
    let locked = current.handle.is_some() && matches!(cooldown, Ok(HandleCooldown::Waiting { .. }));

    let name_length = draft.display_name.trim().chars().count();
    let bio_length = draft.bio.trim().chars().count();
    // A floor under `maxlength`, and deliberately the looser of the two. The attribute and
    // `update-profile`'s `String.length` both count UTF-16 code units; these count `char`s, and
    // an astral character is one here and two there — so this can only ever pass something the
    // attribute would have stopped, never block something the server would have taken. That is
    // the right direction for a mirrored rule: the counter beside the field is the honest number
    // for a human, and the refusal, if there is one, is still the server's.
    let over_long = name_length > DISPLAY_NAME_MAX || bio_length > BIO_MAX;
    let can_save = dirty && !saving() && fault.is_none() && !over_long;

    let handle_hint = match &cooldown {
        // The unreadable arm does NOT lock the field. A timestamp this client could not read is
        // no evidence of a cooldown, and locking on it would be the beta's "in NaN days" — a
        // control with no way out, derived from a value nobody can see. The server still holds
        // the window, so the worst case here is a 429 carrying the real number.
        Err(unreadable) => unreadable.to_string(),
        Ok(HandleCooldown::Waiting { days_left }) if current.handle.is_some() => {
            let day = if *days_left == 1 { "day" } else { "days" };
            format!("You can change your handle again in {days_left} {day}.")
        }
        _ if current.handle.is_some() => {
            format!(
                "Your author page is at {}.",
                author_path(current.handle.as_deref().unwrap_or_default())
            )
        }
        _ => "Pick a handle to give yourself a public author page.".to_string(),
    };

    let save_now = {
        let account = account.clone();
        move |_| {
            let account = account.clone();
            let patch = patch.clone();
            async move {
                if saving() {
                    return;
                }
                saving.set(true);
                refusal.set(None);
                saved.set(false);
                match update(&account, &patch).await {
                    Ok(updated) => {
                        // The server's answer replaces the row wholesale — it carries the new
                        // `handle_changed_at`, which is what the cooldown above reads next frame.
                        form.set(ProfileForm::of(&updated));
                        profile.set(Some(updated));
                        saved.set(true);
                    }
                    Err(error) => refusal.set(Some(error.to_string())),
                }
                saving.set(false);
            }
        }
    };

    rsx! {
        div { class: "sb-profile",
            div { class: "sb-profile__identity",
                if let Some(avatar) = current.avatar_url.as_deref().and_then(avatar_src) {
                    img { class: "sb-profile__avatar", src: "{avatar}", alt: "" }
                } else {
                    div { class: "sb-profile__avatar sb-profile__avatar--none" }
                }
                div { class: "sb-profile__who",
                    if let Some(discord) = current.discord_username.as_deref() {
                        p { class: "sb-profile__discord", "{discord}" }
                    }
                    p { class: "sb-profile__note",
                        "Your avatar and Discord name come from Discord and are refreshed each time you save."
                    }
                }
            }

            label { class: "field-label", "Handle" }
            input {
                class: "sb-save__input",
                value: "{draft.handle}",
                disabled: locked,
                maxlength: HANDLE_MAX as i64,
                placeholder: "your-handle",
                oninput: move |e| {
                    let value = e.value();
                    form.write().handle = value;
                    saved.set(false);
                },
            }
            if let Some(fault) = fault {
                p { class: "sb-save__error", "{fault.message()}" }
            }
            p { class: "sb-save__hint", "{handle_hint}" }

            label { class: "field-label", "Display name" }
            input {
                class: "sb-save__input",
                value: "{draft.display_name}",
                maxlength: DISPLAY_NAME_MAX as i64,
                placeholder: "What should your builds be signed with?",
                oninput: move |e| {
                    let value = e.value();
                    form.write().display_name = value;
                    saved.set(false);
                },
            }
            p { class: "sb-save__hint", "{name_length}/{DISPLAY_NAME_MAX} — shown on every build you share." }

            label { class: "field-label", "Bio" }
            textarea {
                class: "sb-save__input sb-save__input--area",
                value: "{draft.bio}",
                maxlength: BIO_MAX as i64,
                placeholder: "Who are you, and what do you build?",
                oninput: move |e| {
                    let value = e.value();
                    form.write().bio = value;
                    saved.set(false);
                },
            }
            p { class: "sb-save__hint", "{bio_length}/{BIO_MAX}" }

            if let Some(reason) = refusal() {
                p { class: "sb-save__error", "{reason}" }
            }

            div { class: "sb-detail__actions",
                button {
                    class: "seg",
                    r#type: "button",
                    onclick: move |_| open.set(false),
                    "Close"
                }
                button {
                    class: "seg is-primary",
                    r#type: "button",
                    disabled: !can_save,
                    onclick: save_now,
                    if saving() { "Saving…" } else { "Save changes" }
                }
                if saved() && !dirty {
                    span { class: "sb-profile__saved", "Saved." }
                }
            }
        }
    }
}
