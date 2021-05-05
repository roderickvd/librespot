use super::{MappedCtrl, VolumeCtrl};
use super::{Mixer, MixerConfig};

use alsa::ctl::{ElemId, ElemIface};
use alsa::mixer::{MilliBel, SelemChannelId, SelemId};
use alsa::Ctl;

use std::ffi::CString;

#[derive(Clone)]
pub struct AlsaMixer {
    config: MixerConfig,
    min: i64,
    max: i64,
    has_switch: bool,
}

// None of these are Send and cannot be stored in a Mixer struct,
// so resort to using a macro for DRYness.
macro_rules! get_simple_element {
    ($name: ident, $config: expr) => {
        let mixer = alsa::mixer::Mixer::new(&$config.card, false).expect("Unable to open mixer");
        let simple_element_id = SelemId::new(&$config.control, $config.index);
        let $name = mixer
            .find_selem(&simple_element_id)
            .expect("Unable to find mixer control");
    };
}

impl Mixer for AlsaMixer {
    fn open(config: &mut MixerConfig) -> Self {
        info!(
            "Mixing with alsa and volume control: {:?} for card: {} with mixer control: {},{}",
            config.volume_ctrl, config.card, config.control, config.index,
        );

        get_simple_element!(simple_element, config);

        let (min, max) = simple_element.get_playback_volume_range();
        let has_switch = simple_element.has_playback_switch();

        // query the dB range if not overriden with a command line option
        if !config.volume_ctrl.range_ok() {
            // Alsa exposes different APIs for hardware and software mixers
            let is_softvol = simple_element
                .get_playback_vol_db(SelemChannelId::mono())
                .is_err();

            debug!("Alsa mixer control is softvol: {}", is_softvol);

            let (min_millibel, max_millibel) = match is_softvol {
                false => simple_element.get_playback_db_range(),
                true => {
                    let control = Ctl::new(&config.card, false)
                        .expect("Unable to open Alsa software volume control");
                    let mut element_id = ElemId::new(ElemIface::Mixer);
                    element_id.set_name(&CString::new(config.control.as_str()).unwrap());
                    element_id.set_index(config.index);
                    control.get_db_range(&element_id).unwrap_or((
                        MilliBel(0),
                        MilliBel::from_db(VolumeCtrl::DEFAULT_DB_RANGE.into()),
                    ))
                }
            };

            // these parameters might be configured as negative...
            let min_db = min_millibel.to_db() as i8;
            let max_db = max_millibel.to_db() as i8;
            // ...but we only care about the absolute range
            let db_range = i8::abs(max_db - min_db) as u8;

            debug!("dB volume range: [{}..{}]", min_db, max_db);
            config.volume_ctrl.set_db_range(db_range);
        }

        // Many implementations continue to work with the dB API and set up a
        // linear mapping for mixers with ranges <= 24 dB as well. The first is
        // not necessary because we map dB logarithmic and cubic curves to raw
        // values. The second is an unncessary optimization also, because our
        // mappings already provide a linear experience for such low ranges.

        debug!("Raw volume range: [{}..{}]", min, max);
        debug!("Support for playback (mute) switch: {}", has_switch);

        Self {
            config: config.clone(),
            min,
            max,
            has_switch,
        }
    }

    fn volume(&self) -> u16 {
        get_simple_element!(simple_element, self.config);

        if self.switched_off() {
            return 0;
        }

        let alsa_volume = simple_element
            .get_playback_volume(SelemChannelId::mono())
            .expect("Couldn't get current volume");

        let mapped_volume = alsa_volume / i64::abs(self.max - self.min) - self.min;
        self.config.volume_ctrl.unmap(mapped_volume as f32)
    }

    fn set_volume(&self, volume: u16) {
        get_simple_element!(simple_element, self.config);

        let mapped_volume = self.config.volume_ctrl.map(volume);

        if self.has_switch {
            if f32::abs(mapped_volume - 0.0) <= f32::EPSILON {
                simple_element
                    .set_playback_switch_all(0)
                    .expect("Could not disable playback (set mute)");
            } else if self.switched_off() {
                simple_element
                    .set_playback_switch_all(1)
                    .expect("Could not enable playback (unset mute)");
            }
        }

        let alsa_volume =
            (self.min as f32 + mapped_volume * i64::abs(self.max - self.min) as f32) as i64;
        debug!("Setting Alsa raw volume to {}", alsa_volume);

        simple_element
            .set_playback_volume_all(alsa_volume as i64)
            .expect("Could not set volume");
    }
}

impl AlsaMixer {
    fn switched_off(&self) -> bool {
        if !self.has_switch {
            return false;
        }

        get_simple_element!(simple_element, self.config);

        simple_element
            .get_playback_switch(SelemChannelId::mono())
            .map(|b| b == 0)
            .unwrap_or(false)
    }
}
