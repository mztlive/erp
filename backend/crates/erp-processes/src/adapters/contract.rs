//! Contract customer, identity, attachment and audit adapters.

use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use entity_core::BaseModel;
use erp_audit::{AuditActorLogs, AuditLog, AuditLogData, prepare_business_log};
use erp_contract::{
    AccountNamePort, ContractAssignmentFact, ContractAuditPort, ContractParticipantPort, ContractScopePorts,
    ContractService, CustomerAccountFact, CustomerAssignmentFactsPort, CustomerFactsPort,
    FailClosedContractDataScopePort, FailClosedContractParticipantPort, FileAssetFact, FileAssetFactsPort,
    PreparedContractAudit,
};
use erp_core::common::time::BusinessDate;
use erp_core::ids::{CustomerAccountId, FileAssetId};
use erp_customer::repository::prelude::*;
use erp_customer::{AssignmentRole, CustomerExt};
use erp_identity::repository::prelude::*;
use erp_identity::{AccessControlExt, SharedRbacService};
use erp_support::FileAssetExt;
use erp_workflow::DocumentRegistryExt;
use erp_workflow::repository::prelude::*;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction};

use super::contract_data_scope::MongoContractDataScope;
use crate::audit::persist_log;

/// MongoDB adapter that converts contract audit facts into `erp-audit` writes.
#[derive(Clone)]
pub struct MongoContractAudit {
    db: Database,
}

impl MongoContractAudit {
    /// Bind the adapter to `db`.
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Wrap the adapter as a shared port.
    pub fn shared(db: Database) -> Arc<dyn ContractAuditPort> {
        Arc::new(Self::new(db))
    }
}

#[async_trait]
impl ContractAuditPort for MongoContractAudit {
    fn resource_log(
        &self,
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
    ) -> erp_contract::Result<PreparedContractAudit> {
        let log = actor.resource_log(action, resource_type, resource_id).map_err(map_audit_to_contract)?;
        Ok(prepared_contract_audit(&log))
    }

    async fn persist(
        &self,
        audit: &PreparedContractAudit,
        executor: &mut dyn Executor,
    ) -> erp_contract::Result<()> {
        let log = audit_log_from_contract(audit).map_err(map_audit_to_contract)?;
        persist_log(&self.db, &log, executor).await.map_err(map_audit_to_contract)?;
        Ok(())
    }
}

/// MongoDB adapter that reads customer identity facts for contract commands.
#[derive(Clone)]
pub struct MongoContractCustomers {
    db: Database,
}

impl MongoContractCustomers {
    /// Bind the adapter to `db`.
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Wrap the adapter as a shared port.
    pub fn shared(db: Database) -> Arc<dyn CustomerFactsPort> {
        Arc::new(Self::new(db))
    }
}

#[async_trait]
impl CustomerFactsPort for MongoContractCustomers {
    async fn find_by_id(
        &self,
        customer_id: &CustomerAccountId,
    ) -> erp_contract::Result<Option<CustomerAccountFact>> {
        Ok(self
            .db
            .customer_accounts()
            .find_by_id(customer_id.as_ref(), &mut NoTransaction)
            .await
            .map_err(map_customer_to_contract)?
            .map(customer_account_fact))
    }

    async fn find_by_ids(
        &self,
        customer_ids: &[CustomerAccountId],
        executor: &mut dyn Executor,
    ) -> erp_contract::Result<Vec<CustomerAccountFact>> {
        Ok(self
            .db
            .customer_accounts()
            .find_accounts_by_ids(customer_ids, executor)
            .await
            .map_err(map_customer_to_contract)?
            .into_iter()
            .map(customer_account_fact)
            .collect())
    }
}

/// MongoDB adapter that reads assignment visibility for contract lists.
#[derive(Clone)]
pub(crate) struct MongoContractAssignments {
    db: Database,
}

impl MongoContractAssignments {
    /// Bind the adapter to `db`.
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Wrap the adapter as a shared port.
    pub fn shared(db: Database) -> Arc<dyn CustomerAssignmentFactsPort> {
        Arc::new(Self::new(db))
    }
}

#[async_trait]
impl CustomerAssignmentFactsPort for MongoContractAssignments {
    async fn active_assignments_for_user(
        &self,
        user_id: &str,
        as_of: BusinessDate,
        executor: &mut dyn Executor,
    ) -> erp_contract::Result<Vec<ContractAssignmentFact>> {
        let assignments = self
            .db
            .customer_assignments()
            .find_active_assignments_for_user(user_id, as_of, executor)
            .await
            .map_err(map_customer_to_contract)?;
        Ok(assignments
            .into_iter()
            .map(|assignment| ContractAssignmentFact {
                customer_id: assignment.customer_id.to_string(),
                user_id: assignment.user_id,
                is_owner: assignment.assignment_role == AssignmentRole::Owner,
            })
            .collect())
    }

