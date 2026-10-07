//! CoH Sidekick — one Dioxus codebase, desktop (wry webview) and web (WASM).

use dioxus::prelude::*;

mod alert_store;
mod app_actions;
mod app_info;
mod build_bar;
mod build_file;
mod build_io;
mod build_session;
mod build_store;
mod clipboard;
mod cloud;
mod compare_slotting;
mod confirm;
mod controls;
mod crash;
mod damage_metric_store;
mod data_source;
mod desktop_signin;
mod enhancement_list;
mod enhancement_tools;
mod export_font;
mod export_image;
mod feedback;
mod forum_export;
mod granted_powers;
mod grid;
mod help;
mod history;
mod inherents;
mod layout_store;
mod layout_sync;
mod level_control;
mod level_up_store;
mod main_menu;
mod mobile_nav;
mod mobile_order;
mod modal;
mod naming;
mod panel_popout;
mod panel_visibility;
mod panels;
mod picker_defaults;
mod picker_memory;
mod picker_queue;
mod picker_sets;
mod picker_store;
mod pinned_powers;
mod popover;
mod power_art;
mod powerset_compare;
mod quickbar;
mod reorder_menu;
mod shell;
mod slot_level_store;
mod stats_store;
mod storage;
mod theme;
mod ui_scale;
mod update_banner;
mod view;

static TOKENS_CSS: Asset = asset!("/assets/tokens.css");
static APP_CSS: Asset = asset!("/assets/app.css");
static THEME_ASTORIA_CSS: Asset = asset!("/assets/themes/astoria.css");
static THEME_SIDEKICK_CSS: Asset = asset!("/assets/themes/sidekick.css");
static THEME_PARAGON_CSS: Asset = asset!("/assets/themes/paragon.css");
static THEME_MENACE_CSS: Asset = asset!("/assets/themes/menace.css");
static THEME_IMPERIAL_CSS: Asset = asset!("/assets/themes/imperial.css");
static THEME_RENEGADE_CSS: Asset = asset!("/assets/themes/renegade.css");
static THEME_HAMIDON_CSS: Asset = asset!("/assets/themes/hamidon.css");
static THEME_RESISTANCE_CSS: Asset = asset!("/assets/themes/resistance.css");
static THEME_CARNIVAL_CSS: Asset = asset!("/assets/themes/carnival.css");
static THEME_ESPRESSO_CSS: Asset = asset!("/assets/themes/espresso.css");
static THEME_CASSETTE_CSS: Asset = asset!("/assets/themes/cassette.css");
static THEME_SIDEKICK_LIGHT_CSS: Asset = asset!("/assets/themes/sidekick-light.css");
static THEME_PARAGON_LIGHT_CSS: Asset = asset!("/assets/themes/paragon-light.css");
static THEME_ASTORIA_LIGHT_CSS: Asset = asset!("/assets/themes/astoria-light.css");

// The browser tab's mark. The 32px PNG rather than `favicon.svg`: the SVG is 690KB of embedded
// raster, which is a heavy download for a 16px tab icon.
static TAB_ICON: Asset = asset!("/assets/img/favicon-32x32.png");

static FONT_SAIRA: Asset = asset!("/assets/fonts/Saira-memjYa2wxmKQyPMrZX79wwYZQMhsyuSLiIvS.woff2");
static FONT_DM_MONO_400: Asset = asset!("/assets/fonts/DMMono-aFTU7PB1QTsUX8KYthqQBA.woff2");
static FONT_DM_MONO_500: Asset = asset!("/assets/fonts/DMMono-aFTR7PB1QTsUX8KYvumzEYOtbQ.woff2");
static FONT_SN_PRO: Asset = asset!("/assets/fonts/SNPro-NGS1v5zWIAwPIq7hapRO.woff2");
static FONT_NUNITO: Asset = asset!("/assets/fonts/Nunito-XRXV3I6Li01BKofINeaB.woff2");

fn main() {
    #[cfg(feature = "desktop")]
    dioxus::LaunchBuilder::desktop()
        .with_cfg(desktop_window())
        .launch(App);

    #[cfg(not(feature = "desktop"))]
    dioxus::launch(App);
}

