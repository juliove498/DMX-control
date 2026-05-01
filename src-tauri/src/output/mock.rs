use super::{OutputDriver, OutputError};
use crate::engine::DMX_CHANNELS;

pub struct MockDriver;

impl OutputDriver for MockDriver {
    fn name(&self) -> &'static str {
        "mock"
    }

    fn send(&mut self, universe: u16, data: &[u8; DMX_CHANNELS]) -> Result<(), OutputError> {
        let nonzero = data.iter().filter(|&&b| b != 0).count();
        tracing::trace!(target: "dmx::mock", universe, nonzero, "frame");
        Ok(())
    }
}
