#![no_std]
#![no_main]

mod board;
mod radio;
mod status;
mod ui;

use embassy_executor::Spawner;
use embassy_time::Timer;
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::ram;
use esp_hal::timer::timg::TimerGroup;
use esp_radio::wifi::{ControllerConfig, SecondaryChannel};

use board::PanelPins;

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_rtos::main]
async fn main(spawner: Spawner) {
    let p = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));
    esp_alloc::heap_allocator!(#[ram(reclaimed)] size: 64 * 1024);
    esp_alloc::heap_allocator!(size: 36 * 1024);

    let timg0 = TimerGroup::new(p.TIMG0);
    let sw_int = SoftwareInterruptControl::new(p.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    let pwr = Output::new(p.GPIO15, Level::High, OutputConfig::default());
    // The panel needs a few milliseconds after power-on before its pins are driven.
    Timer::after_millis(10).await;
    let pins = PanelPins {
        pwr,
        rd: Output::new(p.GPIO9, Level::High, OutputConfig::default()),
        cs: Output::new(p.GPIO6, Level::Low, OutputConfig::default()),
        wr: Output::new(p.GPIO8, Level::High, OutputConfig::default()),
        dc: Output::new(p.GPIO7, Level::Low, OutputConfig::default()),
        rst: Output::new(p.GPIO5, Level::High, OutputConfig::default()),
        bl: Output::new(p.GPIO38, Level::Low, OutputConfig::default()),
        d: [
            Output::new(p.GPIO39, Level::Low, OutputConfig::default()),
            Output::new(p.GPIO40, Level::Low, OutputConfig::default()),
            Output::new(p.GPIO41, Level::Low, OutputConfig::default()),
            Output::new(p.GPIO42, Level::Low, OutputConfig::default()),
            Output::new(p.GPIO45, Level::Low, OutputConfig::default()),
            Output::new(p.GPIO46, Level::Low, OutputConfig::default()),
            Output::new(p.GPIO47, Level::Low, OutputConfig::default()),
            Output::new(p.GPIO48, Level::Low, OutputConfig::default()),
        ],
    };
    spawner.spawn(ui::ui_task(pins).expect("spawn ui_task"));

    let (mut controller, interfaces) =
        esp_radio::wifi::new(p.WIFI, ControllerConfig::default()).expect("wifi::new");
    let mut sniffer = interfaces.sniffer;
    sniffer.set_receive_cb(radio::sniffer_cb);
    sniffer.set_promiscuous_mode(true).expect("promiscuous");
    controller
        .set_channel(6, SecondaryChannel::None)
        .expect("set_channel");
    spawner.spawn(radio::radio_task(controller, sniffer).expect("spawn radio_task"));

    loop {
        #[cfg(feature = "console-text")]
        {
            use core::sync::atomic::Ordering::Relaxed;
            use status::*;
            esp_println::println!(
                "MGMT {} BEACON {} RID {} DROP {}",
                MGMT_FRAMES.load(Relaxed),
                BEACONS.load(Relaxed),
                RID_FRAMES.load(Relaxed),
                OBS_DROPPED.load(Relaxed)
            );
        }
        Timer::after_millis(1000).await;
    }
}
