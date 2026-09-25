use core::sync::atomic::{AtomicU32, Ordering::Relaxed};

use embassy_time::{Duration, Ticker};
use rid_proto::{Hello, Payload, Status};

use crate::radio::{CURRENT_CHANNEL, HOP_MODE};
use crate::tracks::TRACKS;
use crate::wire::{now_us, send};

pub static MGMT_FRAMES: AtomicU32 = AtomicU32::new(0);
pub static BEACONS: AtomicU32 = AtomicU32::new(0);
pub static RID_FRAMES: AtomicU32 = AtomicU32::new(0);
pub static BEACON_ERRORS: AtomicU32 = AtomicU32::new(0);
pub static PACK_ERRORS: AtomicU32 = AtomicU32::new(0);
pub static OBS_DROPPED: AtomicU32 = AtomicU32::new(0);
pub static TX_DROPPED: AtomicU32 = AtomicU32::new(0);
pub static RADIO_ERRORS: AtomicU32 = AtomicU32::new(0);
pub static LOG_DROPPED: AtomicU32 = AtomicU32::new(0);

pub fn send_hello(device_mac: [u8; 6]) {
    let mut fw_version = [0; 16];
    let v = env!("CARGO_PKG_VERSION").as_bytes();
    fw_version[..v.len()].copy_from_slice(v);
    let mut git_sha = [0; 8];
    let s = option_env!("RID_GIT_SHA").unwrap_or("unknown").as_bytes();
    let n = s.len().min(8);
    git_sha[..n].copy_from_slice(&s[..n]);
    let hello = Hello {
        fw_version,
        git_sha,
        device_mac,
        boot_channel: 6,
    };
    send(now_us(), Payload::Hello(hello), &TX_DROPPED);
}

#[embassy_executor::task]
pub async fn status_task(device_mac: [u8; 6]) {
    let mut ticker = Ticker::every(Duration::from_millis(1000));
    let mut pass: u32 = 0;
    loop {
        ticker.next().await;
        pass = pass.wrapping_add(1);
        let now = now_us();
        let counters = [
            &MGMT_FRAMES,
            &BEACONS,
            &RID_FRAMES,
            &BEACON_ERRORS,
            &PACK_ERRORS,
            &OBS_DROPPED,
            &TX_DROPPED,
            &RADIO_ERRORS,
            &LOG_DROPPED,
        ]
        .map(|c| c.load(Relaxed));
        let status = Status {
            uptime_s: (now / 1_000_000) as u32,
            channel: CURRENT_CHANNEL.load(Relaxed),
            mode: u8::from(HOP_MODE.load(Relaxed)),
            tracks_active: TRACKS.lock(|t| t.borrow().visible_count(now)) as u8,
            heap_free: esp_alloc::HEAP.free() as u32,
            counters,
        };
        send(now, Payload::Status(status), &TX_DROPPED);
        if pass.is_multiple_of(30) {
            send_hello(device_mac);
        }
    }
}
