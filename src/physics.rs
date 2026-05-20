use crate::world::combatant::Combatant;

#[derive(Clone, Copy, Debug)]
pub enum ImpactKind {
    Body,
    Wall,
    Bumper,
}

#[derive(Clone, Copy, Debug)]
pub struct Impact {
    pub kind: ImpactKind,
    /// Closing velocity along the contact normal, in world units/s.
    pub speed: f32,
}

/// Closing velocity below this is treated as a sliding contact, not an impact:
/// no sound, no event. Prevents fatigue from a knight pressing against a wall.
pub const IMPACT_AUDIO_THRESHOLD: f32 = 8.0;

pub const TILT_GAIN: f32 = 90.0;       // pixels/s^2 at full tilt
pub const DRAG: f32 = 0.05;            // quadratic drag; lower means more momentum carry
pub const RESTITUTION: f32 = 0.65;     // wall bounce
pub const MIN_FACING_SPEED: f32 = 4.0; // below this, lance keeps last direction
pub const MIN_HIT_SPEED: f32 = 12.0;   // below this, lance contact doesn't count as a kill
pub const COMBATANT_BOUNCE: f32 = 0.6;
pub const GRAVITY: f32 = 70.0;         // world units / s^2
pub const JUMP_IMPULSE: f32 = 26.0;    // → ~0.74s airtime, ~4.8 unit peak height
pub const AIR_DRAG_SCALE: f32 = 0.4;   // less drag while airborne

// Round arena boundary: r(θ) = arena_radius + WALL_BUMP_AMP * sin(WALL_BUMP_N * θ)
pub const WALL_BUMP_AMP: f32 = 1.5;
pub const WALL_BUMP_N: f32 = 18.0;

pub fn step(
    c: &mut Combatant,
    tilt: (f32, f32),
    arena_size: f32,
    dt: f32,
    impacts: &mut Vec<Impact>,
) {
    // Clamp tilt to unit disk so diagonals aren't faster.
    let mag = (tilt.0 * tilt.0 + tilt.1 * tilt.1).sqrt();
    let (tx, ty) = if mag > 1.0 {
        (tilt.0 / mag, tilt.1 / mag)
    } else {
        tilt
    };

    // Quadratic drag always opposes velocity (sign of v * |v| matches v).
    // Reduced while airborne so a jump preserves momentum.
    let drag_k = if c.airborne() { DRAG * AIR_DRAG_SCALE } else { DRAG };
    let ax = TILT_GAIN * tx - drag_k * c.vel.0 * c.vel.0.abs();
    let ay = TILT_GAIN * ty - drag_k * c.vel.1 * c.vel.1.abs();

    c.vel.0 += ax * dt;
    c.vel.1 += ay * dt;

    c.pos.0 += c.vel.0 * dt;
    c.pos.1 += c.vel.1 * dt;

    // Vertical: ballistic with gravity. Land when z hits 0.
    if c.airborne() || c.vz > 0.0 {
        c.vz -= GRAVITY * dt;
        c.z += c.vz * dt;
        if c.z <= 0.0 {
            c.z = 0.0;
            c.vz = 0.0;
        }
    }

    // Round arena wall with a bumpy boundary `r(θ) = R + amp * sin(N*θ)`. The
    // bump gradient is folded into the collision normal so glancing hits
    // deflect at angles the player can read.
    let cx = arena_size * 0.5;
    let cy = arena_size * 0.5;
    let arena_r = arena_size * 0.5;
    let dxc = c.pos.0 - cx;
    let dyc = c.pos.1 - cy;
    let dist = (dxc * dxc + dyc * dyc).sqrt();
    if dist > 0.001 {
        let theta = dyc.atan2(dxc);
        let phase = WALL_BUMP_N * theta;
        let r_wall = arena_r + WALL_BUMP_AMP * phase.sin();
        let max_dist = r_wall - c.radius;
        if dist > max_dist {
            let dr_dtheta = WALL_BUMP_AMP * WALL_BUMP_N * phase.cos();
            let inv_r = 1.0 / r_wall.max(0.001);
            let nx_raw = theta.cos() - dr_dtheta * inv_r * theta.sin();
            let ny_raw = theta.sin() + dr_dtheta * inv_r * theta.cos();
            let nlen = (nx_raw * nx_raw + ny_raw * ny_raw).sqrt().max(0.001);
            let nx = nx_raw / nlen;
            let ny = ny_raw / nlen;
            // Snap inside along the radial direction.
            c.pos.0 = cx + theta.cos() * max_dist;
            c.pos.1 = cy + theta.sin() * max_dist;
            let vn = c.vel.0 * nx + c.vel.1 * ny;
            if vn > 0.0 {
                if vn > IMPACT_AUDIO_THRESHOLD {
                    impacts.push(Impact {
                        kind: ImpactKind::Wall,
                        speed: vn,
                    });
                }
                c.vel.0 -= (1.0 + RESTITUTION) * vn * nx;
                c.vel.1 -= (1.0 + RESTITUTION) * vn * ny;
            }
        }
    }

    c.update_facing(MIN_FACING_SPEED);
}

