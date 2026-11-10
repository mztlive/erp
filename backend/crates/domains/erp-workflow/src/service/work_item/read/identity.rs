//! 任务读取和对象检查统一调用 identity，登记不接受请求覆盖。
use std::collections::BTreeSet;

use application_core::AuditActor;
use erp_core::Result as CoreResult;
use erp_identity::{Permission, PermissionSet};
use persistence_core::Executor;

use super::WorkItemReadService;
use super::dto::WorkItemFields;
use crate::entity::work_item::required_execution_permissions;
use crate::repository::work_item::authorization::WorkItemAuthorizationExt;
use crate::{Error, Result, WorkItemExt};

impl WorkItemReadService {
    /// 复验本次任务读取及管理动作的完整依赖。
    /// # 参数
    /// access 为本服务生成的访问上下文；executor 为原执行器。
    /// # 返回
    /// 所有已使用的权限和映射仍有效时成功。
    /// # 错误
    /// 撤权或版本变化返回 DATA_SCOPE_CHANGED，其他错误保留原类别。
    pub async fn validate_access(
        &self,
        access: &super::access::QueueAccess,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.authorization.service.validate(access.read_access()?, executor).await?;
        if let Some(manage) = &access.manage {
            self.authorization.service.validate(manage, executor).await?;
        }
        Ok(())
    }
}

impl WorkItemReadService {
    /// 批量核验当前展示任务所需权限；不遍历全局任务注册表。
    async fn task_permissions(
        &self,
        actor: &AuditActor,
        fields: &[WorkItemFields],
        executor: &mut dyn Executor,
    ) -> Result<PermissionSet> {
        let mut codes = BTreeSet::from([
            "work_item:list",
            "work_item:detail",
            "work_item:manage",
            "approval_instance:decide",
        ]);
        for field in fields {
            if let Some(required) =
                required_execution_permissions(field.work_item_type, &field.business_object_type)
            {
                codes.extend(required);
            }
        }
        let permissions = codes.into_iter().map(Permission::parse).collect::<CoreResult<Vec<_>>>()?;
        Ok(self
            .authorization
            .service
            .permissions(actor, &permissions, executor)
            .await?
            .ok_or_else(|| Error::Forbidden("任务操作账号已失效".into()))?
            .permissions())
    }

    /// 只查询当前展示批次的管理资格，不装载全部可管理任务。
    pub(super) async fn projected_access(
        &self,
        access: &super::access::QueueAccess,
        fields: &[WorkItemFields],
        executor: &mut dyn Executor,
    ) -> Result<super::access::QueueAccess> {
        let mut projected = access.clone();
        projected.task.permissions =
            self.task_permissions(access.read_access()?.actor(), fields, executor).await?;
        let ids = fields.iter().map(|field| field.id.clone()).collect::<Vec<_>>();
        if let Some(manage) = &access.manage {
            self.authorization.service.validate(manage, executor).await?;
            projected.task.managed_task_ids =
                self.db.work_items().authorized_ids(&ids, manage, executor).await?.into_iter().collect();
            self.authorization.service.validate(manage, executor).await?;
        }
        Ok(projected)
    }
}
