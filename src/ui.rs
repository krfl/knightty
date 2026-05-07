use crate::fx::Particle;
use crate::render::draw;
use crate::render::framebuffer::{Color, Framebuffer};
use crate::render::iso::{self, Iso};
use crate::world::arena::ArenaTier;
use crate::world::combatant::Combatant;
use crate::world::hazard::{Difficulty, Hazard, HazardKind};

pub const WOOD_DARK: Color = Color::new(86, 56, 30);
pub const WOOD_LIGHT: Color = Color::new(140, 95, 55);
pub const WOOD_GRAIN: Color = Color::new(110, 75, 40);
pub const ARENA_BORDER: Color = Color::new(60, 38, 18);
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

/// UI text scale: glyph width (3 or 5; 0 reserved as "use 3×5 fallback") and
/// cell multiplier (each glyph pixel = `cell_scale × cell_scale` fb pixels).
/// Picked from terminal cell dimensions so UI keeps a consistent fraction of
/// screen real estate as cell density (font size) changes.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct UiScale {
    pub glyph_w: u8,
    pub cell_scale: u8,
}

impl UiScale {
    /// Glyph height in fb pixels (always 5 rows in our font).
    pub fn line_height(&self) -> i32 {
        5 * self.cell_scale as i32
    }
}

/// Target pixel height for the menu/title text. The scale is chosen to hit
/// roughly this many on-screen pixels regardless of the terminal's font size.
const TARGET_TITLE_PX: u32 = 64;

pub fn pick_ui_scale(term_cols: u16, term_rows: u16, cell_h_px: u16) -> UiScale {
    // Below the small-terminal threshold, drop to 3×5 so menu/HUD still fits.
    // No plain-text fallback — too tiny for any bitmap font shows the
    // BelowMin splash instead.
    if term_cols < 80 || term_rows < 28 {
        return UiScale { glyph_w: 3, cell_scale: 1 };
    }

    // Pick `cell_scale` to hit `TARGET_TITLE_PX` of glyph height in absolute
    // screen pixels. At cell_h=8 (font 4) this lands on ~2; at cell_h=15
    // (font 12) it lands on 1; at cell_h=4 (very small fonts) it lands on 4.
    // Same physical readability across font sizes.
    let glyph_h_at_1 = 5u32 * cell_h_px.max(4) as u32;
    let from_target = ((TARGET_TITLE_PX + glyph_h_at_1 - 1) / glyph_h_at_1).max(1) as u8;

    // Cap by terminal cell counts so we don't overflow horizontally or
    // vertically. "JOUST" at scale=N takes 30*N cells wide; menu+title+
    // footer at scale=N takes ~30*N rows tall.
    let h_cap = ((term_cols as u32) / 80).max(1) as u8;
    let v_cap = ((term_rows as u32) / 30).max(1) as u8;

    UiScale {
        glyph_w: 5,
        cell_scale: from_target.min(h_cap).min(v_cap).clamp(1, 4),
    }
}

/// Reserved cell rows at the top of the terminal for the HUD bar — kept as 0
/// since the HUD is drawn into the framebuffer (no separate ratatui row).
pub const HUD_HEIGHT: i32 = 0;

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
    pub tier: ArenaTier,
    pub knight: &'a Combatant,
    pub opponent: &'a Combatant,
    pub tilt: (f32, f32),
    pub hazards: &'a [Hazard],
    pub particles: &'a [Particle],
    pub banner: Option<&'a str>,
}

pub struct HudView {
    pub score: u32,
    pub best: u32,
    pub difficulty: Difficulty,
    pub level: u32,
}


#[allow(dead_code)]
pub fn arena_rect(fb: &Framebuffer) -> (i32, i32, i32) {
    let avail_w = fb.width as i32;
    let avail_h = (fb.height as i32 - HUD_HEIGHT).max(1);
    let side = avail_w.min(avail_h);
    let x = (avail_w - side) / 2;
    let y = HUD_HEIGHT + (avail_h - side) / 2;
    (x, y, side)
}

