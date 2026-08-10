use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::RuntimeEventKind;

/// Configuration for repeated-failure detection.
///
/// A `FailurePattern` is emitted only when the same failure signature is seen
/// at least `min_repeats` times inside the observation window. The window is
/// the most recent `window` events when set; `None` means the whole observed
/// stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FailureDetectorConfig {
    pub min_repeats: usize,
    pub window: Option<usize>,
    pub failure_kinds: Vec<RuntimeEventKind>,
}

impl Default for FailureDetectorConfig {
    fn default() -> Self {
        Self {
            min_repeats: 2,
            window: None,
            failure_kinds: vec![RuntimeEventKind::ToolFailed],
        }
    }
}

impl FailureDetectorConfig {
    pub fn min_repeats(mut self, min_repeats: usize) -> Self {
        self.min_repeats = min_repeats.max(1);
        self
    }

    pub fn window(mut self, window: usize) -> Self {
        self.window = Some(window.max(1));
        self
    }

    pub fn failure_kinds(mut self, kinds: Vec<RuntimeEventKind>) -> Self {
        self.failure_kinds = kinds;
        self
    }
}

/// A detected repeated-failure pattern with full, auditable evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FailurePattern {
    pub signature: String,
    pub kind: RuntimeEventKind,
    pub count: usize,
    pub window_size: usize,
    pub event_ids: Vec<String>,
    pub task_ids: Vec<String>,
    pub first_seen_event_id: String,
    pub last_seen_event_id: String,
    pub summary: String,
}

/// Deterministic, pure repeated-failure detector over observed runtime events.
///
/// This is the first stage of the evolver outer loop: it converts raw
/// observations into structured failure evidence that a rule-change proposal
/// can cite. It never writes anything itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepeatedFailureDetector {
    pub config: FailureDetectorConfig,
}

impl RepeatedFailureDetector {
    pub fn new(config: FailureDetectorConfig) -> Self {
        Self { config }
    }

    pub fn default_config() -> FailureDetectorConfig {
        FailureDetectorConfig::default()
    }

    /// Stable failure signature for an event, taken from structured metadata
    /// when available. Events without any usable signature are not
    /// classifiable and are skipped by detection.
    pub fn failure_signature(event: &super::RuntimeEvent) -> Option<String> {
        for key in [
            "failure_signature",
            "tool",
            "error_code",
            "error",
            "task_kind",
        ] {
            if let Some(value) = event.metadata.get(key) {
                if !value.trim().is_empty() {
                    return Some(format!("{key}={value}"));
                }
            }
        }
        None
    }

    pub fn detect(&self, events: &[super::RuntimeEvent]) -> Vec<FailurePattern> {
        let window_start = self
            .config
            .window
            .map(|window| events.len().saturating_sub(window))
            .unwrap_or(0);
        let windowed = &events[window_start..];

        let mut groups: BTreeMap<String, Vec<&super::RuntimeEvent>> = BTreeMap::new();
        for event in windowed {
            if !self.config.failure_kinds.contains(&event.kind) {
                continue;
            }
            if let Some(signature) = Self::failure_signature(event) {
                groups.entry(signature).or_default().push(event);
            }
        }

        let mut patterns = Vec::new();
        for (signature, group) in groups {
            if group.len() < self.config.min_repeats {
                continue;
            }
            let mut task_ids = Vec::new();
            for event in &group {
                if !task_ids.contains(&event.task_id) {
                    task_ids.push(event.task_id.clone());
                }
            }
            let event_ids = group
                .iter()
                .map(|event| event.event_id.clone())
                .collect::<Vec<_>>();
            patterns.push(FailurePattern {
                signature: signature.clone(),
                kind: group[0].kind.clone(),
                count: group.len(),
                window_size: windowed.len(),
                event_ids: event_ids.clone(),
                task_ids: task_ids.clone(),
                first_seen_event_id: event_ids[0].clone(),
                last_seen_event_id: event_ids[event_ids.len() - 1].clone(),
                summary: format!(
                    "repeated failure {} observed {} times across {} task(s) in a window of {} event(s)",
                    signature,
                    group.len(),
                    task_ids.len(),
                    windowed.len()
                ),
            });
        }
        patterns
    }
}
