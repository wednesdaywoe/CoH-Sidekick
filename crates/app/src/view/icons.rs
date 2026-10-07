//! Icon-path resolution for power and enhancement images. Mirrors the beta
//! `getPowerIconPath` / `EnhancementIcon` resolvers (`CoH-Sidekick/src/utils/power-icons.ts`,
//! `CoH-Sidekick/src/components/enhancements/EnhancementIcon.tsx` — beta-only since DEC8)
//! against the vendored `assets/img` tree (identical to the beta's `public/img`). Icons are
//! *derived* from the export at render — the power's `icon` rides in `Power.extra`, the set's
//! icon in the `IoSet` def — never a stored copy (the Dioxus derived-value rule).
//!
//! A missing/empty icon resolves to `Unknown.png` — a visible fallback, never a broken image
//! (Rule 1).

use dioxus::prelude::*;

/// The whole beta image tree, bundled once as a folder asset. Sub-paths (`powers/…`,
/// `Enhancements/…`) resolve against its runtime URL; the directory structure is preserved.
static IMG_DIR: Asset = asset!("/assets/img");

/// Bare filename used when a power/enhancement carries no icon, or its file is absent.
const UNKNOWN: &str = "Unknown.png";

/// The power-card icon URL. All power icons live in one flat `powers/` folder, lower-cased —
/// the beta `getPowerIconPath`. `None`/empty ⇒ the `Unknown.png` fallback.
pub fn power_icon_url(icon: Option<&str>) -> String {
    match icon.map(str::trim).filter(|s| !s.is_empty()) {
        Some(name) => format!("{IMG_DIR}/powers/{}", vendored_power_icon(name)),
        None => format!("{IMG_DIR}/{UNKNOWN}"),
    }
}

/// The client's extensions for power art, longest first so `.texture.png` unwinds in one pass.
const CLIENT_TEXTURE_EXTENSIONS: [&str; 3] = [".texture", ".dds", ".tga"];

/// An export icon name as the vendored tree spells it: lower-case, and always `.png`.
///
/// Lower-casing alone was the whole rule until the corpus test went in, and it left two powers
/// permanently blank — Rebirth's War Cry (`martialmastery_warcry.dds`) and Thunderspy's Upgrade
/// Equipment (`knights_upgradeequipment.texture.png`). Both names come out of
/// `normalizeIconPath` in `scripts/convert-powerset.cjs`, whose job is to give a bare bin-export
/// name the extension the planner expects and which reads `.dds` as "already has one" and
/// `.texture` as "has none, append `.png`". Neither result is a file anybody can ship: `.dds` and
/// `.texture` are what the art is called *inside the client*, and everything under `powers/` is a
/// PNG we decoded out of it.
///
/// So the rule belongs here as well as there. What a vendored file is named is a fact about our
/// asset tree, not about the export — the same reason the lower-casing lives here — and stating
/// it at the resolver makes it true for every fork at once, including the two whose converter fix
/// is stuck behind an unrelated red gate (Thunderspy's twin-divergence baseline). It is
/// idempotent, so it stays harmless the day `normalizeIconPath` is fixed. It is deliberately a
/// closed list rather than "strip any extension": a power icon with a dot in its stem must keep
/// it, and an unknown extension showing up here is a new fact worth failing on rather than
/// guessing at — `every_shipped_power_icon_is_vendored` is where it surfaces.
fn vendored_power_icon(icon: &str) -> String {
    let mut stem = icon.to_lowercase();
    loop {
        let trimmed = CLIENT_TEXTURE_EXTENSIONS
            .iter()
            .chain(std::iter::once(&".png"))
            .find_map(|ext| stem.strip_suffix(ext));
        match trimmed {
            Some(shorter) => stem = shorter.to_string(),
            None => break,
        }
    }
    format!("{stem}.png")
}

