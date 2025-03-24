use anyhow::{Context, Result, anyhow};
use async_channel::bounded as bounded_async;
use clap::Parser;
use futures_util::StreamExt;
use gilrs::{Event, EventType, Gilrs};
use log::{debug, error, info, warn};
use plane_core::{ControlState, FcInput};
use serial_driver::SerialDriver;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tui::TrimAdjuster;

mod defmt_decoder;
mod serial_driver;
mod tui;
mod tui_logger;

mod types;
use types::*;

#[derive(Debug, Parser)]
#[clap(about = "Ground station for laser plane")]
pub struct Args {
    #[clap(long)]
    firmware_bin_path: Option<String>,
    #[clap(short = 's', long, default_value = "B000IV2L")]
    pilot_radio_serial: String,
    #[clap(short = 'b', long, default_value = "57600")]
    pilot_radio_baud_rate: u32,
    #[clap(short = 'd', long, default_value = "0.05")]
    deadband: f32,
    #[clap(long, default_value = "150.0")]
    filter_rate_hz: f32,
    #[clap(long, default_value = "50.0")]
    send_rate_hz: f32,
    #[clap(long, default_value = "0.7")]
    alpha: f32,
    #[clap(long, default_value = "1.6")]
    exponent: f32,
    #[arg(long, default_missing_value="true", num_args=0..=1)]
    always_send_controls: Option<bool>,
    #[clap(value_parser, default_value = "./logs")]
    log_dir: Option<String>,
}

pub static RUNNING: AtomicBool = AtomicBool::new(true);

