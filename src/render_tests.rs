//! Whole-frame rendering checks that run without a terminal. They build the
//! same frames the game loop does for a range of terminal sizes and inspect
//! the pixels and layout rectangles.

use std::io::Write;

use crate::physics::WALL_BUMP_AMP;
use crate::render::draw;
use crate::render::framebuffer::{Color, Framebuffer};
use crate::render::iso::Iso;
use crate::ui::{self, ArenaView, HudView, Layout, Rect};
use crate::world::arena;
use crate::world::combatant::Combatant;
use crate::world::hazard::{Difficulty, Hazard, HazardKind};

/// Terminal sizes (columns, rows, cell height in screen pixels). The cell
/// height shrinks with the terminal, the way lowering the font size does.
const SIZES: [(u16, u16, u16); 7] = [
    (80, 24, 16),
    (80, 26, 16),
    (100, 30, 16),
    (120, 40, 16),
    (200, 60, 8),
    (280, 80, 8),
    (450, 130, 4),
];

fn hud() -> HudView {
    HudView {
        score: 3,
        best: 7,
        difficulty: Difficulty::Normal,
        level: 4,
    }
}

/// Square pixels: a cell twice as tall as it is wide.
fn cell(cell_h: u16) -> (u16, u16) {
    (cell_h / 2, cell_h)
}

struct Frame {
    fb: Framebuffer,
    layout: Layout,
    arena_size: f32,
    iso: Option<Iso>,
}

impl Frame {
    fn playable(&self) -> bool {
        self.iso.is_some()
    }
}

/// A sample fight: two knights, a pit and a bumper, in fractions of the arena.
fn sample_world(s: f32) -> (Combatant, Combatant, Vec<Hazard>) {
    let knight = Combatant::new((s * 0.3, s * 0.5), (0.8, 0.6), 3.5, 9.0);
    let opponent = Combatant::new((s * 0.7, s * 0.5), (-1.0, 0.0), 3.5, 9.0);
    let hazards = vec![
        Hazard { pos: (s * 0.6, s * 0.3), radius: 4.0, kind: HazardKind::Pit },
        Hazard { pos: (s * 0.35, s * 0.7), radius: 3.5, kind: HazardKind::Bumper },
    ];
    (knight, opponent, hazards)
}

/// Build a full play frame (background, arena, HUD) the way the game loop
/// does, or the "resize" splash when the terminal is too small.
fn build_frame(cols: u16, rows: u16, cell_h: u16, banner: Option<&str>) -> Frame {
    let hud = hud();
    let mut fb = Framebuffer::new(cols as u32, rows as u32 * 2);
    let layout = Layout::new(cols, rows, cell(cell_h), &hud);
    let arena_size = arena::arena_size_for(fb.width, fb.height, layout.top_px, layout.y_scale);
    if arena::too_small(arena_size, fb.width, fb.height, layout.top_px, layout.y_scale) {
        ui::draw_below_min(&mut fb);
        return Frame { fb, layout, arena_size, iso: None };
    }
    let (knight, opponent, hazards) = sample_world(arena_size);
    let view = ArenaView {
        arena_size,
        knight: &knight,
        opponent: &opponent,
        tilt: (0.5, -0.5),
        hazards: &hazards,
        banner,
    };
    let mut bg = ui::ArenaBackground::new();
    let iso = ui::draw_scene(&mut fb, &mut bg, &view, &hud, &layout);
    Frame { fb, layout, arena_size, iso: Some(iso) }
}

fn same(a: Color, b: Color) -> bool {
    (a.r, a.g, a.b) == (b.r, b.g, b.b)
}

fn count(fb: &Framebuffer, c: Color) -> usize {
    fb.color.iter().filter(|p| same(**p, c)).count()
}

fn pixel(fb: &Framebuffer, x: i32, y: i32) -> Option<Color> {
    if x < 0 || y < 0 || x >= fb.width as i32 || y >= fb.height as i32 {
        return None;
    }
    Some(fb.color[(y as u32 * fb.width + x as u32) as usize])
}

/// Bounding box of every pixel that differs from `bg`, as (x0, y0, x1, y1)
/// inclusive.
fn painted_bbox(fb: &Framebuffer, bg: Color) -> Option<(i32, i32, i32, i32)> {
    let mut bb: Option<(i32, i32, i32, i32)> = None;
    for y in 0..fb.height as i32 {
        for x in 0..fb.width as i32 {
            if !same(pixel(fb, x, y).unwrap(), bg) {
                bb = Some(match bb {
                    None => (x, y, x, y),
                    Some((a, b, c, d)) => (a.min(x), b.min(y), c.max(x), d.max(y)),
                });
            }
        }
    }
    bb
}

