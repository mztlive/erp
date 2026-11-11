//! W26 命令和原命令回放的当前账号、静态动作资格。

use application_core::AuditActor;
use erp_identity::Permission;
use erp_workflow::WorkItemType;
use erp_workflow::entity::work_item::required_execution_permissions;
use persistence_core::Executor;

use super::{SupplierFulfillmentProcess, W26_BUSINESS_OBJECT_TYPE};
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
        let codes = required_execution_permissions(WorkItemType::BusinessException, W26_BUSINESS_OBJECT_TYPE)
            .filter(|codes| !codes.is_empty())
            .ok_or_else(|| Error::Internal("供应商履约任务缺少完整执行权限合同".into()))?;
        let permissions = codes
            .into_iter()
            .map(|code| Permission::parse(code).map_err(Error::from))
            .collect::<Result<Vec<_>>>()?;
        let rbac = (self.identity)().rbac;
        let qualified = crate::authorization::features(self.db.clone(), rbac)
            .permissions(actor, &permissions, executor)
            .await?
            .ok_or_else(|| Error::Forbidden("供应商履约任务账号已失效".into()))?;
        if !qualified.same_role(&permissions) {
            return Err(Error::Forbidden("当前账号不具备供应商履约任务所需的完整执行权限".into()));
        }
        Ok(())
    }
}
