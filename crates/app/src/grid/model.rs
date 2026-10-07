//! The pure free-grid data model: `PanelKind` (which surface), `GridItem` (a
//! surface's 0-based `{x,y,w,h}` cell rectangle with size limits), and
//! `GridConfig` (the cell↔pixel geometry). No Dioxus, no I/O — plain data the
//! `math` and `collide` modules operate on and the view renders from.

use serde::{Deserialize, Serialize};

/// The surfaces a fresh install has: the fixed five, plus the four category panels
/// `Dashboards::defaults()` mints.
///
/// Stated here rather than read off the roster, because the roster lives a layer up — the grid
/// is kept ignorant of what a dashboard CONTAINS, and reaching into `panels::dashboards` for the
/// default would be the grid learning it. So this is a restatement, and a restatement drifts:
/// `the_authored_default_seats_exactly_the_default_roster` in `panels::dashboards` is what holds
/// the two together, from the side that can see both.
pub const DEFAULT_SURFACES: [PanelKind; 9] = [
    PanelKind::Powers,
    PanelKind::Available,
    PanelKind::Pools,
    PanelKind::SetBonuses,
    PanelKind::Info,
    PanelKind::Dashboard(DashboardId(1)),
    PanelKind::Dashboard(DashboardId(2)),
    PanelKind::Dashboard(DashboardId(3)),
    PanelKind::Dashboard(DashboardId(4)),
];

/// A user-built dashboard panel's identity, and the whole of it: a dashboard has no fixed
/// meaning for the grid to know. What it is CALLED and which stats it holds are user data, held
/// in [`crate::panels::dashboards`] — the grid only ever needs to tell one surface from another.
///
/// A `u32` newtype rather than a `String` so [`PanelKind`] stays `Copy`, which the whole grid
/// leans on: `GridItem` is `Copy`, the collision pass carries a `HashSet<PanelKind>`, and every
/// read of the committed layout is a `.iter().copied()`.
///
/// Ids are minted monotonically and never reused — see `Dashboards::add_panel`. Reuse would be
/// invisible until it wasn't: a deleted panel's rectangle survives in the OTHER column count's
/// slot of `sk-layout` (a delete only rewrites the count it happened at), so a re-minted id
/// would silently inherit the dead panel's cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DashboardId(pub u32);

/// A surface the grid can hold: one of four fixed kinds, or one of however many dashboard
/// panels the user has built.
///
/// Retiring a variant is a persistence event, not just a code change: a stored layout naming a
/// surface this build no longer has can't deserialize into this enum, so `layout_store` drops
/// such entries rather than letting one retired name discard the whole saved layout (see
/// `load_desktop`). `Build`/`Browser`/`Totals` took the gentler route of aliasing onto a
/// successor surface; `Identity` and `Combat` have none — they moved off the grid into header
/// popovers entirely, and the eight fixed stat panels have none because there is nothing for
/// them to alias ONTO: their successor is a panel whose identity the user minted.
///
/// [`Self::Dashboard`] is the first variant that carries data, and that changes what this enum
/// is. It used to be the roster — `ALL` was a completion set, and "every surface" was a const
/// the grid could read for itself. It is now only the VOCABULARY of surfaces; which ones exist
/// is [`crate::panels::dashboards::surfaces`]'s answer, handed in wherever the grid used to help
/// itself. [`Self::FIXED`] is what is left of `ALL`, and it is not a roster — it is the four
/// surfaces that exist whether or not the user has built anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PanelKind {
    /// The build's loadout: the picked powers as slotted cards (primary/secondary/pools/epic),
    /// each with its enhancement slots and the slot picker. The *available* powers to pick
    /// from live on their own [`PanelKind::Available`] selection surface.
    Powers,
    /// The selection surface: every primary/secondary power as a level-ordered numbered row;
    /// clicking an unpicked one adds it to the loadout on [`PanelKind::Powers`]. The POOLS'
    /// rows are [`PanelKind::Pools`]' — see there for why the split.
    Available,
    /// The pools: the two triggers that choose which pools the build holds, over the same
    /// level-ordered rows [`PanelKind::Available`] draws for the powersets.
    ///
    /// Split off Available in LAY6, on a difference in how the two halves GROW. A powerset's
    /// list is as long as the archetype makes it and never changes again; the pools' is a list
    /// longer every time a pool is taken, which on the shared surface meant taking a fourth
    /// pool pushed the powersets up and out of the panel that exists to show them. A surface
    /// whose height is something the user does to the build needs a rectangle of its own, and
    /// a rectangle is what the grid hands out.
    ///
    /// It is one surface and not two — the triggers could have stayed behind — because the
    /// choice and the picking are one act: nobody takes Fighting, they take Tough. Separating
    /// the control from the list it fills would put the reason for the choice on a different
    /// panel from the choice.
    Pools,
    /// What the build's slotted IO sets are granting, one row per stat they feed. A surface of
    /// its own rather than a section of the stat panels, because it answers a question about the
    /// SLOTTING rather than about a stat: the stat panels are read while planning a number, this
    /// is read while spending slots.
    SetBonuses,
    Info,
    /// One of the user's dashboard panels — whichever stats they put in it, under whatever name
    /// they gave it. Both of those live in [`crate::panels::dashboards`]; all the grid holds is
    /// the id, because a rectangle does not need to know what is drawn in it.
    ///
    /// This replaced eight fixed variants (Offense, Defense, Resistance, Survival, Movement,
    /// StatusProtection, StatusResistance, DebuffResistance). Their doc used to argue that a
    /// stat group deserved its own surface so it could be placed and sized on its own merits,
    /// and that was true as far as it went. What it never weighed is that the grouping itself
    /// was not the user's: a build steered by three numbers had to show three panels to see
    /// them, each carrying a dozen rows nobody was reading.
    Dashboard(DashboardId),
}

/// A cluster of surfaces in the quickbar's More menu, assigned by [`PanelKind::group`].
///
/// Never rendered as a word — the clusters are stated by the seams between them, not by
/// captions, because a caption per cluster would cost four rows of chrome to say what four
/// lines already say. The names exist so the split is decidable in code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelGroup {
    /// The user's own panels.
    Dashboards,
    /// What this build is made of: the picked powers, the ones still to pick, and what the
    /// slotted sets grant.
    Loadout,
    /// The reference surface.
    Reference,
}

impl PanelGroup {
    /// Every cluster, in menu order. Exhaustive for the same reason [`PanelKind::FIXED`] is: a
    /// cluster left out here would take every surface assigned to it out of the menu silently,
    /// which for a surface that is neither pinned nor on the grid is every way back to it.
    ///
    /// Three clusters, down from four. `Capacity` and `Mitigation` divided the eight fixed stat
    /// panels between them — what the character can do, and what reduces what lands on it —
    /// and retiring those eight left both with nothing to name. The division was a real one and
    /// it did not survive the surfaces it described: a panel the user built holds whatever they
    /// put in it, so no cluster can say in advance what a dashboard is about.
    pub const ALL: [PanelGroup; 3] = {
        match PanelGroup::Loadout {
            PanelGroup::Dashboards | PanelGroup::Loadout | PanelGroup::Reference => {}
        }
        [
            PanelGroup::Dashboards,
            PanelGroup::Loadout,
            PanelGroup::Reference,
        ]
    };