fn main() -> Result<()> {
    let args = Args::parse();
    let always_send_controls = args.always_send_controls.unwrap_or(true);

    let (text_log_path, serial_tx_log_path, serial_rx_log_path) = if let Some(dir) = args.log_dir {
        let _ = std::fs::create_dir_all(&dir);

        let mut text = PathBuf::from(&dir);
        text.push("gcs");
        let _ = std::fs::create_dir_all(&text);

        text.push(timestamped_file_name("log", "txt"));

        let mut serial = PathBuf::from(&dir);
        serial.push("serial");
        let _ = std::fs::create_dir_all(&serial);

        let mut serial_tx = PathBuf::from(&serial);
        serial_tx.push(timestamped_file_name("tx", "bin"));

        let mut serial_rx = PathBuf::from(&serial);
        serial_rx.push(timestamped_file_name("rx", "bin"));
        (Some(text), Some(serial_tx), Some(serial_rx))
    } else {
        (None, None, None)
    };

    let (log_tx, log_rx) = crossbeam_channel::bounded(16);
    tui_logger::init(log_tx, text_log_path.as_deref());

    let mut gilrs = Gilrs::new().unwrap();
    let trim = match TrimAdjuster::from_config() {
        Ok(t) => {
            info!("Loaded trim config: {t:#?}");
            t
        }
        Err(e) => {
            warn!("Failed to load trim config: {e:?}");
            Default::default()
        }
    };

    let mut tui = tui::Tui::new(ratatui::init(), trim, log_rx);

    #[cfg(not(target_os = "macos"))] 
    {
        let _ = gilrs
            .gamepads()
            .next()
            .ok_or_else(|| anyhow!("No gamepads detected"))?;
    }

    // Consume stale events
    while gilrs.next_event().is_some() {}

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .expect("Failed to create tokio runtime");

    let (fc_command_tx, fc_command_rx) = bounded_async(8);
    let (fc_telemetry_tx, fc_telemetry_rx) = bounded_async(64);

    let serial_driver = SerialDriver {
        usb_serial_number: args.pilot_radio_serial,
        baud_rate: args.pilot_radio_baud_rate,
        fc_command_rx,
        fc_telemetry_tx,
        tx_log_path: serial_tx_log_path,
        rx_log_path: serial_rx_log_path,
    };

    serial_driver.start_tasks(&runtime);

    let usb_watcher = nusb::watch_devices().context("Failed to start usb watcher")?;
    runtime.spawn(usb_scan_task(usb_watcher));

    let mut defmt_decoder = match args.firmware_bin_path.as_ref() {
        Some(path) => Some(
            defmt_decoder::DefmtLogDecoder::new(path)
                .context("Failed to load firmware bin file for log parsing")?,
        ),
        None => {
            warn!("Missing firmware bin path. Defmt logs will not be displayed");
            None
        }
    };

    const MIN_VAL: f32 = 0.001;

    let mut sent_fc_usb_reset_cmd = false;
    let input_mapping = ControlMapping::default();
    let mut next_send = Instant::now();
    let mut next_filter = Instant::now();

    let mut armed = false;
    let mut raw_state = ControlState::default();
    let mut filtered_state = ControlState::default();

    let mut last_state_sent = ControlState::default();

    fn exp(x: f32, exponent: f32) -> f32 {
        let s = x.signum();
        s * x.abs().powf(exponent)
    }

    while RUNNING.load(Ordering::Acquire) {
        tui.draw();

        while let Ok(m) = fc_telemetry_rx.try_recv() {
            match m {
                plane_core::FcOutput::StringLog(l) => tui.add_log(format!("FC: {l}")),
                plane_core::FcOutput::DefmtLog(defmt_log) => {
                    if let Some(defmt) = defmt_decoder.as_mut() {
                        if let Err(e) = defmt.decode(&defmt_log) {
                            warn!("Pailed to parse defmt logs: {e:?}");
                        }
                    }
                }
                plane_core::FcOutput::Panic {
                    file,
                    line,
                    col,
                    message,
                } => tui.add_log(format!(
                    "FLIGHT CONTROLLER PANICKED: {file} {line}:{col} {message}",
                )),
            }
        }

        tui.update_logs();

        let now = Instant::now();
        let mut new_trim = false;

        if sent_fc_usb_reset_cmd && RPI_CONNECTED_ON_USB.load(Ordering::Relaxed) {
            info!("Detected rpi on usb in boot mode. Ready to reflash - exiting");

            std::thread::sleep(Duration::from_millis(200));
            RUNNING.store(false, Ordering::Relaxed);
        }

        // Update state based on events
        while let Some(Event { id, event, .. }) = gilrs.next_event() {
            let gamepad = gilrs.gamepad(id);
            if let Some(msg) = input_mapping.map_to_message(event, gamepad) {
                match msg {
                    // Invert pitch so that pulling stick towards you makes plane pitch up
                    GcsEvent::Pitch(v) => raw_state.pitch = -exp(v, args.exponent),
                    GcsEvent::Yaw(v) => raw_state.yaw = exp(v, args.exponent),
                    GcsEvent::Roll(v) => raw_state.roll = exp(v, args.exponent),
                    GcsEvent::Throttle(v) => raw_state.throttle = exp(v.max(0.0), args.exponent),
                    GcsEvent::Arm => {
                        if !armed {
                            warn!("Flight controller ARMED");
                        }
                        armed = true;
                    }
                    GcsEvent::Disarm => {
                        if armed {
                            info!("Flight controller disarmed");
                        }
                        armed = false;
                    }
                    GcsEvent::NextTrim => {
                        new_trim = true;
                        tui.trim.next()
                    }
                    GcsEvent::PreviousTrim => {
                        new_trim = true;
                        tui.trim.previous()
                    }
                    GcsEvent::MoreTrim => {
                        new_trim = true;
                        tui.trim.increase()
                    }
                    GcsEvent::LessTrim => {
                        new_trim = true;
                        tui.trim.decrease()
                    }
                    GcsEvent::Exit => {
                        info!("Received exit event. Exiting");
                        RUNNING.store(false, Ordering::Relaxed);
                    }
                    GcsEvent::ResetFcToUsbBoot => {
                        if armed {
                            error!("Refusing to reset flight controller while armed");
                        } else {
                            sent_fc_usb_reset_cmd = true;
                            info!("Sent reset command to flight controller");
                            let command = FcInput::ResetToUsbBoot;
                            if fc_command_tx.try_send(command).is_err() {
                                warn!("Failed to send controls command to serial task");
                            }
                        }
                    }
                }
            }

            if let EventType::Disconnected = &event {
                info!("Controller disconnected. Exiting");
                RUNNING.store(false, Ordering::Relaxed);
            }
        }

        if new_trim {
            let command = FcInput::Trim(tui.trim.values());
            if fc_command_tx.try_send(command).is_err() {
                debug!("Failed to send controls command to serial task");
            }

            if let Err(e) = tui.trim.save() {
                error!("Failed to save trim: {e:?}");
            }
        }

        if !armed {
            if raw_state.pitch > args.deadband
                || raw_state.yaw > args.deadband
                || raw_state.roll > args.deadband
                || raw_state.throttle > args.deadband
            {
                warn!("Controller is disarmed. Press right trigger to arm vehicle");
            }

            raw_state = ControlState {
                pitch: 0.0,
                yaw: 0.0,
                roll: 0.0,
                throttle: -1.0,
            };
        }

        if now > next_filter {
            let a = args.alpha;
            filtered_state.pitch = a * raw_state.pitch + (1.0 - a) * filtered_state.pitch;
            filtered_state.yaw = a * raw_state.yaw + (1.0 - a) * filtered_state.yaw;
            filtered_state.roll = a * raw_state.roll + (1.0 - a) * filtered_state.roll;
            filtered_state.throttle = a * raw_state.throttle + (1.0 - a) * filtered_state.throttle;

            filtered_state = filtered_state.apply_deadband(args.deadband);

            next_filter = now + Duration::from_secs_f32(1.0 / args.filter_rate_hz);
        }

        if now > next_send {
            // If the last two outputs were zero, we can skip sending since we definitely sent zero last time
            if filtered_state.pitch.abs() < MIN_VAL
                && filtered_state.yaw.abs() < MIN_VAL
                && filtered_state.roll.abs() < MIN_VAL
                && filtered_state.throttle.abs() < MIN_VAL
                && last_state_sent.pitch.abs() < MIN_VAL
                && last_state_sent.yaw.abs() < MIN_VAL
                && last_state_sent.roll.abs() < MIN_VAL
                && last_state_sent.throttle.abs() < MIN_VAL
                && !always_send_controls
            {
                continue;
            }
            debug!("Sending: {filtered_state:?}");

            let command = FcInput::Controls(filtered_state.clone());
            if fc_command_tx.try_send(command).is_err() {
                debug!("Failed to send controls command to serial task");
            }
            last_state_sent = filtered_state.clone();

            next_send = now + Duration::from_secs_f32(1.0 / args.send_rate_hz);
        }

        std::thread::sleep(Duration::from_millis(3));
    }
    ratatui::restore();
    // Drop to force logs to be printed to stdout
    drop(tui);

    info!("Disarming flight controller");
    for _ in 0..5 {
        let command = FcInput::Controls(Default::default());
        if fc_command_tx.try_send(command).is_err() {
            warn!("Failed to send disarm command to serial task");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    info!("Disarm commands sent");
    std::thread::sleep(Duration::from_millis(200));

    runtime.shutdown_background();

    return Ok(());
}

static RPI_CONNECTED_ON_USB: AtomicBool = AtomicBool::new(false);

async fn usb_scan_task(mut usb_watcher: nusb::hotplug::HotplugWatch) {
    const VENDOR_ID: u16 = 0x2E8A;
    const PRODUCT_ID: u16 = 0x0003;

    let mut devices: HashMap<nusb::DeviceId, nusb::DeviceInfo> =
        nusb::list_devices().unwrap().map(|d| (d.id(), d)).collect();

    if devices
        .iter()
        .any(|(_, d)| d.vendor_id() == VENDOR_ID && d.product_id() == PRODUCT_ID)
    {
        RPI_CONNECTED_ON_USB.store(true, Ordering::Relaxed);
        info!("RPI detected");
    }

    while let Some(event) = usb_watcher.next().await {
        match event {
            nusb::hotplug::HotplugEvent::Connected(d) => {
                devices.insert(d.id(), d);
            }
            nusb::hotplug::HotplugEvent::Disconnected(id) => {
                devices.remove(&id);
            }
        };

        let rpi_connected = devices
            .iter()
            .any(|(_, d)| d.vendor_id() == VENDOR_ID && d.product_id() == PRODUCT_ID);
        RPI_CONNECTED_ON_USB.store(rpi_connected, Ordering::Relaxed);
    }
}

pub fn timestamped_file_name(prefix: &str, extension: &str) -> String {
    let now = SystemTime::now();
    let now = now.duration_since(UNIX_EPOCH).unwrap_or_default();
    format!(
        "{prefix}_{}.{:03}.{extension}",
        now.as_secs(),
        now.subsec_millis()
    )
}