/// The desktop document's Content-Security-Policy.
///
/// **The policy itself lives in `desktop-csp.txt`, not here, and that is the point.** It is read
/// by two things: this function, and the browser probe that loads it into a real
/// browser engine and measures what it permits. One file means the measurement is of the policy
/// that ships, rather than of a copy that agreed with it on the day it was written.
///
/// The row is two-sided and the web half closed separately (F76 stamps every `public/*.html`);
/// this is the desktop half, which had no policy of any kind because nothing composes the
/// document — `dioxus-desktop` serves its own `prod.index.html` and the app never touched it.
/// [`dioxus::desktop::Config::with_custom_head`] is the hook: it inserts immediately before
/// `</head>` (`protocol.rs:121-123`), ahead of the module loader that goes in before `</body>`,
/// so a `<meta>` policy here governs every script on the page — the one thing a meta CSP has to
/// get right.
///
/// **`'unsafe-eval'` is the precondition this row has been waiting on, and it is a cost rather
/// than a blocker.** `dioxus-desktop-0.7.9/src/query.rs:79-80` runs every `document::eval`
/// through `new AsyncFunction(...)`, and there are 54 such sites in this crate. So `script-src`
/// cannot be tight, and the honest reading is that this policy buys little there. What it buys is
/// everything else, and one line is worth the rest:
///
/// **`connect-src` is the origin plus the loopback socket, and it was `'none'` until a desktop
/// run said otherwise.** The reasoning for `'none'` was that this app's HTTP client is `reqwest`,
/// which runs in the Rust process and is subject to none of this, and that the webview's
/// JavaScript makes no network request at all — `crash.rs`'s `sendBeacon` is
/// `#[cfg(target_arch = "wasm32")]`, the web target only, and the desktop arm is
/// `reqwest::blocking` on a thread of its own. Every word of that is true about the APP's
/// JavaScript, and it is the wrong inventory: `dioxus-desktop`'s own interpreter is JavaScript
/// too, and it is built entirely out of `connect-src`.
///
/// **Every DOM edit arrives over a loopback WebSocket** — `edits.rs:95` builds
/// `ws://127.0.0.1:{port}/{webview_id}/{key}` and `native.ts:460` opens it — and **every user
/// event is a synchronous XHR POST** to `dioxus://index.html//__events`
/// (`handleVirtualdomEventSync`). Neither is a debug convenience; both are how the renderer works
/// in a release build. `examples/csp_probe.rs` measured all three settings in the shipped engine:
///
/// - `connect-src 'none'` — the app never renders at all. The edits socket is refused, so the
///   first edit never lands, and the process exits without drawing a frame.
/// - `connect-src ws://127.0.0.1:*` — the app renders correctly and is **completely inert**.
///   Fonts, icons and stylesheets all arrive; every click is dropped with
///   `connect-src blocked dioxus://index.html//__events`. This is the failure mode a person
///   opening the window would most likely pass, because it looks perfect.
/// - the clause as it now stands — everything works, and `fetch('https://example.com/')` is still
///   refused.
///
/// So the directive still does the job it was chosen for. What it can no longer be is `'none'`,
/// and the narrowest setting that leaves the renderer working is this one: the document's own
/// origin, and the loopback port the framework picks at runtime. An injected script can talk to
/// this app's own webview and to nothing else on the network.
///
/// The three origin spellings in every directive are `dioxus-desktop`'s own, which it picks
/// between by platform (`protocol.rs:15-22`): Android `https://dioxus.index.html/`, Windows
/// `http://dioxus.index.html/`, everything else the `dioxus://` custom scheme. All three are named
/// rather than the one this build will use, because whether `'self'` resolves to a custom scheme
/// is webview-specific and getting it wrong would take the app's own fonts and icons with it.
/// Three fixed literals cost nothing — no host by those names is reachable.
///
/// `img-src` names the one host `cloud::avatar::avatar_src` allows, for the same reason it does;
/// `blob:` and `data:` are the app's own (`build_file.rs:410` and `:1039` hand a download an
/// object URL, `export_image.rs:1258` renders a preview as `data:image/png`).
///
/// **`default-src` is the origin rather than `'none'`, and that is a judgement made under a
/// constraint worth stating.** This app cannot be driven headlessly — it is a Wayland client, and
/// checking it is a request to a person, not a script. `'none'` would also cover the directives
/// not named here (`media-src`, `worker-src`, `manifest-src`, `child-src`) and would be the
/// setting most likely to break something nobody can test from here. Falling back to this
/// document's own origin blocks every one of them from reaching the internet while leaving
/// same-origin machinery alone.
/// The loopback token in `desktop-csp.txt` that a debug build widens. Named here so the swap has
/// one anchor rather than a copy of the whole clause in every place that reasons about it.
#[cfg(feature = "desktop")]
const RELEASE_EDITS_SOCKET: &str = "ws://127.0.0.1:*";

