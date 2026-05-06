use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::subagent_report::SubagentReport;
use crate::subagent_spawner::{QueuedSubagentSpawner, RunId, SubagentDispatch, SubagentError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSubagentQueueConfig {
    pub root: PathBuf,
    pub dispatch_dir: String,
    pub report_dir: String,
    pub claim_dir: String,
    pub claim_release_dir: String,
}

impl FileSubagentQueueConfig {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            dispatch_dir: "dispatch".to_string(),
            report_dir: "reports".to_string(),
            claim_dir: "claims".to_string(),
            claim_release_dir: "claim-releases".to_string(),
        }
    }

    pub fn dispatch_path(&self, run_id: &RunId) -> PathBuf {
        self.root
            .join(&self.dispatch_dir)
            .join(format!("{}.json", safe_run_file_stem(run_id)))
    }

    pub fn report_path(&self, run_id: &RunId) -> PathBuf {
        self.root
            .join(&self.report_dir)
            .join(format!("{}.json", safe_run_file_stem(run_id)))
    }

    pub fn claim_path(&self, run_id: &RunId) -> PathBuf {
        self.root
            .join(&self.claim_dir)
            .join(format!("{}.json", safe_run_file_stem(run_id)))
    }

    pub fn claim_release_path(&self, run_id: &RunId) -> PathBuf {
        self.root
            .join(&self.claim_release_dir)
            .join(format!("{}.json", safe_run_file_stem(run_id)))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileSubagentQueueError {
    StorageUnavailable { path: PathBuf },
    Encode(String),
    Decode(String),
    InvalidRunId(String),
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
        ensure_dir(&config.root.join(&config.claim_dir))?;
        ensure_dir(&config.root.join(&config.claim_release_dir))?;
        Ok(Self { config })
    }

    pub fn write_dispatch(
        &self,
        dispatch: &SubagentDispatch,
    ) -> Result<PathBuf, FileSubagentQueueError> {
        validate_run_id(&dispatch.run_id)?;
        let path = self.config.dispatch_path(&dispatch.run_id);
        let payload = serde_json::to_string_pretty(dispatch)
            .map_err(|e| FileSubagentQueueError::Encode(e.to_string()))?;
        atomic_write(&path, &payload)?;
        Ok(path)
    }

    pub fn read_dispatch(
        &self,
        run_id: &RunId,
    ) -> Result<Option<SubagentDispatch>, FileSubagentQueueError> {
        validate_run_id(run_id)?;
        let path = self.config.dispatch_path(run_id);
        if !path.exists() {
            return Ok(None);
        }

        let payload = fs::read_to_string(&path)
            .map_err(|_| FileSubagentQueueError::StorageUnavailable { path: path.clone() })?;
        let dispatch = serde_json::from_str(&payload)
            .map_err(|e| FileSubagentQueueError::Decode(e.to_string()))?;
        Ok(Some(dispatch))
    }

    pub fn read_report(
        &self,
        run_id: &RunId,
    ) -> Result<Option<SubagentReport>, FileSubagentQueueError> {
        let Some(payload) = self.read_report_raw(run_id)? else {
            return Ok(None);
        };
        let report = serde_json::from_slice(&payload)
            .map_err(|e| FileSubagentQueueError::Decode(e.to_string()))?;
        Ok(Some(report))
    }

    pub fn read_report_raw(
        &self,
        run_id: &RunId,
    ) -> Result<Option<Vec<u8>>, FileSubagentQueueError> {
        validate_run_id(run_id)?;
        let path = self.config.report_path(run_id);
        if !path.exists() {
            return Ok(None);
        }

        let payload = fs::read(&path)
            .map_err(|_| FileSubagentQueueError::StorageUnavailable { path: path.clone() })?;
        Ok(Some(payload))
    }

    pub fn list_dispatches(&self) -> Result<Vec<SubagentDispatch>, FileSubagentQueueError> {
        let dir = self.config.root.join(&self.config.dispatch_dir);
        let mut dispatches: Vec<SubagentDispatch> = Vec::new();
        for path in list_json_files(&dir)? {
            let payload = fs::read_to_string(&path)
                .map_err(|_| FileSubagentQueueError::StorageUnavailable { path: path.clone() })?;
            let dispatch = serde_json::from_str(&payload)
                .map_err(|e| FileSubagentQueueError::Decode(e.to_string()))?;
            dispatches.push(dispatch);
        }
        dispatches.sort_by(|left, right| left.run_id.0.cmp(&right.run_id.0));
        Ok(dispatches)
    }

    pub fn list_report_run_ids(&self) -> Result<Vec<RunId>, FileSubagentQueueError> {
        let dir = self.config.root.join(&self.config.report_dir);
        list_run_ids(&dir)
    }

    pub fn list_claim_run_ids(&self) -> Result<Vec<RunId>, FileSubagentQueueError> {
        let dir = self.config.root.join(&self.config.claim_dir);
        list_run_ids(&dir)
    }

    pub fn list_claim_release_run_ids(&self) -> Result<Vec<RunId>, FileSubagentQueueError> {
        let dir = self.config.root.join(&self.config.claim_release_dir);
        list_run_ids(&dir)
    }

    pub fn write_report_for_test(
        &self,
        run_id: &RunId,
        report: &SubagentReport,
    ) -> Result<PathBuf, FileSubagentQueueError> {
        self.write_report(run_id, report)
    }

    pub fn write_report(
        &self,
        run_id: &RunId,
        report: &SubagentReport,
    ) -> Result<PathBuf, FileSubagentQueueError> {
        validate_run_id(run_id)?;
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
        validate_run_id(run_id)?;
        let Some(report) = self.read_report(run_id)? else {
            return Ok(false);
        };
        spawner
            .attach_report(run_id, report)
            .map_err(FileSubagentQueueError::Spawner)?;
        Ok(true)
    }

    pub fn claim_dispatch(
        &self,
        run_id: &RunId,
        owner: &str,
    ) -> Result<Option<PathBuf>, FileSubagentQueueError> {
        self.claim_dispatch_with_timeout(run_id, owner, None)
    }

    pub fn claim_dispatch_with_timeout(
        &self,
        run_id: &RunId,
        owner: &str,
        stale_after_ms: Option<u64>,
    ) -> Result<Option<PathBuf>, FileSubagentQueueError> {
        validate_run_id(run_id)?;
        let path = self.config.claim_path(run_id);
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        ensure_dir(parent)?;
        let payload = self.claim_payload(run_id, owner);
        if path.exists() {
            if self.is_claim_released(run_id)?
                || stale_after_ms
                    .map(|timeout_ms| self.is_claim_stale(run_id, timeout_ms))
                    .transpose()?
                    .unwrap_or(false)
            {
                atomic_write(&path, &payload)?;
                return Ok(Some(path));
            }
            return Ok(None);
        }

        let mut file = match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => return Ok(None),
            Err(_) => {
                return Err(FileSubagentQueueError::StorageUnavailable { path: path.clone() })
            }
        };
        file.write_all(payload.as_bytes())
            .map_err(|_| FileSubagentQueueError::StorageUnavailable { path: path.clone() })?;
        file.flush()
            .map_err(|_| FileSubagentQueueError::StorageUnavailable { path: path.clone() })?;
        Ok(Some(path))
    }

    pub fn release_claim(
        &self,
        run_id: &RunId,
        owner: &str,
        reason: &str,
    ) -> Result<PathBuf, FileSubagentQueueError> {
        validate_run_id(run_id)?;
        let path = self.config.claim_release_path(run_id);
        let payload = serde_json::json!({
            "run_id": run_id.0,
            "owner": owner,
            "reason": reason,
            "released_at_unix_seconds": current_unix_seconds(),
            "released_at_unix_nanos": current_unix_nanos(),
        })
        .to_string();
        atomic_write(&path, &payload)?;
        Ok(path)
    }

    pub fn is_claim_released(&self, run_id: &RunId) -> Result<bool, FileSubagentQueueError> {
        validate_run_id(run_id)?;
        let release_path = self.config.claim_release_path(run_id);
        if !release_path.exists() {
            return Ok(false);
        }
        let claim_path = self.config.claim_path(run_id);
        if !claim_path.exists() {
            return Ok(true);
        }

        if let Some(released) = read_json_u128_field(&release_path, "released_at_unix_nanos")? {
            if let Some(claimed) = read_json_u128_field(&claim_path, "claimed_at_unix_nanos")? {
                return Ok(released >= claimed);
            }
        }

        let release_modified = fs::metadata(&release_path)
            .map_err(|_| FileSubagentQueueError::StorageUnavailable {
                path: release_path.clone(),
            })?
            .modified()
            .map_err(|_| FileSubagentQueueError::StorageUnavailable {
                path: release_path.clone(),
            })?;
        let claim_modified = fs::metadata(&claim_path)
            .map_err(|_| FileSubagentQueueError::StorageUnavailable {
                path: claim_path.clone(),
            })?
            .modified()
            .map_err(|_| FileSubagentQueueError::StorageUnavailable {
                path: claim_path.clone(),
            })?;

        Ok(release_modified >= claim_modified)
    }

    fn claim_payload(&self, run_id: &RunId, owner: &str) -> String {
        serde_json::json!({
            "run_id": run_id.0,
            "owner": owner,
            "claimed_at_unix_seconds": current_unix_seconds(),
            "claimed_at_unix_nanos": current_unix_nanos(),
        })
        .to_string()
    }

    pub fn is_claimed(&self, run_id: &RunId) -> Result<bool, FileSubagentQueueError> {
        validate_run_id(run_id)?;
        Ok(self.config.claim_path(run_id).exists() && !self.is_claim_released(run_id)?)
    }

    pub fn is_claim_stale(
        &self,
        run_id: &RunId,
        stale_after_ms: u64,
    ) -> Result<bool, FileSubagentQueueError> {
        validate_run_id(run_id)?;
        let claim_path = self.config.claim_path(run_id);
        if !claim_path.exists() || self.is_claim_released(run_id)? {
            return Ok(false);
        }

        if let Some(claimed) = read_json_u128_field(&claim_path, "claimed_at_unix_nanos")? {
            let stale_after_nanos = u128::from(stale_after_ms) * 1_000_000;
            return Ok(current_unix_nanos().saturating_sub(claimed) >= stale_after_nanos);
        }

        let claim_modified = fs::metadata(&claim_path)
            .map_err(|_| FileSubagentQueueError::StorageUnavailable {
                path: claim_path.clone(),
            })?
            .modified()
            .map_err(|_| FileSubagentQueueError::StorageUnavailable {
                path: claim_path.clone(),
            })?;
        let age_ms = SystemTime::now()
            .duration_since(claim_modified)
            .map(|duration| duration.as_millis())
            .unwrap_or(0);
        Ok(age_ms >= u128::from(stale_after_ms))
    }

    pub fn claim_path(&self, run_id: &RunId) -> PathBuf {
        debug_assert!(validate_run_id(run_id).is_ok());
        self.config.claim_path(run_id)
    }

    pub fn claim_release_path(&self, run_id: &RunId) -> PathBuf {
        debug_assert!(validate_run_id(run_id).is_ok());
        self.config.claim_release_path(run_id)
    }

    pub fn report_path(&self, run_id: &RunId) -> PathBuf {
        debug_assert!(validate_run_id(run_id).is_ok());
        self.config.report_path(run_id)
    }

    pub fn dispatch_path(&self, run_id: &RunId) -> PathBuf {
        debug_assert!(validate_run_id(run_id).is_ok());
        self.config.dispatch_path(run_id)
    }

    pub fn queue_root(&self) -> &Path {
        &self.config.root
    }
}

