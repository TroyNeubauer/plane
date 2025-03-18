//! This example shows how to use USB (Universal Serial Bus) in the RP2040 chip.
//!
//! This creates the possibility to send log::info/warn/error/debug! to USB serial port.

#![no_std]
#![no_main]

use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_rp::gpio::{Level, Output};
use embassy_rp::peripherals::{PIN_4, PWM_SLICE2, UART1, USB};
use embassy_rp::pwm::{self, ChannelAPin, ChannelBPin, Pwm, SetDutyCycle};
use embassy_rp::uart::{self, UartRx, UartTx};
use embassy_rp::usb::{self, Driver};
use embassy_rp::{Peripheral, bind_interrupts};
use embassy_time::Timer;
use log::warn;
use log::{error, info};

use plane_core::{ControlState, FcInput, MAGIC};

bind_interrupts!(struct Irqs {
    UART1_IRQ => uart::InterruptHandler<UART1>;
});

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

// async fn logger_task<'d, T: uart::Instance, M: uart::Mode>(uart: UartTx<'d, T, M>) {
//
// }

async fn main_inner(spawner: Spawner) -> Result<(), &'static str> {
    let p = embassy_rp::init(Default::default());
    let mut led = Output::new(p.PIN_25, Level::Low);

    Timer::after_secs(3).await;
    // Start tasks

    // spawner
    //     .spawn(pwm_set_dutycycle(p.PWM_SLICE2, p.PIN_4, led))
    //     .map_err(|_| "failed to spawn logger task")?;

    let mut left_aleron = RawPwm::new(p.PWM_SLICE2, p.PIN_4, 50, 64, 0.02, 0.10);
    let mut right_aleron = RawPwm::new(p.PWM_SLICE1, p.PIN_2, 50, 64, 0.02, 0.10);
    let mut elevator = unsafe {
         RawPwm::new(p.PWM_SLICE3.clone_unchecked(), p.PIN_6, 50, 64, 0.02, 0.10)
    };
    let mut prop = RawPwm::new_b(p.PWM_SLICE3, p.PIN_7, 50, 64, 0.02, 0.10);

    left_aleron.set_from_axis_control(0.0);
    right_aleron.set_from_axis_control(0.0);
    elevator.set_from_axis_control(0.0);

    let mut config = uart::Config::default();
    config.baudrate = 57600;

    // let uart_tx = UartTx::new(p.UART0, p.PIN_0, p.DMA_CH0, config);
    let mut uart_rx = UartRx::new(p.UART1, p.PIN_5, Irqs, p.DMA_CH1, config);

    // spawner
    //     .spawn(logger_task(uart_tx))
    //     .map_err(|_| "failed to spawn logger task")?;

    let mut state = ControlState::default();

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
                state.pitch = cmd.controls.pitch;
                state.yaw = cmd.controls.yaw;
                state.roll = cmd.controls.roll;
                state.throttle = cmd.controls.throttle;

                left_aleron.set_from_axis_control(-state.roll);
                right_aleron.set_from_axis_control(-state.roll);
                elevator.set_from_axis_control(-state.pitch);
                prop.set_from_axis_control(state.throttle);
            }
        }
    }
}

pub struct RawPwm<'a> {
    inner: Pwm<'a>,
    /// Number of ticks in the period
    period: u16,
    min_percent: f32,
    max_percent: f32,
}

impl<'a> RawPwm<'a> {
    pub fn new<T: embassy_rp::pwm::Slice>(
        slice: impl Peripheral<P = T> + 'a,
        pin: impl Peripheral<P = impl ChannelAPin<T>> + 'a,
        duty_cycle_hz: u32,
        divider: u8,
        min_percent: f32,
        max_percent: f32,
    ) -> Self {
        // If we aim for a specific frequency, here is how we can calculate the top value.
        // The top value sets the period of the PWM cycle, so a counter goes from 0 to top and then wraps around to 0.
        // Every such wraparound is one PWM cycle. So here is how we get 50KHz:
        let clock_freq_hz = embassy_rp::clocks::clk_sys_freq();
        let period = (clock_freq_hz / (duty_cycle_hz * divider as u32)) as u16 - 1;
        info!("divider: {divider}, period: {period}");

        let mut c = pwm::Config::default();
        c.top = period;
        c.divider = divider.into();

        Self {
            inner: Pwm::new_output_a(slice, pin, c.clone()),
            period,
            min_percent,
            max_percent,
        }
    }

    pub fn new_ab<T: embassy_rp::pwm::Slice>(
        slice: impl Peripheral<P = T> + 'a,
        pin_a: impl Peripheral<P = impl ChannelAPin<T>> + 'a,
        pin_b: impl Peripheral<P = impl ChannelBPin<T>> + 'a,
        duty_cycle_hz: u32,
        divider: u8,
        min_percent: f32,
        max_percent: f32,
    ) -> Self {
        // If we aim for a specific frequency, here is how we can calculate the top value.
        // The top value sets the period of the PWM cycle, so a counter goes from 0 to top and then wraps around to 0.
        // Every such wraparound is one PWM cycle. So here is how we get 50KHz:
        let clock_freq_hz = embassy_rp::clocks::clk_sys_freq();
        let period = (clock_freq_hz / (duty_cycle_hz * divider as u32)) as u16 - 1;
        info!("divider: {divider}, period: {period}");

        let mut c = pwm::Config::default();
        c.top = period;
        c.divider = divider.into();

        Self {
            inner: Pwm::new_output_ab(slice, pin_a, pin_b, c.clone()),
            period,
            min_percent,
            max_percent,
        }
    }

    pub fn set_pwm_percent(&mut self, percent: f32) {
        let percent = percent.clamp(self.min_percent, self.max_percent);
        let ticks = (self.period as f32 * percent) as u16;
        self.inner.set_duty_cycle(ticks);
    }

    // Axis is [-1..1]
    pub fn set_from_axis_control(&mut self, axis: f32) {
        let f = (axis + 1.0) / 2.0;
        let percent = self.min_percent + f * (self.max_percent - self.min_percent);

        let ticks = (self.period as f32 * percent) as u16;
        self.inner.set_duty_cycle(ticks);
        //
    }
}

pub struct PwmControlSurface<'a> {
    // 19,530 * 2
    inner: RawPwm<'a>,
}

/// Demonstrate PWM by setting duty cycle
///
/// Using GP4 in Slice2, make sure to use an appropriate resistor.
#[embassy_executor::task]
async fn pwm_set_dutycycle(slice2: PWM_SLICE2, pin4: PIN_4, mut led: Output<'static>) {
    let mut pwm = RawPwm::new(slice2, pin4, 50, 64, 0.02, 0.10);

    let min = -1.0;
    let max = 1.0;
    let rate = 1.0;

    let mut value = min;
    loop {
        if value > max {
            value = min;
        }
        pwm.set_from_axis_control(value);
        info!("Value: {value}");
        led.toggle();

        value += rate * (1.0 / 50.0);
        Timer::after_millis(50).await;
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
