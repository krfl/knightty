use crate::fx::Particle;
use crate::physics::{WALL_BUMP_AMP, WALL_BUMP_N};
use crate::render::draw;
use crate::render::framebuffer::{Color, Framebuffer};
use crate::render::iso::{self, Iso};
use crate::render::terminal::PX_PER_COL;
use crate::world::arena;
use crate::world::combatant::Combatant;
use crate::world::hazard::{Difficulty, Hazard, HazardKind};

pub const WOOD_DARK: Color = Color::new(86, 56, 30);
pub const WOOD_LIGHT: Color = Color::new(140, 95, 55);
pub const WOOD_GRAIN: Color = Color::new(110, 75, 40);
pub const ARENA_BORDER: Color = Color::new(60, 38, 18);
pub const SHADOW: Color = Color::new(54, 44, 50);
pub const LANCE_WOOD: Color = Color::new(220, 200, 150);
pub const LANCE_TIP: Color = Color::new(140, 140, 150);
pub const TEXT_COLOR: Color = Color::new(245, 235, 215);
pub const TEXT_DIM: Color = Color::new(150, 130, 100);
pub const HUD_BG: Color = Color::new(28, 18, 10);
pub const PIT_RIM: Color = Color::new(40, 25, 12);
pub const PIT_DARK: Color = Color::new(8, 4, 2);
pub const BUMPER_BODY: Color = Color::new(220, 180, 70);
pub const BUMPER_RIM: Color = Color::new(120, 80, 30);
pub const BANNER_RED: Color = Color::new(170, 38, 35);
pub const BANNER_WHITE: Color = Color::new(235, 230, 215);
pub const BANNER_POLE: Color = Color::new(70, 45, 25);
pub const BANNER_FINIAL: Color = Color::new(215, 175, 60);

const N_BANNERS: usize = 8;

/// Ground shadow half width under a knight, in world units.
const SHADOW_HALF_W: f32 = 2.8;

/// UI text scale: glyph width (3 or 5) and cell multiplier (each glyph pixel
/// = `cell_scale × cell_scale` fb pixels). Picked from the terminal cell
/// size so the UI takes about the same share of the screen at any font size.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct UiScale {
    pub glyph_w: u8,
    pub cell_scale: u8,
}

impl UiScale {
    pub const SMALLEST: UiScale = UiScale { glyph_w: 3, cell_scale: 1 };

    /// Glyph height in fb pixels (always 5 rows in our font).
    pub fn line_height(&self) -> i32 {
        5 * self.cell_scale as i32
    }

    /// Width in fb pixels of `text` at this scale.
    pub fn text_width(&self, text: &str) -> i32 {
        draw::text_bitmap_width(text, self.glyph_w, self.cell_scale)
    }

    /// The next smaller scale, or `None` at 3×5 scale 1. The order is 5×5
    /// then 3×5 at each cell scale, so 3×5 at scale 2 (taller) comes before
    /// 5×5 at scale 1.
    pub fn smaller(&self) -> Option<UiScale> {
        match (self.glyph_w, self.cell_scale) {
            (_, 0) | (3, 1) => None,
            (5, cs) => Some(UiScale { glyph_w: 3, cell_scale: cs }),
            (_, cs) => Some(UiScale { glyph_w: 5, cell_scale: cs - 1 }),
        }
    }

    /// This scale followed by every smaller one, down to 3×5 scale 1.
    pub fn ladder(&self) -> Vec<UiScale> {
        let mut out = vec![*self];
        while let Some(next) = out.last().and_then(|s| s.smaller()) {
            out.push(next);
        }
        out
    }

    /// One tier smaller, used for secondary text under a heading.
    pub fn sub(&self) -> UiScale {
        if self.cell_scale > 1 {
            UiScale { glyph_w: self.glyph_w, cell_scale: self.cell_scale - 1 }
        } else {
            UiScale::SMALLEST
        }
    }
}

/// Target pixel height for the menu/title text. The scale is chosen to hit
/// roughly this many on-screen pixels regardless of the terminal's font size.
const TARGET_TITLE_PX: u32 = 64;

pub fn pick_ui_scale(term_cols: u16, term_rows: u16, cell_h_px: u16) -> UiScale {
    // Below the small-terminal threshold, drop to 3×5 so menu/HUD still fits.
    // There's no plain-text fallback; anything too tiny for the bitmap font
    // shows the "resize terminal" splash instead.
    if term_cols < 80 || term_rows < 28 {
        return UiScale::SMALLEST;
    }

    // Pick `cell_scale` to hit `TARGET_TITLE_PX` of glyph height in absolute
    // screen pixels. At cell_h=8 (font 4) this lands on ~2; at cell_h=15
    // (font 12) it lands on 1; at cell_h=4 (very small fonts) it lands on 4.
    // Same physical readability across font sizes.
    let glyph_h_at_1 = 5u32 * cell_h_px.max(4) as u32;
    let from_target = TARGET_TITLE_PX.div_ceil(glyph_h_at_1).max(1) as u8;

    // Cap by terminal cell counts so the menu stack (knight, title, three
    // items and the footer) fits: it needs roughly 80 columns and 30 rows
    // per scale step.
    let h_cap = ((term_cols as u32) / 80).max(1) as u8;
    let v_cap = ((term_rows as u32) / 30).max(1) as u8;

    UiScale {
        glyph_w: 5,
        cell_scale: from_target.min(h_cap).min(v_cap).clamp(1, 4),
    }
}

/// Everything about the screen that is decided once per frame: text scales,
/// where the arena area starts and the pixel aspect correction.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Layout {
    /// Scale for the menu and the overlays.
    pub ui_scale: UiScale,
    /// Scale for the HUD bar, the round banner and the tilt indicator spacing.
    pub hud_scale: UiScale,
    /// First framebuffer row below the HUD bar and its rule.
    pub top_px: i32,
    /// Vertical pixel aspect correction for the iso projection.
    pub y_scale: f32,
}

impl Layout {
    /// `cell_px` is the terminal cell size in screen pixels (width, height).
    pub fn new(term_cols: u16, term_rows: u16, cell_px: (u16, u16), hud: &HudView) -> Self {
        let fb_w = term_cols.max(1) as i32 * PX_PER_COL as i32;
        let ui_scale = pick_ui_scale(term_cols, term_rows, cell_px.1);
        // The HUD tops out at cell scale 2 so it doesn't dominate the
        // screen at very wide terminals.
        let preferred = UiScale {
            glyph_w: ui_scale.glyph_w,
            cell_scale: ui_scale.cell_scale.min(2),
        };
        let hud_scale = pick_hud_scale(hud, preferred, fb_w);
        Layout {
            ui_scale,
            hud_scale,
            top_px: hud_bar_height(hud_scale) + 1,
            y_scale: iso::pixel_y_scale(cell_px.0, cell_px.1),
        }
    }
}

fn banner_angle(i: usize) -> f32 {
    i as f32 / N_BANNERS as f32 * std::f32::consts::TAU + 0.20
}

pub struct CombatantPalette {
    pub horse: Color,
    pub horse_dark: Color,
    pub stripe: Color,
    pub knight: Color,
    pub helmet: Color,
    pub plume: Color,
    pub shield: Color,
    pub shield_accent: Color,
}

