//! `responder::scripted` 模块。公开接口：struct ScriptedResponder；fn new, with_extra_meta。

use std::collections::BTreeMap;

use super::{
    ProviderAdapterResponder, ProviderAdapterResponse, ProviderIdentity, ResponderRequest,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptedResponder {
    model_name: String,
    scripted_output: String,
    extra_meta: BTreeMap<String, String>,
}

impl ScriptedResponder {
    pub fn new(model_name: impl Into<String>, scripted_output: impl Into<String>) -> Self {
        Self {
            model_name: model_name.into(),
            scripted_output: scripted_output.into(),
            extra_meta: BTreeMap::new(),
        }
    }

    pub fn with_extra_meta(mut self, extra_meta: BTreeMap<String, String>) -> Self {
        self.extra_meta = extra_meta;
        self
    }
}

impl ProviderAdapterResponder for ScriptedResponder {
    fn identity(&self) -> ProviderIdentity {
        ProviderIdentity {
            provider_id: "scripted-responder".to_string(),
            model_name: self.model_name.clone(),
        }
    }

    fn respond(&self, request: &ResponderRequest) -> ProviderAdapterResponse {
        ProviderAdapterResponse {
            body: self.scripted_output.clone(),
            trace: format!(
                "provider={} model={} scripted=true user_input=《{}》 recall_hits={}",
                self.identity().provider_id,
                self.model_name,
                request.user_input,
                request.recall_hit_count
            ),
            finish_reason: Some("scripted".to_string()),
            extra_meta: self.extra_meta.clone(),
        }
    }
}
