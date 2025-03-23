use anyhow::{Result, anyhow, bail};
use defmt_decoder::{
    DecodeError, Encoding, Frame, Location, StreamDecoder, Table, log::format::FormatterConfig,
};
use log::{Level, Record as LogRecord, warn};
use std::{
    collections::BTreeMap,
    env,
    path::{Path, PathBuf},
};

pub struct DefmtLogDecoder {
    stream_decoder: Box<dyn StreamDecoder>,
    show_skipped_frames: bool,
    locs: Option<BTreeMap<u64, Location>>,
    current_dir: PathBuf,
    encoding: Encoding,
}

impl DefmtLogDecoder {
    pub fn new(elf_path: impl AsRef<Path>) -> Result<Self> {
        // read and parse elf file
        let bytes = std::fs::read(elf_path.as_ref())?;
        let current_dir = env::current_dir()?;

        let table = Box::leak(Box::new(
            Table::parse(&bytes)?.ok_or_else(|| anyhow!(".defmt data not found"))?,
        ));
        let locs = table.get_locations(&bytes)?;

        // check if the locations info contains all the indicies
        let locs = if table.indices().all(|idx| locs.contains_key(&(idx as u64))) {
            Some(locs)
        } else {
            warn!("location info is incomplete; it will be omitted from the output");
            None
        };

        let mut formatter_config = FormatterConfig::default().with_location();

        formatter_config.is_timestamp_available = table.has_timestamp();

        let encoding = table.encoding();
        let stream_decoder = table.new_stream_decoder();

        Ok(Self {
            stream_decoder,
            show_skipped_frames: true,
            encoding,
            locs,
            current_dir,
        })
    }

    pub fn decode(&mut self, buf: &[u8]) -> Result<()> {
        self.stream_decoder.received(buf);

        // decode the received data
        loop {
            match self.stream_decoder.decode() {
                Ok(frame) => {
                    let (file, line, mod_path) = if let Some(loc) = self
                        .locs
                        .as_ref()
                        .map(|locs| locs.get(&frame.index()))
                        .flatten()
                    {
                        // try to get the relative path, else the full one
                        let path = loc
                            .file
                            .strip_prefix(&self.current_dir)
                            .unwrap_or(&loc.file);

                        (
                            Some(path.display().to_string()),
                            Some(loc.line as u32),
                            Some(loc.module.clone()),
                        )
                    } else {
                        (None, None, None)
                    };

                    log_defmt(&frame, file.as_deref(), line, mod_path.as_deref());
                }
                Err(DecodeError::UnexpectedEof) => break Ok(()),
                Err(DecodeError::Malformed) => match self.encoding.can_recover() {
                    // if recovery is impossible, abort
                    false => bail!("Got mailformed defmt stream"),
                    // if recovery is possible, skip the current frame and continue with new data
                    true => {
                        // bug: https://github.com/rust-lang/rust-clippy/issues/9810
                        #[allow(clippy::print_literal)]
                        if self.show_skipped_frames {
                            warn!("(HOST) skipping malformed defmt frame from flight controller");
                        }
                        continue;
                    }
                },
            }
        }
    }
}

/// Logs a defmt frame using the `log` facade.
pub fn log_defmt(
    frame: &Frame<'_>,
    file: Option<&str>,
    line: Option<u32>,
    module_path: Option<&str>,
) {
    let _timestamp = frame
        .display_timestamp()
        .map(|ts| ts.to_string())
        .unwrap_or_default();

    let level = frame
        .level()
        .map(|level| match level {
            defmt_parser::Level::Trace => Level::Trace,
            defmt_parser::Level::Debug => Level::Debug,
            defmt_parser::Level::Info => Level::Info,
            defmt_parser::Level::Warn => Level::Warn,
            defmt_parser::Level::Error => Level::Error,
        })
        .unwrap_or(Level::Info);

    log::logger().log(
        &LogRecord::builder()
            .args(format_args!("{}", frame.display_message()))
            .level(level) // no need to set the level, since it is transferred via payload
            .module_path(module_path)
            .file(file)
            .line(line)
            .build(),
    );
}