pub const PLAYER_PALETTE: CombatantPalette = CombatantPalette {
    horse: Color::new(170, 130, 90),
    horse_dark: Color::new(80, 55, 30),
    stripe: Color::new(60, 110, 200),
    knight: Color::new(40, 75, 150),
    helmet: Color::new(210, 215, 225),
    plume: Color::new(150, 195, 235),
    shield: Color::new(60, 110, 200),
    shield_accent: Color::new(230, 230, 235),
};

pub const OPPONENT_PALETTE: CombatantPalette = CombatantPalette {
    horse: Color::new(110, 75, 50),
    horse_dark: Color::new(50, 30, 15),
    stripe: Color::new(180, 60, 50),
    knight: Color::new(135, 35, 30),
    helmet: Color::new(130, 120, 130),
    plume: Color::new(225, 105, 60),
    shield: Color::new(180, 60, 50),
    shield_accent: Color::new(225, 200, 100),
};

pub struct ArenaView<'a> {
    /// Arena side length in world units, fixed for the whole run.
    pub arena_size: f32,
    pub knight: &'a Combatant,
    pub opponent: &'a Combatant,
    pub tilt: (f32, f32),
    pub hazards: &'a [Hazard],
    pub banner: Option<&'a str>,
}

pub struct HudView {
    pub score: u32,
    pub best: u32,
    pub difficulty: Difficulty,
    pub level: u32,
}

/// Cache key for the static arena layer: arena size, framebuffer size, top of
/// the arena area and the pixel aspect (as raw float bits).
type BackgroundKey = (u32, u32, u32, i32, u32);

/// Cached static arena layer: the wood floor and the bumpy border ring. Both
/// depend only on the arena size and the screen layout, so they are rendered
/// once and blitted each frame instead of recomputing the per-pixel wood
/// texture and the border every time.
pub struct ArenaBackground {
    buf: Framebuffer,
    key: Option<BackgroundKey>,
}

impl ArenaBackground {
    pub fn new() -> Self {
        Self {
            buf: Framebuffer::new(1, 1),
            key: None,
        }
    }
}

impl Default for ArenaBackground {
    fn default() -> Self {
        Self::new()
    }
}

pub fn draw_background(fb: &mut Framebuffer) {
    fb.clear(HUD_BG);
}

/// Height in framebuffer pixels of the HUD bar drawn by `draw_hud` at a given
/// scale, not counting the 1 px rule under it.
pub fn hud_bar_height(scale: UiScale) -> i32 {
    let pad_y = (scale.cell_scale as i32).max(1);
    scale.line_height() + pad_y * 2
}

/// Background, arena and HUD for one frame of play. Returns the projection
/// used so overlays and particles can line up with the arena.
pub fn draw_scene(
    fb: &mut Framebuffer,
    bg: &mut ArenaBackground,
    arena: &ArenaView,
    hud: &HudView,
    layout: &Layout,
) -> Iso {
    draw_background(fb);
    let iso = draw_arena(fb, bg, arena, layout);
    draw_hud(fb, hud, layout.hud_scale);
    iso
}

/// The projection for an arena of `arena_size` in this framebuffer.
pub fn arena_iso(fb: &Framebuffer, arena_size: f32, layout: &Layout) -> Iso {
    iso::fit_arena(
        arena_size,
        fb.width as i32,
        fb.height as i32,
        layout.top_px,
        layout.y_scale,
    )
}

/// Render the static floor + border into `fb` (already cleared to `HUD_BG`).
/// Frame-invariant for a given iso and arena size, so `draw_arena` caches it.
pub(crate) fn draw_arena_floor(fb: &mut Framebuffer, iso: &Iso, center: (f32, f32), arena_r: f32) {
    let (cx, cy) = center;
    let s = arena_r * 2.0;
    // Below about one pixel per world unit the grain flecks alias into
    // diagonal bands, so small arenas only get the row gradient.
    let grain = iso.scale >= 1.0;

    // Floor with wood texture in world coords, filling the same bumpy curve
    // the border traces.
    draw::fill_iso_bumpy_disc(fb, iso, (cx, cy), arena_r, WALL_BUMP_AMP, WALL_BUMP_N, |wx, wy| {
        let t = (wy / s * 7.0).sin() * 0.5 + 0.5;
        let row = WOOD_DARK.lerp(WOOD_LIGHT, t * 0.4 + 0.3);
        if grain {
            let n = (((wx * 3.0) as i32 + (wy * 5.0) as i32).rem_euclid(13)) as f32 / 13.0;
            if n > 0.72 {
                return WOOD_GRAIN;
            }
        }
        row
    });

    // Bumpy boundary traced by the polar curve `r(θ) = R + amp * sin(N*θ)`.
    // Enough segments that each chord stays within a fraction of a pixel of
    // the curve at large scales.
    let rim_px = std::f32::consts::TAU * (arena_r + WALL_BUMP_AMP) * iso.scale * 1.5;
    let segments = ((rim_px / 3.0).ceil() as usize).max(144);
    let mut prev: Option<(f32, f32)> = None;
    for i in 0..=segments {
        let theta = i as f32 / segments as f32 * std::f32::consts::TAU;
        let r = arena_r + WALL_BUMP_AMP * (WALL_BUMP_N * theta).sin();
        let pt = iso.project((cx + theta.cos() * r, cy + theta.sin() * r, 0.0));
        if let Some(p) = prev {
            draw::tapered_line(fb, p.0, p.1, pt.0, pt.1, 2.0, 2.0, ARENA_BORDER);
        }
        prev = Some(pt);
    }
}

/// Something standing on the floor that has to be drawn back to front.
enum Prop<'a> {
    Bumper(&'a Hazard),
    Fighter(&'a Combatant, &'static CombatantPalette),
}

pub fn draw_arena(
    fb: &mut Framebuffer,
    bg: &mut ArenaBackground,
    view: &ArenaView,
    layout: &Layout,
) -> Iso {
    let s = view.arena_size;
    let iso = arena_iso(fb, s, layout);
    let cx = s * 0.5;
    let cy = s * 0.5;
    let arena_r = s * 0.5;

    // Rebuild the static floor+border layer only when the arena size or the
    // screen layout changes; otherwise reuse the cached pixels.
    let key = (
        s.to_bits(),
        fb.width,
        fb.height,
        layout.top_px,
        layout.y_scale.to_bits(),
    );
    if bg.key != Some(key) {
        bg.buf.resize(fb.width, fb.height);
        bg.buf.clear(HUD_BG);
        draw_arena_floor(&mut bg.buf, &iso, (cx, cy), arena_r);
        bg.key = Some(key);
    }
    if fb.color.len() == bg.buf.color.len() {
        fb.color.copy_from_slice(&bg.buf.color);
    }

    // Pits are holes in the floor, so they go down before anything stands
    // on it.
    for h in view.hazards {
        if matches!(h.kind, HazardKind::Pit) {
            draw_iso_ground_disc(fb, &iso, h.pos, h.radius, PIT_RIM);
            draw_iso_ground_disc(fb, &iso, h.pos, h.radius * 0.85, PIT_DARK);
        }
    }

    // Back-half banners stand behind everything on the floor.
    draw_banners(fb, &iso, (cx, cy), arena_r, false);

    // All ground shadows before any sprite, so one knight's shadow never
    // paints over the other knight.
    draw_shadow(fb, view.knight, &iso);
    draw_shadow(fb, view.opponent, &iso);

    // Bumpers and knights sorted back to front by `x + y`. The sort is
    // stable, so the knight wins ties against the opponent.
    let mut props: Vec<(f32, Prop)> = view
        .hazards
        .iter()
        .filter(|h| matches!(h.kind, HazardKind::Bumper))
        .map(|h| (h.pos.0 + h.pos.1, Prop::Bumper(h)))
        .collect();
    props.push((
        view.knight.pos.0 + view.knight.pos.1,
        Prop::Fighter(view.knight, &PLAYER_PALETTE),
    ));
    props.push((
        view.opponent.pos.0 + view.opponent.pos.1,
        Prop::Fighter(view.opponent, &OPPONENT_PALETTE),
    ));
    props.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (_, prop) in &props {
        match prop {
            Prop::Bumper(h) => draw_iso_bumper(fb, &iso, h.pos, h.radius),
            Prop::Fighter(c, palette) => draw_combatant(fb, c, &iso, palette),
        }
    }

    // Front-half banners, drawn after combatants.
    draw_banners(fb, &iso, (cx, cy), arena_r, true);

    // Tilt indicator anchored below the HUD bar, with a small gap so the
    // outer ring is fully visible at every font scale.
    let ind_radius = 5.0_f32;
    let ind_cx = 9.0;
    let ind_cy = (layout.top_px + 1) as f32 + ind_radius + 1.0;
    draw::filled_circle(fb, ind_cx, ind_cy, ind_radius, ARENA_BORDER);
    draw::filled_circle(
        fb,
        ind_cx + view.tilt.0 * 4.0,
        ind_cy + view.tilt.1 * 4.0,
        2.0,
        TEXT_COLOR,
    );

    if let Some(msg) = view.banner {
        draw_round_banner(fb, msg, layout);
    }

    iso
}