    /// This cluster's surfaces, in the roster's own order. Filtering the one list rather
    /// than restating a per-cluster one is what keeps the two orders from drifting apart.
    pub fn panels(self, roster: &[PanelKind]) -> Vec<PanelKind> {
        roster
            .iter()
            .copied()
            .filter(|panel| panel.group() == self)
            .collect()
    }
}

impl PanelKind {
    /// The surfaces that exist whether or not the user has built anything.
    ///
    /// **Not a roster.** This was `ALL`, and `ALL` was the completion set every load path
    /// reconciled against — which worked exactly as long as the set was knowable without asking
    /// anyone. It is not any more, and the rename is the point: a caller reaching for "every
    /// surface" and finding `FIXED` is being told it has to ask
    /// [`crate::panels::dashboards::surfaces`] instead, rather than getting five of them and a
    /// silent absence of the rest.
    ///
    /// The exhaustive `match` below fails to compile when a fixed variant is added, forcing this
    /// list to be extended with it. `Dashboard` is deliberately outside it.
    // Unused by any caller BY DESIGN, and that is not the same as dead. The exhaustive `match`
    // in the body is a COMPILE-TIME tripwire: adding a fixed variant to `PanelKind` fails to
    // compile until this list is extended with it, which is the whole point of the const. A
    // runtime caller would be nice to have and is not what makes it load-bearing. Annotated
    // rather than deleted 2026-09-26; deleting it removes a check, not a leftover.
    #[allow(dead_code)]
    pub const FIXED: [PanelKind; 5] = {
        match PanelKind::Powers {
            PanelKind::Powers
            | PanelKind::Available
            | PanelKind::Pools
            | PanelKind::SetBonuses
            | PanelKind::Info
            | PanelKind::Dashboard(_) => {}
        }
        [
            PanelKind::Powers,
            PanelKind::Available,
            PanelKind::Pools,
            PanelKind::SetBonuses,
            PanelKind::Info,
        ]
    };

    /// The dashboard id this surface draws, or `None` for a fixed one.
    pub fn dashboard(&self) -> Option<DashboardId> {
        match self {
            PanelKind::Dashboard(id) => Some(*id),
            _ => None,
        }
    }

    /// The name a FIXED surface carries, or `None` for a dashboard panel, whose name the user
    /// typed and which therefore is not this module's to know.
    ///
    /// Nothing renders from this directly — [`crate::panels::dashboards::PanelNames`] is what
    /// every surface's header, pill, menu row and `aria-label` goes through, and it answers a
    /// `String` for every surface including the user's. The `Option` is here so the split is
    /// checked by the compiler: a fixed title is a fact this enum owns, a dashboard's name is
    /// not, and an arm that returned `"Dashboard"` to avoid saying so would put a placeholder in
    /// the one place a user looks to tell two panels apart.
    pub fn fixed_title(&self) -> Option<&'static str> {
        match self {
            PanelKind::Powers => Some("Powers"),
            PanelKind::Available => Some("Available"),
            PanelKind::Pools => Some("Power Pools"),
            PanelKind::SetBonuses => Some("Set Bonuses"),
            PanelKind::Info => Some("Info"),
            PanelKind::Dashboard(_) => None,
        }
    }

    /// Which cluster of the quickbar's More menu this surface sits in.
    ///
    /// Thirteen evenly-spaced rows is a list read to the end of once and skimmed forever
    /// after; the clusters give the eye somewhere to aim. The grouping is the app's own
    /// division of its surfaces and answers no game question, so it lives here beside the
    /// enum rather than in the export.
    ///
    /// Exhaustive by design: a new surface is a compile error here, not one that quietly
    /// lands in whichever cluster a catch-all arm happened to name.
    pub fn group(&self) -> PanelGroup {
        match self {
            PanelKind::Dashboard(_) => PanelGroup::Dashboards,
            PanelKind::Powers | PanelKind::Available | PanelKind::Pools | PanelKind::SetBonuses => {
                PanelGroup::Loadout
            }
            PanelKind::Info => PanelGroup::Reference,
        }
    }

    /// The width a surface gets when it has no saved one — appended by
    /// [`reconcile_layout`](crate::grid::collide::reconcile_layout) after being added to
    /// `ALL`, or rebuilt from a legacy format that stored no width. A uniform third of the
    /// grid, which is a *safe* width rather than a good one: nothing about a surface arriving
    /// without a saved size says what it should be, and a third reads acceptably for every
    /// one of them. [`default_layout`](GridItem::default_layout) does not use this — it hands
    /// each surface the width its content actually wants.
    pub fn default_width(&self) -> u8 {
        match self {
            PanelKind::Powers
            | PanelKind::Available
            | PanelKind::Pools
            | PanelKind::SetBonuses
            | PanelKind::Info
            | PanelKind::Dashboard(_) => 4,
        }
    }

    /// The surface's key in markup: its `data-id`, its Dioxus render key, the selector a scroll
    /// reaches it by, the prefix on its quickbar slug.
    ///
    /// A `String` rather than a `&'static str`, because a dashboard's key is minted from its id
    /// and there is nothing static to borrow. `QuickBarItem::slug` already answers a `String`
    /// for the same reason, and every caller here is already building a DOM attribute out of it.
    pub fn slug(&self) -> String {
        match self {
            PanelKind::Powers => "powers".to_string(),
            PanelKind::Available => "available".to_string(),
            PanelKind::Pools => "pools".to_string(),
            PanelKind::SetBonuses => "set-bonuses".to_string(),
            PanelKind::Info => "info".to_string(),
            PanelKind::Dashboard(id) => format!("dashboard-{}", id.0),
        }
    }

    /// Whether this surface's height is *measured from its content* rather than chosen.
    ///
    /// A readout — a short labelled list whose natural height is small and knowable — has
    /// no business owning a scrollbar. A stat group scrolled halfway is a number the build
    /// just changed and the eye never saw, and a dozen of them is a dozen places a number
    /// can hide. These surfaces are drawn at exactly the height their content needs,
    /// re-measured whenever that content or the surface's width changes, so `h` for them is
    /// a value the layout carries rather than a size anyone picked.
    ///
    /// The workspace surfaces are the other kind, and deliberately so. Powers, Available and
    /// Info grow without bound as a build is made; fitting them would mean a page metres
    /// tall with the totals scrolled off the end of it. They keep a stored height and own
    /// their scroll, which is what a document pane is for.
    ///
    /// Set Bonuses is not fitted. It reads like a readout on a fresh build and grows like a
    /// workspace on a finished one, and it is the second kind that has to be sized for.
    pub fn fits_content(&self) -> bool {
        match self {
            PanelKind::Dashboard(_) => true,
            PanelKind::Powers
            | PanelKind::Available
            | PanelKind::Pools
            | PanelKind::SetBonuses
            | PanelKind::Info => false,
        }
    }

    /// Smallest usable size in cells `(columns, rows)`. Free `h` means a panel
    /// can be dragged arbitrarily short; each surface names the floor below which
    /// its content stops being usable (the powers/info panels carry internal
    /// scroll and need several rows to show anything). Width is a uniform
    /// 2-column floor (~1/6 of the grid): every surface reflows comfortably that
    /// narrow, so the height floor is what's tuned per surface, not the width.
    ///
    /// The height floor is a bound on what a *drag* may ask for, so it says nothing about
    /// a [`fits_content`](PanelKind::fits_content) surface: its height is answered by its
    /// content, which no drag proposes and no floor may overrule. Every fitted surface still
    /// carries the lowest floor of all, and it is what they would fall back to if one ever
    /// stopped fitting.
    pub fn min_size(&self) -> (u32, u32) {
        match self {
            PanelKind::Powers => (2, 6),
            PanelKind::Available => (2, RAIL_MIN_ROWS),
            // The trigger row plus one pool list under it — what the surface has to draw to
            // be saying anything. [`POOLS_MIN_ROWS`] is this number, and the default's split
            // is derived from the pair: see [`POOLS_SPLIT_ROWS`].
            PanelKind::Pools => (2, POOLS_MIN_ROWS),
            // A couple of stat rows, which is all an intentionally small panel is. The floor
            // has to admit an EMPTY one too, since a user can make a panel before filling it.
            PanelKind::Dashboard(_) => (2, 3),
            // One group heading plus a row or two, the dashboard's floor: this list is as
            // shrinkable as a stat panel, and a build with two set bonuses should be able to
            // keep it that small.
            PanelKind::SetBonuses => (2, 3),
            PanelKind::Info => (2, 4),
        }
    }

    /// Largest size in cells `(columns, rows)`; `None` means unbounded (the grid
    /// column count caps width, nothing caps height). No surface caps its size
    /// today — the hook exists so a surface that becomes unusable when stretched
    /// can say so without the grid engine changing.
    pub fn max_size(&self) -> (Option<u32>, Option<u32>) {
        match self {
            PanelKind::Powers
            | PanelKind::Available
            | PanelKind::Pools
            | PanelKind::SetBonuses
            | PanelKind::Info
            | PanelKind::Dashboard(_) => (None, None),
        }
    }
}

