//! Build image export — the build as a PNG poster.
//!
//! Draws the build to a pixel buffer using the `image` crate — no DOM, no rasterization step.
//! The card is fixed-width (CARD_WIDTH = 960 CSS px) and the output resolution is this width
//! times a scale factor. The card draws powers as tiles, stats as bars, and set bonuses as text.
//!
//! The modal is a Dioxus component with controls on the left and a preview on the right.
//! Download and Copy rasterize the card via [`render_to_png`].

use crate::modal::{Modal, ModalSize};
use crate::naming;
use crate::panels::dashboards::Dashboards;
use crate::panels::stat_registry::{self, StatSection};
use crate::power_art::PowerArt;
use crate::shell::Db;
use coh_data::{CharacterState, SelectedPower};
use coh_math::CalculatedTotals;
use dioxus::prelude::*;
use image::{Rgba, RgbaImage};

// ============================================================
// Options model.
// ============================================================

/// Preset detail levels — Compact / Standard / Full.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ExportPreset {
    Compact,
    Standard,
    Full,
}

/// Which stat sections to include. Mirrors the React `ALL_STAT_SECTIONS`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StatSectionId {
    Offense,
    SurvivalAndMobility,
    StealthAndPerception,
    Defense,
    DamageResistance,
    StatusProtection,
    StatusEffectResistance,
    DebuffResistance,
}

impl StatSectionId {
    pub fn all() -> &'static [StatSectionId] {
        &[
            StatSectionId::Offense,
            StatSectionId::SurvivalAndMobility,
            StatSectionId::StealthAndPerception,
            StatSectionId::Defense,
            StatSectionId::DamageResistance,
            StatSectionId::StatusProtection,
            StatSectionId::StatusEffectResistance,
            StatSectionId::DebuffResistance,
        ]
    }

    pub fn compact() -> &'static [StatSectionId] {
        &[
            StatSectionId::Offense,
            StatSectionId::SurvivalAndMobility,
            StatSectionId::Defense,
            StatSectionId::DamageResistance,
        ]
    }
}

/// User-facing options for the export.
#[derive(Clone, Debug)]
pub struct ExportImageOptions {
    pub preset: ExportPreset,
    pub author_name: String,
    pub show_level: bool,
    pub show_credit: bool,
    pub show_enhancements: bool,
    pub show_inherents: bool,
    pub show_incarnates: bool,
    pub only_slotted: bool,
    pub stat_sections: Vec<StatSectionId>,
    pub show_set_bonuses: bool,
    pub transparent: bool,
    pub scale: u32,
}

impl Default for ExportImageOptions {
    fn default() -> Self {
        Self {
            preset: ExportPreset::Standard,
            author_name: String::new(),
            show_level: true,
            show_credit: true,
            show_enhancements: true,
            show_inherents: true,
            show_incarnates: true,
            only_slotted: false,
            stat_sections: Vec::from(StatSectionId::all()),
            show_set_bonuses: false,
            transparent: false,
            scale: 2,
        }
    }
}

impl ExportImageOptions {
    /// Apply a preset's defaults, preserving identity + appearance choices.
    pub fn apply_preset(&mut self, preset: ExportPreset) {
        self.preset = preset;
        match preset {
            ExportPreset::Compact => {
                self.stat_sections = Vec::from(StatSectionId::compact());
                self.only_slotted = true;
                self.show_set_bonuses = false;
            }
            ExportPreset::Standard => {
                self.stat_sections = Vec::from(StatSectionId::all());
                self.only_slotted = true;
                self.show_set_bonuses = true;
            }
            ExportPreset::Full => {
                self.stat_sections = Vec::from(StatSectionId::all());
                self.only_slotted = false;
                self.show_set_bonuses = true;
            }
        }
    }
}

// ============================================================
// Constants.
// ============================================================

/// Fixed render width in CSS px. Output resolution = this × the scale option.
pub const CARD_WIDTH: usize = 960;
/// Minimum height for a roughly 16:9 landscape.
pub const CARD_MIN_HEIGHT: usize = (CARD_WIDTH * 9) / 16;

/// The social-preview card's fixed size, in pixels. `backfill-preview` shape-checks every
/// upload against exactly this box (the beta's edge fn reads the PNG header and rejects
/// anything else), so this constant IS the client half of the contract — not a styling
/// choice. Matches the beta's `PREVIEW_CARD_WIDTH` / `PREVIEW_CARD_HEIGHT`.
pub const PREVIEW_CARD_WIDTH: usize = 1200;
pub const PREVIEW_CARD_HEIGHT: usize = 880;

/// Working canvas height before cropping. A full 24-power build with every stat section on
/// measures well under this; anything taller is clipped rather than silently mis-laid-out.
const CANVAS_MAX_HEIGHT: usize = 6000;
/// Card padding.
const PAD: usize = 24;
/// Power tile dimensions.
const TILE_H: usize = 52;
const TILE_GAP: usize = 6;
/// Stat section card dimensions.
const STAT_CARD_W: usize = 360;
/// Title band above a stat card's first row, and the pitch of the rows under it.
/// Gap left around text inside a stat card.
const CARD_PAD: usize = 6;
const STAT_CARD_GAP: usize = 8;
/// Zone label size.
/// The bitmap font in `export_font` is 8x8; text scales in whole multiples of it.
const GLYPH_W: usize = 8;
const GLYPH_H: usize = 8;

const ZONE_LABEL_SIZE: usize = 14;
/// Font sizes.
const TITLE_SIZE: usize = 36;
const SUBTITLE_SIZE: usize = 22;
const TILE_NAME_SIZE: usize = 12;
const TILE_LEVEL_SIZE: usize = 8;
const STAT_LABEL_SIZE: usize = 12;
const STAT_VALUE_SIZE: usize = 12;
const SMALL_TEXT_SIZE: usize = 10;
const FOOTER_SIZE: usize = 16;

// ============================================================
// Colors.
// ============================================================

