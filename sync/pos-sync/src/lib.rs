//! Sync identities and conflict decisions, without network I/O.
//!
//! A pending local change is *not* a server acknowledgement. Nothing here
//! uploads data, maps local IDs to remote records, or resolves conflicts.

use pos_entities::LocalRecordId;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordKind {
    Task,
    Project,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeOperation {
    Upsert,
    Delete,
}

/// An immutable, per-write snapshot from the SQLite outbox.
///
/// Sequence numbers are local to a database, and MUST NOT be used as
/// globally unique operation IDs or as proof of successful delivery.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PendingChange {
    pub seq: i64,
    pub format_version: u16,
    pub kind: RecordKind,
    pub sync_id: LocalRecordId,
    pub operation: ChangeOperation,
    /// Full snapshot when upserting; None is a deletion tombstone.
    pub payload: Option<Value>,
    pub created_at: String,
}

impl PendingChange {
    pub fn is_valid(&self) -> bool {
        self.seq > 0
            && self.format_version == 1
            && match self.operation {
                ChangeOperation::Upsert => self.payload.as_ref().is_some_and(Value::is_object),
                ChangeOperation::Delete => self.payload.is_none(),
            }
    }
}

/// Whether a queued write is safe to attempt against an authoritative
/// server record with optimistic concurrency. An unknown base revision is
/// never treated as authorization to replace an existing server record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UploadDecision {
    SafeToAttempt,
    Conflict,
}

/// A server write must use the expected server revision as its precondition.
/// This pure guard is not a substitute for the server's transactional version
/// check and is *not* an automatic conflict resolver.
pub fn compare_revisions(expected: Option<i64>, observed: Option<i64>) -> UploadDecision {
    match (expected, observed) {
        (None, None) => UploadDecision::SafeToAttempt,
        (Some(expected), Some(observed)) if expected > 0 && expected == observed => {
            UploadDecision::SafeToAttempt
        }
        _ => UploadDecision::Conflict,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_overwrite_a_divergent_or_unknown_server_version() {
        assert_eq!(compare_revisions(None, None), UploadDecision::SafeToAttempt);
        assert_eq!(compare_revisions(None, Some(1)), UploadDecision::Conflict);
        assert_eq!(compare_revisions(Some(3), None), UploadDecision::Conflict);
        assert_eq!(
            compare_revisions(Some(3), Some(4)),
            UploadDecision::Conflict
        );
        assert_eq!(
            compare_revisions(Some(0), Some(0)),
            UploadDecision::Conflict
        );
        assert_eq!(
            compare_revisions(Some(3), Some(3)),
            UploadDecision::SafeToAttempt
        );
    }

    #[test]
    fn pending_data_is_distinct_from_a_tombstone() {
        let id = LocalRecordId::parse("bb981f0f-c95f-4ed5-80bb-989304bad804").unwrap();
        let base = PendingChange {
            seq: 1,
            format_version: 1,
            kind: RecordKind::Task,
            sync_id: id,
            operation: ChangeOperation::Upsert,
            payload: Some(serde_json::json!({"title":"Offline"})),
            created_at: "2026-11-14T00:00:00Z".into(),
        };
        assert!(base.is_valid());
        let mut tombstone = base.clone();
        tombstone.operation = ChangeOperation::Delete;
        assert!(!tombstone.is_valid());
        tombstone.payload = None;
        assert!(tombstone.is_valid());
        let encoded = serde_json::to_string(&tombstone).unwrap();
        let recovered: PendingChange = serde_json::from_str(&encoded).unwrap();
        assert_eq!(recovered, tombstone);
    }
}
