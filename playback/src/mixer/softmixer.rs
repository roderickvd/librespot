use std::sync::{Arc, Mutex};

use super::AudioFilter;
use super::{Mixer, MixerConfig};

#[derive(Clone)]
pub struct SoftMixer {
    volume: Arc<Mutex<f64>>,
}

impl Mixer for SoftMixer {
    fn open(_: Option<MixerConfig>) -> SoftMixer {
        SoftMixer {
            volume: Arc::new(Mutex::new(1.0)),
        }
    }
    fn start(&self) {}
    fn stop(&self) {}
    fn volume(&self) -> f64 {
        *self.volume.lock().unwrap()
    }
    fn set_volume(&self, volume: f64) {
        let mut vol = self.volume.lock().unwrap();
        *vol = volume;
    }
    fn get_audio_filter(&self) -> Option<Box<dyn AudioFilter + Send>> {
        Some(Box::new(SoftVolumeApplier {
            volume: self.volume.clone(),
        }))
    }
}

struct SoftVolumeApplier {
    volume: Arc<Mutex<f64>>,
}

impl AudioFilter for SoftVolumeApplier {
    fn modify_stream(&self, data: &mut [f32]) {
        let volume = *self.volume.lock().unwrap();
        if volume < 1.0 {
            for x in data.iter_mut() {
                *x = (*x as f64 * volume) as f32;
            }
        }
    }
}
