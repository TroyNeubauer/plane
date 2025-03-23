//! This example shows how to use USB (Universal Serial Bus) in the RP2040 chip.
//!
//! This creates the possibility to send log::info/warn/error/debug! to USB serial port.

#![no_std]
#![no_main]

mod logger;

use core::cell::UnsafeCell;

use bitflare::BitflareWriter;
use embassy_executor::Spawner;
use embassy_rp::flash::{self, Flash};
use embassy_rp::gpio::{Level, Output};
use embassy_rp::pac::UART1;
use embassy_rp::peripherals::{DMA_CH0, PIN_4, PIN_25, UART1};
use embassy_rp::pwm::{self, ChannelAPin, ChannelBPin, Pwm, PwmError, PwmOutput, SetDutyCycle};
use embassy_rp::uart::{self, Uart, UartRx, UartTx};
use embassy_rp::{Peripheral, bind_interrupts};
use embassy_sync::blocking_mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_time::Duration;
use embassy_time::Timer;
use embedded_hal_1::digital::OutputPin;
use log::{error, info};
use plane_core::{ControlState, FcInput, FcOutput, MAX_FC_OUTPUT_PACKET, TrimConfig};

bind_interrupts!(struct Irqs {
    UART1_IRQ => uart::InterruptHandler<UART1>;
});

fn _embassy_trace_task_new(_executor_id: u32, _task_id: u32) {}
fn _embassy_trace_task_exec_begin(_executor_id: u32, _task_id: u32) {}
fn _embassy_trace_task_exec_end(_excutor_id: u32, _task_id: u32) {}
fn _embassy_trace_task_ready_begin(_executor_id: u32, _task_id: u32) {}
fn _embassy_trace_executor_idle(_executor_id: u32) {}

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

const FLASH_SIZE: usize = 2 * 1024 * 1024;
// const PERSISTED_DATA_OFFSET: u32 = 0x100000;
const MAX_PERSISTED_BYTES: usize = 256;

pub struct PersistedData {
    trim_config: TrimConfig,
}

async fn load_trim_or_default(
    flash: &mut Flash<'_, embassy_rp::peripherals::FLASH, flash::Blocking, FLASH_SIZE>,
) -> TrimConfig {
    let mut buf = [0u8; MAX_PERSISTED_BYTES];

    flash.blocking_read(0, &mut buf).unwrap();
    todo!();

    /*
    let mut trim_config: PersistedData = if buf[0] == 0xba {
        postcard::from_bytes::<TrimConfig>(&buf[1..]).expect("has trim config")
    } else {
        TrimConfig::default()
    };
    */
}

// async fn logger_task<'d, T: uart::Instance, M: uart::Mode>(uart: UartTx<'d, T, M>) {
//
// }

async fn init_esc<'a>(prop: &mut RawPwm<'a>) {
    let _ = prop.set_from_axis_control(1.0);
    Timer::after_secs(3).await;
    let _ = prop.set_from_axis_control(-1.0);
    Timer::after_secs(3).await;
    let _ = prop.set_from_axis_control(0.0);
}

