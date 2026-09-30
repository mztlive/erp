//! W26 命令和原命令回放的当前账号、静态动作资格。

use application_core::AuditActor;
use erp_identity::repository::prelude::*;
use erp_identity::{AccessControlExt, Permission};
use erp_workflow::WorkItemType;
use erp_workflow::service::work_item::access::required_execution_permissions;
use persistence_core::Executor;

use super::{SupplierFulfillmentProcess, W26_BUSINESS_OBJECT_TYPE};
use crate::adapters::identity::shared_rbac_service;
use crate::{Error, Result};

impl SupplierFulfillmentProcess {
    /// 任务赋予指定主体的处理边界；任务写入仍在领域事务内重验责任及版本。
    ///
    /// # 参数
    /// * `actor` - 已认证的调用人
    /// * `executor` - 当前读取执行器
    /// # 返回
    /// 当前账号有效且具备 W26 完整动作权限时成功。
    /// # 错误
    /// 账号失效、角色撤权或政策快照变化时拒绝，包括原命令重放。
    pub(super) async fn require_task_permissions(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db
            .accounts()
            .find_by_id(actor.id(), executor)
            .await?
            .filter(|account| account.is_active_backoffice() && account.kind == actor.kind())
            .ok_or_else(|| Error::Forbidden("供应商履约任务账号已失效".into()))?;
        let codes = required_execution_permissions(WorkItemType::BusinessException, W26_BUSINESS_OBJECT_TYPE)
            .filter(|codes| !codes.is_empty())
            .ok_or_else(|| Error::Internal("供应商履约任务缺少完整执行权限合同".into()))?;
        let permissions = codes
            .into_iter()
            .map(|code| Permission::parse(code).map_err(Error::from))
            .collect::<Result<Vec<_>>>()?;
        let rbac = shared_rbac_service(self.db.clone());
        let snapshot = rbac.role_permission_snapshot(actor.kind(), actor.id(), &permissions).await?;
        rbac.ensure_policy_snapshot_with_executor(snapshot.policy_revision(), executor).await?;
        for permission in &permissions {
            let role_ids = snapshot.granting_role_ids_for_all(std::slice::from_ref(permission));
            if self.db.roles().enabled_roles(&role_ids, executor).await?.is_empty() {
                return Err(Error::Forbidden("当前账号不具备供应商履约任务所需的完整执行权限".into()));
            }
        }
        Ok(())
    }
}
