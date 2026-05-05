/// Cabinet isometric (2:1) projection.
///
/// World is right-handed: +X east on the board, +Y south on the board, +Z up.
/// Screen-x is `(wx - wy) * scale`, screen-y is `(wx + wy) * scale * 0.5 - wz * scale * 0.5`.
/// Origin is the screen position of world (0, 0, 0).
#[derive(Clone, Copy)]
pub struct Iso {
    pub origin: (f32, f32),
    pub scale: f32,
}

impl Iso {
    #[inline]
    pub fn project(&self, w: (f32, f32, f32)) -> (f32, f32) {
        let (wx, wy, wz) = w;
        (
            self.origin.0 + (wx - wy) * self.scale,
            self.origin.1 + (wx + wy) * self.scale * 0.5 - wz * self.scale * 0.5,
        )
    }

    #[inline]
    pub fn project2(&self, w: (f32, f32)) -> (f32, f32) {
        self.project((w.0, w.1, 0.0))
    }
}

/// Compute an Iso that fits an `arena_size` square diamond (2:1 wide) into the
/// available framebuffer area below `hud_h`.
pub fn fit_arena(arena_size: f32, fb_w: i32, fb_h: i32, hud_h: i32) -> Iso {
    let avail_w = fb_w as f32;
    let avail_h = (fb_h - hud_h).max(1) as f32;
    // Diamond: 2*S wide, 1*S tall in scale=1 units (margin to leave room for
    // jump arcs + lance reach extending past the corners).
    let pad = 4.0;
    let scale = ((avail_w - pad) / (2.0 * arena_size))
        .min((avail_h - pad) / (arena_size + 12.0));
    let diamond_h = arena_size * scale;
    let cx = avail_w * 0.5;
    let cy = hud_h as f32 + (avail_h - diamond_h) * 0.5;
    Iso {
        origin: (cx, cy),
        scale,
    }
}
