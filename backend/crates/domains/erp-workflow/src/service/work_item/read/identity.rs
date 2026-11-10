//! 任务读取和对象检查统一调用 identity，登记不接受请求覆盖。
use std::collections::BTreeSet;

use application_core::AuditActor;
use erp_identity::{Permission, PermissionSet};
use persistence_core::Executor;

use super::WorkItemReadService;
use crate::entity::work_item::{WorkItemType, required_execution_permissions};
use crate::repository::work_item::authorization::WorkItemAuthorizationExt;
use crate::{Result, WorkItemExt};

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
    /// 按实际任务注册表检查按钮权限，通配权限仍由 identity 解释。
    pub(super) async fn task_permissions(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<PermissionSet> {
        let mut codes = BTreeSet::from([
            "work_item:list",
            "work_item:detail",
            "work_item:manage",
            "approval_instance:decide",
        ]);
        for relation in WorkItemType::registered_brief_relations() {
            if let Some(required) =
                required_execution_permissions(relation.work_item_type, relation.business_object_type)
            {
                codes.extend(required);
            }
        }
        let mut permissions = Vec::new();
        for code in codes {
            let permission = Permission::parse(code)?;
            if self.authorization.service.has_permission(actor, &permission, executor).await? {
                permissions.push(permission);
            }
        }
        Ok(PermissionSet::new(permissions))
    }

    /// 只查询当前展示批次的管理资格，不装载全部可管理任务。
    pub(super) async fn projected_access(
        &self,
        access: &super::access::QueueAccess,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<super::access::QueueAccess> {
        let mut projected = access.clone();
        if let Some(manage) = &access.manage {
            self.authorization.service.validate(manage, executor).await?;
            projected.task.managed_task_ids =
                self.db.work_items().authorized_ids(ids, manage, executor).await?.into_iter().collect();
            self.authorization.service.validate(manage, executor).await?;
        }
        Ok(projected)
    }
}
