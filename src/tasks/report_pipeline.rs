use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use crate::track::TrackResult;
use once_cell::sync::Lazy;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static RECOVERY_WINNERS: Lazy<Mutex<std::collections::HashSet<(PathBuf, String)>>> =
    Lazy::new(|| Mutex::new(std::collections::HashSet::new()));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Publication {
    Created,
    AlreadyExists,
}

#[derive(Debug, Clone)]
pub struct ReportPipeline {
    out_dir: PathBuf,
    filename: String,
}

impl ReportPipeline {
    pub fn new(out_dir: &Path, filename: impl Into<String>) -> Self {
        Self {
            out_dir: out_dir.to_path_buf(),
            filename: filename.into(),
        }
    }

    pub fn json_path(&self) -> PathBuf {
        self.out_dir.join(&self.filename).with_extension("json")
    }

    pub fn acmi_path(&self) -> PathBuf {
        self.out_dir.join(&self.filename).with_extension("zip.acmi")
    }

    pub fn claim_recovery(&self, recovery_id: &str) -> Option<RecoveryClaim> {
        let key = (self.out_dir.clone(), recovery_id.to_string());
        let mut winners = RECOVERY_WINNERS.lock().ok()?;
        if !winners.insert(key.clone()) {
            return None;
        }
        Some(RecoveryClaim {
            key: Some(key),
            committed: false,
        })
    }

    pub async fn publish_json(&self, bytes: &[u8]) -> std::io::Result<Publication> {
        publish_bytes(self.json_path(), bytes.to_vec()).await
    }

    pub async fn publish_acmi(&self, bytes: &[u8]) -> std::io::Result<Publication> {
        publish_bytes(self.acmi_path(), bytes.to_vec()).await
    }

    pub fn render_and_publish(
        &self,
        track: &TrackResult,
    ) -> Result<(PathBuf, PathBuf), crate::error::Error> {
        let temporary = TemporaryDirectory::create(&self.out_dir)
            .map_err(|source| crate::error::Error::file_at(&self.out_dir, source))?;
        let chart = crate::draw::draw_chart(temporary.path(), &self.filename, track)?;
        let pattern = crate::draw::draw_pattern_chart(temporary.path(), &self.filename, track)?;
        let chart_bytes =
            std::fs::read(&chart).map_err(|source| crate::error::Error::file_at(&chart, source))?;
        let pattern_bytes = std::fs::read(&pattern)
            .map_err(|source| crate::error::Error::file_at(&pattern, source))?;
        let chart_path = self.out_dir.join(&self.filename).with_extension("png");
        let pattern_path = self
            .out_dir
            .join(format!("{}-pattern", self.filename))
            .with_extension("png");
        let chart_publication = atomic_create_if_absent(&chart_path, &chart_bytes)
            .map_err(|source| crate::error::Error::file_at(&chart_path, source))?;
        ensure_created(&chart_path, chart_publication)
            .map_err(|source| crate::error::Error::file_at(&chart_path, source))?;
        let pattern_publication = atomic_create_if_absent(&pattern_path, &pattern_bytes)
            .map_err(|source| crate::error::Error::file_at(&pattern_path, source))?;
        ensure_created(&pattern_path, pattern_publication)
            .map_err(|source| crate::error::Error::file_at(&pattern_path, source))?;
        Ok((chart_path, pattern_path))
    }
}

pub struct RecoveryClaim {
    key: Option<(PathBuf, String)>,
    committed: bool,
}

impl RecoveryClaim {
    pub fn commit(&mut self) {
        self.committed = true;
    }
}

impl Drop for RecoveryClaim {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        if let (Some(key), Ok(mut winners)) = (self.key.take(), RECOVERY_WINNERS.lock()) {
            winners.remove(&key);
        }
    }
}

fn ensure_created(path: &Path, publication: Publication) -> std::io::Result<()> {
    match publication {
        Publication::Created => Ok(()),
        Publication::AlreadyExists => Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!(
                "refusing to replace previously published artifact `{}`",
                path.display()
            ),
        )),
    }
}

async fn publish_bytes(path: PathBuf, bytes: Vec<u8>) -> std::io::Result<Publication> {
    tokio::task::spawn_blocking(move || atomic_create_if_absent(&path, &bytes))
        .await
        .map_err(std::io::Error::other)?
}

