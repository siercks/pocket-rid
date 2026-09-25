use core::sync::atomic::Ordering::Relaxed;

use rid_proto::{Obs, Payload};

use crate::radio::OBS_CH;
use crate::status::{PACK_ERRORS, TX_DROPPED};
use crate::tracks::TRACKS;
use crate::wire::send;

#[embassy_executor::task]
pub async fn obs_task() {
    loop {
        let obs = OBS_CH.receive().await;
        let pack = &obs.pack[..usize::from(obs.pack_len)];
        // Forwarded raw before parsing, so the host sees packs the device rejects.
        let frame = Obs {
            channel: obs.channel,
            rssi_dbm: obs.rssi,
            source: 1,
            src_mac: obs.src_mac,
            msg_counter: obs.msg_counter,
            t_mac_us: obs.t_mac_us,
            ie_len: obs.ie_len,
            pack,
        };
        send(obs.t_dev_us, Payload::Obs(frame), &TX_DROPPED);
        match odid::parse_pack(pack) {
            Ok(msgs) => TRACKS.lock(|t| t.borrow_mut().update(&obs, msgs)),
            Err(_) => {
                PACK_ERRORS.fetch_add(1, Relaxed);
            }
        }
    }
}