/// Every terminal size in a coarse sweep that can show the arena, plus the
/// fixed sample sizes.
fn playable_sizes() -> Vec<(u16, u16, u16)> {
    let mut out: Vec<(u16, u16, u16)> = SIZES.to_vec();
    for cols in (40..=320).step_by(9) {
        for rows in (14..=100).step_by(5) {
            for cell_h in [16u16, 8, 4] {
                out.push((cols, rows, cell_h));
            }
        }
    }
    out.retain(|&(c, r, h)| {
        let layout = Layout::new(c, r, cell(h), &hud());
        let (w, fh) = (c as u32, r as u32 * 2);
        let s = arena::arena_size_for(w, fh, layout.top_px, layout.y_scale);
        !arena::too_small(s, w, fh, layout.top_px, layout.y_scale)
    });
    out
}

#[test]
fn small_terminals_are_playable() {
    for &(cols, rows, cell_h) in &SIZES {
        let f = build_frame(cols, rows, cell_h, None);
        assert!(f.playable(), "{cols}x{rows} should be playable");
        assert!(f.arena_size >= arena::MIN_ARENA_SIZE && f.arena_size <= arena::MAX_ARENA_SIZE);
    }
    assert!(!build_frame(40, 14, 16, None).playable(), "40x14 is too small to play");
}

/// The arena is drawn inside the framebuffer and below the HUD, and it fills
/// most of the space it has.
#[test]
fn arena_fits_and_fills_the_screen() {
    // Render with a wide margin around the real framebuffer, so anything that
    // would be clipped shows up as painted pixels in the margin.
    const M: i32 = 40;
    for &(cols, rows, cell_h) in &SIZES {
        let f = build_frame(cols, rows, cell_h, None);
        let iso = f.iso.expect("playable");
        let (w, h) = (f.fb.width as i32, f.fb.height as i32);
        let mut shifted = iso;
        shifted.origin = (iso.origin.0 + M as f32, iso.origin.1 + M as f32);
        let c = f.arena_size * 0.5;

        let mut floor = Framebuffer::new((w + 2 * M) as u32, (h + 2 * M) as u32);
        floor.clear(ui::HUD_BG);
        ui::draw_arena_floor(&mut floor, &shifted, (c, c), c);
        let mut all = Framebuffer::new(floor.width, floor.height);
        all.color.copy_from_slice(&floor.color);
        ui::draw_banners(&mut all, &shifted, (c, c), c, false);
        ui::draw_banners(&mut all, &shifted, (c, c), c, true);

        let (x0, y0, x1, y1) = painted_bbox(&all, ui::HUD_BG).unwrap();
        let tag = format!("{cols}x{rows}");
        assert!(x0 >= M && x1 < M + w, "{tag}: arena spills sideways {x0}..{x1}");
        assert!(y0 >= M + f.layout.top_px, "{tag}: arena reaches into the HUD at {}", y0 - M);
        assert!(y1 < M + h, "{tag}: arena spills off the bottom");

        let (fx0, _, fx1, _) = painted_bbox(&floor, ui::HUD_BG).unwrap();
        let wide = (fx1 - fx0 + 1) as f32 / (w - 4) as f32;
        let tall = (y1 - y0 + 1) as f32 / (h - f.layout.top_px - 4) as f32;
        assert!(
            wide.max(tall) >= 0.8,
            "{tag}: arena only covers {wide:.2} of the width and {tall:.2} of the height"
        );
    }
}

