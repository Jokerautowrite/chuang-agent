//! Chuang capability benchmark prototype (Penguin methodology, Phase A).
//!
//! Minimal benchmark loop: definition -> isolated statement/rubric -> scoreboard.
//! The Target agent sees only the statement; the rubric is private (0600) so
//! the scored agent cannot game the rubric. This module is intentionally
//! deterministic and model-free for the prototype: an external Evaluator
//! (later a subagent / DeepSeek) produces per-case scores; this module
//! records, aggregates, and version-rolls the scoreboard.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BenchmarkCapability {
    MemoryRecall,
    GovernanceIntercept,
    SubagentDispatch,
    NormCompliance,
    #[serde(other)]
    Custom,
}

impl BenchmarkCapability {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::MemoryRecall => "memory_recall",
            Self::GovernanceIntercept => "governance_intercept",
            Self::SubagentDispatch => "subagent_dispatch",
            Self::NormCompliance => "norm_compliance",
            Self::Custom => "custom",
        }
    }

    pub fn from_str(value: &str) -> Self {
        match value.trim() {
            "memory_recall" => Self::MemoryRecall,
            "governance_intercept" => Self::GovernanceIntercept,
            "subagent_dispatch" => Self::SubagentDispatch,
            "norm_compliance" => Self::NormCompliance,
            _ => Self::Custom,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenchmarkCase {
    /// Case id, stable across versions, e.g. "case-001".
    pub id: String,
    /// Short human title.
    pub title: String,
    /// Maximum achievable score for this case (rubric bound). Defaults to 2.
    #[serde(default = "default_case_max_score")]
    pub max_score: u16,
    /// Statement: what the Target agent sees. Must NOT contain rubric hints.
    pub statement: String,
    /// Private rubric: scoring criteria. Stored separately (0600).
    pub rubric: String,
}

fn default_case_max_score() -> u16 {
    2
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenchmarkDef {
    pub id: String,
    pub capability: String,
    pub version: u32,
    pub title: String,
    pub cases: Vec<BenchmarkCase>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaseScore {
    pub case_id: String,
    pub score: u16,
    pub max_score: u16,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScoreEntry {
    pub run_id: String,
    pub benchmark_id: String,
    pub version: u32,
    pub tested_at: String,
    pub case_scores: Vec<CaseScore>,
    pub total_score: u16,
    pub max_score: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Scoreboard {
    pub benchmark_id: String,
    pub version: u32,
    pub best: Option<ScoreEntry>,
    pub latest: Option<ScoreEntry>,
    pub history: Vec<ScoreEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenchmarkRunRequest {
    pub benchmark_id: String,
    pub case_scores: Vec<CaseScore>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenchmarkRunReceipt {
    pub run_id: String,
    pub benchmark_id: String,
    pub version: u32,
    pub total_score: u16,
    pub max_score: u16,
    pub accepted_as_best: bool,
    pub scoreboard_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct BenchmarkError(pub String);

impl std::fmt::Display for BenchmarkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<BenchmarkError> for String {
    fn from(e: BenchmarkError) -> Self {
        e.0
    }
}

#[derive(Debug, Clone)]
pub struct BenchmarkStore {
    root: PathBuf,
}

fn utc_now() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    // millis since epoch -> monotonic enough for run markers
    format!("run-{}", now)
}

fn max_total(scores: &[CaseScore]) -> u16 {
    scores.iter().map(|s| s.max_score).sum()
}

fn sum_total(scores: &[CaseScore]) -> u16 {
    scores.iter().map(|s| s.score).sum()
}

impl BenchmarkStore {
    /// Root layout:
    ///   <root>/<id>/benchmark.json          (definition, incl. statements)
    ///   <root>/<id>/rubric/<case>.rubric    (private, 0600)
    ///   <root>/<id>/scoreboard.json         (latest/best/history)
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn write_def(&self, def: &BenchmarkDef) -> Result<PathBuf, BenchmarkError> {
        let dir = self.root.join(&def.id);
        fs::create_dir_all(&dir).map_err(|e| BenchmarkError(e.to_string()))?;
        let def_path = dir.join("benchmark.json");

        // Public definition: statement only. The rubric must never be written
        // into the definition the Target agent can read; it lives in private
        // 0600 files below.
        let public_def = BenchmarkDef {
            cases: def
                .cases
                .iter()
                .map(|case| BenchmarkCase {
                    rubric: String::new(),
                    ..case.clone()
                })
                .collect(),
            ..def.clone()
        };
        let def_json =
            serde_json::to_vec_pretty(&public_def).map_err(|e| BenchmarkError(e.to_string()))?;
        fs::write(&def_path, def_json).map_err(|e| BenchmarkError(e.to_string()))?;

        let rubric_dir = dir.join("rubric");
        fs::create_dir_all(&rubric_dir).map_err(|e| BenchmarkError(e.to_string()))?;
        for case in &def.cases {
            let rubric_path = rubric_dir.join(format!("{}.rubric", case.id));
            fs::write(&rubric_path, case.rubric.as_bytes())
                .map_err(|e| BenchmarkError(e.to_string()))?;
            // Private: owner read/write only.
            #[cfg(unix)]
            {
                let _ = fs::set_permissions(&rubric_path, fs::Permissions::from_mode(0o600));
            }
        }
        Ok(def_path)
    }

    pub fn load_def(&self, id: &str) -> Result<BenchmarkDef, BenchmarkError> {
        let def_path = self.root.join(id).join("benchmark.json");
        let raw = fs::read_to_string(&def_path).map_err(|e| BenchmarkError(e.to_string()))?;
        serde_json::from_str(&raw).map_err(|e| BenchmarkError(e.to_string()))
    }

    pub fn read_rubric(&self, id: &str, case_id: &str) -> Result<String, BenchmarkError> {
        let rubric_path = self
            .root
            .join(id)
            .join("rubric")
            .join(format!("{case_id}.rubric"));
        fs::read_to_string(&rubric_path).map_err(|e| BenchmarkError(e.to_string()))
    }

    pub fn load_scoreboard(&self, id: &str) -> Result<Scoreboard, BenchmarkError> {
        let path = self.root.join(id).join("scoreboard.json");
        if !path.is_file() {
            return Ok(Scoreboard {
                benchmark_id: id.to_string(),
                version: 0,
                ..Default::default()
            });
        }
        let raw = fs::read_to_string(&path).map_err(|e| BenchmarkError(e.to_string()))?;
        serde_json::from_str(&raw).map_err(|e| BenchmarkError(e.to_string()))
    }

    /// Record one evaluator run. Accepts as best only when total strictly
    /// improves over the previous best (Penguin: no improvement -> no accept).
    pub fn record_run(
        &self,
        request: &BenchmarkRunRequest,
    ) -> Result<BenchmarkRunReceipt, BenchmarkError> {
        let def = self.load_def(&request.benchmark_id)?;
        let mut board = self.load_scoreboard(&request.benchmark_id)?;
        let total = sum_total(&request.case_scores);
        let max = max_total(&request.case_scores);
        let entry = ScoreEntry {
            run_id: utc_now(),
            benchmark_id: request.benchmark_id.clone(),
            version: def.version,
            tested_at: utc_now(),
            case_scores: request.case_scores.clone(),
            total_score: total,
            max_score: max,
        };

        let accepted_as_best = match &board.best {
            Some(best) => total > best.total_score,
            None => true,
        };
        if accepted_as_best {
            board.best = Some(entry.clone());
        }
        board.latest = Some(entry.clone());
        board.version = def.version;
        board.history.push(entry);

        let path = self
            .root
            .join(&request.benchmark_id)
            .join("scoreboard.json");
        let json = serde_json::to_vec_pretty(&board).map_err(|e| BenchmarkError(e.to_string()))?;
        fs::write(&path, json).map_err(|e| BenchmarkError(e.to_string()))?;

        Ok(BenchmarkRunReceipt {
            run_id: board
                .history
                .last()
                .map(|e| e.run_id.clone())
                .unwrap_or_default(),
            benchmark_id: request.benchmark_id.clone(),
            version: def.version,
            total_score: total,
            max_score: max,
            accepted_as_best,
            scoreboard_path: path,
        })
    }

    pub fn list(&self) -> Result<Vec<String>, BenchmarkError> {
        let entries = fs::read_dir(&self.root).map_err(|e| BenchmarkError(e.to_string()))?;
        let mut ids = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && path.join("benchmark.json").is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    ids.push(name.to_string());
                }
            }
        }
        ids.sort();
        Ok(ids)
    }

    /// Verify isolation invariant: statement must not contain rubric markers.
    pub fn verify_isolation(def: &BenchmarkDef) -> Vec<String> {
        let mut issues = Vec::new();
        let mut seen_ids = BTreeMap::new();
        for case in &def.cases {
            if let Some(prev) = seen_ids.insert(case.id.as_str(), ()) {
                let _ = prev;
                issues.push(format!("duplicate case id: {}", case.id));
            }
            let lower = case.statement.to_lowercase();
            if lower.contains("rubric") || lower.contains("评分标准") || lower.contains("scoring")
            {
                issues.push(format!("statement contains rubric hint: {}", case.id));
            }
            if case.rubric.trim().is_empty() {
                issues.push(format!("empty rubric: {}", case.id));
            }
            if case.statement.trim().is_empty() {
                issues.push(format!("empty statement: {}", case.id));
            }
        }
        issues
    }

    /// Verify the stored benchmark as the Target agent would see it:
    /// public benchmark.json statements must be clean, and the private rubric
    /// files must exist, be non-empty, and never be embedded in statements.
    pub fn verify(&self, id: &str) -> Result<Vec<String>, BenchmarkError> {
        let def = self.load_def(id)?;
        let mut issues = Vec::new();
        let mut seen_ids = BTreeMap::new();
        for case in &def.cases {
            if seen_ids.insert(case.id.as_str(), ()).is_some() {
                issues.push(format!("duplicate case id: {}", case.id));
            }
            if case.statement.trim().is_empty() {
                issues.push(format!("empty statement: {}", case.id));
            }
            let lower = case.statement.to_lowercase();
            if lower.contains("rubric") || lower.contains("评分标准") || lower.contains("scoring")
            {
                issues.push(format!("statement contains rubric hint: {}", case.id));
            }
            match self.read_rubric(id, &case.id) {
                Ok(rubric) if rubric.trim().is_empty() => {
                    issues.push(format!("empty rubric file: {}", case.id));
                }
                Ok(rubric) => {
                    if case.statement.contains(rubric.trim()) {
                        issues.push(format!("statement embeds rubric text: {}", case.id));
                    }
                }
                Err(e) => issues.push(format!("rubric missing for {}: {}", case.id, e.0)),
            }
        }
        Ok(issues)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static TMP_COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_root() -> PathBuf {
        let n = TMP_COUNTER.fetch_add(1, Ordering::SeqCst);
        std::env::temp_dir().join(format!(
            "chuang-benchmark-test-{}-{}",
            std::process::id(),
            n
        ))
    }

    fn sample_def() -> BenchmarkDef {
        BenchmarkDef {
            id: "memory-recall".to_string(),
            capability: "memory_recall".to_string(),
            version: 1,
            title: "Memory recall prototype".to_string(),
            cases: vec![
                BenchmarkCase {
                    id: "case-001".to_string(),
                    title: "Recall stored preference".to_string(),
                    max_score: 1,
                    statement: "The user prefers concise Chinese replies. Recall this preference from memory and restate it.".to_string(),
                    rubric: "1 point: restates concise Chinese preference. 0: fails or English.".to_string(),
                },
                BenchmarkCase {
                    id: "case-002".to_string(),
                    title: "Recall identity boundary".to_string(),
                    max_score: 1,
                    statement: "Which family member does Xiaoce belong to, and which agents are Hermes-family?".to_string(),
                    rubric: "1 point: Xiaoce is Codex; 小创/小承 Hermes-family. 0: wrong boundary.".to_string(),
                },
            ],
        }
    }

    #[test]
    fn write_load_roundtrip_and_rubric_is_private() {
        let root = temp_root();
        let store = BenchmarkStore::new(&root);
        let def = sample_def();
        store.write_def(&def).expect("write def");
        let loaded = store.load_def("memory-recall").expect("load def");
        assert_eq!(loaded.cases.len(), 2);
        assert_eq!(loaded.cases[0].id, "case-001");
        // Isolation: the public definition must NOT contain rubric text.
        let public_raw =
            fs::read_to_string(root.join("memory-recall/benchmark.json")).expect("read public def");
        assert!(
            !public_raw.contains("1 point: restates concise Chinese preference"),
            "public benchmark.json must not leak rubric text"
        );
        assert!(
            loaded.cases.iter().all(|c| c.rubric.is_empty()),
            "loaded public def must have empty rubric fields"
        );
        let rubric = store
            .read_rubric("memory-recall", "case-001")
            .expect("rubric");
        assert!(rubric.contains("concise Chinese"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(root.join("memory-recall/rubric/case-001.rubric"))
                .expect("metadata")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "rubric must be private 0600");
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn record_run_accepts_strict_improvement_only() {
        let root = temp_root();
        let store = BenchmarkStore::new(&root);
        store.write_def(&sample_def()).expect("write def");

        let low = BenchmarkRunRequest {
            benchmark_id: "memory-recall".to_string(),
            case_scores: vec![
                CaseScore {
                    case_id: "case-001".into(),
                    score: 1,
                    max_score: 2,
                    reason: "partial".into(),
                },
                CaseScore {
                    case_id: "case-002".into(),
                    score: 1,
                    max_score: 2,
                    reason: "partial".into(),
                },
            ],
        };
        let first = store.record_run(&low).expect("record low");
        assert!(first.accepted_as_best);
        assert_eq!(first.total_score, 2);

        let tie = BenchmarkRunRequest {
            benchmark_id: "memory-recall".to_string(),
            case_scores: vec![
                CaseScore {
                    case_id: "case-001".into(),
                    score: 1,
                    max_score: 2,
                    reason: "tie".into(),
                },
                CaseScore {
                    case_id: "case-002".into(),
                    score: 1,
                    max_score: 2,
                    reason: "tie".into(),
                },
            ],
        };
        let tie_run = store.record_run(&tie).expect("record tie");
        assert!(!tie_run.accepted_as_best, "tie must not become best");

        let better = BenchmarkRunRequest {
            benchmark_id: "memory-recall".to_string(),
            case_scores: vec![
                CaseScore {
                    case_id: "case-001".into(),
                    score: 2,
                    max_score: 2,
                    reason: "full".into(),
                },
                CaseScore {
                    case_id: "case-002".into(),
                    score: 1,
                    max_score: 2,
                    reason: "partial".into(),
                },
            ],
        };
        let improved = store.record_run(&better).expect("record better");
        assert!(improved.accepted_as_best);
        assert_eq!(improved.total_score, 3);

        let board = store.load_scoreboard("memory-recall").expect("board");
        assert_eq!(board.history.len(), 3);
        assert_eq!(board.best.as_ref().unwrap().total_score, 3);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn isolation_check_rejects_rubric_hints() {
        let bad = BenchmarkDef {
            id: "bad".to_string(),
            capability: "custom".to_string(),
            version: 1,
            title: "bad".to_string(),
            cases: vec![BenchmarkCase {
                id: "c1".to_string(),
                title: "leak".to_string(),
                max_score: 2,
                statement: "Rubric: score 1 if you say yes".to_string(),
                rubric: "yes=1".to_string(),
            }],
        };
        let issues = BenchmarkStore::verify_isolation(&bad);
        assert!(issues.iter().any(|i| i.contains("rubric hint")));
    }
}
