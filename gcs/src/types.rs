use gilrs::{Axis, Button, EventType};

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
    NextTrim,
    PreviousTrim,
    MoreTrim,
    LessTrim,
    Exit,
}

pub struct ControlMapping {
    pitch: Axis,
    yaw: Axis,
    roll: Axis,
    throttle: Axis,
    arm: Button,
    disarm: Button,
    next_trim: Button,
    prev_trim: Button,
    more_trim: Button,
    less_trim: Button,
    exit_ev_code: u32,
}

impl ControlMapping {
    pub fn map_to_message(&self, event: EventType) -> Option<GcsEvent> {
        match event {
            gilrs::EventType::ButtonPressed(button, code) => {
                if button == self.disarm {
                    return Some(GcsEvent::Disarm);
                } else if button == self.next_trim {
                    return Some(GcsEvent::NextTrim);
                } else if button == self.prev_trim {
                    return Some(GcsEvent::PreviousTrim);
                } else if button == self.more_trim {
                    return Some(GcsEvent::MoreTrim);
                } else if button == self.less_trim {
                    return Some(GcsEvent::LessTrim);
                }

                let code = code.into_u32();
                if code == self.exit_ev_code {
                    return Some(GcsEvent::Exit);
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
            next_trim: Button::DPadRight,
            prev_trim: Button::DPadLeft,
            more_trim: Button::DPadUp,
            less_trim: Button::DPadDown,
            exit_ev_code: 65852,
        }
    }
}
