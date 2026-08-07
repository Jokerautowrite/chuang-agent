use std::collections::BTreeMap;

use super::{MemoryQuery, MemoryRecord, MemoryStore, MemoryStoreError, SearchHit};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InMemoryMemoryStore {
    records: BTreeMap<String, MemoryRecord>,
}

impl InMemoryMemoryStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl MemoryStore for InMemoryMemoryStore {
    fn put(&mut self, record: MemoryRecord) -> Result<(), MemoryStoreError> {
        if self.records.contains_key(&record.id) {
            return Err(MemoryStoreError::DuplicateId {
                id: record.id.clone(),
            });
        }
        self.records.insert(record.id.clone(), record);
        Ok(())
    }

    fn get(&self, id: &str) -> Result<Option<MemoryRecord>, MemoryStoreError> {
        Ok(self.records.get(id).cloned())
    }

    fn search(&self, query: &MemoryQuery) -> Result<Vec<SearchHit>, MemoryStoreError> {
        if query.limit == 0 {
            return Err(MemoryStoreError::InvalidQuery("limit_must_be_positive"));
        }

        let mut hits = Vec::new();
        for record in self.records.values() {
            let metadata_match = query
                .metadata
                .iter()
                .all(|(key, value)| record.metadata.get(key) == Some(value));

            let text_score = query
                .text
                .as_ref()
                .and_then(|text| super::text_match_score(text, &record.content))
                .unwrap_or(0);
            let text_match = query.text.is_none() || text_score > 0;

            if text_match && metadata_match {
                hits.push(SearchHit {
                    record: record.clone(),
                    score: text_score,
                });
            }
        }

        hits.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| a.record.id.cmp(&b.record.id))
        });
        hits.truncate(query.limit);
        Ok(hits)
    }

    fn delete(&mut self, id: &str) -> Result<(), MemoryStoreError> {
        self.records.remove(id);
        Ok(())
    }

    fn expire(&mut self, now: &str) -> Result<usize, MemoryStoreError> {
        let expired_ids: Vec<String> = self
            .records
            .iter()
            .filter_map(|(id, record)| match record.expires_at.as_deref() {
                Some(expires_at) if expires_at <= now => Some(id.clone()),
                _ => None,
            })
            .collect();

        let count = expired_ids.len();
        for id in expired_ids {
            self.records.remove(&id);
        }
        Ok(count)
    }
}
