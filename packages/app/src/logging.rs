//! The app's log file: `voxfusion.log` in the log directory.
//!
//! The location, the line format and the size limit are the ones the Tauri
//! log plugin used, so a log that spans the two reads as one and the tooling
//! around it keeps working. Timestamps are UTC whatever the system timezone.

use chrono::{DateTime, Utc};
use log::{Level, Log, Metadata, Record};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

use crate::paths;

const FILE_NAME: &str = "voxfusion.log";
const MAX_FILE_SIZE: u64 = 5_000_000;
const LEVEL: Level = Level::Info;

/// Starts writing the `log` crate's records to the log file, and records
/// panics there too. Never fails: while the file cannot be written, records
/// are dropped.
pub fn init() {
    install_panic_hook();

    let logger = FileLogger {
        file: Mutex::new(LogFile::new(
            paths::log_dir().join(FILE_NAME),
            MAX_FILE_SIZE,
        )),
    };

    // The logger lives as long as the process.
    if log::set_logger(Box::leak(Box::new(logger))).is_ok() {
        log::set_max_level(LEVEL.to_level_filter());
    }
}

fn install_panic_hook() {
    let default_hook = std::panic::take_hook();

    std::panic::set_hook(Box::new(move |panic_info| {
        let payload = panic_info
            .payload()
            .downcast_ref::<&str>()
            .map(|value| (*value).to_string())
            .or_else(|| panic_info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown panic payload".to_string());

        let location = panic_info
            .location()
            .map(|location| {
                format!(
                    "{}:{}:{}",
                    location.file(),
                    location.line(),
                    location.column()
                )
            })
            .unwrap_or_else(|| "unknown".to_string());

        log::error!(target: "runtime", "rust_panic payload={payload:?} location={location}");

        default_hook(panic_info);
    }));
}

struct FileLogger {
    file: Mutex<LogFile>,
}

impl Log for FileLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= LEVEL
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        // Formatted before taking the lock: a message's `Display` may itself
        // log or panic, and the panic hook logs.
        let line = format_line(Utc::now(), record);
        let mut file = self.file.lock().unwrap_or_else(PoisonError::into_inner);

        // There is nowhere to report a failure to write the log.
        let _ = file.append(&line);
    }

    fn flush(&self) {}
}

fn format_line(now: DateTime<Utc>, record: &Record) -> String {
    format!(
        "{}[{}][{}] {}\n",
        now.format("[%Y-%m-%d][%H:%M:%S]"),
        record.target(),
        record.level(),
        record.args()
    )
}

/// A log file that starts over when it is full. No older log is kept: this is
/// the log plugin's default rotation, which the app never changed.
struct LogFile {
    path: PathBuf,
    max_size: u64,
    /// The open file and its size. `None` until it could be opened, and
    /// again after a failed write, so that the next line retries.
    open: Option<(File, u64)>,
}

impl LogFile {
    fn new(path: PathBuf, max_size: u64) -> Self {
        Self {
            path,
            max_size,
            open: None,
        }
    }

    fn append(&mut self, line: &str) -> io::Result<()> {
        let length = line.len() as u64;
        let (mut file, mut size) = match self.open.take() {
            Some(open) => open,
            None => self.open_file()?,
        };

        // An empty file takes any line, or one longer than the limit would
        // start a new file forever.
        if size != 0 && size + length > self.max_size {
            drop(file);
            fs::remove_file(&self.path)?;
            (file, size) = self.open_file()?;
        }

        file.write_all(line.as_bytes())?;
        self.open = Some((file, size + length));

        Ok(())
    }

