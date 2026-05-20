use std::fs::File;
use std::io::{BufReader, Cursor};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rand::Rng;
use rodio::source::Source;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};

use crate::physics::ImpactKind;

const MUSIC_BYTES: &[u8] = include_bytes!("../assets/sfx/music.ogg");
const GALLOP_BYTES: &[u8] = include_bytes!("../assets/sfx/gallop.ogg");
const NEIGH_BYTES: &[u8] = include_bytes!("../assets/sfx/neigh.ogg");
const CLASH_BYTES: &[u8] = include_bytes!("../assets/sfx/clash.ogg");
const FALL_BYTES: &[u8] = include_bytes!("../assets/sfx/fall.ogg");
const DEATH_BYTES: &[u8] = include_bytes!("../assets/sfx/death.ogg");
const LAND_BYTES: &[u8] = include_bytes!("../assets/sfx/land.ogg");

const MUSIC_GAP_SEC: f32 = 4.0;
const MUSIC_VOLUME: f32 = 0.55;
const GALLOP_VOLUME_MAX: f32 = 0.55;
const NEIGH_VOLUME: f32 = 0.85;
const FALL_VOLUME: f32 = 0.80;
const DEATH_VOLUME: f32 = 0.85;
const LAND_VOLUME: f32 = 0.75;
/// Knight speed at which the gallop is at full speed-factor.
const GALLOP_SPEED_REF: f32 = 28.0;

const IMPACT_COOLDOWN: Duration = Duration::from_millis(40);
const IMPACT_MAX_CONCURRENT: usize = 4;
/// Closing velocity that maps to full-volume impact.
const IMPACT_SPEED_FULL: f32 = 30.0;

#[derive(Clone, Copy)]
enum MusicState {
    Playing,
    Gap,
}

pub struct SoundManager {
    _stream: OutputStream,
    handle: OutputStreamHandle,
    music: Sink,
    gallop: Sink,
    music_state: MusicState,
    music_gap_t: f32,
    lfo_t: f32,
    muted: bool,

    clash_bytes: Arc<[u8]>,
    thud_bytes: Option<Arc<[u8]>>,
    contact_bytes: Option<Arc<[u8]>>,
    neigh_bytes: Arc<[u8]>,
    fall_bytes: Arc<[u8]>,
    death_bytes: Arc<[u8]>,
    land_bytes: Arc<[u8]>,
    impact_slots: Vec<Sink>,
    last_impact_t: Instant,
}

impl SoundManager {
    /// Best-effort init. Returns `None` if no audio device is available.
    pub fn new() -> Option<Self> {
        let (stream, handle) = OutputStream::try_default().ok()?;
        let music = Sink::try_new(&handle).ok()?;
        let gallop = Sink::try_new(&handle).ok()?;

        // Music: queue the track once; the loop is handled by the state machine
        // so we can insert a silent gap between repeats.
        if let Ok(source) = Decoder::new(BufReader::new(Cursor::new(MUSIC_BYTES))) {
            music.append(source);
        }
        music.set_volume(MUSIC_VOLUME);

        // Gallop: continuous loop, silent until the knight is moving.
        if let Ok(source) = Decoder::new(BufReader::new(Cursor::new(GALLOP_BYTES))) {
            gallop.append(source.repeat_infinite());
        }
        gallop.set_volume(0.0);

        // Impact assets. Clash is embedded; thud and contact load from disk
        // so users can drop files in without a recompile.
        let clash_bytes: Arc<[u8]> = Arc::from(CLASH_BYTES);
        let thud_bytes: Option<Arc<[u8]>> = std::fs::read("assets/sfx/thud.ogg")
            .ok()
            .map(Arc::from);
        let contact_bytes: Option<Arc<[u8]>> = std::fs::read("assets/sfx/contact.ogg")
            .ok()
            .map(Arc::from);
        let neigh_bytes: Arc<[u8]> = Arc::from(NEIGH_BYTES);
        let fall_bytes: Arc<[u8]> = Arc::from(FALL_BYTES);
        let death_bytes: Arc<[u8]> = Arc::from(DEATH_BYTES);
        let land_bytes: Arc<[u8]> = Arc::from(LAND_BYTES);

        Some(Self {
            _stream: stream,
            handle,
            music,
            gallop,
            music_state: MusicState::Playing,
            music_gap_t: 0.0,
            lfo_t: 0.0,
            muted: false,

            clash_bytes,
            thud_bytes,
            contact_bytes,
            neigh_bytes,
            fall_bytes,
            death_bytes,
            land_bytes,
            impact_slots: Vec::new(),
            last_impact_t: Instant::now() - IMPACT_COOLDOWN,
        })
    }

    /// Fire-and-forget detached one-shot.
    fn play_oneshot(&self, bytes: Arc<[u8]>, volume: f32) {
        let Ok(sink) = Sink::try_new(&self.handle) else {
            return;
        };
        let Ok(source) = Decoder::new(BufReader::new(Cursor::new(bytes))) else {
            return;
        };
        sink.append(source);
        sink.set_volume(volume);
        sink.detach();
    }

