//! The app's own five actions — Help, What's New, Feedback, Discord, Support —
//! at the right end of the quickbar row, under the brand.
//!
//! **They were a floating bottom-right cluster until 2026-09-15** (the beta's
//! own arrangement, ported in `ui-parity`), and moving them up settles what the
//! quickbar band already claimed: "the header is the build's own controls, and
//! these are the app's." Five buttons the app owns, floating over whatever panel
//! happened to be under them, were the one part of that sentence with nowhere
//! to be. Now the band states it — the build's controls on row 1, the app's on
//! row 2 — and the cluster stops paying the containment tax a `fixed` element
//! costs (it sat outside both layout roots precisely so a `transform`ed grid
//! surface could not clip it; in flow there is nothing to escape).
//!
//! **Bare icons, no bubbles — for four of the five** (user-directed). The round
//! pills were the vocabulary of a floating cluster: a thing hovering over
//! content needs its own surface to be legible against. On a band with a
//! background of its own that surface is redundant, and five filled circles
//! beside the pinned row would outweigh the pins — which are the row's subject.
//! The per-action colour stays, on the glyph rather than behind it, because it
//! is what tells the four apart at this size.
//!
//! **Support keeps its capsule, and keeps its words** (user-directed). It is the
//! one entry that asks for something rather than opening something, and a CTA
//! that reads as an unlabelled glyph among four other glyphs is a CTA nobody
//! answers. The capsule is the exception that the other four's plainness pays
//! for: one thing on the row draws an outline, so the outline means something.
//! It is the reason the "no bubbles" rule is about REDUNDANT chrome rather than
//! about chrome — the other four had nothing to say that their mark did not
//! already say, and this one does.
//!
//! What's New, Support and Feedback open modals this tree already owns — the
//! beta's own wiring, its "What's New" entrypoint being the Welcome modal, not
//! the changelog, and its Support the Donate modal. Discord is an external link
//! through [`open_external`], because the desktop webview never registers a
//! window-open handler (dioxus-desktop 0.7.9), so `target="_blank"` would be a
//! silent no-op there. Help raises [`crate::help::HelpOpen`] and Feedback
//! [`crate::feedback::FeedbackOpen`], the flags the main menu's rows raise too —
//! one guide and one form with two doors each, rather than a signal either
//! opener owns and the other cannot reach.
//!
//! **Feedback was that Discord DM until RB4h**, on the reasoning that "the
//! rebuild has no backend" — which was never quite the fact it read as: the
//! worker the beta posts to is live, serves the same inbox, and needed nothing
//! built to accept this client. The DM survives inside the form, as the line
//! that says where to go for an answer.

use dioxus::prelude::*;

/// The beta's `CoH-Sidekick/src/lib/links.ts`, ported verbatim — repo-qualified because DEC8
/// deleted this repo's copy and F84's lesson is that a bare `src/…` names two files in two
/// repos. The test at the bottom of this file re-states each string, so a typo here fails a
/// test rather than shipping a dead link.
/// The public site. Share links are built from this whenever the shell's own origin is not one a
/// recipient could open — see [`crate::cloud::browser::share_origin`].
///
/// No trailing slash: every caller writes `{SITE_ORIGIN}/path`, and a slash here would produce
/// `//path`, which resolves to a different host entirely as a protocol-relative URL.
///
/// Unlike the two below and [`crate::feedback::WORKER_URL`], this is a domain the project owns,
/// so F56's rotation problem does not reach it.
pub const SITE_ORIGIN: &str = "https://coh-sidekick.com";

pub const DISCORD_INVITE_URL: &str = "https://discord.gg/5HmYsACBv6";
pub const DISCORD_FEEDBACK_DM_URL: &str = "https://discord.com/channels/@me/570068130320220172";

/// The five actions, in the beta's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Help,
    WhatsNew,
    Feedback,
    Discord,
    Support,
}