    fn open_file(&self) -> io::Result<(File, u64)> {
        if let Some(directory) = self.path.parent() {
            fs::create_dir_all(directory)?;
        }

        let file = File::options().create(true).append(true).open(&self.path)?;
        let size = file.metadata()?.len();

        Ok((file, size))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn log_file(directory: &tempfile::TempDir, max_size: u64) -> LogFile {
        LogFile::new(directory.path().join("logs").join(FILE_NAME), max_size)
    }

    #[test]
    fn formats_lines_like_the_tauri_log_plugin() {
        let now = Utc.with_ymd_and_hms(2026, 3, 9, 4, 5, 6).unwrap();
        let line = format_line(
            now,
            &Record::builder()
                .target("runtime")
                .level(Level::Info)
                .args(format_args!("setup_started cargo_package_version=0.3.0"))
                .build(),
        );

        assert_eq!(
            line,
            "[2026-03-09][04:05:06][runtime][INFO] setup_started cargo_package_version=0.3.0\n"
        );

        let line = format_line(
            now,
            &Record::builder()
                .target("audio")
                .level(Level::Warn)
                .args(format_args!("recording stream overrun id=3"))
                .build(),
        );

        assert_eq!(
            line,
            "[2026-03-09][04:05:06][audio][WARN] recording stream overrun id=3\n"
        );
    }

    #[test]
    fn logs_info_and_above() {
        let directory = tempfile::tempdir().unwrap();
        let logger = FileLogger {
            file: Mutex::new(log_file(&directory, MAX_FILE_SIZE)),
        };

        for level in [
            Level::Error,
            Level::Warn,
            Level::Info,
            Level::Debug,
            Level::Trace,
        ] {
            logger.log(
                &Record::builder()
                    .target("test")
                    .level(level)
                    .args(format_args!("message"))
                    .build(),
            );
        }

        let contents = fs::read_to_string(directory.path().join("logs").join(FILE_NAME)).unwrap();
        let lines: Vec<&str> = contents.lines().collect();

        assert_eq!(lines.len(), 3);
        assert!(lines[0].ends_with("[test][ERROR] message"));
        assert!(lines[1].ends_with("[test][WARN] message"));
        assert!(lines[2].ends_with("[test][INFO] message"));
    }

    #[test]
    fn appends_to_the_log_of_an_earlier_run() {
        let directory = tempfile::tempdir().unwrap();

        log_file(&directory, 100).append("first run\n").unwrap();
        log_file(&directory, 100).append("second run\n").unwrap();

        let contents = fs::read_to_string(directory.path().join("logs").join(FILE_NAME)).unwrap();
        assert_eq!(contents, "first run\nsecond run\n");
    }

    #[test]
    fn starts_over_when_the_next_line_would_exceed_the_limit() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("logs").join(FILE_NAME);
        let mut file = log_file(&directory, 30);

        file.append("0123456789\n").unwrap();
        file.append("0123456789\n").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap().len(), 22);

        // 22 + 11 bytes would pass the limit of 30.
        file.append("abcdefghij\n").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "abcdefghij\n");

        // Exactly at the limit still fits.
        file.append("0123456789012345678").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap().len(), 30);

        let leftovers: Vec<_> = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(leftovers, [FILE_NAME]);
    }

    #[test]
    fn replaces_a_log_that_was_already_full() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("logs").join(FILE_NAME);

        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "x".repeat(64)).unwrap();

        log_file(&directory, 30).append("fresh\n").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "fresh\n");
    }

    #[test]
    fn a_line_longer_than_the_limit_is_still_written() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("logs").join(FILE_NAME);
        let mut file = log_file(&directory, 8);

        file.append("longer than eight bytes\n").unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "longer than eight bytes\n"
        );
    }

    #[test]
    fn drops_lines_while_the_directory_is_unusable_and_recovers() {
        let directory = tempfile::tempdir().unwrap();
        let blocker = directory.path().join("logs");
        let mut file = log_file(&directory, 100);

        // A file where the log directory should be.
        fs::write(&blocker, "").unwrap();
        assert!(file.append("lost\n").is_err());
        assert!(file.append("lost too\n").is_err());

        fs::remove_file(&blocker).unwrap();
        file.append("kept\n").unwrap();

        let contents = fs::read_to_string(blocker.join(FILE_NAME)).unwrap();
        assert_eq!(contents, "kept\n");
    }
}