/// A surface's placement on the free grid, in 0-based integer cells. Pixels are
/// always derived from this plus the live [`GridConfig`] and container width —
/// never stored — which is what keeps a saved layout responsive and serializable.
///
/// Invariant enforced at every mutation point (never checked after the fact):
/// `x + w <= config.columns`. `y` is unbounded; the grid grows downward.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridItem {
    pub panel: PanelKind,
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub min_width: u32,
    pub min_height: u32,
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
    /// Immovable; other items route around it. No UI sets this yet (routing is
    /// deferred), but `compact`/`resolve_collisions` already honor it, so a
    /// pinned item is never floated or pushed.
    #[serde(default)]
    pub pinned: bool,
    /// Folded down to its header, drawing no body.
    ///
    /// `h` keeps meaning the *expanded* height while this is set, so unfolding restores
    /// the size the surface was folded at with no second stored number to keep in sync.
    /// Everything that asks how many rows the surface occupies asks [`GridItem::height`].
    #[serde(default)]
    pub collapsed: bool,
    /// Off the grid entirely — not drawn, and inert to the collision engine.
    ///
    /// A hidden surface KEEPS its entry and its whole rectangle rather than leaving the
    /// layout: `reconcile_layout` appends any surface a save doesn't name, so a removed
    /// entry would come straight back at the bottom of the grid. Holding the rectangle is
    /// also what lets showing it again return it to the cell it was hidden from.
    ///
    /// Inert means every pure fn treats it as absent: [`collides`](crate::grid::collide::collides)
    /// never reports it, `compact` leaves it where it is, and the container's height and a
    /// reconcile's append row are both measured over the visible surfaces alone. So the
    /// stored rectangle is free to overlap live ones while hidden — which it will, as soon
    /// as its neighbours float up into the space it vacated.
    #[serde(default)]
    pub hidden: bool,
}

/// The rows a folded surface occupies: one, which is its header and nothing else.
pub const COLLAPSED_ROWS: u32 = 1;

/// The row the four build columns start on — the band's own height in the authored default.
pub const COLUMN_START_ROW: u32 = 3;

/// The authored height of a band surface. Every one of them fits its content, so this is a
/// seed the running app overwrites (see [`GridItem::default_layout`]); it only has to be
/// plausible enough that a fresh layout is already compacted.
pub(crate) const BAND_SEED_ROWS: u32 = 3;

/// The column height a window that cannot be measured gets. It is the height the default was
/// designed at — Available needs 20 rows to show an unpicked list without scrolling — and it
/// is the honest guess when the grid is `display: none` and has no room to report.
///
/// **No longer a cap.** VF1 clamped every measured window to this on the reasoning that a
/// taller column is empty space inside the panel rather than more content. That was measured
/// against Available, the one column it is true of, and it is false for the other three: a
/// 1440p screen left ~500px unused under the layout while Powers, Info and Set Bonuses all had
/// content to put there. The cap was reversed in VF3 on that report.
pub const DEFAULT_COLUMN_ROWS: u32 = 20;

/// The fewest rows the pools are ever drawn at: the trigger row over one pool list.
///
/// One number, read twice — it is the pools' half of [`PanelKind::min_size`] AND the floor the
/// default never seeds them under. A seed floor and a drag floor are different things in
/// general (a floor bounds what a gesture may PROPOSE, and an authored default is not a
/// gesture), and they are the same number here because both answer the same question: below
/// this the surface is a title bar over nothing.
const POOLS_MIN_ROWS: u32 = 6;

/// Available's half of the same pair — its [`PanelKind::min_size`] height, restated as a const
/// so [`POOLS_SPLIT_ROWS`] can be derived at compile time. `min_size` is a `match`, not a
/// `const fn`, so the two are held together by `a_split_column_seeds_neither_half_under_its_floor`
/// rather than by the compiler.
const RAIL_MIN_ROWS: u32 = 6;

/// The shortest left column that is authored as TWO surfaces rather than one.
///
/// The two floors added, so the split is taken only where both halves can be read. Under it the pools open [off the grid](GridItem::hidden)
/// instead — the trade the narrow default already makes for Info and Set Bonuses, and for the
/// same reason: a surface with no room to be is better in the Display menu's roster than
/// seeded into a keyhole, or seeded past the bottom of a window the whole default is sized
/// to fit.
///
/// **Only the eight-column default asks this since LAY8.** The twelve-column one gave the pools
/// a column of their own, so there is no left column to divide there and no height at which the
/// pools have to leave the grid; what is left below is about the narrow step, which is the one
/// where a shared column is still the only seat available.
///
/// It is NOT pinned by default for that case, and deliberately: `default_pins` pins what the
/// AUTHORED default ships hidden, and both defaults ship the pools on the grid at the heights
/// they are sized for. A pin minted for a window height would be one the user keeps after
/// resizing out of it.
const POOLS_SPLIT_ROWS: u32 = POOLS_MIN_ROWS + RAIL_MIN_ROWS;

/// The shortest the columns are ever authored. Below this the surfaces would be seeded under
/// their own [`PanelKind::min_size`], so a window this short keeps the floor and pays for it
/// with a page scrollbar — which is the honest failure, not a layout nothing can be read in.
pub const MIN_COLUMN_ROWS: u32 = 8;

/// The column height that fits a grid with `available_px` of room, floored at
/// [`MIN_COLUMN_ROWS`] and bounded above by nothing but the window itself.
///
/// `available_px` is the space the grid container has, not the window: the header, the
/// quickbar and the grid's own bottom inset are already spent by the time this is asked.
///
/// There is no upper clamp because `rows_that_fit` already is one — it answers the tallest
/// extent that still fits, so a taller answer is a taller window and nothing else. A constant
/// ceiling here would be a second opinion about a number the window has already settled, which
/// is exactly what stranded 500px under a 1440p layout.
pub fn column_rows_for(available_px: f64, config: &GridConfig) -> u32 {
    let extent = super::math::rows_that_fit(available_px, config);
    extent.saturating_sub(COLUMN_START_ROW).max(MIN_COLUMN_ROWS)
}

