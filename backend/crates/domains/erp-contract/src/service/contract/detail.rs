//! 合同详情的授权过滤、完整补齐与交付检查使用同一读取阶段。
use application_core::AuditActor;
use async_trait::async_trait;
use erp_permissions::contract;
use persistence_core::Executor;

use super::ContractService;
use crate::Result;
use crate::authorization::CompiledScope;
use crate::dto::contract::ContractDetailView;

/// 详情读取所需的三个阶段；不对业务调用方开放授权凭证。
#[async_trait]
pub(super) trait DetailRead: Sync {
    type Scope: Send + Sync;

    /// 在原执行器生成固定合同详情查询。
    async fn query(&self, actor: &AuditActor, executor: &mut dyn Executor) -> Result<Self::Scope>;
    /// 授权过滤根合同后，补齐完整详情。
    async fn load(
        &self,
        id: &str,
        scope: &Self::Scope,
        executor: &mut dyn Executor,
    ) -> Result<ContractDetailView>;
    /// 在全部异步补齐之后核验原凭证，失败时丢弃详情。
    async fn validate(&self, scope: &Self::Scope, executor: &mut dyn Executor) -> Result<()>;
}

#[async_trait]
impl DetailRead for ContractService {
    type Scope = CompiledScope;

    async fn query(&self, actor: &AuditActor, executor: &mut dyn Executor) -> Result<Self::Scope> {
        self.data_scope.resolve(actor, &contract::DETAIL, executor).await
    }

    async fn load(
        &self,
        id: &str,
        scope: &Self::Scope,
        executor: &mut dyn Executor,
    ) -> Result<ContractDetailView> {
        self.load_contract_detail(id, scope, executor).await
    }

    async fn validate(&self, scope: &Self::Scope, executor: &mut dyn Executor) -> Result<()> {
        scope.validate(executor).await.map_err(Into::into)
    }
}

/// 业务详情只能在原查询凭证完成交付检查之后返回。
pub(super) async fn read(
    port: &impl DetailRead,
    id: &str,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<ContractDetailView> {
    let scope = port.query(actor, executor).await?;
    let view = port.load(id, &scope, executor).await?;
    port.validate(&scope, executor).await?;
    Ok(view)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use erp_core::AccountKind;
    use persistence_core::NoTransaction;

    use super::*;
    use crate::Error;
    use crate::entity::contract::ContractStatus;

    struct Reads {
        denied: bool,
        expires_during_load: bool,
        calls: Mutex<Vec<(&'static str, usize)>>,
    }

    impl Reads {
        fn record(&self, step: &'static str, executor: &mut dyn Executor) {
            self.calls.lock().unwrap().push((step, executor as *mut dyn Executor as *mut () as usize));
        }
    }

    #[async_trait]
    impl DetailRead for Reads {
        type Scope = &'static str;

        async fn query(&self, _: &AuditActor, executor: &mut dyn Executor) -> Result<Self::Scope> {
            self.record("query", executor);
            if self.denied {
                return Err(Error::Forbidden("无合同详情权限".into()));
            }
            Ok("original-proof")
        }

        async fn load(
            &self,
            id: &str,
            scope: &Self::Scope,
            executor: &mut dyn Executor,
        ) -> Result<ContractDetailView> {
            assert_eq!(*scope, "original-proof");
            self.record("complete-detail", executor);
            Ok(ContractDetailView {
                shared_sales_user_ids: vec![],
                owner_user_id: Some("sales".into()),
                owner_user_name: Some("销售".into()),
                id: id.into(),
                contract_no: "HT-1".into(),
                customer_id: "customer".into(),
                settlement_party_id: "company".into(),
                status: ContractStatus::Effective,
                current_revision_id: None,
                created_at: 1,
                version: 1,
                revisions: vec![],
            })
        }

        async fn validate(&self, scope: &Self::Scope, executor: &mut dyn Executor) -> Result<()> {
            assert_eq!(*scope, "original-proof");
            self.record("validate", executor);
            if self.expires_during_load {
                return Err(Error::ConflictError("DATA_SCOPE_CHANGED：授权关系已到期".into()));
            }
            Ok(())
        }
    }

    /// 允许结果仅在完整补齐后的交付检查成功时返回，三个阶段复用执行器。
    #[tokio::test]
    async fn complete_detail_is_delivered_only_after_original_proof_validation() {
        let port = Reads { denied: false, expires_during_load: false, calls: Mutex::new(vec![]) };
        let actor = AuditActor::new("sales".into(), "sales".into(), AccountKind::Admin);
        let mut executor = NoTransaction;
        let pointer = &mut executor as *mut NoTransaction as usize;
        let result = read(&port, "contract", &actor, &mut executor).await.unwrap();
        assert_eq!(result.id, "contract");
        assert_eq!(result.owner_user_name.as_deref(), Some("销售"));
        assert_eq!(
            *port.calls.lock().unwrap(),
            [("query", pointer), ("complete-detail", pointer), ("validate", pointer)]
        );
    }

    /// 首次拒绝不读取业务数据，补齐中到期不交付已经读取的详情。
    #[tokio::test]
    async fn denial_stops_loading_and_expiry_discards_the_complete_detail() {
        let actor = AuditActor::new("sales".into(), "sales".into(), AccountKind::Admin);
        for (denied, expires) in [(true, false), (false, true)] {
            let port = Reads { denied, expires_during_load: expires, calls: Mutex::new(vec![]) };
            let result = read(&port, "contract", &actor, &mut NoTransaction).await;
            if denied {
                assert!(matches!(result, Err(Error::Forbidden(_))));
            } else {
                assert!(
                    matches!(result, Err(Error::ConflictError(message)) if message.starts_with("DATA_SCOPE_CHANGED"))
                );
            }
            let steps = port.calls.lock().unwrap().iter().map(|(step, _)| *step).collect::<Vec<_>>();
            assert_eq!(
                steps,
                if denied { vec!["query"] } else { vec!["query", "complete-detail", "validate"] }
            );
        }
    }
}
