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
    range: i64,
    min_db: i16,
    max_db: i16,
    db_range: u8,
    has_switch: bool,
    is_softvol: bool,
    use_linear_in_db: bool,
}

// min_db cannot be depended on to be mute. Also note that contrary to
// its name copied verbatim from Alsa, this is in millibel scale.
const SND_CTL_TLV_DB_GAIN_MUTE: i64 = -9999999;

impl Mixer for AlsaMixer {
    fn open(config: MixerConfig) -> Self {
        info!(
            "Mixing with alsa and volume control: {:?} for card: {} with mixer control: {},{}",
            config.volume_ctrl, config.card, config.control, config.index,
        );

        let mut config = config; // clone

        let mixer =
            alsa::mixer::Mixer::new(&config.card, false).expect("Could not open Alsa mixer");
        let simple_element = mixer
            .find_selem(&SelemId::new(&config.control, config.index))
            .expect("Could not find Alsa mixer control");

        // Query capabilities
        let has_switch = simple_element.has_playback_switch();
        let is_softvol = simple_element
            .get_playback_vol_db(SelemChannelId::mono())
            .is_err();

        // Query raw volume range
        let (min, max) = simple_element.get_playback_volume_range();
        let range = i64::abs(max - min);

        // Query dB volume range -- note that Alsa exposes a different
        // API for hardware and software mixers
        let (min_millibel, max_millibel) = if is_softvol {
            let control = Ctl::new(&config.card, false).expect("Could not open Alsa softvol");
            let mut element_id = ElemId::new(ElemIface::Mixer);
            element_id.set_name(&CString::new(config.control.as_str()).unwrap());
            element_id.set_index(config.index);
            control
                .get_db_range(&element_id)
                .expect("Could not get Alsa dB range")
        } else {
            simple_element.get_playback_db_range()
        };
        let min_db = min_millibel.to_db() as i16;
        let max_db = max_millibel.to_db() as i16;
        let db_range = i16::abs(max_db - min_db) as u8;

        // Synchronize the volume control dB range with the mixer control,
        // unless it was already set with a command line option.
        if !config.volume_ctrl.range_ok() {
            config.volume_ctrl.set_db_range(db_range);
        }

        // For hardware controls with a small range (24 dB or less),
        // force using the dB API with a linear mapping.
        let mut use_linear_in_db = false;
        if !is_softvol && db_range <= 24 {
            use_linear_in_db = true;
            config.volume_ctrl = VolumeCtrl::Linear;
        }

        debug!("Alsa mixer control is softvol: {}", is_softvol);
        debug!("Alsa support for playback (mute) switch: {}", has_switch);
        debug!("Alsa raw volume range: [{}..{}] ({})", min, max, range);
        debug!(
            "Alsa dB volume range: [{}..{}] ({})",
            min_db, max_db, db_range
        );
        debug!("Alsa forcing linear dB mapping: {}", use_linear_in_db);

        Self {
            config,
            min,
            max,
            range,
            min_db,
            max_db,
            db_range,
            has_switch,
            is_softvol,
            use_linear_in_db,
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

            let mapped_volume = alsa_volume as f32 / self.range as f32 - self.min as f32;
            return self.config.volume_ctrl.unmap(mapped_volume);
        }

        let db_volume = simple_element
            .get_playback_vol_db(SelemChannelId::mono())
            .unwrap_or(MilliBel(0))
            .to_db();

        if self.use_linear_in_db {
            ((db_volume - self.min_db as f32) / self.db_range as f32) as u16
        } else if f32::abs(db_volume - MilliBel(SND_CTL_TLV_DB_GAIN_MUTE).to_db()) <= f32::EPSILON {
            0
        } else {
            self.config
                .volume_ctrl
                .unmap(db_to_ratio(db_volume - self.max_db as f32))
        }
    }

    fn set_volume(&self, volume: u16) {
        let mixer =
            alsa::mixer::Mixer::new(&self.config.card, false).expect("Could not open Alsa mixer");
        let simple_element = mixer
            .find_selem(&SelemId::new(&self.config.control, self.config.index))
            .expect("Could not find Alsa mixer control");

        if self.has_switch {
            if volume == 0 {
                simple_element
                    .set_playback_switch_all(0)
                    .expect("Could not disable playback (set mute) on Alsa");
            } else if self.switched_off() {
                simple_element
                    .set_playback_switch_all(1)
                    .expect("Could not enable playback (unset mute) on Alsa");
            }
        }

        let mapped_volume = self.config.volume_ctrl.map(volume);

        if self.is_softvol {
            let scaled_volume = (self.min as f32 + mapped_volume * self.range as f32) as i64;
            debug!("Setting Alsa raw volume to {}", scaled_volume);
            simple_element
                .set_playback_volume_all(scaled_volume)
                .expect("Could not set Alsa raw volume");
            return;
        }

        let db_volume = if self.use_linear_in_db {
            self.min_db as f32 + mapped_volume as f32 * self.db_range as f32
        } else if volume == 0 {
            // prevent ratio_to_db(0.0) from returning -inf
            MilliBel(SND_CTL_TLV_DB_GAIN_MUTE).to_db()
        } else {
            ratio_to_db(mapped_volume) + self.max_db as f32
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
