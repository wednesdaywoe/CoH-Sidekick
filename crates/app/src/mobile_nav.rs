//! Mobile bottom nav — the fixed <900px tab bar (beta `MobileBottomNav`).
//!
//! Five tabs, a mark over a word each: Home, Dashboard, Options, Incarnate, Menu. The beta had
//! Incarnate before Options; Options is opened far more, and incarnates only matter at 50. Dashboard,
//! Options and Menu open a full-screen sheet that stops at the bar's top edge, so the bar stays
//! on screen and switching tabs is one tap. Incarnate opens the incarnate picker, which is a
//! modal already. Home closes whatever is open, and when nothing is, scrolls the stack back to
//! the top — the beta's "back to the planner", which needs no second meaning while nothing
//! covers the planner.
//!
//! The sheets hold no menus of their own. They list the header's popovers under
//! [`PopoverInline`], which makes each one draw its body in place under its title, so every
//! entry here is the desktop's entry and a fix to one is a fix to both.

use crate::grid::model::{GridItem, PanelKind};
use crate::main_menu::{FileMenu, HelpMenu, OptionsMenu};
use crate::panels::dashboards::{surfaces, title_of, DashboardConfig};
use crate::panels::incarnate_picker::IncarnatePickerOpen;
use crate::popover::PopoverInline;
use crate::quickbar::{model::QuickBarItem, DisplayMenu, ToolsMenu};
use crate::shell::{CombatPopover, Db, IdentityPopover};
use crate::view::marks;
use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Sheet {
    Dashboard,
    Options,
    Menu,
}

impl Sheet {
    fn title(self) -> &'static str {
        match self {
            Sheet::Dashboard => "Dashboard",
            Sheet::Options => "Options",
            Sheet::Menu => "Menu",
        }
    }
}

#[component]
pub fn MobileNav(
    database: Option<Db>,
    dataset: Signal<coh_data::DatasetId>,
    pins: Signal<Vec<QuickBarItem>>,
    layout: Signal<Vec<GridItem>>,
    reorder_open: Signal<bool>,
    reset_confirm_open: Signal<bool>,
) -> Element {
    let mut sheet = use_signal(|| None::<Sheet>);
    let mut incarnate = use_context::<IncarnatePickerOpen>().0;
    // The picker opens on its first tab; it has the slot tabs to move between the rest. `None`
    // while the bundle loads or on a dataset with no incarnates, and the tab is disabled then.
    let first_slot = database.as_ref().and_then(|db| {
        db.offered_incarnate_slots()
            .first()
            .map(|slot| slot.id.clone())
    });

    let mut switch_to = move |next: Sheet| {
        incarnate.set(None);
        sheet.set(if sheet() == Some(next) {
            None
        } else {
            Some(next)
        });
    };
    let home_active = sheet().is_none() && incarnate().is_none();

    rsx! {
        nav {
            // Raised over the modal layer while the incarnate picker is open, so the tab that
            // opened it can close it and the others can switch away — the beta's bar stayed live
            // over its incarnate modal. Only for that picker: any other modal is a task the bar
            // has no part in.
            class: if incarnate().is_some() { "mobile-nav mobile-nav--over-modal" } else { "mobile-nav" },
            "aria-label": "Mobile",
            if let Some(open) = sheet() {
                MobileSheet { key: "{open.title()}", sheet, title: open.title(),
                    match open {
                        Sheet::Dashboard => rsx! { DashboardSheet {} },
                        Sheet::Options => rsx! {
                            if let Some(database) = database.clone() {
                                IdentityPopover { database, dataset }
                            }
                            CombatPopover { database: database.clone() }
                            DisplayMenu { pins, layout, reorder_open, reset_confirm_open }
                        },
                        Sheet::Menu => rsx! {
                            FileMenu { database: database.clone(), dataset }
                            ToolsMenu { pins, layout, database: database.clone() }
                            OptionsMenu {}
                            HelpMenu {}
                        },
                    }
                }
            }
            NavTab {
                label: "Home",
                active: home_active,
                onclick: move |_| {
                    if home_active {
                        let _ = document::eval("window.scrollTo({ top: 0, behavior: 'smooth' });");
                    }
                    sheet.set(None);
                    incarnate.set(None);
                },
                {marks::home()}
            }
            NavTab {
                label: "Dashboard",
                active: sheet() == Some(Sheet::Dashboard),
                onclick: move |_| switch_to(Sheet::Dashboard),
                {marks::dashboard()}
            }
            NavTab {
                label: "Options",
                active: sheet() == Some(Sheet::Options),
                onclick: move |_| switch_to(Sheet::Options),
                {marks::sliders()}
            }
            NavTab {
                label: "Incarnate",
                active: incarnate().is_some(),
                disabled: first_slot.is_none(),
                onclick: move |_| {
                    sheet.set(None);
                    incarnate.set(if incarnate().is_some() { None } else { first_slot.clone() });
                },
                {marks::incarnate()}
            }
            NavTab {
                label: "Menu",
                active: sheet() == Some(Sheet::Menu),
                onclick: move |_| switch_to(Sheet::Menu),
                {marks::menu()}
            }
        }
    }
}

