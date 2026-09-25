use core::cell::Cell;
use core::fmt::Write as _;
use core::sync::atomic::{AtomicU32, Ordering::Relaxed};

use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use esp_hal::time::Instant;
use rid_proto::{Frame, FrameBuf, LOG_TEXT_CAP, Log, Payload};

use crate::status::LOG_DROPPED;

pub static TX_CH: Channel<CriticalSectionRawMutex, FrameBuf, 16> = Channel::new();
static TX_LOCK: Mutex<CriticalSectionRawMutex, Cell<u32>> = Mutex::new(Cell::new(0));

pub fn now_us() -> u64 {
    Instant::now().duration_since_epoch().as_micros()
}

/// Sequencing and queueing share one lock so frames from any context reach `TX_CH` in `seq` order.
pub fn send(t_dev_us: u64, payload: Payload, dropped: &AtomicU32) {
    TX_LOCK.lock(|next| {
        let seq = next.get();
        next.set(seq.wrapping_add(1));
        let queued = FrameBuf::encode(&Frame {
            seq,
            t_dev_us,
            payload,
        })
        .is_some_and(|fb| TX_CH.try_send(fb).is_ok());
        if !queued {
            dropped.fetch_add(1, Relaxed);
        }
    });
}

#[cfg(not(feature = "console-text"))]
#[embassy_executor::task]
pub async fn tx_task(usb: esp_hal::peripherals::USB_DEVICE<'static>) {
    use embedded_io_async::Write;
    let mut usb = esp_hal::usb_serial_jtag::UsbSerialJtag::new(usb).into_async();
    loop {
        let fb = TX_CH.receive().await;
        // With no host reading, this stalls; producers then drop and count.
        let _ = usb.write_all(fb.as_bytes()).await;
    }
}

#[cfg(feature = "console-text")]
#[embassy_executor::task]
pub async fn tx_task() {
    loop {
        let fb = TX_CH.receive().await;
        let bytes = fb.as_bytes();
        let mut raw = [0u8; rid_proto::MAX_RAW];
        let Some(n) = rid_proto::cobs_decode(&bytes[..bytes.len() - 1], &mut raw) else {
            continue;
        };
        let Ok(f) = rid_proto::decode_raw(&raw[..n]) else {
            continue;
        };
        let seq = f.seq;
        match f.payload {
            Payload::Hello(h) => esp_println::println!(
                "HELLO seq={seq} fw={} ch={}",
                core::str::from_utf8(&h.fw_version)
                    .unwrap_or("?")
                    .trim_end_matches('\0'),
                h.boot_channel
            ),
            Payload::Obs(o) => {
                let m = o.src_mac;
                esp_println::println!(
                    "OBS seq={seq} ch={} rssi={} mac={:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x} ctr={} pack={}B",
                    o.channel,
                    o.rssi_dbm,
                    m[0],
                    m[1],
                    m[2],
                    m[3],
                    m[4],
                    m[5],
                    o.msg_counter,
                    o.pack.len()
                )
            }
            Payload::Status(s) => esp_println::println!(
                "STATUS seq={seq} up={}s ch={} mode={} trk={} heap={} counters={:?}",
                s.uptime_s,
                s.channel,
                s.mode,
                s.tracks_active,
                s.heap_free,
                s.counters
            ),
            Payload::Log(l) => esp_println::println!(
                "LOG seq={seq} level={} {}",
                l.level,
                core::str::from_utf8(l.text).unwrap_or("?")
            ),
        }
    }
}

/// Fixed-size text buffer that stops at the last whole char that fits.
struct Text {
    buf: [u8; LOG_TEXT_CAP],
    len: usize,
}

impl core::fmt::Write for Text {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for c in s.chars() {
            let end = self.len + c.len_utf8();
            if end > LOG_TEXT_CAP {
                break;
            }
            c.encode_utf8(&mut self.buf[self.len..end]);
            self.len = end;
        }
        Ok(())
    }
}

pub struct FrameLogger;

impl log::Log for FrameLogger {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Info
    }

    fn log(&self, r: &log::Record) {
        if !self.enabled(r.metadata()) {
            return;
        }
        let mut t = Text {
            buf: [0; LOG_TEXT_CAP],
            len: 0,
        };
        let _ = write!(t, "{}", r.args());
        let log = Log {
            level: r.level() as u8,
            text: &t.buf[..t.len],
        };
        send(now_us(), Payload::Log(log), &LOG_DROPPED);
    }

    fn flush(&self) {}
}
