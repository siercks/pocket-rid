use core::sync::atomic::AtomicU32;

pub static MGMT_FRAMES: AtomicU32 = AtomicU32::new(0);
pub static BEACONS: AtomicU32 = AtomicU32::new(0);
pub static RID_FRAMES: AtomicU32 = AtomicU32::new(0);
pub static BEACON_ERRORS: AtomicU32 = AtomicU32::new(0);
#[expect(dead_code, reason = "used from M7")]
pub static PACK_ERRORS: AtomicU32 = AtomicU32::new(0);
pub static OBS_DROPPED: AtomicU32 = AtomicU32::new(0);
#[expect(dead_code, reason = "used from M7")]
pub static TX_DROPPED: AtomicU32 = AtomicU32::new(0);
pub static RADIO_ERRORS: AtomicU32 = AtomicU32::new(0);
#[expect(dead_code, reason = "used from M7")]
pub static LOG_DROPPED: AtomicU32 = AtomicU32::new(0);