#[component]
fn NavTab(
    label: &'static str,
    active: bool,
    #[props(default)] disabled: bool,
    onclick: EventHandler<MouseEvent>,
    children: Element,
) -> Element {
    rsx! {
        button {
            class: if active { "mobile-nav__tab is-active" } else { "mobile-nav__tab" },
            r#type: "button",
            "aria-pressed": active,
            disabled,
            onclick: move |evt| onclick.call(evt),
            {children}
            span { class: "mobile-nav__label", "{label}" }
        }
    }
}

/// The sheet: everything above the bar, a title row, and a scrolling body.
///
/// It lives inside the bar, which is `position: fixed` and so a stacking context at z 100. That
/// puts the sheet over the sticky mobile header (z 40) and under every modal (z 1000), so a
/// picker or a confirm raised from inside a sheet opens over it rather than behind.
#[component]
fn MobileSheet(sheet: Signal<Option<Sheet>>, title: &'static str, children: Element) -> Element {
    // The popovers listed in this sheet take this as their open state. A menu entry that acts
    // closes its menu, and here that has to close the sheet, or the sheet would stand over the
    // dialog the entry just raised.
    let open = use_signal(|| true);
    use_context_provider(|| PopoverInline(open));
    use_effect(move || {
        if !open() {
            sheet.set(None);
        }
    });

    rsx! {
        div {
            class: "mobile-sheet",
            role: "dialog",
            "aria-label": "{title}",
            tabindex: "-1",
            onmounted: move |evt| async move {
                let _ = evt.set_focus(true).await;
            },
            onkeydown: move |evt| {
                if evt.key() == Key::Escape {
                    sheet.set(None);
                }
            },
            header { class: "mobile-sheet__head",
                h2 { class: "mobile-sheet__title", "{title}" }
                button {
                    class: "mobile-sheet__close",
                    r#type: "button",
                    "aria-label": "Close",
                    onclick: move |_| sheet.set(None),
                    {marks::close()}
                }
            }
            div { class: "mobile-sheet__body", {children} }
        }
    }
}

/// Every stat panel the user has built, in their order, whether or not it is shown in the
/// stack — this tab is where the numbers are read, so a panel hidden to shorten the stack is
/// still one tap away.
#[component]
fn DashboardSheet() -> Element {
    let config = use_context::<DashboardConfig>().0;
    let panels: Vec<(String, PanelKind)> = surfaces(&config.read())
        .into_iter()
        .filter(|panel| matches!(panel, PanelKind::Dashboard(_)))
        .map(|panel| (title_of(panel, &config.read()), panel))
        .collect();

    rsx! {
        for (index, (title, panel)) in panels.into_iter().enumerate() {
            if let PanelKind::Dashboard(id) = panel {
                section { key: "{index}", class: "mobile-sheet__panel",
                    h3 { class: "popover-inline__title", "{title}" }
                    crate::panels::stats::StatGroup { panel: id }
                }
            }
        }
    }
}