/// Short notice between rounds ("DEFEATED", "PIT!"), centred just below the
/// HUD bar at the HUD scale.
fn draw_round_banner(fb: &mut Framebuffer, msg: &str, layout: &Layout) {
    let scale = layout.hud_scale;
    let fw = fb.width as i32;
    let pad = scale.cell_scale as i32 + 1;
    let text_w = scale.text_width(msg);
    let box_w = text_w + pad * 2 + 2;
    let box_h = scale.line_height() + pad * 2 + 2;
    let box_x = (fw - box_w) / 2;
    let box_y = layout.top_px + 2;
    draw::fill_rect(fb, box_x, box_y, box_w, box_h, ARENA_BORDER);
    draw::fill_rect(fb, box_x + 1, box_y + 1, box_w - 2, box_h - 2, HUD_BG);
    draw::draw_text_bitmap(
        fb,
        box_x + 1 + pad,
        box_y + 1 + pad,
        msg,
        TEXT_COLOR,
        scale.glyph_w,
        scale.cell_scale,
    );
}

/// Firework particles, drawn last so the bursts show over the victory scroll.
pub fn draw_particles(fb: &mut Framebuffer, iso: &Iso, particles: &[Particle]) {
    for p in particles {
        let life_frac = (1.0 - p.age / p.lifetime.max(0.001)).clamp(0.0, 1.0);
        let (sx, sy) = iso.project(p.pos);
        let r = (0.7 * iso.scale * (0.5 + 0.5 * life_frac)).max(0.6);
        let color = p.color.scale(life_frac);
        draw::filled_circle(fb, sx, sy, r, color);
    }
}

fn draw_iso_ground_disc(
    fb: &mut Framebuffer,
    iso: &Iso,
    world_xy: (f32, f32),
    world_r: f32,
    color: Color,
) {
    let (sx, sy) = iso.project2(world_xy);
    let (half_w, half_h) = iso.ground_ellipse(world_r);
    draw::fill_axis_ellipse(fb, sx, sy, half_w, half_h, color);
}

pub(crate) fn draw_banners(
    fb: &mut Framebuffer,
    iso: &Iso,
    center: (f32, f32),
    arena_r: f32,
    front_half: bool,
) {
    let pole_h = arena::banner_pole_height(arena_r);
    let fabric_h = pole_h * 0.65;
    let fabric_w = (arena_r * 0.10).max(3.5);

    for i in 0..N_BANNERS {
        let theta = banner_angle(i);
        let depth_proxy = theta.cos() + theta.sin();
        let is_front = depth_proxy > -0.001;
        if is_front != front_half {
            continue;
        }
        let anchor = (
            center.0 + theta.cos() * arena_r,
            center.1 + theta.sin() * arena_r,
        );
        let tangent = (-theta.sin(), theta.cos());
        draw_one_banner(fb, iso, anchor, tangent, pole_h, fabric_h, fabric_w);
    }
}

fn draw_one_banner(
    fb: &mut Framebuffer,
    iso: &Iso,
    anchor: (f32, f32),
    tangent: (f32, f32),
    pole_h: f32,
    fabric_h: f32,
    fabric_w: f32,
) {
    let pole_bot = iso.project((anchor.0, anchor.1, 0.0));
    let pole_top = iso.project((anchor.0, anchor.1, pole_h));
    draw::tapered_line(fb, pole_bot.0, pole_bot.1, pole_top.0, pole_top.1, 1.6, 1.3, BANNER_POLE);

    let finial = iso.project((anchor.0, anchor.1, pole_h + arena::BANNER_FINIAL_UP));
    draw::filled_circle(fb, finial.0, finial.1, arena::BANNER_FINIAL_RADIUS_PX, BANNER_FINIAL);

    let half_w = fabric_w * 0.5;
    let cl_w = (anchor.0 + tangent.0 * half_w, anchor.1 + tangent.1 * half_w);
    let cr_w = (anchor.0 - tangent.0 * half_w, anchor.1 - tangent.1 * half_w);
    let cl_top = iso.project((cl_w.0, cl_w.1, pole_h));
    let cr_top = iso.project((cr_w.0, cr_w.1, pole_h));
    draw::tapered_line(fb, cl_top.0, cl_top.1, cr_top.0, cr_top.1, 0.9, 0.9, BANNER_POLE);

    let bot_z = pole_h - fabric_h;
    let cl_bot = iso.project((cl_w.0, cl_w.1, bot_z));
    let cr_bot = iso.project((cr_w.0, cr_w.1, bot_z));

    let xmin = cl_top.0.min(cr_top.0);
    let xmax = cl_top.0.max(cr_top.0);
    let dx = cr_top.0 - cl_top.0;
    if dx.abs() < 0.5 {
        return;
    }

    // Stripe count follows the fabric height on screen, odd so both ends
    // are red. Short banners get one or three clean stripes instead of noise.
    // An even count rounds up while every stripe still gets a full pixel.
    let fabric_px = fabric_h * 0.5 * iso.scale * iso.y_scale;
    let mut stripes = ((fabric_px / 1.5).floor() as i32).clamp(1, 5);
    if stripes % 2 == 0 {
        stripes += if fabric_px / (stripes + 1) as f32 >= 1.0 { 1 } else { -1 };
    }

    for x_i in (xmin.floor() as i32)..=(xmax.ceil() as i32) {
        let alpha = ((x_i as f32 + 0.5 - cl_top.0) / dx).clamp(0.0, 1.0);
        let top_y = cl_top.1 + (cr_top.1 - cl_top.1) * alpha;
        let bot_y = cl_bot.1 + (cr_bot.1 - cl_bot.1) * alpha;
        let span = bot_y - top_y;
        if span < 1.0 {
            continue;
        }
        for y_i in (top_y.floor() as i32)..=(bot_y.ceil() as i32) {
            let v = ((y_i as f32 + 0.5 - top_y) / span).clamp(0.0, 1.0);
            let stripe = ((v * stripes as f32).floor() as i32).min(stripes - 1);
            let color = if stripe % 2 == 0 { BANNER_RED } else { BANNER_WHITE };
            fb.set(x_i, y_i, color);
        }
    }
}

