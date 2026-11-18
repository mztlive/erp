//! 任务读取和对象检查统一调用 identity，登记不接受请求覆盖。
use persistence_core::Executor;

use super::WorkItemReadService;
use super::dto::WorkItemFields;
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
        self.authorization.service.validate(access.read_access(), executor).await?;
        Ok(())
    }
}

impl WorkItemReadService {
    /// 只查询当前展示批次的管理资格，不装载全部可管理任务。
    pub(super) async fn projected_access(
        &self,
        access: &super::access::QueueAccess,
        fields: &[WorkItemFields],
        executor: &mut dyn Executor,
    ) -> Result<super::access::QueueAccess> {
        let mut projected = access.clone();
        let read = access.read_access();
        let can_decide = read.decision_qualified(executor)?;
        for field in fields {
            if field.work_item_type.brief_relation(&field.business_object_type).is_some()
                && read.execution_qualified(
                    field.work_item_type.as_str(),
                    &field.business_object_type,
                    executor,
                )?
            {
                projected.task.executable_task_ids.insert(field.id.clone());
            }
            if can_decide {
                projected.task.decidable_task_ids.insert(field.id.clone());
            }
        }

        let ids = fields.iter().map(|field| field.id.clone()).collect::<Vec<_>>();
        projected.task.managed_task_ids = self
            .db
            .work_items()
            .authorized_ids(&ids, access.read_access(), executor)
            .await?
            .into_iter()
            .collect();
        Ok(projected)
    }
}
