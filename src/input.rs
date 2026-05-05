use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

const KEY_TIMEOUT_MS: u128 = 120;

pub struct SharedInput {
    pub keys_last_press: HashMap<KeyCode, Instant>,
    pub has_release_support: bool,
    pub pressed_keys: Vec<KeyCode>,
    /// Set when the user presses Ctrl+C — main loop polls this to exit
    /// (raw mode swallows SIGINT, so we route it through here).
    pub exit_requested: bool,
}

impl SharedInput {
    fn new() -> Self {
        Self {
            keys_last_press: HashMap::new(),
            has_release_support: false,
            pressed_keys: Vec::new(),
            exit_requested: false,
        }
    }

    fn handle_key_event(&mut self, event: KeyEvent) {
        if matches!(event.kind, KeyEventKind::Press)
            && event.code == KeyCode::Char('c')
            && event.modifiers.contains(KeyModifiers::CONTROL)
        {
            self.exit_requested = true;
            return;
        }
        match event.kind {
            KeyEventKind::Press => {
                self.keys_last_press.insert(event.code, Instant::now());
                self.pressed_keys.push(event.code);
            }
            KeyEventKind::Repeat => {
                self.keys_last_press.insert(event.code, Instant::now());
            }
            KeyEventKind::Release => {
                self.has_release_support = true;
                self.keys_last_press.remove(&event.code);
            }
        }
    }
}

pub struct InputState {
    pub shared: Arc<Mutex<SharedInput>>,
    /// Smoothed tilt vector in [-1, 1]^2.
    pub tilt: (f32, f32),
    pub pressed: Vec<KeyCode>,
}

impl InputState {
    pub fn new() -> Self {
        Self {
            shared: Arc::new(Mutex::new(SharedInput::new())),
            tilt: (0.0, 0.0),
            pressed: Vec::new(),
        }
    }

    pub fn sync_from_shared(&mut self) {
        let mut shared = self.shared.lock().unwrap();
        self.pressed.clear();
        self.pressed.append(&mut shared.pressed_keys);
    }

    fn is_held(shared: &SharedInput, key: KeyCode) -> bool {
        if let Some(&t) = shared.keys_last_press.get(&key) {
            shared.has_release_support || t.elapsed().as_millis() < KEY_TIMEOUT_MS
        } else {
            false
        }
    }

    /// Update tilt vector — ramps toward held direction, springs back to 0
    /// when released. Knight momentum lives in the velocity drag, not in
    /// the tilt itself, so releasing keys does not freeze the board.
    pub fn update_tilt(&mut self, dt: f32) {
        let ramp_up = 6.0;
        let ramp_down = 7.0;

        let (left, right, up, down);
        {
            let s = self.shared.lock().unwrap();
            left = Self::is_held(&s, KeyCode::Char('a')) || Self::is_held(&s, KeyCode::Left);
            right = Self::is_held(&s, KeyCode::Char('d')) || Self::is_held(&s, KeyCode::Right);
            up = Self::is_held(&s, KeyCode::Char('w')) || Self::is_held(&s, KeyCode::Up);
            down = Self::is_held(&s, KeyCode::Char('s')) || Self::is_held(&s, KeyCode::Down);
        }

        self.tilt.0 = update_axis(self.tilt.0, left, right, ramp_up, ramp_down, dt);
        self.tilt.1 = update_axis(self.tilt.1, up, down, ramp_up, ramp_down, dt);
    }
}

fn update_axis(current: f32, neg: bool, pos: bool, ramp_up: f32, ramp_down: f32, dt: f32) -> f32 {
    let target = match (neg, pos) {
        (true, false) => -1.0,
        (false, true) => 1.0,
        _ => 0.0,
    };
    if (target - current).abs() < 0.001 {
        return target;
    }
    let rate = if target.abs() > current.abs() || target.signum() != current.signum() {
        ramp_up
    } else {
        ramp_down
    };
    let delta = (target - current).signum() * rate * dt;
    let new_val = current + delta;
    if (current < target && new_val > target) || (current > target && new_val < target) {
        target
    } else {
        new_val.clamp(-1.0, 1.0)
    }
}

pub fn spawn_input_thread(shared: Arc<Mutex<SharedInput>>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || loop {
        if event::poll(Duration::from_millis(5)).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                let mut s = shared.lock().unwrap();
                s.handle_key_event(key);
            }
        }
    })
}
