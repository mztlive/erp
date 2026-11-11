//! 销售范围与采购当前任务授权分离，所有命令复用原事务执行器。

use std::sync::Arc;

use application_core::AuditActor;
use erp_identity::{AuthorizationService, CompiledAccess, Permission, SharedRbacService};
use erp_procure_to_pay::procure_to_pay::responsibility::eligibility::ProcurementInquiryEligibility;
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
pub struct SalesInquiryAccess {
    db: Database,
    rbac: SharedRbacService,
}

impl SalesInquiryAccess {
    /// 按当前询价事实核验动作及同角色商品选择资格。
    /// # 参数
    /// actor、action 为当前身份与动作，inquiry 为原事务可信事实，executor 为原执行器。
    /// # 返回
    /// 完整资格与对象范围满足时成功。
    /// # 错误
    /// 无权或事实失效时拒绝。
    pub async fn ensure_selection(
        &self,
        actor: &AuditActor,
        action: &str,
        inquiry: &SalesInquiry,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.require_permissions(actor, action, inquiry, &[Permission::parse("sellable_sku:list")?], executor)
            .await
    }

    /// 按可信候选对象和额外功能资格执行统一授权。
    /// # 参数
    /// actor、action 为服务端入口，inquiry 为业务事实，extra 为固定权限集合，executor 为原事务。
    /// # 返回
    /// 完整对象授权成立时成功。
    /// # 错误
    /// 账号、资格或范围失效时拒绝。
    pub async fn require_permissions(
        &self,
        actor: &AuditActor,
        action: &str,
        inquiry: &SalesInquiry,
        extra: &[Permission],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        authorization::require(self.db.clone(), self.rbac.clone(), actor, action, inquiry, extra, executor)
            .await
    }