const MAX_PERC_DFL: f32 = 0.02;
const MIN_PERC_DFL: f32 = 0.10;
async fn main_inner(_spawner: Spawner) -> Result<(), &'static str> {
    let p = embassy_rp::init(Default::default());
    // let mut memory = Flash::<_, flash::Blocking, FLASH_SIZE>::new_blocking(p.FLASH);

    // pi pico visible LED
    let mut led = Output::new(p.PIN_25, Level::Low);

    let (mut elevator, mut prop) = RawPwm::new_ab(
        p.PWM_SLICE3,
        p.PIN_6,
        p.PIN_7,
        50,
        64,
        MIN_PERC_DFL,
        MAX_PERC_DFL,
    );

    // init_esc(&mut prop).await;

    let (mut left_aleron, mut right_aleron) = RawPwm::new_ab(
        p.PWM_SLICE4,
        p.PIN_8,
        p.PIN_9,
        50,
        64,
        MIN_PERC_DFL,
        MAX_PERC_DFL,
    );

    /*prop.set_from_axis_control(0.5);
    Timer::after_secs(10).await;
    prop.set_from_axis_control(0.0);*/

    let _ = left_aleron.set_from_axis_control(0.0);
    let _ = right_aleron.set_from_axis_control(0.0);
    let _ = elevator.set_from_axis_control(0.0);

    let mut config = uart::Config::default();
    config.baudrate = 57600;

    // NOTE: if changing serial pins, make sure to change the panic handler as well
    let uart = Uart::new(
        p.UART1, p.PIN_4, p.PIN_5, Irqs, p.DMA_CH0, p.DMA_CH1, config,
    );
    let (mut uart_tx, mut uart_rx) = uart.split();

    // spawner
    //     .spawn(logger_task(uart_tx))
    //     .map_err(|_| "failed to spawn logger task")?;

    let mut buf = [0u8; MAX_FC_OUTPUT_PACKET];

    let mut i = 0;
    loop {
        led.toggle();

        defmt::trace!("trace");
        // defmt::debug!("debug");
        // defmt::info!("info");
        // defmt::warn!("warn");
        // defmt::error!("error");

        let mut writer = BitflareWriter::new(&mut buf);
        writer
            .write_payload(|dst| {
                let msg = FcOutput::StringLog("Hello world!".into());
                let payload = postcard::to_slice(&msg, dst).map_err(|_| ())?;
                Ok(payload.len())
            })
            .unwrap();

        let bytes = writer.finish();
        uart_tx.write(bytes).await.unwrap();

        i += 1;
        if i == 5 {
            let name = "Bob Dingus";
            panic!("DINGUS: {i}, {name}");
        }

        Timer::after_secs(1).await;
    }

    let mut armed = false;
    let mut trim_config: TrimConfig = TrimConfig::default();
    let mut flight_controls = ControlState::default();

    log::info!("Reading...");
    loop {
        let mut buf = [0; 40];
        if let Err(e) = uart_rx.read(&mut buf).await {
            uart_tx.write(b"failed to read data!").await;
        }

        let payload = &buf[1..];
        if let Ok(cmd) = postcard::from_bytes::<FcInput>(payload) {
            match cmd {
                FcInput::Trim(new_trim) => {
                    trim_config = new_trim;

                    // TODO: Save trim config to flash

                    elevator.max_percent =
                        MAX_PERC_DFL + (trim_config.elevator_range * (MAX_PERC_DFL - MIN_PERC_DFL));
                    left_aleron.max_percent =
                        MAX_PERC_DFL + (new_trim.roll_range * (MAX_PERC_DFL - MIN_PERC_DFL));
                    right_aleron.max_percent =
                        MAX_PERC_DFL + (new_trim.roll_range * (MAX_PERC_DFL - MIN_PERC_DFL));
                }
                FcInput::Controls(new_controls) => {
                    flight_controls = new_controls;
                }
                FcInput::Arm => armed = true,
                FcInput::Disarm => armed = false,
            }

            let pitch = flight_controls.pitch;
            let yaw = flight_controls.yaw;
            let roll = flight_controls.roll;
            let throttle = if armed { flight_controls.throttle } else { 0.0 };

            let _ = left_aleron.set_from_axis_control(-roll + trim_config.left_aileron);
            let _ = right_aleron.set_from_axis_control(-roll + trim_config.right_aileron);
            let _ = elevator.set_from_axis_control(-pitch - trim_config.elevator);
            let _ = prop.set_from_axis_control(throttle);
        }
    }
}