fn atomic_create_if_absent(path: &Path, bytes: &[u8]) -> std::io::Result<Publication> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("output");
    let temporary = (0..100)
        .find_map(|_| {
            let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let candidate =
                path.with_extension(format!("{extension}.tmp-{}-{sequence}", std::process::id()));
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&candidate)
            {
                Ok(file) => Some(Ok(TemporaryFile {
                    path: candidate,
                    file: Some(file),
                })),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => None,
                Err(error) => Some(Err(error)),
            }
        })
        .transpose()?
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!(
                    "unable to allocate temporary output in `{}`",
                    parent.display()
                ),
            )
        })?;
    let mut temporary = temporary;
    let file = temporary.file.as_mut().expect("temporary file is open");
    file.write_all(bytes)?;
    file.flush()?;
    file.sync_all()?;
    temporary.file.take();

    match std::fs::hard_link(&temporary.path, path) {
        Ok(()) => {
            crate::metrics::RUNTIME_METRICS.add_io_bytes(bytes.len());
            Ok(Publication::Created)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            Ok(Publication::AlreadyExists)
        }
        Err(error) => Err(error),
    }
}

struct TemporaryFile {
    path: PathBuf,
    file: Option<std::fs::File>,
}

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        self.file.take();
        let _ = std::fs::remove_file(&self.path);
    }
}

struct TemporaryDirectory(PathBuf);

impl TemporaryDirectory {
    fn create(parent: &Path) -> std::io::Result<Self> {
        for _ in 0..100 {
            let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = parent.join(format!(".lso-render-{}-{sequence}", std::process::id()));
            match std::fs::create_dir(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "unable to allocate render directory",
        ))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn concurrent_publish_identifies_the_winner_and_never_replaces_it() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("lso-report-pipeline-{unique}"));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("winner.json");
        let (first, second) = tokio::join!(
            publish_bytes(path.clone(), b"producer-a".to_vec()),
            publish_bytes(path.clone(), b"producer-b".to_vec())
        );
        let first = first.unwrap();
        let second = second.unwrap();
        assert_ne!(first, second);
        let expected = if first == Publication::Created {
            b"producer-a".as_slice()
        } else {
            b"producer-b".as_slice()
        };
        assert_eq!(std::fs::read(&path).unwrap(), expected);
        assert_eq!(
            publish_bytes(path.clone(), b"late-producer".to_vec())
                .await
                .unwrap(),
            Publication::AlreadyExists
        );
        assert_eq!(std::fs::read(&path).unwrap(), expected);
        assert!(std::fs::read_dir(&dir).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains(".tmp-")));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn recovery_id_claim_is_independent_of_display_filename() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("lso-claim-{unique}"));
        let first = ReportPipeline::new(&dir, "first-name");
        let second = ReportPipeline::new(&dir, "other-name");
        let mut winner = first.claim_recovery("same-recovery-id").unwrap();
        assert!(second.claim_recovery("same-recovery-id").is_none());
        winner.commit();
        drop(winner);
        assert!(second.claim_recovery("same-recovery-id").is_none());
    }

    #[tokio::test]
    async fn concurrent_same_recovery_id_publishes_only_the_claim_winner() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("lso-recovery-winner-{unique}"));
        std::fs::create_dir(&dir).unwrap();
        let first = ReportPipeline::new(&dir, "producer-a");
        let second = ReportPipeline::new(&dir, "producer-b");
        let produce = |pipeline: ReportPipeline, payload: &'static [u8]| async move {
            let mut claim = pipeline.claim_recovery("shared-id")?;
            tokio::task::yield_now().await;
            assert_eq!(
                pipeline.publish_json(payload).await.unwrap(),
                Publication::Created
            );
            claim.commit();
            Some((pipeline.json_path(), payload))
        };
        let (first_result, second_result) = tokio::join!(
            produce(first, b"producer-a"),
            produce(second, b"producer-b")
        );

        assert_ne!(first_result.is_some(), second_result.is_some());
        let winner = first_result
            .as_ref()
            .or(second_result.as_ref())
            .expect("one winner");
        assert_eq!(std::fs::read(&winner.0).unwrap(), winner.1);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn failed_publish_leaves_no_target_or_temporary_file() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("lso-report-missing-{unique}"));
        let path = dir.join("missing").join("report.json");
        assert!(publish_bytes(path.clone(), b"payload".to_vec())
            .await
            .is_err());
        assert!(!path.exists());
        assert!(!dir.exists());
    }
}