// The unused members below are marked rather than deleted: these are one Tailwind step each
// (emerald-400, purple-400, red-500) and a contiguous slate scale, so the set is the unit. A
// renderer that gains a band picks its colour from here; a scale with holes in it sends the
// next person to pick a hex by eye.
const BG_COLOR: Rgba<u8> = Rgba([20, 20, 30, 255]);
#[allow(dead_code)]
const CARD_BG: Rgba<u8> = Rgba([15, 15, 25, 255]);
const BORDER_COLOR: Rgba<u8> = Rgba([60, 60, 80, 200]);
const TEXT_COLOR: Rgba<u8> = Rgba([230, 230, 240, 255]);
const SUBTITLE_COLOR: Rgba<u8> = Rgba([180, 200, 220, 255]);
const ZONE_LABEL_COLOR: Rgba<u8> = Rgba([160, 170, 190, 255]);
const TILE_BG: Rgba<u8> = Rgba([40, 40, 55, 200]);
const TILE_BORDER: Rgba<u8> = Rgba([70, 70, 90, 180]);
const STAT_CARD_BG: Rgba<u8> = Rgba([35, 35, 50, 220]);
#[allow(dead_code)]
const EMERALD: Rgba<u8> = Rgba([52, 211, 153, 255]);
const CYAN: Rgba<u8> = Rgba([103, 232, 249, 255]);
const AMBER: Rgba<u8> = Rgba([251, 191, 36, 255]);
#[allow(dead_code)]
const PURPLE: Rgba<u8> = Rgba([192, 132, 252, 255]);
#[allow(dead_code)]
const RED: Rgba<u8> = Rgba([239, 68, 68, 255]);
const SLATE_500: Rgba<u8> = Rgba([100, 116, 139, 255]);
const SLATE_400: Rgba<u8> = Rgba([148, 163, 184, 255]);
#[allow(dead_code)]
const SLATE_600: Rgba<u8> = Rgba([71, 85, 105, 255]);
#[allow(dead_code)]
const SLATE_700: Rgba<u8> = Rgba([51, 65, 85, 255]);
#[allow(dead_code)]
const SLATE_800: Rgba<u8> = Rgba([30, 41, 59, 255]);

// ============================================================
// Simple text renderer.
// ============================================================

/// A minimal monospace text drawer. Uses a fixed bitmap font embedded in the
/// module — each character is 8×12 px. Good enough for build posters; not a
/// font library.
struct TextDrawer {
    x: usize,
    y: usize,
    color: Rgba<u8>,
    size: usize,
}

impl TextDrawer {
    fn new(x: usize, y: usize, color: Rgba<u8>, size: usize) -> Self {
        Self { x, y, color, size }
    }

    /// The image is passed per call rather than held. Holding it meant only one
    /// drawer could exist at a time, and the header alone needs four.
    fn write(&mut self, img: &mut RgbaImage, text: &str) {
        let scale = (self.size / GLYPH_H).max(1);
        let advance = glyph_advance(self.size);
        for ch in text.chars() {
            draw_char(img, self.x, self.y, ch, self.color, scale);
            self.x += advance;
        }
    }

    fn advance_y(&mut self, dy: usize) {
        self.y += dy;
    }

    fn cursor(&self) -> (usize, usize) {
        (self.x, self.y)
    }
}

/// Blit one glyph from the 8x8 table, each source pixel drawn as a `scale`-sized
/// square. Anything outside Basic Latin has no glyph and renders as a blank.
fn draw_char(img: &mut RgbaImage, x: usize, y: usize, ch: char, color: Rgba<u8>, scale: usize) {
    let code = ch as usize;
    if code >= 128 {
        return;
    }
    let (iw, ih) = (img.width() as usize, img.height() as usize);
    for (row_idx, row) in crate::export_font::FONT8X8[code * GLYPH_H..code * GLYPH_H + GLYPH_H]
        .iter()
        .enumerate()
    {
        for col in 0..GLYPH_W {
            if row >> col & 1 == 0 {
                continue;
            }
            for dy in 0..scale {
                for dx in 0..scale {
                    let (px, py) = (x + col * scale + dx, y + row_idx * scale + dy);
                    if px < iw && py < ih {
                        img.put_pixel(px as u32, py as u32, color);
                    }
                }
            }
        }
    }
}

// ============================================================
// Rendering.
// ============================================================

