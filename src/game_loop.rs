use std::io::Stdout;
use std::time::{Duration, Instant};

use crossterm::event::KeyCode;
use rand::Rng;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::app::{App, AppState, MENU_OPTIONS, MenuOption};
use crate::fx;
use crate::input::spawn_input_thread;
use crate::physics::{self, MIN_HIT_SPEED};
use crate::render::terminal::{JoustView, PX_PER_COL, PX_PER_ROW, detect_cell_pixel_size};
use crate::ui::{self, ArenaView, HudView};
use crate::world::arena::{self, ArenaTier};
use crate::world::combatant::{Combatant, OpponentMode};
use crate::world::hazard::{self, Difficulty};

const PHYSICS_HZ: f32 = 240.0;
const PHYSICS_DT: f32 = 1.0 / PHYSICS_HZ;
const MAX_FRAME_TIME: f32 = 0.1;
const TARGET_FPS: u64 = 24;
/// Score required to win the tourney.
const WIN_LEVEL: u32 = 10;

pub fn run(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
) -> std::io::Result<()> {
    let mut accumulator = 0.0f32;
    let mut last_time = Instant::now();
    let frame_duration = Duration::from_millis(1000 / TARGET_FPS);
    // Tracks airborne state across frames so we can detect the
    // airborne→grounded transition and play the land sound exactly once.
    let mut knight_was_airborne = false;
    let mut last_size = ratatui::layout::Size { width: 80, height: 24 };

    let _input_handle = spawn_input_thread(app.input.shared.clone());

    loop {
        let now = Instant::now();
        let frame_time = now
            .duration_since(last_time)
            .as_secs_f32()
            .min(MAX_FRAME_TIME);
        last_time = now;
        // Only advance the physics accumulator while in Playing — otherwise
        // time spent in menus/game-over/victory would queue up phantom ticks
        // that all fire when the next round starts (opponent charges and
        // kills the knight before the player has even taken a frame to react).
        if matches!(app.state, AppState::Playing) {
            accumulator += frame_time;
        } else {
            accumulator = 0.0;
        }

        app.input.sync_from_shared();
        // Ctrl+C is the universal exit — we route it through SharedInput
        // since raw mode swallows SIGINT.
        if app.input.shared.lock().unwrap().exit_requested {
            app.running = false;
            break;
        }

        // Mid-frame font/window resizes on macOS can briefly return ENXIO
        // ("Device not configured") from TIOCGWINSZ. Treat any transient I/O
        // failure as "use last known size" rather than crashing the game.
        let size = match terminal.size() {
            Ok(s) => {
                last_size = s;
                s
            }
            Err(_) => last_size,
        };
        resize_framebuffer(app, size.width, size.height);

        // When the terminal is too small to render the arena, pause the
        // simulation entirely — physics shouldn't keep running while the
        // player can't see what's happening.
        let paused_for_size = matches!(app.tier, ArenaTier::BelowMin)
            && matches!(app.state, AppState::Playing);
        if paused_for_size {
            accumulator = 0.0;
        }

        match app.state {
            AppState::Menu => handle_menu_input(app),
            AppState::Playing => {
                handle_playing_input(app);
                if !paused_for_size {
                    update_playing(app, frame_time, &mut accumulator);
                }
            }
            AppState::GameOver => handle_game_over_input(app),
            AppState::Victory => {
                handle_victory_input(app);
                update_victory(app, frame_time);
            }
        }
        if !app.running {
            break;
        }

        // Audio ticks in every state — music continues across menu/game-over;
        // gallop fades naturally because it scales with knight speed (and we
        // force speed=0 while airborne so hoofbeats stop mid-jump).
        let in_play = matches!(app.state, AppState::Playing);
        let knight_airborne = in_play && app.knight.airborne();
        if knight_was_airborne && !knight_airborne && in_play {
            if let Some(s) = &app.sound {
                s.play_land();
            }
        }
        knight_was_airborne = knight_airborne;

        let speed = if in_play && !knight_airborne {
            app.knight.speed()
        } else {
            0.0
        };
        if let Some(s) = &mut app.sound {
            s.update(frame_time, speed);
        }

        // Render everything into the framebuffer — single coordinate system,
        // single visual style. UI scale picked from current terminal cells
        // *and* the actual cell pixel size, so absolute readability stays
        // consistent across font sizes.
        let (_cell_w, cell_h) = detect_cell_pixel_size();
        let ui_scale = ui::pick_ui_scale(size.width, size.height, cell_h);
        // HUD scale capped one tier below menu/overlays so it doesn't
        // dominate the screen at very wide terminals.
        let hud_scale = ui::UiScale {
            glyph_w: ui_scale.glyph_w,
            cell_scale: ui_scale.cell_scale.min(2),
        };

        match app.state {
            AppState::Menu => {
                ui::draw_menu(
                    &mut app.framebuffer,
                    app.menu_selection,
                    app.difficulty,
                    ui_scale,
                );
            }
            AppState::Playing | AppState::GameOver | AppState::Victory => {
                if matches!(app.tier, ArenaTier::BelowMin) {
                    ui::draw_below_min(&mut app.framebuffer);
                } else {
                    let banner_owned = app.banner.as_ref().map(|(s, _)| s.clone());
                    let arena_view = ArenaView {
                        tier: app.tier,
                        knight: &app.knight,
                        opponent: &app.opponent,
                        tilt: app.input.tilt,
                        hazards: &app.hazards,
                        particles: &app.particles,
                        banner: banner_owned.as_deref(),
                    };
                    ui::draw_background(&mut app.framebuffer);
                    ui::draw_arena(&mut app.framebuffer, &arena_view, hud_scale);
                    let hud = HudView {
                        score: app.score,
                        best: app.best,
                        difficulty: app.difficulty,
                        level: app.level(),
                    };
                    ui::draw_hud(&mut app.framebuffer, &hud, hud_scale);
                    if matches!(app.state, AppState::GameOver) {
                        ui::draw_game_over(
                            &mut app.framebuffer,
                            &app.death_message,
                            app.score,
                            ui_scale,
                        );
                    }
                    if matches!(app.state, AppState::Victory) {
                        ui::draw_victory(&mut app.framebuffer, ui_scale);
                    }
                }
            }
        }

        // Same rationale as the size query: a mid-frame resize can fail the
        // draw call transiently. Drop the frame and retry next loop instead
        // of exiting the game.
        let _ = present(terminal, app);

        let elapsed = last_time.elapsed();
        if elapsed < frame_duration {
            std::thread::sleep(frame_duration - elapsed);
        }
    }

    Ok(())
}

