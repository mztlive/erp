//! 合同明确共享只影响本合同读取与选用，不交接客户、销售单或历史业绩。

use application_core::AuditActor;
use erp_identity::{AccessTarget, AccountDirectoryService};
use erp_permissions::internal;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::ContractService;
use crate::dto::contract::{ContractDetailView, ContractSalesAccessRequest};
use crate::error::{Error, Result};
use crate::repository::ContractExt;

impl ContractService {
    /// 在合同更新范围内维护明确共享销售。
    /// 返回更新后的合同详情。
    /// `id` 为合同身份；`request` 为期望版本、共享人员和原因。
    /// `actor` 为已证明合同更新及客户分配功能权限的操作人。
    /// 权限失效、版本冲突、人员无效或事务失败时拒绝。
    pub async fn set_sales_access(
        &self,
        id: &str,
        request: ContractSalesAccessRequest,
        actor: &AuditActor,
    ) -> Result<ContractDetailView> {
        request.validate()?;

        let service = self.clone();
        let actor = actor.clone();
        let contract_id = id.to_owned();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(
                    async move { service.sales_access_stage(&contract_id, request, &actor, executor).await },
                )
            })
            .await
    }

    // 在原快照执行固定共享授权，再读取版本、修改共享关系并生成命令回包。
    async fn sales_access_stage(
        &self,
        id: &str,
        request: ContractSalesAccessRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<ContractDetailView> {
        self.data_scope
            .authorization
            .require(actor, &internal::CONTRACT_SALES_ACCESS, AccessTarget::record(id)?, executor)
            .await?;

        let mut contract = self
            .db
            .contracts()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("合同不存在或无权修改".into()))?;
        if !contract.matches_version(request.version) {
            return Err(Error::ConflictError("合同已被修改，请刷新后重新核对共享人员".into()));
        }

        contract.set_shared_sales(request.shared_sales_user_ids)?;
        contract.sales_access_change_reason = Some(request.change_reason.trim().to_string());
        contract.stable.updated_by = actor.id().to_owned();
        require_active_sales(&self.accounts, &contract.shared_sales_user_ids, executor).await?;

        let audit = self.audit.resource_log(actor.clone(), "contract.share", "contract", id.to_owned())?;
        self.db.contracts().update(&mut contract, executor).await?;
        self.audit.persist(&audit, executor).await?;

        self.detail_view(contract, executor).await
    }
}

// 共享关系只接受当前可登录的真实账号。
async fn require_active_sales(
    accounts: &AccountDirectoryService,
    ids: &[String],
    executor: &mut dyn Executor,
) -> Result<()> {
    for id in ids {
        let account = accounts
            .account(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("共享销售账号不存在".into()))?;
        account.ensure_can_login().map_err(|error| Error::ValidationError(error.to_string()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use erp_core::AccountKind;
    use erp_identity::entity::authorization::ObjectFacts;
    use erp_identity::testing::unavailable_authorization;
    use mongodb::Client;
    use persistence_core::NoTransaction;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::authorization::ContractAccess;
    use crate::ports::{
        ContractCreationPort, FailClosedAuditPort, FailClosedCustomerFactsPort, SigningCompanyFact,
        SigningCompanyPort,
    };

    struct NoExternalFacts;

    #[async_trait]
    impl ContractCreationPort for NoExternalFacts {
        async fn candidate(&self, _: &AuditActor, _: &str, _: &mut dyn Executor) -> Result<ObjectFacts> {
            panic!("共享合同不读取创建候选")
        }
    }

    #[async_trait]
    impl SigningCompanyPort for NoExternalFacts {
        async fn company(&self, _: &str, _: &mut dyn Executor) -> Result<Option<SigningCompanyFact>> {
            panic!("共享合同不读取公司")
        }
    }

    #[tokio::test]
    async fn sharing_requires_fixed_authorization_before_contract_version_and_account_reads() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("contract_sharing_authorization");
        let service = ContractService::new(
            db.clone(),
            Arc::new(FailClosedAuditPort),
            Arc::new(FailClosedCustomerFactsPort),
            Arc::new(NoExternalFacts),
            ContractAccess::new(db, unavailable_authorization(), Arc::new(NoExternalFacts)),
        );
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let request = ContractSalesAccessRequest {
            version: 999,
            shared_sales_user_ids: vec!["unread-account".into()],
            change_reason: "share".into(),
        };

        assert!(matches!(
            service
                .sales_access_stage("unread-contract", request.clone(), &actor, &mut NoTransaction)
                .await,
            Err(Error::ValidationError(message)) if message == "写入授权必须使用快照事务"
        ));

        assert!(matches!(
            service
                .sales_access_stage("unread-contract", request, &actor, &mut SnapshotExecutor::default())
                .await,
            Err(Error::Internal(message)) if message == "测试授权设施不可用"
        ));
    }
}
