//! 责任队列授权快照、范围过滤与允许动作。

use std::sync::Arc;

use application_core::AuditActor;
use erp_identity::{CompiledAccess, Permission, PermissionSet};
use persistence_core::Executor;

use super::{WorkItemFilter, WorkItemReadService, WorkItemScope, dto};
use crate::entity::work_item::TaskAccess;
use crate::error::{Error, Result};

/// 队列读取凭证与当前展示批次的动作事实；不能代替业务办理资格。
#[derive(Clone)]
pub struct QueueAccess {
    pub task: TaskAccess,
    pub(super) manage: Option<Arc<CompiledAccess>>,
    read: Arc<CompiledAccess>,
}

impl QueueAccess {
    /// 把列表查询和授权快照收成仓储过滤条件。
    ///
    /// # 参数
    /// * `query` - 工作项列表查询。
    ///
    /// # 返回
    /// 所有视图先应用 list 授权；Mine 和 History 只追加本人及历史筛选。
    ///
    /// # 错误
    /// 缺少列表权限时返回 `Error::Forbidden`。
    pub(super) fn scope_filter(&self, query: &dto::WorkItemListQuery) -> Result<WorkItemFilter> {
        if self.read.permission() != &Permission::parse("work_item:list")? {
            return Err(Error::Forbidden("没有待办列表权限".into()));
        }
        let mut filter = WorkItemFilter {
            sales_order_ids: query.sales_order_ids.clone(),
            purchase_order_ids: query.purchase_order_ids.clone(),
            work_item_types: query.work_item_types.clone(),
            statuses: query.statuses.clone(),
            priorities: query.priorities.clone(),
            query: query.query.clone(),
            authorization: Some(self.read.clone()),
            page: query.page,
            page_size: query.page_size,
            sort_by: Some(query.sort_by.to_string()),
            sort_ascending: query.sort_ascending,
            ..WorkItemFilter::default()
        };
        match query.scope {
            WorkItemScope::Mine => filter.owner_user_id = Some(self.read.actor().id().to_owned()),
            WorkItemScope::Managed => {},
            WorkItemScope::History => filter.history_actor_id = Some(self.read.actor().id().to_owned()),
        }
        Ok(filter)
    }

    /// 从已编译读取凭证构造队列上下文，不接受缺少授权的中间状态。
    /// # 参数
    /// read 为本服务已核验的任务读取凭证。
    /// # 返回
    /// 返回绑定凭证身份与入口权限的队列上下文。
    /// # 错误
    /// 无。
    pub(super) fn new(read: Arc<CompiledAccess>) -> Self {
        let mut task = TaskAccess::new(read.actor().id().to_owned());
        task.permissions = PermissionSet::new([read.permission().clone()]);
        Self { task, manage: None, read }
    }

    /// 取得已编译的任务读取约束。
    /// # 参数
    /// 无。
    /// # 返回
    /// 当前读取动作绑定的完整授权结果。
    /// # 错误
    /// 无；构造时必须提供读取凭证。
    pub fn read_access(&self) -> &Arc<CompiledAccess> {
        &self.read
    }

    /// 读取包含管理动作依赖的队列版本。
    /// # 参数
    /// 无。
    /// # 返回
    /// 当前读取和可用管理授权的完整版本。
    /// # 错误
    /// 无。
    pub fn version(&self) -> String {
        format!(
            "{}:{}",
            self.read.version(),
            self.manage.as_ref().map(|value| value.version()).unwrap_or_default()
        )
    }
}

impl WorkItemReadService {
    /// 分别编译读取和管理动作的授权条件，操作权限缺失不影响读取。
    /// # 参数
    /// `actor` 为真实账号，`action` 为当前读取动作，`executor` 为同一执行器。
    /// # 返回
    /// 返回相互独立的读取范围和管理范围。
    /// # 错误
    /// 当前读取动作拒绝或基础设施故障时失败关闭。
    pub async fn task_access(
        &self,
        actor: &AuditActor,
        action: &'static str,
        executor: &mut dyn Executor,
    ) -> Result<QueueAccess> {
        let read = self.authorization.query(actor, action, executor).await?;
        let mut access = QueueAccess::new(read);
        access.manage = if action == "manage" {
            Some(access.read.clone())
        } else {
            match self.authorization.query(actor, "manage", executor).await {
                Ok(manage) => Some(manage),
                Err(Error::Forbidden(_)) => None,
                Err(error) => return Err(error),
            }
        };
        Ok(access)
    }
}
