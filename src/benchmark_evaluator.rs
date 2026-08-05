//! Model-backed automatic evaluator for Chuang capability benchmarks.
//!
//! Phase A evaluator: reads the private rubric (0600), asks a provider model
//! to score the Target agent's answer against it, and parses the structured
//! JSON score. The Target never sees the rubric; the Evaluator does.

use serde::{Deserialize, Serialize};

use crate::benchmark::{BenchmarkCase, BenchmarkStore, CaseScore};
use crate::provider_openai_compatible::OpenAICompatibleProviderAdapter;
use crate::responder::{ProviderAdapterResponder, ResponderRequest};
use crate::runtime_config::{OpenAICompatibleConfig, ProviderConfig, RuntimeConfig};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseAnswer {
    pub case_id: String,
    pub answer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluateRequest {
    pub benchmark_id: String,
    pub answers: Vec<CaseAnswer>,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RawCaseEvaluation {
    pub case_id: String,
    pub prompt: String,
    pub model_output: String,
    pub parsed: bool,
    pub parse_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvaluateReceipt {
    pub benchmark_id: String,
    pub provider_id: String,
    pub model_name: String,
    pub evaluated_case_count: usize,
    pub case_scores: Vec<CaseScore>,
    pub raw: Vec<RawCaseEvaluation>,
    pub dry_run: bool,
}

pub struct BenchmarkEvaluator {
    store: BenchmarkStore,
    provider: OpenAICompatibleConfig,
}

impl BenchmarkEvaluator {
    pub fn new(store: BenchmarkStore, provider: OpenAICompatibleConfig) -> Self {
        Self { store, provider }
    }

    /// Build an evaluator from the active runtime config. Fails when the
    /// configured provider is not a real OpenAI-compatible adapter.
    pub fn from_runtime_config(
        store: BenchmarkStore,
        runtime: &RuntimeConfig,
    ) -> Result<Self, String> {
        match &runtime.provider {
            ProviderConfig::OpenAICompatible(config) => Ok(Self::new(store, config.clone())),
            ProviderConfig::Fake { provider_id, .. } => Err(format!(
                "provider={provider_id} is a fake test responder; configure openai_compatible for real evaluation"
            )),
            ProviderConfig::Fallback { primary, .. } => match primary.as_ref() {
                ProviderConfig::OpenAICompatible(config) => Ok(Self::new(store, config.clone())),
                other => Err(format!(
                    "provider fallback primary is {:?}; openai_compatible required for evaluation",
                    other.kind_name()
                )),
            },
        }
    }

    pub fn evaluate(&self, request: &EvaluateRequest) -> Result<EvaluateReceipt, String> {
        let def = self.store
            .load_def(&request.benchmark_id)
            .map_err(|e| e.to_string())?;

        let mut answers = std::collections::BTreeMap::new();
        for answer in &request.answers {
            if answers.insert(answer.case_id.clone(), answer.answer.clone()).is_some() {
                return Err(format!("duplicate answer for case {}", answer.case_id));
            }
        }

        let adapter = OpenAICompatibleProviderAdapter::new(
            &self.provider.provider_id,
            &self.provider.base_url,
            &self.provider.api_key,
            &self.provider.model_name,
        )
        .with_transport(self.provider.transport.clone())
        .with_reasoning_effort(self.provider.reasoning_effort.clone())
        .with_max_output_tokens(Some(512))
        .with_request_timeout_ms(self.provider.request_timeout_ms.unwrap_or(120_000));

        let mut case_scores = Vec::new();
        let mut raw = Vec::new();
        for case in &def.cases {
            let answer = answers
                .get(&case.id)
                .ok_or_else(|| format!("missing answer for case {}", case.id))?;
            let rubric = self
                .store
                .read_rubric(&request.benchmark_id, &case.id)
                .map_err(|e| e.to_string())?;
            let instructions = build_evaluation_instructions(case, &rubric);
            let prompt = format!("{instructions}\n【被测回答】\n{}", answer.trim());

            if request.dry_run {
                raw.push(RawCaseEvaluation {
                    case_id: case.id.clone(),
                    prompt: prompt.clone(),
                    model_output: "<dry-run>".to_string(),
                    parsed: true,
                    parse_error: None,
                });
                continue;
            }

            let output = adapter.respond(&ResponderRequest {
                prompt: instructions.clone(),
                user_input: if answer.trim().is_empty() {
                    "（空回答）".to_string()
                } else {
                    answer.trim().to_string()
                },
                recall_hit_count: 0,
            });
            if output.body.starts_with("PROVIDER_HTTP_ERROR") {
                raw.push(RawCaseEvaluation {
                    case_id: case.id.clone(),
                    prompt,
                    model_output: output.body.clone(),
                    parsed: false,
                    parse_error: Some("provider_error".to_string()),
                });
                return Err(format!(
                    "evaluator provider failed for case {}: {}",
                    case.id, output.body
                ));
            }
            let model_output = output.body;

            match parse_case_score(&model_output) {
                Ok(parsed) => {
                    if parsed.score > parsed.max_score {
                        raw.push(RawCaseEvaluation {
                            case_id: case.id.clone(),
                            prompt,
                            model_output: model_output.clone(),
                            parsed: false,
                            parse_error: Some(format!(
                                "score {} exceeds max {}",
                                parsed.score, parsed.max_score
                            )),
                        });
                        return Err(format!(
                            "evaluator returned invalid score for case {}: score {} > max {}",
                            case.id, parsed.score, parsed.max_score
                        ));
                    }
                    case_scores.push(CaseScore {
                        case_id: case.id.clone(),
                        score: parsed.score,
                        max_score: parsed.max_score,
                        reason: parsed.reason,
                    });
                    raw.push(RawCaseEvaluation {
                        case_id: case.id.clone(),
                        prompt,
                        model_output,
                        parsed: true,
                        parse_error: None,
                    });
                }
                Err(e) => {
                    raw.push(RawCaseEvaluation {
                        case_id: case.id.clone(),
                        prompt,
                        model_output: model_output.clone(),
                        parsed: false,
                        parse_error: Some(e),
                    });
                    return Err(format!(
                        "evaluator could not parse score for case {}: {}",
                        case.id, model_output
                    ));
                }
            }
        }

        Ok(EvaluateReceipt {
            benchmark_id: request.benchmark_id.clone(),
            provider_id: self.provider.provider_id.clone(),
            model_name: self.provider.model_name.clone(),
            evaluated_case_count: case_scores.len(),
            case_scores,
            raw,
            dry_run: request.dry_run,
        })
    }
}

pub fn build_evaluation_instructions(case: &BenchmarkCase, rubric: &str) -> String {
    format!(
        "你是创项目的能力基准评审员。你只做评分，不做其他事。\n\
         \n\
         【题目】\n{statement}\n\
         \n\
         【评分标准（仅评审员可见，不要复述给被测方）】\n{rubric}\n\
         \n\
         请只输出一个 JSON 对象，不要输出任何其他文字或代码围栏：\n\
         {{\"score\": 数字, \"max_score\": 数字, \"reason\": \"一句话评分理由\"}}\n",
        statement = case.statement,
        rubric = rubric.trim(),
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCaseScore {
    pub score: u16,
    pub max_score: u16,
    pub reason: String,
}

/// Parse the model's JSON score output. Accepts a bare object or one wrapped
/// in markdown code fences; extracts the first balanced {...} block.
pub fn parse_case_score(output: &str) -> Result<ParsedCaseScore, String> {
    let Some(start) = output.find('{') else {
        return Err("no JSON object found in model output".to_string());
    };
    let mut depth = 0u32;
    let mut end = None;
    let bytes = output.as_bytes();
    for (i, byte) in bytes.iter().enumerate().skip(start) {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    end = Some(i + 1);
                    break;
                }
            }
            _ => {}
        }
    }
    let end = end.ok_or_else(|| "unbalanced JSON object in model output".to_string())?;
    let slice = &output[start..end];
    let parsed: serde_json::Value =
        serde_json::from_str(slice).map_err(|e| format!("invalid JSON: {e}"))?;
    let score = parsed
        .get("score")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| "missing score field".to_string())?
        .try_into()
        .map_err(|_| "score out of range".to_string())?;
    let max_score = parsed
        .get("max_score")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| "missing max_score field".to_string())?
        .try_into()
        .map_err(|_| "max_score out of range".to_string())?;
    let reason = parsed
        .get("reason")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    Ok(ParsedCaseScore {
        score,
        max_score,
        reason,
    })
}

