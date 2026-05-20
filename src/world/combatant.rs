/// 2D combatant, either knight or opponent. Position and velocity are in arena world units (pixels).
#[derive(Clone, Copy)]
pub struct Combatant {
    pub pos: (f32, f32),
    pub vel: (f32, f32),
    /// Unit vector for the direction the lance is pointing. Updated from velocity when moving fast enough.
    pub facing: (f32, f32),
    pub radius: f32,
    pub lance_len: f32,
    /// Height above the board (world units). 0 = grounded.
    pub z: f32,
    pub vz: f32,
}

impl Combatant {
    pub fn new(pos: (f32, f32), facing: (f32, f32), radius: f32, lance_len: f32) -> Self {
        Self {
            pos,
            vel: (0.0, 0.0),
            facing,
            radius,
            lance_len,
            z: 0.0,
            vz: 0.0,
        }
    }

    pub fn speed(&self) -> f32 {
        (self.vel.0 * self.vel.0 + self.vel.1 * self.vel.1).sqrt()
    }

    pub fn airborne(&self) -> bool {
        self.z > 0.05
    }

    /// Apply a vertical impulse if grounded. Returns true if the jump fired.
    pub fn try_jump(&mut self, impulse: f32) -> bool {
        if self.airborne() {
            return false;
        }
        self.vz = impulse;
        self.z = 0.05; // nudge off the ground so airborne() is true next tick
        true
    }

    /// Update facing to point along velocity if moving fast enough.
    pub fn update_facing(&mut self, min_speed: f32) {
        let s = self.speed();
        if s > min_speed {
            self.facing = (self.vel.0 / s, self.vel.1 / s);
        }
    }

    /// Tip of the lance in world coords.
    pub fn lance_tip(&self) -> (f32, f32) {
        (
            self.pos.0 + self.facing.0 * self.lance_len,
            self.pos.1 + self.facing.1 * self.lance_len,
        )
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum OpponentMode {
    Static,
    TiltShared,
    FreeAi,
}

impl OpponentMode {
    #[allow(dead_code)]
    pub fn label(self) -> &'static str {
        match self {
            OpponentMode::Static => "STATIC",
            OpponentMode::TiltShared => "TILT-SHARED",
            OpponentMode::FreeAi => "FREE-AI",
        }
    }

    pub fn cycle(self) -> Self {
        match self {
            OpponentMode::Static => OpponentMode::TiltShared,
            OpponentMode::TiltShared => OpponentMode::FreeAi,
            OpponentMode::FreeAi => OpponentMode::Static,
        }
    }
}
