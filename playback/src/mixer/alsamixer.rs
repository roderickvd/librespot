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
    min_db: f32,
    max_db: f32,
    db_range: f32,
    has_switch: bool,
    is_softvol: bool,
    use_linear_in_db: bool,
}

// min_db cannot be depended on to be mute. Also note that contrary to
// its name copied verbatim from Alsa, this is in millibel scale.
const SND_CTL_TLV_DB_GAIN_MUTE: i64 = -9999999;
const DEFAULT_MAX_DB: MilliBel = MilliBel(0);

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
            let control =
                Ctl::new(&config.card, false).expect("Could not open Alsa softvol with that card");
            let mut element_id = ElemId::new(ElemIface::Mixer);
            element_id.set_name(
                &CString::new(config.control.as_str())
                    .expect("Could not open Alsa softvol with that name"),
            );
            element_id.set_index(config.index);
            let (min_millibel, mut max_millibel) = control
                .get_db_range(&element_id)
                .expect("Could not get Alsa softvol dB range");

            // Alsa can report incorrect maximum volumes due to rounding
            // errors. e.g. Alsa rounds [-60.0..0.0] in range [0..255] to
            // step size 0.23. Then multiplying 0.23 by 255 incorrectly
            // returns a dB range of 58.65 instead of 60 dB, from
            // [-60.00..-1.35]. This workaround checks the default case
            // where the maximum dB volume is expected to be 0.
            if max_millibel != DEFAULT_MAX_DB {
                let reported_step_size = (max_millibel - min_millibel).0 / range;
                let assumed_step_size = (DEFAULT_MAX_DB - min_millibel).0 / range;
                if reported_step_size == assumed_step_size {
                    max_millibel = DEFAULT_MAX_DB;
                }
            }
            (min_millibel, max_millibel)
        } else {
            let (mut min_millibel, max_millibel) = simple_element.get_playback_db_range();

            // Some controls report that their minimum volume is mute, instead
            // of their actual lowest dB setting before that.
            if min_millibel == MilliBel(SND_CTL_TLV_DB_GAIN_MUTE) && min < max {
                debug!("Alsa mixer reported minimum dB as mute, trying workaround");
                min_millibel = simple_element
                    .ask_playback_vol_db(min + 1)
                    .expect("Could not convert Alsa raw volume to dB volume");
            }
            (min_millibel, max_millibel)
        };

        let min_db = min_millibel.to_db();
        let max_db = max_millibel.to_db();
        let db_range = f32::abs(max_db - min_db);

        // Synchronize the volume control dB range with the mixer control,
        // unless it was already set with a command line option.
        if !config.volume_ctrl.range_ok() {
            config.volume_ctrl.set_db_range(db_range as u8);
        }

        // For hardware controls with a small range (24 dB or less),
        // force using the dB API with a linear mapping.
        let mut use_linear_in_db = false;
        if !is_softvol && db_range <= 24.0 {
            use_linear_in_db = true;
            config.volume_ctrl = VolumeCtrl::Linear;
        }

        debug!("Alsa mixer control is softvol: {}", is_softvol);
        debug!("Alsa support for playback (mute) switch: {}", has_switch);
        debug!("Alsa raw volume range: [{}..{}] ({})", min, max, range);
        debug!(
            "Alsa dB volume range: [{:.2}..{:.2}] ({:.2})",
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

        if self.use_raw_linear() {
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
            ((db_volume - self.min_db) / self.db_range) as u16
        } else if f32::abs(db_volume - MilliBel(SND_CTL_TLV_DB_GAIN_MUTE).to_db()) <= f32::EPSILON {
            0
        } else {
            self.config
                .volume_ctrl
                .unmap(db_to_ratio(db_volume - self.max_db))
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
                debug!("Disabling playback (setting mute) on Alsa");
                simple_element
                    .set_playback_switch_all(0)
                    .expect("Could not disable playback (set mute) on Alsa");
            } else if self.switched_off() {
                debug!("Enabling playback (unsetting mute) on Alsa");
                simple_element
                    .set_playback_switch_all(1)
                    .expect("Could not enable playback (unset mute) on Alsa");
            }
        }

        let mapped_volume = self.config.volume_ctrl.map(volume);

        if self.use_raw_linear() {
            let scaled_volume = (self.min as f32 + mapped_volume * self.range as f32) as i64;
            debug!("Setting Alsa raw volume to {}", scaled_volume);
            simple_element
                .set_playback_volume_all(scaled_volume)
                .expect("Could not set Alsa raw volume");
            return;
        }

        let db_volume = if self.use_linear_in_db {
            self.min_db + mapped_volume * self.db_range
        } else if volume == 0 {
            // prevent ratio_to_db(0.0) from returning -inf
            MilliBel(SND_CTL_TLV_DB_GAIN_MUTE).to_db()
        } else {
            ratio_to_db(mapped_volume) + self.max_db
        };

        debug!("Setting Alsa volume to {:.2} dB", db_volume);
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

    fn use_raw_linear(&self) -> bool {
        self.is_softvol
            || (!self.use_linear_in_db && matches!(self.config.volume_ctrl, VolumeCtrl::Linear))
    }
}
