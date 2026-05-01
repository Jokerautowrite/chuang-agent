use super::{
    ContextBudget, ContextEngine, ContextPackError, ContextPacker, ContextSegment, PackedContext,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryCompressionContextEngine {
    budget: ContextBudget,
}

impl SummaryCompressionContextEngine {
    pub fn new(budget: ContextBudget) -> Self {
        Self { budget }
    }
}

impl ContextEngine for SummaryCompressionContextEngine {
    fn kind(&self) -> &'static str {
        "summary_compression"
    }

    fn pack(&self, segments: Vec<ContextSegment>) -> Result<PackedContext, ContextPackError> {
        ContextPacker::new(self.budget.clone()).pack(segments)
    }
}