/// The base-icon URL for an IO-set piece — the set's own icon, in the `Enhancements/{folder}`
/// subtree the beta `getIOSetFolder` routes to. `None` when the set carries no icon (the caller
/// keeps the type chip rather than showing a wrong image).
pub fn io_set_icon_url(set_icon: &str) -> Option<String> {
    let icon = set_icon.trim();
    (!icon.is_empty()).then(|| format!("{IMG_DIR}/Enhancements/{}/{icon}", io_set_folder(icon)))
}

/// The base-icon URL for a generic IO or an origin enhancement — both keyed on the enhancement's
/// `stat` against the shared `Enhancements/Generic` icon set (beta `getGenericIOIconPath` /
/// `getOriginIconPath`; origins are distinguished by overlay frame, not base icon). `None` for a
/// stat with no mapped icon, so the caller falls back to the type chip rather than a wrong image.
pub fn stat_icon_url(stat: &str) -> Option<String> {
    stat_icon_file(stat).map(|file| format!("{IMG_DIR}/Enhancements/Generic/{file}"))
}

/// The base-icon URL for a special enhancement (Hamidon/Synthetic Hamidon/Titan/Hydra/D-Sync/
/// Prestige) — the
/// beta `createSpecialEnhancement` derivation. The export carries no icon field for specials,
/// so the filename derives from the family's prefix + the capitalized def id; every D-Sync
/// shares one icon, and Prestige icons live in their own folder. `category` is the picker's
/// family tag (`hamidon`, `d-sync`, …); `None` for an unrecognized tag, so the caller keeps
/// the type chip rather than showing a wrong image.
pub fn special_icon_url(category: &str, def_id: &str) -> Option<String> {
    if category == "d-sync" {
        return Some(format!("{IMG_DIR}/Enhancements/Special/DSO_all.png"));
    }
    let prefix = match category {
        // Synthetic pieces share the Hamidon art — the game distinguishes them by frame,
        // and no `SHO*.png` exists in the client's enhancement set.
        "hamidon" | "synthetic-hamidon" => "HO",
        "titan" => "TN",
        "hydra" => "HY",
        "prestige" => "Prestige_",
        _ => return None,
    };
    let folder = if category == "prestige" {
        "Prestige"
    } else {
        "Special"
    };
    // A Synthetic def id is its Hamidon counterpart's id under a `synthetic` prefix, and the
    // art is the counterpart's.
    let art_id = def_id.strip_prefix("synthetic").unwrap_or(def_id);
    Some(format!(
        "{IMG_DIR}/Enhancements/{folder}/{prefix}{}.png",
        special_icon_stem(art_id)
    ))
}

/// The icon URL for an incarnate power. The export carries no icon field for incarnates, so
/// the filename is synthesized the way the beta's `getIconFromTier` does — one art per
/// (slot, tree, tier), shared by both branches of a tree at that tier:
/// `powers/incarnate_<slot>_<tree>_<tier>.png`.
///
/// `tree_stem` is derived from the power's *internal* name, not its display name, because the
/// asset filenames follow the internal spelling (see [`incarnate_tree_stem`]).
pub fn incarnate_icon_url(slot_id: &str, internal_name: &str, tier_token: &str) -> String {
    let stem = incarnate_tree_stem(internal_name);
    format!("{IMG_DIR}/powers/incarnate_{slot_id}_{stem}_{tier_token}.png")
}

/// The tree token in an incarnate icon filename: the first `_`-separated word of the power's
/// internal name, lower-cased, with the handful of overrides where the vendored art is spelled
/// differently from that word.
///
/// The overrides are asset-filename facts, not game rules — the same carve-out
/// [`special_icon_stem`] documents. Each is a lore pet whose internal name leads with a
/// qualifier while its art is named for the pet: `Polar_Lights` → `lights`, `Robotic_Drones`
/// → `drones`, `Storm_Elemental` → `elementals`, `Phantom` → `phantoms`. Every other tree in
/// every dataset derives straight through, and
/// `every_shipped_incarnate_power_resolves_to_a_vendored_icon` is what proves it.
fn incarnate_tree_stem(internal_name: &str) -> String {
    let first = internal_name
        .split('_')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    match first.as_str() {
        "polar" => "lights".to_string(),
        "robotic" => "drones".to_string(),
        "storm" => "elementals".to_string(),
        "phantom" => "phantoms".to_string(),
        _ => first,
    }
}

