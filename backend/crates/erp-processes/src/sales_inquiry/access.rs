//! 销售范围与采购当前任务授权分离，所有命令复用原事务执行器。

use application_core::AuditActor;
use erp_core::Result as CoreResult;
use erp_identity::access_control::ScopedObject;
use erp_identity::repository::prelude::*;
use erp_identity::service::access_control::resolve::{AuthorizedDataScope, DataScopeService};
use erp_identity::{AccessControlExt, Permission, SharedRbacService};
use erp_sales::entity::sales_inquiry::SalesInquiry;
use erp_sales::service::sales_inquiry::SalesInquiryService;
use erp_workflow::entity::work_item::{WorkItem, WorkItemStatus, inquiry_subject_version};
use erp_workflow::repository::{WorkItemExt, WorkItemRepositoryExt};
use mongodb::Database;
use persistence_core::Executor;

use crate::business_ownership::required_business_org;
use crate::{Error, Result};

/// 当前询价销售范围与采购责任读取器。
#[derive(Clone)]
pub struct SalesInquiryAccess {
    db: Database,
    rbac: SharedRbacService,
}

impl SalesInquiryAccess {
    /// 证明当前询价写动作与公司商品选择资格来自同一有效角色。
    ///
    /// # 参数
    /// `actor`、`action` 为销售需求写动作，`executor` 为原事务。
    /// # 返回
    /// 同角色同时具备询价动作与商品选择权限的数据范围。
    /// # 错误
    /// 账号、角色、动作、商品选择权限或范围失效时拒绝。
    pub async fn resolve_selection(
        &self,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<AuthorizedDataScope> {
        let permission = Permission::parse("sellable_sku:list")?;
        Ok(DataScopeService::new(self.db.clone(), self.rbac.clone())
            .resolve_permissions(actor, "sales_inquiry", action, &[permission], executor)
            .await?)
    }

    /// 将商品选择资格与当前询价责任范围求交。
    ///
    /// # 参数
    /// `inquiry` 为当前事务读取的询价，其余为当前需求写动作及执行器。
    /// # 返回
    /// 同角色完整资格的数据范围覆盖该询价时成功。
    /// # 错误
    /// 不允许跨角色拼接选择权限和公司范围；不覆盖时拒绝。
    pub async fn ensure_selection(
        &self,
        actor: &AuditActor,
        action: &str,
        inquiry: &SalesInquiry,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let access = self.resolve_selection(actor, action, executor).await?;
        if !Self::allows_sales(&access, inquiry) {
            return Err(Error::Forbidden("当前询价范围不具备商品选择资格".into()));
        }
        Ok(())
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

    /// 在调用方事务内解析当前销售动作范围。
    ///
    /// # 参数
    /// `actor` 为当前账号，`action` 为已注册销售询价动作，`executor` 为原事务。
    /// # 返回
    /// 当前有效账号与同角色动作的数据范围。
    /// # 错误
    /// 账号、角色或动作失效时失败关闭。
    pub async fn resolve_sales(
        &self,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<AuthorizedDataScope> {
        Ok(DataScopeService::new(self.db.clone(), self.rbac.clone())
            .resolve_permissions(actor, "sales_inquiry", action, &[], executor)
            .await?)
    }

    /// 校验当前销售责任和业务组织是否在本动作范围内。
    ///
    /// # 参数
    /// `access` 为同一事务解析结果，`inquiry` 为当前业务事实。
    /// # 返回
    /// 负责人或组织范围满足时为 `true`；历史参与不授予访问权。
    /// # 错误
    /// 不返回错误。
    pub fn allows_sales(access: &AuthorizedDataScope, inquiry: &SalesInquiry) -> bool {
        access.scope.allows(
            &ScopedObject {
                owned: inquiry.sales_owner_user_id == access.user_id,
                collaborating: false,
                historical_read_participant: false,
                org_unit_id: Some(&inquiry.business_org_unit_id),
                settlement_party_id: None,
                warehouse_id: None,
            },
            false,
        )
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
        let access = self.resolve_sales(actor, action, executor).await?;
        let inquiry = SalesInquiryService::new(self.db.clone()).require(id, executor).await?;
        if !Self::allows_sales(&access, &inquiry) {
            return Err(Error::NotFound("询价不存在或无权操作".into()));
        }
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
    /// 账号、组织或必要执行权限失效时拒绝，不以销售权限代替。
    pub async fn assignment_owner(&self, owner: &str, executor: &mut dyn Executor) -> Result<String> {
        let account = self
            .db
            .accounts()
            .find_by_id(owner, executor)
            .await?
            .filter(|account| account.is_active_backoffice())
            .ok_or_else(|| Error::BusinessLogicError("询价责任采购账号已失效".into()))?;
        let actor =
            AuditActor::new(account.base.id.clone(), account.secret.account().to_string(), account.kind);
        self.ensure_procurement_permissions(&actor, &["detail", "save_feedback", "publish"], executor)
            .await
            .map_err(|error| assignment_permission_error(error, &account.name, account.secret.account()))?;
        required_business_org(&self.db, owner, executor).await
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
        self.db
            .accounts()
            .find_by_id(actor.id(), executor)
            .await?
            .filter(|account| account.is_active_backoffice() && account.kind == actor.kind())
            .ok_or_else(|| Error::Forbidden("账号已失效".into()))?;
        let required = actions
            .iter()
            .map(|action| Permission::parse(format!("procurement_inquiry:{action}")))
            .collect::<CoreResult<Vec<_>>>()?;
        let snapshot = self.rbac.role_permission_snapshot(actor.kind(), actor.id(), &required).await?;
        self.rbac.ensure_policy_snapshot_with_executor(snapshot.policy_revision(), executor).await?;
        let roles = snapshot.granting_role_ids_for_all(&required);
        if self.db.roles().enabled_roles(&roles, executor).await?.is_empty() {
            return Err(Error::Forbidden(snapshot.denial_message(&required)));
        }
        Ok(snapshot.policy_revision())
    }
}

fn task_missing() -> Error {
    Error::NotFound("当前询价任务不存在或无权操作".into())
}

/// 分派目标的授权失败属于责任配置问题，不得误报为提交人的权限不足。
fn assignment_permission_error(error: Error, name: &str, account: &str) -> Error {
    match error {
        Error::Forbidden(_) => Error::BusinessLogicError(format!(
            "责任采购「{name}（{account}）」缺少有效的询价处理权限，请管理员检查其采购角色的查看、保存反馈和发布权限后重试"
        )),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assignment_denial_identifies_target_buyer_and_required_actions() {
        let error = assignment_permission_error(
            Error::Forbidden("当前账号缺少 procurement_inquiry:detail 权限".into()),
            "陈国平",
            "caigou",
        );
        let Error::BusinessLogicError(message) = error else {
            panic!("目标采购权限缺失必须表现为责任配置错误");
        };
        assert_eq!(
            message,
            "责任采购「陈国平（caigou）」缺少有效的询价处理权限，请管理员检查其采购角色的查看、保存反馈和发布权限后重试"
        );
    }

    #[test]
    fn assignment_preserves_policy_conflict_and_internal_failure() {
        let conflict =
            assignment_permission_error(Error::ConflictError("授权策略已变化".into()), "采购", "buyer");
        assert!(matches!(conflict, Error::ConflictError(message) if message == "授权策略已变化"));
        let internal = assignment_permission_error(Error::Internal("授权源暂不可用".into()), "采购", "buyer");
        assert!(matches!(internal, Error::Internal(message) if message == "授权源暂不可用"));
    }
}
