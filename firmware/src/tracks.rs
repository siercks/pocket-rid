use core::cell::RefCell;

use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use odid::{Message, PackIter};

use crate::radio::RawObs;

pub const MAX_TRACKS: usize = 16;
pub const HIDE_AFTER_US: u64 = 10_000_000;

#[derive(Clone, Copy)]
pub struct Track {
    pub mac: [u8; 6],
    #[expect(dead_code, reason = "shown by the DETAIL view in M8")]
    pub first_seen_us: u64,
    pub last_seen_us: u64,
    pub frames: u32,
    pub last_counter: u8,
    pub channel: u8,
    pub rssi_last: i8,
    pub rssi_avg_x16: i16,
    /// [0]: id_type 1 (serial); [1]: any other id_type.
    pub basic_id: [Option<odid::BasicId>; 2],
    pub location: Option<odid::Location>,
    pub location_t_us: u64,
    pub system: Option<odid::System>,
    pub operator_id: Option<odid::OperatorId>,
    pub self_id: Option<odid::SelfId>,
    pub auth_msgs: u16,
}

pub struct TrackTable {
    slots: [Option<Track>; MAX_TRACKS],
}

pub static TRACKS: Mutex<CriticalSectionRawMutex, RefCell<TrackTable>> =
    Mutex::new(RefCell::new(TrackTable {
        slots: [None; MAX_TRACKS],
    }));

impl TrackTable {
    pub fn update(&mut self, obs: &RawObs, msgs: PackIter<'_>) {
        let i = self
            .slots
            .iter()
            .position(|s| s.is_some_and(|t| t.mac == obs.src_mac))
            .or_else(|| self.slots.iter().position(Option::is_none))
            .or_else(|| {
                (0..MAX_TRACKS).min_by_key(|&i| self.slots[i].map_or(0, |t| t.last_seen_us))
            })
            .unwrap_or(0);
        if self.slots[i].is_none_or(|t| t.mac != obs.src_mac) {
            self.slots[i] = Some(Track {
                mac: obs.src_mac,
                first_seen_us: obs.t_dev_us,
                last_seen_us: 0,
                frames: 0,
                last_counter: 0,
                channel: 0,
                rssi_last: 0,
                rssi_avg_x16: i16::from(obs.rssi) * 16,
                basic_id: [None; 2],
                location: None,
                location_t_us: 0,
                system: None,
                operator_id: None,
                self_id: None,
                auth_msgs: 0,
            });
        }
        let Some(t) = self.slots[i].as_mut() else {
            return;
        };
        t.last_seen_us = obs.t_dev_us;
        t.last_counter = obs.msg_counter;
        t.channel = obs.channel;
        t.rssi_last = obs.rssi;
        t.frames = t.frames.saturating_add(1);
        let avg = i32::from(t.rssi_avg_x16);
        t.rssi_avg_x16 = (avg + (i32::from(obs.rssi) * 16 - avg) / 8) as i16;
        for m in msgs.filter_map(|m| odid::decode_message(m).ok()) {
            match m {
                Message::BasicId(b) => t.basic_id[usize::from(b.id_type != 1)] = Some(b),
                Message::Location(l) => {
                    t.location = Some(l);
                    t.location_t_us = obs.t_dev_us;
                }
                Message::Auth(_) => t.auth_msgs = t.auth_msgs.saturating_add(1),
                Message::SelfId(s) => t.self_id = Some(s),
                Message::System(s) => t.system = Some(s),
                Message::OperatorId(o) => t.operator_id = Some(o),
            }
        }
    }

    pub fn visible_count(&self, now_us: u64) -> usize {
        self.slots
            .iter()
            .flatten()
            .filter(|t| now_us.saturating_sub(t.last_seen_us) <= HIDE_AFTER_US)
            .count()
    }
}
