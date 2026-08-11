//! `diary` 模块：节点触发的会话总结日记（非每轮流水）。
//!
//! 设计依据（对齐 opencode 的记忆模型）：
//! - 全量对话由 `session_turn_archive`（SQLite）逐轮落盘，API 中断不丢聊天记录；
//! - 日记只记录**节点总结**：任务收尾信号 或 距上次日记 ≥ N 轮时追加一条
//!   （已完成/进行中/待办/约束），不是每轮都写；
//! - 经验每日从日记提炼（`memory diary distill` → 过滤噪音 → `experiences.md`）。
//!
//! 公开接口：struct DiaryConfig, DiaryEntry；enum DiaryError；fn new,
//! diary_root, path_for_date；struct FileDiaryStore；fn open, append, read_date,
//! last_seq_for_session, list_dates。

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{Datelike, Local, Timelike};

pub const DEFAULT_DIARY_DIR: &str = "diary";
/// 距上次日记达到该轮数时触发中期总结（非收尾信号）。
pub const DIARY_TURN_THRESHOLD: u64 = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiaryConfig {
    /// identity 根目录（与 DualFileMemoryConfig.root 一致，如 ./identity）。
    pub root: PathBuf,
    /// 日记子目录名，默认 "diary"。
    pub dir: String,
}

impl DiaryConfig {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            dir: DEFAULT_DIARY_DIR.to_string(),
        }
    }

    pub fn diary_root(&self) -> PathBuf {
        self.root.join(&self.dir)
    }

    pub fn path_for_date(&self, date: &str) -> PathBuf {
        self.diary_root().join(format!("{date}.md"))
    }
}

/// 一条日记总结（按天分文件，文件内按时间顺序追加）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiaryEntry {
    /// 文件归属日期 YYYY-MM-DD（本地时区）。
    pub date: String,
    /// 触发时的会话归档轮次序号（session_turn_archive.sequence）。
    pub seq: u64,
    /// 条目创建时间（本地时区，格式 HH:MM）。
    pub created_at: String,
    pub session_id: String,
    /// 触发原因：completion_signal（收尾信号）| turn_threshold（轮数阈值）。
    pub trigger: String,
    pub completed: String,
    pub in_progress: String,
    pub pending: String,
    pub constraints: String,
}

impl DiaryEntry {
    /// 渲染为日记文件文本块（易读 + 可解析）。
    pub fn render(&self) -> String {
        let mut s = format!(
            "## {} [seq={} trigger={}]\n",
            self.created_at, self.seq, self.trigger
        );
        s.push_str(&format!("session={}\n", self.session_id));
        s.push_str(&format!("completed={}\n", single_line(&self.completed)));
        s.push_str(&format!("in_progress={}\n", single_line(&self.in_progress)));
        s.push_str(&format!("pending={}\n", single_line(&self.pending)));
        s.push_str(&format!("constraints={}\n", single_line(&self.constraints)));
        s.push('\n');
        s
    }