/// The overlay frame a special enhancement composites over its base icon (beta
/// `getOverlayPath`): the Hamidon-Origin frame, except Prestige which shares the
/// Training-Origin frame.
pub fn special_overlay_url(category: &str) -> String {
    let frame = if category == "prestige" {
        "e_frame_TO.png"
    } else {
        "e_frame_HO.png"
    };
    format!("{IMG_DIR}/Enhancements/Overlay/{frame}")
}

/// The overlay frame an IO-set piece composites over its set icon (beta `getOverlayPath`).
///
/// The frame is what carries rarity — every piece of a set shares one base icon, so without it a
/// purple reads exactly like a common. Two facts decide it: the set's tier, and whether the piece
/// is attuned (attuned frames are the same tier in a different metal).
///
/// Superior archetype and event sets are told from their plain twins by the icon filename's
/// prefix, as in the beta. The export states the same fact a second way, in the `rarity` token
/// (`ECSATO` beside `ECATO`, `ECSWinter` beside `ECWinter`), and that is the more faithful basis
/// — but the two agree across the committed catalogues, and a naive read of the token is a trap
/// the prefix does not have: `ECSpeedRun` and `ECSummer` both begin `ECS` and neither is superior.
/// Matching the beta keeps one piece from wearing two different borders in the two planners.
///
/// An unrecognized tier falls back to the plain IO frame rather than to nothing, as in the beta:
/// a piece with a wrong-metal border still reads as an enhancement, where a missing image reads
/// as a broken one.
pub fn io_set_overlay_url(category: &str, icon: &str, attuned: bool) -> String {
    // Archetype and event sets ship attuned-only, so their frames carry no unattuned twin.
    let frame = if icon.starts_with("SAO_") || icon.starts_with("SEO_") {
        "e_frame_attuned_superior.png"
    } else if icon.starts_with("AO_") || icon.starts_with("EO_") {
        "e_frame_attuned_rare.png"
    } else {
        match (category, attuned) {
            ("uncommon", true) => "e_frame_attuned_uncommon.png",
            ("uncommon", false) => "e_frame_uncommon.png",
            ("rare", true) => "e_frame_attuned_rare.png",
            ("rare", false) => "e_frame_rare.png",
            ("purple", true) => "e_frame_attuned_superior.png",
            ("purple", false) => "e_frame_superior.png",
            ("pvp", true) => "e_frame_attuned_pvp.png",
            ("pvp", false) => "e_frame_pvp.png",
            ("ato" | "event", _) => "e_frame_attuned_rare.png",
            _ => "e_frame_IO.png",
        }
    };
    format!("{IMG_DIR}/Enhancements/Overlay/{frame}")
}

/// The plain (common) IO frame — a generic IO's frame, and the fallback for an io-set piece
/// whose set the loaded dataset doesn't carry (which also has no base icon, so it is never
/// shown). The beta `getOverlayPath` returns this for `io-generic` and for an unknown rarity.
pub fn plain_io_frame_url() -> String {
    format!("{IMG_DIR}/Enhancements/Overlay/e_frame_IO.png")
}

/// The overlay frame for an origin enhancement (beta `getOverlayPath`): TO has one shared
/// frame; a DO/SO frame carries the character origin's flavor, defaulting to Natural when
/// the build doesn't track an origin (the beta default). An unrecognized tier falls back to
/// the generic-IO frame, as in the beta.
pub fn origin_overlay_url(tier: &str, character_origin: Option<&str>) -> String {
    let frame = match tier {
        "TO" => "e_frame_TO.png".to_string(),
        "DO" | "SO" => format!(
            "e_frame_{}{tier}.png",
            origin_frame_prefix(character_origin)
        ),
        _ => "e_frame_IO.png".to_string(),
    };
    format!("{IMG_DIR}/Enhancements/Overlay/{frame}")
}

