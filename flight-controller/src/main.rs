#![no_std]
#![no_main]

mod logger;
mod pwm;
use pwm::*;
#[macro_use]
mod async_task;
mod panic_handler;

pub use async_task::*;

use bitflare::BitflareReader;
use cortex_m_rt::entry;
use defmt::{error, info, unwrap, warn};
use embassy_executor::Executor;
use embassy_rp::bind_interrupts;
use embassy_rp::flash::{self, Flash};
use embassy_rp::gpio::{Level, Output};
use embassy_rp::peripherals::UART1;
use embassy_rp::uart::{self, Uart, UartTx};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex as AsyncMutex;
use embassy_time::{Duration, Ticker, Timer};
use plane_core::{ControlState, FcInput, MAX_FC_INPUT_PAYLOAD, TrimConfig};
use static_cell::StaticCell;

bind_interrupts!(struct Irqs {
    UART1_IRQ => uart::InterruptHandler<UART1>;
});

#[entry]
fn main() -> ! {
    static EXECUTOR: StaticCell<Executor> = StaticCell::new();
    let executor = EXECUTOR.init(Executor::new());

    executor.run(|spawner| {
        let spawner: Spawner = spawner.into();

        unwrap!(spawner.spawn(defmt::intern!("main"), main(spawner)));
    });
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

#[embassy_executor::task]
async fn main(spawner: Spawner) {
    match main_inner(spawner).await {
        Ok(()) => info!("Main returned Ok(())"),
        Err(e) => loop {
            error!("Main returned error: {}", e);
            Timer::after_secs(1).await;
        },
    }
}

const MAX_PERC_DFL: f32 = 0.02;
const MIN_PERC_DFL: f32 = 0.10;

async fn main_inner(spawner: Spawner) -> Result<(), &'static str> {
    logger::set_spawner(spawner);

    // let a = __embassy_main(spawner);

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

    // warn!("Arming esc");
    // arm_esc(&mut prop).await;
    // info!("ESC armed");

    spawner
        .spawn(defmt::intern!("Blink LED"), blink_led(led))
        .expect("failed to spawn task");

    spawner
        .spawn(defmt::intern!("Log 1Hz"), log_1_hz())
        .expect("failed to spawn task");
    spawner
        .spawn(defmt::intern!("Log 2Hz"), log_2_hz())
        .expect("failed to spawn task");
    spawner
        .spawn(defmt::intern!("Log 4Hz"), log_4_hz())
        .expect("failed to spawn task");

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
    let mut ticker = Ticker::every(Duration::from_millis(500));
    loop {
        defmt::info!("2Hz log");
        ticker.next().await;
    }
}

#[embassy_executor::task]
async fn log_4_hz() {
    let mut ticker = Ticker::every(Duration::from_millis(250));

    loop {
        defmt::info!("4Hz log");
        ticker.next().await;
    }
}