    async fn current_owner_customer_ids(
        &self,
        customer_ids: Option<&[String]>,
        owner_ids: Option<&[String]>,
        as_of: BusinessDate,
        executor: &mut dyn Executor,
    ) -> erp_contract::Result<Vec<String>> {
        if owner_ids.is_some_and(<[String]>::is_empty) || customer_ids.is_some_and(<[String]>::is_empty) {
            return Ok(Vec::new());
        }
        Ok(self
            .db
            .customer_assignments()
            .current_owners(customer_ids, owner_ids, as_of, executor)
            .await
            .map_err(map_customer_to_contract)?
            .into_iter()
            .map(|assignment| assignment.customer_id.to_string())
            .collect())
    }

    async fn owner_user_ids_by_customer(
        &self,
        customer_ids: &[String],
        as_of: BusinessDate,
        executor: &mut dyn Executor,
    ) -> erp_contract::Result<HashMap<String, String>> {
        let assignments = self
            .db
            .customer_assignments()
            .list_active_for_customers(customer_ids, as_of, executor)
            .await
            .map_err(map_customer_to_contract)?;
        Ok(assignments
            .into_iter()
            .filter(|assignment| assignment.assignment_role == AssignmentRole::Owner)
            .map(|assignment| (assignment.customer_id.to_string(), assignment.user_id))
            .collect())
    }
}

/// MongoDB adapter that reads legal document participation for contract reads.
#[derive(Clone)]
pub(crate) struct MongoContractParticipants {
    db: Database,
}

impl MongoContractParticipants {
    /// Bind the adapter to `db`.
    ///
    /// # 参数
    /// * `db` - 工作流参与集合所在数据库
    ///
    /// # 返回
    /// 返回未执行 I/O 的 adapter。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 不得把签约经办解释为参与事实。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Wrap the adapter as a shared port.
    ///
    /// # 参数
    /// * `db` - 工作流参与集合所在数据库
    ///
    /// # 返回
    /// 返回合同参与 Port。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 历史参与只用于读取动作。
    pub fn shared(db: Database) -> Arc<dyn ContractParticipantPort> {
        Arc::new(Self::new(db))
    }
}

#[async_trait]
impl ContractParticipantPort for MongoContractParticipants {
    async fn document_ids_by_user(
        &self,
        user_id: &str,
        executor: &mut dyn Executor,
    ) -> erp_contract::Result<Vec<String>> {
        self.db
            .document_participants()
            .document_ids_by_user(user_id, executor)
            .await
            .map_err(map_customer_to_contract)
    }
}

/// MongoDB adapter that reads account display names for contract lists.
#[derive(Clone)]
pub struct MongoContractAccounts {
    db: Database,
}

impl MongoContractAccounts {
    /// Bind the adapter to `db`.
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Wrap the adapter as a shared port.
    pub fn shared(db: Database) -> Arc<dyn AccountNamePort> {
        Arc::new(Self::new(db))
    }
}

#[async_trait]
impl AccountNamePort for MongoContractAccounts {
    async fn names_by_ids(&self, account_ids: &[String]) -> erp_contract::Result<HashMap<String, String>> {
        self.db
            .accounts()
            .names_by_ids(account_ids, &mut NoTransaction)
            .await
            .map_err(map_identity_to_contract)
    }
}

/// MongoDB adapter that confirms contract PDF file-asset existence.
#[derive(Clone)]
pub struct MongoContractFileAssets {
    db: Database,
}

impl MongoContractFileAssets {
    /// Bind the adapter to `db`.
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Wrap the adapter as a shared port.
    pub fn shared(db: Database) -> Arc<dyn FileAssetFactsPort> {
        Arc::new(Self::new(db))
    }
}

#[async_trait]
impl FileAssetFactsPort for MongoContractFileAssets {
    async fn find_by_id(
        &self,
        attachment_id: &FileAssetId,
        executor: &mut dyn Executor,
    ) -> erp_contract::Result<Option<FileAssetFact>> {
        Ok(self
            .db
            .file_assets()
            .find_by_id(attachment_id.as_ref(), executor)
            .await
            .map_err(map_support_to_contract)?
            .map(|asset| FileAssetFact { id: asset.base.id }))
    }
}