    /// Per-frame update. `knight_speed` is in world units/s; pass 0 outside
    /// of active play so the gallop dies down naturally.
    pub fn update(&mut self, dt: f32, knight_speed: f32) {
        self.lfo_t += dt;

        match self.music_state {
            MusicState::Playing => {
                if self.music.empty() {
                    self.music_state = MusicState::Gap;
                    self.music_gap_t = 0.0;
                }
            }
            MusicState::Gap => {
                self.music_gap_t += dt;
                if self.music_gap_t >= MUSIC_GAP_SEC {
                    if let Ok(source) =
                        Decoder::new(BufReader::new(Cursor::new(MUSIC_BYTES)))
                    {
                        self.music.append(source);
                    }
                    self.music_state = MusicState::Playing;
                }
            }
        }

        // Gallop volume = speed factor × dual-LFO modulation.
        // Two slow sines at incommensurate angular frequencies (~10 s and ~23 s
        // periods); their sum is aperiodic and avoids the "tunnel of identical
        // gallop loops" feel.
        let speed_factor = (knight_speed / GALLOP_SPEED_REF).clamp(0.0, 1.0);
        let s1 = (self.lfo_t * 0.62).sin();
        let s2 = (self.lfo_t * 0.27).sin();
        let lfo = 0.70 + 0.15 * s1 + 0.15 * s2; // 0.40 .. 1.00
        let target = if self.muted {
            0.0
        } else {
            speed_factor * lfo * GALLOP_VOLUME_MAX
        };
        self.gallop.set_volume(target);
    }

    /// Restart the music track for a victory celebration. If a dedicated
    /// `assets/sfx/fanfare.ogg` exists at runtime, that's used instead, so you
    /// can drop a fanfare file there to swap it in without recompiling.
    pub fn play_fanfare(&mut self) {
        if self.muted {
            return;
        }
        self.music.stop();

        let mut queued = false;
        if let Ok(file) = File::open("assets/sfx/fanfare.ogg") {
            if let Ok(source) = Decoder::new(BufReader::new(file)) {
                self.music.append(source);
                queued = true;
            }
        }
        if !queued {
            if let Ok(source) = Decoder::new(BufReader::new(Cursor::new(MUSIC_BYTES))) {
                self.music.append(source);
            }
        }
        self.music.set_volume((MUSIC_VOLUME * 1.4).min(1.0));
        self.music_state = MusicState::Playing;
        self.music_gap_t = 0.0;
    }

    /// Play an impact sound for a collision event. Mitigations against ear
    /// fatigue, in order of importance:
    ///   1. Edge-triggered upstream: physics only emits an impact above a
    ///      velocity threshold, so sliding contact stays silent.
    ///   2. Concurrent cap: silently drops new sounds if too many are active.
    ///   3. Global cooldown: back-to-back impacts within `IMPACT_COOLDOWN`
    ///      collapse to a single sound.
    ///   4. Random pitch ±15%, so identical hits never sound identical.
    ///   5. Random volume jitter ±18% adds organic variation.
    ///   6. Velocity-scaled volume, so soft brushes are subtle and hard hits ring.
    pub fn play_impact(&mut self, kind: ImpactKind, speed: f32) {
        if self.muted {
            return;
        }

        let now = Instant::now();
        if now.duration_since(self.last_impact_t) < IMPACT_COOLDOWN {
            return;
        }

        // Reap finished sinks before checking the cap.
        self.impact_slots.retain(|s| !s.empty());
        if self.impact_slots.len() >= IMPACT_MAX_CONCURRENT {
            return;
        }

        // pitch_base bumps the random-jitter range for cases where we're
        // reusing an existing sample for a different surface. Currently the
        // bumper-via-thud fallback rings ~30% brighter than the wall thud.
        let (bytes, pitch_base): (Arc<[u8]>, f32) = match kind {
            ImpactKind::Body => (self.clash_bytes.clone(), 1.0),
            ImpactKind::Wall => match &self.thud_bytes {
                Some(b) => (b.clone(), 1.0),
                None => return,
            },
            ImpactKind::Bumper => match &self.contact_bytes {
                Some(b) => (b.clone(), 1.0),
                None => match &self.thud_bytes {
                    Some(b) => (b.clone(), 1.30),
                    None => return,
                },
            },
        };

        let Ok(sink) = Sink::try_new(&self.handle) else {
            return;
        };
        let Ok(source) = Decoder::new(BufReader::new(Cursor::new(bytes))) else {
            return;
        };

        let mut rng = rand::rng();
        let pitch = pitch_base * rng.random_range(0.88f32..1.16);
        let vol_jitter = rng.random_range(0.82f32..1.0);
        let speed_norm = ((speed - 8.0) / (IMPACT_SPEED_FULL - 8.0))
            .clamp(0.0, 1.0)
            .powf(0.7);
        let base_vol = match kind {
            ImpactKind::Body => 0.55,
            ImpactKind::Wall => 0.45,
            ImpactKind::Bumper => 0.55,
        };
        let vol = base_vol * (0.35 + 0.65 * speed_norm) * vol_jitter;

        sink.append(source.speed(pitch));
        sink.set_volume(vol);
        self.impact_slots.push(sink);
        self.last_impact_t = now;
    }

    /// One-shot neigh that announces a new opponent's arrival at round start.
    pub fn play_neigh(&self) {
        if self.muted {
            return;
        }
        self.play_oneshot(self.neigh_bytes.clone(), NEIGH_VOLUME);
    }

    /// Pit fall (knight or opponent).
    pub fn play_fall(&self) {
        if self.muted {
            return;
        }
        self.play_oneshot(self.fall_bytes.clone(), FALL_VOLUME);
    }

    /// Lance death (knight or opponent unhorsed).
    pub fn play_death(&self) {
        if self.muted {
            return;
        }
        self.play_oneshot(self.death_bytes.clone(), DEATH_VOLUME);
    }

    /// Knight lands after a jump.
    pub fn play_land(&self) {
        if self.muted {
            return;
        }
        self.play_oneshot(self.land_bytes.clone(), LAND_VOLUME);
    }

    #[allow(dead_code)]
    pub fn set_muted(&mut self, m: bool) {
        self.muted = m;
        self.music.set_volume(if m { 0.0 } else { MUSIC_VOLUME });
    }

    #[allow(dead_code)]
    pub fn muted(&self) -> bool {
        self.muted
    }
}
