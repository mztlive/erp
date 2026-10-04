//! Customer audit, party-fact and account-fact adapters.

use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use entity_core::BaseModel;
use erp_audit::{AuditActorLogs, AuditLog, AuditLogData, prepare_business_log};
use erp_core::ids::PartyId;
use erp_customer::{
    AccountFactPort, CustomerAuditPort, PartyFactPort, PartyIdentityFact, PreparedCustomerAudit,
};
use erp_identity::AccessControlExt;
use erp_identity::repository::prelude::*;
use erp_party::PartyExt;
use erp_party::repository::prelude::*;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction};

use crate::audit::persist_log;

/// MongoDB adapter that converts customer audit facts into `erp-audit` writes.
#[derive(Clone)]
pub struct MongoCustomerAudit {
    db: Database,
}

impl MongoCustomerAudit {
    /// Bind the adapter to `db`.
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Wrap the adapter as a shared port.
    pub fn shared(db: Database) -> Arc<dyn CustomerAuditPort> {
        Arc::new(Self::new(db))
    }
}

#[async_trait]
impl CustomerAuditPort for MongoCustomerAudit {
    fn resource_log(
        &self,
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
    ) -> erp_customer::Result<PreparedCustomerAudit> {
        let log = actor.resource_log(action, resource_type, resource_id).map_err(map_audit_to_customer)?;
        Ok(prepared_customer_audit(&log))
    }

    async fn persist(
        &self,
        audit: &PreparedCustomerAudit,
        executor: &mut dyn Executor,
    ) -> erp_customer::Result<()> {
        let log = audit_log_from_customer(audit).map_err(map_audit_to_customer)?;
        persist_log(&self.db, &log, executor).await.map_err(map_audit_to_customer)?;
        Ok(())
    }
}

/// MongoDB adapter that reads Party identity facts for customer commands.
#[derive(Clone)]
pub struct MongoCustomerPartyFacts {
    db: Database,
}

impl MongoCustomerPartyFacts {
    /// Bind the adapter to `db`.
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Wrap the adapter as a shared port.
    pub fn shared(db: Database) -> Arc<dyn PartyFactPort> {
        Arc::new(Self::new(db))
    }
}

#[async_trait]
impl PartyFactPort for MongoCustomerPartyFacts {
    async fn ensure_exists(&self, party_id: &PartyId) -> erp_customer::Result<()> {
        self.db
            .parties()
            .find_party(party_id, &mut NoTransaction)
            .await
            .map_err(erp_customer::Error::from)?
            .ok_or_else(|| erp_customer::Error::NotFound("主体不存在".to_string()))?;
        Ok(())
    }

    async fn identities_by_ids(&self, party_ids: &[PartyId]) -> erp_customer::Result<Vec<PartyIdentityFact>> {
        let (parties, revisions) = self
            .db
            .party()
            .list_with_current_revisions(party_ids, &mut NoTransaction)
            .await
            .map_err(erp_customer::Error::from)?;
        let revisions: HashMap<String, erp_party::PartyRevision> =
            revisions.into_iter().map(|revision| (revision.base.id.clone(), revision)).collect();
        Ok(parties
            .into_iter()
            .map(|party| {
                let current = party.stable.current_revision_id.as_ref().and_then(|id| revisions.get(id));
                PartyIdentityFact {
                    party_id: party.base.id.clone(),
                    party_no: party.party_no.clone(),
                    legal_name: current.map(|revision| revision.legal_name.clone()),
                    short_name: current.and_then(|revision| revision.short_name.clone()),
                }
            })
            .collect())
    }

    async fn matching_ids_by_name(&self, keyword: &str) -> erp_customer::Result<Vec<String>> {
        Ok(self
            .db
            .party()
            .matching_current_party_ids(keyword, &mut NoTransaction)
            .await
            .map_err(erp_customer::Error::from)?
            .into_iter()
            .map(|id| id.to_string())
            .collect())
    }
}