fn resize_framebuffer(app: &mut App, term_cols: u16, term_rows: u16) {
    // Framebuffer covers the entire terminal — HUD and overlays draw into it
    // alongside the arena, so there's no separate ratatui-text row to reserve.
    let fb_w = (term_cols as u32).max(1) * PX_PER_COL;
    let fb_h = (term_rows as u32).max(1) * PX_PER_ROW;
    if fb_w != app.framebuffer.width || fb_h != app.framebuffer.height {
        app.framebuffer.resize(fb_w, fb_h);
    }
    app.tier = arena::pick_tier(term_cols, term_rows);
}

fn present(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
) -> std::io::Result<()> {
    terminal.draw(|f| {
        let area = f.area();
        let view = JoustView {
            framebuffer: &app.framebuffer,
        };
        f.render_widget(view, area);
    })?;
    Ok(())
}

fn handle_menu_input(app: &mut App) {
    // ESC and 'q' deliberately don't quit from the menu — the only ways out
    // are the QUIT menu option or Ctrl+C. Keeps a stray ESC after a joust
    // round from accidentally exiting the whole program.
    for key in app.input.pressed.clone() {
        match key {
            KeyCode::Up | KeyCode::Char('w') => {
                if app.menu_selection > 0 {
                    app.menu_selection -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('s') => {
                if app.menu_selection + 1 < MENU_OPTIONS.len() {
                    app.menu_selection += 1;
                }
            }
            KeyCode::Enter | KeyCode::Char(' ') => activate_menu(app),
            _ => {}
        }
    }
}

fn activate_menu(app: &mut App) {
    match MENU_OPTIONS[app.menu_selection] {
        MenuOption::Play => start_playing(app),
        MenuOption::Difficulty => {
            app.difficulty = app.difficulty.cycle();
        }
        MenuOption::Quit => app.running = false,
    }
}

fn start_playing(app: &mut App) {
    app.state = AppState::Playing;
    app.score = 0;
    app.round_time = 0.0;
    app.banner = None;
    spawn_round(app, true);
}

fn spawn_round(app: &mut App, knight_too: bool) {
    let s = app.tier.size();
    let mut rng = rand::rng();
    if knight_too {
        app.knight = Combatant::new((s * 0.2, s * 0.5), (1.0, 0.0), 3.5, 9.0);
    }

    // Spawn diametrically opposite the knight from arena center.
    let cx = s * 0.5;
    let cy = s * 0.5;
    let kx = app.knight.pos.0;
    let ky = app.knight.pos.1;
    let dx = kx - cx;
    let dy = ky - cy;
    let mag = (dx * dx + dy * dy).sqrt();
    let spawn_r = s * 0.42;
    let (op_x, op_y) = if mag < s * 0.04 {
        let a = rng.random_range(0.0..std::f32::consts::TAU);
        (cx + a.cos() * spawn_r, cy + a.sin() * spawn_r)
    } else {
        let nx = -dx / mag;
        let ny = -dy / mag;
        let j = rng.random_range(-0.45f32..0.45);
        let (cs, sn) = (j.cos(), j.sin());
        (cx + (nx * cs - ny * sn) * spawn_r, cy + (nx * sn + ny * cs) * spawn_r)
    };
    let margin = s * 0.08;
    let op_x = op_x.clamp(margin, s - margin);
    let op_y = op_y.clamp(margin, s - margin);
    let fdx = kx - op_x;
    let fdy = ky - op_y;
    let fd = (fdx * fdx + fdy * fdy).sqrt().max(1.0);
    let facing = (fdx / fd, fdy / fd);
    app.opponent = Combatant::new((op_x, op_y), facing, 3.5, 9.0);

    let count = hazard::hazard_count(app.level(), app.difficulty);
    app.hazards = hazard::spawn_hazards(s, count, app.knight.pos, app.opponent.pos);

    // Mark the spawn time so the AI's tilt ramps up gradually — gives the
    // player a beat to read the new opponent before they're being charged.
    app.opponent_spawn_at = app.round_time;

    // The new challenger's horse announces its arrival.
    if let Some(s) = &app.sound {
        s.play_neigh();
    }
}

/// 0..1 grace factor for a freshly-spawned opponent. Scales their AI tilt so
/// they ease into the round instead of charging at full speed instantly.
const OPPONENT_GRACE_SEC: f32 = 0.6;
fn opponent_grace(app: &App) -> f32 {
    let age = (app.round_time - app.opponent_spawn_at).max(0.0);
    (age / OPPONENT_GRACE_SEC).clamp(0.0, 1.0)
}

/// AI tilt magnitude scales with difficulty.
fn ai_tilt_strength(d: Difficulty) -> f32 {
    match d {
        Difficulty::Easy => 0.35,
        Difficulty::Normal => 0.55,
        Difficulty::Hard => 0.80,
    }
}

fn handle_playing_input(app: &mut App) {
    for key in app.input.pressed.clone() {
        match key {
            KeyCode::Esc => {
                app.state = AppState::Menu;
            }
            KeyCode::Char(' ') => {
                app.knight.try_jump(physics::JUMP_IMPULSE);
            }
            KeyCode::Char('m') => {
                app.opponent_mode = app.opponent_mode.cycle();
            }
            _ => {}
        }
    }
}

fn update_playing(app: &mut App, frame_time: f32, accumulator: &mut f32) {
    app.input.update_tilt(frame_time);
    app.round_time += frame_time;

    if let Some((_, t)) = &mut app.banner {
        *t -= frame_time;
        if *t <= 0.0 {
            app.banner = None;
        }
    }

    while *accumulator >= PHYSICS_DT {
        physics_tick(app, PHYSICS_DT);
        *accumulator -= PHYSICS_DT;
    }
}

fn physics_tick(app: &mut App, dt: f32) {
    // Once a round ends mid-frame, the accumulator can fire another tick on the
    // same overlapping knights — re-triggering death/impact sounds. Bail.
    if !matches!(app.state, AppState::Playing) {
        return;
    }
    let arena_size = app.tier.size();

    // Rotate the player's input tilt by 45° so screen-up actually moves the
    // knight toward screen-up in iso (world NW), matching what the player sees.
    // AI stays world-aligned because it reasons in world coords directly.
    let raw = app.input.tilt;
    let inv_sqrt2 = 1.0 / std::f32::consts::SQRT_2;
    let knight_tilt = (
        (raw.0 + raw.1) * inv_sqrt2,
        (-raw.0 + raw.1) * inv_sqrt2,
    );

    let opp_tilt = match app.opponent_mode {
        OpponentMode::Static => (0.0, 0.0),
        OpponentMode::TiltShared => knight_tilt,
        OpponentMode::FreeAi => {
            let (tx, ty) = ai_tilt(&app.opponent, &app.knight, &app.hazards, app.round_time);
            let strength = ai_tilt_strength(app.difficulty) * opponent_grace(app);
            (tx * strength, ty * strength)
        }
    };

    let mut impacts: Vec<physics::Impact> = Vec::new();

    physics::step(&mut app.knight, knight_tilt, arena_size, dt, &mut impacts);
    if !matches!(app.opponent_mode, OpponentMode::Static) {
        physics::step(&mut app.opponent, opp_tilt, arena_size, dt, &mut impacts);
    } else {
        let dx = app.knight.pos.0 - app.opponent.pos.0;
        let dy = app.knight.pos.1 - app.opponent.pos.1;
        let d = (dx * dx + dy * dy).sqrt().max(1e-3);
        app.opponent.facing = (dx / d, dy / d);
    }

    hazard::resolve_bumpers(&mut app.knight, &app.hazards, &mut impacts);
    hazard::resolve_bumpers(&mut app.opponent, &app.hazards, &mut impacts);
    physics::resolve_body_collision(&mut app.knight, &mut app.opponent, &mut impacts);

    if !impacts.is_empty() {
        if let Some(s) = &mut app.sound {
            for imp in &impacts {
                s.play_impact(imp.kind, imp.speed);
            }
        }
    }

    if hazard::check_pit(&app.knight, &app.hazards) {
        if let Some(s) = &app.sound {
            s.play_fall();
        }
        app.banner = Some(("INTO THE PIT".to_string(), 1.5));
        app.death_message = "INTO THE PIT".to_string();
        app.state = AppState::GameOver;
        return;
    }
    if hazard::check_pit(&app.opponent, &app.hazards) {
        if let Some(s) = &app.sound {
            s.play_fall();
        }
        award_round(app, "PIT!");
        return;
    }

    let knight_hits = physics::lance_hits(&app.knight, &app.opponent)
        && app.knight.speed() > MIN_HIT_SPEED;
    let opp_hits = physics::lance_hits(&app.opponent, &app.knight)
        && app.opponent.speed() > MIN_HIT_SPEED;

    match (knight_hits, opp_hits) {
        (true, false) => {
            if let Some(s) = &app.sound {
                s.play_death();
            }
            award_round(app, "DEFEATED");
        }
        (false, true) => {
            if let Some(s) = &app.sound {
                s.play_death();
            }
            if app.score > app.best {
                app.best = app.score;
            }
            app.banner = Some(("UNHORSED".to_string(), 1.5));
            app.death_message = "UNHORSED".to_string();
            app.state = AppState::GameOver;
        }
        _ => {}
    }
}

fn ai_tilt(
    self_c: &Combatant,
    target: &Combatant,
    hazards: &[hazard::Hazard],
    t: f32,
) -> (f32, f32) {
    let dx = target.pos.0 - self_c.pos.0;
    let dy = target.pos.1 - self_c.pos.1;
    let d = (dx * dx + dy * dy).sqrt().max(1e-3);
    let bob = (t * 1.7).sin() * 0.3;
    let perpx = -dy / d;
    let perpy = dx / d;
    let mut tx = dx / d + perpx * bob;
    let mut ty = dy / d + perpy * bob;

    for h in hazards {
        if !matches!(h.kind, hazard::HazardKind::Pit) {
            continue;
        }
        let rx = self_c.pos.0 - h.pos.0;
        let ry = self_c.pos.1 - h.pos.1;
        let rd = (rx * rx + ry * ry).sqrt().max(1e-3);
        let danger = h.radius + self_c.radius + 6.0;
        if rd < danger {
            let strength = (danger - rd) / danger;
            tx += (rx / rd) * strength * 2.0;
            ty += (ry / rd) * strength * 2.0;
        }
    }
    let mag = (tx * tx + ty * ty).sqrt().max(1e-3);
    (tx / mag, ty / mag)
}

/// Increment the score, then either spawn the next round or trigger victory
/// once the player has cleared `WIN_LEVEL` opponents.
fn award_round(app: &mut App, banner_text: &str) {
    app.score += 1;
    if app.score > app.best {
        app.best = app.score;
    }
    if app.score >= WIN_LEVEL {
        app.banner = None;
        app.state = AppState::Victory;
        app.victory_t = 0.0;
        app.next_firework_t = 0.1;
        app.particles.clear();
        if let Some(s) = &mut app.sound {
            s.play_fanfare();
        }
    } else {
        app.banner = Some((banner_text.to_string(), 1.0));
        spawn_round(app, false);
    }
}

fn update_victory(app: &mut App, frame_time: f32) {
    app.victory_t += frame_time;
    fx::update_particles(&mut app.particles, frame_time);

    if app.victory_t >= app.next_firework_t {
        let s = app.tier.size();
        fx::spawn_firework(&mut app.particles, s);
        let interval = rand::rng().random_range(0.4f32..1.2);
        app.next_firework_t = app.victory_t + interval;
    }
}

fn handle_victory_input(app: &mut App) {
    for key in app.input.pressed.clone() {
        match key {
            KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Esc => {
                app.state = AppState::Menu;
                app.particles.clear();
                app.banner = None;
            }
            _ => {}
        }
    }
}

fn handle_game_over_input(app: &mut App) {
    for key in app.input.pressed.clone() {
        match key {
            KeyCode::Char('r') | KeyCode::Enter | KeyCode::Char(' ') => {
                start_playing(app);
            }
            KeyCode::Esc => {
                app.state = AppState::Menu;
            }
            _ => {}
        }
    }
}