/// What an action resolves to.
///
/// **The test this exists for does not exist.** This doc said "the test grades that the five
/// actions cover exactly these five destinations, one each", and on 2026-09-26 that test was
/// looked for and is in neither sibling: `coh-sidekick-1.0`'s only `crates/app/tests/` file is
/// `panic_census.rs`, a source-text scanner that calls nothing, and none of the beta's 300 test
/// files names `Destination` or the `openHelpModal`/`openWelcomeModal` calls this mirrors. So
/// this enum and [`destination`] are a table with no reader and no grader.
///
/// Kept rather than deleted because the mapping IS the specification -- five actions, five
/// destinations, one each -- and writing the missing test is a smaller job than re-deriving the
/// table from the beta's TSX. Whoever writes it should delete these two `allow`s.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Destination {
    /// The guide. Raised from here and from the main menu's Help row.
    HelpModal,
    /// The beta's "What's New" entrypoint is the Welcome modal, not the changelog.
    WelcomeModal,
    DonateModal,
    /// The report form (RB4h). Raised from here and from the main menu's row.
    FeedbackModal,
    External(&'static str),
}

pub const ACTIONS: [Action; 5] = [
    Action::Help,
    Action::WhatsNew,
    Action::Feedback,
    Action::Discord,
    Action::Support,
];

/// The beta's `openHelpModal` / `openWelcomeModal` / ... calls, as data. See [`Destination`]
/// for why nothing calls this.
#[allow(dead_code)]
pub fn destination(action: Action) -> Destination {
    match action {
        Action::Help => Destination::HelpModal,
        Action::WhatsNew => Destination::WelcomeModal,
        Action::Feedback => Destination::FeedbackModal,
        Action::Discord => Destination::External(DISCORD_INVITE_URL),
        Action::Support => Destination::DonateModal,
    }
}

/// Open a URL in the user's browser. The desktop webview only routes
/// `window.open` if its host registers a window-open handler, and dioxus-desktop
/// 0.7.9 does not — so the native half asks the OS opener instead.
/// `pub(crate)` since RB5: the desktop sign-in hands the provider URL to the OS browser through
/// this same door, because the reason it exists — the desktop webview registers no window-open
/// handler — is the same reason a sign-in must not be a `target="_blank"` there either.
pub(crate) fn open_external(url: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        // Read here, judged in `open_command`. The lookup is the one fact about the host the
        // Windows arm needs and the only part of that arm no machine here can produce, so it is
        // kept to a single line and everything that can be wrong about the value is decided in
        // the pure builder, where a test on any platform reaches it.
        let system_root = std::env::var("SystemRoot").unwrap_or_default();
        let opener = if cfg!(target_os = "macos") {
            Opener::MacOs
        } else if cfg!(target_os = "windows") {
            Opener::Windows {
                system_root: &system_root,
            }
        } else {
            Opener::Other
        };
        let (program, args) = match open_command(opener, url) {
            Ok(command) => command,
            Err(refusal) => {
                eprintln!("app_actions: refused to open {url}: {refusal}");
                return;
            }
        };
        if let Err(err) = std::process::Command::new(&program).args(&args).spawn() {
            eprintln!("app_actions: could not open {url}: {err}");
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        // This arm had no check at all, while F20's recorded verdict said
        // the scheme check applied "on every platform" — it lived in `open_command`, which is
        // native-only, and so was every test of it. `window.open` will follow a `javascript:` URL
        // in the opener's own origin, so "no caller passes one today" was the only thing standing
        // there. Now both arms ask the same function the same question.
        if let Err(refusal) = openable(url) {
            web_sys::console::warn_1(
                &format!("app_actions: refused to open {url}: {refusal}").into(),
            );
            return;
        }
        // The URL reaches JS as a JSON string literal rather than through `{:?}`. Rust's debug
        // escaping is close enough to JS's to be tempting and is not the same language — the rule
        // `crate::build_file` states and `cloud::account::navigate` follows.
        let Some(literal) = serde_json::to_string(url).ok() else {
            return;
        };
        document::eval(&format!("window.open({literal}, '_blank', 'noopener');"));
    }
}