/// 构造只装载合同事实的服务；范围 Port 失败关闭。
///
/// # 参数
/// * `db` - 合同集合所在数据库
///
/// # 返回
/// 返回未接线范围解析的合同服务。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 不得用于列表、详情或写命令；解析范围必须调用 [`scoped_contract_service`]。
pub fn contract_service(db: Database) -> ContractService {
    ContractService::new(
        db.clone(),
        MongoContractAudit::shared(db.clone()),
        MongoContractCustomers::shared(db.clone()),
        MongoContractAssignments::shared(db.clone()),
        MongoContractAccounts::shared(db.clone()),
        MongoContractFileAssets::shared(db),
        ContractScopePorts {
            data_scope: FailClosedContractDataScopePort::shared(),
            participants: FailClosedContractParticipantPort::shared(),
        },
    )
}

/// 构造已接入身份域公共解析器的合同服务。
///
/// # 参数
/// * `db` - 合同与身份集合所在数据库
/// * `rbac` - 当前 RBAC 快照
///
/// # 返回
/// 返回可解析合同范围的服务。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// adapter 必须调用 DataScopeService；合同域不得直接依赖身份域。
pub fn scoped_contract_service(db: Database, rbac: SharedRbacService) -> ContractService {
    ContractService::new(
        db.clone(),
        MongoContractAudit::shared(db.clone()),
        MongoContractCustomers::shared(db.clone()),
        MongoContractAssignments::shared(db.clone()),
        MongoContractAccounts::shared(db.clone()),
        MongoContractFileAssets::shared(db.clone()),
        ContractScopePorts {
            data_scope: MongoContractDataScope::shared(db.clone(), rbac),
            participants: MongoContractParticipants::shared(db),
        },
    )
}

/// 构造绑定身份数据库的合同访问器。
///
/// # 参数
/// * `db` - 数据库
/// * `rbac` - 当前 RBAC 快照
///
/// # 返回
/// 返回已注入本 adapter 的合同访问器。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// HTTP 与命名 Process 必须经此入口，不得把 RBAC 直接交给合同域。
pub fn contract_access(db: Database, rbac: SharedRbacService) -> erp_contract::ContractAccess {
    erp_contract::ContractAccess::new(
        db.clone(),
        MongoContractDataScope::shared(db.clone(), rbac),
        MongoContractAssignments::shared(db.clone()),
        MongoContractParticipants::shared(db.clone()),
        MongoContractCustomers::shared(db),
    )
}

fn customer_account_fact(account: erp_customer::CustomerAccount) -> CustomerAccountFact {
    CustomerAccountFact {
        id: account.base.id.clone(),
        customer_no: account.customer_no.clone(),
        party_id: account.party_id.clone(),
        is_active: account.is_active(),
    }
}