/// The tallest authored surface's height in rows — the statistic the height re-fit compares to
/// decide whether a layout still answers the room it has.
///
/// Fitted surfaces are excluded because their heights are MEASURED and land a frame after the
/// layout that seeded them, so a key that counted them would see its own fit as a change.
/// Hidden ones are excluded because they are not drawn and set no extent.
///
/// **It is a proxy, and both sides of the comparison must go through it.** The number is not the
/// column height: `default_layout_sized` authors Info [`COLUMN_START_ROW`] rows taller than its
/// siblings, so the key exceeds the column height by 3 at the twelve-column step, and the
/// narrow default's split column makes it a different relation again. Comparing this against a
/// bare `column_rows_for` answer therefore reads "already fitted" whenever the layout is short
/// by exactly that offset — which is a window shrunk and grown back to where it was. Measured
/// 2026-09-26 as 134px of dead window at a height that left 26px on the way down, and the three
/// rows between those two numbers are this offset.
pub fn tallest_authored_rows(items: &[GridItem]) -> Option<u32> {
    items
        .iter()
        .filter(|item| !item.hidden && !item.panel.fits_content())
        .map(|item| item.h)
        .max()
}

/// The narrowest a column may draw before the grid stops honouring its column count.
///
/// Derived, not chosen. The grid's uniform width floor is two columns
/// ([`PanelKind::min_size`]), so a two-column surface is the narrowest thing the layout can
/// contain, and it still has to hold a panel header. That header spends a fixed 96px on
/// chrome — a 12px title inset and an 84px control lane (`2 * --panel-seat + 6px`, which is
/// two buttons and does not shrink) — before the title gets a pixel, and the longest title
/// the roster has is `Set Bonuses` at 97px. So the surface needs ~200px, and a two-column
/// surface measures `2 * col + margin.0`, which puts the column floor at 96.
///
/// Measured 2026-08-24 against the release build at 1024px, where twelve columns made a band
/// surface 160px: its header gave the title 54px of the 68 `Survival` needs, and seven of ten
/// titles were clipped. The page never overflowed — this is the axis where nothing was ever
/// too wide, only too thin.
pub const MIN_COL_PX: f64 = 96.0;

/// The column counts the grid steps through, widest first.
///
/// Two entries, and the second bound is why there is no third: the mobile stack takes the
/// whole layout below 900px (`app.css`, `@media (min-width: 900px)`), so the desktop grid
/// never sees a container narrower than ~884px, and eight columns clear [`MIN_COL_PX`] at
/// 824. A six-column step would be unreachable code that reads as a supported width.
///
/// Every count here needs a default authored for it — see [`GridItem::default_layout_for`].
/// Adding a step without one is what turns a narrower window into a taller page.
pub const COLUMN_STEPS: [u32; 2] = [12, 8];

/// The column count a container of `container_width` can pay for at [`MIN_COL_PX`] a column:
/// the widest [`COLUMN_STEPS`] entry that clears the floor, or the narrowest if none does.
///
/// A non-positive width answers the widest rather than the narrowest. The container reports
/// its width asynchronously and reads 0.0 until it has ([`GridContainer`] gates first paint on
/// it), and an unmeasured grid must not spend that gap in the layout a small window gets —
/// the wrong answer for a moment is a reflow the user watches happen.
///
/// [`GridContainer`]: crate::grid::view::GridContainer
pub fn columns_for(container_width: f64, margin_x: f64) -> u32 {
    let widest = COLUMN_STEPS[0];
    if container_width <= 0.0 {
        return widest;
    }
    COLUMN_STEPS
        .into_iter()
        .find(|&count| {
            let gaps = margin_x * (count as f64 - 1.0);
            (container_width - gaps) / count as f64 >= MIN_COL_PX
        })
        .unwrap_or(COLUMN_STEPS[COLUMN_STEPS.len() - 1])
}

impl GridItem {
    /// Place `panel` at `(x, y)` sized `(w, h)`, seeding its size limits from the
    /// surface. The one constructor, so every item carries the surface's real
    /// minimums rather than a caller-remembered guess.
    pub fn new(panel: PanelKind, x: u32, y: u32, w: u32, h: u32) -> GridItem {
        let mut item = GridItem {
            panel,
            x,
            y,
            w,
            h,
            min_width: 0,
            min_height: 0,
            max_width: None,
            max_height: None,
            pinned: false,
            collapsed: false,
            hidden: false,
        };
        item.refresh_limits();
        item
    }

    /// The same item, authored off the grid. The whole rectangle is still meaningful while
    /// hidden — it is the cell the surface returns to when it is shown — so this is a builder
    /// over a placed item rather than a way of leaving one out.
    pub fn off_grid(mut self) -> GridItem {
        self.hidden = true;
        self
    }

    /// The rows this surface occupies right now — [`COLLAPSED_ROWS`] while folded, `h`
    /// otherwise. Collision, compaction, pixel geometry, and the container's own height all
    /// read this instead of `h`; that is what stops a folded surface from reserving room
    /// for a body it isn't drawing, which is the entire point of folding it.
    pub fn height(&self) -> u32 {
        if self.collapsed {
            COLLAPSED_ROWS
        } else {
            self.h
        }
    }

    /// Reseed the size limits from the panel kind. Limits are a property of the
    /// surface, not the build, so a persisted layout carries stale values whenever
    /// a `min_size`/`max_size` is retuned; the load path calls this so the current
    /// floor takes effect everywhere rather than freezing into old saves.
    pub fn refresh_limits(&mut self) {
        let (min_width, min_height) = self.panel.min_size();
        let (max_width, max_height) = self.panel.max_size();
        self.min_width = min_width;
        self.min_height = min_height;
        self.max_width = max_width;
        self.max_height = max_height;
    }

