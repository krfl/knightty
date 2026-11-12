use crate::render::framebuffer::{Color, Framebuffer};

pub fn fill_rect(fb: &mut Framebuffer, x: i32, y: i32, w: i32, h: i32, c: Color) {
    for j in y..(y + h) {
        for i in x..(x + w) {
            fb.set(i, j, c);
        }
    }
}

#[allow(dead_code)]
pub fn rect_outline(fb: &mut Framebuffer, x: i32, y: i32, w: i32, h: i32, c: Color) {
    for i in x..(x + w) {
        fb.set(i, y, c);
        fb.set(i, y + h - 1, c);
    }
    for j in y..(y + h) {
        fb.set(x, j, c);
        fb.set(x + w - 1, j, c);
    }
}

/// Radius below which a disc might not cover any pixel centre. A disc this
/// small always paints the pixel holding its centre so it never vanishes.
const TINY_RADIUS: f32 = 0.71;

pub fn filled_circle(fb: &mut Framebuffer, cx: f32, cy: f32, r: f32, c: Color) {
    let r2 = r * r;
    let x0 = (cx - r).floor() as i32;
    let x1 = (cx + r).ceil() as i32;
    let y0 = (cy - r).floor() as i32;
    let y1 = (cy + r).ceil() as i32;
    for j in y0..=y1 {
        for i in x0..=x1 {
            let dx = i as f32 + 0.5 - cx;
            let dy = j as f32 + 0.5 - cy;
            if dx * dx + dy * dy <= r2 {
                fb.set(i, j, c);
            }
        }
    }
    if r < TINY_RADIUS {
        fb.set(cx.floor() as i32, cy.floor() as i32, c);
    }
}

/// Fill a segment thickened into a capsule whose radius tapers linearly from
/// `w0/2` at the start to `w1/2` at the end. Single bounding-box pass with no
/// overdraw, replacing the old approach of stamping a full disc at every step
/// along the path (which was O(len * r^2) and repainted each pixel ~r times).
///
/// A pixel center `Q` is covered iff some `t` in `[0, 1]` satisfies
/// `|Q - P(t)| <= r(t)`, where `P(t) = P0 + t*(P1-P0)` and
/// `r(t) = r0 + t*(r1-r0)`. Expanding `|Q - P(t)|^2 <= r(t)^2` gives a quadratic
/// `f(t) = a*t^2 + b*t + c`, and the segment covers `Q` iff `f`'s minimum over
/// `[0, 1]` is `<= 0`. The old disc union is exactly this set sampled at
/// discrete `t`, so this is a gap-free, faithful version of it.
///
/// Both radii are clamped to at least half a pixel. A thinner capsule would
/// only show up where it happens to cross a pixel centre, so thin lances and
/// legs would render dashed and blink as they move.
#[allow(clippy::too_many_arguments)]
fn swept_line(
    fb: &mut Framebuffer,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    w0: f32,
    w1: f32,
    c: Color,
) {
    let r0 = (w0 * 0.5).max(0.5);
    let r1 = (w1 * 0.5).max(0.5);
    let dx = x1 - x0;
    let dy = y1 - y0;
    let dd = dx * dx + dy * dy;
    let rmax = r0.max(r1);

    // Degenerate (near-zero-length) segment: just a disc of the larger radius.
    if dd < 1e-6 {
        filled_circle(fb, x0, y0, rmax, c);
        return;
    }

    let dr = r1 - r0;
    let a = dd - dr * dr;

    let xmin = (x0.min(x1) - rmax).floor() as i32;
    let xmax = (x0.max(x1) + rmax).ceil() as i32;
    let ymin = (y0.min(y1) - rmax).floor() as i32;
    let ymax = (y0.max(y1) + rmax).ceil() as i32;

    for j in ymin..=ymax {
        for i in xmin..=xmax {
            let ax = i as f32 + 0.5 - x0;
            let ay = j as f32 + 0.5 - y0;
            let ad = ax * dx + ay * dy; // A . D
            let aa = ax * ax + ay * ay; // |A|^2
            let b = -2.0 * (ad + r0 * dr);
            let cc = aa - r0 * r0;

            // Minimum of f(t) = a*t^2 + b*t + cc over [0, 1]: check both
            // endpoints, plus the parabola vertex when it opens upward and the
            // vertex falls inside the interval.
            let mut fmin = cc.min(a + b + cc);
            if a > 1e-9 {
                let tv = -b / (2.0 * a);
                if tv > 0.0 && tv < 1.0 {
                    fmin = fmin.min(a * tv * tv + b * tv + cc);
                }
            }

            if fmin <= 0.0 {
                fb.set(i, j, c);
            }
        }
    }

    // A half-pixel end can fall between pixel centres, which would leave a
    // very short capsule empty and clip the tip of a thin one. Thin ends
    // always paint their own pixel, the same as a tiny disc does.
    if r0 < TINY_RADIUS {
        fb.set(x0.floor() as i32, y0.floor() as i32, c);
    }
    if r1 < TINY_RADIUS {
        fb.set(x1.floor() as i32, y1.floor() as i32, c);
    }
}

