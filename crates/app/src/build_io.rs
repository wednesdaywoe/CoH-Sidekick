//! Build I/O — the `.skif` file in and out of the planner, as focused flows off the main menu
//! rather than a hub modal. That menu is also where the other ways a build leaves hang: the forum
//! post lives in [`crate::forum_export`], reached from the same row list.
//!
//! What this module owns is the entries and the flows behind them, not the menu — see
//! [`BuildFileEntries`] and [`BuildHandoffEntries`], which [`crate::main_menu`] composes.
//!
//! The codec has been complete since it landed ([`coh_data::skif`], v5 plus the v2/v3/v4 legacy
//! tier) and nothing could reach it: a build could be edited and auto-saved to this browser and
//! nowhere else. This module is the door.
//!
//! **Opening is a two-step act, and the step in the middle is the whole difficulty.** Both
//! readers take a [`PowerDatabase`], the app holds exactly one fork at a time, and a file
//! authored on another fork resolves every powerset, power and set piece against definitions
//! that do not carry it — which rule 8 faithfully retains and reports, turning a good build into
//! a page of unresolved entries. So a file's own fork is read first ([`skif::probe_dataset`]),
//! and where it is not the loaded one the text is PARKED while the right bundle loads.
//! [`open_route`] is that decision, kept pure and out of the component.
//!
//! **The park has to outlast the restore, not race it.** A dataset switch reloads the bundle and
//! then the shell restores that fork's own stored build over the working one; a parked file
//! decoded before that lands would be overwritten by the build it just replaced. The wait is
//! therefore on `loaded_for` — the shell's record that the restore for this fork has *finished*
//! — and not on the bundle being present.
//!
//! **A file from another fork is a QUESTION, not a route.** Reloading onto the file's own fork
//! is right when the user simply isn't on that server today. It is wrong when the file is the
//! thing being compared — a live Homecoming build read against Brainstorm to see what the next
//! patch does to it. Neither answer is safe as the other's default, so [`CrossForkChoice`] parks
//! the file and asks, and [`adopt`] performs whichever act was chosen.
//!
//! **A build ported to another fork is re-stamped to it.** `CharacterState::dataset` is not a
//! label: perma eligibility and the forum export read it, and the beta's engine calculates
//! against its twin. A ported build still carrying the fork it came from computes as one server
//! under the name of another, which is the one failure this whole flow exists to prevent.
//!
//! **A file becomes the build through [`BuildSession::commit`]**, like any other edit, so
//! opening one is undoable and is persisted by the same path everything else writes through.
//! Replacing a build wholesale is exactly the edit undo exists for.

use crate::build_file::{self, Document};
use crate::build_session::BuildSession;
use crate::export_image::ExportImageEntry;
use crate::modal::{Modal, ModalSize};
use crate::shell::Db;
use coh_data::import_link::{self, LinkError, LinkPayload};
use coh_data::skif::{self, Unresolved};
use coh_data::{game_export, mbd, CharacterState, DatasetId, PowerDatabase};
use dioxus::prelude::*;

/// A file read off disk and waiting for its own fork's bundle, plus the restore that follows it.
#[derive(Clone, PartialEq)]
pub struct PendingImport {
    pub dataset: DatasetId,
    pub file_name: String,
    pub text: String,
}

/// The parked file. Provided by the shell, written by the menu, read by [`BuildIoHost`].
#[derive(Clone, Copy)]
pub struct BuildIoPending(pub Signal<Option<PendingImport>>);

/// A file whose fork is not the loaded one, held until the user says which open they meant.
///
/// The raw text is held rather than anything decoded from it: the port has to hydrate the
/// ORIGINAL file against the loaded fork, and the switch has to carry it across the reload
/// untouched.
#[derive(Clone, PartialEq)]
pub struct CrossForkChoice {
    /// The fork the file names.
    pub fork: DatasetId,
    pub file_name: String,
    pub text: String,
}

/// The parked question, or `None`. Provided by the shell, written by the menu.
#[derive(Clone, Copy)]
pub struct BuildIoChoice(pub Signal<Option<CrossForkChoice>>);

/// What an attempt to open or save has to tell the user.
///
/// Every open reports, including a clean one — the build the user was looking at has just been
/// replaced in full, and a receipt naming the file is what says which one they now have. A save
/// reports only when it fails: the browser's own download and the desktop's save dialog are each
/// their own receipt, and a modal confirming what the user just watched happen is noise.
#[derive(Clone, PartialEq)]
pub enum BuildIoOutcome {
    Opened {
        file_name: String,
        /// Picks that arrived, so "some of it read" is distinguishable from "all of it".
        picks: usize,
        /// Everything the file named that this dataset does not carry. Retained in the build
        /// (rule 8), never dropped — so this is a list of things to look at, not of losses.
        unresolved: Vec<Unresolved>,
        /// The text it came from, where the document named no fork and was therefore read
        /// against whatever was loaded. `None` for a `.skif`, which names its own and has
        /// already been routed by it.
        ///
        /// Kept because it is the only thing that can answer the list above: if a `/buildsave`
        /// export resolved badly, the fork was probably wrong, and reading it somewhere else
        /// needs the original bytes rather than the build they became.
        unstamped: Option<String>,
    },
    Refused {
        /// The act that could not complete, in the user's terms ("Open Stone Fist.skif").
        action: String,
        /// Why, in the words of whatever refused it.
        reason: String,
    },
}

/// The outcome awaiting the user, or `None`. Provided by the shell, written by every flow here.
#[derive(Clone, Copy)]
pub struct BuildIoReport(pub Signal<Option<BuildIoOutcome>>);

