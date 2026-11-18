//! 销售范围与采购当前任务授权分离，所有命令复用原事务执行器。

use std::sync::Arc;

use application_core::AuditActor;
use erp_identity::{AuthorizationService, CompiledAccess};
use erp_permissions::{Permission as AccessPermission, sales_inquiry as sales_inquiry_permissions};
use erp_sales::entity::sales_inquiry::SalesInquiry;
use erp_sales::service::sales_inquiry::SalesInquiryService;
use erp_workflow::entity::work_item::{WorkItem, WorkItemStatus, inquiry_subject_version};
use erp_workflow::repository::{WorkItemExt, WorkItemRepositoryExt};
use mongodb::Database;
use persistence_core::Executor;

use super::authorization;
use crate::{Error, Result};

/// 当前询价销售范围与采购责任读取器。
#[derive(Clone)]
pub(super) struct SalesInquiryAccess {
    db: Database,
    authorization: Arc<AuthorizationService>,
}

impl SalesInquiryAccess {
    /// 按可信候选对象和额外功能资格执行统一授权。
    /// # 参数
    /// actor 为当前认证操作人，permission 为服务端固定权限，inquiry 为业务事实，executor 为原事务。
    /// # 返回
    /// 完整对象授权成立时成功。
    /// # 错误
    /// 账号、资格或范围失效时拒绝。
    pub(super) async fn require(
        &self,
        actor: &AuditActor,
        permission: &AccessPermission,
        inquiry: &SalesInquiry,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        authorization::require(&self.authorization, actor, permission, inquiry, executor).await
    }

    /// 绑定身份与询价事实来源，不进行预授权。
    ///
    /// # 参数
    /// `db` 为业务库，`authorization` 为当前授权源。
    /// # 返回
    /// 无授权缓存的当前事实访问器。
    /// # 错误
    /// 无。
    pub(super) fn new(db: Database, authorization: Arc<AuthorizationService>) -> Self {
        Self { db, authorization }
    }

    /// 编译当前询价查询约束，调用仓储前后复验。
    /// # 参数
    /// actor 为当前认证操作人，permission 为服务端固定权限，executor 为原事务执行器。
    /// # 返回
    /// 查询凭证和生成它的授权服务。
    /// # 错误
    /// 账号、动作或编译失败时拒绝。
    pub(super) async fn resolve_sales(
        &self,
        actor: &AuditActor,
        permission: &AccessPermission,
        executor: &mut dyn Executor,
    ) -> Result<(Arc<CompiledAccess>, Arc<AuthorizationService>)> {
        authorization::query(&self.authorization, actor, permission, executor).await
    }

    /// 在当前动作范围内读取询价，销售接口不得调用采购内部报价读取。
    ///
    /// # 参数
    /// `actor` 为当前认证操作人，`permission` 为销售入口固定权限，`id` 为询价 ID，`executor` 为原事务。
    /// # 返回
    /// 当前可操作的询价事实。
    /// # 错误
    /// 不存在或越范围统一返回不存在；授权源失效时拒绝。
    pub(super) async fn require_sales(
        &self,
        actor: &AuditActor,
        permission: &AccessPermission,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<SalesInquiry> {
        let inquiry = SalesInquiryService::new(self.db.clone()).require(id, executor).await?;
        self.require(actor, permission, &inquiry, executor).await?;
        Ok(inquiry)
    }

    /// 按当前开放责任任务读取对应询价的可信事实。
    ///
    /// # 参数
    /// `actor` 为当前认证操作人，`permission` 为采购入口固定权限，`id`、`task_id` 为精确对象身份，
    /// `task_version` 为写命令预期任务版本（仅读可为空），`executor` 为原事务。
    /// # 返回
    /// 当前询价和包含办理责任行的任务；写命令仍按责任行校验。
    /// # 错误
    /// 任务终态、改派、版本或范围损坏、账号或权限失效时拒绝。
    pub(super) async fn require_task(
        &self,
        actor: &AuditActor,
        permission: &AccessPermission,
        id: &str,
        task_id: &str,
        task_version: Option<u64>,
        executor: &mut dyn Executor,
    ) -> Result<(SalesInquiry, WorkItem)> {
        let item = self.authorized_task(actor, permission, task_id, executor).await?;
        let inquiry = self.current_task_inquiry(id, &item, executor).await?;
        if task_version.is_some_and(|version| version != item.base.version) {
            return Err(Error::ConflictError("询价任务责任或版本已变化，请刷新".into()));
        }
        Ok((inquiry, item))
    }

    /// 读取当前责任任务或本人已完成任务；完成任务只能交给历史投影使用。
    /// # 参数
    /// `actor` 为当前认证操作人，`permission` 为固定读取权限，`id`、`task_id` 为精确对象身份，
    /// `executor` 为原事务执行器。
    /// # 返回
    /// 已核验的询价与任务；写动作及内部资料仍须使用 `require_task`。
    /// # 错误
    /// 非读取动作、非本人完成、关闭、改派、快照缺失或权限失效时拒绝。
    pub(super) async fn require_read_task(
        &self,
        actor: &AuditActor,
        permission: &AccessPermission,
        id: &str,
        task_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<(SalesInquiry, WorkItem)> {
        if ![
            sales_inquiry_permissions::QUERY_PROCUREMENT_LIST,
            sales_inquiry_permissions::QUERY_PROCUREMENT_DETAIL,
        ]
        .contains(permission)
        {
            return Err(task_missing());
        }

        let task = self.authorized_task(actor, permission, task_id, executor).await?;
        if task.status == WorkItemStatus::Open {
            let inquiry = self.current_task_inquiry(id, &task, executor).await?;
            return Ok((inquiry, task));
        }

        let inquiry = SalesInquiryService::new(self.db.clone()).require(id, executor).await?;
        let requirement = inquiry
            .requirements
            .iter()
            .find(|row| task.subject_version == inquiry_subject_version(row.version))
            .ok_or_else(task_missing)?;
        let lines = requirement.data.lines.iter().map(|line| line.line_id.clone()).collect::<Vec<_>>();
        if !task.completed_inquiry_current(id, requirement.version, &lines) {
            return Err(task_missing());
        }
        Ok((inquiry, task))
    }

    /// 在原执行器装载任务并由 identity 一次证明动作与当前分配资格。
    async fn authorized_task(
        &self,
        actor: &AuditActor,
        permission: &AccessPermission,
        task_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<WorkItem> {
        let item = self.db.work_items().find_work_item(task_id, executor).await?.ok_or_else(task_missing)?;
        authorization::require_task_owner(&self.authorization, actor, permission, &item, executor).await?;
        Ok(item)
    }

    /// 按已授权任务核对当前询价需求版本与完整责任行。
    async fn current_task_inquiry(
        &self,
        id: &str,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<SalesInquiry> {
        let inquiry = SalesInquiryService::new(self.db.clone()).require(id, executor).await?;
        let ids = inquiry
            .current_procurement_requirement()?
            .data
            .lines
            .iter()
            .map(|line| line.line_id.clone())
            .collect::<Vec<_>>();
        if !item.inquiry_task_current(id, inquiry.requirement_version, &ids) {
            return Err(task_missing());
        }
        Ok(inquiry)
    }
}

fn task_missing() -> Error {
    Error::NotFound("当前询价任务不存在或无权操作".into())
}
