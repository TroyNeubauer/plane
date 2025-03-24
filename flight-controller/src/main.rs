#![no_std]
#![no_main]

mod logger;
mod pwm;
use pwm::*;
mod panic_handler;
mod spawner;
pub use spawner::*;

use bitflare::BitflareReader;
use defmt::{error, info, warn};
use embassy_rp::bind_interrupts;
use embassy_rp::flash::{self, Flash};
use embassy_rp::gpio::{Level, Output};
use embassy_rp::peripherals::UART1;
use embassy_rp::uart::{self, Uart, UartTx};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex as AsyncMutex;
use embassy_time::{Duration, Ticker, Timer};
use plane_core::{ControlState, FcInput, MAX_FC_INPUT_PAYLOAD, TrimConfig};

bind_interrupts!(struct Irqs {
    UART1_IRQ => uart::InterruptHandler<UART1>;
});

// _embassy_trace_executor_idle    537141224
// _embassy_trace_executor_idle    537141224
// _embassy_trace_task_ready_begin 537141224 536871384
// _embassy_trace_task_exec_begin  537141224 536871384
// _embassy_trace_task_exec_end    537141224 536871384
// _embassy_trace_executor_idle    537141224

// #[unsafe(no_mangle)]
// fn _embassy_trace_task_new(_executor_id: u32, _task_id: u32) {
//     defmt::info!("_embassy_trace_task_new {} {}", _executor_id, _task_id);
// }
// #[unsafe(no_mangle)]
// fn _embassy_trace_task_exec_begin(_executor_id: u32, _task_id: u32) {
//     defmt::info!(
//         "_embassy_trace_task_exec_begin {} {}",
//         _executor_id,
//         _task_id
//     );
// }
// #[unsafe(no_mangle)]
// fn _embassy_trace_task_exec_end(_executor_id: u32, _task_id: u32) {
//     defmt::info!("_embassy_trace_task_exec_end {} {}", _executor_id, _task_id);
// }
// #[unsafe(no_mangle)]
// fn _embassy_trace_task_ready_begin(_executor_id: u32, _task_id: u32) {
//     defmt::info!(
//         "_embassy_trace_task_ready_begin {} {}",
//         _executor_id,
//         _task_id
//     );
// }
// #[unsafe(no_mangle)]
// fn _embassy_trace_executor_idle(_executor_id: u32) {
//     defmt::info!("_embassy_trace_executor_idle {}", _executor_id);
// }