/// Whether this URL may leave the process at all.
///
/// **Target-independent, and that is the whole point of it existing separately.** These two rules
/// used to live inside [`open_command`], which is `#[cfg(not(target_arch = "wasm32"))]`, as was
/// every test of them — so F20's actioned row recorded a scheme check that applied "on every
/// platform" while the web build had none. Nothing was exploitable, because the only web caller
/// passes a constant; what was wrong was the record, and a record is what the next person reads
/// before deciding a caller is safe to add.
///
/// `http` and `https` are the whole list, and every caller passes one (the Discord links, and the
/// provider URL). Control characters are refused because nothing legitimate carries one and an
/// argv should never contain one.
pub(crate) fn openable(url: &str) -> Result<(), &'static str> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("not an http(s) URL");
    }
    if url.chars().any(char::is_control) {
        return Err("the URL carries a control character");
    }
    Ok(())
}

/// Which OS opener [`open_external`] should use, and for Windows the one fact about the host that
/// arm needs. **A parameter rather than a `cfg!` inside the builder**, so all three arms are
/// gradeable from any host — which is the whole reason the Windows defect below survived: it
/// lived in a branch no test on a Mac could reach. `system_root` rides along for the same reason:
/// a lookup made inside the builder would be empty on every host that can run these tests, so the
/// refusal path would be the only one any test ever saw.
// Native-only: the web build opens a URL with `window.open`, so there is no OS opener to choose
// and an ungated enum would be a warning on that target.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Opener<'a> {
    MacOs,
    /// `%SystemRoot%`, verbatim from the process environment — validated in [`system32_path`],
    /// never here, because "a string that came from the environment" is the whole of what this
    /// variant knows and the empty string is one of its real values.
    Windows {
        system_root: &'a str,
    },
    Other,
}

/// The program and argv that hand one URL to the OS browser, or the reason it is refused.
///
/// # Windows does NOT go through `cmd`, and that is the fix for a live defect
///
/// This used to spawn `cmd /C start "" <url>`. `cmd.exe` re-parses its command line and treats
/// `&` as a **command separator**, and Rust's argv quoting does not protect it — `Command` escapes
/// for the MSVCRT parser, which runs *after* `cmd` has already split. Every provider URL this app
/// opens contains one: `authorize?provider=discord&redirect_to=http://127.0.0.1:<port>`.
///
/// So on Windows the browser received the authorize URL **truncated at the first `&`**, with no
/// `redirect_to` — GoTrue then falls back to the project's site URL, and the tokens land on
/// coh-sidekick.com instead of on the loopback listener waiting for them. Desktop sign-in could
/// not complete on Windows at all, which is the platform the release candidate is mostly for. The
/// remainder of the URL was handed to `cmd` as a command to run.
///
/// `rundll32.exe url.dll,FileProtocolHandler` reaches the same default browser through
/// `CreateProcess` with no shell in between, so the URL stays one argv element and no character in
/// it is a metacharacter to anything.
///
/// # And Windows names it absolutely, because a bare name is not a `System32` name
///
/// `rundll32.exe` alone is a *file name*, and Rust resolves a file name by searching. `search_paths`
/// (`std/src/sys/process/windows.rs`) tries the child's `PATH`, then **the directory the running
/// executable sits in**, and only then `GetSystemDirectoryW`. The RC ships as a portable zip that
/// unpacks wherever downloads land, so any `rundll32.exe` already beside the unpacked `Sidekick.exe`
/// wins the search and is handed the URL — CWE-426, and no privilege is needed to arrange it.
///
/// Spelled with a separator and an `.exe` suffix, the same std code skips the search entirely and
/// passes the string to `CreateProcessW`. So the program is `%SystemRoot%\System32\rundll32.exe`,
/// and a `%SystemRoot%` that cannot name an absolute path refuses the open rather than being
/// patched into one — see [`system32_path`].
///
/// # And a scheme check, on every platform — which is now true rather than merely recorded
///
/// The opener is a door out of this process to whatever the OS associates with a scheme, so what
/// may go through it is worth stating rather than assuming. The rules live in [`openable`], which
/// is not gated on a target, because this function is — and F67 is the row for the gap that left:
/// the claim sat on the record while the web arm of [`open_external`] had no check at all.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn open_command(
    opener: Opener<'_>,
    url: &str,
) -> Result<(String, Vec<String>), &'static str> {
    openable(url)?;
    let url = url.to_string();
    Ok(match opener {
        Opener::MacOs => ("open".to_string(), vec![url]),
        // `url.dll,FileProtocolHandler` is one argument, and the URL is the next. No shell.
        Opener::Windows { system_root } => (
            system32_path(system_root, "rundll32.exe")?,
            vec!["url.dll,FileProtocolHandler".to_string(), url],
        ),
        Opener::Other => ("xdg-open".to_string(), vec![url]),
    })
}