/// Render the build poster to a PNG byte buffer.
///
/// Takes the build, the database (for power names), the totals (for stats),
/// and the user's options. Returns the PNG bytes or an error string.
pub fn render_to_png(
    build: &CharacterState,
    database: &Db,
    totals: &CalculatedTotals,
    visibility: &Dashboards,
    options: &ExportImageOptions,
    // The build's power art, already decoded. Gathered by `power_art::load_for` before this
    // call, because gathering it is async on the web and this function is neither async nor
    // allowed to be — it is a pure function of its inputs, which is what lets the preview and
    // both export buttons share one render.
    art: &PowerArt,
) -> Result<Vec<u8>, String> {
    let width = CARD_WIDTH * options.scale as usize;
    // Generous working height; the image is cropped to the drawn extent before encoding, so
    // over-allocating here costs a moment of memory and nothing in the output.
    let height = CANVAS_MAX_HEIGHT * options.scale as usize;
    let mut img = RgbaImage::new(width as u32, height as u32);

    // Fill background.
    if options.transparent {
        for pixel in img.pixels_mut() {
            *pixel = Rgba([0, 0, 0, 0]);
        }
    } else {
        fill_rect(&mut img, 0, 0, width, height, BG_COLOR);
    }

    let mut drawer = TextDrawer::new(PAD, PAD, TEXT_COLOR, TITLE_SIZE * options.scale as usize);

    // Header: build name.
    let name = &build.name;
    if !name.is_empty() {
        drawer.write(&mut img, name);
        drawer.advance_y(TITLE_SIZE * options.scale as usize + 12);
    }

    // Archetype line.
    let arch = naming::archetype_name(build, database);
    let arch_text = match arch {
        Some(a) => format!("Level {} {}", build.level, a),
        None => format!("Level {}", build.level),
    };
    let mut sub_drawer = TextDrawer::new(
        PAD,
        drawer.cursor().1,
        CYAN,
        SUBTITLE_SIZE * options.scale as usize,
    );
    sub_drawer.write(&mut img, &arch_text);
    drawer.advance_y(SUBTITLE_SIZE * options.scale as usize + 8);

    // Author credit.
    if !options.author_name.is_empty() {
        let mut author_drawer = TextDrawer::new(
            PAD,
            drawer.cursor().1,
            SLATE_400,
            SMALL_TEXT_SIZE * options.scale as usize,
        );
        author_drawer.write(&mut img, &format!("by {}", options.author_name));
        drawer.advance_y(SMALL_TEXT_SIZE * options.scale as usize + 16);
    }

    // Level + origin.
    if options.show_level {
        let level_text = format!("Lvl {}", build.level);
        let level_size = SMALL_TEXT_SIZE * options.scale as usize;
        let mut level_drawer = TextDrawer::new(
            width - PAD - text_width(&level_text, level_size),
            drawer.cursor().1,
            SLATE_400,
            level_size,
        );
        level_drawer.write(&mut img, &level_text);
    }

    drawer.advance_y(24 * options.scale as usize);

    // Powers section.
    let powers: Vec<&SelectedPower> = if options.only_slotted {
        build
            .all_selected()
            .filter(|p| p.slots.iter().any(Option::is_some))
            .collect()
    } else {
        build.all_selected().collect()
    };

    // Zone label.
    let mut zone_drawer = TextDrawer::new(
        PAD,
        drawer.cursor().1,
        ZONE_LABEL_COLOR,
        ZONE_LABEL_SIZE * options.scale as usize,
    );
    zone_drawer.write(&mut img, "Powers by Level");
    drawer.advance_y(ZONE_LABEL_SIZE * options.scale as usize + 12);

    // Draw power tiles in a grid.
    let cols = if powers.len() > 24 { 5 } else { 4 };
    let tile_area_w = width - 2 * PAD;
    let tile_w = (tile_area_w / cols) - TILE_GAP;
    let tile_h = TILE_H * options.scale as usize;

    for (i, power) in powers.iter().enumerate() {
        let col = i % cols;
        let row = i / cols;
        let tx = PAD + col * (tile_w + TILE_GAP);
        let ty = drawer.cursor().1 + row * (tile_h + TILE_GAP);

        draw_power_tile(
            &mut img,
            tx,
            ty,
            tile_w,
            tile_h,
            power,
            options.scale as usize,
            database,
            art,
        );
    }

    // Advance past power tiles.
    if !powers.is_empty() {
        let rows = powers.len().div_ceil(cols);
        drawer.advance_y(rows * (tile_h + TILE_GAP) + 24 * options.scale as usize);
    }

    // Inherent powers.
    if options.show_inherents {
        let inherents: Vec<&SelectedPower> = build
            .all_selected()
            .filter(|p| p.inherent_category.is_some())
            .collect();
        if !inherents.is_empty() {
            let mut inh_drawer = TextDrawer::new(
                PAD,
                drawer.cursor().1,
                ZONE_LABEL_COLOR,
                ZONE_LABEL_SIZE * options.scale as usize,
            );
            inh_drawer.write(&mut img, "Inherent Powers");
            drawer.advance_y(ZONE_LABEL_SIZE * options.scale as usize + 12);

            for (i, power) in inherents.iter().enumerate() {
                let col = i % cols;
                let row = i / cols;
                let tx = PAD + col * (tile_w + TILE_GAP);
                let ty = drawer.cursor().1 + row * (tile_h + TILE_GAP);
                draw_power_tile(
                    &mut img,
                    tx,
                    ty,
                    tile_w,
                    tile_h,
                    power,
                    options.scale as usize,
                    database,
                    art,
                );
            }
            if !inherents.is_empty() {
                let rows = inherents.len().div_ceil(cols);
                drawer.advance_y(rows * (tile_h + TILE_GAP) + 24 * options.scale as usize);
            }
        }
    }

    // Stats section.
    // Incarnates.
    if options.show_incarnates {
        let picks: Vec<_> = build.incarnates.occupied().collect();
        if !picks.is_empty() {
            let mut inc_drawer = TextDrawer::new(
                PAD,
                drawer.cursor().1,
                ZONE_LABEL_COLOR,
                ZONE_LABEL_SIZE * options.scale as usize,
            );
            inc_drawer.write(&mut img, "Incarnates");
            drawer.advance_y(ZONE_LABEL_SIZE * options.scale as usize + 12);

            for (i, (_slot_id, pick)) in picks.iter().enumerate() {
                let col = i % cols;
                let row = i / cols;
                let tx = PAD + col * (tile_w + TILE_GAP);
                let ty = drawer.cursor().1 + row * (tile_h + TILE_GAP);
                draw_incarnate_tile(
                    &mut img,
                    tx,
                    ty,
                    tile_w,
                    tile_h,
                    pick,
                    options.scale as usize,
                );
            }
            let rows = picks.len().div_ceil(cols);
            drawer.advance_y(rows * (tile_h + TILE_GAP) + 24 * options.scale as usize);
        }
    }

    if !options.stat_sections.is_empty() {
        let mut stats_drawer = TextDrawer::new(
            PAD,
            drawer.cursor().1,
            ZONE_LABEL_COLOR,
            ZONE_LABEL_SIZE * options.scale as usize,
        );
        stats_drawer.write(&mut img, "Character Totals");
        drawer.advance_y(ZONE_LABEL_SIZE * options.scale as usize + 12);

        // Draw stat sections in columns.
        let visible_sections: Vec<StatSection> = options
            .stat_sections
            .iter()
            .filter_map(|s| stat_section_to_stat_section(*s))
            .collect();

        let card_w = STAT_CARD_W * options.scale as usize;
        let stat_cols = ((width - PAD * 2 + STAT_CARD_GAP) / (card_w + STAT_CARD_GAP)).max(1);
        let mut col = 0usize;
        let mut row_top = drawer.cursor().1;
        let mut row_tallest = 0usize;
        for section in visible_sections {
            let rows = stat_registry::in_section(section)
                .filter(|stat| visibility.shows(stat.id))
                .count();
            let card_h = stat_card_height(rows, options.scale as usize);
            draw_stat_section(
                &mut img,
                PAD + col * (card_w + STAT_CARD_GAP),
                row_top,
                card_w,
                &section,
                totals,
                visibility,
                options.scale as usize,
            );
            row_tallest = row_tallest.max(card_h);
            col += 1;
            // A full row of cards moves the cursor down by the tallest one in it, so a short
            // card next to a tall one does not leave the next row overlapping.
            if col == stat_cols {
                col = 0;
                row_top += row_tallest + STAT_CARD_GAP;
                row_tallest = 0;
            }
        }
        if col > 0 {
            row_top += row_tallest + STAT_CARD_GAP;
        }
        drawer.advance_y(row_top.saturating_sub(drawer.cursor().1));
    }

    // Set bonuses.
    if options.show_set_bonuses {
        // Simplified: just note that set bonuses would be rendered here.
        let mut sb_drawer = TextDrawer::new(
            PAD,
            drawer.cursor().1,
            ZONE_LABEL_COLOR,
            ZONE_LABEL_SIZE * options.scale as usize,
        );
        sb_drawer.write(&mut img, "Set Bonuses");
        drawer.advance_y(ZONE_LABEL_SIZE * options.scale as usize + 12);
    }

    // Footer.
    if options.show_credit {
        let mut footer_drawer = TextDrawer::new(
            PAD,
            height - PAD - FOOTER_SIZE * options.scale as usize,
            SLATE_500,
            FOOTER_SIZE * options.scale as usize,
        );
        footer_drawer.write(&mut img, "Made with coh-sidekick.com");
        let mut date_drawer = TextDrawer::new(
            width - PAD - 120,
            height - PAD - FOOTER_SIZE * options.scale as usize,
            SLATE_500,
            FOOTER_SIZE * options.scale as usize,
        );
        let now = chrono::Local::now();
        date_drawer.write(&mut img, &now.format("%Y-%m-%d").to_string());
    }

    // Crop to what was actually drawn, with the footer's own band kept below it.
    let used = (drawer.cursor().1 + PAD * options.scale as usize)
        .max(CARD_MIN_HEIGHT * options.scale as usize)
        .min(height);
    let img = image::imageops::crop_imm(&img, 0, 0, width as u32, used as u32).to_image();

    encode_plain_png(&img)
}

