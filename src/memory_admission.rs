//! `memory_admission` 模块。公开接口：struct MemoryEntryView, TextMemoryAdmission；enum TextMemoryAdmissionDecision；fn new, evaluate, preview_chars；const DEFAULT_MEMORY_WRITE_MAX_CHARS。

pub const DEFAULT_MEMORY_WRITE_MAX_CHARS: usize = 2200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryEntryView {
    pub id: String,
    pub content_preview: String,
    pub chars: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextMemoryAdmission {
    pub max_chars: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextMemoryAdmissionDecision {
    Accepted,
    Rejected {
        limit_chars: usize,
        attempted_chars: usize,
        existing_entries: Vec<MemoryEntryView>,
    },
}

impl TextMemoryAdmission {
    pub fn new(max_chars: usize) -> Self {
        Self { max_chars }
    }

    pub fn evaluate(
        &self,
        content: &str,
        existing_entries: Vec<MemoryEntryView>,
    ) -> TextMemoryAdmissionDecision {
        let attempted_chars = content.chars().count();
        if attempted_chars > self.max_chars {
            return TextMemoryAdmissionDecision::Rejected {
                limit_chars: self.max_chars,
                attempted_chars,
                existing_entries,
            };
        }

        TextMemoryAdmissionDecision::Accepted
    }
}

pub fn preview_chars(content: &str, limit: usize) -> String {
    content.chars().take(limit).collect()
}
