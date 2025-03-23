#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
pub mod byte_rate_counter;

use serde::{Deserialize, Serialize};

pub const MAX_FC_INPUT_PAYLOAD: usize = 32;
pub const MAX_FC_OUTPUT_PACKET: usize = 64;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum FcInput {
    Trim(TrimConfig),
    Controls(ControlState),
    Arm,
    Disarm,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum FcOutput {
    // Save two bytes for the discriminant
    StringLog(heapless::String<62>),
    DefmtLog(heapless::Vec<u8, 62>),
    Panic {
        //62 bytes total
        file: heapless::String<24>,
        line: u16,
        col: u16,
        message: heapless::String<34>,
    },
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ControlState {
    pub pitch: f32,
    pub yaw: f32,
    pub roll: f32,
    pub throttle: f32,
}

impl ControlState {
    pub fn apply_deadband(&self, deadband: f32) -> ControlState {
        fn apply(x: f32, deadband: f32) -> f32 {
            if x.abs() < deadband {
                0.0
            } else {
                // Map above deadband back to regular range
                (x - x.signum() * deadband) / (1.0 - deadband)
            }
        }

        let mut ret = self.clone();
        ret.pitch = apply(self.pitch, deadband);
        ret.yaw = apply(self.yaw, deadband);
        ret.roll = apply(self.roll, deadband);
        ret.throttle = apply(self.throttle, deadband);

        ret
    }
}

#[derive(PartialEq, Clone, Debug, Default, Serialize, Deserialize)]
pub struct TrimConfig {
    pub elevator: f32,
    pub left_aileron: f32,
    pub right_aileron: f32,
    pub roll_range: f32,
    pub elevator_range: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fc_input_sizes() {
        let _: heapless::Vec<u8, MAX_FC_INPUT_PAYLOAD> =
            postcard::to_vec(&FcInput::Trim(crate::TrimConfig {
                elevator: 0.736475,
                left_aileron: 0.1574,
                right_aileron: 0.875462,
                roll_range: 0.3367,
                elevator_range: 0.75462,
            }))
            .unwrap();
    }

    #[test]
    fn gcs_input_sizes() {
        let msg = "[ERROR] test log".try_into().unwrap();
        let _: heapless::Vec<u8, MAX_FC_OUTPUT_PACKET> =
            postcard::to_vec(&FcOutput::Log(msg)).unwrap();

        let msg: heapless::String<{ MAX_FC_OUTPUT_PACKET - 2 }> =
            (0..(MAX_FC_OUTPUT_PACKET - 2)).map(|_| ' ').collect();

        let _: heapless::Vec<u8, MAX_FC_OUTPUT_PACKET> =
            postcard::to_vec(&FcOutput::Log(msg)).unwrap();
    }
}