#[cfg(feature = "desktop")]
fn desktop_csp() -> String {
    let shipped = include_str!("desktop-csp.txt").trim();
    // `dx serve` may speak to the webview over spellings of the loopback the shipped binary has
    // no use for, so a debug build widens the one token that names it. Done by REPLACING that
    // token rather than adding a clause, so a debug build cannot carry a second, looser
    // `connect-src` beside the first — the browser takes the first and the second is either dead
    // or, worse, first.
    //
    // Unlike the release clause, this widening is precautionary rather than measured: nothing
    // observed here needed it, because `dioxus-devtools` connects from the Rust process and not
    // from the webview. It costs the shipped binary nothing, so it stays until a `dx serve`
    // desktop session says which of these it actually uses.
    if cfg!(debug_assertions) {
        return shipped.replace(
            RELEASE_EDITS_SOCKET,
            "ws://127.0.0.1:* ws://localhost:* http://127.0.0.1:* http://localhost:*",
        );
    }
    shipped.to_string()
}

/// The native window. A bare `dioxus::launch` ships no window of its own, which leaves the
/// title at the renderer's "Dioxus App" placeholder and the size entirely to the compositor
/// — and app.css picks the mobile stack over the grid below 900px, so a window opened or
/// dragged narrower than that renders the planner in its phone layout. The floor sits above
/// the breakpoint rather than leaving that to the window manager's discretion.
#[cfg(feature = "desktop")]
fn desktop_window() -> dioxus::desktop::Config {
    use dioxus::desktop::{Config, LogicalSize, WindowBuilder};

    Config::new()
        // F28. Before `</head>` and therefore before the module loader, which is what makes a
        // meta policy bind at all — see `desktop_csp`.
        .with_custom_head(format!(
            r#"<meta http-equiv="Content-Security-Policy" content="{}">"#,
            desktop_csp()
        ))
        .with_window(
            WindowBuilder::new()
                .with_title("CoH Sidekick")
                .with_inner_size(LogicalSize::new(1440.0, 960.0))
                .with_min_inner_size(LogicalSize::new(960.0, 640.0)),
        )
}

/// Self-hosted fonts, declared at runtime so the asset system's hashed URLs are
/// interpolated (a static CSS file's relative `url()` would break under hashing).
fn font_faces() -> String {
    format!(
        "@font-face{{font-family:'Saira';font-style:normal;font-weight:100 900;font-display:swap;src:url({FONT_SAIRA}) format('woff2');}}\
         @font-face{{font-family:'DM Mono';font-style:normal;font-weight:400;font-display:swap;src:url({FONT_DM_MONO_400}) format('woff2');}}\
         @font-face{{font-family:'DM Mono';font-style:normal;font-weight:500;font-display:swap;src:url({FONT_DM_MONO_500}) format('woff2');}}\
         @font-face{{font-family:'SN Pro';font-style:normal;font-weight:200 900;font-display:swap;src:url({FONT_SN_PRO}) format('woff2');}}\
         @font-face{{font-family:'Nunito';font-style:normal;font-weight:200 1000;font-display:swap;src:url({FONT_NUNITO}) format('woff2');}}"
    )
}

