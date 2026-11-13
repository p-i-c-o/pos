//! Shared cross-module identifiers and relationships.
//!
//! LocalRecordId is intentionally distinct from server-side UUIDs. Even
//! when their underlying representation matches, they have different
//! lifetimes, ownership and mapping semantics.

use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};
use uuid::Uuid;

/// Stable ID assigned once by the local POS SQLite database.
///
/// It is not a server record UUID and must never be sent as an authoritative
/// server-side identifier without an explicit, verified identity mapping.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LocalRecordId(Uuid);

impl LocalRecordId {
    pub fn parse(value: &str) -> Result<Self, uuid::Error> {
        Uuid::parse_str(value).map(Self)
    }

    pub fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl FromStr for LocalRecordId {
    type Err = uuid::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl fmt::Display for LocalRecordId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_ids_roundtrip_as_uuids_without_becoming_server_ids() {
        let id = LocalRecordId::parse("178c43ef-ff98-477e-b976-1005492ff51a").unwrap();
        assert_eq!(id.to_string(), "178c43ef-ff98-477e-b976-1005492ff51a");
        assert_eq!(id.as_uuid().get_version_num(), 4);
        assert_eq!(
            serde_json::to_string(&id).unwrap(),
            "\"178c43ef-ff98-477e-b976-1005492ff51a\""
        );
        assert_eq!(
            serde_json::from_str::<LocalRecordId>(&serde_json::to_string(&id).unwrap()).unwrap(),
            id
        );
        assert!(LocalRecordId::parse("not-an-uuid").is_err());
    }
}