// ============================================================
// The decision.
// ============================================================

/// What has to happen before a file's text can be decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenRoute {
    /// The fork this file needs is the one already loaded.
    Decode,
    /// Load this fork first. Decoding against the loaded one would resolve nothing.
    SwitchTo(DatasetId),
    /// The document names no fork and never will — the game client writes one build format and
    /// has no idea other servers exist. Read it against whatever is loaded; what fails to
    /// resolve is the report, and the report is what lets the user say it was the wrong one.
    DecodeUnstamped,
    /// No loaded fork answers to what the file names, so no switch can help. The text goes to
    /// a reader anyway, which refuses it in words that name the fork — a refusal belongs to the
    /// decoder that can state it precisely, not to a route.
    LetTheReaderRefuse,
}

/// Read where a file has to be decoded, given the fork the app currently holds.
///
/// The one decision whose wrong answer is not an error message: a file decoded against the
/// wrong fork produces a build that is *structurally* fine and contains nothing, because every
/// id in it resolved against definitions that never had it.
pub fn open_route(text: &str, loaded: DatasetId) -> OpenRoute {
    // Each format is asked for its fork by the reader that owns it, and the `None`s do not
    // mean the same thing — which is why the formats are separated before the probe rather
    // than after it, and why this cannot be one probe returning an `Option`.
    match build_file::document_kind(text) {
        // The game client's export names no fork and never will.
        Document::GameExport => OpenRoute::DecodeUnstamped,
        // Neither does a `.mxd`, and for a stronger reason than the export's: that format has no
        // field for one. It predates the forks, and its data block carries an archetype class,
        // eight powerset paths and nothing that says whose game they are from. So it reads
        // against the loaded fork, and the receipt is what says it was the wrong one.
        Document::Mxd => OpenRoute::DecodeUnstamped,
        // The client knows its own server only, and never names it.
        Document::ClientBuild => OpenRoute::DecodeUnstamped,
        // A `.skif` naming a fork nothing carries is a file this build cannot serve. No switch
        // can help, so the text goes to a reader that can refuse it in those words.
        Document::Skif => match skif::probe_dataset(text) {
            Some(dataset) if dataset == loaded => OpenRoute::Decode,
            Some(dataset) => OpenRoute::SwitchTo(dataset),
            None => OpenRoute::LetTheReaderRefuse,
        },
        // A `.mbd`'s `None` is the game export's, not the `.skif`'s: it means the file names a
        // Mids database no fork carries as its OWN, which is what a build authored in Mids'
        // `Generic` database says — and Thunderspy's writer stamps `Generic` deliberately,
        // because a file naming `Thunderspy` kills Mids outright (MBDEXPORT-2). So the honest
        // answer is the fork the app is holding, with the receipt offering to read it
        // elsewhere. Refusing here would refuse every Thunderspy `.mbd` in existence.
        Document::Mbd => match mbd::probe_dataset(text) {
            Some(dataset) if dataset == loaded => OpenRoute::Decode,
            Some(dataset) => OpenRoute::SwitchTo(dataset),
            None => OpenRoute::DecodeUnstamped,
        },
    }
}

/// Read a build's text out of whatever the user handed over: a share URL, the bare fragment
/// from one, or a document as it stands.
///
/// Telling those apart is not the user's job — they copy an address bar, or a chat message, or
/// the contents of a file — so this is the one place that decides, and all three doors into the
/// app go through it.
///
/// A document is claimed FIRST, before anything is treated as a link. A `.skif` can hold a `#`
/// anywhere in it and a character's name is allowed one, so splitting on `#` before asking
/// whether this is already a build would cut a pasted document in half.
pub fn read_pasted(pasted: &str) -> Result<String, LinkError> {
    let trimmed = pasted.trim();
    if build_file::is_json_document(trimmed) || game_export::parse(trimmed).is_ok() {
        return Ok(trimmed.to_string());
    }
    let fragment = trimmed.split_once('#').map_or(trimmed, |(_, tail)| tail);
    match import_link::decode_fragment(fragment)? {
        LinkPayload::Json(text) => Ok(text),
        LinkPayload::GameExport { text, .. } => Ok(text),
    }
}

/// How many powers a build holds, across every partition that can hold one.
///
/// The receipt's one number. Counted rather than taken from the file so it describes what
/// arrived in the build, which is the question a reader of the receipt is asking.
fn pick_count(build: &CharacterState) -> usize {
    build.primary.powers.len()
        + build.secondary.powers.len()
        + build
            .pools
            .iter()
            .chain(build.epic_pool.iter())
            .map(|pool| pool.powers.len())
            .sum::<usize>()
}

// ============================================================
// The acts.
// ============================================================

/// Decode a file, and stamp the result with `fork` where one is given.
///
/// Split out of [`adopt_as`] so the stamp is code a test can drive: everything around it in
/// that function needs a Dioxus runtime, and the one line that matters here does not.
///
/// The decoder is faithful to the file — it stamps the build with the fork the FILE names,
/// whichever database it was handed — so this is where a port corrects it, and the only place.
///
/// `loaded` and `fork` are different questions and both are needed. `loaded` is what a document
/// naming no fork gets READ against; `fork` is the stamp a port writes afterwards. They are the
/// same value on a port and unrelated everywhere else.
pub fn decode_stamped(
    text: &str,
    database: &PowerDatabase,
    loaded: DatasetId,
    fork: Option<DatasetId>,
) -> Result<coh_data::skif::Decoded, build_file::ImportError> {
    let mut decoded = build_file::decode_import(text, database, loaded)?;
    if let Some(fork) = fork {
        decoded.build.dataset = fork;
    }
    Ok(decoded)
}

