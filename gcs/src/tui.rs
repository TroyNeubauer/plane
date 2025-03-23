use anyhow::{Context, Result};
use plane_core::{ControlState, TrimConfig};
use ratatui::{
    DefaultTerminal,
    layout::{Constraint, Layout},
    text::{Line, Text},
    widgets::{Block, List, Widget},
};
use std::{collections::VecDeque, fmt::Display, time::Instant};

#[derive(Debug, Default, Copy, Clone, strum_macros::FromRepr)]
#[repr(u8)]
enum TrimItem {
    #[default]
    Elevator,
    LeftAileron,
    RightAileron,
    RollRange,
    ElevatorRange,
}
impl TrimItem {
    fn next_trim(self) -> TrimItem {
        let v = self as u8;
        TrimItem::from_repr(v + 1).unwrap_or_else(|| Self::from_repr(0).unwrap())
    }

    fn previous_trim(self) -> TrimItem {
        let v = self as u8;
        match v.checked_sub(1) {
            Some(v) => TrimItem::from_repr(v).unwrap(),
            // NOTE: Must be the last variant in `TrimItem`
            None => TrimItem::ElevatorRange,
        }
    }
}

const TRIM_PATH: &str = "./trim_config.json";

pub struct Tui {
    terminal: DefaultTerminal,
    log: Log,
    pub trim: TrimAdjuster,
    last_tx_command: LastTxCommand,
    log_rx: crossbeam_channel::Receiver<Line<'static>>,
}

impl Tui {
    pub fn new(
        terminal: DefaultTerminal,
        trim: TrimAdjuster,
        log_rx: crossbeam_channel::Receiver<Line<'static>>,
    ) -> Self {
        Tui {
            terminal,
            log: Default::default(),
            trim,
            log_rx,
            last_tx_command: Default::default(),
        }
    }

    pub fn draw(&mut self) {
        self.terminal
            .draw(|frame| {
                let main = Layout::vertical([
                    Constraint::Percentage(85),
                    Constraint::Percentage(15),
                    Constraint::Min(1),
                    Constraint::Min(1),
                ])
                .split(frame.area());

                frame.render_widget(&mut self.log, main[0]);
                frame.render_widget(&mut self.trim, main[1]);
                frame.render_widget(&mut self.last_tx_command, main[2]);
                frame.render_widget(SerialRates, main[3]);
            })
            .expect("render");
    }

    pub fn add_log(&mut self, msg: String) {
        self.log.inner.push_back(Line::from(msg));
        while self.log.inner.len() > 200 {
            self.log.inner.pop_front();
        }
    }

    pub fn update_logs(&mut self) {
        while let Ok(l) = self.log_rx.try_recv() {
            self.log.inner.push_back(l);
        }
        while self.log.inner.len() > 200 {
            self.log.inner.pop_front();
        }
    }
}

impl Display for TrimAdjuster {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "Editing {:?} (Use dpad left/right to edit other value, dpad up/down to inc/dec)",
            self.currently_editing
        ))?;

        match self.currently_editing {
            TrimItem::Elevator => self.config.elevator.fmt(f),
            TrimItem::LeftAileron => self.config.left_aileron.fmt(f),
            TrimItem::RightAileron => self.config.right_aileron.fmt(f),
            TrimItem::RollRange => self.config.roll_range.fmt(f),
            TrimItem::ElevatorRange => self.config.elevator_range.fmt(f),
        }
    }
}

impl TrimAdjuster {
    pub fn increase(&mut self) {
        match self.currently_editing {
            TrimItem::Elevator => {
                self.config.elevator += 0.1;
            }
            TrimItem::LeftAileron => {
                self.config.left_aileron += 0.1;
            }
            TrimItem::RightAileron => {
                self.config.right_aileron += 0.1;
            }
            TrimItem::RollRange => {
                self.config.roll_range += 0.1;
            }
            TrimItem::ElevatorRange => {
                self.config.elevator_range += 0.1;
            }
        }
    }

    pub fn decrease(&mut self) {
        match self.currently_editing {
            TrimItem::Elevator => {
                self.config.elevator -= 0.05;
            }
            TrimItem::LeftAileron => {
                self.config.left_aileron -= 0.05;
            }
            TrimItem::RightAileron => {
                self.config.right_aileron -= 0.05;
            }
            TrimItem::RollRange => {
                self.config.roll_range -= 0.05;
            }
            TrimItem::ElevatorRange => {
                self.config.elevator_range -= 0.05;
            }
        }
    }

    pub fn next(&mut self) {
        self.currently_editing = self.currently_editing.next_trim();
    }

    pub fn previous(&mut self) {
        self.currently_editing = self.currently_editing.previous_trim();
    }

    pub fn values(&self) -> TrimConfig {
        self.config.clone()
    }

    pub fn save(&self) -> Result<()> {
        let json = serde_json::to_string(&self.config).context("Failed to serialize trim json")?;
        std::fs::write(TRIM_PATH, &json)
            .with_context(|| format!("Failed to write trim to path {TRIM_PATH}"))?;

        Ok(())
    }
}

#[derive(Debug, Default, Clone)]
pub struct TrimAdjuster {
    currently_editing: TrimItem,
    pub config: TrimConfig,
}

impl TrimAdjuster {
    pub fn from_config() -> Result<Self> {
        let json = std::fs::read_to_string(TRIM_PATH)
            .with_context(|| format!("Failed to read trim file at {TRIM_PATH}"))?;
        let config: TrimConfig =
            serde_json::from_str(&json).context("Failed to deserialize trim json")?;
        Ok(Self {
            config,
            currently_editing: Default::default(),
        })
    }
}

impl Widget for &mut TrimAdjuster {
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
    inner: VecDeque<Line<'static>>,
}

impl Widget for &mut Log {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer)
    where
        Self: Sized,
    {
        let len = self.inner.len();
        List::default()
            .items(
                self.inner
                    .iter()
                    .skip((len.max(area.height as usize) - area.height as usize).max(0))
                    .cloned(),
            )
            .render(area, buf);
    }
}

#[derive(Debug, Clone, Default)]
pub enum LastTxCommand {
    Sent {
        input: ControlState,
        armed: bool,
        timestamp: Instant,
    },
    #[default]
    None,
}

impl Widget for &mut LastTxCommand {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer)
    where
        Self: Sized,
    {
        match self {
            LastTxCommand::Sent {
                input,
                armed,
                timestamp,
            } => Text::raw(format!(
                "input: {input:?}, armed: {armed}, ago: {:?}",
                timestamp.elapsed()
            ))
            .render(area, buf),
            LastTxCommand::None => Text::raw("No data").render(area, buf),
        };
    }
}

struct SerialRates;

impl Widget for SerialRates {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer)
    where
        Self: Sized,
    {
        let up = crate::serial_driver::rates::get_up_rate() / 1000.0;
        let down = crate::serial_driver::rates::get_down_rate() / 1000.0;
        Text::raw(format!("{up:.1}KB/s up |  {down:.1}KB/s down",)).render(area, buf);
    }
}
