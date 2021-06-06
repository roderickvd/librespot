use super::{AudioDecoder, AudioError, AudioPacket};

use std::error;
use std::fmt;
use std::io::{Read, Seek};
use std::marker::PhantomData;

pub struct SymphoniaDecoder<R: Read + Seek>(Box<dyn symphonia::core::io::MediaSource>, PhantomData<R>);
pub struct SymphoniaError(symphonia::core::errors::Error);

impl<R> SymphoniaDecoder<R>
where
    R: Read + Seek,
{
    pub fn new(input: R) -> Result<SymphoniaDecoder<R>, SymphoniaError> {
        Ok(SymphoniaDecoder(input?))
    }
}

impl<R> AudioDecoder for SymphoniaDecoder<R>
where
    R: Read + Seek,
{
    fn seek(&mut self, ms: i64) -> Result<(), AudioError> {
        let absgp = symphonia::core::units::Time::from(ms as u64 * crate::SAMPLE_RATE as u64);
        match self.0.seek(absgp as u64) {
            Ok(_) => Ok(()),
            Err(err) => Err(AudioError::SymphoniaError(err.into())),
        }
    }

    fn next_packet(&mut self) -> Result<Option<AudioPacket>, AudioError> {
        loop {
            match self.0.next_packet() {
                Ok(Some(packet)) => return Ok(Some(AudioPacket::samples_from_i16(packet.samples))),
                Ok(None) => return Ok(None),
                Err(err) => return Err(AudioError::SymphoniaError(err.into())),
            }
        }
    }
}

impl From<symphonia::core::errors::Error> for SymphoniaError {
    fn from(err: symphonia::core::errors::Error) -> SymphoniaError {
        SymphoniaError(err)
    }
}

impl fmt::Debug for SymphoniaError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl fmt::Display for SymphoniaError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl error::Error for SymphoniaError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        error::Error::source(&self.0)
    }
}