/// Decode `text` into the working build, or report why it could not be.
///
/// The single place a file becomes the build, shared by the decode-now and the decode-after-a-
/// switch paths so the two cannot diverge on what opening a file means.
///
/// The two syncs are the same pair the shell runs when it restores a build from storage, for the
/// same reason: a `.skif` stores no granted or locked selection (rule 6 — anything the reader can
/// compute is never stored), so the grants have to be materialized against the fork now loaded
/// before the build is anyone's.
pub fn adopt(
    text: &str,
    file_name: &str,
    database: &PowerDatabase,
    loaded: DatasetId,
    session: BuildSession,
    report: Signal<Option<BuildIoOutcome>>,
) {
    adopt_as(text, file_name, database, loaded, None, session, report);
}

/// [`adopt`], plus the fork to stamp the build with.
///
/// `Some(fork)` is a PORT: the file named another fork, the user chose to read it here anyway,
/// and the build now belongs to the fork it was read against. Leaving the file's own stamp on
/// it would leave `dataset` naming a fork whose definitions produced none of these numbers —
/// and `dataset` is read (perma eligibility, the forum export's header), not decoration.
#[allow(clippy::too_many_arguments)]
pub fn adopt_as(
    text: &str,
    file_name: &str,
    database: &PowerDatabase,
    loaded: DatasetId,
    stamp: Option<DatasetId>,
    session: BuildSession,
    mut report: Signal<Option<BuildIoOutcome>>,
) {
    match decode_stamped(text, database, loaded, stamp) {
        Ok(decoded) => {
            let mut build = decoded.build;
            crate::inherents::sync(&mut build, database);
            crate::granted_powers::sync(&mut build, database);
            let picks = pick_count(&build);
            session.commit(move |working| *working = build);
            report.set(Some(BuildIoOutcome::Opened {
                file_name: file_name.to_string(),
                picks,
                unresolved: decoded.unresolved,
                // "Names no fork", not "is not a `.skif`" — the question the receipt's offer
                // to read it elsewhere is answering. A `.mbd` authored in Mids' `Generic`
                // database names none either, and that is the file every Thunderspy build
                // arrives as.
                unstamped: (open_route(text, loaded) == OpenRoute::DecodeUnstamped)
                    .then(|| text.to_string()),
            }));
        }
        Err(error) => report.set(Some(BuildIoOutcome::Refused {
            action: format!("Open {file_name}"),
            reason: error.to_string(),
        })),
    }
}

/// Take a build's text into the app: decode it, park it while its fork loads, or ask which open
/// was meant.
///
/// The single place the three ways in converge — a file off the input, a paste, and the URL
/// fragment the shell reads at boot — so none of them can grow its own idea of what opening a
/// build means. The route is read BEFORE the database is consulted, because a probe needs no
/// definitions: a link arrives before the bundle does, and parking it under the loaded fork
/// would decode a Rebirth build against Homecoming the moment that bundle landed.
#[allow(clippy::too_many_arguments)]
/// Decode `text` into the working build, or report why it could not be, parking it when a fork
/// switch comes first.
///
/// `pub(crate)`: the shared-builds detail view (RB4d) loads a shared build's `build_json`
/// through this same door, so a load and an open behave identically — including parking an
/// answer when the text belongs to a fork the app is not holding.
pub(crate) fn take_build(
    text: String,
    file_name: String,
    database: Option<Db>,
    dataset: Signal<DatasetId>,
    session: BuildSession,
    mut pending: Signal<Option<PendingImport>>,
    mut choice: Signal<Option<CrossForkChoice>>,
    report: Signal<Option<BuildIoOutcome>>,
) {
    let loaded = dataset();
    match (database, open_route(&text, loaded)) {
        (_, OpenRoute::SwitchTo(fork)) => choice.set(Some(CrossForkChoice {
            fork,
            file_name,
            text,
        })),
        // No database means the bundle is still loading, which is also the one case where
        // parking the text is the whole answer.
        (None, _) => pending.set(Some(PendingImport {
            dataset: loaded,
            file_name,
            text,
        })),
        (Some(database), _) => adopt(&text, &file_name, &database, loaded, session, report),
    }
}

// ============================================================
// The entries, for the main menu to compose.
// ============================================================

/// Everything a build does as a FILE, as menu entries rather than as a menu.
///
/// A menu rather than a hub modal (the beta's `ExportImportModal` put save, load, import, forum
/// export, image export and the cloud vault behind one dialog with tabs). Each entry is one act
/// with one outcome — the one exception being Share, which opens a surface because a post has
/// choices to make before it is a post. What is not built yet is shown disabled with its reason,
/// the same way the rest of the menu's unbuilt actions are, because an action you cannot see is
/// one you have to remember exists.
///
/// Offered as two groups rather than one component, because the menu that hosts them
/// ([`crate::main_menu`]) orders itself by CONSEQUENCE and these two groups sit either side of
/// its steepest step: everything here changes which build you are looking at, everything in
/// [`BuildHandoffEntries`] leaves it alone and hands a copy to someone else. A single entries
/// component would have put that hairline inside a black box.
///
/// Each entry closes the menu itself, through [`crate::popover::PopoverOpen`], so neither group
/// needs to know it is in a popover at all.
#[component]
pub fn BuildFileEntries(database: Option<Db>, dataset: Signal<DatasetId>) -> Element {
    rsx! {
        NewBuildEntry { dataset }
        OpenFileEntry { database: database.clone(), dataset }
        PasteBuildEntry {}
        SaveFileEntry { database }
    }
}

