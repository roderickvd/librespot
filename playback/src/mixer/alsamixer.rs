use crate::player::{db_to_ratio, ratio_to_db};

use super::{MappedCtrl, VolumeCtrl};
use super::{Mixer, MixerConfig};

use alsa::ctl::{ElemId, ElemIface};
use alsa::mixer::{MilliBel, SelemChannelId, SelemId};
use alsa::{Ctl, Round};

use std::ffi::CString;

#[derive(Clone)]
pub struct AlsaMixer {
    config: MixerConfig,
    min: i64,
    max: i64,
    min_db: i16,
    max_db: i16,
    has_switch: bool,
    is_softvol: bool,
    use_linear: bool,
}

impl Mixer for AlsaMixer {
    fn open(config: &mut MixerConfig) -> Self {
        info!(
            "Mixing with alsa and volume control: {:?} for card: {} with mixer control: {},{}",
            config.volume_ctrl, config.card, config.control, config.index,
        );

        let mixer =
            alsa::mixer::Mixer::new(&config.card, false).expect("Could not open Alsa mixer");
        let simple_element = mixer
            .find_selem(&SelemId::new(&config.control, config.index))
            .expect("Could not find Alsa mixer control");

        let has_switch = simple_element.has_playback_switch();
        let is_softvol = simple_element
            .get_playback_vol_db(SelemChannelId::mono())
            .is_err();

        let (min, max) = simple_element.get_playback_volume_range();
        let mut min_db = MilliBel(0).to_db() as i16;
        let mut max_db = VolumeCtrl::DEFAULT_DB_RANGE as i16;

        // query the dB range if not overriden with a command line option
        let mut db_range = config.volume_ctrl.db_range();
        if !config.volume_ctrl.range_ok() {
            // Alsa exposes different APIs for hardware and software mixers
            let (min_millibel, max_millibel) = match is_softvol {
                false => simple_element.get_playback_db_range(),
                true => {
                    let control =
                        Ctl::new(&config.card, false).expect("Could not open Alsa softvol");
                    let mut element_id = ElemId::new(ElemIface::Mixer);
                    element_id.set_name(&CString::new(config.control.as_str()).unwrap());
                    element_id.set_index(config.index);
                    control
                        .get_db_range(&element_id)
                        .expect("Could not get Alsa dB range")
                }
            };

            min_db = min_millibel.to_db() as i16;
            max_db = max_millibel.to_db() as i16;
            db_range = i16::abs(max_db - min_db) as u8;

            config.volume_ctrl.set_db_range(db_range);
        }

        // For controls with a small range (24 dB or less), the mapping is
        // linear in the dB values.
        let use_linear = db_range <= 24;

        debug!("Alsa mixer control is softvol: {}", is_softvol);
        debug!("Alsa support for playback (mute) switch: {}", has_switch);
        debug!("Alsa raw volume range: [{}..{}]", min, max);
        debug!("Alsa dB volume range: [{}..{}]", min_db, max_db);
        debug!(
            "Alsa forcing linear dB mapping: {}",
            use_linear && !is_softvol
        );

        Self {
            config: config.clone(),
            min,
            max,
            min_db,
            max_db,
            has_switch,
            is_softvol,
            use_linear,
        }
    }

    fn volume(&self) -> u16 {
        let mixer =
            alsa::mixer::Mixer::new(&self.config.card, false).expect("Could not open Alsa mixer");
        let simple_element = mixer
            .find_selem(&SelemId::new(&self.config.control, self.config.index))
            .expect("Could not find Alsa mixer control");

        if self.switched_off() {
            return 0;
        }

        if self.is_softvol {
            let alsa_volume = simple_element
                .get_playback_volume(SelemChannelId::mono())
                .expect("Could not get current Alsa volume");

            let mapped_volume =
                alsa_volume as f32 / i64::abs(self.max - self.min) as f32 - self.min as f32;
            return self.config.volume_ctrl.unmap(mapped_volume);
        }

        let db_volume = simple_element
            .get_playback_vol_db(SelemChannelId::mono())
            .unwrap_or(MilliBel(0))
            .to_db();

        match self.use_linear {
            true => {
                ((db_volume - self.min_db as f32) / i16::abs(self.max_db - self.min_db) as f32)
                    as u16
            }
            false => {
                if f32::abs(db_volume - self.min_db as f32) <= f32::EPSILON {
                    0
                } else {
                    self.config
                        .volume_ctrl
                        .unmap(db_to_ratio(db_volume - self.max_db as f32))
                }
            }
        }
    }

    fn set_volume(&self, volume: u16) {
        let mixer =
            alsa::mixer::Mixer::new(&self.config.card, false).expect("Could not open Alsa mixer");
        let simple_element = mixer
            .find_selem(&SelemId::new(&self.config.control, self.config.index))
            .expect("Could not find Alsa mixer control");

        let mapped_volume = self.config.volume_ctrl.map(volume);

        if self.has_switch {
            if f32::abs(mapped_volume - 0.0) <= f32::EPSILON {
                simple_element
                    .set_playback_switch_all(0)
                    .expect("Could not disable playback (set mute) on Alsa");
            } else if self.switched_off() {
                simple_element
                    .set_playback_switch_all(1)
                    .expect("Could not enable playback (unset mute) on Alsa");
            }
        }

        if self.is_softvol {
            let scaled_volume =
                (self.min as f32 + mapped_volume * i64::abs(self.max - self.min) as f32) as i64;
            debug!("Setting Alsa raw volume to {}", scaled_volume);
            simple_element
                .set_playback_volume_all(scaled_volume as i64)
                .expect("Could not set Alsa raw volume");
            return;
        }

        let db_volume = match self.use_linear {
            true => self.min_db as f32 + volume as f32 * i16::abs(self.max_db - self.min_db) as f32,
            false => {
                if f32::abs(mapped_volume - 0.0) <= f32::EPSILON {
                    self.min_db as f32 // prevent ratio_to_db(0.0) from returning -inf
                } else {
                    ratio_to_db(mapped_volume) + self.max_db as f32
                }
            }
        };
        debug!("Setting Alsa dB volume to {}", db_volume);
        simple_element
            .set_playback_db_all(MilliBel::from_db(db_volume), Round::Floor)
            .expect("Could not set Alsa dB volume");
    }
}

impl AlsaMixer {
    fn switched_off(&self) -> bool {
        if !self.has_switch {
            return false;
        }

        let mixer =
            alsa::mixer::Mixer::new(&self.config.card, false).expect("Could not open Alsa mixer");
        let simple_element = mixer
            .find_selem(&SelemId::new(&self.config.control, self.config.index))
            .expect("Could not find Alsa mixer control");

        simple_element
            .get_playback_switch(SelemChannelId::mono())
            .map(|b| b == 0)
            .unwrap_or(false)
    }
}
