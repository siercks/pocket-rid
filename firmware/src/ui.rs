use embassy_time::Timer;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::FONT_8X13;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::text::{Baseline, Text};
use esp_hal::delay::Delay;
use mipidsi::interface::{Generic8BitBus, ParallelInterface};
use mipidsi::models::ST7789;
use mipidsi::options::{ColorInversion, Orientation, Rotation};
use mipidsi::{Builder, TestImage};

use crate::board::{PanelKeepAlive, PanelPins};

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

    TestImage::<Rgb565>::new().draw(&mut display).ok();
    Timer::after_secs(3).await;
    display.clear(Rgb565::BLACK).ok();
    let style = MonoTextStyle::new(&FONT_8X13, Rgb565::WHITE);
    Text::with_baseline("BOOT", Point::new(8, 80), style, Baseline::Top)
        .draw(&mut display)
        .ok();

    loop {
        Timer::after_secs(3600).await;
    }
}
