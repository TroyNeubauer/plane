use std::{fmt::Display, sync::mpsc::Receiver};

use plane_core::TrimConfig;
use ratatui::{crossterm::{event::{self as tui_event, Event as TUIEvent}, terminal}, layout::{Constraint, Layout}, text::Text, widgets::{Block, List, Widget}, DefaultTerminal, Frame};

#[derive(Debug, Default, Clone)]
enum TrimItem {
    #[default]
    Elevator,
    LeftAileron,
    RightAileron,
    RollRange,
    ElevatorRange,
}

#[derive(Debug, Default, Clone)]
pub struct TrimAdjuster {
    pub currently_editing: TrimItem,
    pub config: TrimConfig
}

impl Display for TrimAdjuster {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        format!("Editing {:?} (Use dpad left/right to edit other value, dpad up/down to inc/dec)", self.currently_editing).fmt(f);

        match self.currently_editing {
            TrimItem::Elevator => self.config.elevator,
            TrimItem::LeftAileron => self.config.left_aileron,
            TrimItem::RightAileron => self.config.right_aileron,
            TrimItem::RollRange => self.config.roll_range,
            TrimItem::ElevatorRange => self.config.elevator_range,
        }.fmt(f);
        Ok(())
    }
}

impl TrimAdjuster {
    pub fn more_trim(&mut self) {
        match self.currently_editing {
            TrimItem::Elevator => {
                self.config.elevator += 0.1;
            },
            TrimItem::LeftAileron => {
                self.config.left_aileron += 0.1;
            },
            TrimItem::RightAileron => {
                self.config.right_aileron += 0.1;
            },
            TrimItem::RollRange => {
                self.config.roll_range += 0.1;
            }
            TrimItem::ElevatorRange => {
                self.config.elevator_range += 0.1;
            }
        }
    }

    pub fn less_trim(&mut self) {
        match self.currently_editing {
            TrimItem::Elevator => {
                self.config.elevator -= 0.05;
            },
            TrimItem::LeftAileron => {
                self.config.left_aileron -= 0.05;
            },
            TrimItem::RightAileron => {
                self.config.right_aileron -= 0.05;
            },
            TrimItem::RollRange => {
                self.config.roll_range -= 0.05;
            }
            TrimItem::ElevatorRange => {
                self.config.elevator_range -= 0.05;
            }
        }
    }

    pub fn next_trim(&mut self) {
        match self.currently_editing {
            TrimItem::Elevator => {
                self.currently_editing = TrimItem::LeftAileron;
            },
            TrimItem::LeftAileron => {
                self.currently_editing = TrimItem::RightAileron;
            },
            TrimItem::RightAileron => {
                self.currently_editing = TrimItem::RollRange;
            },
            TrimItem::RollRange => {
                self.currently_editing = TrimItem::ElevatorRange;
            }
            TrimItem::ElevatorRange => {
                self.currently_editing = TrimItem::Elevator;
            }
        }
    }

    pub fn previous_trim(&mut self) {
        match self.currently_editing {
            TrimItem::LeftAileron => {
                self.currently_editing = TrimItem::Elevator;
            },
            TrimItem::RightAileron => {
                self.currently_editing = TrimItem::LeftAileron;
            },
            TrimItem::RollRange => {
                self.currently_editing = TrimItem::RightAileron;
            }
            TrimItem::ElevatorRange => {
                self.currently_editing = TrimItem::RollRange;
            }
            TrimItem::Elevator => {
                self.currently_editing = TrimItem::ElevatorRange;
            }
        }
    }

    pub fn trim(&self) -> TrimConfig {
        self.config.clone()
    }
}

#[derive(Debug)]
pub enum GUIMessage {
    Log(String)
}
