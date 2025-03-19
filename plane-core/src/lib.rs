#![no_std]

use serde::{Deserialize, Serialize};

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


#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TrimConfig {
    pub elevator: f32,
    pub left_aileron: f32,
    pub right_aileron: f32,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct FcInput {
    pub trim: TrimConfig,
    pub controls: ControlState,
    pub armed: bool,
}

pub const MSG_LEN: usize = 32;

pub const MAGIC: u8 = 0xFC;