fn draw_iso_bumper(fb: &mut Framebuffer, iso: &Iso, world_xy: (f32, f32), world_r: f32) {
    let height = 2.5_f32;
    let (bx, by) = iso.project((world_xy.0, world_xy.1, 0.0));
    let (tx, ty) = iso.project((world_xy.0, world_xy.1, height));
    let (half_w, half_h) = iso.ground_ellipse(world_r);
    let side_y0 = ty.round() as i32;
    let side_y1 = by.round() as i32;
    if side_y1 > side_y0 {
        let x0 = (bx - half_w).round() as i32;
        let x1 = (bx + half_w).round() as i32;
        draw::fill_rect(fb, x0, side_y0, x1 - x0, side_y1 - side_y0, BUMPER_RIM);
    }
    draw::fill_axis_ellipse(fb, bx, by, half_w, half_h, BUMPER_RIM);
    draw::fill_axis_ellipse(fb, tx, ty, half_w, half_h, BUMPER_BODY);
    draw::fill_axis_ellipse(fb, tx, ty, half_w * 0.42, half_h * 0.42, TEXT_COLOR);
}

/// Pixels per world unit used for knight parts. Never below 1, so knights
/// stay readable on small arenas even though they are then a bit larger than
/// their true footprint.
pub fn sprite_scale(iso: &Iso) -> f32 {
    iso.scale.max(1.0)
}

fn draw_shadow(fb: &mut Framebuffer, c: &Combatant, iso: &Iso) {
    let shrink = 1.0 - (c.z / 12.0).clamp(0.0, 0.5);
    let (shx, shy) = iso.project2(c.pos);
    let half_w = SHADOW_HALF_W * sprite_scale(iso) * shrink * std::f32::consts::SQRT_2;
    draw::fill_axis_ellipse(fb, shx, shy, half_w, half_w * 0.5 * iso.y_scale, SHADOW);
}

pub fn draw_combatant(fb: &mut Framebuffer, c: &Combatant, iso: &Iso, p: &CombatantPalette) {
    let (fx, fy) = c.facing;
    let perp = (-fy, fx);
    // Part sizes use the sprite scale. Offsets from the knight's position are
    // stretched by the same factor so the parts stay in proportion, while the
    // position itself (and the lance tip, which is what kills) stays true.
    let scale = sprite_scale(iso);
    let k = scale / iso.scale.max(0.001);

    let proj = |fwd: f32, lat: f32, up: f32| -> (f32, f32) {
        iso.project((
            c.pos.0 + (fwd * fx + lat * perp.0) * k,
            c.pos.1 + (fwd * fy + lat * perp.1) * k,
            c.z + up * k,
        ))
    };

    let p0 = iso.project((c.pos.0, c.pos.1, c.z + 1.0));
    let p1 = iso.project((c.pos.0 + fx, c.pos.1 + fy, c.z + 1.0));
    let raw = (p1.0 - p0.0, p1.1 - p0.1);
    let raw_len = (raw.0 * raw.0 + raw.1 * raw.1).sqrt().max(0.001);
    let sf = (raw.0 / raw_len, raw.1 / raw_len);

    let belly = 1.1;
    let foot_up = if c.airborne() { 0.4 } else { -c.z / k };
    for &(lf, ll) in &[(-2.4, 0.8), (-2.4, -0.8), (2.0, 0.8), (2.0, -0.8)] {
        let (tx, ty) = proj(lf, ll, belly);
        let (bx, by) = proj(lf, ll, foot_up);
        draw::tapered_line(fb, tx, ty, bx, by, 0.55 * scale, 0.45 * scale, p.horse_dark);
    }

    let (tx, ty) = proj(-3.0, 0.0, 1.6);
    draw::filled_circle(fb, tx, ty, 0.6 * scale, p.horse_dark);

    let (hbx, hby) = proj(0.0, 0.0, 1.6);
    draw::filled_ellipse_oriented(fb, hbx, hby, sf.0, sf.1, 3.0 * scale, 1.4 * scale, p.horse);

    let (cpx, cpy) = proj(-0.2, 0.0, 2.1);
    draw::filled_ellipse_oriented(fb, cpx, cpy, sf.0, sf.1, 2.0 * scale, 1.1 * scale, p.stripe);

    let (n0x, n0y) = proj(2.0, 0.0, 1.9);
    let (n1x, n1y) = proj(2.7, 0.0, 2.4);
    draw::tapered_line(fb, n0x, n0y, n1x, n1y, 1.4 * scale, 1.0 * scale, p.horse);

    let (hdx, hdy) = proj(2.9, 0.0, 2.5);
    draw::filled_ellipse_oriented(fb, hdx, hdy, sf.0, sf.1, 1.2 * scale, 0.7 * scale, p.horse);

    let (mx, my) = proj(2.0, 0.0, 2.7);
    draw::filled_circle(fb, mx, my, 0.55 * scale, p.horse_dark);

    // Rider, shield and lance. The shield and lance hang off the rider's
    // sides, so whichever side faces away from the viewer goes behind the
    // rider. Depth grows with `x + y`, toward the bottom of the screen.
    let depth = |fwd: f32, lat: f32| (fwd * fx + lat * perp.0) + (fwd * fy + lat * perp.1);
    let rider_depth = depth(-0.2, 0.0);
    let shield_behind = depth(0.4, -1.3) < rider_depth;
    let lance_behind = depth(2.0, 0.7) < rider_depth;

    let shield = |fb: &mut Framebuffer| {
        let (s_x, s_y) = proj(0.4, -1.3, 2.9);
        draw::filled_circle(fb, s_x, s_y, 0.85 * scale, p.shield);
        draw::filled_circle(fb, s_x, s_y, 0.4 * scale, p.shield_accent);
    };
    let lance = |fb: &mut Framebuffer| {
        let saddle = proj(0.6, 0.7, 2.7);
        let tip = iso.project((
            c.pos.0 + fx * c.lance_len,
            c.pos.1 + fy * c.lance_len,
            c.z + 2.7 * k,
        ));
        draw::tapered_line(fb, saddle.0, saddle.1, tip.0, tip.1, 1.0 * scale, 0.35 * scale, LANCE_WOOD);
        draw::filled_circle(fb, tip.0, tip.1, 0.5 * scale, LANCE_TIP);
    };

    if shield_behind {
        shield(fb);
    }
    if lance_behind {
        lance(fb);
    }

    let (ktx, kty) = proj(-0.2, 0.0, 3.0);
    draw::filled_circle(fb, ktx, kty, 1.4 * scale, p.knight);

    // The head sits high enough above the torso that some of the body still
    // shows between helmet, shield and lance on the smallest sprites.
    let (pl0x, pl0y) = proj(0.1, 0.0, 4.6);
    let (pl1x, pl1y) = proj(-1.3, 0.0, 5.0);
    draw::tapered_line(fb, pl0x, pl0y, pl1x, pl1y, 0.9 * scale, 0.3 * scale, p.plume);

    let (hex, hey) = proj(0.3, 0.0, 4.6);
    draw::filled_circle(fb, hex, hey, 0.85 * scale, p.helmet);

    if !shield_behind {
        shield(fb);
    }
    if !lance_behind {
        lance(fb);
    }
}