fn prepared_contract_audit(log: &AuditLog) -> PreparedContractAudit {
    PreparedContractAudit::from_validated(
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

fn audit_log_from_contract(audit: &PreparedContractAudit) -> erp_audit::Result<AuditLog> {
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

fn map_audit_to_contract(error: erp_audit::Error) -> erp_contract::Error {
    match error {
        erp_audit::Error::Internal(message) => erp_contract::Error::Internal(message),
        erp_audit::Error::NotFound(message) => erp_contract::Error::NotFound(message),
        erp_audit::Error::ValidationError(message) => erp_contract::Error::ValidationError(message),
        erp_audit::Error::BusinessLogicError(message) => erp_contract::Error::BusinessLogicError(message),
        erp_audit::Error::ConflictError(message) => erp_contract::Error::ConflictError(message),
        erp_audit::Error::ReceiptDuplicate(error) => erp_contract::Error::ReceiptDuplicate(error),
        erp_audit::Error::TransientTransaction(error) => erp_contract::Error::TransientTransaction(error),
        erp_audit::Error::Forbidden(message) => erp_contract::Error::Forbidden(message),
        erp_audit::Error::Unauthenticated(message) => erp_contract::Error::Unauthenticated(message),
        erp_audit::Error::Logic(error) => erp_contract::Error::Logic(error),
        erp_audit::Error::OutcomeUnknown(error) => erp_contract::Error::OutcomeUnknown(error),
        erp_audit::Error::RepositoryError(error) => erp_contract::Error::RepositoryError(error),
    }
}

fn map_customer_to_contract(error: persistence_core::Error) -> erp_contract::Error {
    erp_contract::Error::from(error)
}

fn map_identity_to_contract(error: persistence_core::Error) -> erp_contract::Error {
    erp_contract::Error::from(error)
}

fn map_support_to_contract(error: persistence_core::Error) -> erp_contract::Error {
    erp_contract::Error::from(error)
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;

    use super::*;

    #[test]
    fn prepared_audit_preserves_original_metadata_and_actor_name_snapshot() {
        for name in [Some("发生时名称".to_string()), None] {
            let actor = AuditActor::new("actor".into(), "login".into(), AccountKind::Admin)
                .with_actor_name_snapshot(name.clone())
                .unwrap();
            let domain_prepared = PreparedContractAudit::resource(
                actor.clone(),
                "contract.create",
                "contract",
                "contract-1".into(),
            )
            .unwrap();
            assert_eq!(domain_prepared.actor_name_snapshot, name);
            let mut log = actor
                .clone()
                .resource_log_with_id(
                    "audit-original".into(),
                    "contract.create",
                    "contract",
                    "contract-1".into(),
                    None,
                )
                .unwrap();
            log.base = BaseModel {
                id: "audit-original".into(),
                version: 7,
                created_at: 11,
                updated_at: 19,
                deleted_at: 23,
            };
            log.structured_event.as_mut().unwrap().occurred_at = 11;
            let prepared = prepared_contract_audit(&log);
            let renamed = actor.with_actor_name_snapshot(Some("当前名称".into())).unwrap();
            assert_eq!(renamed.actor_name_snapshot(), Some("当前名称"));
            let restored = audit_log_from_contract(&prepared).unwrap();
            assert_eq!(restored, log);
            assert_eq!(restored.structured_event.unwrap().actor_name_snapshot, name);
        }
    }

    #[test]
    fn prepared_audit_preserves_request_correlation_and_event_sequence() {
        for request_id in [Some("request-original".to_string()), None] {
            let actor = AuditActor::new("actor".into(), "login".into(), AccountKind::Admin)
                .with_actor_name_snapshot(Some("发生时名称".into()))
                .unwrap()
                .with_request_id(request_id.clone())
                .unwrap();
            let domain_prepared = PreparedContractAudit::resource(
                actor.clone(),
                "contract.create",
                "contract",
                "contract-1".into(),
            )
            .unwrap();
            assert_eq!(domain_prepared.request_id, request_id);
            assert_eq!(domain_prepared.event_sequence, NonZeroU32::MIN);
            let log = actor
                .clone()
                .resource_log_with_id(
                    "request-audit-original".into(),
                    "contract.create",
                    "contract",
                    "contract-1".into(),
                    None,
                )
                .unwrap()
                .with_event_sequence(4)
                .unwrap();
            let prepared = prepared_contract_audit(&log);
            let changed = actor.with_request_id(Some("request-current".into())).unwrap();
            assert_eq!(changed.request_id(), Some("request-current"));
            let restored = audit_log_from_contract(&prepared).unwrap();
            assert_eq!(restored, log);
            let event = restored.structured_event.as_ref().unwrap();
            assert_eq!(event.request_id, request_id);
            assert_eq!(event.event_sequence.get(), 4);
            assert_eq!(event.actor_name_snapshot.as_deref(), Some("发生时名称"));
            assert!(!restored.message.as_deref().unwrap().contains("request-original"));
        }
    }

    #[test]
    fn prepared_audit_rejects_unsafe_request_correlation_and_omits_raw_input() {
        let log = AuditActor::new("actor".into(), "login".into(), AccountKind::Admin)
            .with_request_id(Some("request-original".into()))
            .unwrap()
            .resource_log("contract.create", "contract", "contract-1".into())
            .unwrap();
        let mut prepared = prepared_contract_audit(&log);
        prepared.message = Some("body=private-request-body;token=private-token".into());
        let restored = audit_log_from_contract(&prepared).unwrap();
        let serialized = serde_json::to_string(&restored).unwrap();
        assert!(!serialized.contains("private-request-body"));
        assert!(!serialized.contains("private-token"));
        for request_id in ["request\nforged".to_string(), "r".repeat(129)] {
            prepared.request_id = Some(request_id);
            assert!(audit_log_from_contract(&prepared).is_err());
        }
    }

    #[test]
    fn prepared_audit_rejects_unsafe_names_and_drops_unprojected_message() {
        let log = AuditActor::new("actor".into(), "login".into(), AccountKind::Admin)
            .with_actor_name_snapshot(Some("发生时名称".into()))
            .unwrap()
            .resource_log("contract.create", "contract", "contract-1".into())
            .unwrap();
        let mut prepared = prepared_contract_audit(&log);
        prepared.message = Some("token=private-request;bank=private-bank".into());
        let restored = audit_log_from_contract(&prepared).unwrap();
        let serialized = serde_json::to_string(&restored).unwrap();
        assert!(!serialized.contains("private-request"));
        assert!(!serialized.contains("private-bank"));
        assert!(restored.message.as_deref().unwrap().contains("发生时名称"));
        prepared.actor_name_snapshot = Some("非法\n名称".into());
        assert!(audit_log_from_contract(&prepared).is_err());
        prepared.actor_name_snapshot = Some("名".repeat(129));
        assert!(audit_log_from_contract(&prepared).is_err());
        prepared.actor_id = " ".into();
        prepared.action = " ".into();
        assert!(audit_log_from_contract(&prepared).unwrap_err().to_string().contains("操作人ID不能为空"));
    }
}
