use core::fmt::Write as _;
use core::sync::atomic::Ordering::Relaxed;

use embassy_futures::select::{Either, select};
use embassy_futures::yield_now;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::Timer;
use embedded_graphics::mono_font::ascii::{FONT_6X10, FONT_8X13, FONT_8X13_BOLD};
use embedded_graphics::mono_font::{MonoFont, MonoTextStyle};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::{Baseline, Text};
use esp_hal::delay::Delay;
use mipidsi::Builder;
use mipidsi::interface::{Generic8BitBus, ParallelInterface};
use mipidsi::models::ST7789;
use mipidsi::options::{ColorInversion, Orientation, Rotation};

use crate::board::{PanelKeepAlive, PanelPins};
use crate::radio::{CURRENT_CHANNEL, HOP_MODE};
use crate::status::{OBS_DROPPED, RID_FRAMES, TX_DROPPED};
use crate::tracks::{SNAPSHOT_ROWS, TRACKS, Track};
use crate::wire::now_us;

#[derive(Clone, Copy)]
pub enum UiEvt {
    NextTrack,
    ToggleView,
    ClearTracks,
    Redraw,
}

pub static UI_EVT: Channel<CriticalSectionRawMutex, UiEvt, 8> = Channel::new();

const LIST_ROWS: usize = 6;
/// Header, then up to 15 DETAIL lines or 6 × 2 LIST lines plus the empty-state line.
const LINES: usize = 16;

/// Fixed line buffer: truncates silently, stores non-printable bytes as `?`.
#[derive(Clone, Copy, PartialEq)]
struct FmtBuf<const N: usize> {
    buf: [u8; N],
    len: usize,
}

impl<const N: usize> FmtBuf<N> {
    const fn new() -> Self {
        Self {
            buf: [0; N],
            len: 0,
        }
    }
    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }
}

impl<const N: usize> core::fmt::Write for FmtBuf<N> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for b in s.bytes() {
            if self.len == N {
                break;
            }
            self.buf[self.len] = if (0x20..0x7F).contains(&b) { b } else { b'?' };
            self.len += 1;
        }
        Ok(())
    }
}

type Line = FmtBuf<64>;

#[derive(Clone, Copy, PartialEq)]
struct Drawn {
    text: Line,
    color: Rgb565,
}

const BLANK: Drawn = Drawn {
    text: Line::new(),
    color: Rgb565::BLACK,
};

/// Where a line goes: a full-width band at `y`, cleared to `bg`, with text at (`x`, `y + dy`).
struct Slot {
    x: i32,
    y: i32,
    dy: i32,
    h: u32,
    font: &'static MonoFont<'static>,
    bg: Rgb565,
}

async fn draw<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    cache: &mut Drawn,
    slot: Slot,
    text: Line,
    color: Rgb565,
) {
    let new = Drawn { text, color };
    if *cache == new {
        return;
    }
    Rectangle::new(Point::new(0, slot.y), Size::new(320, slot.h))
        .into_styled(PrimitiveStyle::with_fill(slot.bg))
        .draw(d)
        .ok();
    Text::with_baseline(
        text.as_str(),
        Point::new(slot.x, slot.y + slot.dy),
        MonoTextStyle::new(slot.font, color),
        Baseline::Top,
    )
    .draw(d)
    .ok();
    *cache = new;
    yield_now().await;
}

fn opt(b: &mut Line, v: Option<f32>, prec: usize) {
    let _ = match v {
        Some(v) => write!(b, "{v:.prec$}"),
        None => b.write_str("---"),
    };
}

fn mac(b: &mut Line, m: &[u8; 6]) {
    let _ = write!(
        b,
        "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        m[0], m[1], m[2], m[3], m[4], m[5]
    );
}

fn label(t: &Track) -> Line {
    let mut b = Line::new();
    match t.basic_id.iter().flatten().next() {
        Some(id) => {
            let _ = b.write_str(id.as_str());
        }
        None => mac(&mut b, &t.mac),
    }
    b
}

fn secs(now_us: u64, t_us: u64) -> u64 {
    now_us.saturating_sub(t_us) / 1_000_000
}