/// Screen rectangle in framebuffer pixels.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    pub fn right(&self) -> i32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }

    /// True when the whole rectangle lies inside a `w` by `h` framebuffer.
    pub fn inside(&self, w: i32, h: i32) -> bool {
        self.x >= 0 && self.y >= 0 && self.right() <= w && self.bottom() <= h
    }

    #[cfg(test)]
    pub fn contains(&self, other: &Rect) -> bool {
        other.x >= self.x
            && other.y >= self.y
            && other.right() <= self.right()
            && other.bottom() <= self.bottom()
    }

    #[cfg(test)]
    pub fn overlaps(&self, other: &Rect) -> bool {
        self.x < other.right()
            && other.x < self.right()
            && self.y < other.bottom()
            && other.y < self.bottom()
    }
}

/// One line of bitmap text placed on screen.
#[derive(Clone, Debug)]
pub struct TextLine {
    pub text: String,
    pub x: i32,
    pub y: i32,
    pub scale: UiScale,
    pub color: Color,
}

impl TextLine {
    /// A line horizontally centred in a framebuffer `fw` pixels wide.
    fn centered(text: &str, y: i32, fw: i32, color: Color, scale: UiScale) -> Self {
        TextLine {
            text: text.to_string(),
            x: (fw - scale.text_width(text)) / 2,
            y,
            scale,
            color,
        }
    }

    /// The pixels the glyphs can cover.
    #[cfg(test)]
    pub fn rect(&self) -> Rect {
        Rect::new(self.x, self.y, self.scale.text_width(&self.text), self.scale.line_height())
    }

    fn draw(&self, fb: &mut Framebuffer) {
        draw::draw_text_bitmap(
            fb,
            self.x,
            self.y,
            &self.text,
            self.color,
            self.scale.glyph_w,
            self.scale.cell_scale,
        );
    }
}

/// The longest of `candidates` no wider than `max_w` at `scale`.
fn longest_fitting<'a>(candidates: &[&'a str], scale: UiScale, max_w: i32) -> Option<&'a str> {
    candidates.iter().copied().find(|c| scale.text_width(c) <= max_w)
}

pub(crate) const MENU_FOOTERS: &[&str] = &[
    "WASD TILT  SPACE JUMP  CTRL-C EXIT",
    "WASD MOVE  SPACE JUMP",
    "WASD  SPACE",
];

/// Where everything on the main menu goes.
pub struct MenuLayout {
    /// Title and the three menu items.
    pub lines: Vec<TextLine>,
    /// Controls hint at the bottom, when it fits.
    pub footer: Option<TextLine>,
    /// Menu knight anchor (horse body centre) and pixel scale, when it fits.
    pub knight: Option<(i32, i32, i32)>,
}

/// Menu knight extents around its anchor at scale 1: left, top, right and
/// bottom in pixels (tail to lance tip, lance tip to hooves).
const MENU_KNIGHT_BOUNDS: (i32, i32, i32, i32) = (-15, -21, 26, 13);

impl MenuLayout {
    #[cfg(test)]
    pub fn knight_rect(&self) -> Option<Rect> {
        self.knight.map(|(cx, cy, s)| {
            let (l, t, r, b) = MENU_KNIGHT_BOUNDS;
            Rect::new(cx + l * s, cy + t * s, (r - l) * s, (b - t) * s)
        })
    }
}

pub fn menu_layout(
    fw: i32,
    fh: i32,
    selection: usize,
    difficulty: Difficulty,
    scale: UiScale,
) -> MenuLayout {
    let title_h = scale.line_height();
    let item_h = scale.line_height();
    // The footer is one tier smaller so it doesn't take much vertical space.
    let sub_scale = scale.sub();
    let sub_h = sub_scale.line_height();
    let margin = 2;

    let footer_text = longest_fitting(MENU_FOOTERS, sub_scale, fw - 4);

    // Try the roomy layout with a footer first, then tighter gaps, then give
    // up the footer. The last option is used even if it overflows, which only
    // happens far below any playable size.
    let mut options = Vec::new();
    for with_footer in [true, false] {
        if with_footer && footer_text.is_none() {
            continue;
        }
        for compact in [false, true] {
            options.push((with_footer, compact));
        }
    }
    let gaps = |compact: bool| {
        if compact {
            (sub_h, (sub_h / 2).max(1), (sub_h / 2).max(2))
        } else {
            (sub_h * 2, sub_h, sub_h)
        }
    };
    let stack_h = |compact: bool| {
        let (title_gap, item_gap, _) = gaps(compact);
        title_h + title_gap + item_h * 3 + item_gap * 2
    };
    let footer_space = |compact: bool| {
        let (_, _, footer_gap) = gaps(compact);
        sub_h + footer_gap + margin
    };
    let &(with_footer, compact) = options
        .iter()
        .find(|&&(f, c)| margin + stack_h(c) + if f { footer_space(c) } else { margin } <= fh)
        .unwrap_or(options.last().unwrap());

    let (title_gap, item_gap, _) = gaps(compact);
    let menu_h = stack_h(compact);
    let footer = if with_footer {
        footer_text.map(|t| TextLine::centered(t, fh - margin - sub_h, fw, TEXT_DIM, sub_scale))
    } else {
        None
    };
    // The title stack is centred in whatever the footer leaves free.
    let region_h = if footer.is_some() { fh - footer_space(compact) } else { fh };

    // Knight backdrop scales with the menu scale. When it doesn't fit it
    // tries a smaller scale before it is hidden.
    let (kl, kt, kr, kb) = MENU_KNIGHT_BOUNDS;
    let knight_gap = sub_h;
    let knight_fits = |ks: i32| {
        ks * (kr - kl) <= fw - 4 && margin + (kb - kt) * ks + knight_gap + menu_h + margin <= region_h
    };
    let knight_scale = if compact {
        None
    } else {
        (1..=(scale.cell_scale as i32).min(2)).rev().find(|&ks| knight_fits(ks))
    };
    let show_knight = knight_scale.is_some();
    let knight_scale = knight_scale.unwrap_or(1);
    let knight_h = (kb - kt) * knight_scale;

    let total_h = if show_knight { knight_h + knight_gap + menu_h } else { menu_h };
    let mut y = ((region_h - total_h) / 2).max(0);

    let mut knight = None;
    if show_knight {
        // Centre the knight's bounding box, not its anchor.
        let cx = fw / 2 - (kl + kr) * knight_scale / 2;
        knight = Some((cx, y - kt * knight_scale, knight_scale));
        y += knight_h + knight_gap;
    }

    let mut lines = vec![TextLine::centered("KNIGHTTY", y, fw, TEXT_COLOR, scale)];
    y += title_h + title_gap;
    let labels: [&str; 3] = ["PLAY", difficulty.label(), "QUIT"];
    for (i, label) in labels.iter().enumerate() {
        let color = if i == selection { TEXT_COLOR } else { TEXT_DIM };
        lines.push(TextLine::centered(label, y, fw, color, scale));
        y += item_h + item_gap;
    }

    MenuLayout { lines, footer, knight }
}