/// MongoDB adapter that reads account login facts for customer commands.
#[derive(Clone)]
pub struct MongoCustomerAccountFacts {
    db: Database,
}

impl MongoCustomerAccountFacts {
    /// Bind the adapter to `db`.
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Wrap the adapter as a shared port.
    pub fn shared(db: Database) -> Arc<dyn AccountFactPort> {
        Arc::new(Self::new(db))
    }
}

#[async_trait]
impl AccountFactPort for MongoCustomerAccountFacts {
    async fn ensure_can_login(&self, user_id: &str) -> erp_customer::Result<()> {
        let account = self
            .db
            .accounts()
            .find_account(user_id, &mut NoTransaction)
            .await
            .map_err(erp_customer::Error::from)?
            .ok_or_else(|| erp_customer::Error::NotFound("负责销售账号不存在".to_string()))?;
        account.ensure_can_login().map_err(|error| erp_customer::Error::BusinessLogicError(error.to_string()))
    }

    async fn names_by_ids(&self, account_ids: &[String]) -> erp_customer::Result<HashMap<String, String>> {
        self.db
            .accounts()
            .names_by_ids(account_ids, &mut NoTransaction)
            .await
            .map_err(erp_customer::Error::from)
    }
}

fn prepared_customer_audit(log: &AuditLog) -> PreparedCustomerAudit {
    PreparedCustomerAudit::from_validated(
        &log.base,
        log.actor_id.clone(),
        log.actor_account.clone(),
        log.actor_type,
        log.action.clone(),
        log.resource_type.clone(),
        log.resource_id.clone(),
        log.success,
        log.message.clone(),
    )
    .with_actor_name_snapshot(
        log.structured_event.as_ref().and_then(|event| event.actor_name_snapshot.clone()),
    )
    .with_request_id(log.structured_event.as_ref().and_then(|event| event.request_id.clone()))
    .with_event_sequence(
        log.structured_event.as_ref().map(|event| event.event_sequence).unwrap_or(NonZeroU32::MIN),
    )
}

fn audit_log_from_customer(audit: &PreparedCustomerAudit) -> erp_audit::Result<AuditLog> {
    let mut log = AuditLog::new(
        audit.id.clone(),
        AuditLogData {
            actor_id: audit.actor_id.clone(),
            actor_account: audit.actor_account.clone(),
            actor_type: audit.actor_type,
            action: audit.action.clone(),
            resource_type: audit.resource_type.clone(),
            resource_id: audit.resource_id.clone(),
            success: audit.success,
            message: audit.message.clone(),
        },
    )?;
    log.base = BaseModel {
        id: audit.id.clone(),
        version: audit.version,
        created_at: audit.created_at,
        updated_at: audit.updated_at,
        deleted_at: audit.deleted_at,
    };
    prepare_business_log(&log)?
        .with_actor_name_snapshot(audit.actor_name_snapshot.clone())?
        .with_request_id(audit.request_id.clone())?
        .with_event_sequence(audit.event_sequence.get())
}

