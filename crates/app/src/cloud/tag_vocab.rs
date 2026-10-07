//! The curated tag vocabulary: what the save dialog offers, and what the browser filters on.
//!
//! Tags were free text, and free text does not cluster. Six people describe the same build as
//! `farm`, `farming`, `AFK`, `afk farm`, `fire farm` and `Farming`, and a tag filter over that
//! returns a sixth of the builds it should. The picker exists so the common cases land on one
//! spelling; the free-text field stays beside it, because a fixed vocabulary that cannot be
//! escaped just moves the problem into the description.
//!
//! # These names are labels, not branches
//!
//! Half this list is CoH proper nouns — Hasten, Domination, Fold Space. Rule 0 bans those inside
//! an `if` or a `match`, and nothing here is one: this is a string table the user picks from,
//! rendered verbatim, matched against nothing but other tag strings. The line it must not cross
//! is *deriving* a tag from a build. "Perma Hasten" ticked automatically would mean naming
//! Hasten in logic, and there is no effect in the export that says "this power is Hasten." So a
//! benchmark tag is a claim the author makes and the app repeats without checking it.
//!
//! # Why the spellings are the export's
//!
//! `Super Speed`, not `Superspeed`; `Accelerate Metabolism`, not `Accel Metab`. Checked against
//! `exported_powers/` so a tag reads the way the power reads in the planner beside it. That is
//! the export as the source of truth for a *label*, which is the one thing Rule 0 has no quarrel
//! with.

/// One section of the picker. Groups are shown in declaration order, and the tags within them
/// in declaration order too — alphabetising would split `Perma …` from `Capped …` and scatter
/// the benchmark family across the list.
pub struct TagGroup {
    pub name: &'static str,
    pub tags: &'static [&'static str],
}

/// The ten the server keeps (`share-build/index.ts:237` slices to ten). Named here rather than
/// left as a literal at each site so the picker's counter, the parser's cut and the server's
/// slice cannot drift apart.
pub const MAX_TAGS: usize = 10;

/// The vocabulary, grouped by the axis each tag is actually about. The axes are not
/// interchangeable — "Complete" says how finished the build is, "Tanking" what it does, "Perma
/// Hasten" what it achieves — and a flat list of thirty makes that impossible to see.
pub const GROUPS: &[TagGroup] = &[
    TagGroup {
        name: "Status",
        tags: &["Complete", "Work in progress"],
    },
    TagGroup {
        name: "Role",
        tags: &[
            "Tanking",
            "DPS",
            "Support",
            "Control",
            "Survival",
            "Glass cannon",
            "Generalist",
        ],
    },
    TagGroup {
        name: "Content",
        tags: &[
            "PvE",
            "PvP",
            "AFK farming",
            "Active farming",
            "S/L Farming",
            "Fire Farming",
            "Death from Below",
            "Labyrinth",
            "4-star",
            "Onslaught",
        ],
    },
    TagGroup {
        // Claims about what the build reaches. The app does not verify them.
        name: "Benchmark",
        tags: &[
            "Perma Hasten",
            "Perma Domination",
            "Perma Accelerate Metabolism",
            "Perma Phantom Army",
            "Perma Link Minds",
            "Perma Fade",
            "Capped Fly",
            "Capped Super Speed",
            "Capped Super Jump",
        ],
    },
    TagGroup {
        name: "Cost",
        tags: &["Budget", "Expensive"],
    },
    TagGroup {
        name: "Flavor",
        tags: &["Themed", "Meme", "Petless", "Low level", "Fold Space"],
    },
];

/// Every curated tag, in picker order.
pub fn all() -> impl Iterator<Item = &'static str> {
    GROUPS.iter().flat_map(|group| group.tags.iter().copied())
}

/// Fold a typed tag onto its curated spelling, if it is one.
///
/// `perma hasten` and `PERMA HASTEN` are the same tag as `Perma Hasten` and have to filter as it,
/// or the picker's whole purpose — one spelling per idea — is defeated the moment somebody types
/// instead of clicking. Case-insensitive only: this deliberately does not try to guess that
/// `afk farm` means `AFK farming`, because a fuzzy match that is wrong silently retags someone
/// else's build.
pub fn canonical(raw: &str) -> Option<&'static str> {
    let raw = raw.trim();
    all().find(|tag| tag.eq_ignore_ascii_case(raw))
}
