use std::fs;
use std::path::{Path, PathBuf};

use crate::subagent_report::SubagentReport;
use crate::subagent_spawner::{QueuedSubagentSpawner, RunId, SubagentDispatch, SubagentError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSubagentQueueConfig {
    pub root: PathBuf,
    pub dispatch_dir: String,
    pub report_dir: String,
}

impl FileSubagentQueueConfig {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            dispatch_dir: "dispatch".to_string(),
            report_dir: "reports".to_string(),
        }
    }

    pub fn dispatch_path(&self, run_id: &RunId) -> PathBuf {
        self.root
            .join(&self.dispatch_dir)
            .join(format!("{}.json", run_id.0))
    }

    pub fn report_path(&self, run_id: &RunId) -> PathBuf {
        self.root
            .join(&self.report_dir)
            .join(format!("{}.json", run_id.0))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileSubagentQueueError {
    StorageUnavailable { path: PathBuf },
    Encode(String),
    Decode(String),
    Spawner(SubagentError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSubagentQueue {
    config: FileSubagentQueueConfig,
}

impl FileSubagentQueue {
    pub fn open(config: FileSubagentQueueConfig) -> Result<Self, FileSubagentQueueError> {
        ensure_dir(&config.root.join(&config.dispatch_dir))?;
        ensure_dir(&config.root.join(&config.report_dir))?;
        Ok(Self { config })
    }

    pub fn write_dispatch(
        &self,
        dispatch: &SubagentDispatch,
    ) -> Result<PathBuf, FileSubagentQueueError> {
        let path = self.config.dispatch_path(&dispatch.run_id);
        let payload = serde_json::to_string_pretty(dispatch)
            .map_err(|e| FileSubagentQueueError::Encode(e.to_string()))?;
        atomic_write(&path, &payload)?;
        Ok(path)
    }

    pub fn read_report(
        &self,
        run_id: &RunId,
    ) -> Result<Option<SubagentReport>, FileSubagentQueueError> {
        let path = self.config.report_path(run_id);
        if !path.exists() {
            return Ok(None);
        }

        let payload = fs::read_to_string(&path)
            .map_err(|_| FileSubagentQueueError::StorageUnavailable { path: path.clone() })?;
        let report = serde_json::from_str(&payload)
            .map_err(|e| FileSubagentQueueError::Decode(e.to_string()))?;
        Ok(Some(report))
    }

    pub fn write_report_for_test(
        &self,
        run_id: &RunId,
        report: &SubagentReport,
    ) -> Result<PathBuf, FileSubagentQueueError> {
        let path = self.config.report_path(run_id);
        let payload = serde_json::to_string_pretty(report)
            .map_err(|e| FileSubagentQueueError::Encode(e.to_string()))?;
        atomic_write(&path, &payload)?;
        Ok(path)
    }

    pub fn flush_pending_dispatches(
        &self,
        spawner: &QueuedSubagentSpawner,
    ) -> Result<Vec<PathBuf>, FileSubagentQueueError> {
        spawner
            .pending_dispatches()
            .iter()
            .map(|dispatch| self.write_dispatch(dispatch))
            .collect()
    }

    pub fn attach_report_if_present(
        &self,
        spawner: &mut QueuedSubagentSpawner,
        run_id: &RunId,
    ) -> Result<bool, FileSubagentQueueError> {
        let Some(report) = self.read_report(run_id)? else {
            return Ok(false);
        };
        spawner
            .attach_report(run_id, report)
            .map_err(FileSubagentQueueError::Spawner)?;
        Ok(true)
    }
}

fn ensure_dir(path: &Path) -> Result<(), FileSubagentQueueError> {
    fs::create_dir_all(path).map_err(|_| FileSubagentQueueError::StorageUnavailable {
        path: path.to_path_buf(),
    })
}

fn atomic_write(path: &Path, content: &str) -> Result<(), FileSubagentQueueError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    ensure_dir(parent)?;
    let tmp_path = path.with_extension("tmp");
    fs::write(&tmp_path, content).map_err(|_| FileSubagentQueueError::StorageUnavailable {
        path: tmp_path.clone(),
    })?;
    fs::rename(&tmp_path, path).map_err(|_| FileSubagentQueueError::StorageUnavailable {
        path: path.to_path_buf(),
    })
}
