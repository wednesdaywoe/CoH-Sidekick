//! The "Sidekick has received an update" banner (the beta's `UpdateBanner`), for a tab left open
//! across a release.
//!
//! The beta learned of a release from its service worker. This build has none, and needs none for
//! this: the page names its entry script after a hash of the build (`Sidekick-dxh….js`), and that
//! script names the hashed wasm, which names the hashed datasets. So any change to code or data
//! changes the one name the page loads, and comparing the live page's name with the running one
//! is the whole check. No version file, no build step, nothing to forget on deploy.
//!
//! Web only. The desktop app updates by installer, and a dev server serves unhashed names, so
//! there the running page names no build and the check never starts.

use dioxus::prelude::*;

/// How often an open tab asks. A release is rare and the request is one small page.
const CHECK_EVERY_MS: u32 = 5 * 60 * 1000;

/// Set just before the Refresh reload, so the new build opens What's new once.
const KEY_SHOW_WHATS_NEW: &str = "sk-show-whats-new";

/// The entry script's hashed name, read off a page's markup. Shared by both reads so the running
/// page and the live one are matched by the same pattern.
const ENTRY_PATTERN: &str = r#"/assets\/([A-Za-z0-9_]+-dxh[0-9a-f]+\.js)/"#;

/// The build this tab is running, or `None` where the page names no hashed build.
async fn running_build() -> Option<String> {
    let js = format!(
        "const s = document.querySelector('script[type=module][src*=\"/assets/\"]'); \
         const m = s && s.getAttribute('src').match({ENTRY_PATTERN}); \
         return m ? m[1] : null;"
    );
    document::eval(&js).await.ok()?.as_str().map(str::to_string)
}

/// Wait for the next check: the interval, or the moment the tab is looked at again, whichever
/// comes first. A tab brought back after a day should not show a stale build for five minutes.
async fn wait_for_next_check() {
    let js = format!(
        "await new Promise(done => {{ \
           const wake = () => {{ if (document.visibilityState === 'visible') finish(); }}; \
           const finish = () => {{ clearTimeout(t); document.removeEventListener('visibilitychange', wake); done(); }}; \
           const t = setTimeout(finish, {CHECK_EVERY_MS}); \
           document.addEventListener('visibilitychange', wake); \
         }}); return true;"
    );
    let _ = document::eval(&js).await;
}

/// The build the server is serving now, or `None` when the page could not be fetched (offline,
/// a deploy mid-flight) — which is not news, so it shows nothing.
async fn live_build() -> Option<String> {
    let js = format!(
        "try {{ \
           const r = await fetch('/', {{ cache: 'no-store' }}); \
           if (!r.ok) return null; \
           const m = (await r.text()).match({ENTRY_PATTERN}); \
           return m ? m[1] : null; \
         }} catch (_) {{ return null; }}"
    );
    document::eval(&js).await.ok()?.as_str().map(str::to_string)
}

/// Whether the last session's Refresh asked for What's new, clearing the request as it reads.
pub async fn take_whats_new_request() -> bool {
    let js = format!(
        "try {{ const v = localStorage.getItem({KEY_SHOW_WHATS_NEW:?}); \
           localStorage.removeItem({KEY_SHOW_WHATS_NEW:?}); return v === '1'; }} \
         catch (_) {{ return false; }}"
    );
    matches!(
        document::eval(&js).await.ok().and_then(|v| v.as_bool()),
        Some(true)
    )
}

#[component]
pub fn UpdateBanner() -> Element {
    // The newer build found, held so a dismissal can name what it dismissed: a second release
    // after a dismissed one is news again.
    let mut found = use_signal(|| Option::<String>::None);
    let mut dismissed = use_signal(|| Option::<String>::None);

    use_future(move || async move {
        if !cfg!(target_arch = "wasm32") {
            return;
        }
        let Some(running) = running_build().await else {
            return;
        };
        loop {
            wait_for_next_check().await;
            if let Some(live) = live_build().await {
                if live != running {
                    found.set(Some(live));
                }
            }
        }
    });

    let Some(build) = found() else {
        return rsx! {};
    };
    if dismissed().as_deref() == Some(build.as_str()) {
        return rsx! {};
    }
    rsx! {
        div { class: "update-banner", role: "status",
            span { "Sidekick has received an update! Please" }
            button {
                class: "update-banner__refresh",
                r#type: "button",
                onclick: move |_| {
                    document::eval(&format!(
                        "try {{ localStorage.setItem({KEY_SHOW_WHATS_NEW:?}, '1'); }} catch (_) {{}} \
                         location.reload();"
                    ));
                },
                "Refresh"
            }
            span { "to load it. What's new will open once it has." }
            button {
                class: "update-banner__dismiss",
                r#type: "button",
                "aria-label": "Dismiss",
                title: "Hide until the next update",
                onclick: move |_| dismissed.set(Some(build.clone())),
                "×"
            }
        }
    }
}
