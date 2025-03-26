#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
pub mod byte_rate_counter;

use defmt::{Format, Str};
use serde::{Deserialize, Serialize};

pub const MAX_FC_INPUT_PAYLOAD: usize = 32;
pub const MAX_FC_OUTPUT_PACKET: usize = 128;

#[derive(Clone, Debug, Deserialize, Serialize, Format)]
pub enum FcInput {
    Trim(TrimConfig),
    Controls(ControlState),
    Arm,
    Disarm,
    ResetToUsbBoot,
}

#[derive(Clone, Debug, Deserialize, Serialize, Format)]
pub enum FcOutput {
    // Save two bytes for the discriminant
    StringLog(heapless::String<126>),
    DefmtLog(heapless::Vec<u8, 126>),
    Panic {
        //70 bytes total
        file: heapless::String<40>,
        line: u16,
        col: u16,
        message: heapless::String<82>,
    },
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Format)]
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

#[derive(PartialEq, Clone, Debug, Default, Serialize, Deserialize, Format)]
pub struct TrimConfig {
    pub elevator: f32,
    pub left_aileron: f32,
    pub right_aileron: f32,
    pub roll_range: f32,
    pub elevator_range: f32,
}

/// Max number of async tasks
pub const MAX_ASYNC_TASKS: usize = 8;
pub const MAX_DMA_ACTIONS: usize = 2;

pub struct AsyncState {
    /// The number of ticks that passed since the last update
    pub ticks_total: u32,
    /// Overhead with performing this bookkeeping
    pub ticks_overhead: u32,
    pub tasks: heapless::Vec<AsyncTaskState, { MAX_ASYNC_TASKS }>,
    pub dma_actions: heapless::Vec<DmaAction, { MAX_DMA_ACTIONS }>,
}

pub struct DmaAction {
    pub name: Str,
    /// The number of ticks this DMA transfer was running for
    pub ticks_running: u32,
    /// The number of bytes copied
    pub bytes_copied: u32,
}

pub struct AsyncTaskState {
    pub name: Str,
    /// The number of ticks the CPU was running this task's poll function
    pub ticks_running: u32,
    /// The number of ticks where this task was runnable, but the CPU was consumed with other things
    pub ticks_blocked: u32,
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
            postcard::to_vec(&FcOutput::StringLog(msg)).unwrap();

        let msg: heapless::String<{ MAX_FC_OUTPUT_PACKET - 2 }> =
            (0..(MAX_FC_OUTPUT_PACKET - 2)).map(|_| ' ').collect();

        let _: heapless::Vec<u8, MAX_FC_OUTPUT_PACKET> =
            postcard::to_vec(&FcOutput::StringLog(msg)).unwrap();
    }
}