// THE EXPORTED IMAGE NO LONGER CARRIES THE BUILD INSIDE IT, by decision on 2026-09-26. An
// `encode_png` stood here that differed from [`encode_plain_png`] below in one respect: it called
// `coh_data::skif::encode` and wrote the result into a PNG iTXt chunk keyed `skif`
// (`SKIF_CHUNK_KEYWORD`), so every poster the planner produced held a complete, invisible copy of
// the build. Recoverable from `git show 30cd8e918:crates/app/src/export_image.rs`.
//
// NOTHING HAS EVER READ IT. Not this app -- `skif::decode` is reached only from
// `build_file.rs:149`, on text -- and not the beta, where `skif` names the pasteable build code
// and no PNG is parsed anywhere. Importing a build works, and only works, through the text door:
// a pasted code or a link (`build_io::read_pasted`, `build_io::open_route`,
// `build_file::decode_import`). Image import was never planned, and the decision is that the
// export stays one-way.
//
// The doc that stood here said "the poster is both the picture you post and the file someone loads
// back", and the menu hint said "PNG of the build with the build embedded in it". Both were the
// only places the round trip was ever claimed, and both are gone with it -- the second matters
// beyond tidiness, because a shared poster was handing its recipient the whole build without
// saying so.
//
// With the chunk gone the two encoders were the same function, so there is now one:
// [`encode_plain_png`].

/// Encode a canvas as a PNG, carrying the picture and nothing else.
///
/// The one encoder for both the exported poster and the social preview card, since 2026-09-26 --
/// see the note above for what the poster used to carry and why it stopped. An image is not a
/// document here: nothing loads one back into the planner, so nothing is stored in one.
fn encode_plain_png(img: &RgbaImage) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut buf, img.width(), img.height());
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|e| format!("PNG encode failed: {e}"))?;
        writer
            .write_image_data(img.as_raw())
            .map_err(|e| format!("PNG encode failed: {e}"))?;
    }
    Ok(buf)
}

// ============================================================
// Social preview card (RB4d) — the 1200x880 unfurl thumbnail.
// ============================================================

/// Rows per column in the powers band, mirroring the beta's `BuildPreviewCard`.
const PREVIEW_ROWS_PER_COL: usize = 8;
/// Vertical pitch of one power/inherent row.
const PREVIEW_ROW_PITCH: usize = 16;
/// Horizontal gap between power columns.
const PREVIEW_COL_GAP: usize = 10;

/// Draw the social-preview card for a shared build: name, archetype — sets, the compact stat
/// sections, powers in pick order, inherents.
///
/// This is the thumbnail an unfurl shows, not a document. The size is the fixed server
/// contract rather than cropped content, and like every image this app writes it carries no build
/// inside it (see [`encode_plain_png`]) — a preview is never expected to become a build. The stats
/// are drawn by the same [`draw_stat_section`] the poster uses, so the card's numbers are the
/// poster's numbers, computed from the same [`CalculatedTotals`].
pub fn render_preview_png(
    build: &CharacterState,
    database: &Db,
    totals: &CalculatedTotals,
) -> Result<Vec<u8>, String> {
    let mut img = RgbaImage::new(PREVIEW_CARD_WIDTH as u32, PREVIEW_CARD_HEIGHT as u32);
    fill_rect(
        &mut img,
        0,
        0,
        PREVIEW_CARD_WIDTH,
        PREVIEW_CARD_HEIGHT,
        BG_COLOR,
    );

    let mut y = PAD;

    // Header: build name, cut to the card's width. An empty name (a build never titled)
    // simply draws nothing here, like the poster.
    if !build.name.is_empty() {
        let mut title = TextDrawer::new(PAD, y, TEXT_COLOR, TITLE_SIZE);
        title.write(
            &mut img,
            &truncate_to(&build.name, TITLE_SIZE, PREVIEW_CARD_WIDTH - PAD * 2),
        );
        y += text_height(TITLE_SIZE) + 4;
    }

    // Archetype — sets line. The data decides every word; this code only arranges it.
    let arch = naming::archetype_name(build, database);
    let primary = naming::powerset_name(&build.primary, database);
    let secondary = naming::powerset_name(&build.secondary, database);
    let mut arch_text = format!("Level {}", build.level);
    if let Some(name) = arch {
        arch_text = format!("{} {}", arch_text, name);
    }
    if let Some(p) = primary {
        if let Some(s) = secondary {
            arch_text = format!("{} — {} / {}", arch_text, p, s);
        }
    }
    let mut sub = TextDrawer::new(PAD, y, CYAN, SUBTITLE_SIZE);
    sub.write(
        &mut img,
        &truncate_to(&arch_text, SUBTITLE_SIZE, PREVIEW_CARD_WIDTH - PAD * 2),
    );
    y += text_height(SUBTITLE_SIZE) + 8;

    // Divider under the header.
    fill_rect(
        &mut img,
        PAD,
        y,
        PREVIEW_CARD_WIDTH - PAD * 2,
        1,
        BORDER_COLOR,
    );

    // Stat band: the compact sections in a 2x2 grid. The height of each card is computed the
    // way the poster's own grid computes it (the rows the current Dashboard visibility shows),
    // so a short card next to a tall one never overlaps the row under it.
    let visibility = Dashboards::default();
    let mut stat_y = y + 12;
    let mut col = 0usize;
    let mut row_tallest = 0usize;
    for id in StatSectionId::compact() {
        let Some(section) = stat_section_to_stat_section(*id) else {
            continue;
        };
        let rows = stat_registry::in_section(section)
            .filter(|stat| visibility.shows(stat.id))
            .count();
        let card_h = stat_card_height(rows, 1);
        draw_stat_section(
            &mut img,
            PAD + col * (STAT_CARD_W + STAT_CARD_GAP),
            stat_y,
            STAT_CARD_W,
            &section,
            totals,
            &visibility,
            1,
        );
        row_tallest = row_tallest.max(card_h);
        col += 1;
        if col == 2 {
            col = 0;
            stat_y += row_tallest + STAT_CARD_GAP;
            row_tallest = 0;
        }
    }
    stat_y += row_tallest;
    y = stat_y + 12;

    // Powers, in pick order, 8 per column — the same split the poster's inherent section
    // uses, so a pick is either a power or an inherent, never both.
    let powers: Vec<&SelectedPower> = build
        .all_selected()
        .filter(|p| p.inherent_category.is_none())
        .collect();
    let inherents: Vec<&SelectedPower> = build
        .all_selected()
        .filter(|p| p.inherent_category.is_some())
        .collect();
    if !powers.is_empty() {
        let mut label = TextDrawer::new(PAD, y, ZONE_LABEL_COLOR, ZONE_LABEL_SIZE);
        label.write(&mut img, "Powers");
        y += text_height(ZONE_LABEL_SIZE) + 8;
        y = draw_preview_power_rows(&mut img, y, &powers, database) + 8;
    }
    if !inherents.is_empty() {
        let mut label = TextDrawer::new(PAD, y, ZONE_LABEL_COLOR, ZONE_LABEL_SIZE);
        label.write(&mut img, "Inherent");
        y += text_height(ZONE_LABEL_SIZE) + 8;
        // The returned cursor is dropped rather than stored: this is the last band on the
        // card, so nothing below reads `y`. The call is kept for what it draws into `img`.
        draw_preview_power_rows(&mut img, y, &inherents, database);
    }

    encode_plain_png(&img)
}