/// `%SystemRoot%\System32\<program>`, or the reason the value cannot be trusted to build one.
///
/// Every check here is about who chooses the binary that runs. Naming the program absolutely takes
/// that choice away from anything a non-administrator can write to, and each rejected shape hands
/// some of it back. A root with no drive letter reintroduces the working directory. A `..`
/// component means the result never named `System32` in the first place — `C:\WINDOWS\..\Users\Public`
/// appends just as cleanly and lands somewhere any user can write, so whether anything downstream
/// normalises it away is beside the point.
///
/// **Refused rather than repaired** (Rule 1). The obvious repair — fall back to a literal
/// `C:\Windows` — is not safe on a machine whose Windows lives elsewhere: the default ACL on `C:\`
/// lets an ordinary user create directories at the root, so the fallback names a path an attacker
/// can supply. Windows sets `SystemRoot` in every process environment it creates, so an `Err` here
/// means something upstream already went wrong, and a link that fails to open with a printed
/// reason beats one opened by a program chosen somewhere else.
#[cfg(not(target_arch = "wasm32"))]
fn system32_path(system_root: &str, program: &str) -> Result<String, &'static str> {
    let root = system_root.trim_end_matches('\\');
    let mut head = root.bytes();
    let absolute = matches!(head.next(), Some(letter) if letter.is_ascii_alphabetic())
        && head.next() == Some(b':')
        && head.next() == Some(b'\\');
    if !absolute {
        return Err(r"%SystemRoot% is not an absolute X:\ path");
    }
    if root.contains('/') || root.contains('"') || root.chars().any(char::is_control) {
        return Err("%SystemRoot% carries a character a Windows program path should not");
    }
    if root
        .split('\\')
        .skip(1)
        .any(|component| component.is_empty() || component == "." || component == "..")
    {
        return Err("%SystemRoot% has an empty or relative path component");
    }
    Ok(format!(r"{root}\System32\{program}"))
}

/// Mounted by [`crate::quickbar::Quickbar`] as the band's trailing cluster — a sibling of the
/// pinned row, which carries `flex: 1` and pushes this to the right edge under the brand.
#[component]
pub fn AppActions() -> Element {
    let help = use_context::<crate::help::HelpOpen>().0;
    let whats_new = use_context::<crate::app_info::WelcomeOpen>().0;
    let support = use_context::<crate::app_info::DonateOpen>().0;
    let feedback = use_context::<crate::feedback::FeedbackOpen>().0;
    rsx! {
        div { class: "app-actions",
            for action in ACTIONS {
                { control(action, help, whats_new, support, feedback) }
            }
        }
    }
}

fn control(
    action: Action,
    mut help: Signal<bool>,
    mut whats_new: Signal<bool>,
    mut support: Signal<bool>,
    mut feedback: Signal<bool>,
) -> Element {
    match action {
        Action::Help => rsx! {
            button {
                class: "app-actions__button app-actions__help",
                r#type: "button",
                title: "How to use Sidekick",
                "aria-label": "Open help",
                onclick: move |_| help.set(true),
                { help_icon() }
            }
        },
        Action::WhatsNew => rsx! {
            button {
                class: "app-actions__button app-actions__whats-new",
                r#type: "button",
                title: "What's New",
                "aria-label": "What's new in Sidekick",
                onclick: move |_| whats_new.set(true),
                { whats_new_icon() }
            }
        },
        Action::Feedback => rsx! {
            button {
                class: "app-actions__button app-actions__feedback",
                r#type: "button",
                title: "Send feedback or report a bug",
                "aria-label": "Send feedback or report a bug",
                onclick: move |_| feedback.set(true),
                { feedback_icon() }
            }
        },
        Action::Discord => rsx! {
            button {
                class: "app-actions__button app-actions__discord",
                r#type: "button",
                title: "Join the Sidekick Discord",
                "aria-label": "Join the Sidekick Discord",
                onclick: move |_| open_external(DISCORD_INVITE_URL),
                { discord_icon() }
            }
        },
        Action::Support => rsx! {
            // The label is inside the capsule with the mark, not beside it: the two are one
            // control and one hit target. `aria-label` stays anyway — it is the accessible
            // name the other four rely on, and letting this one fall back to its text content
            // would make the row's names come from two places.
            button {
                class: "app-actions__button app-actions__support",
                r#type: "button",
                title: "I run on espresso, thank you for enabling me!",
                "aria-label": "Buy Me a Coffee",
                onclick: move |_| support.set(true),
                { coffee_icon() }
                span { class: "app-actions__label", "Buy Me a Coffee" }
            }
        },
    }
}

