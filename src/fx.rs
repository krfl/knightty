use rand::Rng;

use crate::render::framebuffer::Color;

#[derive(Clone, Copy)]
pub struct Particle {
    pub pos: (f32, f32, f32),
    pub vel: (f32, f32, f32),
    pub color: Color,
    pub age: f32,
    pub lifetime: f32,
}

const FIREWORK_COLORS: &[Color] = &[
    Color::new(255, 95, 95),
    Color::new(255, 215, 80),
    Color::new(80, 180, 255),
    Color::new(120, 240, 130),
    Color::new(255, 160, 220),
    Color::new(255, 250, 200),
];

const PARTICLE_COUNT: usize = 36;
const GRAVITY: f32 = 14.0;
const PARTICLE_CAP: usize = 600;

pub fn spawn_firework(particles: &mut Vec<Particle>, arena_size: f32) {
    let mut rng = rand::rng();
    let cx = arena_size * 0.5;
    let cy = arena_size * 0.5;
    let r = arena_size * 0.42;
    let bx = cx + rng.random_range(-r..r);
    let by = cy + rng.random_range(-r..r);
    let bz = rng.random_range(arena_size * 0.20..arena_size * 0.34);
    let color = FIREWORK_COLORS[rng.random_range(0..FIREWORK_COLORS.len())];
    let speed = rng.random_range(8.0..14.0);

    for _ in 0..PARTICLE_COUNT {
        // Uniform random on a hemisphere (dz >= 0) so the burst rises before
        // gravity pulls it down.
        let theta = rng.random_range(0.0..std::f32::consts::TAU);
        let u = rng.random_range(-1.0f32..1.0);
        let r_xy = (1.0 - u * u).sqrt();
        let dx = r_xy * theta.cos();
        let dy = r_xy * theta.sin();
        let dz = u.abs();
        let speed_var = rng.random_range(0.7..1.0);
        let vx = dx * speed * speed_var;
        let vy = dy * speed * speed_var;
        let vz = dz * speed * speed_var * 1.3;

        particles.push(Particle {
            pos: (bx, by, bz),
            vel: (vx, vy, vz),
            color,
            age: 0.0,
            lifetime: rng.random_range(0.9..1.5),
        });
    }

    if particles.len() > PARTICLE_CAP {
        let excess = particles.len() - PARTICLE_CAP;
        particles.drain(0..excess);
    }
}

pub fn update_particles(particles: &mut Vec<Particle>, dt: f32) {
    for p in particles.iter_mut() {
        p.pos.0 += p.vel.0 * dt;
        p.pos.1 += p.vel.1 * dt;
        p.pos.2 += p.vel.2 * dt;
        p.vel.2 -= GRAVITY * dt;
        p.age += dt;
    }
    particles.retain(|p| p.age < p.lifetime);
}