/// Walking out from the centre along any direction, the floor runs straight
/// into the border: the background is only ever entered from a border pixel,
/// and no floor shows up once the background has started.
#[test]
fn floor_meets_border() {
    #[derive(Clone, Copy, PartialEq, Debug)]
    enum Kind {
        Floor,
        Border,
        Background,
    }
    for &(cols, rows, cell_h) in &SIZES {
        let f = build_frame(cols, rows, cell_h, None);
        let iso = f.iso.unwrap();
        let mut fb = Framebuffer::new(f.fb.width, f.fb.height);
        fb.clear(ui::HUD_BG);
        let c = f.arena_size * 0.5;
        ui::draw_arena_floor(&mut fb, &iso, (c, c), c);

        let steps = 720;
        for k in 0..steps {
            let theta = k as f32 / steps as f32 * std::f32::consts::TAU;
            let max_r = c + WALL_BUMP_AMP + 4.0 / iso.scale;
            let dr = 0.05 / iso.scale;
            let mut prev = Kind::Floor;
            let mut seen_border = false;
            let mut seen_bg = false;
            let mut r = 0.0;
            let mut last = (i32::MIN, i32::MIN);
            while r < max_r {
                let (sx, sy) = iso.project2((c + theta.cos() * r, c + theta.sin() * r));
                let p = (sx.floor() as i32, sy.floor() as i32);
                r += dr;
                if p == last {
                    continue;
                }
                last = p;
                let Some(px) = pixel(&fb, p.0, p.1) else { continue };
                let kind = if same(px, ui::ARENA_BORDER) {
                    Kind::Border
                } else if same(px, ui::HUD_BG) {
                    Kind::Background
                } else {
                    Kind::Floor
                };
                let tag = format!("{cols}x{rows} theta={theta:.3} at {p:?}");
                match kind {
                    Kind::Border => seen_border = true,
                    Kind::Background => {
                        assert!(prev != Kind::Floor, "{tag}: background between floor and border");
                        seen_bg = true;
                    }
                    Kind::Floor => assert!(!seen_bg, "{tag}: floor outside the border"),
                }
                prev = kind;
            }
            assert!(seen_border && seen_bg, "{cols}x{rows} theta={theta:.3}: never crossed the wall");
        }
    }
}

/// Sliding a knight by sub-pixel steps never makes a part vanish.
#[test]
fn knight_parts_never_drop_out() {
    let p = &ui::PLAYER_PALETTE;
    let parts = [
        ("helmet", p.helmet),
        ("torso", p.knight),
        ("lance", ui::LANCE_WOOD),
        ("lance tip", ui::LANCE_TIP),
    ];
    let facings: Vec<(f32, f32)> = (0..16)
        .map(|i| {
            let a = i as f32 / 16.0 * std::f32::consts::TAU + 0.1;
            (a.cos(), a.sin())
        })
        .collect();
    for &(cols, rows, cell_h) in &[(80u16, 24u16, 16u16), (120, 40, 16), (200, 60, 8)] {
        let f = build_frame(cols, rows, cell_h, None);
        let iso = f.iso.unwrap();
        let s = f.arena_size;
        for &facing in &facings {
            let mut fb = Framebuffer::new(f.fb.width, f.fb.height);
            for i in 0..200 {
                // About three pixels of travel over the whole slide.
                let t = i as f32 / 200.0 * 3.0 / iso.scale;
                let mut c = Combatant::new((s * 0.45 + t, s * 0.5 + t * 0.37), facing, 3.5, 9.0);
                c.z = 0.0;
                fb.clear(Color::BLACK);
                ui::draw_combatant(&mut fb, &c, &iso, p);
                for (name, color) in parts {
                    assert!(
                        count(&fb, color) > 0,
                        "{cols}x{rows} facing {facing:?} step {i}: {name} vanished"
                    );
                }
            }
        }
    }
}

fn assert_inside(r: Rect, w: i32, h: i32, what: &str) {
    assert!(r.inside(w, h), "{what}: {r:?} is not inside {w}x{h}");
}

/// Menu, game over and victory keep all their text on screen at every
/// playable size, and the menu never stacks text on top of itself.
#[test]
fn overlays_stay_on_screen() {
    for (cols, rows, cell_h) in playable_sizes() {
        let layout = Layout::new(cols, rows, cell(cell_h), &hud());
        let (w, h) = (cols as i32, rows as i32 * 2);
        let tag = format!("{cols}x{rows}@{cell_h}");

        let menu = ui::menu_layout(w, h, 1, Difficulty::Normal, layout.ui_scale);
        let mut rects: Vec<Rect> = menu.lines.iter().map(|l| l.rect()).collect();
        if let Some(f) = &menu.footer {
            rects.push(f.rect());
        }
        if let Some(k) = menu.knight_rect() {
            rects.push(k);
        }
        for (i, r) in rects.iter().enumerate() {
            assert_inside(*r, w, h, &format!("{tag} menu item {i}"));
            for other in &rects[i + 1..] {
                assert!(!r.overlaps(other), "{tag} menu: {r:?} overlaps {other:?}");
            }
        }

        for title in ["UNHORSED", "INTO THE PIT"] {
            let over = ui::game_over_layout(w, h, title, 10, layout.ui_scale);
            for l in &over.lines {
                assert_inside(l.rect(), w, h, &format!("{tag} game over '{}'", l.text));
                if over.rect.inside(w, h) {
                    assert!(over.rect.contains(&l.rect()), "{tag}: '{}' outside its box", l.text);
                }
            }
        }

        let vic = ui::victory_layout(w, h, layout.top_px, layout.ui_scale);
        assert_inside(vic.outer(), w, h, &format!("{tag} victory scroll"));
        for l in &vic.lines {
            assert!(vic.scroll.contains(&l.rect()), "{tag}: '{}' outside the scroll", l.text);
        }
        let prompt = vic.lines.last().unwrap();
        assert!(prompt.text.starts_with("ENTER"), "{tag}: victory prompt missing");

        // The drawn pixels agree with the layouts: nothing lands outside the
        // rectangles the layout reports.
        let mut fb = Framebuffer::new(w as u32, h as u32);
        ui::draw_menu(&mut fb, 1, Difficulty::Normal, layout.ui_scale);
        assert_painted_within(&fb, &rects, &format!("{tag} menu"));

        fb.clear(ui::HUD_BG);
        ui::draw_game_over(&mut fb, "INTO THE PIT", 10, layout.ui_scale);
        let over = ui::game_over_layout(w, h, "INTO THE PIT", 10, layout.ui_scale);
        assert_painted_within(&fb, &[over.rect], &format!("{tag} game over"));

        fb.clear(ui::HUD_BG);
        ui::draw_victory(&mut fb, layout.ui_scale, layout.top_px);
        assert_painted_within(&fb, &[vic.outer()], &format!("{tag} victory"));
    }
}

