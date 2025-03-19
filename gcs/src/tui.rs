use std::sync::mpsc::Receiver;

use plane_core::TrimConfig;
use ratatui::{crossterm::{event::{self as tui_event, Event as TUIEvent}, terminal}, layout::{Constraint, Layout}, text::Text, widgets::{Block, List, Widget}, DefaultTerminal, Frame};

#[derive(Debug, Default, Clone)]
enum ControlSurface {
    #[default]
    Elevator,
    LeftAileron,
    RightAileron
}

#[derive(Debug, Default, Clone)]
pub struct TrimAdjuster {
    currently_editing: ControlSurface,
    config: TrimConfig
}

impl Widget for TrimAdjuster {
    fn render(self, area0: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer)
    where
        Self: Sized {
            let block = Block::bordered().title(format!("Editing {:?}", self.currently_editing));
            let area = block.inner(area0);
            block.render(area0, buf);
            Text::from(match self.currently_editing {
                ControlSurface::Elevator => self.config.elevator,
                ControlSurface::LeftAileron => self.config.left_aileron,
                ControlSurface::RightAileron => self.config.right_aileron,
            }.to_string()).render(area, buf);
    }
}

#[derive(Debug, Clone, Default)]
struct Log {
    inner: Vec<String>
}

impl Widget for Log {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer)
    where
        Self: Sized {
            let len = self.inner.len();
            let items = self.inner;
            List::default().items(items.into_iter().skip((len.max(area.height as usize) - area.height as usize).max(0))).render(area, buf);
    }
}

pub fn render(frame: &mut Frame, content: String) {
    frame.render_widget(content, frame.area());
}

pub struct Tui {
    terminal: DefaultTerminal,
    log: Log,
    trim: TrimAdjuster
}

impl Tui {
    pub fn new(terminal: DefaultTerminal) -> Self {
        Tui {
            terminal,
            log: Default::default(),
            trim: Default::default()
        }
    }

    pub fn run(&mut self) {
        self.terminal.draw(|frame| {
            let main = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(frame.area());
            let left = main[0];
            let right = main[1];

            frame.render_widget(self.log.clone(), left);
            frame.render_widget(self.trim.clone(), right);
        }).expect("render");
    }

    pub fn log(&mut self, s: impl Into<String>) {
        self.log.inner.push(s.into());
    }

    pub fn more_trim(&mut self) {
        match self.trim.currently_editing {
            ControlSurface::Elevator => {
                self.trim.config.elevator += 0.1;
            },
            ControlSurface::LeftAileron => {
                self.trim.config.left_aileron += 0.1;
            },
            ControlSurface::RightAileron => {
                self.trim.config.right_aileron += 0.1;
            }
        }
    }

    pub fn less_trim(&mut self) {
        match self.trim.currently_editing {
            ControlSurface::Elevator => {
                self.trim.config.elevator -= 0.05;
            },
            ControlSurface::LeftAileron => {
                self.trim.config.left_aileron -= 0.05;
            },
            ControlSurface::RightAileron => {
                self.trim.config.right_aileron -= 0.05;
            }
        }
    }

    pub fn next_trim(&mut self) {
        match self.trim.currently_editing {
            ControlSurface::Elevator => {
                self.trim.currently_editing = ControlSurface::LeftAileron;
            },
            ControlSurface::LeftAileron => {
                self.trim.currently_editing = ControlSurface::RightAileron;
            },
            ControlSurface::RightAileron => {
                self.trim.currently_editing = ControlSurface::Elevator;
            }
        }
    }

    pub fn previous_trim(&mut self) {
        match self.trim.currently_editing {
            ControlSurface::Elevator => {
                self.trim.currently_editing = ControlSurface::RightAileron;
            },
            ControlSurface::LeftAileron => {
                self.trim.currently_editing = ControlSurface::Elevator;
            },
            ControlSurface::RightAileron => {
                self.trim.currently_editing = ControlSurface::LeftAileron;
            }
        }
    }

    pub fn trim(&self) -> TrimConfig {
        self.trim.config.clone()
    }
}

#[derive(Debug)]
pub enum GUIMessage {
    Log(String)
}
