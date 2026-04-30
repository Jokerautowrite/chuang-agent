use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponderRequest {
    pub prompt: String,
    pub user_input: String,
    pub recall_hit_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderIdentity {
    pub provider_id: String,
    pub model_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponderProvider {
    pub provider_id: String,
    pub model_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponderMeta {
    pub provider: Option<String>,
    pub recall_hit_count: Option<usize>,
    pub finish_reason: Option<String>,
    pub extra: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponderOutput {
    pub model_name: String,
    pub body: String,
    pub trace: String,
    pub meta: ResponderMeta,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderAdapterResponse {
    pub body: String,
    pub trace: String,
    pub finish_reason: Option<String>,
    pub extra_meta: BTreeMap<String, String>,
}

pub trait Responder {
    fn generate(&self, request: &ResponderRequest) -> ResponderOutput;
    fn provider(&self) -> ResponderProvider;
}

pub trait ProviderAdapterResponder {
    fn identity(&self) -> ProviderIdentity;
    fn respond(&self, request: &ResponderRequest) -> ProviderAdapterResponse;
}

impl<T: ProviderAdapterResponder> Responder for T {
    fn generate(&self, request: &ResponderRequest) -> ResponderOutput {
        let identity = self.identity();
        let response = self.respond(request);

        ResponderOutput {
            model_name: identity.model_name.clone(),
            body: response.body,
            trace: response.trace,
            meta: ResponderMeta {
                provider: Some(identity.provider_id),
                recall_hit_count: Some(request.recall_hit_count),
                finish_reason: response.finish_reason,
                extra: response.extra_meta,
            },
        }
    }

    fn provider(&self) -> ResponderProvider {
        let identity = self.identity();
        ResponderProvider {
            provider_id: identity.provider_id,
            model_name: identity.model_name,
        }
    }
}

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