/// The frame-filename prefix for a character origin (beta `ORIGIN_TO_PREFIX`); Natural for
/// `None` or an unrecognized origin, the beta default. Case-insensitive: this crate's own
/// vocabulary is lowercase (see the picker in `panels::identity`), but the beta's own `Origin`
/// type is capitalized (`build.ts`'s `settings.origin: 'Natural'`) and its `ORIGIN_TO_PREFIX`
/// map is keyed lowercase with no normalization before the lookup — reading that literally
/// would make every DO/SO frame silently read Natural regardless of the chosen origin. Not
/// worth reproducing: a case fold costs nothing and the miss it prevents is quiet.
fn origin_frame_prefix(origin: Option<&str>) -> &'static str {
    match origin.map(str::to_ascii_lowercase).as_deref() {
        Some("magic") => "Mag",
        Some("mutation") => "Mut",
        Some("science") => "Sci",
        Some("technology") => "Tech",
        _ => "Nat",
    }
}

/// The capitalized filename stem for a special def id — first letter upper-cased, with the
/// beta's overrides for compound-word ids whose simple capitalize doesn't match the asset
/// filename. `tanzanite` is our addition, not the beta's: the vendored asset is spelled
/// `TNTanzenite.png`, so the beta derives a filename that doesn't exist and shows its
/// Unknown fallback for that Titan.
fn special_icon_stem(def_id: &str) -> String {
    let overridden = match def_id {
        "antiproton" => "AntiProton",
        "clockwork_efficiency" => "ClockworkEfficiency",
        "might_of_the_empire" => "MarkoftheEmpire",
        "resistance_tactics" => "ResistanceTactics",
        "syndicate_techniques" => "SyndicateTechniques",
        "will_of_the_seers" => "WilloftheSeers",
        "tanzanite" => "Tanzenite",
        _ => "",
    };
    if !overridden.is_empty() {
        return overridden.to_string();
    }
    let mut chars = def_id.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// The `Enhancements/` subfolder an IO-set icon lives in, keyed on its filename prefix — the beta
/// `getIOSetFolder`. Archetype/event/universal sets are foldered apart; everything else is a
/// standard set.
fn io_set_folder(icon: &str) -> &'static str {
    if icon.starts_with("AO_") || icon.starts_with("SAO_") {
        "Archetype"
    } else if icon.starts_with("EO_") || icon.starts_with("SEO_") {
        "Event"
    } else if icon.starts_with("UD_") {
        "Universal"
    } else {
        "IO Sets"
    }
}

/// The Generic-folder icon filename for an enhancement stat — the beta `STAT_ICON_MAP`. `None`
/// for an unmapped stat (the beta shows `Unknown.png`; we keep the type chip instead). The stat
/// strings are export attribute names, never power proper nouns (Rule 0).
fn stat_icon_file(stat: &str) -> Option<&'static str> {
    let file = match stat {
        "Accuracy" => "Acc.png",
        "Damage" => "Damage.png",
        "Recharge" => "Recharge.png",
        "EnduranceReduction" => "EndRdx.png",
        "Range" => "Range.png",
        "Defense" => "Defbuff.png",
        "Resistance" => "DamRes.png",
        "Healing" | "Absorb" => "Heal.png",
        "ToHit" => "ToHitBuff.png",
        "Hold" => "Hold.png",
        "Stun" => "Disorient.png",
        "Immobilize" => "Immob.png",
        "Sleep" => "Sleep.png",
        "Confuse" => "Confuse.png",
        "Fear" => "Fear.png",
        "Knockback" => "Knockback.png",
        "Run Speed" => "Run.png",
        "Jump" => "Jump.png",
        "Fly" => "Fly.png",
        "ToHit Debuff" => "ToHitDebuff.png",
        "Defense Debuff" => "DefDebuff.png",
        "EnduranceModification" => "EndMod.png",
        "Interrupt" => "Interrupt.png",
        "Slow" => "Slow.png",
        "Intangible" => "Intan.png",
        "Taunt" => "Taunt.png",
        _ => return None,
    };
    Some(file)
}
