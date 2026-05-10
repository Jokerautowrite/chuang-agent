use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::permission_profile_slot::PermissionProfileId;
use crate::runtime_config::ConfigSummary;
use crate::tool_registry_slot::ToolDescriptor;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnContextSnapshot {
    pub thread_id: String,
    pub turn_id: String,
    pub workspace_root: String,
    pub provider: ProviderModelSummary,
    pub permission_profile_id: PermissionProfileId,
    pub tools: Vec<ToolDescriptorSummary>,
    pub memory_segment_ids: Vec<String>,
    pub recent_history_segment_ids: Vec<String>,
    pub env_vars: Vec<EnvVarStateSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderModelSummary {
    pub provider_id: String,
    pub model_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolDescriptorSummary {
    pub name: String,
    pub namespace: String,
    pub read_only: bool,
    pub mutating: bool,
    pub destructive: bool,
    pub requires_approval: bool,
    pub descriptor_signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvVarStateSnapshot {
    pub name: String,
    pub value_state: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnContextSnapshotInput {
    pub thread_id: String,
    pub turn_id: String,
    pub workspace_root: PathBuf,
    pub provider_id: String,
    pub model_name: String,
    pub permission_profile_id: PermissionProfileId,
    pub tools: Vec<ToolDescriptor>,
    pub memory_segment_ids: Vec<String>,
    pub recent_history_segment_ids: Vec<String>,
    pub env_pairs: Vec<(String, Option<String>)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeTurnContextInput {
    pub thread_id: String,
    pub turn_id: String,
    pub workspace_root: PathBuf,
    pub config_summary: ConfigSummary,
    pub permission_profile_id: PermissionProfileId,
    pub tools: Vec<ToolDescriptor>,
    pub memory_segment_ids: Vec<String>,
    pub recent_history_segment_ids: Vec<String>,
    pub env_pairs: Vec<(String, Option<String>)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TurnContextSnapshotError {
    MissingRequiredField { field: &'static str },
}

impl TurnContextSnapshot {
    pub fn from_fake_input(
        input: TurnContextSnapshotInput,
    ) -> Result<Self, TurnContextSnapshotError> {
        require_non_empty("thread_id", &input.thread_id)?;
        require_non_empty("turn_id", &input.turn_id)?;
        require_non_empty_path("workspace_root", &input.workspace_root)?;
        require_non_empty("provider_id", &input.provider_id)?;
        require_non_empty("model_name", &input.model_name)?;

        let mut tools = input
            .tools
            .into_iter()
            .map(|descriptor| ToolDescriptorSummary {
                name: descriptor.name.to_string(),
                namespace: descriptor.namespace.to_string(),
                read_only: descriptor.read_only,
                mutating: descriptor.mutating,
                destructive: descriptor.destructive,
                requires_approval: descriptor.requires_approval,
                descriptor_signature: format!(
                    "{}:{}:{}:{}:{}",
                    descriptor.name,
                    descriptor.namespace,
                    descriptor.schema_fields.join(","),
                    descriptor.risk_tags.join(","),
                    descriptor.requires_approval
                ),
            })
            .collect::<Vec<_>>();
        tools.sort_by(|a, b| a.name.cmp(&b.name).then(a.namespace.cmp(&b.namespace)));

        let mut env_vars = input
            .env_pairs
            .into_iter()
            .map(|(name, value)| EnvVarStateSnapshot {
                name: name.clone(),
                value_state: classify_env_value_state(&name, value.as_deref()).to_string(),
            })
            .collect::<Vec<_>>();
        env_vars.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(Self {
            thread_id: input.thread_id,
            turn_id: input.turn_id,
            workspace_root: input.workspace_root.to_string_lossy().to_string(),
            provider: ProviderModelSummary {
                provider_id: input.provider_id,
                model_name: input.model_name,
            },
            permission_profile_id: input.permission_profile_id,
            tools,
            memory_segment_ids: input.memory_segment_ids,
            recent_history_segment_ids: input.recent_history_segment_ids,
            env_vars,
        })
    }

    pub fn from_runtime_config_summary(
        input: RuntimeTurnContextInput,
    ) -> Result<Self, TurnContextSnapshotError> {
        let snapshot_input = TurnContextSnapshotInput::from_runtime_config_summary(input)?;
        Self::from_fake_input(snapshot_input)
    }
}

impl TurnContextSnapshotInput {
    pub fn from_runtime_config_summary(
        input: RuntimeTurnContextInput,
    ) -> Result<Self, TurnContextSnapshotError> {
        require_non_empty("thread_id", &input.thread_id)?;
        require_non_empty("turn_id", &input.turn_id)?;
        require_non_empty_path("workspace_root", &input.workspace_root)?;
        require_non_empty("provider_id", &input.config_summary.provider_id)?;
        require_non_empty("model_name", &input.config_summary.model_name)?;

        Ok(Self {
            thread_id: input.thread_id,
            turn_id: input.turn_id,
            workspace_root: input.workspace_root,
            provider_id: input.config_summary.provider_id,
            model_name: input.config_summary.model_name,
            permission_profile_id: input.permission_profile_id,
            tools: input.tools,
            memory_segment_ids: input.memory_segment_ids,
            recent_history_segment_ids: input.recent_history_segment_ids,
            env_pairs: input.env_pairs,
        })
    }
}

fn require_non_empty(field: &'static str, value: &str) -> Result<(), TurnContextSnapshotError> {
    if value.trim().is_empty() {
        return Err(TurnContextSnapshotError::MissingRequiredField { field });
    }
    Ok(())
}

fn require_non_empty_path(
    field: &'static str,
    value: &Path,
) -> Result<(), TurnContextSnapshotError> {
    if value.as_os_str().is_empty() {
        return Err(TurnContextSnapshotError::MissingRequiredField { field });
    }
    Ok(())
}

fn classify_env_value_state(key: &str, value: Option<&str>) -> &'static str {
    match value {
        None => "<missing>",
        Some("") => "<missing>",
        Some(raw) if is_secret_like_env(key, raw) => "<redacted>",
        Some(_) => "<set>",
    }
}

fn is_secret_like_env(key: &str, value: &str) -> bool {
    let key = key.to_ascii_lowercase();
    if key.contains("secret")
        || key.contains("token")
        || key.contains("password")
        || key.contains("passwd")
        || key.contains("api_key")
        || key.contains("apikey")
        || key.contains("private_key")
    {
        return true;
    }

    let value = value.to_ascii_lowercase();
    value.starts_with("sk-")
        || value.contains("bearer ")
        || value.contains("xoxb-")
        || value.contains("ghp_")
}
