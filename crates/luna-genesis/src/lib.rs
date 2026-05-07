use chrono::{DateTime, Utc};
use luna_core::{LunaError, Result};
use luna_ledger::RawEvent;
use luna_node::MemoryNode;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenesisCertificate {
    pub id: String,
    pub node_id: String,
    pub source_event_id: String,
    pub source_event_hash: String,
    pub created_at: DateTime<Utc>,
    pub certificate_hash: String,
}

impl GenesisCertificate {
    pub fn for_node(
        id: impl Into<String>,
        node: &MemoryNode,
        event: &RawEvent,
    ) -> Result<Self> {
        if node.source_event_id != event.id {
            return Err(LunaError::new(format!(
                "node {} source event {} does not match event {}",
                node.id, node.source_event_id, event.id
            )));
        }
        if node.source_event_hash != event.hash {
            return Err(LunaError::new(format!(
                "node {} source hash does not match event {}",
                node.id, event.id
            )));
        }

        let id = require_non_empty("genesis certificate id", id.into())?;
        let created_at = Utc::now();
        let certificate_hash = certificate_hash(&id, &node.id, &event.id, &event.hash, created_at)?;
        Ok(Self {
            id,
            node_id: node.id.clone(),
            source_event_id: event.id.clone(),
            source_event_hash: event.hash.clone(),
            created_at,
            certificate_hash,
        })
    }

    pub fn verify_hash(&self) -> Result<()> {
        let recomputed = certificate_hash(
            &self.id,
            &self.node_id,
            &self.source_event_id,
            &self.source_event_hash,
            self.created_at,
        )?;
        if recomputed == self.certificate_hash {
            Ok(())
        } else {
            Err(LunaError::new(format!(
                "genesis certificate {} hash mismatch",
                self.id
            )))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GenesisRegistry {
    certificates: BTreeMap<String, GenesisCertificate>,
    certificate_by_node: BTreeMap<String, String>,
}

impl GenesisRegistry {
    pub fn insert(&mut self, certificate: GenesisCertificate) -> Result<()> {
        certificate.verify_hash()?;
        if self.certificates.contains_key(&certificate.id) {
            return Err(LunaError::new(format!(
                "genesis certificate {} already exists",
                certificate.id
            )));
        }
        if self
            .certificate_by_node
            .contains_key(&certificate.node_id)
        {
            return Err(LunaError::new(format!(
                "node {} already has a genesis certificate",
                certificate.node_id
            )));
        }
        self.certificate_by_node
            .insert(certificate.node_id.clone(), certificate.id.clone());
        self.certificates
            .insert(certificate.id.clone(), certificate);
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&GenesisCertificate> {
        self.certificates.get(id)
    }

    pub fn certificate_for_node(&self, node_id: &str) -> Option<&GenesisCertificate> {
        self.certificate_by_node
            .get(node_id)
            .and_then(|certificate_id| self.certificates.get(certificate_id))
    }

    pub fn certificates(&self) -> &BTreeMap<String, GenesisCertificate> {
        &self.certificates
    }
}

fn certificate_hash(
    id: &str,
    node_id: &str,
    source_event_id: &str,
    source_event_hash: &str,
    created_at: DateTime<Utc>,
) -> Result<String> {
    let canonical = serde_json::to_vec(&(
        id,
        node_id,
        source_event_id,
        source_event_hash,
        created_at.to_rfc3339(),
    ))
    .map_err(|err| LunaError::new(err.to_string()))?;
    let mut hasher = Sha256::new();
    hasher.update(canonical);
    let digest = hasher.finalize();
    Ok(format!("{digest:x}"))
}

fn require_non_empty(field: &str, value: String) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(LunaError::new(format!("{field} cannot be empty")))
    } else {
        Ok(trimmed.to_string())
    }
}
