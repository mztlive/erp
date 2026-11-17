//! 合同客户事实、统一授权与审计能力装配。

use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_contract::{
    ContractAuditPort, ContractService, CustomerAccountFact, CustomerFactsPort, PreparedContractAudit,
};
use erp_core::ids::CustomerAccountId;
use erp_customer::CustomerExt;
use erp_customer::repository::prelude::*;
use erp_identity::AuthorizationService;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction};

use super::audit_conversion::{PreparedAudit, map_audit_error};
use super::authorization::contract_access_with_db;
use super::contract_template::MongoSigningCompanies;

/// 把合同审计事实写入 `erp-audit` 的 Mongo adapter。
#[derive(Clone)]
pub struct MongoContractAudit {
    db: Database,
}

impl MongoContractAudit {
    /// 绑定审计日志所在数据库，构造时不写库。
    ///
    /// # 参数
    /// * `db` - 持久化审计日志的数据库。
    ///
    /// # 返回
    /// 返回未执行 I/O 的 adapter。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 包装为合同域可注入的共享审计 Port。
    ///
    /// # 参数
    /// * `db` - 持久化审计日志的数据库。
    ///
    /// # 返回
    /// 返回共享的合同审计 Port。
    ///
    /// # 错误
    /// 不返回错误。
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
        Ok(PreparedContractAudit::from_log(&log))
    }

    async fn persist(
        &self,
        audit: &PreparedContractAudit,
        executor: &mut dyn Executor,
    ) -> erp_contract::Result<()> {
        let log = audit.to_log().map_err(map_audit_to_contract)?;
        persist_log(&self.db, &log, executor).await.map_err(map_audit_to_contract)?;
        Ok(())
    }
}

/// 为合同命令读取客户身份事实的 Mongo adapter。
#[derive(Clone)]
pub struct MongoContractCustomers {
    db: Database,
}

impl MongoContractCustomers {
    /// 绑定客户账户集合所在数据库，构造时不读取。
    ///
    /// # 参数
    /// * `db` - 客户账户集合所在数据库。
    ///
    /// # 返回
    /// 返回未执行 I/O 的 adapter。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 包装为合同域可注入的客户事实 Port。
    ///
    /// # 参数
    /// * `db` - 客户账户集合所在数据库。
    ///
    /// # 返回
    /// 返回共享的客户事实 Port。
    ///
    /// # 错误
    /// 不返回错误。
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

/// 构造已接入身份域公共解析器的合同服务。
///
/// # 参数
/// * `db` - 合同与身份集合所在数据库
/// * `authorization` - 进程共享统一授权服务
///
/// # 返回
/// 返回可解析合同范围的服务。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// adapter 必须调用 identity 统一授权服务，不解释范围条款。
pub fn scoped_contract_service(db: Database, authorization: Arc<AuthorizationService>) -> ContractService {
    ContractService::new(
        db.clone(),
        MongoContractAudit::shared(db.clone()),
        MongoContractCustomers::shared(db.clone()),
        Arc::new(MongoSigningCompanies(db.clone())),
        contract_access_with_db(db, authorization),
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

map_audit_error!(fn map_audit_to_contract -> erp_contract);

fn map_customer_to_contract(error: persistence_core::Error) -> erp_contract::Error {
    erp_contract::Error::from(error)
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use entity_core::BaseModel;
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
            let prepared = PreparedContractAudit::from_log(&log);
            let renamed = actor.with_actor_name_snapshot(Some("当前名称".into())).unwrap();
            assert_eq!(renamed.actor_name_snapshot(), Some("当前名称"));
            let restored = prepared.to_log().unwrap();
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
            let prepared = PreparedContractAudit::from_log(&log);
            let changed = actor.with_request_id(Some("request-current".into())).unwrap();
            assert_eq!(changed.request_id(), Some("request-current"));
            let restored = prepared.to_log().unwrap();
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
        let mut prepared = PreparedContractAudit::from_log(&log);
        prepared.message = Some("body=private-request-body;token=private-token".into());
        let restored = prepared.to_log().unwrap();
        let serialized = serde_json::to_string(&restored).unwrap();
        assert!(!serialized.contains("private-request-body"));
        assert!(!serialized.contains("private-token"));
        for request_id in ["request\nforged".to_string(), "r".repeat(129)] {
            prepared.request_id = Some(request_id);
            assert!(prepared.to_log().is_err());
        }
    }

    #[test]
    fn prepared_audit_rejects_unsafe_names_and_drops_unprojected_message() {
        let log = AuditActor::new("actor".into(), "login".into(), AccountKind::Admin)
            .with_actor_name_snapshot(Some("发生时名称".into()))
            .unwrap()
            .resource_log("contract.create", "contract", "contract-1".into())
            .unwrap();
        let mut prepared = PreparedContractAudit::from_log(&log);
        prepared.message = Some("token=private-request;bank=private-bank".into());
        let restored = prepared.to_log().unwrap();
        let serialized = serde_json::to_string(&restored).unwrap();
        assert!(!serialized.contains("private-request"));
        assert!(!serialized.contains("private-bank"));
        assert!(restored.message.as_deref().unwrap().contains("发生时名称"));
        prepared.actor_name_snapshot = Some("非法\n名称".into());
        assert!(prepared.to_log().is_err());
        prepared.actor_name_snapshot = Some("名".repeat(129));
        assert!(prepared.to_log().is_err());
        prepared.actor_id = " ".into();
        prepared.action = " ".into();
        assert!(prepared.to_log().unwrap_err().to_string().contains("操作人ID不能为空"));
    }
}