fn status_letter(t: &Track) -> char {
    match t.location.map(|l| l.status) {
        None => '-',
        Some(0) => 'U',
        Some(1) => 'G',
        Some(2) => 'A',
        Some(3) => 'E',
        Some(_) => 'F',
    }
}

fn status_color(t: &Track) -> Rgb565 {
    match t.location.map(|l| l.status) {
        Some(3) => Rgb565::RED,
        Some(4) => Rgb565::YELLOW,
        _ => Rgb565::WHITE,
    }
}

const STATUS_NAMES: [&str; 5] = ["UNDECLARED", "GROUND", "AIRBORNE", "EMERGENCY", "RID-FAIL"];
const UA_NAMES: [&str; 16] = [
    "NONE",
    "AEROPLANE",
    "MULTIROTOR",
    "GYROPLANE",
    "HYBRID",
    "ORNITHOPTER",
    "GLIDER",
    "KITE",
    "FREE-BALLOON",
    "CAPTIVE-BALLOON",
    "AIRSHIP",
    "PARACHUTE",
    "ROCKET",
    "TETHERED",
    "GROUND-OBSTACLE",
    "OTHER",
];

fn detail_line(k: usize, t: &Track, now: u64) -> Line {
    let mut b = Line::new();
    let loc = t.location;
    let sys = t.system;
    let _ = match k {
        0 => write!(
            b,
            "ID  {}",
            t.basic_id[0].as_ref().map_or("", |i| i.as_str())
        ),
        1 => write!(
            b,
            "ID2 {}",
            t.basic_id[1].as_ref().map_or("", |i| i.as_str())
        ),
        2 => {
            let _ = b.write_str("MAC ");
            mac(&mut b, &t.mac);
            write!(
                b,
                " CH {} RSSI {} ({})",
                t.channel,
                t.rssi_last,
                t.rssi_avg_x16 / 16
            )
        }
        3 => {
            let st = loc
                .and_then(|l| STATUS_NAMES.get(usize::from(l.status)))
                .unwrap_or(&"---");
            let ua = t
                .basic_id
                .iter()
                .flatten()
                .next()
                .and_then(|i| UA_NAMES.get(usize::from(i.ua_type)));
            write!(b, "ST {st} UA {}", ua.unwrap_or(&"---"))
        }
        4 => {
            let _ = b.write_str("LAT ");
            match loc.and_then(|l| l.latitude_deg().zip(l.longitude_deg())) {
                Some((lat, lon)) => write!(b, "{lat:.7} LON {lon:.7}"),
                None => b.write_str("--- LON ---"),
            }
        }
        5 => {
            let _ = b.write_str("ALT geo ");
            opt(&mut b, loc.and_then(|l| l.alt_geo_m()), 1);
            let _ = b.write_str("m baro ");
            opt(&mut b, loc.and_then(|l| l.alt_baro_m()), 1);
            b.write_str("m")
        }
        6 => {
            let _ = b.write_str("HGT ");
            opt(&mut b, loc.and_then(|l| l.height_m()), 1);
            let reference = match loc.map(|l| l.height_type) {
                Some(0) => "TO",
                Some(_) => "AGL",
                None => "---",
            };
            write!(b, "m {reference}")
        }
        7 => {
            let _ = b.write_str("SPD ");
            opt(&mut b, loc.and_then(|l| l.speed_h_mps()), 2);
            let _ = b.write_str("m/s VS ");
            match loc.and_then(|l| l.speed_v_mps()) {
                Some(v) => {
                    let _ = write!(b, "{v:+.1}");
                }
                None => {
                    let _ = b.write_str("---");
                }
            }
            match loc.and_then(|l| l.track_deg()) {
                Some(trk) => write!(b, " TRK {trk}"),
                None => b.write_str(" TRK ---"),
            }
        }
        8 => {
            let _ = b.write_str("TS ");
            opt(&mut b, loc.and_then(|l| l.timestamp_s()), 1);
            match loc {
                Some(_) => write!(b, "s past hour, fix age {}s", secs(now, t.location_t_us)),
                None => b.write_str("s past hour, fix age ---"),
            }
        }
        9 => {
            let _ = b.write_str("OP ");
            match sys.and_then(|s| s.latitude_deg().zip(s.longitude_deg())) {
                Some((lat, lon)) => write!(b, "{lat:.7} {lon:.7}"),
                None => b.write_str("--- ---"),
            }
        }
        10 => {
            let _ = b.write_str("OPALT ");
            opt(&mut b, sys.and_then(|s| s.op_alt_geo_m()), 1);
            let kind = match sys.map(|s| s.operator_location_type) {
                Some(0) => "TAKEOFF",
                Some(1) => "LIVE",
                Some(2) => "FIXED",
                _ => "---",
            };
            write!(b, "m {kind}")
        }
        11 => write!(
            b,
            "OPID {}",
            t.operator_id.as_ref().map_or("", |o| o.as_str())
        ),
        12 => write!(b, "SELF {}", t.self_id.as_ref().map_or("", |s| s.as_str())),
        13 => write!(
            b,
            "FRAMES {} CTR {} AUTH {}",
            t.frames, t.last_counter, t.auth_msgs
        ),
        _ => write!(b, "SEEN {}s ago", secs(now, t.first_seen_us)),
    };
    b
}