pub fn draw_background(fb: &mut Framebuffer) {
    fb.clear(HUD_BG);
}

/// Height in framebuffer pixels of the HUD bar drawn by `draw_hud` at a given
/// scale. Other drawing code uses this to keep below the bar.
pub fn hud_bar_height(scale: UiScale) -> i32 {
    let pad_y = (scale.cell_scale as i32).max(1);
    scale.line_height() + pad_y * 2
}

pub fn draw_arena(fb: &mut Framebuffer, view: &ArenaView, hud_scale: UiScale) {
    let s = view.tier.size();
    let iso = iso::fit_arena(s, fb.width as i32, fb.height as i32, HUD_HEIGHT);
    let cx = s * 0.5;
    let cy = s * 0.5;
    let arena_r = s * 0.5;

    // Round floor — wood texture in world coords.
    draw::fill_iso_circle(fb, &iso, (cx, cy), arena_r, |wx, wy| {
        let t = (wy / s * 7.0).sin() * 0.5 + 0.5;
        let row = WOOD_DARK.lerp(WOOD_LIGHT, t * 0.4 + 0.3);
        let n = (((wx * 3.0) as i32 + (wy * 5.0) as i32).rem_euclid(13)) as f32 / 13.0;
        if n > 0.72 { WOOD_GRAIN } else { row }
    });

    // Bumpy boundary — polar curve `r(θ) = R + amp * sin(N*θ)`.
    let segments = 144usize;
    let mut prev: Option<(f32, f32)> = None;
    for i in 0..=segments {
        let theta = i as f32 / segments as f32 * std::f32::consts::TAU;
        let bump = crate::physics::WALL_BUMP_AMP * (crate::physics::WALL_BUMP_N * theta).sin();
        let r = arena_r + bump;
        let wx = cx + theta.cos() * r;
        let wy = cy + theta.sin() * r;
        let pt = iso.project((wx, wy, 0.0));
        if let Some(p) = prev {
            draw::tapered_line(fb, p.0, p.1, pt.0, pt.1, 2.0, 2.0, ARENA_BORDER);
        }
        prev = Some(pt);
    }

    // Tilt indicator anchored below the HUD bar, with a small gap so the
    // outer ring is fully visible at every font scale.
    let ind_radius = 5.0_f32;
    let bar_bottom = hud_bar_height(hud_scale) + 2;
    let ind_cx = 9.0;
    let ind_cy = bar_bottom as f32 + ind_radius + 1.0;
    draw::filled_circle(fb, ind_cx, ind_cy, ind_radius, ARENA_BORDER);
    draw::filled_circle(
        fb,
        ind_cx + view.tilt.0 * 4.0,
        ind_cy + view.tilt.1 * 4.0,
        2.0,
        TEXT_COLOR,
    );

    // Back-half banners (drawn before combatants).
    draw_banners(fb, &iso, (cx, cy), arena_r, false);

    // Hazards on the floor.
    for h in view.hazards {
        match h.kind {
            HazardKind::Pit => {
                draw_iso_ground_disc(fb, &iso, h.pos, h.radius, PIT_RIM);
                draw_iso_ground_disc(fb, &iso, h.pos, h.radius * 0.85, PIT_DARK);
            }
            HazardKind::Bumper => draw_iso_bumper(fb, &iso, h.pos, h.radius),
        }
    }

    // Combatants — depth-sorted.
    let kd = view.knight.pos.0 + view.knight.pos.1;
    let od = view.opponent.pos.0 + view.opponent.pos.1;
    if kd <= od {
        draw_combatant(fb, view.knight, &iso, &PLAYER_PALETTE);
        draw_combatant(fb, view.opponent, &iso, &OPPONENT_PALETTE);
    } else {
        draw_combatant(fb, view.opponent, &iso, &OPPONENT_PALETTE);
        draw_combatant(fb, view.knight, &iso, &PLAYER_PALETTE);
    }

    // Front-half banners — drawn after combatants.
    draw_banners(fb, &iso, (cx, cy), arena_r, true);

    // Particles (fireworks). Rendered last so sky bursts always show on top.
    for p in view.particles {
        let life_frac = (1.0 - p.age / p.lifetime.max(0.001)).clamp(0.0, 1.0);
        let (sx, sy) = iso.project(p.pos);
        let r = (0.7 * iso.scale * (0.5 + 0.5 * life_frac)).max(0.6);
        let color = p.color.scale(life_frac);
        draw::filled_circle(fb, sx, sy, r, color);
    }

    if let Some(msg) = view.banner {
        let text_w = msg.chars().count() as i32 * 6;
        let center = iso.project((s * 0.5, s * 0.5, 0.0));
        let bx = center.0 as i32 - text_w / 2;
        let by = center.1 as i32 - 3;
        draw::fill_rect(fb, bx - 4, by - 3, text_w + 8, 11, ARENA_BORDER);
        draw::draw_text(fb, bx, by, msg, TEXT_COLOR);
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
    let half_w = world_r * iso.scale * std::f32::consts::SQRT_2;
    let half_h = half_w * 0.5;
    draw::fill_axis_ellipse(fb, sx, sy, half_w, half_h, color);
}

fn draw_banners(
    fb: &mut Framebuffer,
    iso: &Iso,
    center: (f32, f32),
    arena_r: f32,
    front_half: bool,
) {
    let pole_h = (arena_r * 0.20).max(6.0);
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

    let finial = iso.project((anchor.0, anchor.1, pole_h + 1.2));
    draw::filled_circle(fb, finial.0, finial.1, 1.2, BANNER_FINIAL);

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

    let stripes = 5;
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
            let stripe = (v * stripes as f32).floor() as i32;
            let color = if stripe % 2 == 0 { BANNER_RED } else { BANNER_WHITE };
            fb.set(x_i, y_i, color);
        }
    }
}