    /// The default arrangement, 0-based (decision 2026-08-06, user-chosen): a band of readouts
    /// across the top, and under it the four columns a build is made in — the powers you can
    /// take, the ones you took, the detail on one of them, and what the slotting is granting.
    ///
    /// The band is five two-cell surfaces across ten of the twelve columns in one row:
    /// Survival, Offense, Defense, Resistance, Movement. Two cells is a sixth of the grid
    /// (~285px), the width a label/value stat row was sized against, and reading the groups
    /// across rather than down puts every number a build is judged by on one line above the
    /// columns instead of in a stack beside them. It also costs the columns nothing: the band
    /// is three rows, and the height it takes is height the tallest column was not using.
    ///
    /// That width is no longer one stat row wide: `.stats` seats two columns once its body
    /// clears ~252px, which a two-cell surface does from roughly a 1680px window up — so the
    /// band's groups draw half as tall there, and the longest labels ellipsize to buy it. A third cell clears both at any window, which is the trade to revisit here if
    /// the truncation reads worse than the extra row costs.
    ///
    /// Every `3` in the band is a seed, not a measurement. All five surfaces
    /// [fit their content](PanelKind::fits_content), so the running app measures each one and
    /// writes its real row count before the first paint the user sees; the seed only has to be
    /// plausible enough that a fresh layout is already compacted and doesn't reshuffle on the
    /// way to its fitted heights. The columns' heights below ARE chosen, and the paragraph
    /// after them says against what.
    ///
    /// Resistance and Movement are in the band open, though the default stat set leaves both
    /// empty (`stat_registry::DEFAULT_VISIBLE`), so each draws its "choose some" state. In a row
    /// of filled groups a labelled empty one reads as an offer and costs three rows shared with
    /// its neighbours; the same surface in a column read as dead space and cost its own.
    ///
    /// **The band's sixth seat went to Info when Incarnates was retired** (2026-09-13,
    /// user-directed). Incarnates used to end the band — a band has no push-path, so nothing
    /// above it could open and slide it down the page — and its socket row now leads the
    /// Available rail instead. The two cells it freed are the two Info already sat under, so
    /// Info simply runs the whole height of the grid rather than a new surface being found for
    /// the seat. That is not a choice so much as a description: `compact` floats a surface up
    /// into any clear cell above it, so Info rises into those rows whatever height it is
    /// authored at, and authoring it short only makes it end short of its siblings.
    ///
    /// **Four columns, one row deep: Available 3, Pools 2, Powers 5, Info 2** (LAY8,
    /// 2026-09-16, user-directed — the arrangement the user had dragged this layout into, read
    /// back and authored). All four start under the band except Info, which starts at the top
    /// because nothing sits above it, so it is the band's rows taller and all four bottom-align.
    /// That is simpler than it was twice over: the band used to be level in five of its six
    /// surfaces with Incarnates taller, and Info — the column beneath it — had to be authored
    /// shorter by exactly that drop to end level, which is the whole reason a named
    /// `INCARNATES_DROP` constant existed; and the left column used to hold two surfaces
    /// stacked, which is the whole reason [`pools_seat`] exists.
    ///
    /// **The pools get a column instead of the bottom of one.** They were authored under
    /// Available in a shared 4-wide column, which made the left column the only one in the
    /// layout whose two halves had to be divided — a fraction ([`pools_rows`]) to split it, a
    /// floor under each half, a derived [`POOLS_SPLIT_ROWS`] below which the pools were pushed
    /// off the grid entirely, and a test to prove neither half was ever seeded under its own
    /// minimum. Side by side, none of that applies at this step: the pools hold a column's
    /// height at any height the columns are authored at, so the short-window case that used to
    /// send them off the grid cannot arise here. The machinery stays because the eight-column
    /// default still stacks them — at eight columns three side-by-side columns plus Info is
    /// more columns than there are — and that is the step it was really written for.
    ///
    /// **Powers gives up a cell rather than the pools taking one from the rail.** It takes five
    /// of twelve where it took six: both of its layouts deal cards into columns that need
    /// ~210px each, so width is the difference between one column of cards and three, and five
    /// cells still buys three. It is measured against a FULL build (17 cells), because it is the
    /// only surface that grows monotonically as the build is made and sizing it to a one-power
    /// build would mean opening at a height it outgrows on the second pick. Available takes
    /// three and keeps the two-column body that LAY2/LAY3 bought it — its body is an `auto-fit`
    /// grid, so the count is the width's answer rather than a number named here, and three cells
    /// of twelve is still over the ~250px where the second column appears. Its height
    /// requirement came down with the same change: 20 cells came from a single 690px stack of
    /// unpicked rows, and two columns of that stack is half as tall.
    /// [`DEFAULT_COLUMN_ROWS`] stays 20 because Powers and Info both still have content to put
    /// in it. Info takes the column's height rather than a content fit: as a full-height column
    /// its height is the column's, and what it holds is a property of a selection, so there is
    /// no one height to fit.
    ///
    /// **Every cell is spent.** 3 + 2 + 5 = 10 under the band, and Info's 2 beside it makes 12.
    /// The dragged layout this was authored from left the twelfth column empty; a default that
    /// shipped a blank column would be handing every fresh install a gap to close by hand, so
    /// the spare cell went to Powers — the surface whose width the paragraph above says is
    /// load-bearing, and the one the drag had taken it from.
    ///
    /// Set Bonuses, Status Protection, Status Resistance and Debuff Resist open [off the
    /// grid](GridItem::hidden). The three mez surfaces answer a question — what am I proof
    /// against — that a build in progress is not yet asking, and Set Bonuses answers one a
    /// fresh build cannot ask at all: its own empty state says to slot two pieces of an IO set
    /// first, so on every new build it is a twelfth of the window showing a sentence about how
    /// to make it say something (LAY1). All four are the same trade the grid was already making
    /// three times — the wrong three, until the fourth was measured against them. Hidden and not
    /// folded because the quickbar's panel pills make the way back a single visible click, which
    /// is what a folded title used to be for. They keep the rectangle they would return to, in
    /// the row under the columns, so showing one lands it at the foot of the grid rather than
    /// wherever gravity would have dragged it.
    ///
    /// **The column height is the window's answer, not this fn's.** [`DEFAULT_COLUMN_ROWS`] is
    /// only what a 1080p screen gets and the cap on what any screen gets; the app measures the
    /// room the grid actually has and calls [`default_layout_sized`](GridItem::default_layout_sized)
    /// through [`column_rows_for`]. Twenty rows of column is 712px, and with the app header,
    /// the quickbar, both grid insets and the band above them that is a ~980px page — which
    /// fits a 1080p screen with room for the window's chrome and overflows a 13" laptop by
    /// about 150px. Deriving the number instead is what stops the default from being a poster
    /// sized for one screen: the columns already carry their own scroll, so a shorter column
    /// costs rows of a list, and a page taller than the window costs the bottom of the layout.
    ///
    /// The band still fits itself at render, and the measured height only has to be an upper
    /// bound on it: a band surface that fits SHORTER than its seed compacts upward and the
    /// page ends under the window, while one that fits taller pushes past it. Every band
    /// surface is now a stat group, and a stat group's height is its visible rows, so the seed
    /// is wrong only by rows the user chose. Incarnates was the one with room to do worse —
    /// its socket row wrapped below about 1660px, a step function of the width rather than a
    /// count — and it is no longer in the band.
    ///
    /// Authored already compacted — the band runs from `y = 0` and the columns from the row
    /// under it — so a fresh layout looks identical before and after the first drag (`compact`
    /// is a no-op on it, pinned by `default_layout_is_already_compact`). The three hidden
    /// rectangles sit below the columns and take no part in that: `compact` skips a hidden
    /// surface, which is what lets them name a return cell deeper than anything on screen.
    pub fn default_layout() -> Vec<GridItem> {
        GridItem::default_layout_sized(DEFAULT_COLUMN_ROWS, &DEFAULT_SURFACES)
    }

    /// The default arrangement for a given column count — what a fresh install and a Reset
    /// actually call, with the count answered by the window through [`columns_for`].
    ///
    /// Each [`COLUMN_STEPS`] entry is authored, never reflowed into. Reflowing the twelve-column
    /// default into eight was the first thing tried and it is why this fn exists: the layout is
    /// six two-cell band surfaces over four columns, and reading-order placement at eight
    /// columns wraps the two reference columns (Info, Set Bonuses) *under* the two build ones
    /// rather than beside them. Two twenty-row columns stacked is a forty-row page — measured
    /// ~1290px against an 800px window, which is a worse version of the poster VF1 just
    /// retired. A narrower window has to mean a different arrangement, not the same one folded.
    pub fn default_layout_for(
        columns: u32,
        column_rows: u32,
        expected: &[PanelKind],
    ) -> Vec<GridItem> {
        if columns >= COLUMN_STEPS[0] {
            GridItem::default_layout_sized(column_rows, expected)
        } else {
            GridItem::default_layout_narrow(column_rows, expected)
        }
    }