pub struct RawPwm<'a> {
    inner: PwmOutput<'a>,
    /// Number of ticks in the period
    period: u16,
    pub min_percent: f32,
    pub max_percent: f32,
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
            inner: Pwm::new_output_a(slice, pin, c.clone())
                .split()
                .0
                .expect("When just making channel a it should have it"),
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
    ) -> (Self, Self) {
        // If we aim for a specific frequency, here is how we can calculate the top value.
        // The top value sets the period of the PWM cycle, so a counter goes from 0 to top and then wraps around to 0.
        // Every such wraparound is one PWM cycle. So here is how we get 50KHz:
        let clock_freq_hz = embassy_rp::clocks::clk_sys_freq();
        let period = (clock_freq_hz / (duty_cycle_hz * divider as u32)) as u16 - 1;
        info!("divider: {divider}, period: {period}");

        let mut c = pwm::Config::default();
        c.top = period;
        c.divider = divider.into();

        let portions = Pwm::new_output_ab(slice, pin_a, pin_b, c.clone()).split();
        (
            Self {
                inner: portions.0.expect("PWM Channel A"),
                period,
                min_percent,
                max_percent,
            },
            Self {
                inner: portions.1.expect("PWM Channel B"),
                period,
                min_percent,
                max_percent,
            },
        )
    }

    pub fn set_pwm_percent(&mut self, percent: f32) -> Result<(), PwmError> {
        let percent = percent.clamp(self.min_percent, self.max_percent);
        let ticks = (self.period as f32 * percent) as u16;
        self.inner.set_duty_cycle(ticks)
    }

    // Axis is [-1..1]
    pub fn set_from_axis_control(&mut self, axis: f32) -> Result<(), PwmError> {
        let f = (axis + 1.0) / 2.0;
        let percent = self.min_percent + f * (self.max_percent - self.min_percent);

        let ticks = (self.period as f32 * percent) as u16;
        self.inner.set_duty_cycle(ticks)
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
    critical_section::with(|_cs| {
        use core::fmt::Write;

        let mut buf = [0u8; MAX_FC_OUTPUT_PACKET];

        let mut config = uart::Config::default();
        config.baudrate = 57600;

        // Safety: not safe, were panicking hope for the best.
        // At least were in a critical section, but logs will be likely corrupted
        let uart = unsafe { UART1::steal() };
        let gpio = unsafe { PIN_4::steal() };
        let dma = unsafe { DMA_CH0::steal() };

        let mut uart = UartTx::<UART1, uart::Blocking>::new(uart, gpio, dma, config);

        let mut writer = BitflareWriter::new(&mut buf);
        writer
            .write_payload(|dst| {
                let (file, line, col) = match info.location() {
                    Some(loc) => {
                        // If name is too big, grab last part since that is most important
                        let start_idx = loc.file().len().saturating_sub(24);
                        let file = &loc.file()[start_idx..];

                        (file.into(), loc.line() as u16, loc.column() as u16)
                    }
                    None => (Default::default(), 0, 0),
                };
                let mut message = heapless::String::new();
                write!(&mut message, "{}", info.message());

                let payload = FcOutput::Panic {
                    file,
                    line,
                    col,
                    message,
                };
                let payload = postcard::to_slice(&payload, dst).map_err(|_| ())?;
                Ok(payload.len())
            })
            .unwrap();

        let bytes = writer.finish();

        let _ = uart.blocking_write(bytes);
        let _ = uart.blocking_write(bytes);
        let _ = uart.blocking_write(bytes);

        let led_pin = unsafe { PIN_25::steal() };
        let mut led = Output::new(led_pin, Level::Low);

        // Rapid help blick
        for _ in 0..200 {
            for action in small_morse::encode("SOS ") {
                if action.state == small_morse::State::On {
                    led.set_high();
                } else {
                    led.set_low();
                }

                let timeout = action.duration as u32 * Duration::from_millis(100);
                embassy_time::block_for(timeout);
            }

            led.set_low();
            embassy_time::block_for(Duration::from_secs(1));
        }
        cortex_m::asm::udf();
    })
}

#[defmt::panic_handler]
fn defmt_panic() -> ! {
    // reset, defmt already called our regular panic
    cortex_m::asm::udf();
}
