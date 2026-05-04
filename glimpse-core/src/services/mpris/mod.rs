pub mod model;
pub mod mpris_client;
pub mod service;

pub use model::{Artwork, Command, PlaybackStatus, Player, State};
pub use service::{MprisHandle, MprisService};
