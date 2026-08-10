//! `browser_worker::transcript` 模块。公开接口：struct BrowserTranscriptEntry, BrowserTranscriptRecord, BrowserTranscript；fn new, start_record, complete_record。

use crate::browser_worker::{DispatchReceipt, ProviderKind, WorkerOutput, WorkerTask};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserTranscriptEntry {
    pub role: String,
    pub content: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserTranscriptRecord {
    pub task_id: String,
    pub worker_id: String,
    pub provider: ProviderKind,
    pub prompt: String,
    pub output: Option<String>,
    pub raw_snapshot_ref: Option<String>,
    pub entries: Vec<BrowserTranscriptEntry>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct BrowserTranscript;

impl BrowserTranscript {
    pub fn new() -> Self {
        Self
    }

    pub fn start_record(
        &self,
        task: &WorkerTask,
        receipt: &DispatchReceipt,
    ) -> BrowserTranscriptRecord {
        BrowserTranscriptRecord {
            task_id: task.task_id.clone(),
            worker_id: receipt.worker_id.clone(),
            provider: receipt.provider.clone(),
            prompt: task.prompt.clone(),
            output: None,
            raw_snapshot_ref: None,
            entries: vec![BrowserTranscriptEntry {
                role: "user".to_string(),
                content: task.prompt.clone(),
                timestamp: receipt.submitted_at.clone(),
            }],
        }
    }

    pub fn complete_record(
        &self,
        mut record: BrowserTranscriptRecord,
        output: &WorkerOutput,
    ) -> BrowserTranscriptRecord {
        record.output = Some(output.content.clone());
        record.raw_snapshot_ref = output.raw_snapshot_ref.clone();
        record.entries.push(BrowserTranscriptEntry {
            role: "assistant".to_string(),
            content: output.content.clone(),
            timestamp: output.completed_at.clone(),
        });
        record
    }
}

#[cfg(test)]
mod tests {
    use super::{BrowserTranscript, BrowserTranscriptEntry};
    use crate::browser_worker::{
        BrowserMode, DispatchReceipt, DispatchStatus, ProviderKind, WorkerFinishReason,
        WorkerOutput, WorkerTask,
    };

    #[test]
    fn starts_record_from_task_and_receipt() {
        let transcript = BrowserTranscript::new();
        let task = WorkerTask {
            task_id: "task-1".into(),
            title: "title".into(),
            prompt: "Summarize this page".into(),
        };
        let receipt = DispatchReceipt {
            task_id: task.task_id.clone(),
            worker_id: "worker-1".into(),
            provider: ProviderKind::DeepSeekWeb,
            submitted_at: "2026-04-30T15:14:00Z".into(),
            prompt_hash: "hash".into(),
            mode: BrowserMode::Fast,
            status: DispatchStatus::Submitted,
        };

        let record = transcript.start_record(&task, &receipt);

        assert_eq!(record.task_id, "task-1");
        assert_eq!(record.worker_id, "worker-1");
        assert_eq!(record.provider, ProviderKind::DeepSeekWeb);
        assert_eq!(record.prompt, "Summarize this page");
        assert_eq!(record.output, None);
        assert_eq!(record.raw_snapshot_ref, None);
        assert_eq!(
            record.entries,
            vec![BrowserTranscriptEntry {
                role: "user".into(),
                content: "Summarize this page".into(),
                timestamp: "2026-04-30T15:14:00Z".into(),
            }]
        );
    }

    #[test]
    fn appends_output_entry_and_sets_output() {
        let transcript = BrowserTranscript::new();
        let task = WorkerTask {
            task_id: "task-1".into(),
            title: "title".into(),
            prompt: "Summarize this page".into(),
        };
        let receipt = DispatchReceipt {
            task_id: task.task_id.clone(),
            worker_id: "worker-1".into(),
            provider: ProviderKind::DeepSeekWeb,
            submitted_at: "2026-04-30T15:14:00Z".into(),
            prompt_hash: "hash".into(),
            mode: BrowserMode::Fast,
            status: DispatchStatus::Submitted,
        };
        let output = WorkerOutput {
            worker_id: "worker-1".into(),
            provider: ProviderKind::DeepSeekWeb,
            task_id: "task-1".into(),
            content: "Done".into(),
            raw_snapshot_ref: Some("opencli://deepseek/task-1".into()),
            completed_at: "2026-04-30T15:15:00Z".into(),
            finish_reason: WorkerFinishReason::Completed,
        };

        let record = transcript.complete_record(transcript.start_record(&task, &receipt), &output);

        assert_eq!(record.output, Some("Done".into()));
        assert_eq!(
            record.raw_snapshot_ref,
            Some("opencli://deepseek/task-1".into())
        );
        assert_eq!(record.entries.len(), 2);
        assert_eq!(
            record.entries[1],
            BrowserTranscriptEntry {
                role: "assistant".into(),
                content: "Done".into(),
                timestamp: "2026-04-30T15:15:00Z".into(),
            }
        );
    }
}