/// Thick line of constant width `w`.
#[allow(dead_code)]
pub fn thick_line(fb: &mut Framebuffer, x0: f32, y0: f32, x1: f32, y1: f32, w: f32, c: Color) {
    swept_line(fb, x0, y0, x1, y1, w, w, c);
}

/// Tapered line whose width shrinks from `w0` to `w1` along the path.
#[allow(clippy::too_many_arguments)]
pub fn tapered_line(
    fb: &mut Framebuffer,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    w0: f32,
    w1: f32,
    c: Color,
) {
    swept_line(fb, x0, y0, x1, y1, w0, w1, c);
}

/// Axis-aligned filled ellipse centred on the float point `(cx, cy)`.
pub fn fill_axis_ellipse(fb: &mut Framebuffer, cx: f32, cy: f32, half_w: f32, half_h: f32, c: Color) {
    filled_ellipse_oriented(fb, cx, cy, 1.0, 0.0, half_w, half_h, c);
}

/// Fill the iso-projected arena floor: the world region around `center_world`
/// where `dist <= radius + amp * sin(n * theta)`. That is the same bumpy curve
/// the wall follows, so floor and border line up. `pixel_color` is sampled per
/// pixel in world coordinates so textures follow the iso tilt.
#[allow(clippy::too_many_arguments)]
pub fn fill_iso_bumpy_disc(
    fb: &mut Framebuffer,
    iso: &crate::render::iso::Iso,
    center_world: (f32, f32),
    radius_world: f32,
    amp: f32,
    n: f32,
    pixel_color: impl Fn(f32, f32) -> Color,
) {
    let amp = amp.abs();
    let outer = radius_world + amp;
    let inner = (radius_world - amp).max(0.0);
    let (outer2, inner2) = (outer * outer, inner * inner);
    let cs = iso.project((center_world.0, center_world.1, 0.0));
    let (half_w, half_h) = iso.ground_ellipse(outer);
    let x_min = (cs.0 - half_w - 1.0).floor() as i32;
    let x_max = (cs.0 + half_w + 1.0).ceil() as i32;
    let y_min = (cs.1 - half_h - 1.0).floor() as i32;
    let y_max = (cs.1 + half_h + 1.0).ceil() as i32;

    for sy_i in y_min.max(0)..=y_max.min(fb.height as i32 - 1) {
        for sx_i in x_min.max(0)..=x_max.min(fb.width as i32 - 1) {
            let (wx, wy) = iso.unproject_ground((sx_i as f32 + 0.5, sy_i as f32 + 0.5));
            let dx = wx - center_world.0;
            let dy = wy - center_world.1;
            let d2 = dx * dx + dy * dy;
            if d2 > outer2 {
                continue;
            }
            // Only the thin band where the bumps live needs the exact test.
            if d2 > inner2 {
                let edge = radius_world + amp * (n * dy.atan2(dx)).sin();
                if d2 > edge * edge {
                    continue;
                }
            }
            fb.set(sx_i, sy_i, pixel_color(wx, wy));
        }
    }
}