/// One band of power rows, 8 per column, pick order filling the first column before the next
/// — the beta's card does the same, so a build reads top-to-bottom like a build list.
/// Returns the next free y.
fn draw_preview_power_rows(
    img: &mut RgbaImage,
    y: usize,
    powers: &[&SelectedPower],
    database: &Db,
) -> usize {
    let cols = powers.len().div_ceil(PREVIEW_ROWS_PER_COL).max(1);
    let content_w = PREVIEW_CARD_WIDTH - PAD * 2;
    let col_w = (content_w - (cols - 1) * PREVIEW_COL_GAP) / cols;
    for (i, power) in powers.iter().enumerate() {
        let col = i % cols;
        let row = i / cols;
        draw_preview_power_row(
            img,
            PAD + col * (col_w + PREVIEW_COL_GAP),
            y + row * PREVIEW_ROW_PITCH,
            col_w,
            power,
            database,
        );
    }
    let rows = powers.len().div_ceil(cols);
    y + rows * PREVIEW_ROW_PITCH
}

/// A compact single-line power row: name, hard right, `L{level}` beside it. The name keeps
/// whatever the level's label did not take, cut to fit (see [`truncate_to`]).
fn draw_preview_power_row(
    img: &mut RgbaImage,
    x: usize,
    y: usize,
    w: usize,
    power: &SelectedPower,
    database: &Db,
) {
    let mut name = TextDrawer::new(x, y, TEXT_COLOR, SMALL_TEXT_SIZE);
    name.write(
        img,
        &truncate_to(
            tile_label(power, database),
            SMALL_TEXT_SIZE,
            w.saturating_sub(CARD_PAD),
        ),
    );
    if power.level > 0 {
        let level = format!("L{}", power.level);
        let level_w = text_width(&level, SMALL_TEXT_SIZE);
        let mut level_drawer = TextDrawer::new(x + w - level_w, y, SLATE_400, SMALL_TEXT_SIZE);
        level_drawer.write(img, &level);
    }
}

/// Convert our StatSectionId to the real StatSection.
fn stat_section_to_stat_section(id: StatSectionId) -> Option<StatSection> {
    match id {
        StatSectionId::Offense => Some(StatSection::Offense),
        StatSectionId::SurvivalAndMobility => Some(StatSection::Survival),
        StatSectionId::StealthAndPerception => Some(StatSection::Movement),
        StatSectionId::Defense => Some(StatSection::Defense),
        StatSectionId::DamageResistance => Some(StatSection::Resistance),
        StatSectionId::StatusProtection => Some(StatSection::StatusProtection),
        StatSectionId::StatusEffectResistance => Some(StatSection::StatusResistance),
        StatSectionId::DebuffResistance => Some(StatSection::DebuffResistance),
    }
}

/// Draw a power tile.
fn draw_power_tile(
    img: &mut RgbaImage,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    power: &SelectedPower,
    scale: usize,
    database: &Db,
    art: &PowerArt,
) {
    // Draw tile background.
    fill_rect(img, x, y, w, h, TILE_BG);
    // Draw tile border.
    stroke_rect(img, x, y, w, h, TILE_BORDER);

    let pad = CARD_PAD * scale;

    // One lookup answers both halves of the tile: which picture it wears and what it is called.
    let def =
        crate::view::power_view::resolve_power_def(database, &power.powerset, &power.internal_name);

    // The power's art leads the tile, when we have it. An icon that did not load costs the tile
    // its picture and nothing else — the name and level draw where they always did, against the
    // gutter the icon would have occupied only if there is one to occupy it (see `power_art`).
    let icon = def
        .and_then(|def| def.extra.get("icon"))
        .and_then(|value| value.as_str())
        .and_then(|name| art.get(Some(name)));

    let icon_gutter = match icon {
        Some(source) => {
            // Square, and as tall as the tile's inner box allows — the vendored art is 32x32,
            // so at scale 1 this draws it very near its native size and the nearest-neighbour
            // sampling in `draw_image_scaled` costs nothing visible.
            let size = h.saturating_sub(pad * 2);
            draw_image_scaled(img, source, x + pad, y + pad, size, size);
            size + pad
        }
        None => 0,
    };

    // The level sits hard right; the name gets whatever is left and is cut to it.
    let level = format!("L{}", power.level);
    let level_w = text_width(&level, TILE_LEVEL_SIZE * scale);
    let name_space = w.saturating_sub(pad * 3 + level_w + icon_gutter);
    let mut drawer = TextDrawer::new(
        x + pad + icon_gutter,
        y + pad,
        TEXT_COLOR,
        TILE_NAME_SIZE * scale,
    );
    drawer.write(
        img,
        &truncate_to(
            tile_label(power, database),
            TILE_NAME_SIZE * scale,
            name_space,
        ),
    );

    if power.level > 0 {
        let mut level_drawer = TextDrawer::new(
            x + w - pad - level_w,
            y + pad,
            SLATE_400,
            TILE_LEVEL_SIZE * scale,
        );
        level_drawer.write(img, &level);
    }
}

/// What a tile calls its power: the name a reader knows it by.
///
/// The poster used to print the export's `internalName` — `Stone_Armor` where the card beside it
/// says Rock Armor, `Earths_Embrace` for Earth's Embrace. Nothing caught it because nothing read
/// the def here at all until the icon needed one, and the only test that drew a tile was the
/// ignored viewer, whose fixture did not resolve either (see `a_stone_build`).
///
/// A power whose def will not resolve keeps its internal name rather than going blank: the raw
/// string is what we actually know about it, and a tile with no words at all would read as a
/// power the build does not have (Rule 1).
fn tile_label<'a>(power: &'a SelectedPower, database: &'a Db) -> &'a str {
    crate::view::power_view::resolve_power_def(database, &power.powerset, &power.internal_name)
        .map_or(power.internal_name.as_str(), |def| def.name.as_str())
}

