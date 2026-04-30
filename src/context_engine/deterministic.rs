use super::{
    ContextBudget, ContextEngine, ContextPackError, ContextPacker, ContextSegment, PackedContext,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeterministicContextEngine {
    budget: ContextBudget,
}

impl DeterministicContextEngine {
    pub fn new(budget: ContextBudget) -> Self {
        Self { budget }
    }
}

impl ContextEngine for DeterministicContextEngine {
    fn kind(&self) -> &'static str {
        "deterministic_budget"
    }

    fn pack(&self, segments: Vec<ContextSegment>) -> Result<PackedContext, ContextPackError> {
        ContextPacker::new(self.budget.clone()).pack(segments)
    }
}
