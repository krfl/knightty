use rand::Rng;

use crate::physics::{IMPACT_AUDIO_THRESHOLD, Impact, ImpactKind};
use crate::world::combatant::Combatant;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Difficulty {
    Easy,
    Normal,
    Hard,
}

impl Difficulty {
    pub fn factor(self) -> u32 {
        match self {
            Difficulty::Easy => 1,
            Difficulty::Normal => 2,
            Difficulty::Hard => 3,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Difficulty::Easy => "EASY",
            Difficulty::Normal => "NORMAL",
            Difficulty::Hard => "HARD",
        }
    }

    pub fn cycle(self) -> Self {
        match self {
            Difficulty::Easy => Difficulty::Normal,
            Difficulty::Normal => Difficulty::Hard,
            Difficulty::Hard => Difficulty::Easy,
        }
    }
}

/// Number of hazards: floor(level * difficulty * 0.5).
pub fn hazard_count(level: u32, difficulty: Difficulty) -> usize {
    (level * difficulty.factor() / 2) as usize
}

#[derive(Clone, Copy, Debug)]
pub enum HazardKind {
    Pit,
    Bumper,
}

#[derive(Clone, Copy, Debug)]
pub struct Hazard {
    pub pos: (f32, f32),
    pub radius: f32,
    pub kind: HazardKind,
}

/// Pick hazards that don't overlap each other or the spawn points.
pub fn spawn_hazards(
    arena_size: f32,
    count: usize,
    knight_pos: (f32, f32),
    opp_pos: (f32, f32),
) -> Vec<Hazard> {
    let mut rng = rand::rng();
    let max = count.min(12);
    let mut out: Vec<Hazard> = Vec::with_capacity(max);
    let margin = 6.0;
    let player_clear = 14.0;

    let cx = arena_size * 0.5;
    let cy = arena_size * 0.5;
    let arena_r = arena_size * 0.5;

    'outer: for _ in 0..max {
        for _ in 0..50 {
            let kind = if rng.random_bool(0.5) {
                HazardKind::Pit
            } else {
                HazardKind::Bumper
            };
            let radius = match kind {
                HazardKind::Pit => rng.random_range(3.0..5.0),
                HazardKind::Bumper => rng.random_range(2.5..4.0),
            };
            let x = rng.random_range(margin + radius..arena_size - margin - radius);
            let y = rng.random_range(margin + radius..arena_size - margin - radius);

            // Constrain to the round arena.
            let dx = x - cx;
            let dy = y - cy;
            let max_d = arena_r - radius - margin;
            if dx * dx + dy * dy > max_d * max_d {
                continue;
            }
            // Keep clear of starting positions.
            if dist2((x, y), knight_pos) < player_clear * player_clear
                || dist2((x, y), opp_pos) < player_clear * player_clear
            {
                continue;
            }
            // Keep clear of other hazards.
            if out
                .iter()
                .any(|h| dist2((x, y), h.pos) < (h.radius + radius + 2.0).powi(2))
            {
                continue;
            }
            out.push(Hazard {
                pos: (x, y),
                radius,
                kind,
            });
            continue 'outer;
        }
        // Couldn't place this one; give up on the slot.
    }
    out
}

#[inline]
fn dist2(a: (f32, f32), b: (f32, f32)) -> f32 {
    let dx = a.0 - b.0;
    let dy = a.1 - b.1;
    dx * dx + dy * dy
}

/// Returns true if the combatant has fallen into a pit (mostly over it).
/// Airborne combatants jump clear.
pub fn check_pit(c: &Combatant, hazards: &[Hazard]) -> bool {
    if c.airborne() {
        return false;
    }
    for h in hazards {
        if matches!(h.kind, HazardKind::Pit)
            && dist2(c.pos, h.pos) < (h.radius - c.radius * 0.4).max(0.0).powi(2)
        {
            return true;
        }
    }
    false
}

/// Resolve bumper collisions for one combatant. Bumpers are static, so we push
/// the combatant out and reflect velocity with restitution > 1 for a kick.
/// Airborne combatants pass over bumpers.
pub fn resolve_bumpers(c: &mut Combatant, hazards: &[Hazard], impacts: &mut Vec<Impact>) {
    if c.airborne() {
        return;
    }
    const BUMPER_RESTITUTION: f32 = 1.15;
    for h in hazards {
        if !matches!(h.kind, HazardKind::Bumper) {
            continue;
        }
        let dx = c.pos.0 - h.pos.0;
        let dy = c.pos.1 - h.pos.1;
        let d2 = dx * dx + dy * dy;
        let min_d = h.radius + c.radius;
        if d2 >= min_d * min_d || d2 == 0.0 {
            continue;
        }
        let d = d2.sqrt();
        let nx = dx / d;
        let ny = dy / d;
        c.pos.0 = h.pos.0 + nx * min_d;
        c.pos.1 = h.pos.1 + ny * min_d;
        let vn = c.vel.0 * nx + c.vel.1 * ny;
        if vn < 0.0 {
            let closing = -vn;
            if closing > IMPACT_AUDIO_THRESHOLD {
                impacts.push(Impact {
                    kind: ImpactKind::Bumper,
                    speed: closing,
                });
            }
            c.vel.0 -= (1.0 + BUMPER_RESTITUTION) * vn * nx;
            c.vel.1 -= (1.0 + BUMPER_RESTITUTION) * vn * ny;
        }
    }
}
