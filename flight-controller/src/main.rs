//! This example shows how to use USB (Universal Serial Bus) in the RP2040 chip.
//!
//! This creates the possibility to send log::info/warn/error/debug! to USB serial port.

#![no_std]
#![no_main]

use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::gpio::{Level, Output};
use embassy_rp::peripherals::{PIN_4, PWM_SLICE2, UART1, USB};
use embassy_rp::pwm::{self, Pwm, SetDutyCycle};
use embassy_rp::uart::{self, UartRx};
use embassy_rp::usb::{self, Driver};
use embassy_time::Timer;
use log::warn;
use log::{error, info};

use plane_core::{FcInput, MAGIC};

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => usb::InterruptHandler<USB>;
    UART1_IRQ => uart::InterruptHandler<UART1>;
});

#[embassy_executor::task]
async fn logger_task(driver: Driver<'static, USB>) {
    embassy_usb_logger::run!(1024, log::LevelFilter::Info, driver);
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    match main_inner(spawner).await {
        Ok(()) => info!("Main returned Ok(())"),
        Err(e) => loop {
            error!("Main returned error: {e}");
            Timer::after_secs(1).await;
        },
    }
}

async fn main_inner(spawner: Spawner) -> Result<(), &'static str> {
    let p = embassy_rp::init(Default::default());
    let mut led = Output::new(p.PIN_25, Level::Low);

    let driver = Driver::new(p.USB, Irqs);
    spawner
        .spawn(logger_task(driver))
        .map_err(|_| "failed to spawn logger task")?;

    Timer::after_secs(3).await;
    // Start tasks

    spawner
        .spawn(pwm_set_dutycycle(p.PWM_SLICE2, p.PIN_4, led))
        .map_err(|_| "failed to spawn logger task")?;

    let mut config = uart::Config::default();
    config.baudrate = 57600;

    // let mut uart_tx = UartTx::new(p.UART0, p.PIN_0, p.DMA_CH0, config);
    let mut uart_rx = UartRx::new(p.UART1, p.PIN_5, Irqs, p.DMA_CH1, config);

    log::info!("Reading...");
    loop {
        let mut buf = [0; 20];
        if let Err(e) = uart_rx.read(&mut buf).await {
            warn!("Failed to read data: {e:?}");
        }
        if buf[0] != MAGIC {
            warn!("Off of magic!");
        } else {
            let payload = &buf[1..];
            if let Ok(cmd) = postcard::from_bytes::<FcInput>(payload) {
                info!("{cmd:?}");
            }
        }
    }
}

/// Demonstrate PWM by setting duty cycle
///
/// Using GP4 in Slice2, make sure to use an appropriate resistor.
#[embassy_executor::task]
async fn pwm_set_dutycycle(slice2: PWM_SLICE2, pin4: PIN_4, mut led: Output<'static>) {
    // If we aim for a specific frequency, here is how we can calculate the top value.
    // The top value sets the period of the PWM cycle, so a counter goes from 0 to top and then wraps around to 0.
    // Every such wraparound is one PWM cycle. So here is how we get 50KHz:
    let desired_freq_hz = 50;
    let clock_freq_hz = embassy_rp::clocks::clk_sys_freq();
    let divider = 128u8;
    let period = (clock_freq_hz / (desired_freq_hz * divider as u32)) as u16 - 1;
    info!("clock_freq_hz: {clock_freq_hz}, period: {period}");

    let mut c = pwm::Config::default();
    c.top = period;
    c.divider = divider.into();

    let mut pwm = Pwm::new_output_a(slice2, pin4, c.clone());

    loop {
        pwm.set_duty_cycle(10 * c.top / 100).unwrap();
        Timer::after_secs(1).await;
        led.toggle();

        pwm.set_duty_cycle(15 * c.top / 100).unwrap();
        Timer::after_secs(1).await;
        led.toggle();

        pwm.set_duty_cycle(20 * c.top / 100).unwrap();
        Timer::after_secs(1).await;
        led.toggle();
    }
}

#[embassy_executor::task]
async fn blink_led(mut led: Output<'static>) {
    loop {
        led.set_high();
        Timer::after_millis(500).await;

        led.set_low();
        Timer::after_millis(500).await;
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    loop {
        log::error!("{info:?}");
        let _ = embassy_futures::poll_once(embassy_futures::yield_now());
    }
}
