//! Bounded sound-stimulus simulation shared by perception and presentation.
//! This module calculates audibility and stereo gains; it does not open an
//! audio device or synthesize samples. A host can feed the same result to
//! AI hearing and its native/WebAudio playback backend.

use glam::Vec3;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SoundKind {
    Footstep,
    Gunfire,
    Mechanism,
    Ambient,
}

#[derive(Copy, Clone, Debug)]
pub struct SoundEvent {
    pub source: u64,
    pub kind: SoundKind,
    pub position: Vec3,
    pub radius: f32,
    pub loudness: f32,
    pub duration: f32,
}

#[derive(Copy, Clone, Debug)]
struct ActiveSound {
    event: SoundEvent,
    remaining: f32,
    id: u64,
}

#[derive(Copy, Clone, Debug)]
pub struct HeardSound {
    pub id: u64,
    pub source: u64,
    pub kind: SoundKind,
    pub position: Vec3,
    /// The same scalar supplied to perception and playback.
    pub level: f32,
    pub left_gain: f32,
    pub right_gain: f32,
    pub occluded: bool,
}

pub struct SoundField {
    active: Vec<ActiveSound>,
    capacity: usize,
    next_id: u64,
    evicted: u64,
}

impl SoundField {
    pub fn new(capacity: usize) -> Self {
        Self {
            active: Vec::with_capacity(capacity),
            capacity,
            next_id: 0,
            evicted: 0,
        }
    }

    /// Invalid events are rejected. A full field evicts the oldest sound
    /// deterministically, and reports it in the ledger rather than growing.
    pub fn emit(&mut self, event: SoundEvent) -> Option<u64> {
        if self.capacity == 0
            || !event.position.is_finite()
            || !event.radius.is_finite()
            || !event.loudness.is_finite()
            || !event.duration.is_finite()
            || event.radius <= 0.0
            || event.loudness <= 0.0
            || event.duration <= 0.0
        {
            return None;
        }
        if self.active.len() == self.capacity {
            self.active.remove(0);
            self.evicted += 1;
        }
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.active.push(ActiveSound {
            remaining: event.duration,
            event,
            id,
        });
        Some(id)
    }

    pub fn advance(&mut self, seconds: f32) {
        if !seconds.is_finite() || seconds <= 0.0 {
            return;
        }
        for sound in &mut self.active {
            sound.remaining -= seconds;
        }
        self.active.retain(|sound| sound.remaining > 0.0);
    }

    pub fn len(&self) -> usize {
        self.active.len()
    }
    pub fn is_empty(&self) -> bool {
        self.active.is_empty()
    }
    pub fn evicted(&self) -> u64 {
        self.evicted
    }

    /// Sample in emission order, without allocating. `blocked` can be
    /// `|a, b| world.occluded(a, b)` over the renderer's triangle BVH.
    /// `right` is the listener's right axis; zero means centered audio.
    /// Occlusion multiplies the level by `transmission` (clamped 0..1).
    pub fn sample<'a>(
        &'a self,
        listener: Vec3,
        right: Vec3,
        transmission: f32,
        mut blocked: impl FnMut(Vec3, Vec3) -> bool + 'a,
    ) -> impl Iterator<Item = HeardSound> + 'a {
        let right = right.normalize_or_zero();
        let transmission = if transmission.is_finite() {
            transmission.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.active.iter().filter_map(move |sound| {
            if !listener.is_finite() {
                return None;
            }
            let delta = sound.event.position - listener;
            let distance = delta.length();
            if distance >= sound.event.radius {
                return None;
            }
            let occluded = blocked(listener, sound.event.position);
            let falloff = (1.0 - distance / sound.event.radius).powi(2);
            let level = sound.event.loudness
                * falloff
                * (sound.remaining / sound.event.duration)
                * if occluded { transmission } else { 1.0 };
            let pan = right.dot(delta.normalize_or_zero()).clamp(-1.0, 1.0);
            let angle = (pan + 1.0) * std::f32::consts::FRAC_PI_4;
            Some(HeardSound {
                id: sound.id,
                source: sound.event.source,
                kind: sound.event.kind,
                position: sound.event.position,
                level,
                left_gain: level * angle.cos(),
                right_gain: level * angle.sin(),
                occluded,
            })
        })
    }
}