/// Filled ellipse centred on the float point `(cx, cy)` and oriented along
/// the unit vector `(fx, fy)`. `half_len` is along the forward axis, `half_w`
/// along the perpendicular. Pixels are tested at their centres against the
/// true centre, the same way `filled_circle` does, so the shape does not
/// snap to whole pixels as it moves.
#[allow(clippy::too_many_arguments)]
pub fn filled_ellipse_oriented(
    fb: &mut Framebuffer,
    cx: f32,
    cy: f32,
    fx: f32,
    fy: f32,
    half_len: f32,
    half_w: f32,
    c: Color,
) {
    let half_len = half_len.abs();
    let half_w = half_w.abs();
    // Screen-space half extents of the rotated ellipse.
    let ext_x = ((half_len * fx).powi(2) + (half_w * fy).powi(2)).sqrt();
    let ext_y = ((half_len * fy).powi(2) + (half_w * fx).powi(2)).sqrt();
    let x0 = (cx - ext_x).floor() as i32;
    let x1 = (cx + ext_x).ceil() as i32;
    let y0 = (cy - ext_y).floor() as i32;
    let y1 = (cy + ext_y).ceil() as i32;
    let inv_len = 1.0 / half_len.max(0.001);
    let inv_w = 1.0 / half_w.max(0.001);
    for j in y0..=y1 {
        for i in x0..=x1 {
            let dx = i as f32 + 0.5 - cx;
            let dy = j as f32 + 0.5 - cy;
            // Local frame: forward = (fx, fy), right = (-fy, fx).
            let ex = (dx * fx + dy * fy) * inv_len;
            let ey = (-dx * fy + dy * fx) * inv_w;
            if ex * ex + ey * ey <= 1.0 {
                fb.set(i, j, c);
            }
        }
    }
    if half_len.min(half_w) < TINY_RADIUS {
        fb.set(cx.floor() as i32, cy.floor() as i32, c);
    }
}

/// Compact 3-wide × 5-tall bitmap font used at the medium UI tier where 5×5
/// would overflow but plain text is unreadable. Designed to keep `M N W H V U Y`
/// visually distinct (the letters that collapse at 3-wide in naive designs).
/// Bits: row-major, low 3 bits per row used (bit 2 = leftmost).
pub fn glyph_3x5(ch: char) -> Option<&'static [u8; 5]> {
    Some(match ch {
        ' ' => &[0; 5],
        '-' => &[0b000, 0b000, 0b111, 0b000, 0b000],
        '>' => &[0b100, 0b010, 0b001, 0b010, 0b100],
        ':' => &[0b000, 0b010, 0b000, 0b010, 0b000],
        ',' => &[0b000, 0b000, 0b000, 0b010, 0b100],
        '.' => &[0b000, 0b000, 0b000, 0b000, 0b010],
        '!' => &[0b010, 0b010, 0b010, 0b000, 0b010],
        '0' => &[0b010, 0b101, 0b101, 0b101, 0b010],
        '1' => &[0b010, 0b110, 0b010, 0b010, 0b111],
        '2' => &[0b110, 0b001, 0b010, 0b100, 0b111],
        '3' => &[0b110, 0b001, 0b010, 0b001, 0b110],
        '4' => &[0b101, 0b101, 0b111, 0b001, 0b001],
        '5' => &[0b111, 0b100, 0b110, 0b001, 0b110],
        '6' => &[0b011, 0b100, 0b110, 0b101, 0b010],
        '7' => &[0b111, 0b001, 0b010, 0b100, 0b100],
        '8' => &[0b010, 0b101, 0b010, 0b101, 0b010],
        '9' => &[0b010, 0b101, 0b011, 0b001, 0b110],
        'A' => &[0b010, 0b101, 0b111, 0b101, 0b101],
        'B' => &[0b110, 0b101, 0b110, 0b101, 0b110],
        'C' => &[0b011, 0b100, 0b100, 0b100, 0b011],
        'D' => &[0b110, 0b101, 0b101, 0b101, 0b110],
        'E' => &[0b111, 0b100, 0b110, 0b100, 0b111],
        'F' => &[0b111, 0b100, 0b110, 0b100, 0b100],
        'G' => &[0b011, 0b100, 0b101, 0b101, 0b011],
        // `H` keeps one crossbar in the middle; `M` doubles up its top.
        'H' => &[0b101, 0b101, 0b111, 0b101, 0b101],
        'I' => &[0b111, 0b010, 0b010, 0b010, 0b111],
        'J' => &[0b001, 0b001, 0b001, 0b101, 0b010],
        'K' => &[0b101, 0b110, 0b100, 0b110, 0b101],
        'L' => &[0b100, 0b100, 0b100, 0b100, 0b111],
        // `M` has a 2-row crown so it can't be confused with `H`.
        'M' => &[0b101, 0b111, 0b111, 0b101, 0b101],
        // `N` is an arch, like a small `n`. Three columns can't hold a
        // diagonal without it turning into `M` or `H`.
        'N' => &[0b111, 0b101, 0b101, 0b101, 0b101],
        'O' => &[0b010, 0b101, 0b101, 0b101, 0b010],
        'P' => &[0b110, 0b101, 0b110, 0b100, 0b100],
        // `Q` is an `O` with a full fourth row and a tail to the right.
        'Q' => &[0b010, 0b101, 0b101, 0b111, 0b011],
        'R' => &[0b110, 0b101, 0b110, 0b110, 0b101],
        'S' => &[0b011, 0b100, 0b010, 0b001, 0b110],
        'T' => &[0b111, 0b010, 0b010, 0b010, 0b010],
        // `U` narrows on the last row; `V` narrows on the last two; `Y` narrows higher.
        'U' => &[0b101, 0b101, 0b101, 0b101, 0b010],
        'V' => &[0b101, 0b101, 0b101, 0b010, 0b010],
        // `W` is M's mirror, with a wide bottom and a single peak.
        'W' => &[0b101, 0b101, 0b111, 0b111, 0b010],
        'X' => &[0b101, 0b010, 0b010, 0b010, 0b101],
        'Y' => &[0b101, 0b101, 0b010, 0b010, 0b010],
        'Z' => &[0b111, 0b001, 0b010, 0b100, 0b111],
        _ => return None,
    })
}