    /// 把整条日记拼成可评估的经验候选文本（供每日提炼过滤用）。
    pub fn as_candidate_text(&self) -> String {
        format!(
            "会话总结：\n已完成：{}\n进行中：{}\n待办：{}\n约束：{}",
            self.completed, self.in_progress, self.pending, self.constraints
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiaryError {
    StorageUnavailable {
        path: PathBuf,
    },
    ReadFailed {
        path: PathBuf,
    },
    ParseFailed {
        path: PathBuf,
        line: usize,
        detail: String,
    },
}

impl DiaryError {
    pub fn storage(path: impl Into<PathBuf>) -> Self {
        Self::StorageUnavailable { path: path.into() }
    }

    pub fn read(path: impl Into<PathBuf>) -> Self {
        Self::ReadFailed { path: path.into() }
    }

    pub fn parse(path: impl Into<PathBuf>, line: usize, detail: impl Into<String>) -> Self {
        Self::ParseFailed {
            path: path.into(),
            line,
            detail: detail.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDiaryStore {
    config: DiaryConfig,
}

impl FileDiaryStore {
    pub fn open(config: DiaryConfig) -> Result<Self, DiaryError> {
        fs::create_dir_all(config.diary_root())
            .map_err(|_| DiaryError::storage(config.diary_root()))?;
        Ok(Self { config })
    }

    pub fn config(&self) -> &DiaryConfig {
        &self.config
    }

    /// 追加一条日记到当日文件（文件不存在则创建）。
    pub fn append(&mut self, entry: DiaryEntry) -> Result<(), DiaryError> {
        let path = self.config.path_for_date(&entry.date);
        let mut content = fs::read_to_string(&path).unwrap_or_default();
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(&entry.render());
        atomic_write(&path, &content)
    }

    /// 读取指定日期的全部日记条目。
    pub fn read_date(&self, date: &str) -> Result<Vec<DiaryEntry>, DiaryError> {
        let path = self.config.path_for_date(date);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let content = fs::read_to_string(&path).map_err(|_| DiaryError::read(&path))?;
        parse_date_file(&content, date, &path)
    }

    /// 读取某个会话在指定日期最后一条日记的轮次序号（无则 None）。
    pub fn last_seq_for_session(
        &self,
        date: &str,
        session_id: &str,
    ) -> Result<Option<u64>, DiaryError> {
        let entries = self.read_date(date)?;
        Ok(entries
            .iter()
            .filter(|entry| entry.session_id == session_id)
            .map(|entry| entry.seq)
            .max())
    }

    /// 列出已存在的日记日期（YYYY-MM-DD，升序）。
    pub fn list_dates(&self) -> Result<Vec<String>, DiaryError> {
        let root = self.config.diary_root();
        let mut dates = Vec::new();
        let entries = fs::read_dir(&root).map_err(|_| DiaryError::read(&root))?;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(date) = name.strip_suffix(".md") {
                if is_date_like(date) {
                    dates.push(date.to_string());
                }
            }
        }
        dates.sort();
        Ok(dates)
    }
}

fn parse_date_file(content: &str, date: &str, path: &Path) -> Result<Vec<DiaryEntry>, DiaryError> {
    let mut entries = Vec::new();
    let mut current: Option<EntryBuilder> = None;
    for (idx, line) in content.lines().enumerate() {
        let line_no = idx + 1;
        if let Some(rest) = line.strip_prefix("## ") {
            if let Some(builder) = current.take() {
                entries.push(builder.build(path, line_no)?);
            }
            current = Some(parse_header(rest, date, path, line_no)?);
        } else if let Some(builder) = current.as_mut() {
            if let Some((key, value)) = line.split_once('=') {
                match key.trim() {
                    "session" => builder.session_id = Some(value.trim().to_string()),
                    "completed" => builder.completed = Some(value.trim().to_string()),
                    "in_progress" => builder.in_progress = Some(value.trim().to_string()),
                    "pending" => builder.pending = Some(value.trim().to_string()),
                    "constraints" => builder.constraints = Some(value.trim().to_string()),
                    _ => {}
                }
            }
        }
    }
    if let Some(builder) = current.take() {
        entries.push(builder.build(path, content.lines().count())?);
    }
    Ok(entries)
}

#[derive(Debug, Default)]
struct EntryBuilder {
    date: Option<String>,
    seq: Option<u64>,
    created_at: Option<String>,
    session_id: Option<String>,
    trigger: Option<String>,
    completed: Option<String>,
    in_progress: Option<String>,
    pending: Option<String>,
    constraints: Option<String>,
}

impl EntryBuilder {
    fn build(self, path: &Path, line: usize) -> Result<DiaryEntry, DiaryError> {
        Ok(DiaryEntry {
            date: self
                .date
                .ok_or_else(|| DiaryError::parse(path, line, "missing date in diary header"))?,
            seq: self
                .seq
                .ok_or_else(|| DiaryError::parse(path, line, "missing seq in diary header"))?,
            created_at: self.created_at.ok_or_else(|| {
                DiaryError::parse(path, line, "missing created_at in diary header")
            })?,
            session_id: self.session_id.unwrap_or_default(),
            trigger: self.trigger.unwrap_or_else(|| "unknown".to_string()),
            completed: self.completed.unwrap_or_default(),
            in_progress: self.in_progress.unwrap_or_default(),
            pending: self.pending.unwrap_or_default(),
            constraints: self.constraints.unwrap_or_default(),
        })
    }
}

/// 解析条目头：`2026-08-11 05:42 [seq=12 trigger=completion_signal]`。
fn parse_header(
    rest: &str,
    date: &str,
    path: &Path,
    line: usize,
) -> Result<EntryBuilder, DiaryError> {
    let mut builder = EntryBuilder::default();
    builder.date = Some(date.to_string());
    if let Some((time, meta)) = rest.split_once('[') {
        builder.created_at = Some(time.trim().to_string());
        let meta = meta.trim_end_matches(']');
        for part in meta.split_whitespace() {
            if let Some((key, value)) = part.split_once('=') {
                match key {
                    "seq" => {
                        builder.seq = Some(value.parse::<u64>().map_err(|_| {
                            DiaryError::parse(path, line, format!("invalid seq: {value}"))
                        })?);
                    }
                    "trigger" => builder.trigger = Some(value.to_string()),
                    _ => {}
                }
            }
        }
    }
    if builder.seq.is_none() {
        return Err(DiaryError::parse(
            path,
            line,
            "missing [seq=N] in diary header",
        ));
    }
    Ok(builder)
}

fn single_line(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch == '\n' || ch == '\r' {
            if !out.ends_with('；') && !out.ends_with(' ') && !out.is_empty() {
                out.push('；');
            }
        } else if ch.is_control() {
            continue;
        } else {
            out.push(ch);
        }
    }
    while out.ends_with('；') || out.ends_with(' ') {
        out.pop();
    }
    out
}

fn is_date_like(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    parts.len() == 3
        && parts[0].len() == 4
        && parts[1].len() == 2
        && parts[2].len() == 2
        && parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit()))
}

pub fn today_local() -> String {
    let now = Local::now();
    format!("{:04}-{:02}-{:02}", now.year(), now.month(), now.day())
}

pub fn now_local_hm() -> String {
    let now = Local::now();
    format!("{:02}:{:02}", now.hour(), now.minute())
}

fn atomic_write(path: &Path, content: &str) -> Result<(), DiaryError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|_| DiaryError::storage(parent.to_path_buf()))?;
    }
    fs::write(path, content).map_err(|_| DiaryError::storage(path.to_path_buf()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entry(date: &str, seq: u64, trigger: &str) -> DiaryEntry {
        DiaryEntry {
            date: date.to_string(),
            seq,
            created_at: "05:42".to_string(),
            session_id: "ses-test".to_string(),
            trigger: trigger.to_string(),
            completed: "完成经验记忆链路调整".to_string(),
            in_progress: "验证每日提炼命令".to_string(),
            pending: "待用户验收".to_string(),
            constraints: "禁止删除任何文件".to_string(),
        }
    }

    #[test]
    fn append_and_read_roundtrip() {
        let temp = std::env::temp_dir().join(format!("chuang-diary-test-{}", std::process::id()));
        let config = DiaryConfig::new(temp.join("identity"));
        let mut store = FileDiaryStore::open(config.clone()).expect("open should succeed");
        let date = "2026-08-11";
        store
            .append(sample_entry(date, 1, "completion_signal"))
            .expect("append should succeed");
        store
            .append(sample_entry(date, 12, "turn_threshold"))
            .expect("append should succeed");

        let entries = store.read_date(date).expect("read should succeed");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].seq, 1);
        assert_eq!(entries[0].trigger, "completion_signal");
        assert_eq!(entries[1].seq, 12);
        assert_eq!(entries[1].trigger, "turn_threshold");
        assert_eq!(entries[1].completed, "完成经验记忆链路调整");
        assert_eq!(
            store
                .last_seq_for_session(date, "ses-test")
                .expect("last_seq should succeed"),
            Some(12)
        );
    }

    #[test]
    fn read_missing_date_returns_empty() {
        let temp =
            std::env::temp_dir().join(format!("chuang-diary-empty-test-{}", std::process::id()));
        let config = DiaryConfig::new(temp.join("identity"));
        let store = FileDiaryStore::open(config).expect("open should succeed");
        assert!(store
            .read_date("2026-01-01")
            .expect("read should succeed")
            .is_empty());
    }

    #[test]
    fn list_dates_returns_sorted_dates() {
        let temp =
            std::env::temp_dir().join(format!("chuang-diary-list-test-{}", std::process::id()));
        let config = DiaryConfig::new(temp.join("identity"));
        let mut store = FileDiaryStore::open(config).expect("open should succeed");
        store
            .append(sample_entry("2026-08-10", 1, "completion_signal"))
            .expect("append should succeed");
        store
            .append(sample_entry("2026-08-11", 1, "completion_signal"))
            .expect("append should succeed");
        let dates = store.list_dates().expect("list should succeed");
        assert_eq!(
            dates,
            vec!["2026-08-10".to_string(), "2026-08-11".to_string()]
        );
    }

    #[test]
    fn single_line_flattens_newlines() {
        assert_eq!(single_line("第一行\n第二行"), "第一行；第二行");
        assert_eq!(single_line("无换行内容"), "无换行内容");
    }
}
