use std::collections::BTreeMap;

use super::{
    ProviderAdapterResponder, ProviderAdapterResponse, ProviderIdentity, ResponderRequest,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FakeResponder {
    model_name: String,
}

impl FakeResponder {
    pub fn new(model_name: impl Into<String>) -> Self {
        Self {
            model_name: model_name.into(),
        }
    }
}

impl ProviderAdapterResponder for FakeResponder {
    fn identity(&self) -> ProviderIdentity {
        ProviderIdentity {
            provider_id: "fake-responder".to_string(),
            model_name: self.model_name.clone(),
        }
    }

    fn respond(&self, request: &ResponderRequest) -> ProviderAdapterResponse {
        ProviderAdapterResponse {
            body: format!(
                "fake-responder[{}]: user_input=《{}》 recall_hits={}",
                self.model_name, request.user_input, request.recall_hit_count
            ),
            trace: format!(
                "provider={} model={} user_input=《{}》 recall_hits={}",
                self.identity().provider_id,
                self.model_name,
                request.user_input,
                request.recall_hit_count
            ),
            finish_reason: Some("stubbed".to_string()),
            extra_meta: BTreeMap::new(),
        }
    }
}