/// Composite `source` into `img` at `(x, y)`, scaled to `w` x `h`, over whatever is already
/// there.
///
/// Nearest-neighbour, because every input is a 32x32 icon drawn at very near 32 px and a
/// filtered resample would only soften art that was authored for this size. Alpha is a real
/// `over` blend rather than a copy: these icons carry transparent corners, and copying would
/// stamp the tile's rounded background out into hard black squares.
///
/// Anything that would land outside the canvas is skipped, so a tile near the crop edge draws
/// what fits instead of panicking on a bounds check.
fn draw_image_scaled(
    img: &mut RgbaImage,
    source: &RgbaImage,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
) {
    if w == 0 || h == 0 || source.width() == 0 || source.height() == 0 {
        return;
    }
    for row in 0..h {
        let dest_y = (y + row) as u32;
        if dest_y >= img.height() {
            break;
        }
        // Map the destination pixel back to a source pixel. `min` guards the last row/column
        // against the rounding that would otherwise index one past the edge.
        let src_y = ((row * source.height() as usize) / h).min(source.height() as usize - 1) as u32;
        for col in 0..w {
            let dest_x = (x + col) as u32;
            if dest_x >= img.width() {
                break;
            }
            let src_x =
                ((col * source.width() as usize) / w).min(source.width() as usize - 1) as u32;
            let src = source.get_pixel(src_x, src_y).0;
            if src[3] == 0 {
                continue;
            }
            if src[3] == 255 {
                img.put_pixel(dest_x, dest_y, Rgba(src));
                continue;
            }
            let dst = img.get_pixel(dest_x, dest_y).0;
            let a = src[3] as u32;
            let inv = 255 - a;
            let blend = |s: u8, d: u8| ((s as u32 * a + d as u32 * inv) / 255) as u8;
            img.put_pixel(
                dest_x,
                dest_y,
                Rgba([
                    blend(src[0], dst[0]),
                    blend(src[1], dst[1]),
                    blend(src[2], dst[2]),
                    // The destination keeps the stronger alpha: on a transparent export the
                    // icon is what makes its pixels opaque, and on an opaque one this is 255
                    // either way.
                    dst[3].max(src[3]),
                ]),
            );
        }
    }
}

/// Draw an incarnate tile (simplified).
fn draw_incarnate_tile(
    img: &mut RgbaImage,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    pick: &coh_data::IncarnateSlot,
    scale: usize,
) {
    // Draw tile background.
    fill_rect(img, x, y, w, h, TILE_BG);
    // Draw tile border.
    stroke_rect(img, x, y, w, h, TILE_BORDER);

    // Draw incarnate name.
    let mut drawer = TextDrawer::new(x + 8, y + 8, TEXT_COLOR, TILE_NAME_SIZE * scale);
    drawer.write(img, &pick.power_name);
}

/// Draw a stat section card.
fn draw_stat_section(
    img: &mut RgbaImage,
    x: usize,
    y: usize,
    w: usize,
    section: &StatSection,
    totals: &CalculatedTotals,
    visibility: &Dashboards,
    scale: usize,
) {
    let rows: Vec<_> = stat_registry::in_section(*section)
        .filter(|stat| visibility.shows(stat.id))
        .map(|stat| (stat.label, stat.resolve(totals)))
        .collect();

    let height = stat_card_height(rows.len(), scale);
    fill_rect(img, x, y, w, height, STAT_CARD_BG);
    stroke_rect(img, x, y, w, height, BORDER_COLOR);

    let mut title = TextDrawer::new(
        x + 8 * scale,
        y + 8 * scale,
        TEXT_COLOR,
        STAT_LABEL_SIZE * scale,
    );
    title.write(img, section.title());

    let row_h = text_height(STAT_VALUE_SIZE * scale) + CARD_PAD * scale;
    for (i, (label, resolved)) in rows.iter().enumerate() {
        let row_y = y + stat_card_header(scale) + i * row_h;
        let value_w_hint =
            text_width(&resolved.text, STAT_VALUE_SIZE * scale) + CARD_PAD * 4 * scale;
        let label_space = w.saturating_sub(value_w_hint);
        let mut name = TextDrawer::new(
            x + CARD_PAD * scale,
            row_y,
            SLATE_400,
            SMALL_TEXT_SIZE * scale,
        );
        name.write(
            img,
            &truncate_to(label, SMALL_TEXT_SIZE * scale, label_space),
        );

        // The value is right-aligned, so its start depends on how wide it draws.
        let text = if resolved.simulated {
            // The dashboard marks simulated numbers so a screenshot of a buffed build cannot
            // pass as the build's own. An exported poster IS that screenshot, so it carries the
            // same mark rather than dropping it.
            format!("{} sim", resolved.text)
        } else {
            resolved.text.clone()
        };
        let value_w = text_width(&text, STAT_VALUE_SIZE * scale);
        let value_x = (x + w).saturating_sub(CARD_PAD * scale + value_w);
        let colour = if resolved.at_cap {
            AMBER
        } else {
            SUBTITLE_COLOR
        };
        let mut value = TextDrawer::new(value_x, row_y, colour, STAT_VALUE_SIZE * scale);
        value.write(img, &text);
    }

    if rows.is_empty() {
        let mut empty = TextDrawer::new(
            x + CARD_PAD * scale,
            y + stat_card_header(scale),
            SLATE_500,
            SMALL_TEXT_SIZE * scale,
        );
        empty.write(img, "No stats shown");
    }
}

/// The title band above a card's first row: the title itself plus padding either side.
fn stat_card_header(scale: usize) -> usize {
    CARD_PAD * scale + text_height(STAT_LABEL_SIZE * scale) + CARD_PAD * scale
}

/// How tall a card with `rows` rows draws, so the layout can reserve it before drawing.
fn stat_card_height(rows: usize, scale: usize) -> usize {
    let row_h = text_height(STAT_VALUE_SIZE * scale) + CARD_PAD * scale;
    stat_card_header(scale) + rows.max(1) * row_h + CARD_PAD * scale
}

/// How wide `text` draws at `size`. The font is fixed-width, so this is exact rather than an
/// estimate — right-aligning a value depends on it.
fn text_width(text: &str, size: usize) -> usize {
    text.chars().count() * glyph_advance(size)
}

/// How tall a line drawn at `size` actually is. `size` is a request; the bitmap font only scales
/// in whole multiples of 8, so the drawn height is the request rounded down. Row pitch has to
/// follow the DRAWN height or rows overlap.
fn text_height(size: usize) -> usize {
    (size / GLYPH_H).max(1) * GLYPH_H
}

/// Distance from one character's left edge to the next.
fn glyph_advance(size: usize) -> usize {
    (size / GLYPH_H).max(1) * (GLYPH_W + 1)
}

/// `text` cut to whatever fits in `max_width`, with an ellipsis when it had to cut. A name that
/// silently ran past its tile collided with the level label beside it.
fn truncate_to(text: &str, size: usize, max_width: usize) -> String {
    if text_width(text, size) <= max_width {
        return text.to_string();
    }
    let fits = max_width / glyph_advance(size);
    if fits <= 1 {
        return String::new();
    }
    text.chars().take(fits - 1).chain(['…']).collect()
}

/// Fill a rectangle with a solid color.
fn fill_rect(img: &mut RgbaImage, x: usize, y: usize, w: usize, h: usize, color: Rgba<u8>) {
    for py in y..y.saturating_add(h) {
        for px in x..x.saturating_add(w) {
            if px < img.width() as usize && py < img.height() as usize {
                img.put_pixel(px as u32, py as u32, color);
            }
        }
    }
}