/// Every pixel that differs from the background lies inside one of `rects`.
fn assert_painted_within(fb: &Framebuffer, rects: &[Rect], what: &str) {
    for y in 0..fb.height as i32 {
        for x in 0..fb.width as i32 {
            if same(pixel(fb, x, y).unwrap(), ui::HUD_BG) {
                continue;
            }
            let dot = Rect::new(x, y, 1, 1);
            assert!(
                rects.iter().any(|r| r.contains(&dot)),
                "{what}: pixel at {x},{y} is outside its layout"
            );
        }
    }
}

/// The menu footer shows up on the common small sizes instead of being
/// dropped, and the victory scroll keeps the arena visible around it.
#[test]
fn overlays_use_the_space_well() {
    for &(cols, rows, cell_h) in &SIZES {
        let layout = Layout::new(cols, rows, cell(cell_h), &hud());
        let (w, h) = (cols as i32, rows as i32 * 2);
        let menu = ui::menu_layout(w, h, 0, Difficulty::Normal, layout.ui_scale);
        assert!(menu.footer.is_some(), "{cols}x{rows}: menu footer dropped");
        let vic = ui::victory_layout(w, h, layout.top_px, layout.ui_scale);
        assert!(vic.outer().w < w, "{cols}x{rows}: scroll covers the full width");
    }
}

#[test]
fn hud_prefers_smaller_text_over_dropping_fields() {
    // 120 columns can't fit the full line in 5×5 but can in 3×5.
    let layout = Layout::new(120, 40, cell(16), &hud());
    assert_eq!(layout.hud_scale, ui::UiScale::SMALLEST);
    assert_eq!(layout.top_px, ui::hud_bar_height(layout.hud_scale) + 1);
    // Very wide terminals keep the large HUD.
    let layout = Layout::new(450, 130, cell(4), &hud());
    assert_eq!(layout.hud_scale, ui::UiScale { glyph_w: 5, cell_scale: 2 });
}

/// With the HUD height held fixed, a bigger framebuffer never gives a smaller
/// arena, and the size stays within its clamps.
#[test]
fn arena_size_grows_with_the_terminal() {
    for top in [8, 15] {
        for ys in [1.0f32, 0.8, 1.25] {
            let mut prev_row: Option<Vec<f32>> = None;
            for h in (30..=400).step_by(10) {
                let row: Vec<f32> = (40..=900)
                    .step_by(20)
                    .map(|w| arena::arena_size_for(w, h, top, ys))
                    .collect();
                for pair in row.windows(2) {
                    assert!(pair[1] >= pair[0], "size shrank going wider at h={h}");
                }
                if let Some(prev) = &prev_row {
                    for (a, b) in prev.iter().zip(&row) {
                        assert!(b >= a, "size shrank going taller at h={h}");
                    }
                }
                for &s in &row {
                    assert!((arena::MIN_ARENA_SIZE..=arena::MAX_ARENA_SIZE).contains(&s));
                    assert_eq!(s, s.round());
                }
                prev_row = Some(row);
            }
        }
    }
    assert_eq!(arena::arena_size_for(80, 48, 8, 1.0), arena::MIN_ARENA_SIZE);
    assert_eq!(arena::arena_size_for(4000, 2000, 8, 1.0), arena::MAX_ARENA_SIZE);
}