// 5x5 bitmap font for digits and a few uppercase letters / glyphs we use in the HUD.
// Bits: row-major, leftmost bit = leftmost pixel. Only low 5 bits per row used.
pub fn glyph(ch: char) -> Option<&'static [u8; 5]> {
    Some(match ch {
        '0' => &[0b01110, 0b10001, 0b10001, 0b10001, 0b01110],
        '1' => &[0b00100, 0b01100, 0b00100, 0b00100, 0b01110],
        '2' => &[0b01110, 0b10001, 0b00010, 0b00100, 0b11111],
        '3' => &[0b11110, 0b00001, 0b01110, 0b00001, 0b11110],
        '4' => &[0b00010, 0b00110, 0b01010, 0b11111, 0b00010],
        '5' => &[0b11111, 0b10000, 0b11110, 0b00001, 0b11110],
        '6' => &[0b01110, 0b10000, 0b11110, 0b10001, 0b01110],
        '7' => &[0b11111, 0b00001, 0b00010, 0b00100, 0b00100],
        '8' => &[0b01110, 0b10001, 0b01110, 0b10001, 0b01110],
        '9' => &[0b01110, 0b10001, 0b01111, 0b00001, 0b01110],
        ':' => &[0b00000, 0b00100, 0b00000, 0b00100, 0b00000],
        ',' => &[0b00000, 0b00000, 0b00000, 0b00100, 0b01000],
        '.' => &[0b00000, 0b00000, 0b00000, 0b00000, 0b00100],
        '!' => &[0b00100, 0b00100, 0b00100, 0b00000, 0b00100],
        ' ' => &[0; 5],
        '-' => &[0b00000, 0b00000, 0b01110, 0b00000, 0b00000],
        '>' => &[0b10000, 0b01000, 0b00100, 0b01000, 0b10000],
        'A' => &[0b01110, 0b10001, 0b11111, 0b10001, 0b10001],
        'B' => &[0b11110, 0b10001, 0b11110, 0b10001, 0b11110],
        'C' => &[0b01111, 0b10000, 0b10000, 0b10000, 0b01111],
        'D' => &[0b11110, 0b10001, 0b10001, 0b10001, 0b11110],
        'E' => &[0b11111, 0b10000, 0b11110, 0b10000, 0b11111],
        'F' => &[0b11111, 0b10000, 0b11110, 0b10000, 0b10000],
        'G' => &[0b01111, 0b10000, 0b10011, 0b10001, 0b01111],
        'H' => &[0b10001, 0b10001, 0b11111, 0b10001, 0b10001],
        'I' => &[0b01110, 0b00100, 0b00100, 0b00100, 0b01110],
        'J' => &[0b00111, 0b00010, 0b00010, 0b10010, 0b01100],
        'K' => &[0b10001, 0b10010, 0b11100, 0b10010, 0b10001],
        'L' => &[0b10000, 0b10000, 0b10000, 0b10000, 0b11111],
        'M' => &[0b10001, 0b11011, 0b10101, 0b10001, 0b10001],
        'N' => &[0b10001, 0b11001, 0b10101, 0b10011, 0b10001],
        'O' => &[0b01110, 0b10001, 0b10001, 0b10001, 0b01110],
        'P' => &[0b11110, 0b10001, 0b11110, 0b10000, 0b10000],
        'Q' => &[0b01110, 0b10001, 0b10001, 0b10011, 0b01111],
        'R' => &[0b11110, 0b10001, 0b11110, 0b10010, 0b10001],
        'S' => &[0b01111, 0b10000, 0b01110, 0b00001, 0b11110],
        'T' => &[0b11111, 0b00100, 0b00100, 0b00100, 0b00100],
        'U' => &[0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'V' => &[0b10001, 0b10001, 0b10001, 0b01010, 0b00100],
        'W' => &[0b10001, 0b10001, 0b10101, 0b11011, 0b10001],
        'X' => &[0b10001, 0b01010, 0b00100, 0b01010, 0b10001],
        'Y' => &[0b10001, 0b01010, 0b00100, 0b00100, 0b00100],
        'Z' => &[0b11111, 0b00010, 0b00100, 0b01000, 0b11111],
        _ => return None,
    })
}

