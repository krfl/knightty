use crate::physics::WALL_BUMP_AMP;
use crate::world::arena;

/// Cabinet isometric (2:1) projection.
///
/// World is right-handed: +X east on the board, +Y south on the board, +Z up.
/// Screen-x is `(wx - wy) * scale`, screen-y is
/// `((wx + wy) * 0.5 - wz * 0.5) * scale * y_scale`.
/// Origin is the screen position of world (0, 0, 0).
///
/// `y_scale` corrects for framebuffer pixels that are not square on screen
/// (a halfblock pixel is one cell wide and half a cell tall). It is 1.0 when
/// the pixels are square or the terminal does not report its cell size.
#[derive(Clone, Copy, Debug)]
pub struct Iso {
    pub origin: (f32, f32),
    pub scale: f32,
    pub y_scale: f32,
}

impl Iso {
    #[inline]
    pub fn project(&self, w: (f32, f32, f32)) -> (f32, f32) {
        let (wx, wy, wz) = w;
        (
            self.origin.0 + (wx - wy) * self.scale,
            self.origin.1 + ((wx + wy) * 0.5 - wz * 0.5) * self.scale * self.y_scale,
        )
    }

    #[inline]
    pub fn project2(&self, w: (f32, f32)) -> (f32, f32) {
        self.project((w.0, w.1, 0.0))
    }

    /// Inverse of `project` on the ground plane (z = 0): the world point that
    /// lands on screen position `s`.
    #[inline]
    pub fn unproject_ground(&self, s: (f32, f32)) -> (f32, f32) {
        let inv = 1.0 / self.scale.max(0.001);
        let diff = (s.0 - self.origin.0) * inv; // wx - wy
        let sum = 2.0 * (s.1 - self.origin.1) * inv / self.y_scale.max(0.001); // wx + wy
        ((sum + diff) * 0.5, (sum - diff) * 0.5)
    }

    /// Screen half width and half height of a world circle of radius `r`
    /// lying on the ground.
    #[inline]
    pub fn ground_ellipse(&self, r: f32) -> (f32, f32) {
        let half_w = r * self.scale * std::f32::consts::SQRT_2;
        (half_w, half_w * 0.5 * self.y_scale)
    }
}

/// Vertical correction for non-square framebuffer pixels, from the terminal
/// cell size in screen pixels. A halfblock pixel is `cell_w` wide and
/// `cell_h / 2` tall. The result is clamped to a sane range and snapped to
/// exactly 1.0 when it is close enough that the correction would only add
/// noise.
pub fn pixel_y_scale(cell_w: u16, cell_h: u16) -> f32 {
    if cell_w == 0 || cell_h == 0 {
        return 1.0;
    }
    let raw = (2.0 * cell_w as f32) / cell_h as f32;
    if (raw - 1.0).abs() <= 0.05 {
        1.0
    } else {
        raw.clamp(0.8, 1.25)
    }
}

/// Free space kept between the arena and the framebuffer edges, in pixels.
const FIT_PAD: f32 = 2.0;

/// Height in pixels that the banner poles and their finials reach above the
/// arena rim at a given scale. Shared with the banner drawing code so the fit
/// always leaves room for what is drawn.
pub fn banner_headroom_px(arena_size: f32, scale: f32, y_scale: f32) -> f32 {
    let pole_h = arena::banner_pole_height(arena_size * 0.5);
    (pole_h + arena::BANNER_FINIAL_UP) * 0.5 * scale * y_scale + arena::BANNER_FINIAL_RADIUS_PX
}

/// Compute the Iso that fits the arena as it is really drawn (the bumpy round
/// floor plus the banner poles above its back rim) into the framebuffer below
/// `top_px`, with a small pad on every side. The scale is the largest that
/// fits both ways; the arena is centred horizontally and the rim plus pole
/// headroom is centred vertically in the space below `top_px`.
pub fn fit_arena(arena_size: f32, fb_w: i32, fb_h: i32, top_px: i32, y_scale: f32) -> Iso {
    let ys = y_scale;
    let rim_r = arena_size * 0.5 + WALL_BUMP_AMP;
    let avail_w = (fb_w as f32 - 2.0 * FIT_PAD).max(0.0);
    let avail_h = ((fb_h - top_px) as f32 - 2.0 * FIT_PAD).max(0.0);

    // Per unit of scale: full rim width, full rim height and pole headroom.
    let rim_w = 2.0 * rim_r * std::f32::consts::SQRT_2;
    let rim_h = rim_w * 0.5 * ys;
    let pole_h = arena::banner_pole_height(arena_size * 0.5);
    let head = (pole_h + arena::BANNER_FINIAL_UP) * 0.5 * ys;

    let scale_w = avail_w / rim_w;
    let scale_h = (avail_h - arena::BANNER_FINIAL_RADIUS_PX).max(0.0) / (rim_h + head);
    let scale = scale_w.min(scale_h).max(0.01);

    let headroom = banner_headroom_px(arena_size, scale, ys);
    let content_h = headroom + rim_h * scale;
    let top = top_px as f32 + ((fb_h - top_px) as f32 - content_h) * 0.5;
    let center_y = top + headroom + rim_h * scale * 0.5;
    // World centre (R, R) projects to origin + (0, R * scale * ys).
    let origin_y = center_y - arena_size * 0.5 * scale * ys;
    Iso {
        origin: (fb_w as f32 * 0.5, origin_y),
        scale,
        y_scale: ys,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ground_round_trip() {
        for &ys in &[1.0f32, 0.8, 1.17, 1.25] {
            let iso = Iso { origin: (37.5, -12.25), scale: 1.7, y_scale: ys };
            for &(wx, wy) in &[(0.0f32, 0.0f32), (10.0, 3.5), (-4.0, 77.25), (120.0, 119.0)] {
                let s = iso.project2((wx, wy));
                let (bx, by) = iso.unproject_ground(s);
                assert!((bx - wx).abs() < 1e-3 && (by - wy).abs() < 1e-3, "{ys}: {wx},{wy} -> {bx},{by}");
            }
        }
    }

    #[test]
    fn y_scale_snaps_and_clamps() {
        assert_eq!(pixel_y_scale(8, 16), 1.0);
        assert_eq!(pixel_y_scale(33, 64), 1.0); // within 5%
        assert_eq!(pixel_y_scale(0, 0), 1.0);
        assert_eq!(pixel_y_scale(10, 10), 1.25);
        assert_eq!(pixel_y_scale(4, 16), 0.8);
        assert!((pixel_y_scale(9, 16) - 1.125).abs() < 1e-6);
    }

    #[test]
    fn fit_keeps_rim_and_poles_inside() {
        for &(w, h, top) in &[(80, 48, 8), (120, 80, 8), (450, 260, 15), (300, 60, 8)] {
            for &size in &[60.0f32, 100.0, 200.0] {
                for &ys in &[1.0f32, 0.8, 1.25] {
                    let iso = fit_arena(size, w, h, top, ys);
                    let c = size * 0.5;
                    let (hw, hh) = iso.ground_ellipse(c + WALL_BUMP_AMP);
                    let cs = iso.project2((c, c));
                    let head = banner_headroom_px(size, iso.scale, ys);
                    let eps = 1e-3;
                    assert!(cs.0 - hw >= FIT_PAD - eps && cs.0 + hw <= w as f32 - FIT_PAD + eps);
                    assert!(cs.1 - hh - head >= top as f32 + FIT_PAD - eps, "{w}x{h} {size} {ys}");
                    assert!(cs.1 + hh <= h as f32 - FIT_PAD + eps, "{w}x{h} {size} {ys}");
                }
            }
        }
    }
}
