//! LCD key rendering for the Stream Deck.
//!
//! Each key is a 72×72 RGB image generated on demand. The exact pixel
//! format reaches the device through `Kind::key_image_format()` — this
//! module renders into a [`DynamicImage`] which the
//! `elgato-streamdeck` crate then transcodes (JPEG for OriginalV2 /
//! Mk2; BMP for older models).
//!
//! Visual language:
//! - **Idle** tiles are solid dim, with a small icon and label. Cached.
//! - **Active** tiles pulse, get a bright border, and add type-specific
//!   decoration (live RGB strip for chasers, orbiting dot for movements,
//!   strobe for blackout/blind). Re-rendered each animation tick — they
//!   carry a `phase` field that advances ~10 fps and drives the
//!   animation maths in this file.
//!
//! Render budget: re-painting all 15 keys per tick would saturate the
//! USB pipe. The caller diffs against the previous frame and only sends
//! changed keys. A cache (`RenderCache`) memoises idle visuals so the
//! 80 % of keys sitting still cost nothing per tick.

use std::collections::HashMap;

use ab_glyph::{FontRef, PxScale};
use image::{DynamicImage, Rgb, RgbImage};
use imageproc::drawing::{
    draw_filled_circle_mut, draw_filled_rect_mut, draw_hollow_rect_mut, draw_line_segment_mut,
    draw_polygon_mut, draw_text_mut, text_size,
};
use imageproc::point::Point;
use imageproc::rect::Rect;

/// Stream Deck MK.2 / OriginalV2 image side. The crate handles
/// rotation/mirroring per device internally so we always render in the
/// natural top-down 72×72.
pub const KEY_IMAGE_SIZE: u32 = 72;

/// Embedded font: DejaVu Sans Bold. Bundled so we don't depend on
/// system fonts (which differ between macOS / Windows / Linux). The
/// linker strips unused glyph tables under `lto`, so the binary cost is
/// well under the file's 700 KB.
const FONT_BYTES: &[u8] = include_bytes!("../../assets/DejaVuSans-Bold.ttf");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TileKind {
    /// First-row chasers. Active tile shows a triangle play glyph plus
    /// the live RGB output of the chaser's slots in a strip across the
    /// bottom — replaces the Launchpad's separate "miniature stage"
    /// CC row by inlining it into the tile itself.
    Chaser,
    /// Second-row movement generators. Active tile shows a small dot
    /// orbiting the icon, driven by `phase`, to communicate "this is
    /// running" at a glance.
    Movement,
    /// Third-row scenes. Active tile gets a bold label and a top
    /// "now playing" stripe.
    Scene,
    /// Momentary blind. Strobes white-on-grey when held — fast cadence
    /// to read as urgent.
    Blind,
    /// Toggleable blackout. Strobes red-on-black when active.
    Blackout,
    /// TAP button — registers a tap timestamp, derives the overall BPM
    /// from the rolling window. Idle: hand-tap glyph. Active (just
    /// pressed) flashes briefly so the operator gets visual feedback
    /// of the press registering.
    Tap,
    /// Overall-BPM toggle — flips the global override on/off. Idle:
    /// dim metronome glyph. Active: bright pulsing metronome.
    BpmToggle,
}

/// One key's desired visual. Cached when `state == Idle`; re-rendered
/// every animation tick when `state == Active` because `phase` mutates.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum KeyVisual {
    /// No assignment — the LCD stays black.
    Empty,
    Tile {
        kind: TileKind,
        /// Short label drawn near the top. Empty string draws nothing.
        label: String,
        /// `(off_color, on_color)`. Idle tiles use `off_color`; active
        /// tiles pulse between a darker shade and `on_color`.
        palette: ((u8, u8, u8), (u8, u8, u8)),
        active: bool,
        /// Animation phase counter, in animation-frames. Advances once
        /// per `ANIMATION_FRAME_MS` (see controller). Idle tiles set
        /// this to 0 so the cache key is stable.
        phase: u32,
        /// Live RGB outputs of the chaser's 8 slots — only meaningful
        /// when `kind == Chaser` and `active == true`. `None` for
        /// every other case.
        slots: Option<[(u8, u8, u8); 8]>,
    },
}

/// Per-controller render cache. Only memoises idle visuals: active
/// tiles change every animation tick (phase advances) so caching them
/// would just churn the map. ~30 entries covers a typical show: each
/// chaser/movement/scene has one idle visual, plus blind+blackout idle.
pub struct RenderCache {
    map: HashMap<KeyVisual, DynamicImage>,
    font: FontRef<'static>,
}

