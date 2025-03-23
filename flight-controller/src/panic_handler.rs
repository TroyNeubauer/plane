use bitflare::BitflareWriter;
use embassy_rp::gpio::{Level, Output};
use embassy_rp::peripherals::{DMA_CH0, PIN_4, PIN_25, UART1};
use embassy_rp::uart::{self, UartTx};
use embassy_time::Duration;
use plane_core::{FcOutput, MAX_FC_OUTPUT_PACKET};

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
                let _ = write!(&mut message, "{}", info.message());

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

        morse_code_sos()
    })
}

fn morse_code_sos() -> ! {
    let led_pin = unsafe { PIN_25::steal() };
    let mut led = Output::new(led_pin, Level::Low);

    loop {
        // Rapid help blick
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
}

#[defmt::panic_handler]
fn defmt_panic() -> ! {
    morse_code_sos()
}
