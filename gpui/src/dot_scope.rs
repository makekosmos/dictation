//! HEX Dot Scope recording model, ported from anomalyco/hex's `dot_scope`.
//! The GPU-only Rune compositor is adapted to the existing GPUI popup.

mod scope;
pub use scope::*;

const MAX_DOTS: usize = 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Processing {
    Sweep,
    #[default]
    Flow,
    Pulse,
}

impl Processing {
    #[allow(dead_code)]
    pub const ALL: [Self; 3] = [Self::Sweep, Self::Flow, Self::Pulse];
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tuning {
    pub columns: f32,
    pub rows: f32,
    pub spacing: f32,
    pub width: f32,
    pub processing_width: f32,
    pub height: f32,
    pub braid_speed: f32,
    pub helix_cycles: f32,
    pub gain: f32,
    pub response: f32,
    pub release: f32,
    pub history_speed: f32,
    pub phosphor: f32,
    pub glow: f32,
    pub saturation: f32,
    pub shimmer_pass: f32,
    pub shimmer: f32,
    pub transition: f32,
    pub bounce: f32,
    pub auto_level: bool,
    pub auto_release: f32,
    pub noise_floor: f32,
    pub processing: Processing,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            columns: 43.0,
            rows: 9.0,
            spacing: 2.6,
            width: 112.0,
            processing_width: 76.0,
            height: 26.0,
            braid_speed: 1.0,
            helix_cycles: 1.6,
            gain: 1.0,
            response: 6.5,
            release: 0.15,
            history_speed: 150.0,
            phosphor: 0.6,
            glow: 1.0,
            saturation: 1.0,
            shimmer_pass: 0.55,
            shimmer: 1.0,
            transition: 0.3,
            bounce: 0.15,
            auto_level: true,
            auto_release: 3.0,
            noise_floor: -60.0,
            processing: Processing::Flow,
        }
    }
}

impl Tuning {
    pub fn clamped(mut self) -> Self {
        self.columns = self.columns.clamp(11.0, 96.0).round();
        self.rows = self
            .rows
            .clamp(3.0, (MAX_DOTS / self.columns as usize).min(15) as f32)
            .round();
        self.spacing = self.spacing.clamp(1.6, 4.0);
        self.width = self.width.clamp(40.0, 280.0);
        self.processing_width = self.processing_width.clamp(26.0, 280.0);
        self.height = self.height.clamp(14.0, 40.0);
        self.braid_speed = self.braid_speed.clamp(0.0, 3.0);
        self.helix_cycles = self.helix_cycles.clamp(0.0, 4.0);
        self.gain = self.gain.clamp(0.25, 3.0);
        self.response = self.response.clamp(1.0, 16.0);
        self.release = self.release.clamp(0.03, 0.6);
        self.history_speed = self.history_speed.clamp(40.0, 400.0);
        self.phosphor = self.phosphor.clamp(0.05, 1.5);
        self.glow = self.glow.clamp(0.0, 3.0);
        self.saturation = self.saturation.clamp(0.0, 2.5);
        self.shimmer_pass = self.shimmer_pass.clamp(0.2, 1.5);
        self.shimmer = self.shimmer.clamp(0.0, 2.5);
        self.transition = self.transition.clamp(0.1, 0.8);
        self.bounce = self.bounce.clamp(0.0, 0.6);
        self.auto_release = self.auto_release.clamp(0.5, 10.0);
        self.noise_floor = self.noise_floor.clamp(-80.0, -30.0);
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AutoLevel {
    peak: f32,
    room: f32,
}

impl Default for AutoLevel {
    fn default() -> Self {
        Self {
            peak: -30.0,
            room: -55.0,
        }
    }
}

impl AutoLevel {
    pub fn step(&mut self, loudness: f32, h: f32, release: f32, noise_floor: f32) -> f32 {
        const MIN_SPAN: f32 = 16.0;
        const RANGE: f32 = 24.0;
        const ROOM_MARGIN: f32 = 5.0;
        let db = loudness.max(-120.0);
        let follow = |value: &mut f32, target: f32, time: f32| {
            *value += (target - *value) * (1.0 - (-h / time.max(1e-3)).exp());
        };
        let room_time = if db < self.room { 0.3 } else { 6.0 };
        follow(&mut self.room, db, room_time);
        let lowest = self.room.max(noise_floor) + MIN_SPAN;
        let (peak_target, peak_time) = if db > self.peak {
            (db, 0.04)
        } else {
            (lowest, release)
        };
        follow(&mut self.peak, peak_target, peak_time);
        self.peak = self.peak.max(lowest);
        let low = (self.room.max(noise_floor) + ROOM_MARGIN).max(self.peak - RANGE);
        ((db - low) / (self.peak - low).max(1.0)).clamp(0.0, 1.0)
    }
}

pub fn fixed_level(loudness: f32) -> f32 {
    ((loudness + 40.0) / 30.0).clamp(0.0, 1.0)
}

pub fn loudness(rms: f32) -> f32 {
    20.0 * (rms.max(0.0) + 1e-6).log10()
}