fn map_audit_to_customer(error: erp_audit::Error) -> erp_customer::Error {
    match error {
        erp_audit::Error::Internal(message) => erp_customer::Error::Internal(message),
        erp_audit::Error::NotFound(message) => erp_customer::Error::NotFound(message),
        erp_audit::Error::ValidationError(message) => erp_customer::Error::ValidationError(message),
        erp_audit::Error::BusinessLogicError(message) => erp_customer::Error::BusinessLogicError(message),
        erp_audit::Error::ConflictError(message) => erp_customer::Error::ConflictError(message),
        erp_audit::Error::ReceiptDuplicate(error) => erp_customer::Error::ReceiptDuplicate(error),
        erp_audit::Error::TransientTransaction(error) => erp_customer::Error::TransientTransaction(error),
        erp_audit::Error::Forbidden(message) => erp_customer::Error::Forbidden(message),
        erp_audit::Error::Unauthenticated(message) => erp_customer::Error::Unauthenticated(message),
        erp_audit::Error::Logic(error) => erp_customer::Error::Logic(error),
        erp_audit::Error::OutcomeUnknown(error) => erp_customer::Error::OutcomeUnknown(error),
        erp_audit::Error::RepositoryError(error) => erp_customer::Error::RepositoryError(error),
    }
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;

    use super::*;

    fn actor() -> AuditActor {
        AuditActor::new("actor".into(), "login".into(), AccountKind::Admin)
    }

    #[test]
    fn prepared_audit_round_trip_freezes_name_and_original_event_metadata() {
        let mut log = actor()
            .with_actor_name_snapshot(Some("周晓彤".into()))
            .unwrap()
            .with_request_id(Some("request-original".into()))
            .unwrap()
            .resource_log("customer.update", "customer", "object-1".into())
            .unwrap()
            .with_event_sequence(7)
            .unwrap();
        log.base = BaseModel {
            id: "audit-original".into(),
            version: 7,
            created_at: 11,
            updated_at: 19,
            deleted_at: 23,
        };
        log.structured_event.as_mut().unwrap().occurred_at = log.base.created_at;
        let log = prepare_business_log(&log).unwrap();
        let prepared = prepared_customer_audit(&log);
        assert_eq!(prepared.actor_name_snapshot.as_deref(), Some("周晓彤"));
        assert_eq!(prepared.request_id.as_deref(), Some("request-original"));
        assert_eq!(prepared.event_sequence.get(), 7);
        let renamed_actor = actor()
            .with_actor_name_snapshot(Some("新名称".into()))
            .unwrap()
            .with_request_id(Some("request-next".into()))
            .unwrap();
        assert_eq!(renamed_actor.actor_name_snapshot(), Some("新名称"));
        assert_eq!(renamed_actor.request_id(), Some("request-next"));
        let restored = audit_log_from_customer(&prepared).unwrap();
        assert_eq!(restored, log);
        assert!(restored.message.as_deref().unwrap().contains("周晓彤"));
        assert!(restored.message.as_deref().unwrap().contains("客户"));
    }

    #[test]
    fn prepared_audit_preserves_unknown_name_and_discards_private_body() {
        let log = actor().resource_log("customer.update", "customer", "object-1".into()).unwrap();
        let mut prepared = prepared_customer_audit(&log);
        prepared.message = Some("password bank-account ciphertext private-request".into());
        let restored = audit_log_from_customer(&prepared).unwrap();
        let event = restored.structured_event.as_ref().unwrap();
        assert_eq!(event.actor_name_snapshot, None);
        assert_eq!(event.request_id, None);
        assert_eq!(event.event_sequence, NonZeroU32::MIN);
        assert_eq!(event.actor_account, "login");
        let serialized = serde_json::to_string(&restored).unwrap();
        assert!(!serialized.contains("password"));
        assert!(!serialized.contains("bank-account"));
        assert!(!serialized.contains("ciphertext"));
        assert!(!serialized.contains("private-request"));
        assert!(!serialized.contains("actor_name_snapshot"));
        assert!(!serialized.contains("request_id"));
        assert!(restored.message.as_deref().unwrap().contains("login"));
    }

    #[test]
    fn prepared_audit_rejects_invalid_request_id() {
        let log = actor().resource_log("customer.update", "customer", "object-1".into()).unwrap();
        for request_id in ["unsafe\nrequest".into(), "x".repeat(129)] {
            let prepared = prepared_customer_audit(&log).with_request_id(Some(request_id));
            assert!(audit_log_from_customer(&prepared).unwrap_err().to_string().contains("请求编号"));
        }
    }

    #[test]
    fn prepared_audit_rejects_invalid_name_snapshot() {
        let log = actor().resource_log("customer.update", "customer", "object-1".into()).unwrap();
        let prepared = prepared_customer_audit(&log).with_actor_name_snapshot(Some("unsafe\nname".into()));
        assert!(audit_log_from_customer(&prepared).is_err());
    }
}