impl Default for RenderCache {
    fn default() -> Self {
        Self {
            map: HashMap::new(),
            // The embedded font is validated at build time by
            // `include_bytes!` not actually parsing it — but
            // `FontRef::try_from_slice` does parse, and a bad TTF here
            // would panic on first render. We'd rather crash early
            // than silently draw blank keys.
            font: FontRef::try_from_slice(FONT_BYTES)
                .expect("embedded DejaVuSans-Bold.ttf is a valid TTF"),
        }
    }
}

impl RenderCache {
    pub fn render(&mut self, visual: &KeyVisual) -> DynamicImage {
        match visual {
            KeyVisual::Empty => render_empty(),
            KeyVisual::Tile { active: false, .. } => self.cached(visual),
            KeyVisual::Tile { active: true, .. } => render_uncached(&self.font, visual),
        }
    }

    fn cached(&mut self, visual: &KeyVisual) -> DynamicImage {
        if let Some(img) = self.map.get(visual) {
            return img.clone();
        }
        // Bound the cache so a show with hundreds of unique scene names
        // doesn't grow without limit.
        if self.map.len() >= 64 {
            self.map.clear();
        }
        let img = render_uncached(&self.font, visual);
        self.map.insert(visual.clone(), img.clone());
        img
    }
}

fn render_empty() -> DynamicImage {
    let mut img = RgbImage::new(KEY_IMAGE_SIZE, KEY_IMAGE_SIZE);
    fill(&mut img, Rgb([0, 0, 0]));
    DynamicImage::ImageRgb8(img)
}

fn render_uncached(font: &FontRef<'static>, visual: &KeyVisual) -> DynamicImage {
    let mut img = RgbImage::new(KEY_IMAGE_SIZE, KEY_IMAGE_SIZE);
    match visual {
        KeyVisual::Empty => fill(&mut img, Rgb([0, 0, 0])),
        KeyVisual::Tile {
            kind,
            label,
            palette,
            active,
            phase,
            slots,
        } => {
            draw_tile(&mut img, font, *kind, label, *palette, *active, *phase, slots.as_ref());
        }
    }
    DynamicImage::ImageRgb8(img)
}