pub fn draw_menu(
    fb: &mut Framebuffer,
    selection: usize,
    difficulty: Difficulty,
    scale: UiScale,
) {
    fb.clear(HUD_BG);
    let layout = menu_layout(fb.width as i32, fb.height as i32, selection, difficulty, scale);
    if let Some((cx, cy, s)) = layout.knight {
        // Mounted knight, profile view, lance forward.
        draw_menu_knight(fb, cx, cy, s);
    }
    for line in layout.lines.iter().chain(layout.footer.iter()) {
        line.draw(fb);
    }
}

/// Procedural side-profile mounted knight, drawn centered at `(cx, cy)` where
/// `(cx, cy)` is the center of the horse's body. `scale` multiplies every
/// pixel offset and radius for proportional growth at bigger menu scales.
fn draw_menu_knight(fb: &mut Framebuffer, cx: i32, cy: i32, scale: i32) {
    let s = scale.max(1);
    let sf = s as f32;
    let p = |dx: i32, dy: i32| ((cx + dx * s) as f32, (cy + dy * s) as f32);
    let pi = |dx: i32, dy: i32| (cx + dx * s, cy + dy * s);

    let horse = Color::new(170, 130, 90);
    let horse_dark = Color::new(80, 55, 30);
    let knight_blue = Color::new(40, 75, 150);
    let stripe_blue = Color::new(60, 110, 200);
    let helmet = Color::new(210, 215, 225);
    let plume = Color::new(150, 195, 235);
    let lance_wood = Color::new(220, 200, 150);
    let lance_tip_color = Color::new(140, 140, 150);
    let shield = Color::new(60, 110, 200);
    let shield_accent = Color::new(230, 230, 235);
    let eye = Color::new(35, 25, 20);

    // Back legs first (they sit behind the body).
    for &x in &[-9, -6] {
        let (lx, ly_top) = p(x, 4);
        let (_, ly_bot) = p(x, 12);
        draw::tapered_line(fb, lx, ly_top, lx, ly_bot, 1.6 * sf, 1.4 * sf, horse_dark);
    }

    // Tail.
    let (tx, ty) = p(-13, -1);
    draw::filled_circle(fb, tx, ty, 2.0 * sf, horse_dark);

    // Horse body: a broad oval at center.
    let (bx, by) = p(0, 0);
    draw::fill_axis_ellipse(fb, bx, by, 12.0 * sf, 5.0 * sf, horse);

    // Caparison (saddle blanket): a blue stripe across the body's upper half.
    draw::fill_axis_ellipse(fb, bx, by - 1.0 * sf, 9.0 * sf, 3.0 * sf, stripe_blue);

    // Front legs (drawn over the body so the silhouette reads correctly).
    for &x in &[6, 9] {
        let (lx, ly_top) = p(x, 4);
        let (_, ly_bot) = p(x, 12);
        draw::tapered_line(fb, lx, ly_top, lx, ly_bot, 1.6 * sf, 1.4 * sf, horse_dark);
    }

    // Neck: a tapered line angled up to the head.
    let (n0x, n0y) = p(9, -3);
    let (n1x, n1y) = p(14, -8);
    draw::tapered_line(fb, n0x, n0y, n1x, n1y, 4.0 * sf, 2.5 * sf, horse);

    // Mane lump.
    let (mx, my) = p(11, -5);
    draw::filled_circle(fb, mx, my, 1.8 * sf, horse_dark);

    // Horse head: a small ellipse pointing forward.
    let (hx, hy) = p(15, -8);
    draw::fill_axis_ellipse(fb, hx, hy, 4.0 * sf, 2.5 * sf, horse);

    // Horse eye.
    let (ex, ey) = p(16, -9);
    draw::filled_circle(fb, ex, ey, (0.6 * sf).max(1.0), eye);

    // Knight torso / cape.
    let (ktx, kty) = p(-1, -7);
    draw::filled_circle(fb, ktx, kty, 4.0 * sf, knight_blue);

    // Helmet.
    let (hex, hey) = p(0, -13);
    draw::filled_circle(fb, hex, hey, 3.0 * sf, helmet);

    // Visor slit across the helmet.
    let (vx, vy) = pi(0, -13);
    let visor_w = 5 * s;
    let visor_h = s.max(1);
    draw::fill_rect(fb, vx - visor_w / 2, vy, visor_w, visor_h, eye);

    // Plume sweeping back from the helmet.
    let (pl0x, pl0y) = p(-1, -14);
    let (pl1x, pl1y) = p(-7, -16);
    draw::tapered_line(fb, pl0x, pl0y, pl1x, pl1y, 3.0 * sf, 1.0 * sf, plume);

    // Lance, angled forward and up, with a metal tip.
    let (l0x, l0y) = p(2, -8);
    let (l1x, l1y) = p(24, -19);
    draw::tapered_line(fb, l0x, l0y, l1x, l1y, 2.5 * sf, 0.9 * sf, lance_wood);
    draw::filled_circle(fb, l1x, l1y, 1.2 * sf, lance_tip_color);

    // Shield over the rider's front.
    let (shx, shy) = p(4, -7);
    draw::filled_circle(fb, shx, shy, 2.5 * sf, shield);
    draw::filled_circle(fb, shx, shy, 1.2 * sf, shield_accent);
}

/// HUD text variants, longest first.
fn hud_candidates(view: &HudView) -> [String; 5] {
    [
        format!(
            "LV {}  SCR {}  BST {}  {}",
            view.level, view.score, view.best, view.difficulty.label()
        ),
        format!(
            "LV {}  SCR {}  {}",
            view.level, view.score, view.difficulty.label()
        ),
        format!("LV {}  SCR {}  BST {}", view.level, view.score, view.best),
        format!("LV {}  SCR {}", view.level, view.score),
        format!("L{} S{}", view.level, view.score),
    ]
}

fn hud_text_room(fw: i32, scale: UiScale) -> i32 {
    fw - 4 * scale.cell_scale as i32
}

/// Largest scale, from `preferred` down to 3×5, at which the full HUD line
/// fits. Only when even 3×5 is too wide does `draw_hud` start dropping
/// fields.
pub fn pick_hud_scale(view: &HudView, preferred: UiScale, fw: i32) -> UiScale {
    let full = &hud_candidates(view)[0];
    preferred
        .ladder()
        .into_iter()
        .find(|s| s.text_width(full) <= hud_text_room(fw, *s))
        .unwrap_or(UiScale::SMALLEST)
}

pub fn draw_hud(fb: &mut Framebuffer, view: &HudView, scale: UiScale) {
    let fw = fb.width as i32;
    let pad_y = (scale.cell_scale as i32).max(1);
    let bar_h = hud_bar_height(scale);
    draw::fill_rect(fb, 0, 0, fw, bar_h, HUD_BG);
    draw::fill_rect(fb, 0, bar_h, fw, 1, ARENA_BORDER);

    // Pick the longest variant that fits, falling back to the shortest.
    let candidates = hud_candidates(view);
    let refs: Vec<&str> = candidates.iter().map(String::as_str).collect();
    let text = longest_fitting(&refs, scale, hud_text_room(fw, scale))
        .unwrap_or(refs[refs.len() - 1]);
    TextLine::centered(text, pad_y, fw, TEXT_COLOR, scale).draw(fb);
}

