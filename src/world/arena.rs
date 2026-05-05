/// Arena tier derived from terminal cell count, controlling logical arena size.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ArenaTier {
    Pocket,
    Standard,
    Grand,
    BelowMin,
}

impl ArenaTier {
    /// Logical arena world size (square side length, in framebuffer pixels).
    pub fn size(self) -> f32 {
        match self {
            ArenaTier::Pocket => 60.0,
            ArenaTier::Standard => 100.0,
            ArenaTier::Grand => 160.0,
            ArenaTier::BelowMin => 60.0,
        }
    }

    #[allow(dead_code)]
    pub fn label(self) -> &'static str {
        match self {
            ArenaTier::Pocket => "POCKET",
            ArenaTier::Standard => "STANDARD",
            ArenaTier::Grand => "GRAND",
            ArenaTier::BelowMin => "TOO SMALL",
        }
    }
}

/// Pick a tier from terminal cell dimensions.
/// Cells are roughly 2:1 (height:width), so visual size is measured in "visual
/// units" where 1 cell = 1 vu wide × 2 vu tall. This is independent of render
/// mode so the same terminal yields the same tier in halfblock and kitty.
pub fn pick_tier(cells_w: u16, cells_h: u16) -> ArenaTier {
    let visual_w = cells_w as u32;
    let visual_h = (cells_h as u32).saturating_mul(2);
    // Reserve ~12 vu (= 6 cell rows) for the HUD before measuring.
    let side = visual_w.min(visual_h.saturating_sub(12));
    if side < 40 {
        ArenaTier::BelowMin
    } else if side < 65 {
        ArenaTier::Pocket
    } else if side < 130 {
        ArenaTier::Standard
    } else {
        ArenaTier::Grand
    }
}
