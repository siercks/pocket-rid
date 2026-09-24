#![no_std]
#![forbid(unsafe_code)]
#![deny(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects
)]

mod beacon;
#[cfg(feature = "encode")]
mod encode;
mod message;
mod pack;
mod units;

pub use beacon::{BeaconError, BeaconRid, parse_beacon};
#[cfg(feature = "encode")]
pub use encode::*;
pub use message::*;
pub use pack::{PackError, PackIter, parse_pack};

pub const MESSAGE_SIZE: usize = 25;
pub const MAX_PACK_MESSAGES: usize = 9;
pub const ASTM_OUI: [u8; 3] = [0xFA, 0x0B, 0xBC];
pub const ODID_OUI_TYPE: u8 = 0x0D;
pub const ODID_EPOCH_UNIX_S: u64 = 1_546_300_800;