pub fn draw_below_min(fb: &mut Framebuffer) {
    fb.clear(HUD_BG);
    let fw = fb.width as i32;
    let fh = fb.height as i32;
    let scale = UiScale::SMALLEST;
    let lines = ["RESIZE TERMINAL", "TO PLAY"];
    let total = scale.line_height() * lines.len() as i32 + (lines.len() as i32 - 1) * 2;
    let mut y = (fh - total) / 2;
    for line in &lines {
        TextLine::centered(line, y, fw, TEXT_COLOR, scale).draw(fb);
        y += scale.line_height() + 2;
    }
}

/// A framed box of centred text lines.
pub struct BoxLayout {
    /// Outer edge of the box, border included.
    pub rect: Rect,
    pub lines: Vec<TextLine>,
}

pub(crate) const GAME_OVER_PROMPTS: &[&str] = &["R RESTART  ESC MENU", "R RETRY  ESC MENU", "R RETRY"];

pub fn game_over_layout(fw: i32, fh: i32, title: &str, score: u32, scale: UiScale) -> BoxLayout {
    let stats = format!("SCORE {}", score);

    let build = |prompt: &str, title_scale: UiScale, pad: i32| {
        let sub = title_scale.sub();
        let title_h = title_scale.line_height();
        let sub_h = sub.line_height();
        let inner_w = title_scale
            .text_width(title)
            .max(sub.text_width(&stats))
            .max(sub.text_width(prompt));
        let box_w = inner_w + pad * 4;
        let box_h = title_h + sub_h * 2 + pad * 6;
        let rect = Rect::new((fw - box_w) / 2, (fh - box_h) / 2, box_w, box_h);
        let mut y = rect.y + pad * 2;
        let mut lines = vec![TextLine::centered(title, y, fw, TEXT_COLOR, title_scale)];
        y += title_h + pad;
        lines.push(TextLine::centered(&stats, y, fw, TEXT_DIM, sub));
        y += sub_h + pad;
        lines.push(TextLine::centered(prompt, y, fw, TEXT_DIM, sub));
        BoxLayout { rect, lines }
    };

    // Prefer the full prompt over a bigger title, and a bigger title over
    // roomy padding.
    for prompt in GAME_OVER_PROMPTS {
        for title_scale in scale.ladder() {
            for pad_mult in [4, 2] {
                let layout = build(prompt, title_scale, pad_mult * title_scale.cell_scale as i32);
                if layout.rect.inside(fw, fh) {
                    return layout;
                }
            }
        }
    }
    // Far below any comfortable size: smallest everything. The frame may
    // clip but the text stays centred.
    build(GAME_OVER_PROMPTS[GAME_OVER_PROMPTS.len() - 1], UiScale::SMALLEST, 1)
}

pub fn draw_game_over(fb: &mut Framebuffer, title: &str, score: u32, scale: UiScale) {
    let layout = game_over_layout(fb.width as i32, fb.height as i32, title, score, scale);
    let r = layout.rect;
    draw::fill_rect(fb, r.x, r.y, r.w, r.h, ARENA_BORDER);
    draw::fill_rect(fb, r.x + 1, r.y + 1, r.w - 2, r.h - 2, HUD_BG);
    for line in &layout.lines {
        line.draw(fb);
    }
}

pub(crate) const VICTORY_TEXTS: &[&[&str]] = &[
    &["THE REALM SINGS YOUR NAME,", "BRAVE KNIGHT."],
    &["THE REALM SINGS", "YOUR NAME,", "BRAVE KNIGHT."],
    &["VICTORY!"],
];
pub(crate) const VICTORY_PROMPTS: &[&str] = &["ENTER  RETURN TO COURT", "ENTER  TO COURT", "ENTER"];

/// King portrait extents around its centre in pixels: from the top of the
/// halo to the bottom of the robe, plus the halo width.
const KING_HALF_TOP: i32 = 14;
const KING_HALF_BOT: i32 = 16;
const KING_W: i32 = 30;
/// Dark frame drawn around the parchment.
const SCROLL_FRAME: i32 = 2;

/// Where the victory scroll and its contents go.
pub struct VictoryLayout {
    /// The parchment, not counting its dark frame.
    pub scroll: Rect,
    /// King portrait centre, when there is room for it.
    pub king: Option<(i32, i32)>,
    pub lines: Vec<TextLine>,
}

impl VictoryLayout {
    /// The scroll including its frame.
    pub fn outer(&self) -> Rect {
        let f = SCROLL_FRAME;
        Rect::new(self.scroll.x - f, self.scroll.y - f, self.scroll.w + 2 * f, self.scroll.h + 2 * f)
    }
}

/// Lay out the victory scroll. Tries, in order: smaller body scales, then no
/// king portrait, then shorter body text. Each attempt prefers the space below
/// the HUD bar and roomier padding. The prompt is always the longest variant
/// that fits, so it is always fully visible.
pub fn victory_layout(fw: i32, fh: i32, top_px: i32, scale: UiScale) -> VictoryLayout {
    let regions = [(top_px, fh - top_px), (0, fh)];
    // Body text stays in the 5×5 font until the smallest scale; large 3×5
    // text reads poorly in a paragraph.
    let scales: Vec<UiScale> = scale
        .ladder()
        .into_iter()
        .filter(|s| s.glyph_w == 5 || s.cell_scale == 1)
        .collect();
    for body in VICTORY_TEXTS {
        for king in [true, false] {
            // On wide screens the scroll stays within three quarters of the
            // width so the arena and fireworks show on both sides. Narrow
            // screens may use it all rather than lose text.
            for max_w in [fw * 3 / 4, fw] {
                for &s in &scales {
                    for &region in &regions {
                        for pad_div in [1, 2] {
                            let width = (fw, max_w);
                            if let Some(l) =
                                try_victory_layout(width, region, body, king, s, pad_div, false)
                            {
                                return l;
                            }
                        }
                    }
                }
            }
        }
    }
    let body = VICTORY_TEXTS[VICTORY_TEXTS.len() - 1];
    try_victory_layout((fw, fw), (0, fh), body, false, UiScale::SMALLEST, 2, true)
        .expect("forced layout always succeeds")
}

