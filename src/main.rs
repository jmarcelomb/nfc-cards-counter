use esp_idf_sys as _; // If using the `binstart` feature of `esp-idf-sys`, always keep this module imported
use log::*;

use pn532::{i2c::I2CInterface, requests::SAMMode, Pn532, Request};

use esp_idf_hal::delay::FreeRtos;
use esp_idf_hal::gpio::*;
use esp_idf_hal::i2c::*;
use esp_idf_hal::prelude::*;
use esp_idf_hal::timer::*;

use embedded_hal::timer::CountDown;
use void::Void;

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

static mut ALARM_TRIGGERED: AtomicBool = AtomicBool::new(false);

fn alarm_callback() {
    unsafe {
        ALARM_TRIGGERED.store(true, Ordering::Relaxed);
    }
}

pub struct TimerWrapper<'a> {
    esp_timer: TimerDriver<'a>,
}

impl<'a> TimerWrapper<'a> {
    pub fn new(timer: TimerDriver<'a>) -> Self {
        TimerWrapper { esp_timer: timer }
    }
}

impl CountDown for TimerWrapper<'_> {
    type Time = Duration;

    fn start<T>(&mut self, count: T)
    where
        T: Into<Self::Time>,
    {
        let count_value: Self::Time = count.into();
        self.esp_timer.enable_interrupt();
        self.esp_timer.set_counter(0);
        self.esp_timer.set_alarm(count_value.as_micros() as u64);
        info!("Setting timer with counter {}", count_value.as_micros());
        // self.esp_timer.set_auto_reload(true);
        unsafe {
            ALARM_TRIGGERED.store(false, Ordering::Release);
            self.esp_timer.subscribe(alarm_callback);
        };
        self.esp_timer.enable_alarm(true);
        self.esp_timer.enable(true);
    }

    fn wait(&mut self) -> nb::Result<(), Void> {
        unsafe { while ALARM_TRIGGERED.load(Ordering::Relaxed) == false {} }
        self.esp_timer.enable(false).unwrap();
        Ok(())
    }
}

fn main() -> anyhow::Result<()> {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_sys::link_patches();
    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    info!("Hello, world!");

    let peripherals = Peripherals::take().unwrap();
    let i2c = peripherals.i2c0;
    let sda = peripherals.pins.gpio5;
    let scl: Gpio6 = peripherals.pins.gpio6;

    let mut led = PinDriver::output(peripherals.pins.gpio3)?;

    let config = I2cConfig::new().baudrate(100.kHz().into());
    let mut i2c = I2cDriver::new(i2c, sda, scl, &config)?;

    let config = TimerConfig::new();
    let timer: TimerDriver = TimerDriver::new(peripherals.timer00, &config)?;
    let mut timer_wrapper = TimerWrapper::new(timer);

    let interface = I2CInterface { i2c };
    let mut pn532: Pn532<_, _, 32> = Pn532::new(interface, timer_wrapper);
    if let Err(e) = pn532.process(
        &Request::sam_configuration(SAMMode::Normal, false),
        0,
        Duration::from_millis(50),
    ) {
        println!("Could not initialize PN532: {e:?}")
    }
    if let Ok(uid) = pn532.process(
        &Request::INLIST_ONE_ISO_A_TARGET,
        7,
        Duration::from_millis(1000),
    ) {}

    info!("Scanning..");
    loop {
        // led.set_high()?;
        let result = pn532.process(&Request::ntag_read(10), 17, Duration::from_millis(50));
        match result {
            Ok(page) => {
                if page[0] == 0x00 {
                    println!("page 10: {:?}", &page[1..5]);
                }
            }
            Err(e) => {
                error!("I2C Error {:?}", e);
            }
        }
        // led.set_low()?;
        FreeRtos::delay_ms(500);
    }
}