/// Resolve body-vs-body collision (impulse + positional separation).
/// Skipped when either combatant is airborne, since they pass over each other.
pub fn resolve_body_collision(a: &mut Combatant, b: &mut Combatant, impacts: &mut Vec<Impact>) {
    if a.airborne() || b.airborne() {
        return;
    }
    let dx = b.pos.0 - a.pos.0;
    let dy = b.pos.1 - a.pos.1;
    let dist2 = dx * dx + dy * dy;
    let min_d = a.radius + b.radius;
    if dist2 >= min_d * min_d || dist2 == 0.0 {
        return;
    }
    let dist = dist2.sqrt();
    let nx = dx / dist;
    let ny = dy / dist;
    let overlap = min_d - dist;
    a.pos.0 -= nx * overlap * 0.5;
    a.pos.1 -= ny * overlap * 0.5;
    b.pos.0 += nx * overlap * 0.5;
    b.pos.1 += ny * overlap * 0.5;

    let rvx = b.vel.0 - a.vel.0;
    let rvy = b.vel.1 - a.vel.1;
    let vn = rvx * nx + rvy * ny;
    if vn > 0.0 {
        return;
    }
    let closing = -vn;
    if closing > IMPACT_AUDIO_THRESHOLD {
        impacts.push(Impact {
            kind: ImpactKind::Body,
            speed: closing,
        });
    }
    let j = -(1.0 + COMBATANT_BOUNCE) * vn * 0.5;
    a.vel.0 -= j * nx;
    a.vel.1 -= j * ny;
    b.vel.0 += j * nx;
    b.vel.1 += j * ny;
}

/// Closest distance from segment (s0..s1) to point p.
fn segment_point_dist2(s0: (f32, f32), s1: (f32, f32), p: (f32, f32)) -> f32 {
    let dx = s1.0 - s0.0;
    let dy = s1.1 - s0.1;
    let len2 = dx * dx + dy * dy;
    if len2 < 1e-6 {
        let ex = p.0 - s0.0;
        let ey = p.1 - s0.1;
        return ex * ex + ey * ey;
    }
    let t = ((p.0 - s0.0) * dx + (p.1 - s0.1) * dy) / len2;
    let t = t.clamp(0.0, 1.0);
    let cx = s0.0 + dx * t;
    let cy = s0.1 + dy * t;
    let ex = p.0 - cx;
    let ey = p.1 - cy;
    ex * ex + ey * ey
}

/// True if `attacker`'s lance segment intersects `defender`'s body circle.
/// Either being airborne disables the hit. Lances are horizontal in the
/// ground plane, so a jumping defender goes over and a jumping attacker's
/// lance points high.
pub fn lance_hits(attacker: &Combatant, defender: &Combatant) -> bool {
    if attacker.airborne() || defender.airborne() {
        return false;
    }
    let s0 = attacker.pos;
    let s1 = attacker.lance_tip();
    let r = defender.radius;
    segment_point_dist2(s0, s1, defender.pos) <= r * r
}
