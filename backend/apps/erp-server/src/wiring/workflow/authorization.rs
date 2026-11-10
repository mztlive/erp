//! 将身份域权限与 policy 事务装配到工作流授权端口。

use std::collections::{BTreeSet, HashSet};

use application_core::AuditActor;
use erp_approval_runtime::resource_grants::append_assignments;
use erp_identity::SharedRbacService;
use erp_supplier_catalog::supplier_portal::authorization::request_readable as portal_request_readable;
use erp_supplier_catalog::supplier_portal::request_reviewable as portal_request_reviewable;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::ports::{
    OrderTaskSource, WorkflowAuthorizationPort, WorkflowScopeObject, WorkflowScopeObjects,
};
use erp_workflow::service::approval::execution::PlannedWrites;
use erp_workflow::{Error as WorkflowError, Result as WorkflowResult};
use mongodb::Database;
use persistence_core::Executor;

use super::map_service;
use super::order_access::readable_sources;
use crate::wiring::contract_access;
use crate::wiring::errors::{Error, Result};

/// 工作流跨域业务来源授权 adapter。
#[derive(Clone)]
pub struct WorkflowAuth {
    db: Database,
    rbac: SharedRbacService,
}

impl WorkflowAuth {
    /// 以业务数据库和共享身份服务装配来源授权，构造时不查询。
    ///
    /// # 参数
    /// * `db` - 身份与业务集合所在数据库。
    /// * `rbac` - 现有 RBAC 快照服务。
    ///
    /// # 返回
    /// 返回未执行 I/O 的授权 adapter。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }

    #[cfg(test)]
    pub(crate) fn shares_policy_state(&self, other: &Self) -> bool {
        SharedRbacService::ptr_eq(&self.rbac, &other.rbac)
    }
}

