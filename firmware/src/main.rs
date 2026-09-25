#![no_std]
#![no_main]

mod board;
mod obs;
mod radio;
mod status;
mod tracks;
mod ui;
mod wire;

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
    log::set_logger(&wire::FrameLogger).expect("set_logger");
    log::set_max_level(log::LevelFilter::Info);

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
    let device_mac = interfaces.station.mac_address();
    let mut sniffer = interfaces.sniffer;
    sniffer.set_receive_cb(radio::sniffer_cb);
    sniffer.set_promiscuous_mode(true).expect("promiscuous");
    controller
        .set_channel(6, SecondaryChannel::None)
        .expect("set_channel");
    log::info!("radio up ch=6");

    #[cfg(not(feature = "console-text"))]
    spawner.spawn(wire::tx_task(p.USB_DEVICE).expect("spawn tx_task"));
    #[cfg(feature = "console-text")]
    spawner.spawn(wire::tx_task().expect("spawn tx_task"));
    spawner.spawn(obs::obs_task().expect("spawn obs_task"));
    spawner.spawn(status::status_task(device_mac).expect("spawn status_task"));
    spawner.spawn(radio::radio_task(controller, sniffer).expect("spawn radio_task"));
    status::send_hello(device_mac);
}