/// Resizing mid run keeps the arena size and only changes the view scale.
#[test]
fn locked_arena_size_rescales_on_resize() {
    let size = 90.0;
    let small = crate::render::iso::fit_arena(size, 120, 80, 8, 1.0);
    let big = crate::render::iso::fit_arena(size, 240, 160, 8, 1.0);
    assert!(big.scale > small.scale);
    // Shrinking far enough pauses the run rather than shrinking the world.
    assert!(arena::too_small(size, 60, 40, 8, 1.0));
}

/// Every character the UI can print exists in both bitmap fonts.
#[test]
fn ui_text_has_glyphs() {
    let mut strings: Vec<String> = vec![
        "KNIGHTTY".into(),
        "PLAY".into(),
        "QUIT".into(),
        "EASY".into(),
        "NORMAL".into(),
        "HARD".into(),
        "RESIZE TERMINAL".into(),
        "TO PLAY".into(),
        "UNHORSED".into(),
        "INTO THE PIT".into(),
        "DEFEATED".into(),
        "PIT!".into(),
        "SCORE 0123456789".into(),
        "LV  SCR  BST L S".into(),
    ];
    strings.extend(ui::MENU_FOOTERS.iter().map(|s| s.to_string()));
    strings.extend(ui::GAME_OVER_PROMPTS.iter().map(|s| s.to_string()));
    strings.extend(ui::VICTORY_PROMPTS.iter().map(|s| s.to_string()));
    for body in ui::VICTORY_TEXTS {
        strings.extend(body.iter().map(|s| s.to_string()));
    }
    for s in &strings {
        for ch in s.chars() {
            assert!(draw::glyph(ch).is_some(), "5x5 font has no '{ch}' (in \"{s}\")");
            assert!(draw::glyph_3x5(ch).is_some(), "3x5 font has no '{ch}' (in \"{s}\")");
        }
    }
}

fn write_ppm(fb: &Framebuffer, path: &std::path::Path) {
    let mut f = std::fs::File::create(path).unwrap();
    write!(f, "P6\n{} {}\n255\n", fb.width, fb.height).unwrap();
    let bytes: Vec<u8> = fb.color.iter().flat_map(|c| [c.r, c.g, c.b]).collect();
    f.write_all(&bytes).unwrap();
}

/// Writes arena, menu, game over and victory frames at every sample size as
/// PPM files into `$KNIGHTTY_DUMP_DIR`, for looking at by eye:
///
/// `KNIGHTTY_DUMP_DIR=/tmp/frames cargo test dump_frames -- --ignored`
#[test]
#[ignore]
fn dump_frames() {
    let dir = std::env::var("KNIGHTTY_DUMP_DIR").expect("set KNIGHTTY_DUMP_DIR");
    let dir = std::path::Path::new(&dir);
    std::fs::create_dir_all(dir).unwrap();
    for &(cols, rows, cell_h) in &SIZES {
        let name = |kind: &str| dir.join(format!("{kind}_{cols}x{rows}.ppm"));

        let f = build_frame(cols, rows, cell_h, None);
        write_ppm(&f.fb, &name("arena"));
        if let Some(iso) = f.iso {
            println!(
                "{cols}x{rows}: arena {} at {:.2} px per unit, hud {:?}",
                f.arena_size, iso.scale, f.layout.hud_scale
            );
        }

        let f = build_frame(cols, rows, cell_h, Some("DEFEATED"));
        write_ppm(&f.fb, &name("banner"));

        let mut menu = Framebuffer::new(cols as u32, rows as u32 * 2);
        ui::draw_menu(&mut menu, 0, Difficulty::Normal, f.layout.ui_scale);
        write_ppm(&menu, &name("menu"));

        let mut over = build_frame(cols, rows, cell_h, None);
        ui::draw_game_over(&mut over.fb, "INTO THE PIT", 3, over.layout.ui_scale);
        write_ppm(&over.fb, &name("over"));

        let mut vic = build_frame(cols, rows, cell_h, None);
        ui::draw_victory(&mut vic.fb, vic.layout.ui_scale, vic.layout.top_px);
        if let Some(iso) = vic.iso {
            let mut particles = Vec::new();
            crate::fx::spawn_firework(&mut particles, vic.arena_size);
            crate::fx::update_particles(&mut particles, 0.4);
            ui::draw_particles(&mut vic.fb, &iso, &particles);
        }
        write_ppm(&vic.fb, &name("victory"));
    }
}