fn draw_iso_bumper(fb: &mut Framebuffer, iso: &Iso, world_xy: (f32, f32), world_r: f32) {
    let height = 2.5_f32;
    let (bx, by) = iso.project((world_xy.0, world_xy.1, 0.0));
    let (tx, ty) = iso.project((world_xy.0, world_xy.1, height));
    let half_w = world_r * iso.scale * std::f32::consts::SQRT_2;
    let half_h = half_w * 0.5;
    let side_y0 = ty as i32;
    let side_y1 = by as i32;
    if side_y1 > side_y0 {
        draw::fill_rect(
            fb,
            (bx - half_w) as i32,
            side_y0,
            (2.0 * half_w) as i32,
            side_y1 - side_y0,
            BUMPER_RIM,
        );
    }
    draw::fill_axis_ellipse(fb, bx, by, half_w, half_h, BUMPER_RIM);
    draw::fill_axis_ellipse(fb, tx, ty, half_w, half_h, BUMPER_BODY);
    draw::fill_axis_ellipse(fb, tx, ty, half_w * 0.42, half_h * 0.42, TEXT_COLOR);
}

fn draw_combatant(fb: &mut Framebuffer, c: &Combatant, iso: &Iso, p: &CombatantPalette) {
    let (fx, fy) = c.facing;
    let perp = (-fy, fx);
    let scale = iso.scale;

    let proj = |fwd: f32, lat: f32, up: f32| -> (f32, f32) {
        iso.project((
            c.pos.0 + fwd * fx + lat * perp.0,
            c.pos.1 + fwd * fy + lat * perp.1,
            c.z + up,
        ))
    };

    let p0 = iso.project((c.pos.0, c.pos.1, c.z + 1.0));
    let p1 = iso.project((c.pos.0 + fx, c.pos.1 + fy, c.z + 1.0));
    let raw = (p1.0 - p0.0, p1.1 - p0.1);
    let raw_len = (raw.0 * raw.0 + raw.1 * raw.1).sqrt().max(0.001);
    let sf = (raw.0 / raw_len, raw.1 / raw_len);

    let shadow_shrink = 1.0 - (c.z / 12.0).clamp(0.0, 0.5);
    let (shx, shy) = iso.project2(c.pos);
    let shadow_w = 3.6 * scale * shadow_shrink * std::f32::consts::SQRT_2;
    draw::fill_axis_ellipse(fb, shx, shy, shadow_w, shadow_w * 0.5, ARENA_BORDER);

    let belly = 1.1;
    let foot_up = if c.airborne() { 0.4 } else { -c.z };
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

    let (ktx, kty) = proj(-0.2, 0.0, 3.0);
    draw::filled_circle(fb, ktx, kty, 1.25 * scale, p.knight);

    let (pl0x, pl0y) = proj(0.1, 0.0, 4.0);
    let (pl1x, pl1y) = proj(-1.3, 0.0, 4.4);
    draw::tapered_line(fb, pl0x, pl0y, pl1x, pl1y, 0.9 * scale, 0.3 * scale, p.plume);

    let (hex, hey) = proj(0.3, 0.0, 4.0);
    draw::filled_circle(fb, hex, hey, 0.85 * scale, p.helmet);

    let (s_x, s_y) = proj(0.4, -1.3, 2.9);
    draw::filled_circle(fb, s_x, s_y, 0.85 * scale, p.shield);
    draw::filled_circle(fb, s_x, s_y, 0.4 * scale, p.shield_accent);

    let saddle = proj(0.6, 0.7, 2.7);
    let lance_tip_world = (
        c.pos.0 + fx * c.lance_len,
        c.pos.1 + fy * c.lance_len,
        c.z + 2.7,
    );
    let lance_tip_screen = iso.project(lance_tip_world);
    draw::tapered_line(
        fb,
        saddle.0,
        saddle.1,
        lance_tip_screen.0,
        lance_tip_screen.1,
        1.0 * scale,
        0.35 * scale,
        LANCE_WOOD,
    );
    draw::filled_circle(fb, lance_tip_screen.0, lance_tip_screen.1, 0.5 * scale, LANCE_TIP);
}