fn draw_tile(
    img: &mut RgbImage,
    font: &FontRef<'static>,
    kind: TileKind,
    label: &str,
    palette: ((u8, u8, u8), (u8, u8, u8)),
    active: bool,
    phase: u32,
    slots: Option<&[(u8, u8, u8); 8]>,
) {
    let (_off_color, on_color) = palette;

    // -- background ------------------------------------------------------
    // The contrast between idle and active is intentionally dramatic:
    // idle sits at ~15 % of the on-tone (almost black with a hint of
    // colour) while active pulses 55 %–100 %. Earlier the gap was too
    // narrow (off=palette dim, active=65–100 %) and on the device's
    // gamma curve activations looked nearly identical to idles.
    let bg = match (kind, active) {
        (_, false) => dim(on_color, 0.15),
        // Blind: rapid full-power strobe — toggles between near-black
        // and full white-tone every animation frame to read as "alarm".
        (TileKind::Blind, true) => {
            if phase % 2 == 0 { on_color } else { dim(on_color, 0.10) }
        }
        // Blackout: red strobe with the same on/off shape but at half
        // the rate, so when both are pressed at once they're audibly
        // distinguishable as different rhythms.
        (TileKind::Blackout, true) => {
            if (phase / 2) % 2 == 0 { on_color } else { dim(on_color, 0.10) }
        }
        // Chaser / Movement / Scene: smooth sine pulse 55 %–100 %.
        _ => {
            let t = pulse(phase, 14); // ~1.4 s per cycle at 10 fps
            mix_color(dim(on_color, 0.55), on_color, t)
        }
    };
    fill(img, Rgb([bg.0, bg.1, bg.2]));

    // -- border ----------------------------------------------------------
    if active {
        // 3 px bright-white inner border. Visible from a stage away.
        for inset in 0..3_i32 {
            draw_hollow_rect_mut(
                img,
                Rect::at(inset, inset).of_size(
                    KEY_IMAGE_SIZE - 2 * inset as u32,
                    KEY_IMAGE_SIZE - 2 * inset as u32,
                ),
                Rgb([255, 255, 255]),
            );
        }
    } else {
        // Idle: 1 px frame at 50 % of the on-tone so the tile reads as
        // "available, not running" rather than "off / empty".
        let edge = dim(on_color, 0.5);
        draw_hollow_rect_mut(
            img,
            Rect::at(0, 0).of_size(KEY_IMAGE_SIZE, KEY_IMAGE_SIZE),
            Rgb([edge.0, edge.1, edge.2]),
        );
    }

    // Foreground colour for icons and text. Active tiles use BLACK on
    // the bright pulse so the contrast jump from idle (light fg on
    // dark bg) to active (dark fg on light bg) is unmistakable.
    let fg = if active {
        Rgb([0, 0, 0])
    } else {
        let light = brighten(on_color, 100);
        Rgb([light.0, light.1, light.2])
    };

    // -- label (top band) -----------------------------------------------
    if !label.is_empty() {
        let display = truncate_label(label, 9);
        let scale = pick_scale(font, &display, KEY_IMAGE_SIZE - 6);
        let (tw, _) = text_size(scale, font, &display);
        let x = ((KEY_IMAGE_SIZE as i32 - tw as i32) / 2).max(2);
        // Y kept small: text sits in the top band so the icon area
        // below stays clean.
        draw_text_mut(img, fg, x, 4, scale, font, &display);
    }

    // -- per-kind decoration --------------------------------------------
    let icon_cy = 38_i32; // vertical centre of icon area
    match kind {
        TileKind::Chaser => draw_play_glyph(img, icon_cy, fg),
        TileKind::Movement => draw_movement_glyph(img, icon_cy, fg, active, phase),
        TileKind::Scene => draw_scene_glyph(img, icon_cy, fg),
        TileKind::Blind => draw_eye_glyph(img, icon_cy, fg, active, phase),
        TileKind::Blackout => draw_bolt_glyph(img, icon_cy, fg),
        TileKind::Tap => draw_tap_glyph(img, icon_cy, fg, active, phase),
        TileKind::BpmToggle => draw_metronome_glyph(img, icon_cy, fg, active, phase),
    }

    // -- chaser live RGB strip ------------------------------------------
    // Only chasers when active carry slot data. Eight equal stripes
    // across the bottom, mirror of the Launchpad's top-row CCs.
    if let (TileKind::Chaser, true, Some(slots)) = (kind, active, slots) {
        let strip_h = 12_u32;
        let strip_top = (KEY_IMAGE_SIZE - strip_h - 2) as i32;
        let slot_w = KEY_IMAGE_SIZE / 8; // 9 px on a 72 px key
        for (i, (r, g, b)) in slots.iter().enumerate() {
            let x0 = i as i32 * slot_w as i32;
            draw_filled_rect_mut(
                img,
                Rect::at(x0, strip_top).of_size(slot_w, strip_h),
                Rgb([*r, *g, *b]),
            );
        }
        // Hairline under the strip so it reads as a separate UI element
        // and not background bleed.
        draw_line_segment_mut(
            img,
            (0.0, (KEY_IMAGE_SIZE - 1) as f32),
            ((KEY_IMAGE_SIZE - 1) as f32, (KEY_IMAGE_SIZE - 1) as f32),
            Rgb([0, 0, 0]),
        );
    }
}

// ---- glyphs ------------------------------------------------------------

fn draw_play_glyph(img: &mut RgbImage, cy: i32, fg: Rgb<u8>) {
    // Right-pointing triangle, ~18 px wide / 18 px tall, centred on cy.
    let cx = KEY_IMAGE_SIZE as i32 / 2;
    let half = 9_i32;
    let pts = [
        Point::new(cx - half, cy - half),
        Point::new(cx - half, cy + half),
        Point::new(cx + half, cy),
    ];
    draw_polygon_mut(img, &pts, fg);
}

fn draw_movement_glyph(img: &mut RgbImage, cy: i32, fg: Rgb<u8>, active: bool, phase: u32) {
    // Hollow circle (the "orbit") plus a filled dot positioned around it
    // by `phase` — looks like a slow rotation when active. Idle just
    // draws the outline + a static dot at 12 o'clock so the tile still
    // reads as a movement.
    let cx = KEY_IMAGE_SIZE as i32 / 2;
    let radius = 12_i32;
    // Hollow circle: fake it with `draw_filled_circle_mut` of inner
    // radius then re-fill the centre with the background, but easier
    // to just stroke a thin ring by drawing two filled circles in
    // contrast colours.
    draw_filled_circle_mut(img, (cx, cy), radius, fg);
    // Read background colour underneath to "punch out" the centre.
    let inner_color = img.get_pixel(0, 0).0;
    draw_filled_circle_mut(img, (cx, cy), radius - 2, Rgb(inner_color));
    // Orbiting dot.
    let cycle = 12_u32; // 1.2 s per orbit at 10 fps
    let t = if active { phase % cycle } else { 0 } as f32 / cycle as f32;
    let angle = t * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
    let dx = (radius as f32 * angle.cos()) as i32;
    let dy = (radius as f32 * angle.sin()) as i32;
    draw_filled_circle_mut(img, (cx + dx, cy + dy), 4, fg);
}

