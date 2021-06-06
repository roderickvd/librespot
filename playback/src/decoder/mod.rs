use std::fmt;

mod lewton_decoder;
pub use lewton_decoder::{VorbisDecoder, VorbisError};

mod passthrough_decoder;
pub use passthrough_decoder::{PassthroughDecoder, PassthroughError};

mod symphonia_decoder;
pub use symphonia_decoder::{SymphoniaDecoder, SymphoniaError};

pub enum AudioPacket {
    Samples(Vec<f64>),
    OggData(Vec<u8>),
}

impl AudioPacket {
    pub fn samples_from_f32(f32_samples: Vec<f32>) -> Self {
        let f64_samples = f32_samples.iter().map(|sample| *sample as f64).collect();
        AudioPacket::Samples(f64_samples)
    }

    pub fn samples_from_i16(i16_samples: Vec<i16>) -> Self {
        let f64_samples = i16_samples
            .iter()
            .map(|sample| *sample as f64 / 32768.)
            .collect();
        AudioPacket::Samples(f64_samples)
    }

    pub fn samples(&self) -> &[f64] {
        match self {
            AudioPacket::Samples(s) => s,
            AudioPacket::OggData(_) => panic!("can't return OggData on samples"),
        }
    }

    pub fn oggdata(&self) -> &[u8] {
        match self {
            AudioPacket::Samples(_) => panic!("can't return samples on OggData"),
            AudioPacket::OggData(d) => d,
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            AudioPacket::Samples(s) => s.is_empty(),
            AudioPacket::OggData(d) => d.is_empty(),
        }
    }
}

#[derive(Debug)]
pub enum AudioError {
    PassthroughError(PassthroughError),
    SymphoniaError(SymphoniaError),
    VorbisError(VorbisError),
}

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            AudioError::PassthroughError(err) => write!(f, "PassthroughError({})", err),
            AudioError::SymphoniaError(err) => write!(f, "SymphoniaError({})", err),
            AudioError::VorbisError(err) => write!(f, "VorbisError({})", err),
        }
    }
}

impl From<PassthroughError> for AudioError {
    fn from(err: PassthroughError) -> AudioError {
        AudioError::PassthroughError(err)
    }
}

impl From<SymphoniaError> for AudioError {
    fn from(err: SymphoniaError) -> AudioError {
        AudioError::SymphoniaError(err)
    }
}

impl From<VorbisError> for AudioError {
    fn from(err: VorbisError) -> AudioError {
        AudioError::VorbisError(err)
    }
}

pub trait AudioDecoder {
    fn seek(&mut self, ms: i64) -> Result<(), AudioError>;
    fn next_packet(&mut self) -> Result<Option<AudioPacket>, AudioError>;
}