/// Pick the longest of `candidates` whose width fits `max_fb` fb pixels at
/// the given scale. Falls back to the last (shortest) candidate.
fn fit_candidate<'a>(candidates: &'a [String], scale: UiScale, max_fb: i32) -> &'a String {
    for cand in candidates {
        if draw::text_bitmap_width(cand, scale.glyph_w, scale.cell_scale) <= max_fb {
            return cand;
        }
    }
    candidates.last().unwrap()
}

fn draw_centered(
    fb: &mut Framebuffer,
    text: &str,
    cy_top: i32,
    fb_w: i32,
    color: Color,
    scale: UiScale,
) {
    let w = draw::text_bitmap_width(text, scale.glyph_w, scale.cell_scale);
    let x = (fb_w - w) / 2;
    draw::draw_text_bitmap(fb, x, cy_top, text, color, scale.glyph_w, scale.cell_scale);
}

pub fn draw_menu(
    fb: &mut Framebuffer,
    selection: usize,
    difficulty: Difficulty,
    scale: UiScale,
) {
    let fw = fb.width as i32;
    let fh = fb.height as i32;

    fb.clear(HUD_BG);

    let title_h = scale.line_height();
    let item_h = scale.line_height();
    // Sub-scale: secondary text (footer) one tier smaller for visual
    // hierarchy without dominating vertical space.
    let sub_scale = if scale.cell_scale > 1 {
        UiScale {
            glyph_w: scale.glyph_w,
            cell_scale: scale.cell_scale - 1,
        }
    } else if scale.glyph_w == 5 {
        UiScale { glyph_w: 3, cell_scale: 1 }
    } else {
        scale
    };
    let sub_h = sub_scale.line_height();

    // Knight backdrop scales with the menu scale, capped so it doesn't
    // overflow narrow terminals — design is roughly 40 fb pixels wide at
    // scale 1 (37 wide × 28 tall before margin).
    let knight_scale_w_cap = ((fw - 4) / 50).max(1);
    let knight_scale = (scale.cell_scale as i32).min(2).min(knight_scale_w_cap).max(1);
    let knight_h = 28 * knight_scale;
    let knight_gap = sub_h;

    // Gaps tied to sub_h so they scale proportionally with the menu.
    let item_gap = sub_h;
    let title_gap = sub_h * 2;

    let menu_h = title_h + title_gap + item_h * 3 + item_gap * 2;
    // Reserve space for the footer (sub_h text + 2px margin + knight_gap above)
    // so the knight only draws when everything fits inside the framebuffer.
    let footer_reserve = sub_h + 2 + knight_gap;
    let show_knight = menu_h + knight_h + knight_gap + footer_reserve <= fh;

    let total_h = if show_knight { knight_h + knight_gap + menu_h } else { menu_h };
    let mut y = ((fh - total_h) / 2).max(2);

    if show_knight {
        // Mounted knight, profile view, lance forward.
        draw_menu_knight(fb, fw / 2, y + 16 * knight_scale, knight_scale);
        y += knight_h + knight_gap;
    }

    draw_centered(fb, "KNIGHTTY", y, fw, TEXT_COLOR, scale);
    y += title_h + title_gap;

    let labels: [&str; 3] = [
        "PLAY",
        difficulty.label(),
        "QUIT",
    ];
    for (i, label) in labels.iter().enumerate() {
        let color = if i == selection { TEXT_COLOR } else { TEXT_DIM };
        draw_centered(fb, label, y, fw, color, scale);
        y += item_h;
        if i < labels.len() - 1 {
            y += item_gap;
        }
    }

    let foot_y = (fh - sub_h - 2).max(y + sub_h);
    draw_centered(fb, "WASD TILT  SPACE JUMP  CTRL-C EXIT", foot_y, fw, TEXT_DIM, sub_scale);
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

    // Horse body — broad oval at center.
    let (bx, by) = p(0, 0);
    draw::fill_axis_ellipse(fb, bx, by, 12.0 * sf, 5.0 * sf, horse);

    // Caparison (saddle blanket) — blue stripe on the body's upper half.
    draw::fill_axis_ellipse(fb, bx, by - 1.0 * sf, 9.0 * sf, 3.0 * sf, stripe_blue);

    // Front legs (drawn over the body so the silhouette reads correctly).
    for &x in &[6, 9] {
        let (lx, ly_top) = p(x, 4);
        let (_, ly_bot) = p(x, 12);
        draw::tapered_line(fb, lx, ly_top, lx, ly_bot, 1.6 * sf, 1.4 * sf, horse_dark);
    }

    // Neck — tapered line angled up to the head.
    let (n0x, n0y) = p(9, -3);
    let (n1x, n1y) = p(14, -8);
    draw::tapered_line(fb, n0x, n0y, n1x, n1y, 4.0 * sf, 2.5 * sf, horse);

    // Mane lump.
    let (mx, my) = p(11, -5);
    draw::filled_circle(fb, mx, my, 1.8 * sf, horse_dark);

    // Horse head — small ellipse pointing forward.
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

    // Lance — angled forward and up, with a metal tip.
    let (l0x, l0y) = p(2, -8);
    let (l1x, l1y) = p(24, -19);
    draw::tapered_line(fb, l0x, l0y, l1x, l1y, 2.5 * sf, 0.9 * sf, lance_wood);
    draw::filled_circle(fb, l1x, l1y, 1.2 * sf, lance_tip_color);

    // Shield over the rider's front.
    let (shx, shy) = p(4, -7);
    draw::filled_circle(fb, shx, shy, 2.5 * sf, shield);
    draw::filled_circle(fb, shx, shy, 1.2 * sf, shield_accent);
}

