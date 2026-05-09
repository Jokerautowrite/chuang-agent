use crate::context_engine::{ContextSegment, SegmentSource};

pub const DEFAULT_CAPABILITY_PRIMER_ID: &str = "system-capabilities";
pub const DEFAULT_CAPABILITY_PRIMER_KIND: &str = "capability_primer";

pub fn capability_primer_content() -> &'static str {
    include_str!("../assets/capability_primer.txt").trim()
}

pub fn capability_primer_text() -> String {
    capability_primer_content().to_string()
}

pub fn capability_primer_segment() -> ContextSegment {
    let content = capability_primer_content().to_string();
    let now = default_timestamp();
    ContextSegment {
        id: DEFAULT_CAPABILITY_PRIMER_ID.to_string(),
        source: SegmentSource::Identity,
        tokens: Some(content.chars().count().min(u16::MAX as usize) as u16),
        content,
        priority: 254,
        created_at: now,
        last_accessed: now,
        metadata: std::collections::HashMap::from([(
            "kind".to_string(),
            DEFAULT_CAPABILITY_PRIMER_KIND.to_string(),
        )]),
    }
}

fn default_timestamp() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339("2026-04-30T00:00:00Z")
        .expect("static runtime timestamp should parse")
        .with_timezone(&chrono::Utc)
}
