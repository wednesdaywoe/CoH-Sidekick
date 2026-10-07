//! Generic header popover — a labelled trigger button plus a panel that opens beneath it
//! (the beta's `BuildIdentityPopover` / `SettingsPopover` shape). The rebuild's second
//! overlay primitive, beside [`crate::modal`]: a modal takes the whole viewport and suits
//! content-dense pickers, while a popover stays anchored to the control that opened it and
//! suits a short form the user is dipping into and back out of.
//!
//! Positioned `absolute` under the trigger, which is only safe because the header is NOT
//! inside the free grid — a grid surface clips with `overflow: hidden` and is its own stacking
//! context, so an overlay inside one is cut off at the panel edge (the caveat [`crate::modal`]
//! carries, and the reason the enhancement picker is hosted at the shell root). Nothing here
//! may be rendered inside a grid surface.
//!
//! Closes on trigger click, on Escape, and on a click anywhere outside — the last via a
//! transparent full-viewport backdrop behind the panel rather than a document listener,
//! the same mechanism `reorder_menu` uses.

use dioxus::prelude::*;

/// This popover's own open state, offered to whatever it wraps.
///
/// A popover holding a FORM has nothing to say to its contents — the form is edited in place
/// and the user closes it when they are done. A popover holding a MENU does: an entry that
/// acted and left the menu standing over its own result reads as not having worked. Only the
/// second kind consumes this, so it is a context rather than a required prop.
#[derive(Clone, Copy)]
pub struct PopoverOpen(pub Signal<bool>);

/// Provided by a full-screen mobile sheet ([`crate::mobile_nav`]) around the popovers it lists.
/// A popover under it drops its trigger and panel and draws its body in place under its title,
/// and its [`PopoverOpen`] becomes the sheet's, so a menu entry that closes its menu closes the
/// sheet. Every popover reaches the phone this way without a second copy of what it holds.
#[derive(Clone, Copy)]
pub struct PopoverInline(pub Signal<bool>);

/// Slide an open panel sideways until it sits inside the window, with an 8px margin.
///
/// The CSS picks a side per menu (`left: 0`, or `right: 0` for `popover--menu-end`), and that
/// choice is only right while the trigger is where it was designed to be. `.shell-header` wraps
/// on a narrow window, and a trailing trigger that wraps lands at the LEFT edge of the second
/// row, where a panel opening leftward from it runs off the page. So the anchoring stays as the
/// CSS says and this corrects the overshoot after layout, whichever side it is on.
///
/// Every open panel rather than one by id: the backdrop lets one popover be open at a time, and
/// the correction is idempotent — `translate` is cleared before measuring, so a panel that
/// already fits is left exactly where the CSS put it.
const KEEP_PANEL_ON_SCREEN: &str = "\
document.querySelectorAll('.popover-panel').forEach(function (panel) {\
  panel.style.translate = '';\
  var rect = panel.getBoundingClientRect();\
  var margin = 8;\
  var shift = 0;\
  if (rect.right > window.innerWidth - margin) { shift = window.innerWidth - margin - rect.right; }\
  if (rect.left + shift < margin) { shift = margin - rect.left; }\
  if (shift !== 0) { panel.style.translate = shift + 'px 0'; }\
});";

/// A popover. The caller owns nothing: open/closed is local state, since a popover is
/// transient UI with no meaning to persist.
///
/// `label` is the trigger's text — a live summary of what's inside, not a static noun,
/// wherever the content has one to show. `title` heads the open panel.
#[component]
pub fn Popover(
    label: String,
    title: String,
    /// Extra classes on the root, so a caller can restyle its own chip without this
    /// component knowing which popover it is.
    #[props(default = String::new())]
    modifier: String,
    /// A drawn mark for the trigger, where the row this popover sits in uses one to say what
    /// kind of control it is — the quickbar's actions all carry one, and that mark is the
    /// channel separating them from the panel pills below, so a popover on that row without
    /// one would read as a pill.
    #[props(default)]
    mark: Option<Element>,
    children: Element,
) -> Element {
    let inline = try_use_context::<PopoverInline>();
    let mut open = use_signal(|| false);
    use_context_provider(|| PopoverOpen(inline.map_or(open, |sheet| sheet.0)));

    if inline.is_some() {
        return rsx! {
            section { class: "popover-inline {modifier}", "aria-label": "{title}",
                h3 { class: "popover-inline__title", "{title}" }
                div { class: "popover-body", {children} }
            }
        };
    }

    rsx! {
        div { class: "popover {modifier}",
            button {
                class: "popover-trigger",
                "aria-expanded": open(),
                "aria-haspopup": "dialog",
                onclick: move |_| open.toggle(),
                if let Some(mark) = mark {
                    {mark}
                }
                span { class: "popover-label", "{label}" }
                span { class: "popover-caret", "▼" }
            }
            if open() {
                // Transparent, full-viewport, and BEHIND the panel: an outside click lands
                // here and closes; a click on the panel never reaches it.
                div { class: "popover-backdrop", onclick: move |_| open.set(false) }
                div {
                    class: "popover-panel",
                    role: "dialog",
                    "aria-label": "{title}",
                    tabindex: "-1",
                    // Focused on mount so Escape reaches this node without a document-level
                    // listener (the same trick `Modal` uses).
                    onmounted: move |evt| async move {
                        document::eval(KEEP_PANEL_ON_SCREEN);
                        let _ = evt.set_focus(true).await;
                    },
                    onkeydown: move |evt| {
                        if evt.key() == Key::Escape {
                            open.set(false);
                        }
                    },
                    div { class: "popover-head",
                        span { class: "popover-title", "{title}" }
                        button {
                            class: "popover-close",
                            "aria-label": "Close",
                            onclick: move |_| open.set(false),
                            "✕"
                        }
                    }
                    div { class: "popover-body", {children} }
                }
            }
        }
    }
}