pub fn draw_hud(fb: &mut Framebuffer, view: &HudView, scale: UiScale) {
    let fw = fb.width as i32;
    let pad_y = (scale.cell_scale as i32).max(1);
    let bar_h = hud_bar_height(scale);
    draw::fill_rect(fb, 0, 0, fw, bar_h, HUD_BG);
    draw::fill_rect(fb, 0, bar_h, fw, 1, ARENA_BORDER);

    // Pick the longest variant that fits.
    let candidates = [
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
    ];
    let text = fit_candidate(&candidates, scale, fw - 4 * scale.cell_scale as i32);
    draw_centered(fb, text, pad_y, fw, TEXT_COLOR, scale);
}

pub fn draw_below_min(fb: &mut Framebuffer) {
    fb.clear(HUD_BG);
    let fw = fb.width as i32;
    let fh = fb.height as i32;
    let scale = UiScale { glyph_w: 3, cell_scale: 1 };
    let lines = ["RESIZE TERMINAL", "TO PLAY"];
    let total = scale.line_height() * lines.len() as i32 + (lines.len() as i32 - 1) * 2;
    let mut y = (fh - total) / 2;
    for line in &lines {
        draw_centered(fb, line, y, fw, TEXT_COLOR, scale);
        y += scale.line_height() + 2;
    }
}