/// The acts that hand this build to someone who does not run this planner — a post, an image,
/// and (not yet) another planner's file read back the other way. None of them touch the build.
#[component]
pub fn BuildHandoffEntries(database: Option<Db>) -> Element {
    rsx! {
        SaveToCloudEntry { database: database.clone() }
        CopyShortLinkEntry { database: database.clone() }
        ShareToForumEntry { database: database.clone() }
        ExportImageEntry { database }
        UnbuiltEntries {}
    }
}

/// Start over. Two clicks, because it discards a build and the second click is where the first
/// one is stated back — an in-place confirm rather than a modal over a popover, which is two
/// overlays deep for a question with two words in it.
///
/// It commits like any edit, so it is undoable and the discarded build is one Ctrl+Z away for as
/// long as the session lasts. That is what makes one confirm enough.
#[component]
fn NewBuildEntry(dataset: Signal<DatasetId>) -> Element {
    let session = use_context::<BuildSession>();
    let mut menu = use_context::<crate::popover::PopoverOpen>().0;
    let mut confirming = use_signal(|| false);

    if confirming() {
        return rsx! {
            div { class: "main-menu__confirm",
                span { class: "main-menu__confirm-text", "Discard this build and start over?" }
                div { class: "main-menu__confirm-actions",
                    button {
                        class: "seg",
                        r#type: "button",
                        onclick: move |_| confirming.set(false),
                        "Keep it"
                    }
                    button {
                        class: "seg is-destructive",
                        r#type: "button",
                        onclick: move |_| {
                            let empty = CharacterState::empty(dataset());
                            session.commit(move |working| *working = empty);
                            confirming.set(false);
                            menu.set(false);
                        },
                        "Start over"
                    }
                }
            }
        };
    }

    rsx! {
        button {
            class: "main-menu__item",
            r#type: "button",
            onclick: move |_| confirming.set(true),
            span { class: "main-menu__label", "New build" }
            span { class: "main-menu__hint", "Clears every pick and slot" }
        }
    }
}

/// Open a build file. A label over a hidden file input, because that input is the only thing
/// that opens a file picker the user's own platform owns — the browser's on the web, and a
/// native one on the desktop, where `dioxus-desktop` intercepts the click.
///
/// One entry for every format rather than one each, because it is one act with one outcome:
/// which reader a text belongs to is [`build_file::document_kind`]'s question and it answers it
/// from the document, so a second menu row would only ask the user to answer it first. That is
/// why `.mbd` landed here rather than behind the `Import from Mids…` row this menu used to
/// reserve for it: a `.mbd` also arrives by paste, and a menu row cannot own a decision two of
/// the three doors never reach.
///
/// The input is re-keyed after every read. An `<input type="file">` fires `change` only when its
/// value changes, so opening the same file twice in a row is silently ignored otherwise — and
/// "I clicked it and nothing happened" is indistinguishable from a broken import.
/// Every extension a reader here owns, and the two MIME types the same files arrive under.
///
/// The dialog filters by this list, so **a format the router can read is one a user cannot reach
/// if it is not named** — and the failure looks like the file not existing rather than like a
/// missing feature. Named as a constant so the routing test can assert the pair agree;
/// `text/plain` lets a `.mxd` through on some platforms and not on others, which is exactly the
/// "works on my machine" this list exists to settle.
const OPEN_FILE_ACCEPT: &str = ".skif,.mbd,.mxd,.txt,application/json,text/plain";

#[component]
fn OpenFileEntry(database: Option<Db>, dataset: Signal<DatasetId>) -> Element {
    let session = use_context::<BuildSession>();
    let pending = use_context::<BuildIoPending>().0;
    let choice = use_context::<BuildIoChoice>().0;
    let mut report = use_context::<BuildIoReport>().0;
    let mut menu = use_context::<crate::popover::PopoverOpen>().0;
    let mut reads = use_signal(|| 0_u32);

    rsx! {
        label { class: "main-menu__item main-menu__item--file",
            span { class: "main-menu__label", "Open file…" }
            span { class: "main-menu__hint",
                "A .skif, a Mids .mbd or .mxd, or the .txt the game's /buildsave wrote"
            }
            input {
                key: "{reads()}",
                class: "main-menu__file-input",
                r#type: "file",
                accept: OPEN_FILE_ACCEPT,
                onchange: move |evt| {
                    let database = database.clone();
                    async move {
                        let Some(file) = evt.files().into_iter().next() else {
                            return;
                        };
                        let next = reads.peek().wrapping_add(1);
                        reads.set(next);
                        let file_name = file.name();
                        match file.read_string().await {
                            Err(_) => {
                                report
                                    .set(
                                        Some(BuildIoOutcome::Refused {
                                            action: format!("Open {file_name}"),
                                            reason: "the file could not be read as text"
                                                .to_string(),
                                        }),
                                    );
                            }
                            Ok(text) => {
                                take_build(
                                    text,
                                    file_name,
                                    database,
                                    dataset,
                                    session,
                                    pending,
                                    choice,
                                    report,
                                );
                            }
                        }
                        // LAST, and never mid-flight: closing the menu unmounts this component,
                        // and a Dioxus scope owns the futures its handlers return — dropping the
                        // scope cancels this one where it stands. Closing before the `await`
                        // above silently swallowed every file the user opened.
                        menu.set(false);
                    }
                },
            }
        }
    }
}

/// Whether the paste surface is open. Provided by the shell, raised by [`PasteBuildEntry`].
#[derive(Clone, Copy)]
pub struct PasteBuildOpen(pub Signal<bool>);