/// Draw a rectangle outline.
fn stroke_rect(img: &mut RgbaImage, x: usize, y: usize, w: usize, h: usize, color: Rgba<u8>) {
    // Top edge.
    for px in x..x.saturating_add(w) {
        if px < img.width() as usize && y < img.height() as usize {
            img.put_pixel(px as u32, y as u32, color);
        }
    }
    // Bottom edge.
    let bottom_y = y + h - 1;
    for px in x..x.saturating_add(w) {
        if px < img.width() as usize && bottom_y < img.height() as usize {
            img.put_pixel(px as u32, bottom_y as u32, color);
        }
    }
    // Left edge.
    for py in y..y.saturating_add(h) {
        if x < img.width() as usize && py < img.height() as usize {
            img.put_pixel(x as u32, py as u32, color);
        }
    }
    // Right edge.
    let right_x = x + w - 1;
    for py in y..y.saturating_add(h) {
        if right_x < img.width() as usize && py < img.height() as usize {
            img.put_pixel(right_x as u32, py as u32, color);
        }
    }
}

// ============================================================
// Modal.
// ============================================================

/// The export image modal.
#[component]
pub fn ExportImageHost(database: Option<Db>) -> Element {
    let mut open = use_context::<ExportImageOpen>().0;
    if !open() {
        return rsx! {};
    }
    rsx! {
        Modal {
            title: "Export as image".to_string(),
            size: ModalSize::Xl,
            on_close: move |_| open.set(false),
            ExportImageBody { database }
        }
    }
}

/// The export's open state, held at the shell root for the containment reason every modal here
/// shares (see [`crate::modal`]).
#[derive(Clone, Copy)]
pub struct ExportImageOpen(pub Signal<bool>);

