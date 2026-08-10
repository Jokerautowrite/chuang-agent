//! `responder` 模块。公开接口：trait Responder, ProviderAdapterResponder；struct ResponderRequest, ProviderIdentity, ResponderProvider, ResponderMeta, ResponderOutput, ProviderAdapterResponse；use fake, scripted。

use std::collections::BTreeMap;

mod fake;
mod scripted;

pub use fake::FakeResponder;
pub use scripted::ScriptedResponder;

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