fn draw_scene_glyph(img: &mut RgbImage, cy: i32, fg: Rgb<u8>) {
    // Stylised "stage" — three vertical bars descending in height like
    // theatre curtains. Reads as "scene" without needing a literal
    // film-strip icon.
    let cx = KEY_IMAGE_SIZE as i32 / 2;
    let bar_w = 4_i32;
    let gap = 2_i32;
    let heights = [10_i32, 16, 12];
    for (i, h) in heights.iter().enumerate() {
        let x = cx - (bar_w * 3 + gap * 2) / 2 + i as i32 * (bar_w + gap);
        draw_filled_rect_mut(
            img,
            Rect::at(x, cy + 8 - h).of_size(bar_w as u32, *h as u32),
            fg,
        );
    }
}

fn draw_eye_glyph(img: &mut RgbImage, cy: i32, fg: Rgb<u8>, active: bool, phase: u32) {
    // Almond eye outline + filled pupil. Active state blinks the pupil
    // every other frame for an extra "look out" beat.
    let cx = KEY_IMAGE_SIZE as i32 / 2;
    // Eye lens: filled ellipse approximated with two stacked filled
    // circles trimmed to a horizontal band — but easier: a wide rect
    // with rounded ends approximated by two circles.
    draw_filled_circle_mut(img, (cx - 8, cy), 7, fg);
    draw_filled_circle_mut(img, (cx + 8, cy), 7, fg);
    draw_filled_rect_mut(img, Rect::at(cx - 8, cy - 7).of_size(16, 14), fg);
    // Pupil.
    let bg = img.get_pixel(0, 0).0;
    let pupil_visible = !active || phase % 2 == 0;
    if pupil_visible {
        draw_filled_circle_mut(img, (cx, cy), 4, Rgb(bg));
        draw_filled_circle_mut(img, (cx, cy), 2, fg);
    } else {
        // "Blink": the eye closes — just a horizontal bar.
        draw_filled_rect_mut(img, Rect::at(cx - 12, cy - 1).of_size(24, 3), Rgb(bg));
    }
}

fn draw_tap_glyph(img: &mut RgbImage, cy: i32, fg: Rgb<u8>, active: bool, phase: u32) {
    // Concentric "ripple" rings — like a finger has just tapped a
    // surface. When active, the rings expand-and-fade with `phase` to
    // suggest the next beat is incoming. We draw at most three rings;
    // the ones that have "expanded" past the key edge are clipped by
    // the image bounds.
    let cx = KEY_IMAGE_SIZE as i32 / 2;
    // Animation cycle: 8 frames at 10fps = 0.8s per ripple sequence,
    // close enough to the standard 1-2 Hz tap motion.
    let cycle = 8_u32;
    let t = (phase % cycle) as f32 / cycle as f32;
    // Three concentric rings, each offset by 1/3 of the cycle.
    for i in 0..3 {
        let local_t = (t + i as f32 / 3.0) % 1.0;
        let radius = (4.0 + local_t * 18.0) as i32;
        // Approximate stroke by drawing a filled outer disc and a
        // "punched-out" inner disc the same colour as the background.
        if active {
            draw_filled_circle_mut(img, (cx, cy), radius, fg);
            let bg = img.get_pixel(0, 0).0;
            draw_filled_circle_mut(img, (cx, cy), (radius - 2).max(0), Rgb(bg));
        }
    }
    // Static centre dot — always visible so the key reads as TAP-ish
    // even when the rings are expanded off-screen.
    draw_filled_circle_mut(img, (cx, cy), 4, fg);
}