    /// The eight-column default: one band row over the two build columns, everything else off
    /// the grid.
    ///
    /// Eight columns hold four two-cell surfaces, so the band is a choice of four rather than a
    /// wrapped six. It keeps the four groups the default stat set actually fills
    /// (`stat_registry::DEFAULT_VISIBLE`) — Movement and Resistance both draw "choose some" at
    /// twelve columns, and a labelled empty group reading as an offer is a trade a wide band can
    /// afford and a narrow one cannot. Resistance stays because it is the one of that pair a
    /// build fills first.
    ///
    /// Available and Powers keep the full column height and the whole second band: they are
    /// where the work happens, and the surfaces given up are the ones read *about* a build
    /// rather than used to make one. Available keeps **two** cells here where the twelve-column
    /// default gives it four (LAY2). The four buy the rail a two-column body; two cells of
    /// eight is ~250px, which is one column of rows however the CSS is written, and the only
    /// way to buy a second would be to take it off Powers — the surface this step already
    /// decided is where the work happens. The rail's body answers this itself: it is an
    /// `auto-fit` grid, so it draws one column at the narrow step and two at the wide one
    /// without either layout naming a count. Off-grid is not gone — the quickbar's More menu is a single
    /// visible click, which is the same mechanism the twelve-column default already spends on
    /// Status Protection, Status Resistance and Debuff Resist for the same reason: the band has
    /// no column left to seat them in.
    ///
    /// Off-grid rectangles are still real rectangles ([`validate_layout`] checks every item's
    /// span against the column count, hidden or not), so they name a cell inside eight columns
    /// and in the row under the columns — where showing one lands it at the foot of the grid.
    ///
    /// [`validate_layout`]: crate::grid::collide::validate_layout
    fn default_layout_narrow(column_rows: u32, expected: &[PanelKind]) -> Vec<GridItem> {
        let tall = column_rows.max(MIN_COLUMN_ROWS);
        let below = COLUMN_START_ROW + tall;
        let mut items = vec![
            GridItem::new(
                PanelKind::Available,
                0,
                COLUMN_START_ROW,
                2,
                rail_rows(tall),
            ),
            pools_seat(tall, 0, 2, below),
            GridItem::new(PanelKind::Powers, 2, COLUMN_START_ROW, 6, tall),
            GridItem::new(PanelKind::Info, 4, below, 2, tall).off_grid(),
            GridItem::new(PanelKind::SetBonuses, 6, below, 2, tall).off_grid(),
        ];
        seat_dashboards(&mut items, expected, 8, below);
        items
    }

    /// The same arrangement at a chosen column height — what [`default_layout`] delegates to,
    /// and what the app calls with a height derived from the window it is actually open in
    /// (see [`column_rows_for`]).
    ///
    /// `column_rows` is the height of Available and Powers, which start under the band. Info
    /// starts at `y = 0` instead — nothing sits above it since Incarnates was retired — so it
    /// is [`COLUMN_START_ROW`] rows taller than its siblings and the three still bottom-align.
    ///
    /// Clamped rather than trusted: a viewport short enough to ask for two-row columns would
    /// author surfaces below their own `min_size`, so [`MIN_COLUMN_ROWS`] floors it and a
    /// window shorter than that gets a page scrollbar instead of an unusable layout.
    pub fn default_layout_sized(column_rows: u32, expected: &[PanelKind]) -> Vec<GridItem> {
        let tall = column_rows.max(MIN_COLUMN_ROWS);
        // Info runs the full height of the grid — the band's rows plus a column's — because
        // it is the one column with no band surface above it since Incarnates was retired
        // (2026-09-13). Authoring it shorter would not leave the seat empty: `compact`
        // floats a surface up into any clear cell above it, so Info would rise into the
        // band's rows anyway and end short of its siblings by exactly what it gave up.
        let full = COLUMN_START_ROW + tall;
        // The row under the columns, where an off-grid surface waits.
        let below = full;
        let mut items = vec![
            GridItem::new(PanelKind::Available, 0, COLUMN_START_ROW, 3, tall),
            // A column of its own rather than the foot of Available's (LAY8), so the pools
            // take a column's full height and `pools_seat`'s split does not apply at this step.
            GridItem::new(PanelKind::Pools, 3, COLUMN_START_ROW, 2, tall),
            GridItem::new(PanelKind::Powers, 5, COLUMN_START_ROW, 5, tall),
            GridItem::new(PanelKind::Info, 10, 0, 2, full),
            GridItem::new(PanelKind::SetBonuses, 6, below, 2, tall).off_grid(),
        ];
        seat_dashboards(&mut items, expected, 10, below);
        items
    }
}

/// The pools' share of a `tall`-row left column: two fifths of it, never under
/// [`POOLS_MIN_ROWS`].
///
/// A fraction rather than a constant, measured 2026-09-15 in a 1100px window. A constant was
/// tried first on the argument that the two halves do not scale together — the rail holds two
/// lists as long as the archetype makes them, the pools at most five short ones — and the
/// argument is sound but points the other way. It is the RAIL whose content is fixed: two
/// nine-power sets are ~400px whatever the window is, so every row a taller window adds to a
/// fixed rail is a row nothing is drawn in, while the pools' content is the thing that grows.
/// At six constant rows a 1100px window left the rail ~275px of blank and clipped a four-pool
/// bag; three-to-two leaves the rail the bigger share (it still has to hold an eighteen-power
/// set) and lets the pools grow with the window they are in.
fn pools_rows(tall: u32) -> u32 {
    (tall * 2 / 5).max(POOLS_MIN_ROWS)
}

/// The rail's share of a `tall`-row left column: all of it where the column is too short to
/// split, and the rest of it where it is not.
///
/// Never under [`RAIL_MIN_ROWS`], which is what [`POOLS_SPLIT_ROWS`] is derived to guarantee:
/// under it there is no split, and at or over it the pools' share always leaves that much.
fn rail_rows(tall: u32) -> u32 {
    if tall >= POOLS_SPLIT_ROWS {
        tall - pools_rows(tall)
    } else {
        tall
    }
}

/// The pools' rectangle in a `tall`-row left column of `width` cells starting at `x`: under
/// the rail where the column can pay for both, off the grid in the row `below` the columns
/// where it cannot.
///
/// The off-grid cell starts at `x + 2` rather than at `x`, clear of the column the dashboard
/// overflow stacks in ([`seat_dashboards`]) — an off-grid rectangle is still a real one that
/// [`validate_layout`](crate::grid::collide::validate_layout) checks for overlap, hidden or
/// not, and it is the cell the surface returns to when it is shown.
fn pools_seat(tall: u32, x: u32, width: u32, below: u32) -> GridItem {
    if tall >= POOLS_SPLIT_ROWS {
        GridItem::new(
            PanelKind::Pools,
            x,
            COLUMN_START_ROW + rail_rows(tall),
            width,
            pools_rows(tall),
        )
    } else {
        GridItem::new(PanelKind::Pools, x + 2, below, width, POOLS_MIN_ROWS).off_grid()
    }
}

