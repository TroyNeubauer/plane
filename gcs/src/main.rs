use anyhow::{Context, Result, anyhow, bail};
use clap::Parser;
use gilrs::{Axis, Button, Event, EventType, Gilrs};
use plane_core::{ControlState, FcInput, MAGIC, MSG_LEN};
use serialport::SerialPortType;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub enum GcsEvent {
    // -1..1 desired pitch offset for elevators
    Pitch(f32),
    // -1..1 desired yaw offset for tail
    Yaw(f32),
    // -1..1 desired yaw offset for ailerons
    Roll(f32),
    // 0..1 desired throttle
    Throttle(f32),
    Arm,
    Disarm,
}

pub struct ControlMapping {
    pitch: Axis,
    yaw: Axis,
    roll: Axis,
    throttle: Axis,
    arm: Button,
    disarm: Button,
}

impl ControlMapping {
    fn map_to_message(&self, event: EventType) -> Option<GcsEvent> {
        match event {
            gilrs::EventType::ButtonPressed(button, _) => {
                if button == self.disarm {
                    return Some(GcsEvent::Disarm);
                }
            }
            gilrs::EventType::ButtonRepeated(button, _) => {
                if button == self.disarm {
                    return Some(GcsEvent::Disarm);
                }
            }
            gilrs::EventType::ButtonReleased(button, _) => {
                if button == self.disarm {
                    return Some(GcsEvent::Disarm);
                } else if button == self.arm {
                    return Some(GcsEvent::Arm);
                }
            }
            gilrs::EventType::AxisChanged(axis, value, _) => {
                if axis == self.pitch {
                    return Some(GcsEvent::Pitch(value));
                } else if axis == self.yaw {
                    return Some(GcsEvent::Yaw(value));
                } else if axis == self.roll {
                    return Some(GcsEvent::Roll(value));
                } else if axis == self.throttle {
                    return Some(GcsEvent::Throttle(value));
                }
            }
            _ => {}
        }
        None
    }
}

impl Default for ControlMapping {
    fn default() -> Self {
        Self {
            pitch: Axis::LeftStickY,
            yaw: Axis::RightStickX,
            roll: Axis::LeftStickX,
            throttle: Axis::RightStickY,
            arm: Button::RightTrigger,
            disarm: Button::LeftTrigger,
        }
    }
}

#[derive(Debug, Parser)]
#[clap(about = "Ground station for laser plane")]
pub struct Args {
    #[clap(short = 'd', value_parser, default_value = "0.05")]
    deadband: f32,
    #[clap(value_parser, default_value = "150.0")]
    filter_rate_hz: f32,
    #[clap(value_parser, default_value = "50.0")]
    send_rate_hz: f32,
    #[clap(value_parser, default_value = "0.7")]
    alpha: f32,
    #[clap(value_parser, default_value = "1.6")]
    exponent: f32,
}

fn packet_for_input(input: &FcInput) -> Result<[u8; MSG_LEN]> {
    let mut dst = [0u8; MSG_LEN];
    // Magic
    dst[0] = MAGIC;
    postcard::to_slice(&input, &mut dst[1..])?;

    Ok(dst)
}

fn main() -> Result<()> {
    let args = Args::parse();

    let mut gilrs = Gilrs::new().unwrap();

    /*let _ = gilrs
        .gamepads()
        .next()
        .ok_or_else(|| anyhow!("No gamepads detected"))?;*/

    let port_info = 'outer: loop {
        for info in serialport::available_ports().context("Failed to list serial ports")? {
            if let SerialPortType::UsbPort(usb) = &info.port_type {
                if usb.manufacturer == Some("Embassy".to_string()) {
                    println!("Found flight controller serial - skipping");
                    continue;
                }
            }
            break 'outer info;
        }
        bail!("Failed to find local GCS radio");
    };

    println!("Opening: {port_info:?}");

    let builder = serialport::new("/dev/tty.usbserial-B000IV2L"/*port_info.port_name*/, 57600);
    let mut port = builder.open().context("Failed to open serial port")?;

    const MIN_VAL: f32 = 0.001;

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

    loop {
        let now = Instant::now();
        // Update state based on events
        while let Some(Event { event, .. }) = gilrs.next_event() {
            if let Some(msg) = input_mapping.map_to_message(event) {
                match msg {
                    // Invert pitch so that pulling stick towards you makes plane pitch up
                    GcsEvent::Pitch(v) => raw_state.pitch = -exp(v, args.exponent),
                    GcsEvent::Yaw(v) => raw_state.yaw = exp(v, args.exponent),
                    GcsEvent::Roll(v) => raw_state.roll = exp(v, args.exponent),
                    GcsEvent::Throttle(v) => raw_state.throttle = exp(v.max(0.0), args.exponent),
                    GcsEvent::Arm => armed = true,
                    GcsEvent::Disarm => armed = false,
                }
            }

            if let EventType::Disconnected = &event {
                println!("Controller disconnected. Exiting");
                let Ok(bytes) = packet_for_input(&FcInput {
                    controls: Default::default(),
                    armed: false,
                })
                .map_err(|e| println!("Failed to serialize control packet: {e:?}")) else {
                    continue;
                };
                println!("Sending: {filtered_state:?}: {bytes:02X?}");
                port.write_all(&bytes)
                    .context("Failed to write to serial port")?;

                port.flush().context("Failed to flush serial port")?;
                return Ok(());
            }
        }

        if !armed {
            if raw_state.pitch > args.deadband
                || raw_state.yaw > args.deadband
                || raw_state.roll > args.deadband
                || raw_state.throttle > args.deadband
            {
                println!("Controller is disarmed. Press right trigger to arm vehicle");
            }

            raw_state = ControlState {
                pitch: 0.0,
                yaw: 0.0,
                roll: 0.0,
                throttle: 0.0,
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
            {
                continue;
            }
            println!("Sending: {filtered_state:?}");

            last_state_sent = filtered_state.clone();
            let Ok(bytes) = packet_for_input(&FcInput {
                controls: filtered_state.clone(),
                armed,
            })
            .map_err(|e| println!("Failed to serialize control packet: {e:?}")) else {
                continue;
            };
            println!("Sending: {filtered_state:?}: {bytes:02X?}");
            port.write_all(&bytes)
                .context("Failed to write to serial port")?;
            port.flush().context("Failed to flush serial port")?;

            next_send = now + Duration::from_secs_f32(1.0 / args.send_rate_hz);
        }

        std::thread::sleep(Duration::from_millis(1));
    }
}