impl WorkflowAuthorizationPort for WorkflowAuth {
    async fn require_approval_source(
        &self,
        actor: &AuditActor,
        source: &OrderTaskSource,
        instance: &str,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<()> {
        super::resource_access::require_approval(&self.db, &self.rbac, actor, source, instance, executor)
            .await
            .map_err(map_service)
    }

    async fn append_assignment_grants(
        &self,
        writes: &PlannedWrites,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<()> {
        append_assignments(&self.db, writes, executor).await.map_err(|error| map_service(error.into()))
    }

    async fn supplier_portal_request_reviewable(
        &self,
        actor: &AuditActor,
        request_id: &str,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<bool> {
        portal_request_reviewable(
            &self.db,
            &self.rbac,
            crate::wiring::catalog_services(self.db.clone(), self.rbac.clone()),
            actor,
            request_id,
            executor,
        )
        .await
        .map_err(|error| map_service(error.into()))
    }
    async fn supplier_portal_request_readable(
        &self,
        actor: &AuditActor,
        request_id: &str,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<bool> {
        portal_request_readable(
            &self.db,
            &crate::wiring::catalog_services(self.db.clone(), self.rbac.clone()),
            actor,
            request_id,
            executor,
        )
        .await
        .map_err(|error| map_service(error.into()))
    }
    async fn approval_source_readable(
        &self,
        actor: &AuditActor,
        document_type: DocumentType,
        document_id: &str,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<bool> {
        super::approval_source::readable(&self.db, &self.rbac, actor, document_type, document_id, executor)
            .await
            .map_err(map_service)
    }

    /// 在提交原执行器中复用合同详情授权，来源客户责任由合同领域解释。
    ///
    /// # 参数
    /// * `actor` - 当前提交者。
    /// * `contract_id` - 提交锁定的合同。
    /// * `executor` - 原业务提交执行器。
    /// # 返回
    /// 当前合同动作及来源边界已证明时返回 true；不可见时返回 false。
    /// # 错误
    /// 配置、授权版本及基础设施错误保持失败关闭。
    async fn approval_contract_readable(
        &self,
        actor: &AuditActor,
        contract_id: &str,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<bool> {
        let result = contract_access(self.db.clone(), self.rbac.clone())
            .require_with(actor.clone(), "detail", contract_id, executor)
            .await
            .map(|_| ())
            .map_err(Error::from);
        contract_readable_result(result)
    }

    async fn approval_scope_objects(
        &self,
        keys: &HashSet<(DocumentType, String)>,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<WorkflowScopeObjects> {
        super::approval_objects::load(&self.db, keys, executor).await.map_err(map_service)
    }

    async fn approval_scope_object(
        &self,
        document_type: DocumentType,
        document_id: &str,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<WorkflowScopeObject> {
        self.approval_scope_objects(&HashSet::from([(document_type, document_id.to_string())]), executor)
            .await?
            .remove(&(document_type, document_id.to_string()))
            .ok_or_else(|| WorkflowError::NotFound("审批当前业务对象不存在".into()))
    }

    async fn readable_order_sources(
        &self,
        actor: &AuditActor,
        sources: &BTreeSet<OrderTaskSource>,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<BTreeSet<OrderTaskSource>> {
        readable_sources(&self.db, &self.rbac, actor, sources, executor).await
    }

    async fn require_order_task_read(
        &self,
        actor: &AuditActor,
        source: &OrderTaskSource,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<()> {
        if !super::resource_access::readable(
            &self.db,
            &self.rbac,
            actor,
            source,
            "work_item:detail",
            executor,
        )
        .await
        .map_err(map_service)?
        {
            return Err(WorkflowError::Forbidden("没有任务来源的读取资格".into()));
        }
        Ok(())
    }
}

/// 对象不可见只证明材料转授被拒绝；配置和版本错误不得吞成普通拒绝。
fn contract_readable_result(result: Result<()>) -> WorkflowResult<bool> {
    match result {
        Ok(()) => Ok(true),
        Err(Error::Forbidden(_) | Error::NotFound(_)) => Ok(false),
        Err(error) => Err(map_service(error)),
    }
}

#[cfg(test)]
mod tests {
    use erp_workflow::ErrorCode;

    use super::*;

    #[test]
    fn contract_material_authorization_preserves_denial_and_operational_errors() {
        assert!(contract_readable_result(Ok(())).unwrap());
        assert!(!contract_readable_result(Err(Error::Forbidden("revoked".into()))).unwrap());
        assert!(!contract_readable_result(Err(Error::NotFound("outside scope".into()))).unwrap());
        assert!(matches!(
            contract_readable_result(Err(Error::ConflictError("DATA_SCOPE_CHANGED".into()))),
            Err(WorkflowError::ConflictError(message)) if message == "DATA_SCOPE_CHANGED"
        ));
        assert!(matches!(
            contract_readable_result(Err(Error::Internal("unwired source".into()))),
            Err(WorkflowError::Internal(message)) if message == "unwired source"
        ));
    }

    #[test]
    fn workflow_error_mapping_preserves_source_variants_and_repository_reclassification() {
        let repository = map_service(Error::RepositoryError(persistence_core::Error::OptimisticLockingError));
        assert!(
            matches!(repository, WorkflowError::ConflictError(message) if message == "数据已被其他请求修改，请刷新后重试")
        );
        assert!(matches!(
            map_service(Error::ReceiptDuplicate(persistence_core::Error::OptimisticLockingError)),
            WorkflowError::ReceiptDuplicate(persistence_core::Error::OptimisticLockingError)
        ));
        assert!(matches!(
            map_service(Error::TransientTransaction(persistence_core::Error::OptimisticLockingError)),
            WorkflowError::TransientTransaction(persistence_core::Error::OptimisticLockingError)
        ));
        assert!(matches!(
            map_service(Error::OutcomeUnknown(persistence_core::Error::OptimisticLockingError)),
            WorkflowError::OutcomeUnknown(persistence_core::Error::OptimisticLockingError)
        ));
        macro_rules! preserves_message {
            ($variant:ident) => {
                let mapped = map_service(Error::$variant("original".into()));
                assert!(matches!(mapped, WorkflowError::$variant(message) if message == "original"));
            };
        }
        preserves_message!(Internal);
        preserves_message!(NotFound);
        preserves_message!(ValidationError);
        preserves_message!(BusinessLogicError);
        preserves_message!(ConflictError);
        preserves_message!(Forbidden);
        preserves_message!(Unauthenticated);
        preserves_message!(Rbac);
        assert!(
            matches!(map_service(Error::Logic(erp_core::Error::from("original"))), WorkflowError::Logic(error) if error.to_string() == "original")
        );
        for code in ErrorCode::ALL {
            let mapped = map_service(Error::Coded(code));
            assert_eq!(mapped.to_string(), code.to_string());
            assert_eq!(mapped.class(), code.class());
            assert!(mapped.code().is_some());
        }
    }
}