/// Give every dashboard in the roster a cell in the band — the strip above the build columns —
/// sharing `band_width` between them, and stack whatever will not fit below the columns.
///
/// The band has to span the full width above the columns, and that is a consequence rather than
/// a preference: `compact` floats a surface up into any clear cell above it, so a narrow
/// dashboard at `y = 0` would let Powers rise alongside it and there would be no band at all,
/// only a ragged top edge. Sharing the width is what keeps one row of readouts across the top
/// however many panels the user has built.
///
/// The width is shared as evenly as whole cells allow — `band_width / seats` each, with the
/// remainder handed out one cell at a time from the left rather than dumped on the last seat.
/// Four panels across ten cells is `3,3,2,2` and not `2,2,2,4`: the band ends flush either way,
/// so a gate on "no gaps" passes over the lopsided one, and the difference is the whole
/// difference between a row of readouts and one panel with three offcuts beside it.
///
/// Below two cells a panel is under its own `min_size`, so once the band cannot give every panel
/// that much, the overflow goes under the columns — visible, and `compact` will lift it as high
/// as the surfaces above allow.
fn seat_dashboards(items: &mut Vec<GridItem>, expected: &[PanelKind], band_width: u32, below: u32) {
    let dashboards: Vec<PanelKind> = expected
        .iter()
        .copied()
        .filter(|panel| panel.dashboard().is_some())
        .collect();
    if dashboards.is_empty() {
        return;
    }

    const MIN_SEAT: u32 = 2;
    let seats = (band_width / MIN_SEAT).min(dashboards.len() as u32);
    let share = band_width / seats;
    let remainder = band_width % seats;

    let mut x = 0;
    let mut stacked = below;
    for (index, panel) in dashboards.into_iter().enumerate() {
        if (index as u32) < seats {
            let w = share + u32::from((index as u32) < remainder);
            items.push(GridItem::new(panel, x, 0, w, BAND_SEED_ROWS));
            x += w;
        } else {
            items.push(GridItem::new(panel, 0, stacked, MIN_SEAT, BAND_SEED_ROWS));
            stacked += BAND_SEED_ROWS;
        }
    }
}

/// The grid's cell↔pixel geometry. `columns` is fixed; a cell's pixel width is
/// derived from the live container width so the layout reflows on resize.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridConfig {
    pub columns: u32,
    pub row_height: f64,
    /// Gap between items in pixels, `(x, y)`.
    pub margin: (f64, f64),
    /// Inset from the container edge in pixels, `(x, y)`.
    pub padding: (f64, f64),
}

impl GridConfig {
    /// The geometry a container of `container_width` gets: the default, with its column count
    /// answered by the window through [`columns_for`] instead of taken as a literal.
    ///
    /// This is the width axis' half of what [`column_rows_for`] does for height. The X axis
    /// always had the window as an input — a column is `container / columns`, so a surface is
    /// a fraction of whatever the window is and the grid structurally cannot clip the page.
    /// What it had no input for was a *floor*: a fraction has none, so the grid would go on
    /// thinning surfaces past the width their content stops fitting in and never reflow.
    /// Stepping the count is what turns that continuous thinning into a reflow.
    pub fn for_width(container_width: f64) -> GridConfig {
        let base = GridConfig::default();
        GridConfig {
            columns: columns_for(container_width, base.margin.0),
            ..base
        }
    }

    /// The default geometry at an already-decided column count — what a persisted layout is
    /// validated against, since a save carries the count it was arranged at rather than a
    /// width to re-derive one from.
    pub fn for_columns(columns: u32) -> GridConfig {
        GridConfig {
            columns,
            ..GridConfig::default()
        }
    }
}

