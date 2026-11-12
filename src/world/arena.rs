use crate::render::iso;

/// Smallest and largest arena side length in world units.
pub const MIN_ARENA_SIZE: f32 = 60.0;
pub const MAX_ARENA_SIZE: f32 = 200.0;

/// Below this many framebuffer pixels per world unit the arena is too small
/// to play, and the game pauses behind a "resize" splash.
pub const MIN_ARENA_SCALE: f32 = 0.5;

/// Arena size used to measure how much screen the arena can have. The fitted
/// diameter in pixels barely depends on the size picked here.
const REFERENCE_SIZE: f32 = 100.0;

/// Banner pole height in world units for an arena of radius `arena_r`.
pub fn banner_pole_height(arena_r: f32) -> f32 {
    (arena_r * 0.20).max(6.0)
}

/// How far above the pole top the finial sits, in world units.
pub const BANNER_FINIAL_UP: f32 = 1.2;
/// Finial radius in framebuffer pixels.
pub const BANNER_FINIAL_RADIUS_PX: f32 = 1.2;

/// Pick the arena size for a new run from the space the framebuffer offers.
///
/// The arena diameter budget `D` (in pixels) is what a best fit gives. Using
/// `sqrt(60 * D)` as the side length means a larger screen grows both the map
/// and the pixels per world unit, so lowering the font size buys a bigger
/// arena and prettier knights at the same time.
pub fn arena_size_for(fb_w: u32, fb_h: u32, top_px: i32, y_scale: f32) -> f32 {
    let fit = iso::fit_arena(REFERENCE_SIZE, fb_w as i32, fb_h as i32, top_px, y_scale);
    let budget = REFERENCE_SIZE * fit.scale;
    (60.0 * budget)
        .sqrt()
        .round()
        .clamp(MIN_ARENA_SIZE, MAX_ARENA_SIZE)
}

/// True when an arena of `arena_size` would be drawn below `MIN_ARENA_SCALE`
/// pixels per world unit in this framebuffer.
pub fn too_small(arena_size: f32, fb_w: u32, fb_h: u32, top_px: i32, y_scale: f32) -> bool {
    iso::fit_arena(arena_size, fb_w as i32, fb_h as i32, top_px, y_scale).scale < MIN_ARENA_SCALE
}