#[component]
fn ExportImageBody(database: Option<Db>) -> Element {
    let session = use_context::<crate::build_session::BuildSession>();
    let totals = use_context::<crate::panels::stats::BuildTotals>().0;
    let config = use_context::<crate::panels::dashboards::DashboardConfig>().0;
    let mut saved = use_signal(|| Option::<Result<crate::build_file::WrittenTo, String>>::None);
    let mut copied = use_signal(|| Option::<Result<(), String>>::None);

    let mut options = use_signal(ExportImageOptions::default);
    // Read once here rather than inside the markup: an rsx! element body holds
    // attributes and children, not statements.
    // rsx! interpolation takes an identifier or expression, not an if-block, so the
    // selected/unselected classes are picked here.
    const PRESET_ON: &str = "bg-primary border-primary-hover text-on-primary";
    const PRESET_OFF: &str = "bg-gray-800 border-gray-700 text-gray-300 hover:bg-gray-700";
    let preset = options.read().preset;
    let compact_class = if preset == ExportPreset::Compact {
        PRESET_ON
    } else {
        PRESET_OFF
    };
    let standard_class = if preset == ExportPreset::Standard {
        PRESET_ON
    } else {
        PRESET_OFF
    };
    let full_class = if preset == ExportPreset::Full {
        PRESET_ON
    } else {
        PRESET_OFF
    };
    let scale = options.read().scale;
    let scale_1_class = if scale == 1 { PRESET_ON } else { PRESET_OFF };
    let scale_2_class = if scale == 2 { PRESET_ON } else { PRESET_OFF };

    let Some(database) = database else {
        return rsx! {
            div { class: "load-state", "Loading the dataset…" }
        };
    };

    // Derived, not stored: the poster is a function of the build, the totals, the visible stats
    // and the options, and derived state that is STORED is state that can be stale. It is also
    // computed once for the preview and both buttons rather than three times.
    //
    // A resource rather than a memo, because the power art has to be gathered before the render
    // and gathering it is async on the web (see `power_art`). The render itself stays the pure
    // synchronous function it was; only the wait moved out here. That the whole thing is one
    // resource — gather THEN draw — is what keeps a half-drawn poster from being exportable:
    // until the art is in, there are no bytes at all, so there is no window in which Download
    // could hand back a build whose icons had not arrived yet.
    let png = use_resource({
        let database = database.clone();
        move || {
            // Read on the outside of the future, so the resource is reactive on all four and
            // re-renders when any of them moves.
            let database = database.clone();
            let build = session.build.read().clone();
            let totals = totals.read().clone();
            let config = config.read().clone();
            let options = options.read().clone();
            async move {
                let art = crate::power_art::load_for(&build, &database).await;
                render_to_png(&build, &database, &totals, &config, &options, &art)
            }
        }
    });
    let preview_src = png
        .read()
        .as_ref()
        .and_then(|rendered| rendered.as_ref().ok())
        .map(|bytes| format!("data:image/png;base64,{}", crate::clipboard::base64(bytes)));
    // Only a finished render is exportable; a pending one leaves both buttons off rather than
    // offering bytes that do not exist yet.
    let ready = matches!(&*png.read(), Some(Ok(_)));
    let file_name = format!(
        "{}.png",
        crate::build_file::file_name_for(&session.build.read()).trim_end_matches(".skif")
    );

    rsx! {
        div { class: "flex flex-col lg:flex-row gap-5 p-5",
                // Controls.
                div { class: "lg:w-[300px] shrink-0 space-y-4",
                    // Preset.
                    div {
                        div { class: "text-xs font-semibold text-gray-300 uppercase mb-1.5", "Detail preset" }
                        div { class: "flex gap-2",
                            button {
                                r#type: "button",
                                class: "flex-1 px-2 py-1.5 rounded text-sm border transition-colors {compact_class}",
                                onclick: move |_| options.write().apply_preset(ExportPreset::Compact),
                                "Compact"
                            }
                        }
                        div { class: "flex gap-2 mt-1",
                            button {
                                r#type: "button",
                                class: "flex-1 px-2 py-1.5 rounded text-sm border transition-colors {standard_class}",
                                onclick: move |_| options.write().apply_preset(ExportPreset::Standard),
                                "Standard"
                            }
                        }
                        div { class: "flex gap-2 mt-1",
                            button {
                                r#type: "button",
                                class: "flex-1 px-2 py-1.5 rounded text-sm border transition-colors {full_class}",
                                onclick: move |_| options.write().apply_preset(ExportPreset::Full),
                                "Full"
                            }
                        }
                    }

                    // Author.
                    div {
                        label { class: "text-xs font-semibold text-gray-300 uppercase mb-1.5 block", "Author / character" }
                        input {
                            r#type: "text",
                            value: options.read().author_name.clone(),
                            oninput: move |evt| options.write().author_name = evt.value().to_string(),
                            placeholder: "Optional credit",
                            class: "w-full bg-gray-900 border border-gray-700 rounded px-2 py-1.5 text-sm text-gray-200 focus:outline-none focus:ring-1 focus:ring-[var(--color-ring)]/50"
                        }
                    }

                    // Toggles.
                    div { class: "space-y-2",
                        label { class: "flex items-center gap-2 text-sm text-gray-200 cursor-pointer",
                            input {
                                r#type: "checkbox",
                                checked: options.read().show_level,
                                onchange: move |evt| options.write().show_level = evt.checked(),
                            }
                            "Show level"
                        }
                        label { class: "flex items-center gap-2 text-sm text-gray-200 cursor-pointer",
                            input {
                                r#type: "checkbox",
                                checked: options.read().show_credit,
                                onchange: move |evt| options.write().show_credit = evt.checked(),
                            }
                            "Show credit footer"
                        }
                        label { class: "flex items-center gap-2 text-sm text-gray-200 cursor-pointer",
                            input {
                                r#type: "checkbox",
                                checked: options.read().show_enhancements,
                                onchange: move |evt| options.write().show_enhancements = evt.checked(),
                            }
                            "Show enhancements"
                        }
                        label { class: "flex items-center gap-2 text-sm text-gray-200 cursor-pointer",
                            input {
                                r#type: "checkbox",
                                checked: options.read().show_inherents,
                                onchange: move |evt| options.write().show_inherents = evt.checked(),
                            }
                            "Show inherent powers"
                        }
                        label { class: "flex items-center gap-2 text-sm text-gray-200 cursor-pointer",
                            input {
                                r#type: "checkbox",
                                checked: options.read().show_incarnates,
                                onchange: move |evt| options.write().show_incarnates = evt.checked(),
                            }
                            "Show incarnates"
                        }
                        label { class: "flex items-center gap-2 text-sm text-gray-200 cursor-pointer",
                            input {
                                r#type: "checkbox",
                                checked: options.read().only_slotted,
                                onchange: move |evt| options.write().only_slotted = evt.checked(),
                            }
                            "Only slotted powers"
                        }
                        label { class: "flex items-center gap-2 text-sm text-gray-200 cursor-pointer",
                            input {
                                r#type: "checkbox",
                                checked: options.read().show_set_bonuses,
                                onchange: move |evt| options.write().show_set_bonuses = evt.checked(),
                            }
                            "Show set bonuses"
                        }
                        label { class: "flex items-center gap-2 text-sm text-gray-200 cursor-pointer",
                            input {
                                r#type: "checkbox",
                                checked: options.read().transparent,
                                onchange: move |evt| options.write().transparent = evt.checked(),
                            }
                            "Transparent background"
                        }
                    }

                    // Scale.
                    div {
                        label { class: "text-xs font-semibold text-gray-300 uppercase mb-1.5 block", "Resolution" }
                        div { class: "flex gap-2",
                            button {
                                r#type: "button",
                                class: "px-2 py-0.5 rounded text-xs border {scale_1_class}",
                                onclick: move |_| options.write().scale = 1,
                                "1x"
                            }
                            button {
                                r#type: "button",
                                class: "px-2 py-0.5 rounded text-xs border {scale_2_class}",
                                onclick: move |_| options.write().scale = 2,
                                "2x"
                            }
                        }
                    }
                }

                // Preview.
                div { class: "flex-1",
                    div { class: "bg-gray-900 rounded border border-gray-700 p-4",
                        match (&*png.read(), &preview_src) {
                            (Some(Ok(bytes)), Some(src)) => rsx! {
                                div { class: "text-xs text-gray-400 mb-2",
                                    "Preview — {bytes.len() / 1024} KB"
                                }
                                img {
                                    class: "max-w-full rounded",
                                    src: "{src}",
                                    alt: "The build as a poster",
                                }
                            },
                            (Some(Err(why)), _) => rsx! {
                                div { class: "text-sm text-red-300", "Could not render: {why}" }
                            },
                            // Pending. Its own state, and not the error state: on the web the
                            // first poster of a session waits on a few dozen icon fetches, and
                            // a red "could not render" is a lie about a render still going.
                            (None, _) => rsx! {
                                div { class: "text-sm text-gray-400", "Drawing the poster…" }
                            },
                            _ => rsx! {},
                        }
                    }

                    // Actions.
                    div { class: "flex gap-2 mt-4",
                        button {
                            r#type: "button",
                            class: "flex-1 px-4 py-2 bg-primary border border-primary-hover text-on-primary rounded text-sm font-medium transition-colors hover:bg-primary-hover",
                            disabled: !ready,
                            onclick: {
                                let file_name = file_name.clone();
                                move |_| {
                                    let file_name = file_name.clone();
                                    let bytes = png.read().clone();
                                    async move {
                                        match bytes {
                                            Some(Ok(bytes)) => saved.set(Some(
                                                crate::build_file::write_png_file(&file_name, &bytes).await,
                                            )),
                                            Some(Err(why)) => saved.set(Some(Err(why))),
                                            // Unreachable while the button is gated on `ready`.
                                            None => {}
                                        }
                                    }
                                }
                            },
                            "Download PNG"
                        }
                        button {
                            r#type: "button",
                            class: "flex-1 px-4 py-2 bg-gray-700 border border-gray-600 text-gray-200 rounded text-sm font-medium transition-colors hover:bg-gray-600",
                            disabled: !ready,
                            // The copy runs in this handler's own task: both clipboard
                            // mechanisms are gated on a live user gesture, and the gesture is
                            // over by the time a later task would run.
                            onclick: move |_| {
                                let bytes = png.read().clone();
                                async move {
                                    match bytes {
                                        Some(Ok(bytes)) => copied.set(Some(crate::clipboard::copy_png(&bytes).await)),
                                        Some(Err(why)) => copied.set(Some(Err(why))),
                                        // Unreachable while the button is gated on `ready`.
                                        None => {}
                                    }
                                }
                            },
                            "Copy to Clipboard"
                        }
                    }

                    // Both outcomes are reported. A copy button that claims success while the
                    // clipboard still holds what it held before is the soft-wrong outcome the
                    // clipboard module exists to prevent, and the user finds out by pasting.
                    match &*saved.read() {
                        Some(Ok(Some(path))) => rsx! {
                            p { class: "text-xs text-emerald-300 mt-2", "Saved to {path}" }
                        },
                        Some(Ok(None)) => rsx! {
                            p { class: "text-xs text-emerald-300 mt-2", "Saved." }
                        },
                        Some(Err(why)) => rsx! {
                            p { class: "text-xs text-red-300 mt-2", "Could not save: {why}" }
                        },
                        None => rsx! {},
                    }
                    match &*copied.read() {
                        Some(Ok(())) => rsx! {
                            p { class: "text-xs text-emerald-300 mt-2", "Copied to the clipboard." }
                        },
                        Some(Err(why)) => rsx! {
                            p { class: "text-xs text-red-300 mt-2", "Could not copy: {why}" }
                        },
                        None => rsx! {},
                    }
                }
        }
    }
}

/// Entry point from the build menu.
#[component]
pub fn ExportImageEntry(database: Option<Db>) -> Element {
    let mut export = use_context::<ExportImageOpen>().0;
    let mut menu = use_context::<crate::popover::PopoverOpen>().0;

    rsx! {
        button {
            r#type: "button",
            class: "main-menu__item",
            disabled: database.is_none(),
            onclick: move |_| {
                export.set(true);
                menu.set(false);
            },
            span { class: "main-menu__label", "Export as image…" }
            span { class: "main-menu__hint", "PNG of the build, to post or save" }
        }
    }
}