/// Draw `s` using a bitmap font, where `glyph_w` selects 3×5 or 5×5 glyph
/// tables and `cell_scale` repeats every glyph pixel into a `cell_scale ×
/// cell_scale` block of fb pixels. Returns the next x position (so callers
/// can chain styled spans).
pub fn draw_text_bitmap(
    fb: &mut Framebuffer,
    x: i32,
    y: i32,
    s: &str,
    c: Color,
    glyph_w: u8,
    cell_scale: u8,
) -> i32 {
    let scale = cell_scale.max(1) as i32;
    let gw = glyph_w as i32;
    let mut cx = x;
    for ch in s.chars() {
        let upper = ch.to_ascii_uppercase();
        let g = if glyph_w == 3 {
            glyph_3x5(upper)
        } else {
            glyph(upper)
        };
        if let Some(g) = g {
            for (row, bits) in g.iter().enumerate() {
                for col in 0..(glyph_w as usize) {
                    let bit_pos = glyph_w as usize - 1 - col;
                    if bits & (1u8 << bit_pos as u8) != 0 {
                        for dy in 0..scale {
                            for dx in 0..scale {
                                fb.set(
                                    cx + col as i32 * scale + dx,
                                    y + row as i32 * scale + dy,
                                    c,
                                );
                            }
                        }
                    }
                }
            }
        }
        // Advance by glyph_w + 1 (gap), all scaled.
        cx += (gw + 1) * scale;
    }
    cx
}