impl Default for GridConfig {
    /// The widest step's geometry. `columns` is a literal here and derived in
    /// [`GridConfig::for_width`] — this is the unmeasured answer, which every pure consumer
    /// (validation, the authored default, the tests) wants and the renderer does not.
    fn default() -> GridConfig {
        GridConfig {
            columns: COLUMN_STEPS[0],
            // A row is the folded header's height plus headroom: `COLLAPSED_ROWS` is one, so a
            // folded surface has to fit inside exactly one of these (`.panel.is-collapsed` is
            // 26px including both borders), and nothing else floors the row.
            row_height: 28.0,
            // The seam between two surfaces. It is paid once per row boundary down the whole
            // grid, so it is the cheapest vertical space here to give back — at 12 the eleven
            // gaps of the default column cost more than a whole panel body.
            margin: (8.0, 8.0),
            padding: (0.0, 0.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::collide::validate_layout;
    use crate::grid::math::container_height;
    use crate::panels::dashboards::{surfaces, Dashboards};

    /// One row's pitch in pixels. Derived from the config rather than written down, because a
    /// row height that changed under a literal here would leave every bound below passing for
    /// the wrong reason.
    fn pitch(config: &GridConfig) -> f64 {
        config.row_height + config.margin.1
    }

    /// The roster a fresh install opens on, read through the same fn `GridContainer` reads it by.
    fn roster() -> Vec<PanelKind> {
        surfaces(&Dashboards::default())
    }

    /// The height re-fit exactly as `GridContainer`'s effect makes it: build the layout the room
    /// pays for, then adopt it unless [`tallest_authored_rows`] says the live one already is it.
    ///
    /// `popped.reapply` is the one line of the effect this leaves out, and it is a no-op with
    /// nothing popped out — which is the state the browser gate drives and the state a fresh
    /// window is in. A pop-out's effect on this decision is not covered here.
    fn refit(state: &mut Vec<GridItem>, room: f64, config: &GridConfig, roster: &[PanelKind]) {
        let rows = column_rows_for(room, config);
        let candidate = GridItem::default_layout_for(config.columns, rows, roster);
        if tallest_authored_rows(state) == tallest_authored_rows(&candidate) {
            return;
        }
        *state = candidate;
    }

    /// The room left below a layout: what the grid was given, minus what it used.
    fn dead_px(state: &[GridItem], room: f64, config: &GridConfig) -> f64 {
        room - container_height(state, config)
    }

    /// The regression this module was written for.
    ///
    /// `npm run audit:grid-viewport` drives a real OS window and measured, at the SAME window
    /// height, 26px of unused window on the way down and 134px on the way back up. The 108px
    /// between them is three rows, and three rows is exactly how much taller than its siblings
    /// `default_layout_sized` authors Info — so the re-fit's guard was comparing Info's height
    /// against a column height and reading "already fitted" on a layout three rows short.
    ///
    /// The room values are the grid's room, not window heights: the header and the quickbar
    /// above it are spent before this geometry is asked anything.
    #[test]
    fn regrowing_a_window_gives_the_columns_their_rows_back() {
        let config = GridConfig::for_width(1800.0);
        let roster = roster();
        let mut state = GridItem::default_layout_for(config.columns, DEFAULT_COLUMN_ROWS, &roster);

        for room in [1340.0, 1140.0, 940.0, 840.0, 940.0, 1140.0, 1340.0] {
            refit(&mut state, room, &config, &roster);
            let dead = dead_px(&state, room, &config);
            assert!(
                dead < pitch(&config),
                "{room}px of room left {dead}px unused, which is more than one row"
            );
        }
    }

    /// The property under that regression, stated without reference to any one step: where the
    /// re-fit lands is a function of the room it is given and nothing else. A layout that
    /// remembers the room it *had* is the defect, whichever direction it arrived from.
    #[test]
    fn a_refit_answers_the_room_it_has_and_not_the_room_it_had() {
        let config = GridConfig::for_width(1800.0);
        let roster = roster();
        let rooms: Vec<f64> = (600..=1500).step_by(6).map(f64::from).collect();

        for &first in &rooms {
            for &second in &rooms {
                let mut walked =
                    GridItem::default_layout_for(config.columns, DEFAULT_COLUMN_ROWS, &roster);
                refit(&mut walked, first, &config, &roster);
                refit(&mut walked, second, &config, &roster);

                let mut direct =
                    GridItem::default_layout_for(config.columns, DEFAULT_COLUMN_ROWS, &roster);
                refit(&mut direct, second, &config, &roster);

                assert_eq!(
                    walked, direct,
                    "a window at {second}px of room settles differently for having been at {first}px first"
                );
            }
        }
    }

    /// No layout is ever more than one row short of its room, at either column count. This is
    /// the bound the browser gate checks at 40px; the pure geometry can be held to the row.
    #[test]
    fn no_layout_strands_more_than_one_row_of_its_room() {
        let roster = roster();
        for width in [1800.0, 1000.0] {
            let config = GridConfig::for_width(width);
            for room in (600..=1500).step_by(2).map(f64::from) {
                let mut state =
                    GridItem::default_layout_for(config.columns, DEFAULT_COLUMN_ROWS, &roster);
                refit(&mut state, room, &config, &roster);
                let dead = dead_px(&state, room, &config);
                assert!(
                    dead < pitch(&config),
                    "{} columns, {room}px of room: {dead}px unused",
                    config.columns
                );
            }
        }
    }

    /// A second re-fit at the same room changes nothing. The guard exists to make the re-author
    /// stop, and a guard that never fires would re-author on every poll of the window.
    #[test]
    fn a_refit_settles_in_one_step() {
        let roster = roster();
        for width in [1800.0, 1000.0] {
            let config = GridConfig::for_width(width);
            for room in (600..=1500).step_by(2).map(f64::from) {
                let mut state =
                    GridItem::default_layout_for(config.columns, DEFAULT_COLUMN_ROWS, &roster);
                refit(&mut state, room, &config, &roster);
                let settled = state.clone();
                refit(&mut state, room, &config, &roster);
                assert_eq!(state, settled, "{room}px of room re-authored a second time");
            }
        }
    }

    /// Every height the window can ask for authors a layout the collision engine accepts: one
    /// entry per roster surface, every rectangle inside the column count, no two visible ones
    /// overlapping. The authored defaults are the only layouts in the app that no gesture has
    /// been through, so nothing else would catch a bad one.
    #[test]
    fn every_authored_default_is_a_valid_layout() {
        let roster = roster();
        for columns in COLUMN_STEPS {
            let config = GridConfig::for_columns(columns);
            for rows in 1..=60 {
                let items = GridItem::default_layout_for(columns, rows, &roster);
                assert!(
                    validate_layout(&items, &config, &roster),
                    "{columns} columns at {rows} rows authored an invalid layout: {items:#?}"
                );
            }
        }
    }

    /// The guard [`POOLS_SPLIT_ROWS`]'s own doc comment names, written 2026-09-26: the narrow
    /// default divides its left column only where both halves clear their own `min_size` floor,
    /// and the derivation of that constant from the two floors is what makes it true. It was
    /// cited by name in this file for a test that had never been written.
    #[test]
    fn a_split_column_seeds_neither_half_under_its_floor() {
        for tall in 1..=60u32 {
            let rail = rail_rows(tall);
            assert!(
                rail >= RAIL_MIN_ROWS || tall < RAIL_MIN_ROWS,
                "a {tall}-row column gave the rail {rail} rows, under its {RAIL_MIN_ROWS}-row floor"
            );
            if tall >= POOLS_SPLIT_ROWS {
                let pools = pools_rows(tall);
                assert!(
                    pools >= POOLS_MIN_ROWS,
                    "a split {tall}-row column gave the pools {pools} rows, under its {POOLS_MIN_ROWS}-row floor"
                );
                assert_eq!(
                    rail + pools,
                    tall,
                    "a split {tall}-row column's two halves do not add up to it"
                );
            }
        }
    }

    /// What the re-fit's key counts, and what it must not. A fitted surface's height is measured
    /// a frame after the layout that seeded it, so counting one would make the fit see its own
    /// result as a change; a hidden surface is not drawn and sets no extent.
    #[test]
    fn the_fit_key_reads_past_the_band_and_the_hidden() {
        let tall = GridItem::new(PanelKind::Powers, 0, 0, 2, 12);
        let taller_but_fitted = GridItem::new(PanelKind::Dashboard(DashboardId(1)), 2, 0, 2, 30);
        let taller_but_hidden = GridItem::new(PanelKind::Info, 4, 0, 2, 40).off_grid();

        assert_eq!(
            tallest_authored_rows(&[tall, taller_but_fitted, taller_but_hidden]),
            Some(12)
        );
        assert_eq!(tallest_authored_rows(&[]), None);
        assert_eq!(tallest_authored_rows(&[taller_but_hidden]), None);
    }

    /// The gate [`GridItem::default_layout`]'s own doc has cited since it was written, and which
    /// did not exist until 2026-09-27: the authored default is already compacted, so a fresh
    /// layout looks identical before and after the first drag.
    ///
    /// It matters because `compact` runs on every commit. If the default were NOT compact, the
    /// first gesture a user made anywhere on the grid would silently rearrange the whole thing
    /// as a side effect — the layout they had been looking at would not be the layout they got.
    ///
    /// Checked at every column count and over a sweep of heights, because the default is
    /// authored per count and per height and each one is a separate arrangement.
    #[test]
    fn default_layout_is_already_compact() {
        let roster = roster();
        for columns in COLUMN_STEPS {
            for rows in [MIN_COLUMN_ROWS, 12, DEFAULT_COLUMN_ROWS, 40] {
                let authored = GridItem::default_layout_for(columns, rows, &roster);
                let mut compacted = authored.clone();
                crate::grid::collide::compact(&mut compacted);
                // `compact` sorts, so compare by surface rather than by position in the list.
                for item in &authored {
                    let after = compacted
                        .iter()
                        .find(|c| c.panel == item.panel)
                        .expect("compact neither adds nor drops a surface");
                    assert_eq!(
                        (after.x, after.y, after.w, after.h),
                        (item.x, item.y, item.w, item.h),
                        "{columns} columns at {rows} rows: compact moved {:?}",
                        item.panel
                    );
                }
            }
        }
        // And the same for the no-argument default, which is the one a fresh install gets.
        let authored = GridItem::default_layout();
        let mut compacted = authored.clone();
        crate::grid::collide::compact(&mut compacted);
        for item in &authored {
            let after = compacted.iter().find(|c| c.panel == item.panel).unwrap();
            assert_eq!(
                (after.x, after.y, after.w, after.h),
                (item.x, item.y, item.w, item.h),
                "compact moved {:?} in the fresh-install default",
                item.panel
            );
        }
    }

    /// The floor under a column height, and the absence of a ceiling over it. VF1 clamped every
    /// measured window to [`DEFAULT_COLUMN_ROWS`] and stranded 500px under a 1440p layout; the
    /// clamp is gone and this is what says so.
    #[test]
    fn a_column_height_has_a_floor_and_no_ceiling() {
        let config = GridConfig::default();
        assert_eq!(column_rows_for(0.0, &config), MIN_COLUMN_ROWS);
        assert_eq!(column_rows_for(1.0, &config), MIN_COLUMN_ROWS);
        assert!(column_rows_for(4000.0, &config) > DEFAULT_COLUMN_ROWS);

        let mut last = 0;
        for room in (0..=4000).step_by(4).map(f64::from) {
            let rows = column_rows_for(room, &config);
            assert!(
                rows >= last,
                "{room}px of room answered fewer rows than less room did"
            );
            last = rows;
        }
    }
}
