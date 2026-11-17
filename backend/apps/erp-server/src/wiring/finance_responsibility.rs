//! 财务任务资格与负责人池保留各自原有的权限实例来源。

use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_finance_posting::FinanceTaskPort;
use erp_identity::{AuthorizationService, SharedRbacService};
use erp_workflow::entity::work_item::{FinanceResponsibilityOperation, WorkItem, WorkItemType};
use erp_workflow::ports::FinanceResponsibilityPort;
use erp_workflow::service::work_item::ResolvedFinanceResponsibility;
use erp_workflow::{Result, WorkItemService};
use mongodb::Database;
use persistence_core::Executor;

use super::identity::shared_rbac_service;
use super::workflow::{WorkflowAuth, work_item_service};

/// 负责人解析每次使用独立身份；既有任务资格继续使用调用方身份。
struct FinanceWorkItems {
    db: Database,
    caller_rbac: SharedRbacService,
    authorization: Arc<AuthorizationService>,
}

impl FinanceWorkItems {
    fn caller_service(&self) -> WorkItemService<WorkflowAuth> {
        work_item_service(self.db.clone(), self.caller_rbac.clone(), self.authorization.clone())
    }

    fn fresh_identity(&self) -> SharedRbacService {
        shared_rbac_service(self.db.clone())
    }

    fn fresh_service(&self) -> WorkItemService<WorkflowAuth> {
        work_item_service(self.db.clone(), self.fresh_identity(), self.authorization.clone())
    }
}

#[async_trait]
impl FinanceResponsibilityPort for FinanceWorkItems {
    async fn resolve_finance_responsibility(
        &self,
        operation: FinanceResponsibilityOperation,
        counterparty: &str,
        executor: &mut dyn Executor,
    ) -> Result<ResolvedFinanceResponsibility> {
        self.fresh_service().resolve_finance_responsibility(operation, counterparty, executor).await
    }

    async fn resolve_return_finance_responsibility(
        &self,
        kind: WorkItemType,
        counterparty: &str,
        executor: &mut dyn Executor,
    ) -> Result<ResolvedFinanceResponsibility> {
        self.fresh_service().resolve_return_finance_responsibility(kind, counterparty, executor).await
    }
}

#[async_trait]
impl FinanceTaskPort for FinanceWorkItems {
    async fn ensure_domain_decision_access(
        &self,
        actor: &AuditActor,
        task: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.caller_service().ensure_domain_decision_access(actor, task, executor).await
    }

    async fn ensure_item_access(
        &self,
        actor: &AuditActor,
        task: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let service = self.caller_service();
        service.ensure_item_access_with_executor(task, actor, executor).await
    }
}

/// 装配原指定任务资格和独立负责人解析能力。
/// # 参数
/// db 为工作项数据库，caller_rbac 为身份设施，authorization 为进程共享统一授权服务。
/// # 返回
/// 返回任务资格沿用调用方、负责人解析每次独立创建身份的能力。
/// # 错误
/// 构造不执行 I/O，不返回错误；资格和责任分配错误由原调用位置返回。
pub fn finance_tasks(
    db: Database,
    caller_rbac: SharedRbacService,
    authorization: Arc<AuthorizationService>,
) -> Arc<dyn FinanceTaskPort> {
    Arc::new(FinanceWorkItems { db, caller_rbac, authorization })
}

/// 装配每次独立创建身份的财务责任池分配能力。
/// # 参数
/// db 为工作项数据库，caller_rbac 为身份设施，authorization 为进程共享统一授权服务。
/// # 返回
/// 返回两个责任解析方法均在调用时创建新身份的窄合同。
/// # 错误
/// 构造不执行 I/O，不返回错误；分配时保持原领域错误及执行器。
pub fn finance_responsibility(
    db: Database,
    caller_rbac: SharedRbacService,
    authorization: Arc<AuthorizationService>,
) -> Arc<dyn FinanceResponsibilityPort> {
    Arc::new(FinanceWorkItems { db, caller_rbac, authorization })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn responsibility_identity_is_fresh_per_resolution_and_independent_from_caller() {
        let client = mongodb::Client::with_uri_str("mongodb://localhost:27017").await.unwrap();
        let db = client.database("finance-responsibility-factory-unit-test");
        let caller = shared_rbac_service(db.clone());
        let authorization = crate::wiring::authorization::build(db.clone(), caller.clone()).unwrap();
        let provider = FinanceWorkItems { db, caller_rbac: caller.clone(), authorization };
        let first = provider.fresh_identity();
        let second = provider.fresh_identity();

        assert!(Arc::ptr_eq(&caller, &provider.caller_rbac));
        assert!(!Arc::ptr_eq(&caller, &first));
        assert!(!Arc::ptr_eq(&caller, &second));
        assert!(!Arc::ptr_eq(&first, &second));
    }
}