#[embassy_executor::task]
pub async fn ui_task(pins: PanelPins) {
    let PanelPins {
        pwr,
        rd,
        cs,
        dc,
        wr,
        rst,
        mut bl,
        d,
    } = pins;
    let [d0, d1, d2, d3, d4, d5, d6, d7] = d;
    let bus = Generic8BitBus::new((d0, d1, d2, d3, d4, d5, d6, d7));
    let di = ParallelInterface::new(bus, dc, wr);
    let mut display = Builder::new(ST7789, di)
        .reset_pin(rst)
        .display_size(170, 320)
        .display_offset(35, 0)
        .invert_colors(ColorInversion::Inverted)
        .orientation(Orientation::new().rotate(Rotation::Deg90))
        .init(&mut Delay::new())
        .expect("display init");
    display.clear(Rgb565::BLACK).ok();
    bl.set_high();
    let _keep = PanelKeepAlive {
        _pwr: pwr,
        _rd: rd,
        _cs: cs,
        _bl: bl,
    };
    Text::with_baseline(
        "BOOT",
        Point::new(8, 80),
        MonoTextStyle::new(&FONT_8X13, Rgb565::WHITE),
        Baseline::Top,
    )
    .draw(&mut display)
    .ok();

    let mut cache = [BLANK; LINES];
    let mut detail = false;
    let mut selected: Option<[u8; 6]> = None;
    let mut was_empty = false;
    // Forces the first pass to clear BOOT and draw everything.
    let mut dirty = true;
    loop {
        let now = now_us();
        let rows: [Option<Track>; SNAPSHOT_ROWS] = TRACKS.lock(|t| {
            let mut t = t.borrow_mut();
            t.purge(now);
            t.snapshot(now)
        });
        let visible = TRACKS.lock(|t| t.borrow().visible_count(now));
        let shown = rows.iter().flatten().count();
        if !rows.iter().flatten().any(|t| Some(t.mac) == selected) {
            selected = rows[0].map(|t| t.mac);
        }

        // Switching between empty and non-empty repaints, since the empty-state text overlaps other lines.
        if dirty || was_empty != (shown == 0) {
            display.clear(Rgb565::BLACK).ok();
            cache = [BLANK; LINES];
            dirty = false;
        }
        was_empty = shown == 0;

        let ch = CURRENT_CHANNEL.load(Relaxed);
        let hop = HOP_MODE.load(Relaxed);
        let mut h = Line::new();
        let drop = (OBS_DROPPED.load(Relaxed) + TX_DROPPED.load(Relaxed)).min(9999);
        let _ = write!(
            h,
            "CH{ch:02} {}  TRK {visible:<2} RID {:06} DROP {drop}",
            if hop { "HOP" } else { "FIX" },
            RID_FRAMES.load(Relaxed) % 1_000_000
        );
        let header = Slot {
            x: 4,
            y: 0,
            dy: 2,
            h: 16,
            font: &FONT_8X13_BOLD,
            bg: Rgb565::BLUE,
        };
        draw(&mut display, &mut cache[0], header, h, Rgb565::WHITE).await;

        let sel = rows.iter().flatten().find(|t| Some(t.mac) == selected);
        match (detail, sel) {
            (true, Some(t)) => {
                for k in 0..15 {
                    let slot = Slot {
                        x: 0,
                        y: 18 + 10 * k as i32,
                        dy: 0,
                        h: 10,
                        font: &FONT_6X10,
                        bg: Rgb565::BLACK,
                    };
                    draw(
                        &mut display,
                        &mut cache[1 + k],
                        slot,
                        detail_line(k, t, now),
                        Rgb565::WHITE,
                    )
                    .await;
                }
            }
            (false, _) if shown > 0 => {
                for (i, row) in rows.iter().take(LIST_ROWS).enumerate() {
                    let top = 18 + 25 * i as i32;
                    let (mut l1, mut l2) = (Line::new(), Line::new());
                    let mut color = Rgb565::WHITE;
                    if let Some(t) = row {
                        let mut age = Line::new();
                        let _ = write!(age, "{}s", secs(now, t.last_seen_us).min(99));
                        let marker = if Some(t.mac) == selected { '>' } else { ' ' };
                        let _ = write!(
                            l1,
                            "{marker}{:<20.20}{:>19}",
                            label(t).as_str(),
                            age.as_str()
                        );
                        let _ = write!(l2, "{} ALT ", status_letter(t));
                        let loc = t.location;
                        opt(&mut l2, loc.and_then(|l| l.alt_geo_m()), 0);
                        let _ = l2.write_str(" SPD ");
                        opt(&mut l2, loc.and_then(|l| l.speed_h_mps()), 1);
                        let _ = match loc.and_then(|l| l.track_deg()) {
                            Some(trk) => write!(l2, " HDG {trk:03}"),
                            None => l2.write_str(" HDG ---"),
                        };
                        let _ = write!(l2, " {}dBm", t.rssi_avg_x16 / 16);
                        color = status_color(t);
                    }
                    let s1 = Slot {
                        x: 0,
                        y: top + 1,
                        dy: 0,
                        h: 13,
                        font: &FONT_8X13,
                        bg: Rgb565::BLACK,
                    };
                    draw(&mut display, &mut cache[1 + 2 * i], s1, l1, color).await;
                    let s2 = Slot {
                        x: 8,
                        y: top + 14,
                        dy: 0,
                        h: 10,
                        font: &FONT_6X10,
                        bg: Rgb565::BLACK,
                    };
                    draw(&mut display, &mut cache[2 + 2 * i], s2, l2, Rgb565::CYAN).await;
                }
            }
            _ => {
                let mut b = Line::new();
                let _ = if hop {
                    write!(b, "No Remote ID on HOP")
                } else {
                    write!(b, "No Remote ID on CH{ch:02}")
                };
                let slot = Slot {
                    x: 8,
                    y: 80,
                    dy: 0,
                    h: 13,
                    font: &FONT_8X13,
                    bg: Rgb565::BLACK,
                };
                draw(&mut display, &mut cache[LINES - 1], slot, b, Rgb565::WHITE).await;
            }
        }

        let evt = match select(UI_EVT.receive(), Timer::after_millis(500)).await {
            Either::First(e) => Some(e),
            Either::Second(()) => None,
        };
        match evt {
            Some(UiEvt::NextTrack) => {
                let macs = rows.iter().flatten().map(|t| t.mac);
                let pos = macs.clone().position(|m| Some(m) == selected);
                selected = pos
                    .and_then(|p| macs.clone().nth(p + 1))
                    .or(rows[0].map(|t| t.mac));
            }
            Some(UiEvt::ToggleView) => {
                detail = !detail;
                dirty = true;
            }
            Some(UiEvt::ClearTracks) => {
                TRACKS.lock(|t| t.borrow_mut().clear());
                dirty = true;
            }
            Some(UiEvt::Redraw) | None => {}
        }
    }
}
