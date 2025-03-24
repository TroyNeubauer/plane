use anyhow::{Context, Result, bail};
use async_channel::{Receiver, Sender};
use bitflare::{BitflareReader, BitflareWriter};
use log::{error, info, warn};
use plane_core::{FcInput, FcOutput, MAX_FC_INPUT_PAYLOAD, MAX_FC_OUTPUT_PACKET};
use std::{
    path::{Path, PathBuf},
    sync::atomic::Ordering,
    time::Duration,
};
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
    pub tx_log_path: Option<PathBuf>,
    pub rx_log_path: Option<PathBuf>,
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

                let (tx_file, rx_file) =
                    create_log_files(self.tx_log_path.as_deref(), self.rx_log_path.as_deref())
                        .await;

                match tokio::try_join!(
                    write_commands_task(tx, &self.fc_command_rx, tx_file),
                    read_telemetry_task(rx, &self.fc_telemetry_tx, rx_file)
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

async fn create_log_files(
    tx_log_path: Option<&Path>,
    rx_log_path: Option<&Path>,
) -> (Option<tokio::fs::File>, Option<tokio::fs::File>) {
    let tx = match tx_log_path.as_ref() {
        Some(p) => match tokio::fs::File::create(p).await {
            Ok(f) => {
                info!("Created log file {p:?} successfully");
                Some(f)
            }
            Err(e) => {
                warn!("Failed to create output file {p:?}: {e:?}");
                None
            }
        },
        None => None,
    };

    let rx = match rx_log_path.as_ref() {
        Some(p) => match tokio::fs::File::create(p).await {
            Ok(f) => {
                info!("Created log file {p:?} successfully");
                Some(f)
            }
            Err(e) => {
                warn!("Failed to create output file {p:?}: {e:?}");
                None
            }
        },
        None => None,
    };

    (tx, rx)
}

async fn write_commands_task(
    mut port: WriteHalf<SerialStream>,
    fc_command_rx: &Receiver<FcInput>,
    mut log_file: Option<tokio::fs::File>,
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

            if let Some(f) = log_file.as_mut() {
                if let Err(e) = f.write_all(bytes).await {
                    warn!("Failed to write to serial tx logfile: {e:?}");
                }
            }
        }
    }
    bail!("flight controller command channel closed early");
}

async fn read_telemetry_task(
    mut port: ReadHalf<SerialStream>,
    fc_telemetry_tx: &Sender<FcOutput>,
    mut log_file: Option<tokio::fs::File>,
) -> Result<()> {
    let mut buf = vec![0u8; 4096];

    let mut reader = BitflareReader::<MAX_FC_OUTPUT_PACKET>::new();
    loop {
        let n = port
            .read(&mut buf)
            .await
            .context("Failed to read from pilot radio")?;
        let buf = &buf[..n];
        rates::add_bytes_down(n);

        reader.decode(buf, |payload| {
            let Ok(t) = postcard::from_bytes::<FcOutput>(payload)
                .map_err(|e| warn!("Failed to deserialize postcard message: {e:?}"))
            else {
                return;
            };

            if let Err(e) = fc_telemetry_tx.try_send(t) {
                // Send errors are expected when things are shutting down
                if crate::RUNNING.load(Ordering::Relaxed) {
                    warn!("Failed to send fc telemetry message to main thread: {e:?}");
                }
            }
        });

        if let Some(f) = log_file.as_mut() {
            if let Err(e) = f.write_all(buf).await {
                warn!("Failed to write to serial rx logfile: {e:?}");
            }
        }
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
