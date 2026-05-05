use crate::audio::SoundManager;
use crate::fx::Particle;
use crate::input::InputState;
use crate::render::framebuffer::Framebuffer;
use crate::world::arena::ArenaTier;
use crate::world::combatant::{Combatant, OpponentMode};
use crate::world::hazard::{Difficulty, Hazard};

#[derive(Clone, Copy, PartialEq)]
pub enum AppState {
    Menu,
    Playing,
    GameOver,
    Victory,
}

#[derive(Clone, Copy, PartialEq)]
pub enum MenuOption {
    Play,
    Difficulty,
    Quit,
}

pub const MENU_OPTIONS: &[MenuOption] = &[
    MenuOption::Play,
    MenuOption::Difficulty,
    MenuOption::Quit,
];

pub struct App {
    pub state: AppState,
    pub menu_selection: usize,
    pub running: bool,

    pub input: InputState,
    pub framebuffer: Framebuffer,

    pub tier: ArenaTier,
    pub knight: Combatant,
    pub opponent: Combatant,
    pub opponent_mode: OpponentMode,
    pub difficulty: Difficulty,
    pub hazards: Vec<Hazard>,

    pub score: u32,
    pub best: u32,
    pub round_time: f32,
    /// `round_time` value when the current opponent was spawned. Used to ramp
    /// the AI's tilt strength from 0 → 1 over the grace period.
    pub opponent_spawn_at: f32,
    pub banner: Option<(String, f32)>,
    pub death_message: String,

    pub particles: Vec<Particle>,
    pub victory_t: f32,
    pub next_firework_t: f32,

    pub sound: Option<SoundManager>,
}

impl App {
    pub fn new() -> Self {
        Self {
            state: AppState::Menu,
            menu_selection: 0,
            running: true,

            input: InputState::new(),
            framebuffer: Framebuffer::new(160, 96),

            tier: ArenaTier::Standard,
            knight: Combatant::new((30.0, 50.0), (1.0, 0.0), 3.5, 9.0),
            opponent: Combatant::new((70.0, 50.0), (-1.0, 0.0), 3.5, 9.0),
            opponent_mode: OpponentMode::FreeAi,
            difficulty: Difficulty::Normal,
            hazards: Vec::new(),

            score: 0,
            best: 0,
            round_time: 0.0,
            opponent_spawn_at: 0.0,
            banner: None,
            death_message: "UNHORSED".to_string(),

            particles: Vec::new(),
            victory_t: 0.0,
            next_firework_t: 0.0,

            sound: SoundManager::new(),
        }
    }

    /// Round number (1-based): rounds completed + 1.
    pub fn level(&self) -> u32 {
        self.score + 1
    }
}
