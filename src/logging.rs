//! Rolling log files for `lso --log-dir <dir>`: one UTF-8 file per UTC day
//! (`lso.YYYY-MM-DD.log`) written beside the console output, the oldest files deleted once more
//! than `--log-max-files` exist, and every write handed to a background thread so that a slow or
//! full disk never sits on the 10-20 Hz telemetry path. Replaces the launch script's
//! `Tee-Object -Append`, which had no size or age limit and wrote UTF-16 (1.39 GB for 7 September
//! to 25 October 2026 on the dedicated server).

use std::path::{Path, PathBuf};

use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};
use tracing_appender::rolling::{InitError, RollingFileAppender, Rotation};

/// File name is `<prefix>.<UTC date>.<suffix>`, e.g. `lso.2026-10-26.log`.
pub const LOG_FILE_PREFIX: &str = "lso";
pub const LOG_FILE_SUFFIX: &str = "log";
/// Default for `--log-max-files`: two weeks of daily files.
pub const DEFAULT_MAX_LOG_FILES: usize = 14;

#[derive(Debug, thiserror::Error)]
pub enum LogDirError {
    #[error("cannot create log directory {}: {source}", path.display())]
    Create {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot open a rolling log file in {}: {source}", path.display())]
    Open {
        path: PathBuf,
        #[source]
        source: InitError,
    },
    #[error("--log-max-files must be at least 1")]
    ZeroFiles,
}

/// Opens the rolling writer for `dir`, creating the directory if needed. The returned guard must
/// live until the process is about to exit: dropping it flushes the lines still queued in the
/// background thread. The queue is lossy (tracing-appender's default, 128 000 lines): if the disk
/// cannot keep up, lines are dropped and a count of dropped lines is written when it recovers,
/// rather than stalling the recorder. Rotation happens on the first write after midnight UTC.
pub fn rolling_writer(
    dir: &Path,
    max_files: usize,
) -> Result<(NonBlocking, WorkerGuard), LogDirError> {
    if max_files == 0 {
        return Err(LogDirError::ZeroFiles);
    }
    std::fs::create_dir_all(dir).map_err(|source| LogDirError::Create {
        path: dir.to_path_buf(),
        source,
    })?;
    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(LOG_FILE_PREFIX)
        .filename_suffix(LOG_FILE_SUFFIX)
        .max_log_files(max_files)
        .build(dir)
        .map_err(|source| LogDirError::Open {
            path: dir.to_path_buf(),
            source,
        })?;
    Ok(tracing_appender::non_blocking(appender))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::layer::SubscriberExt;

    fn scratch_dir(label: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "lso-logging-{label}-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn log_files(dir: &Path) -> Vec<String> {
        let mut names = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with("lso.") && name.ends_with(".log"))
            .collect::<Vec<_>>();
        names.sort();
        names
    }

    #[test]
    fn refuses_zero_files_and_a_directory_it_cannot_create() {
        let dir = scratch_dir("refuse");
        assert!(matches!(
            rolling_writer(&dir, 0),
            Err(LogDirError::ZeroFiles)
        ));
        // A regular file where the directory should be: `create_dir_all` fails.
        let blocker = dir.join("blocker");
        std::fs::write(&blocker, b"not a directory").unwrap();
        let err = rolling_writer(&blocker.join("logs"), 1).err().unwrap();
        assert!(matches!(err, LogDirError::Create { .. }), "{err}");
        assert!(err.to_string().contains("cannot create log directory"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn writes_one_utf8_daily_file_and_prunes_the_oldest() {
        let dir = scratch_dir("write");
        // Three stale daily files from a previous deployment; with `max_files = 2` the appender
        // keeps today's file plus one of them.
        for date in ["2020-01-01", "2020-01-02", "2020-01-03"] {
            std::fs::write(dir.join(format!("lso.{date}.log")), b"old\n").unwrap();
        }
        let (writer, guard) = rolling_writer(&dir, 2).unwrap();
        let subscriber = tracing_subscriber::registry().with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(writer),
        );
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(marker = "rotation-smoke", "rolling log file line");
        });
        // Dropping the guard flushes the background writer.
        drop(guard);

        let files = log_files(&dir);
        assert!(files.len() <= 2, "pruning kept {files:?}");
        let today = files
            .iter()
            .find(|name| !name.starts_with("lso.2020-"))
            .unwrap_or_else(|| panic!("no file for today in {files:?}"));
        assert!(
            today.len() == "lso.YYYY-MM-DD.log".len(),
            "unexpected file name {today}"
        );
        let bytes = std::fs::read(dir.join(today)).unwrap();
        // UTF-8 without a byte-order mark, unlike the former `Tee-Object` UTF-16 file.
        assert!(!bytes.starts_with(&[0xFF, 0xFE]) && !bytes.starts_with(&[0xEF, 0xBB, 0xBF]));
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("rolling log file line"), "{text}");
        assert!(text.contains("rotation-smoke"), "{text}");
        assert!(!text.contains("\u{1b}["), "ANSI escape in file: {text}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