pub fn draw_game_over(fb: &mut Framebuffer, title: &str, score: u32, scale: UiScale) {
    let fw = fb.width as i32;
    let fh = fb.height as i32;

    let sub_scale = if scale.cell_scale > 1 {
        UiScale {
            glyph_w: scale.glyph_w,
            cell_scale: scale.cell_scale - 1,
        }
    } else if scale.glyph_w == 5 {
        UiScale { glyph_w: 3, cell_scale: 1 }
    } else {
        scale
    };

    let title_w = draw::text_bitmap_width(title, scale.glyph_w, scale.cell_scale);
    let title_h = scale.line_height();
    let sub_h = sub_scale.line_height();
    let stats = format!("SCORE {}", score);
    let prompt = "R RESTART  ESC MENU";
    let stats_w = draw::text_bitmap_width(&stats, sub_scale.glyph_w, sub_scale.cell_scale);
    let prompt_w = draw::text_bitmap_width(prompt, sub_scale.glyph_w, sub_scale.cell_scale);

    let pad = 4 * scale.cell_scale as i32;
    let inner_w = title_w.max(stats_w).max(prompt_w);
    let inner_h = title_h + sub_h * 2 + pad * 3;
    let box_w = inner_w + pad * 4;
    let box_h = inner_h + pad * 2;
    let box_x = (fw - box_w) / 2;
    let box_y = (fh - box_h) / 2;

    draw::fill_rect(fb, box_x, box_y, box_w, box_h, ARENA_BORDER);
    draw::fill_rect(fb, box_x + 1, box_y + 1, box_w - 2, box_h - 2, HUD_BG);

    let mut y = box_y + pad * 2;
    draw_centered(fb, title, y, fw, TEXT_COLOR, scale);
    y += title_h + pad;
    draw_centered(fb, &stats, y, fw, TEXT_DIM, sub_scale);
    y += sub_h + pad;
    draw_centered(fb, prompt, y, fw, TEXT_DIM, sub_scale);
}

