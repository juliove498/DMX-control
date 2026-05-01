pub mod artnet;
pub mod config;
pub mod d2xx;
pub mod discovery;
pub mod enttec;
pub mod mock;
pub mod open_dmx;
pub mod sacn;

use crate::engine::DMX_CHANNELS;

#[derive(Debug, thiserror::Error)]
pub enum OutputError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid config: {0}")]
    Config(String),
}

pub trait OutputDriver: Send {
    fn name(&self) -> &'static str;
    fn send(&mut self, universe: u16, data: &[u8; DMX_CHANNELS]) -> Result<(), OutputError>;
}