/// One attempt at the victory layout. `width` is the framebuffer width and
/// the most the scroll may take; `region` is the top and height of the band
/// the scroll is centred in. With `force` it skips the fit checks.
fn try_victory_layout(
    width: (i32, i32),
    region: (i32, i32),
    body: &[&str],
    king: bool,
    s: UiScale,
    pad_div: i32,
    force: bool,
) -> Option<VictoryLayout> {
    let (fw, max_w) = width;
    let (region_y, region_h) = region;
    let pad = 4 * s.cell_scale as i32;
    // Space above and below the content; halved when room is tight.
    let outer_pad = pad * 2 / pad_div;
    let body_gap = (pad / 2).max(2);
    let line_h = s.line_height();
    let min_side = 2;
    let max_text_w = max_w.min(fw) - 2 * SCROLL_FRAME - 2 * min_side;

    let prompt = match longest_fitting(VICTORY_PROMPTS, s, max_text_w) {
        Some(p) => p,
        None if force => VICTORY_PROMPTS[VICTORY_PROMPTS.len() - 1],
        None => return None,
    };
    let mut content_w = s.text_width(prompt);
    for line in body {
        content_w = content_w.max(s.text_width(line));
    }
    if king {
        content_w = content_w.max(KING_W);
    }
    if content_w > max_text_w && !force {
        return None;
    }

    let n_body = body.len() as i32;
    let king_h = if king { KING_HALF_TOP + KING_HALF_BOT + pad } else { 0 };
    let scroll_h = outer_pad
        + king_h
        + line_h * n_body
        + body_gap * (n_body - 1).max(0)
        + pad
        + line_h
        + outer_pad;
    if scroll_h + 2 * SCROLL_FRAME > region_h && !force {
        return None;
    }

    // Keep the scroll only as wide as its content needs.
    let side = ((max_w.min(fw) - 2 * SCROLL_FRAME - content_w) / 2).clamp(min_side, pad * 2);
    let scroll_w = content_w + side * 2;
    let scroll = Rect::new(
        (fw - scroll_w) / 2,
        region_y + (region_h - scroll_h) / 2,
        scroll_w,
        scroll_h,
    );

    let ink = Color::new(20, 12, 5);
    let ink_dim = Color::new(90, 65, 35);
    let mut y = scroll.y + outer_pad;
    let king_pos = if king {
        let pos = (fw / 2, y + KING_HALF_TOP);
        y += king_h;
        Some(pos)
    } else {
        None
    };
    let mut lines = Vec::new();
    for (i, line) in body.iter().enumerate() {
        lines.push(TextLine::centered(line, y, fw, ink, s));
        y += line_h;
        if i + 1 < body.len() {
            y += body_gap;
        }
    }
    y += pad;
    lines.push(TextLine::centered(prompt, y, fw, ink_dim, s));

    Some(VictoryLayout { scroll, king: king_pos, lines })
}

pub fn draw_victory(fb: &mut Framebuffer, scale: UiScale, top_px: i32) {
    let parchment = Color::new(240, 226, 188);
    let parchment_dim = Color::new(160, 130, 80);
    let ink = Color::new(20, 12, 5);

    let layout = victory_layout(fb.width as i32, fb.height as i32, top_px, scale);
    let o = layout.outer();
    let r = layout.scroll;

    // Scroll backdrop, drawn before the king so the parchment is the surface
    // the king is "painted" on.
    draw::fill_rect(fb, o.x, o.y, o.w, o.h, ink);
    draw::fill_rect(fb, r.x, r.y, r.w, r.h, parchment);
    draw::fill_rect(fb, r.x, r.y, r.w, 2, parchment_dim);
    draw::fill_rect(fb, r.x, r.bottom() - 2, r.w, 2, parchment_dim);

    // King portrait at the top of the scroll, with a parchment-toned halo
    // so it blends into the parchment instead of leaving a dark hole.
    if let Some((kx, ky)) = layout.king {
        draw_king(fb, kx, ky, parchment_dim);
    }
    for line in &layout.lines {
        line.draw(fb);
    }
}

/// Procedural pixel-art king portrait, drawn centered at `(cx, cy)`.
/// `backdrop` is the color of the soft halo behind the head. Pass the
/// surrounding background tone (e.g. parchment_dim when on a parchment scroll,
/// near-black when on the dark game background).
fn draw_king(fb: &mut Framebuffer, cx: i32, cy: i32, backdrop: Color) {
    let gold = Color::new(232, 195, 80);
    let red_gem = Color::new(220, 50, 50);
    let blue_gem = Color::new(60, 110, 200);
    let face = Color::new(225, 185, 150);
    let face_shadow = Color::new(170, 130, 100);
    let beard = Color::new(230, 225, 220);
    let eye = Color::new(35, 25, 20);
    let robe = Color::new(150, 30, 30);
    let robe_trim = Color::new(232, 195, 80);

    // Backdrop circle behind the head.
    draw::filled_circle(fb, cx as f32, cy as f32, 14.0, backdrop);

    // Crown band (4-pixel-tall rectangle across the top of the head).
    let band_y = cy - 9;
    draw::fill_rect(fb, cx - 8, band_y, 16, 3, gold);

    // Three crown tips.
    for (i, gem) in [red_gem, blue_gem, red_gem].iter().enumerate() {
        let tx = cx - 7 + i as i32 * 7;
        for dy in 0..3 {
            let half = 2 - dy;
            for dx in -half..=half {
                fb.set(tx + dx, band_y - 3 + dy, gold);
            }
        }
        // Gem at tip.
        fb.set(tx, band_y - 3, *gem);
    }

    // Face.
    draw::fill_axis_ellipse(fb, cx as f32, cy as f32, 7.0, 6.5, face);

    // Eyebrows.
    for dx in -1..=1 {
        fb.set(cx - 3 + dx, cy - 3, eye);
        fb.set(cx + 3 + dx, cy - 3, eye);
    }

    // Eyes.
    fb.set(cx - 3, cy - 1, eye);
    fb.set(cx + 3, cy - 1, eye);

    // Nose hint.
    fb.set(cx, cy + 1, face_shadow);

    // Beard: a wider ellipse below the face.
    draw::fill_axis_ellipse(fb, cx as f32, (cy + 6) as f32, 9.0, 5.0, beard);

    // Mustache curls just under the nose.
    for dx in -3i32..=3 {
        if dx.abs() == 3 || dx.abs() == 1 {
            fb.set(cx + dx, cy + 3, beard);
        }
    }

    // Mouth: a small dark line.
    for dx in -1..=1 {
        fb.set(cx + dx, cy + 4, eye);
    }

    // Robe collar.
    draw::fill_rect(fb, cx - 12, cy + 11, 24, 4, robe);
    draw::fill_rect(fb, cx - 12, cy + 11, 24, 1, robe_trim);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn color_tuples(fb: &Framebuffer) -> Vec<(u8, u8, u8)> {
        fb.color.iter().map(|c| (c.r, c.g, c.b)).collect()
    }

    /// The cached floor+border layer must be frame-invariant to be safe to
    /// cache: two renders with the same inputs produce identical pixels, and it
    /// covers a large area (so caching is worthwhile).
    #[test]
    fn arena_floor_is_deterministic_and_nonempty() {
        let iso = iso::fit_arena(100.0, 160, 96, 8, 1.0);
        let mut a = Framebuffer::new(160, 96);
        let mut b = Framebuffer::new(160, 96);
        a.clear(HUD_BG);
        b.clear(HUD_BG);
        draw_arena_floor(&mut a, &iso, (50.0, 50.0), 50.0);
        draw_arena_floor(&mut b, &iso, (50.0, 50.0), 50.0);

        assert_eq!(
            color_tuples(&a),
            color_tuples(&b),
            "floor render must be deterministic to cache safely"
        );

        let bg = (HUD_BG.r, HUD_BG.g, HUD_BG.b);
        let painted = color_tuples(&a).into_iter().filter(|&c| c != bg).count();
        assert!(painted > 500, "floor should cover a large area, got {painted}");
    }

    #[test]
    fn scale_ladder_shrinks_to_smallest() {
        let l = UiScale { glyph_w: 5, cell_scale: 2 }.ladder();
        let expect = [(5, 2), (3, 2), (5, 1), (3, 1)];
        let got: Vec<(u8, u8)> = l.iter().map(|s| (s.glyph_w, s.cell_scale)).collect();
        assert_eq!(got, expect);
        assert_eq!(UiScale::SMALLEST.ladder(), vec![UiScale::SMALLEST]);
    }
}