pub fn draw_victory(fb: &mut Framebuffer, scale: UiScale) {
    let fw = fb.width as i32;
    let fh = fb.height as i32;
    let cx = fw / 2;

    let parchment = Color::new(240, 226, 188);
    let parchment_dim = Color::new(160, 130, 80);
    let ink = Color::new(20, 12, 5);
    let ink_dim = Color::new(90, 65, 35);

    let body = [
        "THE REALM SINGS YOUR NAME,",
        "BRAVE KNIGHT.",
    ];
    let prompt = "ENTER  RETURN TO COURT";

    // Approximate procedural-king-portrait extents in fb pixels (top of crown
    // to bottom of robe collar, plus a small gap).
    const KING_HALF_TOP: i32 = 13;
    const KING_HALF_BOT: i32 = 16;
    const KING_TOTAL_H: i32 = KING_HALF_TOP + KING_HALF_BOT;

    let pad = 4 * scale.cell_scale as i32;
    let outer_pad = pad * 2;
    // Subtle gap between body lines so they breathe.
    let body_gap = (pad / 2).max(2);
    // Available width for body lines inside the scroll, minus side margins.
    let max_inner_w = fw - 4 - outer_pad * 2;

    // Auto-fit body scale — pick the largest variant where every body and
    // prompt line fits inside the scroll panel.
    let body_candidates: &[UiScale] = &[
        scale,
        UiScale {
            glyph_w: scale.glyph_w,
            cell_scale: scale.cell_scale.saturating_sub(1).max(1),
        },
        UiScale { glyph_w: 5, cell_scale: 1 },
        UiScale { glyph_w: 3, cell_scale: 1 },
    ];
    let body_scale = *body_candidates
        .iter()
        .find(|s| {
            let lines = body.iter().chain(std::iter::once(&prompt));
            lines
                .map(|line| draw::text_bitmap_width(line, s.glyph_w, s.cell_scale))
                .max()
                .unwrap_or(0)
                <= max_inner_w
        })
        .unwrap_or(&UiScale { glyph_w: 3, cell_scale: 1 });

    let body_h = body_scale.line_height();

    // Scroll width tracks the widest text line, with floor at king diameter
    // so the head fits comfortably inside.
    let mut max_w = 36;
    for line in &body {
        max_w = max_w
            .max(draw::text_bitmap_width(line, body_scale.glyph_w, body_scale.cell_scale));
    }
    max_w = max_w.max(draw::text_bitmap_width(prompt, body_scale.glyph_w, body_scale.cell_scale));

    let scroll_w = (max_w + outer_pad * 2).min(fw - 4);
    // Layout (top → bottom): outer_pad, king portrait, pad, body lines with
    // body_gap between them, pad, prompt, outer_pad.
    let n_body = body.len() as i32;
    let scroll_h = outer_pad
        + KING_TOTAL_H
        + pad
        + body_h * n_body
        + body_gap * (n_body - 1).max(0)
        + pad
        + body_h
        + outer_pad;
    let scroll_x = (fw - scroll_w) / 2;
    let scroll_y = ((fh - scroll_h) / 2).max(8);

    // Scroll backdrop — drawn before the king so the parchment is the
    // surface the king is "painted" on.
    draw::fill_rect(fb, scroll_x - 2, scroll_y - 2, scroll_w + 4, scroll_h + 4, ink);
    draw::fill_rect(fb, scroll_x, scroll_y, scroll_w, scroll_h, parchment);
    draw::fill_rect(fb, scroll_x, scroll_y, scroll_w, 2, parchment_dim);
    draw::fill_rect(fb, scroll_x, scroll_y + scroll_h - 2, scroll_w, 2, parchment_dim);

    // King portrait at the top of the scroll, with a parchment-toned halo
    // so it integrates with the scroll background instead of carving a
    // dark hole into it.
    let king_cy = scroll_y + outer_pad + KING_HALF_TOP;
    draw_king(fb, cx, king_cy, parchment_dim);

    // Body and prompt below the king.
    let mut y = scroll_y + outer_pad + KING_TOTAL_H + pad;
    for (i, line) in body.iter().enumerate() {
        draw_centered(fb, line, y, fw, ink, body_scale);
        y += body_h;
        if i < body.len() - 1 {
            y += body_gap;
        }
    }
    y += pad;
    draw_centered(fb, prompt, y, fw, ink_dim, body_scale);
}

/// Procedural pixel-art king portrait, drawn centered at `(cx, cy)`.
/// `backdrop` is the color of the soft halo behind the head — pass the
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

    // Beard — wider ellipse below the face.
    draw::fill_axis_ellipse(fb, cx as f32, (cy + 6) as f32, 9.0, 5.0, beard);

    // Mustache curls just under the nose.
    for dx in -3i32..=3 {
        if dx.abs() == 3 || dx.abs() == 1 {
            fb.set(cx + dx, cy + 3, beard);
        }
    }

    // Mouth — small dark line.
    for dx in -1..=1 {
        fb.set(cx + dx, cy + 4, eye);
    }

    // Robe collar.
    draw::fill_rect(fb, cx - 12, cy + 11, 24, 4, robe);
    draw::fill_rect(fb, cx - 12, cy + 11, 24, 1, robe_trim);
}