    /// 绑定身份与询价事实来源，不进行预授权。
    ///
    /// # 参数
    /// `db` 为业务库，`rbac` 为当前授权源。
    /// # 返回
    /// 无授权缓存的当前事实访问器。
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }

    /// 编译当前询价查询约束，调用仓储前后复验。
    /// # 参数
    /// actor、action 为当前身份及动作，executor 为原执行器。
    /// # 返回
    /// 查询凭证和生成它的授权服务。
    /// # 错误
    /// 账号、动作或编译失败时拒绝。
    pub async fn resolve_sales(
        &self,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<(Arc<CompiledAccess>, AuthorizationService)> {
        authorization::query(self.db.clone(), self.rbac.clone(), actor, action, executor).await
    }

    /// 在当前动作范围内读取询价，销售接口不得调用采购内部报价读取。
    ///
    /// # 参数
    /// `actor`、`action` 为销售动作，`id` 为询价 ID，`executor` 为原事务。
    /// # 返回
    /// 当前可操作的询价事实。
    /// # 错误
    /// 不存在或越范围统一返回不存在；授权源失效时拒绝。
    pub async fn require_sales(
        &self,
        actor: &AuditActor,
        action: &str,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<SalesInquiry> {
        let inquiry = SalesInquiryService::new(self.db.clone()).require(id, executor).await?;
        self.require_permissions(actor, action, &inquiry, &[], executor).await?;
        Ok(inquiry)
    }

    /// 只按当前开放责任任务读取对应询价必要事实。
    ///
    /// # 参数
    /// `actor`、`action` 为采购当前动作，`id`、`task_id` 为精确对象身份，
    /// `task_version` 为写命令预期任务版本（仅读可为空），`executor` 为原事务。
    /// # 返回
    /// 当前询价和包含责任行白名单的任务；调用方只能投影责任行。
    /// # 错误
    /// 任务终态、改派、版本或范围损坏、账号或权限失效时拒绝。
    pub async fn require_task(
        &self,
        actor: &AuditActor,
        action: &str,
        id: &str,
        task_id: &str,
        task_version: Option<u64>,
        executor: &mut dyn Executor,
    ) -> Result<(SalesInquiry, WorkItem)> {
        self.ensure_procurement_permission(actor, action, executor).await?;
        let item = self.db.work_items().find_work_item(task_id, executor).await?.ok_or_else(task_missing)?;
        authorization::require_task_owner(self.db.clone(), self.rbac.clone(), actor, action, &item, executor)
            .await?;
        let inquiry = SalesInquiryService::new(self.db.clone()).require(id, executor).await?;
        let ids = inquiry
            .current_procurement_requirement()?
            .data
            .lines
            .iter()
            .map(|line| line.line_id.clone())
            .collect::<Vec<_>>();
        if !item.owns_inquiry_task(actor.id(), id, inquiry.requirement_version, &ids) {
            return Err(task_missing());
        }
        if task_version.is_some_and(|version| version != item.base.version) {
            return Err(Error::ConflictError("询价任务责任或版本已变化，请刷新".into()));
        }
        Ok((inquiry, item))
    }

    /// 读取当前责任任务或本人已完成任务；完成任务只能交给历史投影使用。
    /// # 参数
    /// `actor`、`action` 为读取动作，`id`、`task_id` 为精确对象身份。
    /// # 返回
    /// 已核验的询价与任务；写动作及内部资料仍须使用 `require_task`。
    /// # 错误
    /// 非读取动作、非本人完成、关闭、改派、快照缺失或权限失效时拒绝。
    pub async fn require_read_task(
        &self,
        actor: &AuditActor,
        action: &str,
        id: &str,
        task_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<(SalesInquiry, WorkItem)> {
        if !matches!(action, "list" | "detail") {
            return Err(task_missing());
        }
        self.ensure_procurement_permission(actor, action, executor).await?;
        let task = self.db.work_items().find_work_item(task_id, executor).await?.ok_or_else(task_missing)?;
        authorization::require_task_owner(self.db.clone(), self.rbac.clone(), actor, action, &task, executor)
            .await?;
        if task.status == WorkItemStatus::Open {
            return self.require_task(actor, action, id, task_id, None, executor).await;
        }
        let inquiry = SalesInquiryService::new(self.db.clone()).require(id, executor).await?;
        let requirement = inquiry
            .requirements
            .iter()
            .find(|row| task.subject_version == inquiry_subject_version(row.version))
            .ok_or_else(task_missing)?;
        let lines = requirement.data.lines.iter().map(|line| line.line_id.clone()).collect::<Vec<_>>();
        if !task.owns_completed_inquiry(actor.id(), id, requirement.version, &lines) {
            return Err(task_missing());
        }
        Ok((inquiry, task))
    }

    /// 重新证明目标采购账号、组织与全部询价处理权限。
    ///
    /// # 参数
    /// `owner` 为目标账号，`executor` 为当前写事务。
    /// # 返回
    /// 目标账号当前有效主属组织。
    /// # 错误
    /// 账号、组织或必要执行权限失效时拒绝。
    pub async fn assignment_owner(&self, owner: &str, executor: &mut dyn Executor) -> Result<String> {
        ProcurementInquiryEligibility::new(self.db.clone(), self.rbac.clone())
            .assignment_owner(owner, executor)
            .await
            .map_err(Error::from)
    }

    /// 证明采购任务动作权限，不解析普通人员或销售数据范围。
    ///
    /// # 参数
    /// `actor` 为当前后台账号，`action` 为任务动作，`executor` 为原事务。
    /// # 返回
    /// 当前已核验的权限策略版本；任务范围须另外由 `require_task` 证明。
    /// # 错误
    /// 账号失效、角色停用、动作权限缺失或策略版本变化时拒绝。
    pub async fn ensure_procurement_permission(
        &self,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<u64> {
        self.ensure_procurement_permissions(actor, &[action], executor).await
    }

    async fn ensure_procurement_permissions(
        &self,
        actor: &AuditActor,
        actions: &[&str],
        executor: &mut dyn Executor,
    ) -> Result<u64> {
        let permissions = actions
            .iter()
            .map(|action| Permission::parse(format!("procurement_inquiry:{action}")))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let qualified = crate::authorization::features(self.db.clone(), self.rbac.clone())
            .permissions(actor, &permissions, executor)
            .await?
            .filter(|qualified| qualified.same_role(&permissions))
            .ok_or_else(|| Error::Forbidden("当前账号缺少完整询价操作权限".into()))?;
        Ok(qualified.policy_revision())
    }
}

fn task_missing() -> Error {
    Error::NotFound("当前询价任务不存在或无权操作".into())
}
