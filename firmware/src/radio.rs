use core::sync::atomic::{AtomicBool, AtomicU8, Ordering::Relaxed};

use embassy_futures::select::{Either, select};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::{Duration, Ticker};
use esp_hal::time::Instant;
use esp_radio::wifi::sniffer::{PromiscuousPkt, Sniffer};
use esp_radio::wifi::{SecondaryChannel, WifiController};
use rid_proto::PACK_CAP;

use crate::status::{BEACON_ERRORS, BEACONS, MGMT_FRAMES, OBS_DROPPED, RADIO_ERRORS, RID_FRAMES};

pub struct RawObs {
    pub t_dev_us: u64,
    pub t_mac_us: u32,
    pub channel: u8,
    pub rssi: i8,
    pub src_mac: [u8; 6],
    pub msg_counter: u8,
    pub ie_len: u8,
    pub pack_len: u8,
    pub pack: [u8; PACK_CAP],
}

pub static OBS_CH: Channel<CriticalSectionRawMutex, RawObs, 32> = Channel::new();

#[derive(Clone, Copy)]
#[expect(dead_code, reason = "sent by button_task in M8")]
pub enum RadioCmd {
    SetFixed(u8),
    SetHop,
}

pub static RADIO_CMD: Channel<CriticalSectionRawMutex, RadioCmd, 4> = Channel::new();
pub static CURRENT_CHANNEL: AtomicU8 = AtomicU8::new(6);
pub static HOP_MODE: AtomicBool = AtomicBool::new(false);

const HOP_SEQUENCE: [u8; 3] = [1, 6, 11];

/// Runs in the Wi-Fi driver's context: filter, copy, enqueue. Nothing here may block or allocate.
pub fn sniffer_cb(pkt: PromiscuousPkt<'_>) {
    // 0 = WIFI_PKT_MGMT in ESP-IDF.
    if pkt.frame_type != 0 {
        return;
    }
    MGMT_FRAMES.fetch_add(1, Relaxed);
    if pkt.rx_cntl.rx_state != 0 {
        return;
    }
    let n = pkt.len;
    // `sig_len` includes the 4-byte FCS.
    let Some(mpdu) = pkt.data.get(..n.saturating_sub(4)).filter(|_| n >= 40) else {
        return;
    };
    if mpdu.first() != Some(&0x80) {
        return;
    }
    BEACONS.fetch_add(1, Relaxed);
    let rid = match odid::parse_beacon(mpdu) {
        Ok(rid) => rid,
        Err(odid::BeaconError::NotRemoteId) => return,
        Err(_) => {
            BEACON_ERRORS.fetch_add(1, Relaxed);
            return;
        }
    };
    RID_FRAMES.fetch_add(1, Relaxed);
    let pack_len = rid.pack.len().min(PACK_CAP);
    let mut obs = RawObs {
        t_dev_us: Instant::now().duration_since_epoch().as_micros(),
        t_mac_us: pkt.rx_cntl.timestamp.duration_since_epoch().as_micros() as u32,
        channel: pkt.rx_cntl.channel as u8,
        rssi: pkt.rx_cntl.rssi.clamp(-128, 127) as i8,
        src_mac: rid.src_mac,
        msg_counter: rid.msg_counter,
        ie_len: (rid.pack.len() + 5) as u8,
        pack_len: pack_len as u8,
        pack: [0; PACK_CAP],
    };
    obs.pack[..pack_len].copy_from_slice(&rid.pack[..pack_len]);
    if OBS_CH.try_send(obs).is_err() {
        OBS_DROPPED.fetch_add(1, Relaxed);
    }
}

fn apply(controller: &mut WifiController<'static>, ch: u8) {
    match controller.set_channel(ch, SecondaryChannel::None) {
        Ok(()) => CURRENT_CHANNEL.store(ch, Relaxed),
        Err(e) => {
            RADIO_ERRORS.fetch_add(1, Relaxed);
            log::warn!("set_channel({ch}) failed: {e:?}");
        }
    }
}

/// Owns the controller for the program's lifetime: dropping it deinitializes Wi-Fi.
#[embassy_executor::task]
pub async fn radio_task(mut controller: WifiController<'static>, _sniffer: Sniffer<'static>) {
    let mut ticker = Ticker::every(Duration::from_millis(500));
    let mut hop = 0;
    loop {
        match select(RADIO_CMD.receive(), ticker.next()).await {
            Either::First(RadioCmd::SetFixed(ch)) if (1..=13).contains(&ch) => {
                HOP_MODE.store(false, Relaxed);
                apply(&mut controller, ch);
            }
            Either::First(RadioCmd::SetFixed(_)) => {}
            Either::First(RadioCmd::SetHop) => {
                HOP_MODE.store(true, Relaxed);
                hop = 0;
                apply(&mut controller, HOP_SEQUENCE[0]);
                ticker.reset();
            }
            Either::Second(()) if HOP_MODE.load(Relaxed) => {
                hop = (hop + 1) % HOP_SEQUENCE.len();
                apply(&mut controller, HOP_SEQUENCE[hop]);
            }
            Either::Second(()) => {}
        }
    }
}