#[embassy_executor::main]
async fn main(spawner: embassy_executor::Spawner) {
    match main_inner(spawner).await {
        Ok(()) => info!("Main returned Ok(())"),
        Err(e) => loop {
            error!("Main returned error: {}", e);
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

pub static RADIO_SERIAL: AsyncMutex<CriticalSectionRawMutex, Option<UartTx<UART1, uart::Async>>> =
    AsyncMutex::new(None);

pub async fn with_radio_serial<F, R>(f: F) -> Option<R>
where
    F: AsyncFn(&mut UartTx<UART1, uart::Async>) -> R,
{
    let mut guard = RADIO_SERIAL.lock().await;
    if let Some(serial) = guard.as_mut() {
        let future = f(serial);
        Some(future.await)
    } else {
        None
    }
}

// async fn logger_task<'d, T: uart::Instance, M: uart::Mode>(uart: UartTx<'d, T, M>) {
//
// }

async fn arm_esc<'a>(prop: &mut RawPwm<'a>) {
    let _ = prop.set_from_axis_control(1.0);
    info!("set upper limit");
    Timer::after_secs(3).await;

    let _ = prop.set_from_axis_control(-1.0);
    info!("set lower limit");

    Timer::after_secs(3).await;
    let _ = prop.set_from_axis_control(0.0);
    info!("neutral");
}

const MAX_PERC_DFL: f32 = 0.02;
const MIN_PERC_DFL: f32 = 0.10;
async fn main_inner(spawner: embassy_executor::Spawner) -> Result<(), &'static str> {
    logger::set_spawner(spawner);

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

    let (mut left_aleron, mut right_aleron) = RawPwm::new_ab(
        p.PWM_SLICE4,
        p.PIN_8,
        p.PIN_9,
        50,
        64,
        MIN_PERC_DFL,
        MAX_PERC_DFL,
    );
    
    warn!("Arming esc");
    arm_esc(&mut prop).await;
    info!("ESC armed");

    let _ = left_aleron.set_from_axis_control(0.0);
    let _ = right_aleron.set_from_axis_control(0.0);
    let _ = elevator.set_from_axis_control(0.0);

    let mut config = uart::Config::default();
    config.baudrate = 57600;

    // NOTE: if changing serial pins, make sure to change the panic handler as well
    let uart = Uart::new(
        p.UART1, p.PIN_4, p.PIN_5, Irqs, p.DMA_CH0, p.DMA_CH1, config,
    );
    let (uart_tx, mut uart_rx) = uart.split();

    {
        let mut radio_serial = RADIO_SERIAL.lock().await;
        *radio_serial = Some(uart_tx);
    }

    Timer::after_secs(2).await;


    // spawner.spawn(blink_led(led)).expect("failed to spawn task");

    // spawner.spawn(log_1_hz()).expect("failed to spawn task");
    // spawner.spawn(log_2_hz()).expect("failed to spawn task");
    // spawner.spawn(log_4_hz()).expect("failed to spawn task");

    let mut armed = false;
    let mut trim_config: TrimConfig = TrimConfig::default();
    let mut flight_controls = ControlState::default();
    let mut reader = BitflareReader::<MAX_FC_INPUT_PAYLOAD>::new();

    info!("Starting run loop...");
    loop {
        let mut buf = [0; MAX_FC_INPUT_PAYLOAD];

        if let Err(e) = uart_rx.read(&mut buf).await {
            warn!("Failed to read from uart: {}", e);
        }
        led.toggle();

        reader.decode(&buf, |payload| {
            if let Ok(cmd) = postcard::from_bytes::<FcInput>(payload) {
                match cmd {
                    FcInput::Trim(new_trim) => {
                        trim_config = new_trim;

                        // TODO: Save trim config to flash

                        elevator.max_percent = MAX_PERC_DFL
                            + (trim_config.elevator_range * (MAX_PERC_DFL - MIN_PERC_DFL));
                        left_aleron.max_percent =
                            MAX_PERC_DFL + (trim_config.roll_range * (MAX_PERC_DFL - MIN_PERC_DFL));
                        right_aleron.max_percent =
                            MAX_PERC_DFL + (trim_config.roll_range * (MAX_PERC_DFL - MIN_PERC_DFL));
                    }
                    FcInput::Controls(new_controls) => {
                        flight_controls = new_controls;
                    }
                    FcInput::Arm => armed = true,
                    FcInput::Disarm => armed = false,
                    FcInput::ResetToUsbBoot => {
                        embassy_rp::rom_data::reset_to_usb_boot(0, 0);
                    }
                }

                let pitch = flight_controls.pitch.clamp(-1.0, 1.0);
                let yaw = flight_controls.yaw.clamp(-1.0, 1.0);
                let roll = flight_controls.roll.clamp(-1.0, 1.0);
                let throttle = if armed {
                    flight_controls.throttle.clamp(0.0, 1.0)
                } else {
                    0.0
                };

                let _ = left_aleron.set_from_axis_control(roll + trim_config.left_aileron);
                let _ = right_aleron.set_from_axis_control(roll + trim_config.right_aileron);
                let _ = elevator.set_from_axis_control(pitch - trim_config.elevator);
                info!("-throttle {} {}", -throttle, armed);
                let _ = prop.set_from_axis_control(-throttle);
            }
        });
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

#[embassy_executor::task]
async fn log_1_hz() {
    let mut ticker = Ticker::every(Duration::from_millis(1000));
    loop {
        defmt::info!("1Hz log");
        ticker.next().await;
    }
}

#[embassy_executor::task]
async fn log_2_hz() {
    let mut ticker = Ticker::every(Duration::from_millis(1000));
    loop {
        defmt::info!("2Hz log");
        ticker.next().await;
    }
}

#[embassy_executor::task]
async fn log_4_hz() {
    let mut ticker = Ticker::every(Duration::from_millis(1000));

    loop {
        defmt::info!("4Hz log");
        ticker.next().await;
    }
}