fn list_run_ids(path: &Path) -> Result<Vec<RunId>, FileSubagentQueueError> {
    let mut run_ids = Vec::new();
    for path in list_json_files(path)? {
        let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        let run_id = RunId(stem.to_string());
        if validate_run_id(&run_id).is_ok() {
            run_ids.push(run_id);
        }
    }
    run_ids.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(run_ids)
}

fn current_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn current_unix_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}

fn validate_run_id(run_id: &RunId) -> Result<(), FileSubagentQueueError> {
    let value = run_id.0.as_str();
    if value.is_empty() || value.len() > 128 {
        return Err(FileSubagentQueueError::InvalidRunId(
            "run_id must be 1..=128 characters".to_string(),
        ));
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(FileSubagentQueueError::InvalidRunId(
            "run_id may only contain ASCII letters, digits, '-' and '_'".to_string(),
        ));
    }
    Ok(())
}

fn safe_run_file_stem(run_id: &RunId) -> &str {
    if validate_run_id(run_id).is_ok() {
        run_id.0.as_str()
    } else {
        "__invalid_run_id__"
    }
}

fn read_json_u128_field(path: &Path, field: &str) -> Result<Option<u128>, FileSubagentQueueError> {
    let payload =
        fs::read_to_string(path).map_err(|_| FileSubagentQueueError::StorageUnavailable {
            path: path.to_path_buf(),
        })?;
    let value: serde_json::Value = serde_json::from_str(&payload)
        .map_err(|e| FileSubagentQueueError::Decode(e.to_string()))?;
    Ok(value
        .get(field)
        .and_then(|value| value.as_u64())
        .map(u128::from))
}

fn ensure_dir(path: &Path) -> Result<(), FileSubagentQueueError> {
    fs::create_dir_all(path).map_err(|_| FileSubagentQueueError::StorageUnavailable {
        path: path.to_path_buf(),
    })
}

fn list_json_files(path: &Path) -> Result<Vec<PathBuf>, FileSubagentQueueError> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    let entries = fs::read_dir(path).map_err(|_| FileSubagentQueueError::StorageUnavailable {
        path: path.to_path_buf(),
    })?;
    for entry in entries {
        let entry = entry.map_err(|_| FileSubagentQueueError::StorageUnavailable {
            path: path.to_path_buf(),
        })?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) == Some("json") {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
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