/// Width in fb pixels of `s` rendered by `draw_text_bitmap` with the given
/// glyph width and cell scale. The trailing inter-glyph gap is excluded.
pub fn text_bitmap_width(s: &str, glyph_w: u8, cell_scale: u8) -> i32 {
    let scale = cell_scale.max(1) as i32;
    let gw = glyph_w as i32;
    let n = s.chars().count() as i32;
    if n == 0 {
        0
    } else {
        n * (gw + 1) * scale - scale
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// The previous implementation: stamp a full disc at every integer step.
    /// Kept here as the reference the swept fill must reproduce.
    #[allow(clippy::too_many_arguments)]
    fn disc_stamp(
        fb: &mut Framebuffer,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        w0: f32,
        w1: f32,
        c: Color,
    ) {
        let dx = x1 - x0;
        let dy = y1 - y0;
        let len = (dx * dx + dy * dy).sqrt().max(1.0);
        let steps = len.ceil() as i32;
        for s in 0..=steps {
            let t = s as f32 / steps as f32;
            let x = x0 + dx * t;
            let y = y0 + dy * t;
            // Same half-pixel floor as `swept_line`.
            let r = ((w0 * (1.0 - t) + w1 * t) * 0.5).max(0.5);
            filled_circle(fb, x, y, r, c);
        }
    }

    fn set_pixels(fb: &Framebuffer) -> HashSet<(i32, i32)> {
        let mut s = HashSet::new();
        for y in 0..fb.height as i32 {
            for x in 0..fb.width as i32 {
                let px = fb.color[(y as u32 * fb.width + x as u32) as usize];
                if (px.r, px.g, px.b) != (0, 0, 0) {
                    s.insert((x, y));
                }
            }
        }
        s
    }

    #[test]
    fn swept_line_matches_disc_stamp() {
        // (x0, y0, x1, y1, w0, w1): assorted angles, lengths, and tapers.
        let cases = [
            (10.0, 10.0, 50.0, 30.0, 3.0, 3.0),
            (10.0, 40.0, 55.0, 40.0, 4.0, 1.0), // horizontal taper
            (20.0, 55.0, 20.0, 10.0, 2.0, 2.0), // vertical
            (5.0, 5.0, 60.0, 60.0, 5.0, 1.5),   // diagonal taper
            (60.0, 12.0, 12.0, 50.0, 1.5, 4.0), // reverse taper
            (30.0, 30.0, 30.4, 30.4, 4.0, 4.0), // near-degenerate
        ];

        for (x0, y0, x1, y1, w0, w1) in cases {
            let c = Color::new(255, 0, 0);
            let mut a = Framebuffer::new(80, 80);
            let mut b = Framebuffer::new(80, 80);
            a.clear(Color::BLACK);
            b.clear(Color::BLACK);
            disc_stamp(&mut a, x0, y0, x1, y1, w0, w1, c);
            swept_line(&mut b, x0, y0, x1, y1, w0, w1, c);

            let old = set_pixels(&a);
            let new = set_pixels(&b);
            assert!(!old.is_empty(), "reference drew nothing for {x0},{y0},{x1},{y1}");

            // No gaps: every disc-stamped pixel is covered by the swept fill.
            let missing = old.difference(&new).count();
            assert_eq!(
                missing, 0,
                "swept fill left {missing} gaps vs disc stamp for {x0},{y0},{x1},{y1}"
            );

            // No meaningful over-cover: the swept region is a tight superset
            // (only sub-pixel boundary pixels the discrete stamping missed).
            let extra = new.difference(&old).count();
            assert!(
                extra as f32 <= old.len() as f32 * 0.15 + 4.0,
                "swept over-covered by {extra} (base {}) for {x0},{y0},{x1},{y1}",
                old.len()
            );
        }
    }

    fn painted(fb: &Framebuffer) -> usize {
        set_pixels(fb).len()
    }

    /// Tiny discs, ellipses and lines always leave a mark, wherever their
    /// centre falls inside a pixel.
    #[test]
    fn tiny_shapes_never_vanish() {
        let c = Color::new(255, 0, 0);
        for i in 0..10 {
            for j in 0..10 {
                let (x, y) = (5.0 + i as f32 * 0.1, 5.0 + j as f32 * 0.1);
                let mut fb = Framebuffer::new(12, 12);
                filled_circle(&mut fb, x, y, 0.3, c);
                assert!(painted(&fb) > 0, "circle at {x},{y}");

                let mut fb = Framebuffer::new(12, 12);
                fill_axis_ellipse(&mut fb, x, y, 0.6, 0.3, c);
                assert!(painted(&fb) > 0, "ellipse at {x},{y}");

                let mut fb = Framebuffer::new(12, 12);
                filled_ellipse_oriented(&mut fb, x, y, 0.6, 0.8, 0.5, 0.2, c);
                assert!(painted(&fb) > 0, "oriented ellipse at {x},{y}");

                let mut fb = Framebuffer::new(12, 12);
                tapered_line(&mut fb, x, y, x + 0.2, y + 0.1, 0.2, 0.1, c);
                assert!(painted(&fb) > 0, "short line at {x},{y}");
            }
        }
    }

    /// A thin line has no holes: every column it crosses gets a pixel.
    #[test]
    fn thin_line_is_continuous() {
        let c = Color::new(255, 0, 0);
        let mut fb = Framebuffer::new(80, 40);
        tapered_line(&mut fb, 3.3, 7.6, 71.8, 29.1, 0.3, 0.2, c);
        let px = set_pixels(&fb);
        for x in 4..71 {
            assert!(px.iter().any(|&(i, _)| i == x), "gap at column {x}");
        }
    }

    /// Ellipses follow the fractional centre instead of snapping to the
    /// pixel grid: the painted area is symmetric around the true centre.
    #[test]
    fn ellipse_follows_fractional_centre() {
        let c = Color::new(255, 0, 0);
        for &(cx, cy) in &[(20.0f32, 15.0f32), (20.5, 15.5), (20.25, 15.75)] {
            for oriented in [false, true] {
                let mut fb = Framebuffer::new(40, 30);
                if oriented {
                    filled_ellipse_oriented(&mut fb, cx, cy, 0.6, 0.8, 6.0, 3.0, c);
                } else {
                    fill_axis_ellipse(&mut fb, cx, cy, 6.0, 3.0, c);
                }
                let px = set_pixels(&fb);
                let n = px.len() as f32;
                let mx = px.iter().map(|p| p.0 as f32 + 0.5).sum::<f32>() / n;
                let my = px.iter().map(|p| p.1 as f32 + 0.5).sum::<f32>() / n;
                assert!(
                    (mx - cx).abs() < 0.2 && (my - cy).abs() < 0.2,
                    "centroid {mx},{my} for centre {cx},{cy} oriented={oriented}"
                );
            }
        }
    }
}
