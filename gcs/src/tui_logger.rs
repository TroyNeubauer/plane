use crossbeam_channel::TrySendError;
use log::{Level, LevelFilter, Metadata, Record};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use std::sync::OnceLock;
use std::time::Instant;

struct TuiLogger {
    tx: crossbeam_channel::Sender<Line<'static>>,
}

fn get_timestamp() -> String {
    static START: OnceLock<Instant> = OnceLock::new();
    let start = START.get_or_init(|| Instant::now());

    let duration = start.elapsed();
    format!("{}.{:03}s", duration.as_secs(), duration.subsec_millis())
}

impl log::Log for TuiLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Info
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let ts = get_timestamp();
            let level_text = format!("{:5}", record.level());
            let level_style = match record.level() {
                Level::Error => Style::default().fg(Color::Red),
                Level::Warn => Style::default().fg(Color::Yellow),
                Level::Info => Style::default().fg(Color::Green),
                Level::Debug => Style::default().fg(Color::Blue),
                Level::Trace => Style::default().fg(Color::Cyan),
            };

            // Use a subtle style for brackets and non-critical parts.
            let subtle = Style::default().fg(Color::DarkGray);
            let module = record.module_path().unwrap_or("");

            // Build header spans: [timestamp level module?]
            let mut spans = Vec::new();
            spans.push(Span::styled("[", subtle));
            spans.push(Span::raw(format!("{} ", ts)));
            spans.push(Span::styled(level_text, level_style));
            if !module.is_empty() {
                spans.push(Span::raw(format!(" {} ", module)));
            } else {
                spans.push(Span::raw(" "));
            }
            spans.push(Span::styled("]", subtle));
            spans.push(Span::raw(" "));
            // Append the actual log message.
            spans.push(Span::raw(format!("{}", record.args())));

            let line = Line::from(spans);
            if let Err(TrySendError::Disconnected(line)) = self.tx.try_send(line) {
                let s = line.to_string();
                println!("{s}");
            }
        }
    }

    fn flush(&self) {}
}

pub fn init(tx: crossbeam_channel::Sender<Line<'static>>) {
    let logger = Box::leak(Box::new(TuiLogger { tx }));
    log::set_logger(logger).expect("Global logger already initialized!");
    log::set_max_level(LevelFilter::Info);
}
