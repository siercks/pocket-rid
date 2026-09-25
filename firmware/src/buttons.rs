use core::sync::atomic::Ordering::Relaxed;

use embassy_time::{Duration, Ticker};
use esp_hal::gpio::Input;

use crate::radio::{CURRENT_CHANNEL, HOP_MODE, RADIO_CMD, RadioCmd};
use crate::ui::{UI_EVT, UiEvt};

const POLL_MS: u32 = 10;
const LONG_MS: u32 = 800;

enum Press {
    Short,
    Long,
}

#[derive(Default)]
struct Button {
    pressed: bool,
    candidate: bool,
    same: u8,
    held_ms: u32,
    long_sent: bool,
}

impl Button {
    fn poll(&mut self, raw_pressed: bool) -> Option<Press> {
        if raw_pressed == self.candidate {
            self.same = self.same.saturating_add(1);
        } else {
            self.candidate = raw_pressed;
            self.same = 1;
        }
        // Three identical 10 ms samples debounce the edge.
        if self.same >= 3 && self.candidate != self.pressed {
            self.pressed = self.candidate;
            if self.pressed {
                self.held_ms = 0;
                self.long_sent = false;
            } else if !self.long_sent {
                return Some(Press::Short);
            }
        }
        if self.pressed && !self.long_sent {
            self.held_ms += POLL_MS;
            if self.held_ms >= LONG_MS {
                self.long_sent = true;
                return Some(Press::Long);
            }
        }
        None
    }
}

/// Preset cycle: Fixed 6 → Fixed 1 → Fixed 11 → Hop → Fixed 6.
fn next_preset() -> RadioCmd {
    if HOP_MODE.load(Relaxed) {
        return RadioCmd::SetFixed(6);
    }
    match CURRENT_CHANNEL.load(Relaxed) {
        6 => RadioCmd::SetFixed(1),
        1 => RadioCmd::SetFixed(11),
        11 => RadioCmd::SetHop,
        _ => RadioCmd::SetFixed(6),
    }
}

#[embassy_executor::task]
pub async fn button_task(a: Input<'static>, b: Input<'static>) {
    let mut ticker = Ticker::every(Duration::from_millis(u64::from(POLL_MS)));
    let (mut ba, mut bb) = (Button::default(), Button::default());
    loop {
        ticker.next().await;
        let evt = match ba.poll(a.is_low()) {
            Some(Press::Short) => Some(UiEvt::NextTrack),
            Some(Press::Long) => Some(UiEvt::ToggleView),
            None => None,
        };
        let evt = evt.or(match bb.poll(b.is_low()) {
            Some(Press::Short) => {
                let _ = RADIO_CMD.try_send(next_preset());
                Some(UiEvt::Redraw)
            }
            Some(Press::Long) => Some(UiEvt::ClearTracks),
            None => None,
        });
        if let Some(e) = evt {
            let _ = UI_EVT.try_send(e);
        }
    }
}