// The icons' paths are the beta's, ported verbatim — they are data, and a
// re-drawn mark would be a second source of truth for the brand shapes.

fn help_icon() -> Element {
    stroke_icon("M8.228 9c.549-1.165 2.03-2 3.772-2 2.21 0 4 1.343 4 3 0 1.4-1.278 2.575-3.006 2.907-.542.104-.994.54-.994 1.093m0 3h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z")
}

fn whats_new_icon() -> Element {
    stroke_icon("M5 3v4M3 5h4M6 17v4m-2-2h4m5-16l2.286 6.857L21 12l-5.714 2.143L13 21l-2.286-6.857L5 12l5.714-2.143L13 3z")
}

fn feedback_icon() -> Element {
    stroke_icon("M8 10h.01M12 10h.01M16 10h.01M9 16H5a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v8a2 2 0 01-2 2h-5l-5 5v-5z")
}

fn coffee_icon() -> Element {
    rsx! {
        svg {
            class: "app-actions__icon",
            view_box: "0 0 24 24",
            "aria-hidden": "true",
            path {
                d: "M5 18h10a2 2 0 002-2V8H5v8a2 2 0 002 2zM17 8h2a2 2 0 010 4h-2M8 2v3M12 2v3"
            }
        }
    }
}

fn stroke_icon(d: &str) -> Element {
    rsx! {
        svg {
            class: "app-actions__icon",
            view_box: "0 0 24 24",
            "aria-hidden": "true",
            path { d: "{d}" }
        }
    }
}

/// Filled on a 16-unit box, the rest stroked on 24 — the beta's two cuts.
fn discord_icon() -> Element {
    rsx! {
        svg {
            class: "app-actions__icon app-actions__icon--fill",
            view_box: "0 0 16 16",
            "aria-hidden": "true",
            path {
                d: "M13.545 2.907a13.2 13.2 0 0 0-3.257-1.011.05.05 0 0 0-.052.025c-.141.25-.297.577-.406.833a12.2 12.2 0 0 0-3.658 0 8 8 0 0 0-.412-.833.05.05 0 0 0-.052-.025c-1.125.194-2.22.534-3.257 1.011a.04.04 0 0 0-.021.018C.356 6.024-.213 9.047.066 12.032q.003.022.021.037a13.3 13.3 0 0 0 3.995 2.02.05.05 0 0 0 .056-.019q.463-.63.818-1.329a.05.05 0 0 0-.01-.059l-.018-.011a9 9 0 0 1-1.248-.595.05.05 0 0 1-.02-.066l.015-.019q.127-.095.248-.195a.05.05 0 0 1 .051-.007c2.619 1.196 5.454 1.196 8.041 0a.05.05 0 0 1 .053.007q.121.1.248.195a.05.05 0 0 1-.004.085 8 8 0 0 1-1.249.594.05.05 0 0 0-.03.03.05.05 0 0 0 .003.041c.24.465.515.909.817 1.329a.05.05 0 0 0 .056.019 13.2 13.2 0 0 0 4.001-2.02.05.05 0 0 0 .021-.037c.334-3.451-.559-6.449-2.366-9.106a.03.03 0 0 0-.02-.019m-8.198 7.307c-.789 0-1.438-.724-1.438-1.612s.637-1.613 1.438-1.613c.807 0 1.45.73 1.438 1.613 0 .888-.637 1.612-1.438 1.612m5.316 0c-.788 0-1.438-.724-1.438-1.612s.637-1.613 1.438-1.613c.807 0 1.451.73 1.438 1.613 0 .888-.631 1.612-1.438 1.612"
            }
        }
    }
}
