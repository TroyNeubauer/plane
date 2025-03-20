use plane_core::TrimConfig;
use ratatui::{
    DefaultTerminal,
    layout::{Constraint, Layout},
    text::Text,
    widgets::{Block, List, Widget},
};

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
    currently_editing: TrimItem,
    config: TrimConfig,
}

impl Widget for TrimAdjuster {
    fn render(self, area0: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer)
    where
        Self: Sized,
    {
        let block = Block::bordered().title(format!(
            "Editing {:?} (Use dpad left/right to edit other value, dpad up/down to inc/dec)",
            self.currently_editing
        ));
        let area = block.inner(area0);
        block.render(area0, buf);
        Text::from(
            match self.currently_editing {
                TrimItem::Elevator => self.config.elevator,
                TrimItem::LeftAileron => self.config.left_aileron,
                TrimItem::RightAileron => self.config.right_aileron,
                TrimItem::RollRange => self.config.roll_range,
                TrimItem::ElevatorRange => self.config.elevator_range,
            }
            .to_string(),
        )
        .render(area, buf);
    }
}

#[derive(Debug, Clone, Default)]
struct Log {
    inner: Vec<String>,
}

impl Widget for Log {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer)
    where
        Self: Sized,
    {
        let len = self.inner.len();
        let items = self.inner;
        List::default()
            .items(
                items
                    .into_iter()
                    .skip((len.max(area.height as usize) - area.height as usize).max(0)),
            )
            .render(area, buf);
    }
}

pub struct Tui {
    terminal: DefaultTerminal,
    log: Log,
    trim: TrimAdjuster,
}

impl Tui {
    pub fn new(terminal: DefaultTerminal) -> Self {
        Tui {
            terminal,
            log: Default::default(),
            trim: Default::default(),
        }
    }

    pub fn run(&mut self) {
        self.terminal
            .draw(|frame| {
                let main =
                    Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                        .split(frame.area());
                let left = main[0];
                let right = main[1];

                frame.render_widget(self.log.clone(), left);
                frame.render_widget(self.trim.clone(), right);
            })
            .expect("render");
    }

    pub fn log(&mut self, s: impl Into<String>) {
        self.log.inner.push(s.into());
    }

    pub fn more_trim(&mut self) {
        match self.trim.currently_editing {
            TrimItem::Elevator => {
                self.trim.config.elevator += 0.1;
            }
            TrimItem::LeftAileron => {
                self.trim.config.left_aileron += 0.1;
            }
            TrimItem::RightAileron => {
                self.trim.config.right_aileron += 0.1;
            }
            TrimItem::RollRange => {
                self.trim.config.roll_range += 0.1;
            }
            TrimItem::ElevatorRange => {
                self.trim.config.elevator_range += 0.1;
            }
        }
    }

    pub fn less_trim(&mut self) {
        match self.trim.currently_editing {
            TrimItem::Elevator => {
                self.trim.config.elevator -= 0.05;
            }
            TrimItem::LeftAileron => {
                self.trim.config.left_aileron -= 0.05;
            }
            TrimItem::RightAileron => {
                self.trim.config.right_aileron -= 0.05;
            }
            TrimItem::RollRange => {
                self.trim.config.roll_range -= 0.05;
            }
            TrimItem::ElevatorRange => {
                self.trim.config.elevator_range -= 0.05;
            }
        }
    }

    pub fn next_trim(&mut self) {
        match self.trim.currently_editing {
            TrimItem::Elevator => {
                self.trim.currently_editing = TrimItem::LeftAileron;
            }
            TrimItem::LeftAileron => {
                self.trim.currently_editing = TrimItem::RightAileron;
            }
            TrimItem::RightAileron => {
                self.trim.currently_editing = TrimItem::RollRange;
            }
            TrimItem::RollRange => {
                self.trim.currently_editing = TrimItem::ElevatorRange;
            }
            TrimItem::ElevatorRange => {
                self.trim.currently_editing = TrimItem::Elevator;
            }
        }
    }

    pub fn previous_trim(&mut self) {
        match self.trim.currently_editing {
            TrimItem::LeftAileron => {
                self.trim.currently_editing = TrimItem::Elevator;
            }
            TrimItem::RightAileron => {
                self.trim.currently_editing = TrimItem::LeftAileron;
            }
            TrimItem::RollRange => {
                self.trim.currently_editing = TrimItem::RightAileron;
            }
            TrimItem::ElevatorRange => {
                self.trim.currently_editing = TrimItem::RollRange;
            }
            TrimItem::Elevator => {
                self.trim.currently_editing = TrimItem::ElevatorRange;
            }
        }
    }

    pub fn trim(&self) -> TrimConfig {
        self.trim.config.clone()
    }
}
