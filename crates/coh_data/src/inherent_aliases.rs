//! Retired inherent internal names, mapped to the name the game actually uses.
//!
//! A build stores each inherent by `internal_name`, so renaming one breaks every
//! build already saved under the old name. These eight were renamed when the
//! universal inherents stopped being hand-authored and started coming out of the
//! export (INHERENT-4 / INHERENT-5): the old names were invented by the hand
//! table and match nothing in any fork's data. `PowerSurge` was worse than
//! invented — Electric Armor has a real power by that name, so one address named
//! two different powers.
//!
//! The mapping is by DISPLAY name, which is the one thing that did not change:
//! `Inherent.Prestige.PowerSlide` and `Prestige.Prestige_Sprints.prestige_DVD_Glidep`
//! both show "Prestige Power Slide". Note how little the invented names told you
//! — the five prestige sprints map across in a scrambled order, which is exactly
//! the kind of thing a hand table gets wrong and nobody notices.
//!
//! One-way, and read at exactly two doors: the `.skif` reader resolving a stored
//! selection, and the grant reconcile carrying a build's slotting forward.
//! Nothing writes an old name. The twin of `src/data/inherent-aliases.ts`.

/// Old name → current name, or the name unchanged when it was never retired.
///
/// A retired name that the active fork does not grant still translates here; the
/// caller then finds nothing under the new name and drops the power, which is the
/// right answer. Thunderspy grants none of these eight.
pub fn current_inherent_name(stored: &str) -> &str {
    match stored {
        // The free travel toggles. The game files all three under a `Prestige_` prefix.
        "Ninja_Run" => "Prestige_Ninja_Run",
        "Beast_Run" => "Prestige_Beast_Run",
        "Athletic_Run" => "Prestige_Athletic_Run",
        // The prestige sprints, by display name:
        "PowerSlide" => "prestige_DVD_Glidep", // Prestige Power Slide
        "PowerRush" => "prestige_Gamestop_Sprintp", // Prestige Power Rush
        "PowerSurge" => "prestige_generic_Sprintp", // Prestige Power Surge
        "PowerDash" => "prestige_BestBuy_Sprintp", // Prestige Power Dash
        "PowerQuick" => "prestige_EB_Sprintp", // Prestige Power Quick
        other => other,
    }
}