/// Take a build out of the clipboard: a share link, or the text `/buildsave` wrote.
///
/// The door the file input cannot be. A share link is not a file and never becomes one, and on
/// the desktop there is no address bar to paste it into either — so this is the only way a link
/// reaches the desktop app at all, and the fragment the shell reads at boot is the browser's
/// convenience on top of it rather than the way in.
///
/// Not disabled while the bundle loads, unlike the acts around it: a pasted build parks exactly
/// as an opened file does, and refusing a paste for the two seconds a fetch takes would be the
/// app being unavailable for no reason.
#[component]
fn PasteBuildEntry() -> Element {
    let mut paste = use_context::<PasteBuildOpen>().0;
    let mut menu = use_context::<crate::popover::PopoverOpen>().0;

    rsx! {
        button {
            class: "main-menu__item",
            r#type: "button",
            onclick: move |_| {
                paste.set(true);
                menu.set(false);
            },
            span { class: "main-menu__label", "Paste a build…" }
            span { class: "main-menu__hint", "A share link, or a build's text pasted whole" }
        }
    }
}

/// The paste surface, hosted at the shell root like every other overlay.
///
/// A refusal is shown here rather than through [`BuildIoReportHost`], because the box is still
/// open and the text is still in it: what the user needs is to see what was wrong with what
/// they pasted and fix it, not a second modal over the first telling them to start again.
#[component]
pub fn PasteBuildHost(database: Option<Db>, dataset: Signal<DatasetId>) -> Element {
    let session = use_context::<BuildSession>();
    let mut open = use_context::<PasteBuildOpen>().0;
    let pending = use_context::<BuildIoPending>().0;
    let choice = use_context::<BuildIoChoice>().0;
    let report = use_context::<BuildIoReport>().0;
    let mut pasted = use_signal(String::new);
    let mut refusal = use_signal(|| Option::<String>::None);

    if !open() {
        return rsx! {};
    }

    let mut close = move || {
        pasted.set(String::new());
        refusal.set(None);
        open.set(false);
    };
    let take = move |_| match read_pasted(&pasted()) {
        Ok(document) => {
            take_build(
                document,
                "the pasted build".to_string(),
                database.clone(),
                dataset,
                session,
                pending,
                choice,
                report,
            );
            close();
        }
        Err(error) => refusal.set(Some(error.to_string())),
    };

    rsx! {
        Modal {
            title: "Paste a build".to_string(),
            size: ModalSize::Md,
            on_close: move |_| close(),
            div { class: "build-report",
                p { class: "build-report__note",
                    "A share link, the part of one after the #, or a whole build's text; a "
                    ".skif, a Mids .mbd, or what /buildsave wrote."
                }
                textarea {
                    class: "build-paste__box mono",
                    spellcheck: false,
                    autofocus: true,
                    placeholder: "Paste here",
                    "aria-label": "The build to read",
                    value: "{pasted}",
                    oninput: move |evt| {
                        pasted.set(evt.value());
                        refusal.set(None);
                    },
                }
                if let Some(reason) = refusal() {
                    div { class: "load-state error", "{reason}" }
                }
                div { class: "build-report__actions",
                    button {
                        class: "seg active",
                        r#type: "button",
                        disabled: pasted.read().trim().is_empty(),
                        onclick: take,
                        "Read it"
                    }
                    button { class: "seg", r#type: "button", onclick: move |_| close(), "Cancel" }
                }
            }
        }
    }
}

/// Write the build out as a `.skif`. Where it lands is the platform's business (see
/// [`build_file::write_file`]); what it is called is [`build_file::file_name_for`].
#[component]
fn SaveFileEntry(database: Option<Db>) -> Element {
    let session = use_context::<BuildSession>();
    let mut report = use_context::<BuildIoReport>().0;
    let mut menu = use_context::<crate::popover::PopoverOpen>().0;

    rsx! {
        button {
            class: "main-menu__item",
            r#type: "button",
            disabled: database.is_none(),
            onclick: move |_| {
                let database = database.clone();
                async move {
                    let Some(database) = database else {
                        return;
                    };
                    // Both derived before the await: a `Signal` read guard held across one
                    // would outlive the render it belongs to.
                    let file_name = build_file::file_name_for(&session.build.read());
                    let encoded = skif::encode(&session.build.read(), &database);
                    let written = match encoded {
                        Ok(text) => build_file::write_file(&file_name, &text).await.err(),
                        Err(error) => Some(error.to_string()),
                    };
                    if let Some(reason) = written {
                        report
                            .set(
                                Some(BuildIoOutcome::Refused {
                                    action: format!("Save {file_name}"),
                                    reason,
                                }),
                            );
                    }
                    // LAST, for the reason `OpenFileEntry` states: closing the menu unmounts
                    // this component, and that cancels the future it is standing in — which on
                    // the desktop is the save dialog itself.
                    menu.set(false);
                }
            },
            span { class: "main-menu__label", "Save to file…" }
            span { class: "main-menu__hint", "The whole build, in this planner's own format" }
        }
    }
}

/// Hand the build to someone who does not run this planner.
///
/// Unlike the two acts above it opens a surface rather than completing: the post has three
/// formats and three optional sections, and which one a destination wants is the user's to see
/// before they paste. See [`crate::forum_export`].
#[component]
fn ShareToForumEntry(database: Option<Db>) -> Element {
    let mut share = use_context::<crate::forum_export::ForumExportOpen>().0;
    let mut menu = use_context::<crate::popover::PopoverOpen>().0;

    rsx! {
        button {
            class: "main-menu__item",
            r#type: "button",
            disabled: database.is_none(),
            onclick: move |_| {
                share.set(true);
                menu.set(false);
            },
            span { class: "main-menu__label", "Share to a forum…" }
            span { class: "main-menu__hint", "The build as a post — BBCode, Markdown or plain" }
        }
    }
}

/// Save the build to the shared repository (RB4e).
///
/// Opens a surface for the same reason the forum export does: a shared build has a name, a
/// description, tags and a visibility, and none of those have an answer the app could pick. The
/// visibility least of all — it is the difference between a build in a personal vault and one
/// listed publicly under the user's name.
#[component]
fn SaveToCloudEntry(database: Option<Db>) -> Element {
    let mut save = use_context::<crate::cloud::save_build::SaveBuildOpen>().0;
    let mut menu = use_context::<crate::popover::PopoverOpen>().0;

    rsx! {
        button {
            class: "main-menu__item",
            r#type: "button",
            disabled: database.is_none(),
            onclick: move |_| {
                save.set(true);
                menu.set(false);
            },
            span { class: "main-menu__label", "Save to the cloud…" }
            span { class: "main-menu__hint", "Keep it in your vault, or share it with everyone" }
        }
    }
}

/// The one-click unlisted short link — the act the forum export cannot offer.
///
/// No surface, on purpose: that is the whole point of it. One click puts a URL on the clipboard,
/// and pressing it again on an unchanged build re-copies the same URL without touching the
/// network at all ([`crate::cloud::save_build::quick_share`] has the cache rules).
///
/// **Success keeps the menu open and flips the label; only a failure closes it.** A clipboard
/// copy that worked does not deserve a modal — the receipt host is a `Modal`, and popping one
/// over a one-click act would cost more clicks than the act saved. "Copied!" in place is the
/// same feedback the shared-build detail's own copy button gives, so the two agree. A REFUSAL is
/// a different matter: it has a reason, the reason is the only evidence there is, and the
/// receipt is where every other act in this menu puts one.
///
/// Account-only, and shown disabled with its reason rather than hidden: an unlisted row has to be
/// reclaimable by whoever made it, and an anonymous caller has no identity to reclaim it with —
/// which is why the server forces an anonymous save public.
#[component]
fn CopyShortLinkEntry(database: Option<Db>) -> Element {
    let account = use_context::<crate::cloud::account::Account>();
    let session = use_context::<BuildSession>();
    let totals = use_context::<crate::panels::stats::BuildTotals>().0;
    let mut report = use_context::<BuildIoReport>().0;
    let mut menu = use_context::<crate::popover::PopoverOpen>().0;
    let mut working = use_signal(|| false);
    let mut copied = use_signal(|| false);

    let user = account.user();
    let signed_in = user.read().is_some();

    rsx! {
        button {
            class: "main-menu__item",
            r#type: "button",
            disabled: database.is_none() || !signed_in || working(),
            title: if signed_in { "" } else { "Sign in to create a short link" },
            onclick: {
                let database = database.clone();
                let account = account.clone();
                move |_| {
                    let database = database.clone();
                    let account = account.clone();
                    async move {
                        let Some(database) = database else { return };
                        working.set(true);
                        let build = session.build.peek().clone();
                        let computed = totals.peek().clone();
                        let author = user
                            .peek()
                            .as_ref()
                            .and_then(|u| u.display_name.clone())
                            .unwrap_or_default();
                        let shared = crate::cloud::save_build::quick_share(
                            &account,
                            &build,
                            &database,
                            &computed,
                            &author,
                            signed_in,
                        )
                        .await;
                        let refusal = match shared {
                            Err(reason) => Some(reason.to_string()),
                            Ok(id) => crate::cloud::browser::copy_url(&id).await.err(),
                        };
                        working.set(false);
                        match refusal {
                            None => copied.set(true),
                            Some(reason) => {
                                report.set(Some(BuildIoOutcome::Refused {
                                    action: "Copying a short link".to_string(),
                                    reason,
                                }));
                                menu.set(false);
                            }
                        }
                    }
                }
            },
            span { class: "main-menu__label",
                if working() {
                    "Copying…"
                } else if copied() {
                    "Copied!"
                } else {
                    "Copy short link"
                }
            }
            span { class: "main-menu__hint", "A one-click unlisted URL, on the clipboard" }
        }
    }
}

/// One flow off this menu that is not built yet.
struct UnbuiltFlow {
    label: &'static str,
    /// Why it is inert, as a whole sentence — it goes to `title` verbatim, because a disabled
    /// row says unavailable and does not say why.
    why: &'static str,
}

/// The rest of the beta's export/import surface, shown rather than omitted.
///
/// **Empty, and kept rather than deleted.** Both Mids formats now open through `Open file…`
/// with everything else — which reader owns a text is `build_file::document_kind`'s question,
/// answered from the document every time, so a menu row per format would only have asked the
/// user to answer it first. The row this held last was `Open a Mids .mxd…`; the type and the
/// renderer stay because the next unbuilt flow should cost one line rather than a component.
const UNBUILT: [UnbuiltFlow; 0] = [];

#[component]
fn UnbuiltEntries() -> Element {
    rsx! {
        for flow in UNBUILT {
            button {
                key: "{flow.label}",
                class: "main-menu__item",
                r#type: "button",
                disabled: true,
                title: "{flow.why}",
                span { class: "main-menu__label", "{flow.label}" }
            }
        }
    }
}

// ============================================================
// The question.
// ============================================================

/// Ask which open the user meant, for a file whose fork is not the loaded one.
///
/// Mounted at the shell root beside [`BuildIoReportHost`], and for the same reason: a modal
/// hosted inside a free-grid surface is contained by that surface's `transform`.
///
/// The two answers are deliberately both spelled out rather than one being a plain confirm.
/// "Open on Rebirth" throws away the session's current fork; "Read it on Homecoming" throws
/// away nothing but produces numbers the file's author would not recognize. A default here
/// would be this module guessing which of those the user came for.
#[component]
pub fn BuildIoChoiceHost(database: Option<Db>, dataset: Signal<DatasetId>) -> Element {
    let session = use_context::<BuildSession>();
    let mut choice = use_context::<BuildIoChoice>().0;
    let mut pending = use_context::<BuildIoPending>().0;
    let report = use_context::<BuildIoReport>().0;

    let Some(parked) = choice.read().clone() else {
        return rsx! {};
    };
    let loaded = dataset();

    let switch = {
        let parked = parked.clone();
        move |_| {
            let mut dataset = dataset;
            pending.set(Some(PendingImport {
                dataset: parked.fork,
                file_name: parked.file_name.clone(),
                text: parked.text.clone(),
            }));
            dataset.set(parked.fork);
            choice.set(None);
        }
    };
    let port = {
        let parked = parked.clone();
        let database = database.clone();
        move |_| {
            if let Some(database) = database.clone() {
                adopt_as(
                    &parked.text,
                    &parked.file_name,
                    &database,
                    loaded,
                    Some(loaded),
                    session,
                    report,
                );
            }
            choice.set(None);
        }
    };

    rsx! {
        Modal {
            title: "Which server?".to_string(),
            size: ModalSize::Md,
            on_close: move |_| choice.set(None),
            div { class: "build-report",
                p { class: "build-report__line",
                    "{parked.file_name} was saved on {parked.fork.display_name()}, "
                    "and {loaded.display_name()} is loaded."
                }
                p { class: "build-report__note",
                    "Reading it here keeps every pick and every slot, and re-exports them "
                    "unchanged — but anything {loaded.display_name()} does not carry contributes "
                    "nothing to the totals."
                }
                div { class: "build-report__actions",
                    button {
                        class: "seg active",
                        r#type: "button",
                        onclick: switch,
                        "Open on {parked.fork.display_name()}"
                    }
                    button {
                        class: "seg",
                        r#type: "button",
                        disabled: database.is_none(),
                        onclick: port,
                        "Read it on {loaded.display_name()}"
                    }
                }
            }
        }
    }
}

// ============================================================
// The link.
// ============================================================

/// Read the fragment off the address bar, without touching it.
///
/// **Reading and removing used to be one script, and that was a collision waiting for a second
/// reader.** A sign-in coming back lands on the same fragment — `#access_token=…` — and this
/// future runs at boot beside [`crate::cloud::account::Account::start`]'s. Whichever resolved
/// first won, and when it was this one the tokens were gone from the URL before the account could
/// read them: sign-in silently never completes, and the decode failure below is "passed over in
/// silence" by design, so nothing says so anywhere. The two readers are disjoint on content — an
/// auth fragment does not decode as a build and a build carries no `access_token` — so the fix is
/// that neither one clears what it did not understand.
const READ_URL_FRAGMENT: &str =
    "try { return window.location.hash || ''; } catch (_) { return ''; }";

/// Take the fragment off the address bar, once this reader has claimed it.
///
/// Removed rather than left, so a refresh does not re-import the build the user has since
/// edited, and so the address bar stops carrying a whole build around. `replaceState` rather
/// than assigning `location.hash`, which would push a history entry and make Back re-import.
const CLEAR_URL_FRAGMENT: &str = "\
try {\
  window.history.replaceState(null, '', window.location.pathname + window.location.search);\
} catch (_) {}";

/// A build arriving as `#…` on the URL, read once at boot.
///
/// Not gated on the platform, and deliberately: `window.location` exists in the desktop webview
/// too and simply has no fragment there, so both targets run the same code and the desktop gets
/// an empty string rather than a `None` twin standing in for a feature it was denied. A share
/// link still reaches the desktop, through [`PasteBuildHost`].
///
/// The text is handed to [`take_build`] with no database, because at first paint there is never
/// one — and parking is the right answer regardless: the shell restores this fork's stored
/// build moments later, and a fragment decoded before that would be overwritten by the build it
/// arrived to replace.
///
/// **A fragment that does not decode at all is passed over in silence, and left where it is.**
/// The address bar is not this app's alone — an anchor, a tracking parameter, a sign-in coming
/// back or a client's own rewrite can put anything there — so treating every unreadable `#…` as a
/// failed import would raise a modal over someone's bookmark, and clearing one would take a
/// fragment another reader is waiting on. Once it decodes to text, it was meant to be something:
/// then it is claimed, cleared, and the reader's refusal is worth saying.
#[component]
pub fn BuildIoFragmentHost(dataset: Signal<DatasetId>) -> Element {
    let session = use_context::<BuildSession>();
    let pending = use_context::<BuildIoPending>().0;
    let choice = use_context::<BuildIoChoice>().0;
    let mut report = use_context::<BuildIoReport>().0;

    use_future(move || async move {
        let Ok(value) = document::eval(READ_URL_FRAGMENT).await else {
            return;
        };
        let fragment = value.as_str().unwrap_or_default().to_string();
        if fragment.trim_start_matches('#').trim().is_empty() {
            return;
        }
        match read_pasted(&fragment) {
            // Claimed, so cleared — on the refusal too, because a fragment that decoded to text
            // was meant to be a build, and leaving it makes a refresh retry the refusal.
            Ok(text) => {
                document::eval(CLEAR_URL_FRAGMENT);
                take_build(
                    text,
                    "the linked build".to_string(),
                    None,
                    dataset,
                    session,
                    pending,
                    choice,
                    report,
                )
            }
            Err(LinkError::Unreadable { refusal }) => {
                document::eval(CLEAR_URL_FRAGMENT);
                report.set(Some(BuildIoOutcome::Refused {
                    action: "Open the linked build".to_string(),
                    reason: refusal.to_string(),
                }));
            }
            // Not ours. Left exactly as it arrived — this is the arm a sign-in coming back lands
            // in, and it is also someone's bookmark anchor.
            Err(_) => {}
        }
    });

    rsx! {}
}

// ============================================================
// The receipt.
// ============================================================

/// The receipt. One surface for both outcomes, because they answer the same question — what
/// happened to the file — and a refusal is not a different kind of event from a completion.
///
/// Mounted at the shell root, outside both layout roots, so its `position: fixed` backdrop is
/// not contained by a free-grid surface's `transform` — the same hosting every other overlay in
/// this app needs.
///
/// The other half of the flow — decoding a file parked while its own fork loads — is an effect
/// in the shell rather than a hook here, because what it waits on is the shell's own record that
/// the restore for that fork has FINISHED (`loaded_for`), and a prop captured by an effect
/// freezes at the render that created it.
#[component]
pub fn BuildIoReportHost(dataset: Signal<DatasetId>) -> Element {
    let mut report = use_context::<BuildIoReport>().0;
    let Some(outcome) = report.read().clone() else {
        return rsx! {};
    };

    let title = match &outcome {
        BuildIoOutcome::Opened { .. } => "Build opened",
        BuildIoOutcome::Refused { .. } => "That didn't work",
    };

    rsx! {
        Modal {
            title: title.to_string(),
            size: ModalSize::Md,
            on_close: move |_| report.set(None),
            match outcome {
                BuildIoOutcome::Opened { file_name, picks, unresolved, unstamped } => rsx! {
                    OpenedReport { file_name, picks, unresolved, unstamped, dataset }
                },
                BuildIoOutcome::Refused { action, reason } => rsx! {
                    div { class: "build-report",
                        p { class: "build-report__line", "{action} could not complete." }
                        div { class: "load-state error", "{reason}" }
                    }
                },
            }
        }
    }
}

/// What arrived, and — for a build that named no fork — the one thing that can be done about
/// what didn't.
///
/// The retry is offered only when something failed to resolve, which is the user's own rule for
/// this: read it against the server that is loaded, and ask only if that produces errors. A
/// `/buildsave` export carries no fork and the client that wrote it never will, so there is
/// nothing to probe and no question worth asking in advance — the unresolved list IS the
/// evidence, and it does not exist until the read has happened.
///
/// **The gate is wider than that rule, and knowingly so.** `mbd_import::ImportNote` also carries
/// "a fact about the read the user has to be told" — the derived level, on both its arms — and
/// those land in this same list, so a `.mbd` naming no fork can reach the offer without anything
/// having failed to resolve. Reachable for every Thunderspy file, which Mids stamps `Generic`
/// (MBDEXPORT-2). Narrowing it means giving `skif::Unresolved` a kind at its five construction
/// sites; recorded under MBDIMPORT-15 rather than done here.
///
/// It re-parks the ORIGINAL text rather than re-reading the build that came out of it: the
/// build has already resolved against the wrong definitions, and what has to cross the reload
/// is what the game client wrote.
#[component]
fn OpenedReport(
    file_name: String,
    picks: usize,
    unresolved: Vec<Unresolved>,
    unstamped: Option<String>,
    dataset: Signal<DatasetId>,
) -> Element {
    let mut pending = use_context::<BuildIoPending>().0;
    let mut report = use_context::<BuildIoReport>().0;
    let powers = if picks == 1 { "power" } else { "powers" };
    let elsewhere = unstamped.filter(|_| !unresolved.is_empty());

    rsx! {
        div { class: "build-report",
            p { class: "build-report__line", "{file_name} — {picks} {powers}." }
            if unresolved.is_empty() {
                p { class: "build-report__note",
                    "Everything in the file resolved against this dataset."
                }
            } else {
                p { class: "build-report__note",
                    "Nothing here was dropped. Anything not in the dataset is kept in the build "
                    "exactly as the file wrote it and re-exports unchanged, though not included "
                    "in calculations."
                }
                ul { class: "build-report__unresolved",
                    for entry in unresolved {
                        li { key: "{entry.context}/{entry.detail}",
                            span { class: "build-report__context", "{entry.context}" }
                            span { class: "build-report__detail", "{entry.detail}" }
                        }
                    }
                }
            }
            if let Some(text) = elsewhere {
                p { class: "build-report__note",
                    "This build has no server, so it was read against {dataset().display_name()}"
                }
                div { class: "build-report__actions",
                    for fork in DatasetId::ALL.into_iter().filter(|fork| *fork != dataset()) {
                        button {
                            key: "{fork.as_str()}",
                            class: "seg",
                            r#type: "button",
                            onclick: {
                                let text = text.clone();
                                let file_name = file_name.clone();
                                move |_| {
                                    let mut dataset = dataset;
                                    pending
                                        .set(
                                            Some(PendingImport {
                                                dataset: fork,
                                                file_name: file_name.clone(),
                                                text: text.clone(),
                                            }),
                                        );
                                    dataset.set(fork);
                                    report.set(None);
                                }
                            },
                            "Read it on {fork.display_name()}"
                        }
                    }
                }
            }
        }
    }
}
