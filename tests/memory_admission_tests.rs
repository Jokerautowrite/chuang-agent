use chuang_agent::memory_admission::{
    preview_chars, MemoryEntryView, TextMemoryAdmission, TextMemoryAdmissionDecision,
    DEFAULT_MEMORY_WRITE_MAX_CHARS,
};

#[test]
fn text_memory_admission_accepts_content_within_limit() {
    let admission = TextMemoryAdmission::new(10);

    let decision = admission.evaluate("12345", Vec::new());

    assert_eq!(decision, TextMemoryAdmissionDecision::Accepted);
}

#[test]
fn text_memory_admission_rejects_content_over_limit_with_existing_entries() {
    let admission = TextMemoryAdmission::new(4);
    let existing = vec![MemoryEntryView {
        id: "mem-1".to_string(),
        content_preview: "旧记忆".to_string(),
        chars: 3,
    }];

    let decision = admission.evaluate("12345", existing.clone());

    assert_eq!(
        decision,
        TextMemoryAdmissionDecision::Rejected {
            limit_chars: 4,
            attempted_chars: 5,
            existing_entries: existing,
        }
    );
}

#[test]
fn text_memory_admission_preview_truncates_by_chars() {
    assert_eq!(preview_chars("abcdef", 3), "abc");
    assert_eq!(preview_chars("旧记忆条目", 2), "旧记");
}

#[test]
fn text_memory_admission_default_limit_matches_mvp_policy() {
    assert_eq!(DEFAULT_MEMORY_WRITE_MAX_CHARS, 2200);
}