fn draw_metronome_glyph(img: &mut RgbImage, cy: i32, fg: Rgb<u8>, active: bool, phase: u32) {
    // Trapezoid body (the metronome housing) with a swinging arm. Arm
    // angle driven by `phase` when active; static at centre when idle.
    let cx = KEY_IMAGE_SIZE as i32 / 2;
    // Body — a flat-topped trapezoid drawn as a polygon.
    let body = [
        imageproc::point::Point::new(cx - 10, cy + 12),
        imageproc::point::Point::new(cx + 10, cy + 12),
        imageproc::point::Point::new(cx + 6, cy - 12),
        imageproc::point::Point::new(cx - 6, cy - 12),
    ];
    imageproc::drawing::draw_polygon_mut(img, &body, fg);
    // Hollow inside so it doesn't look like a solid block.
    let bg = img.get_pixel(0, 0).0;
    let inner = [
        imageproc::point::Point::new(cx - 8, cy + 10),
        imageproc::point::Point::new(cx + 8, cy + 10),
        imageproc::point::Point::new(cx + 4, cy - 10),
        imageproc::point::Point::new(cx - 4, cy - 10),
    ];
    imageproc::drawing::draw_polygon_mut(img, &inner, Rgb(bg));
    // Arm: pivot at body bottom-centre, swings ±25° at metronome rate.
    let cycle = 10_u32; // 1.0 s/cycle at 10 fps ≈ 60 BPM swing — close enough
    let pivot = (cx, cy + 11);
    let angle = if active {
        let t = pulse(phase, cycle);  // 0..1 sine
        (t - 0.5) * 2.0 * std::f32::consts::FRAC_PI_4 // ±π/4
    } else {
        0.0 // idle: arm dead-centre
    };
    let arm_len = 22_f32;
    let tip = (
        pivot.0 + (arm_len * angle.sin()) as i32,
        pivot.1 - (arm_len * angle.cos()) as i32,
    );
    draw_line_segment_mut(
        img,
        (pivot.0 as f32, pivot.1 as f32),
        (tip.0 as f32, tip.1 as f32),
        fg,
    );
    // Counterweight bead near the tip.
    draw_filled_circle_mut(img, tip, 3, fg);
}

fn draw_bolt_glyph(img: &mut RgbImage, cy: i32, fg: Rgb<u8>) {
    // Lightning bolt as two stacked rotated rectangles that share an
    // edge near the centre. Drawn as a polygon so the diagonals are
    // crisp.
    let cx = KEY_IMAGE_SIZE as i32 / 2;
    let pts = [
        Point::new(cx + 2, cy - 14),
        Point::new(cx + 8, cy - 14),
        Point::new(cx - 2, cy + 1),
        Point::new(cx + 4, cy + 1),
        Point::new(cx - 6, cy + 16),
        Point::new(cx, cy + 3),
        Point::new(cx - 6, cy + 3),
    ];
    draw_polygon_mut(img, &pts, fg);
}

// ---- helpers -----------------------------------------------------------

fn fill(img: &mut RgbImage, color: Rgb<u8>) {
    let (w, h) = img.dimensions();
    draw_filled_rect_mut(img, Rect::at(0, 0).of_size(w, h), color);
}

/// Sine-based 0..1 pulse over `period_frames` frames.
fn pulse(phase: u32, period_frames: u32) -> f32 {
    let t = (phase % period_frames) as f32 / period_frames as f32;
    (1.0 + (t * std::f32::consts::TAU).sin()) / 2.0
}

/// Lerp two RGB colours by `t` ∈ [0,1].
fn mix_color(a: (u8, u8, u8), b: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
    let lerp = |x: u8, y: u8| {
        ((x as f32 * (1.0 - t)) + (y as f32 * t)).clamp(0.0, 255.0) as u8
    };
    (lerp(a.0, b.0), lerp(a.1, b.1), lerp(a.2, b.2))
}

fn dim(c: (u8, u8, u8), factor: f32) -> (u8, u8, u8) {
    (
        (c.0 as f32 * factor).clamp(0.0, 255.0) as u8,
        (c.1 as f32 * factor).clamp(0.0, 255.0) as u8,
        (c.2 as f32 * factor).clamp(0.0, 255.0) as u8,
    )
}

fn brighten(c: (u8, u8, u8), add: u8) -> (u8, u8, u8) {
    let b = |x: u8| ((x as u16) + add as u16).min(255) as u8;
    (b(c.0), b(c.1), b(c.2))
}

fn pick_scale(font: &FontRef<'static>, text: &str, max_w: u32) -> PxScale {
    for px in [20.0_f32, 16.0, 13.0, 11.0] {
        let scale = PxScale { x: px, y: px };
        let (w, _) = text_size(scale, font, text);
        if w <= max_w {
            return scale;
        }
    }
    PxScale { x: 10.0, y: 10.0 }
}

fn truncate_label(s: &str, max_chars: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_chars {
        s.to_string()
    } else {
        chars[..max_chars].iter().collect()
    }
}