/// Touch pointers get IMPLICIT pointer capture on pointerdown, which would pin
/// every subsequent pointer event to the drag origin and starve other hit-test targets.
/// Releasing the capture in a capture-phase listener (before pointerdown dispatches)
/// makes drag-origin elements hit-test normally — mouse pointers are unaffected.
/// The `.panel-head`/`.resize-handle` branches are what let the fixed
/// `.drag-overlay` (grid::view) receive the move/up after a grab: with capture
/// released, those events hit the overlay under the cursor rather than staying
/// pinned to the header or handle the gesture began on. Also covers
/// .reorder-handle (mobile menu).
const POINTER_CAPTURE_SHIM: &str = "\
if (!window.__skPointerShim) {\
  window.__skPointerShim = true;\
  document.addEventListener('pointerdown', (e) => {\
    const origin = e.target && e.target.closest ? (e.target.closest('.panel-head') || e.target.closest('.resize-handle') || e.target.closest('.reorder-handle')) : null;\
    if (origin && e.target.releasePointerCapture) {\
      try { e.target.releasePointerCapture(e.pointerId); } catch (_) {}\
    }\
  }, true);\
}";

#[component]
fn App() -> Element {
    // The panic hook, installed from the first render rather than from `main` because
    // `dioxus-web` sets one of its own inside `run()` and would replace anything set earlier —
    // [`crash::install`] carries the argument.
    use_hook(crash::install);
    // F36. The hook above is installed immediately; the answer to "may this leave the machine"
    // arrives here, because reading it needs the DOM and a panic hook cannot await. Until this
    // resolves the gate is shut, so the window between the two reports nothing rather than
    // reporting against a stored opt-out.
    use_future(crash::load_consent);
    // Apply the persisted (or manifest-default) theme before anything meaningful paints.
    use_effect(theme::apply_saved_theme);
    // One-time drag groundwork (see the shim doc above).
    use_effect(|| {
        document::eval(POINTER_CAPTURE_SHIM);
    });

    rsx! {
        // The tab's name and mark. Here and not in `DocumentAssets`, which a popped-out panel
        // also renders: those windows are named by `with_title` in `main`, and each would
        // otherwise be retitled to the app's name. `dioxus.toml` sets the same title in the web
        // build's `index.html`, so the tab is named during the download too, before this runs.
        document::Title { "CoH Sidekick" }
        document::Link { rel: "icon", r#type: "image/png", href: TAB_ICON }
        DocumentAssets {}
        shell::Shell {}
    }
}

/// Every stylesheet and font-face the app draws with.
///
/// A component rather than four lines in [`App`] because a second webview is a second document
/// and inherits none of this — a popped-out panel ([`crate::panel_popout`]) renders it at its own
/// root. Keeping the list in one place is what stops the two windows from drifting into
/// different stylesheets, which would show up as a panel that looks subtly wrong only when it is
/// out.
#[component]
pub fn DocumentAssets() -> Element {
    rsx! {
        document::Stylesheet { href: TOKENS_CSS }
        document::Stylesheet { href: THEME_ASTORIA_CSS }
        document::Stylesheet { href: THEME_SIDEKICK_CSS }
        document::Stylesheet { href: THEME_PARAGON_CSS }
        document::Stylesheet { href: THEME_MENACE_CSS }
        document::Stylesheet { href: THEME_IMPERIAL_CSS }
        document::Stylesheet { href: THEME_RENEGADE_CSS }
        document::Stylesheet { href: THEME_HAMIDON_CSS }
        document::Stylesheet { href: THEME_RESISTANCE_CSS }
        document::Stylesheet { href: THEME_CARNIVAL_CSS }
        document::Stylesheet { href: THEME_ESPRESSO_CSS }
        document::Stylesheet { href: THEME_CASSETTE_CSS }
        document::Stylesheet { href: THEME_SIDEKICK_LIGHT_CSS }
        document::Stylesheet { href: THEME_PARAGON_LIGHT_CSS }
        document::Stylesheet { href: THEME_ASTORIA_LIGHT_CSS }
        document::Stylesheet { href: APP_CSS }
        document::Style { {font_faces()} }
    }
}