trait ProviderConfigKindName {
    fn kind_name(&self) -> &'static str;
}

impl ProviderConfigKindName for ProviderConfig {
    fn kind_name(&self) -> &'static str {
        match self {
            ProviderConfig::Fake { .. } => "fake",
            ProviderConfig::OpenAICompatible(_) => "openai_compatible",
            ProviderConfig::Fallback { .. } => "fallback",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_case() -> BenchmarkCase {
        BenchmarkCase {
            id: "case-001".to_string(),
            title: "test".to_string(),
            statement: "召回用户偏好".to_string(),
            rubric: "2分：准确；0分：错误".to_string(),
        }
    }

    #[test]
    fn parse_case_score_accepts_bare_json() {
        let parsed =
            parse_case_score(r#"{"score": 2, "max_score": 2, "reason": "准确"}"#).expect("parse");
        assert_eq!(parsed.score, 2);
        assert_eq!(parsed.max_score, 2);
        assert_eq!(parsed.reason, "准确");
    }

    #[test]
    fn parse_case_score_accepts_code_fence() {
        let parsed = parse_case_score(
            "好的，这是评分：\n```json\n{\"score\": 1, \"max_score\": 2, \"reason\": \"部分正确\"}\n```\n",
        )
        .expect("parse");
        assert_eq!(parsed.score, 1);
    }

    #[test]
    fn parse_case_score_rejects_missing_fields() {
        assert!(parse_case_score(r#"{"score": 2}"#).is_err());
        assert!(parse_case_score("no json at all").is_err());
    }

    #[test]
    fn prompt_embeds_rubric_for_evaluator_only() {
        let prompt = build_evaluation_instructions(&sample_case(), "2分：准确；0分：错误");
        assert!(prompt.contains("评分标准"));
        assert!(prompt.contains("2分：准确；0分：错误"));
        assert!(!prompt.contains("被测回答"));
    }
}
