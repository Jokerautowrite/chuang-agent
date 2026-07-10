use chrono::Utc;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

use crate::path_utils::resolve_candidate_preserving_existing_symlinks;
use crate::secret_redaction::redact_sensitive_text;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkspaceListResult {
    pub path: String,
    pub resolved_path: String,
    pub entries: Vec<WorkspaceDirectoryEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkspaceDirectoryEntry {
    pub name: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkspaceReadResult {
    pub path: String,
    pub resolved_path: String,
    pub content: String,
    pub bytes: usize,
    pub lines: usize,
    pub redacted: bool,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkspaceWriteResult {
    pub path: String,
    pub resolved_path: String,
    pub before_bytes: Option<usize>,
    pub after_bytes: usize,
    pub changed: bool,
    pub operation: WorkspaceWriteOperation,
    pub diff_preview: String,
    pub diff_truncated: bool,
    pub redacted: bool,
    pub backup_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkspacePatchResult {
    pub changed_files: Vec<String>,
    pub backup_paths: Vec<String>,
    pub diff_preview: String,
    pub diff_truncated: bool,
    pub operation_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceWriteOperation {
    Created,
    Modified,
    Unchanged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceFileAdapter {
    workspace_root: PathBuf,
    audit_root: PathBuf,
}

impl WorkspaceFileAdapter {
    pub fn new(workspace_root: impl Into<PathBuf>) -> Self {
        let workspace_root = workspace_root.into();
        let audit_root = workspace_root.join(".chuang-file-audit");
        Self {
            workspace_root,
            audit_root,
        }
    }

    pub fn list_dir(&self, path: &str) -> Result<WorkspaceListResult, String> {
        let dir = self.resolve_workspace_path(path)?;
        let mut entries = fs::read_dir(&dir)
            .map_err(|error| format!("list_dir_failed path={} error={error}", dir.display()))?
            .filter_map(|entry| entry.ok())
            .map(|entry| {
                let file_type = entry.file_type().ok();
                let kind = if file_type.as_ref().is_some_and(|ft| ft.is_dir()) {
                    "dir"
                } else if file_type.as_ref().is_some_and(|ft| ft.is_file()) {
                    "file"
                } else {
                    "other"
                };
                WorkspaceDirectoryEntry {
                    name: entry.file_name().to_string_lossy().to_string(),
                    kind: kind.to_string(),
                }
            })
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| left.name.cmp(&right.name).then(left.kind.cmp(&right.kind)));

        Ok(WorkspaceListResult {
            path: path.trim().to_string(),
            resolved_path: dir.display().to_string(),
            entries,
        })
    }

    pub fn read_file(&self, path: &str) -> Result<WorkspaceReadResult, String> {
        let file = self.resolve_workspace_path(path)?;
        let content = fs::read_to_string(&file)
            .map_err(|error| format!("read_file_failed path={} error={error}", file.display()))?;
        let redaction = redact_sensitive_text(path, &content);
        let truncated = truncate_text(&redaction.text, 10_000);

        Ok(WorkspaceReadResult {
            path: path.trim().to_string(),
            resolved_path: file.display().to_string(),
            content: truncated.text,
            bytes: content.len(),
            lines: count_lines(&content),
            redacted: redaction.redacted,
            truncated: truncated.truncated,
        })
    }

    pub fn write_file(&self, path: &str, content: &str) -> Result<WorkspaceWriteResult, String> {
        let file = self.resolve_workspace_path(path)?;
        let previous_content = if file.exists() {
            Some(fs::read_to_string(&file).map_err(|error| {
                format!(
                    "write_file_read_existing_failed path={} error={error}",
                    file.display()
                )
            })?)
        } else {
            None
        };
        let backup_paths = self.backup_existing(&file, previous_content.as_deref())?;
        if let Some(parent) = file.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "write_file_mkdir_failed path={} error={error}",
                    parent.display()
                )
            })?;
        }
        fs::write(&file, content)
            .map_err(|error| format!("write_file_failed path={} error={error}", file.display()))?;

        let diff_preview = build_write_diff_preview(path, previous_content.as_deref(), content);
        Ok(WorkspaceWriteResult {
            path: path.trim().to_string(),
            resolved_path: file.display().to_string(),
            before_bytes: previous_content.as_ref().map(|value| value.len()),
            after_bytes: content.len(),
            changed: previous_content.as_deref() != Some(content),
            operation: match previous_content.as_deref() {
                None => WorkspaceWriteOperation::Created,
                Some(previous) if previous == content => WorkspaceWriteOperation::Unchanged,
                Some(_) => WorkspaceWriteOperation::Modified,
            },
            diff_preview: diff_preview.text,
            diff_truncated: diff_preview.truncated,
            redacted: diff_preview.redacted,
            backup_paths,
        })
    }

    pub fn apply_patch(&self, patch: &str) -> Result<WorkspacePatchResult, String> {
        let parsed = ParsedPatch::parse(patch)?;
        let operation_count = parsed.ops.len();
        let mut pending_writes = Vec::new();

        for op in parsed.ops {
            pending_writes.push(self.prepare_patch_write(op)?);
        }

        let mut changed_files = Vec::new();
        let mut backup_paths = Vec::new();
        let mut diff_parts = Vec::new();
        let mut diff_truncated = false;

        for write in pending_writes {
            let backup = self.backup_existing(&write.file, write.previous.as_deref())?;
            backup_paths.extend(backup);
            if let Some(parent) = write.file.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    format!(
                        "apply_patch_mkdir_failed path={} error={error}",
                        parent.display()
                    )
                })?;
            }
            fs::write(&write.file, &write.next).map_err(|error| {
                format!(
                    "apply_patch_write_failed path={} error={error}",
                    write.file.display()
                )
            })?;
            changed_files.push(write.file.display().to_string());
            let preview =
                build_write_diff_preview(&write.path, write.previous.as_deref(), &write.next);
            diff_parts.push(format_patch_preview(write.kind, &write.path, &preview.text));
            diff_truncated |= preview.truncated;
        }

        let diff_preview = join_diff_preview(&diff_parts);
        Ok(WorkspacePatchResult {
            changed_files,
            backup_paths,
            diff_preview,
            diff_truncated,
            operation_count,
        })
    }

    fn prepare_patch_write(&self, op: PatchOp) -> Result<PendingPatchWrite, String> {
        match op {
            PatchOp::Add { path, content } => {
                let file = self.resolve_workspace_path(&path)?;
                if file.exists() {
                    return Err(format!("apply_patch_add_exists path={}", file.display()));
                }
                Ok(PendingPatchWrite {
                    kind: "add",
                    path,
                    file,
                    previous: None,
                    next: content,
                })
            }
            PatchOp::Delete { path } => {
                let file = self.resolve_workspace_path(&path)?;
                Err(format!(
                    "apply_patch_delete_not_allowed path={} reason=deletion_requires_explicit_operator_approval",
                    file.display()
                ))
            }
            PatchOp::Update {
                path,
                move_to,
                hunks,
            } => {
                let source = self.resolve_workspace_path(&path)?;
                let target_path = move_to.as_ref().unwrap_or(&path).trim().to_string();
                let target = self.resolve_workspace_path(&target_path)?;
                if target != source {
                    return Err(format!(
                        "apply_patch_move_not_allowed source={} target={} reason=move_requires_explicit_operator_approval",
                        source.display(),
                        target.display()
                    ));
                }
                let previous = fs::read_to_string(&source).map_err(|error| {
                    format!(
                        "apply_patch_read_existing_failed path={} error={error}",
                        source.display()
                    )
                })?;
                let updated_lines = apply_hunks(&previous, &hunks).map_err(|error| {
                    format!(
                        "apply_patch_hunk_failed path={} error={error}",
                        source.display()
                    )
                })?;
                Ok(PendingPatchWrite {
                    kind: "update",
                    path,
                    file: source,
                    previous: Some(previous),
                    next: join_lines(&updated_lines),
                })
            }
        }
    }

    fn backup_existing(&self, path: &Path, content: Option<&str>) -> Result<Vec<String>, String> {
        let Some(content) = content else {
            return Ok(Vec::new());
        };
        let relative = path.strip_prefix(&self.workspace_root).map_err(|_| {
            format!(
                "backup_path_escape path={} workspace_root={}",
                path.display(),
                self.workspace_root.display()
            )
        })?;
        let stamp = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        let backup_path = self
            .audit_root
            .join(stamp)
            .join(relative)
            .with_extension("bak");
        if let Some(parent) = backup_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "backup_dir_create_failed path={} error={error}",
                    parent.display()
                )
            })?;
        }
        fs::write(&backup_path, content).map_err(|error| {
            format!(
                "backup_write_failed path={} error={error}",
                backup_path.display()
            )
        })?;
        Ok(vec![backup_path.display().to_string()])
    }

    fn resolve_workspace_path(&self, raw_path: &str) -> Result<PathBuf, String> {
        let candidate = if raw_path.trim().is_empty() {
            self.workspace_root.clone()
        } else {
            let path = Path::new(raw_path.trim());
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                self.workspace_root.join(path)
            }
        };

        let normalized_root = fs::canonicalize(&self.workspace_root).map_err(|error| {
            format!(
                "workspace_root_invalid path={} error={error}",
                self.workspace_root.display()
            )
        })?;
        let normalized_candidate = resolve_candidate_preserving_existing_symlinks(&candidate)?;

        if !normalized_candidate.starts_with(&normalized_root) {
            return Err(format!(
                "path_outside_workspace path={} workspace_root={}",
                normalized_candidate.display(),
                normalized_root.display()
            ));
        }

        Ok(normalized_candidate)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedPatch {
    ops: Vec<PatchOp>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PatchOp {
    Add {
        path: String,
        content: String,
    },
    Delete {
        path: String,
    },
    Update {
        path: String,
        move_to: Option<String>,
        hunks: Vec<PatchHunk>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingPatchWrite {
    kind: &'static str,
    path: String,
    file: PathBuf,
    previous: Option<String>,
    next: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PatchHunk {
    lines: Vec<PatchLine>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PatchLine {
    Context(String),
    Remove(String),
    Add(String),
    EndOfFile,
}

impl ParsedPatch {
    fn parse(text: &str) -> Result<Self, String> {
        let mut lines = text.lines().peekable();
        let mut ops = Vec::new();
        let mut saw_begin = false;

        while let Some(line) = lines.next() {
            if line.trim().is_empty() {
                continue;
            }
            if line == "*** Begin Patch" {
                saw_begin = true;
                continue;
            }
            if line == "*** End Patch" {
                break;
            }
            if !saw_begin {
                return Err("apply_patch must start with *** Begin Patch".to_string());
            }
            if let Some(path) = line.strip_prefix("*** Add File: ") {
                let content = collect_prefixed_lines(&mut lines, '+');
                ops.push(PatchOp::Add {
                    path: path.trim().to_string(),
                    content,
                });
                continue;
            }
            if let Some(path) = line.strip_prefix("*** Delete File: ") {
                ops.push(PatchOp::Delete {
                    path: path.trim().to_string(),
                });
                continue;
            }
            if let Some(path) = line.strip_prefix("*** Update File: ") {
                let (move_to, hunks) = parse_update_block(&mut lines);
                ops.push(PatchOp::Update {
                    path: path.trim().to_string(),
                    move_to,
                    hunks,
                });
                continue;
            }
            return Err(format!("unsupported patch line: {line}"));
        }

        if !saw_begin {
            return Err("apply_patch must start with *** Begin Patch".to_string());
        }

        Ok(Self { ops })
    }
}

fn parse_update_block<'a, I>(lines: &mut std::iter::Peekable<I>) -> (Option<String>, Vec<PatchHunk>)
where
    I: Iterator<Item = &'a str>,
{
    let mut move_to = None;
    let mut hunks = Vec::new();
    let mut current_lines: Vec<PatchLine> = Vec::new();

    while let Some(&line) = lines.peek() {
        if line == "*** End Patch"
            || line.starts_with("*** Add File: ")
            || line.starts_with("*** Delete File: ")
            || line.starts_with("*** Update File: ")
        {
            break;
        }
        let line = lines.next().unwrap_or_default();
        if let Some(path) = line.strip_prefix("*** Move to: ") {
            move_to = Some(path.trim().to_string());
            continue;
        }
        if line.starts_with("@@") {
            if !current_lines.is_empty() {
                hunks.push(PatchHunk {
                    lines: std::mem::take(&mut current_lines),
                });
            }
            continue;
        }
        if line == "*** End of File" {
            current_lines.push(PatchLine::EndOfFile);
            continue;
        }
        if let Some(rest) = line.strip_prefix('+') {
            current_lines.push(PatchLine::Add(rest.to_string()));
            continue;
        }
        if let Some(rest) = line.strip_prefix('-') {
            current_lines.push(PatchLine::Remove(rest.to_string()));
            continue;
        }
        if let Some(rest) = line.strip_prefix(' ') {
            current_lines.push(PatchLine::Context(rest.to_string()));
            continue;
        }
        current_lines.push(PatchLine::Context(line.to_string()));
    }

    if !current_lines.is_empty() {
        hunks.push(PatchHunk {
            lines: current_lines,
        });
    }

    (move_to, hunks)
}

fn collect_prefixed_lines<'a, I>(lines: &mut std::iter::Peekable<I>, prefix: char) -> String
where
    I: Iterator<Item = &'a str>,
{
    let mut content = String::new();
    while let Some(&line) = lines.peek() {
        if line == "*** End Patch"
            || line.starts_with("*** Add File: ")
            || line.starts_with("*** Delete File: ")
            || line.starts_with("*** Update File: ")
        {
            break;
        }
        let line = lines.next().unwrap_or_default();
        let text = line.strip_prefix(prefix).unwrap_or(line);
        content.push_str(text);
        content.push('\n');
    }
    content
}

fn apply_hunks(original: &str, hunks: &[PatchHunk]) -> Result<Vec<String>, String> {
    let original_lines = original
        .lines()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let mut input_index = 0usize;
    let mut output = Vec::new();

    for hunk in hunks {
        for line in &hunk.lines {
            match line {
                PatchLine::Context(text) => {
                    consume_until_match(&original_lines, &mut input_index, &mut output, text)
                        .map_err(|error| error.to_string())?;
                    output.push(text.clone());
                    input_index += 1;
                }
                PatchLine::Remove(text) => {
                    consume_until_match(&original_lines, &mut input_index, &mut output, text)
                        .map_err(|error| error.to_string())?;
                    input_index += 1;
                }
                PatchLine::Add(text) => output.push(text.clone()),
                PatchLine::EndOfFile => break,
            }
        }
    }

    output.extend(original_lines[input_index..].iter().cloned());
    Ok(output)
}

fn consume_until_match(
    original_lines: &[String],
    input_index: &mut usize,
    output: &mut Vec<String>,
    expected: &str,
) -> Result<(), &'static str> {
    if let Some(current) = original_lines.get(*input_index) {
        if current == expected {
            return Ok(());
        }
    } else {
        return Err("patch hunk exceeds file length");
    }

    if let Some(relative) = original_lines[*input_index + 1..]
        .iter()
        .position(|line| line == expected)
    {
        let match_index = *input_index + 1 + relative;
        output.extend(original_lines[*input_index..match_index].iter().cloned());
        *input_index = match_index;
        Ok(())
    } else {
        Err("patch hunk context mismatch")
    }
}

fn join_lines(lines: &[String]) -> String {
    if lines.is_empty() {
        String::new()
    } else {
        let mut content = lines.join("\n");
        content.push('\n');
        content
    }
}

fn format_patch_preview(kind: &str, path: &str, preview: &str) -> String {
    format!("{} path={} preview=\n{}", kind, path, preview)
}

fn join_diff_preview(parts: &[String]) -> String {
    if parts.is_empty() {
        "none".to_string()
    } else {
        parts.join("\n")
    }
}

fn build_write_diff_preview(path: &str, previous: Option<&str>, next: &str) -> DiffPreview {
    if previous == Some(next) {
        return DiffPreview {
            text: "unchanged".to_string(),
            truncated: false,
            redacted: false,
        };
    }

    let before_lines = previous.unwrap_or("").lines().collect::<Vec<_>>();
    let after_lines = next.lines().collect::<Vec<_>>();
    let max_len = before_lines.len().max(after_lines.len());
    let mut preview = String::from("--- before\n+++ after\n");
    let mut truncated = false;
    let mut emitted = 0usize;

    for index in 0..max_len {
        let before = before_lines.get(index).copied();
        let after = after_lines.get(index).copied();
        if before == after {
            continue;
        }
        if let Some(line) = before {
            push_diff_line(&mut preview, '-', line);
            emitted += 1;
        }
        if let Some(line) = after {
            push_diff_line(&mut preview, '+', line);
            emitted += 1;
        }
        if emitted >= 80 || preview.len() >= 4_000 {
            truncated = index + 1 < max_len;
            break;
        }
    }

    if preview.len() > 4_000 {
        preview = truncate_text(&preview, 4_000).text;
        truncated = true;
    }

    let redaction = redact_sensitive_text(path, &preview);
    DiffPreview {
        text: redaction.text,
        truncated,
        redacted: redaction.redacted,
    }
}

fn push_diff_line(preview: &mut String, prefix: char, line: &str) {
    preview.push(prefix);
    preview.push_str(line);
    preview.push('\n');
}

fn truncate_text(value: &str, max_len: usize) -> TruncatedText {
    if value.len() <= max_len {
        return TruncatedText {
            text: value.to_string(),
            truncated: false,
        };
    }
    let mut truncated = value
        .chars()
        .take(max_len.saturating_sub(1))
        .collect::<String>();
    truncated.push('…');
    TruncatedText {
        text: truncated,
        truncated: true,
    }
}

fn count_lines(value: &str) -> usize {
    if value.is_empty() {
        0
    } else {
        value.lines().count()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TruncatedText {
    text: String,
    truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DiffPreview {
    text: String,
    truncated: bool,
    redacted: bool,
}
