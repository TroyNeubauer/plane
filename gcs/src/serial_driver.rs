use anyhow::{Context, Result, bail};
use async_channel::{Receiver, Sender};
use bitflare::{BitflareReader, BitflareWriter};
use log::{error, info, warn};
use plane_core::{FcInput, FcOutput, MAX_FC_INPUT_PAYLOAD, MAX_FC_OUTPUT_PACKET};
use std::time::Duration;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf},
    runtime::Runtime,
};
use tokio_serial::{SerialPortBuilderExt, SerialPortType, SerialStream};

pub struct SerialDriver {
    pub usb_serial_number: String,
    pub baud_rate: u32,
    pub fc_command_rx: Receiver<FcInput>,
    pub fc_telemetry_tx: Sender<FcOutput>,
}

pub mod rates {
    use plane_core::byte_rate_counter::ByteRateCounter;
    use std::{
        sync::{Mutex, OnceLock},
        time::Duration,
    };

    pub static UP: OnceLock<Mutex<ByteRateCounter>> = OnceLock::new();
    pub static DOWN: OnceLock<Mutex<ByteRateCounter>> = OnceLock::new();

    pub fn add_bytes_up(n: usize) {
        let up = UP
            .get_or_init(|| Mutex::new(ByteRateCounter::over_internal(Duration::from_millis(250))));
        up.lock().unwrap().update(n);
    }

    pub fn add_bytes_down(n: usize) {
        let down = DOWN
            .get_or_init(|| Mutex::new(ByteRateCounter::over_internal(Duration::from_millis(250))));
        down.lock().unwrap().update(n);
    }

    pub fn get_up_rate() -> f64 {
        UP.get()
            .map(|up| up.lock().unwrap().rate_f64())
            .unwrap_or_default()
    }

    pub fn get_down_rate() -> f64 {
        DOWN.get()
            .map(|up| up.lock().unwrap().rate_f64())
            .unwrap_or_default()
    }
}

impl SerialDriver {
    pub fn start_tasks(self, runtime: &Runtime) {
        runtime.spawn(async move {
            loop {
                let Ok(port) = open_port(&self.usb_serial_number, self.baud_rate)
                    .await
                    .map_err(|e| warn!("{e:?}"))
                else {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                };
                let (rx, tx) = tokio::io::split(port);

                match tokio::try_join!(
                    write_commands_task(tx, &self.fc_command_rx),
                    read_telemetry_task(rx, &self.fc_telemetry_tx)
                ) {
                    Ok(_) => {
                        error!("Serial tasks ended early");
                    }
                    Err(e) => {
                        warn!("Serial task failed: {e:?}");
                        tokio::time::sleep(Duration::from_secs(2)).await;
                    }
                }
            }
        });
    }
}

async fn write_commands_task(
    mut port: WriteHalf<SerialStream>,
    fc_command_rx: &Receiver<FcInput>,
) -> Result<()> {
    let mut buf = [0u8; MAX_FC_INPUT_PAYLOAD + 4];
    while let Ok(msg) = fc_command_rx.recv().await {
        let mut writer = BitflareWriter::new(&mut buf);
        if writer
            .write_payload(|dst| {
                let written = postcard::to_slice(&msg, dst)
                    .map_err(|e| warn!("Failed to serialize postcard message: {e:?}"))?;
                Ok(written.len())
            })
            .is_ok()
        {
            let bytes = writer.finish();
            port.write(bytes)
                .await
                .context("Failed to write to pilot radio")?;
            rates::add_bytes_up(bytes.len());
        }
    }
    bail!("flight controller command channel closed early");
}

async fn read_telemetry_task(
    mut port: ReadHalf<SerialStream>,
    fc_telemetry_tx: &Sender<FcOutput>,
) -> Result<()> {
    let mut buf = vec![0u8; 4096];

    let mut reader = BitflareReader::<MAX_FC_OUTPUT_PACKET>::new();
    loop {
        let n = port
            .read(&mut buf)
            .await
            .context("Failed to read from pilot radio")?;
        rates::add_bytes_down(n);

        reader.decode(&buf[..n], |payload| {
            let Ok(t) = postcard::from_bytes::<FcOutput>(payload)
                .map_err(|e| warn!("Failed to deserialize postcard message: {e:?}"))
            else {
                return;
            };

            if let Err(e) = fc_telemetry_tx.try_send(t) {
                warn!("Failed to send fc telemetry message to main thread: {e:?}");
            }
        });
    }
}

async fn open_port(serial_number: &str, baud_rate: u32) -> Result<SerialStream> {
    for info in tokio_serial::available_ports().context("Failed to list serial ports")? {
        if let SerialPortType::UsbPort(usb) = &info.port_type {
            if usb.serial_number.as_deref() == Some(serial_number) {
                let builder = tokio_serial::new(&info.port_name, baud_rate);

                let p = builder
                    .open_native_async()
                    .context("Failed to open serial port")?;
                info!("Opened port {}", info.port_name);
                return Ok(p);
            }
        }
    }
    bail!("No GCS radio connected");
}
