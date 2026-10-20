//! 销售单跨域查询：组合销售历史快照、采购覆盖、财务与审批投影。

mod customer_names;
mod detail;

use std::collections::{HashMap, HashSet};

use application_core::AuditActor;
use erp_contract::repository::prelude::*;
use erp_core::ids::{SalesOrderId, SalesOrderSubmissionId};
use erp_core::money::Amount;
use erp_customer::repository::prelude::*;
use erp_identity::repository::prelude::*;
use erp_identity::{AccessControlExt, Permission, subject};
use erp_sales::dto::sales_order::{PageView, SubmissionView};
use erp_sales::entity::sales_order::{BusinessType, ReviewStatus};
use erp_sales::repository::sales_order::SalesOrderRow;
use erp_sales::repository::{SalesOrderExt, SalesOrderRevisionRepositoryExt};
use erp_workflow::WorkItemExt;
use erp_workflow::repository::prelude::*;
use persistence_core::NoTransaction;

use super::dto::{
    ActiveCardSalesApprovalView, PurchaseCreationAccessView, SalesOrderView, SalesProcurementCoverageView,
};
use super::status::stage_code_label_tone;
use super::{SalesOrderReadService, dto};
use crate::Result;
use crate::purchase_center::freight_follow_up::{FreightFollowUpView, freight_follow_ups};

/// 销售单阶段责任快照：`(责任角色, 责任人账号, 责任人姓名, 时限)`。
type StageOwnerSnapshot = HashMap<String, (Option<String>, Option<String>, Option<String>, Option<u64>)>;

impl SalesOrderReadService {
    /// 分页查询销售单列表。
    ///
    /// 排序字段白名单在 Service 层校验（api-contract §4），禁止任意字段透传。
    /// 负责销售候选不由本接口生产。
    ///
    /// # 参数
    /// * `params` - 查询参数
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回带范围版本的分页视图，不含负责销售候选。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `ConflictError` - 跨页范围版本缺失或已变化
    /// * `RepositoryError` - 数据库查询失败
    ///
    /// # 关键业务约束
    /// 负责销售筛选只收窄授权结果。行负责人姓名由当前页销售单事实解析，不依赖销售人员目录。
    /// 创建人筛选保持独立字段，不并入负责销售候选。
    #[tracing::instrument(
        name = "sales_order.list",
        skip_all,
        fields(layer = "service", domain = "sales_order", operation = "list")
    )]
    pub async fn sales_order_list(
        &self,
        params: &super::SalesListParams,
        actor: &AuditActor,
    ) -> Result<super::SalesListView> {
        let expected = params.scope_version.as_deref();
        crate::support::ensure_deep_page(params.page.unwrap_or(1), expected)?;
        validator::Validate::validate(params)?;
        let search = self.keyword_search(params.q.as_deref()).await?;
        let super::scope::SalesSnapshot { page, context, no_scope } =
            self.list_snapshot(params, search.clone(), actor).await?;
        if expected.is_some_and(|value| value != context.scope_version) {
            return Err(crate::support::data_scope_changed("数据范围已变化，请从第一页刷新"));
        }

        let owners = self
            .resolve_stage_owners_batch(
                &page
                    .items
                    .iter()
                    .map(|row| (row.id.clone(), row.business_type, row.review_status))
                    .collect::<Vec<_>>(),
            )
            .await?;
        let owner_names = self
            .resolve_account_names_batch(
                &page.items.iter().map(|row| row.sales_owner_user_id.clone()).collect::<Vec<_>>(),
            )
            .await?;

        let customer_names = self
            .customer_names_batch(&page.items.iter().map(|row| row.customer_id.clone()).collect::<Vec<_>>())
            .await?;
        let revision_amounts = self.revision_amounts(&page.items).await?;
        let items = page
            .items
            .into_iter()
            .map(|row| {
                let customer_name = customer_names.get(&row.customer_id).cloned();
                let mut view = map_sales_row(row, &owners, &owner_names, &revision_amounts);
                view.customer_name = customer_name;
                view
            })
            .collect();

        let current_version = self.scope_fingerprint(params, search, actor).await?;
        if current_version != context.scope_version {
            return Err(crate::support::data_scope_changed("数据范围或业务单据已变化，请刷新"));
        }
        Ok(super::SalesListView {
            scope_version: context.scope_version,
            policy_version: context.policy_version,
            organization_version: context.organizations.version,
            as_of: context.as_of.as_utc().to_rfc3339(),
            empty_reason: no_scope.then_some("no_scope"),
            scope_summary: "销售单当前负责人、业务组织及合法单据参与范围",
            ownership_basis: "document_sales_owner",
            data: PageView { items, total: page.total, page: page.page, page_size: page.page_size },
        })
    }

    /// 计算当前账号从销售单继续执行供给分配的访问投影。
    ///
    /// # 参数
    /// * `order` - 当前销售稳定单
    /// * `coverage` - 按销售与采购当前版本计算的采购覆盖
    /// * `actor` - 当前已认证账号；内部无账号上下文时为空
    ///
    /// # 返回
    /// 返回账号状态、静态供给分配权限与开放责任任务共同决定的访问投影.
    ///
    /// # 错误
    /// 任务查询失败，以及权限值对象不合法时返回错误。
    ///
    /// # 关键业务约束
    /// `allowed` 只在账号仍可登录、身份未变化、拥有 `purchase_order:create`
    /// 且持有该销售单开放供给分配任务时为真，与 basis/create 接口的认证和
    /// 授权边界一致。
    async fn purchase_creation_access(
        &self,
        order: &erp_sales::entity::sales_order::SalesOrder,
        coverage: &SalesProcurementCoverageView,
        actor: Option<&AuditActor>,
    ) -> Result<PurchaseCreationAccessView> {
        if let Some(message) = order.procurement_creation_blocker(coverage.remaining_quantity) {
            return Ok(blocked_purchase_creation_access(message));
        }
        let Some(actor) = actor else {
            return Ok(blocked_purchase_creation_access("当前调用缺少供给分配责任上下文"));
        };
        if let Some(message) = self.purchase_creation_actor_blocker(actor).await? {
            return Ok(blocked_purchase_creation_access(message));
        }
        let task_count = self.purchase_creation_task_count(&order.base.id, actor.id()).await?;
        if task_count == 0 {
            return Ok(blocked_purchase_creation_access("当前账号不是该销售单供给分配任务负责人"));
        }
        Ok(PurchaseCreationAccessView { allowed: true, task_count, blocker: None })
    }

    /// 读取运费另计尚未处理的采购商品，仅对可追加采购的账号返回。
    ///
    /// # 参数
    /// * `order` - 销售单
    /// * `actor` - 当前操作人；内部查询为空
    ///
    /// # 返回
    /// 返回待追加运费的采购商品；无权或无待跟进时为空。
    ///
    /// # 错误
    /// 账号、权限或采购事实读取失败时返回错误。
    pub(super) async fn freight_follow_ups(
        &self,
        order: &erp_sales::entity::sales_order::SalesOrder,
        actor: Option<&AuditActor>,
    ) -> Result<Vec<FreightFollowUpView>> {
        let Some(actor) = actor else {
            return Ok(Vec::new());
        };
        if self.purchase_creation_actor_blocker(actor).await?.is_some() {
            return Ok(Vec::new());
        }
        let mut grouped =
            freight_follow_ups(&self.db, std::slice::from_ref(&order.base.id), &mut NoTransaction).await?;
        Ok(grouped.remove(&order.base.id).unwrap_or_default())
    }

    /// 重验当前账号的登录状态、身份与供给分配权限。
    ///
    /// # 参数
    /// * `actor` - JWT 已认证但需要按当前账号和 RBAC 事实重验的操作人
    ///
    /// # 返回
    /// 账号与权限均有效时返回 `None`；否则返回可直接下发的明确 blocker。
    ///
    /// # 错误
    /// 账号或 RBAC 查询失败，以及权限值对象不合法时返回错误。
    ///
    /// # 关键业务约束
    /// 必须使用当前账号记录和规范 Casbin 主体，不能仅信任请求中的历史认证
    /// 快照。
    async fn purchase_creation_actor_blocker(&self, actor: &AuditActor) -> Result<Option<&'static str>> {
        let account = self.db.accounts().find_by_id(actor.id(), &mut NoTransaction).await?;
        let Some(account) = account.filter(|account| account.kind == actor.kind() && account.can_login())
        else {
            return Ok(Some("当前账号不存在、已停用或身份已变化，不能分配供给"));
        };
        let permission = Permission::parse("purchase_order:create")?;
        let allowed =
            self.require_rbac()?.enforce(&subject(account.kind, &account.base.id), &permission).await?;
        Ok((!allowed).then_some("当前账号缺少 purchase_order:create 权限"))
    }

    /// 统计当前账号在指定销售单下拥有的开放供给分配任务。
    ///
    /// # 参数
    /// * `sales_order_id` - 销售单稳定主键
    /// * `actor_id` - 已通过当前账号与采购建单权限重验的账号主键
    ///
    /// # 返回
    /// 返回该账号拥有的开放供给分配任务数量。
    ///
    /// # 错误
    /// 工作项查询失败时返回仓储错误。
    ///
    /// # 关键业务约束
    /// 只统计任务仓储认定为开放且由当前账号负责的供给分配任务。
    async fn purchase_creation_task_count(&self, sales_order_id: &str, actor_id: &str) -> Result<usize> {
        Ok(self
            .db
            .work_items()
            .list_open_procurement_owned_by(actor_id, Some(sales_order_id), None, &mut NoTransaction)
            .await?
            .len())
    }

    /// 解析当前审核轨阶段的责任角色、责任人和时限（详情页专用）。
    ///
    /// # 参数
    /// * `sales_order_id` - 销售单稳定身份
    /// * `business_type` - 业务性质，用于确定审批任务对象类型
    /// * `review_status` - 当前审核轨状态
    ///
    /// # 返回
    /// 返回 `(责任角色, 责任人账号, 时限)`；非在途状态或无开放审批任务时均为空。
    ///
    /// # 错误
    /// 数据库查询失败时返回仓储错误。
    async fn resolve_stage_owner(
        &self,
        sales_order_id: &SalesOrderId,
        business_type: BusinessType,
        review_status: ReviewStatus,
    ) -> Result<(Option<String>, Option<String>, Option<u64>)> {
        if !review_status.has_active_review_task() {
            return Ok((None, None, None));
        }
        let object_type = super::document_type_of_sales_business(business_type).as_str().to_string();
        let tasks = self
            .db
            .work_items()
            .list_active_approval_by_objects(&[(object_type, sales_order_id.to_string())], &mut NoTransaction)
            .await?;
        let Some(task) = tasks.into_iter().next() else {
            return Ok((None, None, None));
        };
        Ok((Some(task.owner_role), task.owner_user_id, task.due_at.map(|due_at| due_at.unix_secs() as u64)))
    }

    /// 构建当前操作人可安全执行的卡券审批工作面投影。
    async fn resolve_active_card_sales_approval(
        &self,
        _order: &erp_sales::entity::sales_order::SalesOrder,
        _submission_id: &SalesOrderSubmissionId,
        _submission: Option<&SubmissionView>,
        _actor: &AuditActor,
    ) -> Result<Option<ActiveCardSalesApprovalView>> {
        Ok(None)
    }

    /// 批量解析本页销售单的当前阶段责任人和时限。
    ///
    /// # 参数
    /// * `rows` - 本页销售单 `(id, 业务性质, 审核轨状态)` 集合
    ///
    /// # 返回
    /// 返回按销售单 ID 索引的 `(责任角色, 责任人账号, 责任人姓名, 时限)`；
    /// 非在途状态或无开放审批任务的销售单不进入结果。
    ///
    /// # 错误
    /// 工作项或账号批量查询失败时返回仓储错误。
    async fn resolve_stage_owners_batch(
        &self,
        rows: &[(String, BusinessType, ReviewStatus)],
    ) -> Result<StageOwnerSnapshot> {
        let business_objects = rows
            .iter()
            .filter(|(_, _, review_status)| review_status.has_active_review_task())
            .map(|(id, business_type, _)| {
                (super::document_type_of_sales_business(*business_type).as_str().to_string(), id.clone())
            })
            .collect::<Vec<_>>();
        let tasks = self
            .db
            .work_items()
            .list_active_approval_by_objects(&business_objects, &mut NoTransaction)
            .await?;
        let owner_names = self
            .resolve_account_names_batch(
                &tasks.iter().filter_map(|task| task.owner_user_id.clone()).collect::<Vec<_>>(),
            )
            .await?;
        Ok(tasks
            .into_iter()
            .map(|task| {
                let owner_name =
                    task.owner_user_id.as_ref().and_then(|owner_id| owner_names.get(owner_id).cloned());
                (
                    task.business_object_id,
                    (
                        Some(task.owner_role),
                        task.owner_user_id,
                        owner_name,
                        task.due_at.map(|due_at| due_at.unix_secs() as u64),
                    ),
                )
            })
            .collect())
    }

    /// 批量读取列表当前正式版本的金额。
    ///
    /// # 参数
    /// * `rows` - 当前页销售单投影行
    ///
    /// # 返回
    /// 返回按正式版本身份索引的金额快照；无正式版本的单据不在结果中。
    ///
    /// # 错误
    /// 版本仓储查询失败时返回仓储错误。
    async fn revision_amounts(
        &self,
        rows: &[SalesOrderRow],
    ) -> Result<HashMap<String, SalesRevisionAmounts>> {
        let revision_ids = rows
            .iter()
            .filter_map(|row| row.current_revision_id.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let revisions =
            self.db.sales_order_revisions().find_revisions_by_ids(&revision_ids, &mut NoTransaction).await?;
        Ok(revisions
            .into_iter()
            .map(|revision| {
                (
                    revision.base.id.clone(),
                    SalesRevisionAmounts {
                        gross: revision.gross_amount,
                        net: revision.net_amount,
                        tax: revision.tax_amount,
                    },
                )
            })
            .collect())
    }

    /// 批量解析账号展示姓名。
    ///
    /// # 参数
    /// * `account_ids` - 账号 ID 集合，允许重复或为空
    ///
    /// # 返回
    /// 返回按账号 ID 索引的展示姓名；已删除或不存在的账号不会进入结果。
    ///
    /// # 错误
    /// 账号仓储查询失败时返回仓储错误。
    async fn resolve_account_names_batch(&self, account_ids: &[String]) -> Result<HashMap<String, String>> {
        let unique_ids = account_ids.iter().cloned().collect::<HashSet<_>>().into_iter().collect::<Vec<_>>();
        let accounts = self.db.accounts().list_by_ids(&unique_ids, &mut NoTransaction).await?;
        Ok(accounts.into_iter().map(|account| (account.base.id, account.name)).collect())
    }
}

/// 当前正式版本的列表金额快照。
#[derive(Debug, Clone, Copy)]
struct SalesRevisionAmounts {
    /// 含税成交金额。
    gross: Amount,
    /// 不含税成交金额。
    net: Amount,
    /// 税额。
    tax: Amount,
}

/// 映射销售列表行与阶段责任快照，纯内存组装不触库.
fn map_sales_row(
    row: SalesOrderRow,
    owners: &StageOwnerSnapshot,
    owner_names: &HashMap<String, String>,
    revision_amounts: &HashMap<String, SalesRevisionAmounts>,
) -> SalesOrderView {
    let (code, label, tone) = stage_code_label_tone(
        row.commercial_status,
        row.review_status,
        row.close_status,
        row.fulfillment_progress,
    );
    let (owner_role, stage_owner_user_id, stage_owner_user_name, due_at) =
        owners.get(&row.id).cloned().unwrap_or_default();
    let owner_user_id = row.sales_owner_user_id.clone();
    let owner_user_name = owner_names.get(&owner_user_id).cloned();
    let amounts =
        row.current_revision_id.as_deref().and_then(|revision_id| revision_amounts.get(revision_id)).copied();
    SalesOrderView {
        id: row.id,
        order_no: row.order_no,
        business_type: row.business_type,
        origin_system: row.origin_system,
        customer_id: row.customer_id,
        customer_name: None,
        contract_id: row.contract_id,
        commercial_status: row.commercial_status,
        review_status: row.review_status,
        fulfillment_progress: row.fulfillment_progress,
        collection_progress: row.collection_progress,
        invoice_progress: row.invoice_progress,
        close_status: row.close_status,
        effective_at: row.effective_at,
        closed_at: row.closed_at,
        version: row.version,
        created_at: row.created_at,
        updated_at: row.updated_at,
        owner_user_id,
        owner_user_name,
        gross_amount: amounts.map_or_else(Amount::zero, |value| value.gross),
        net_amount: amounts.map_or_else(Amount::zero, |value| value.net),
        tax_amount: amounts.map_or_else(Amount::zero, |value| value.tax),
        stage: dto::SalesOrderStageSummary {
            code,
            label,
            tone,
            owner_role,
            owner_user_id: stage_owner_user_id,
            owner_user_name: stage_owner_user_name,
            due_at,
        },
    }
}

/// 构造禁止创建采购单的稳定访问投影。
///
/// # 参数
/// * `message` - 可直接展示给调用方的明确业务阻塞说明
///
/// # 返回
/// 返回 `allowed = false`、任务数为零且携带 blocker 的访问投影。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 禁止分支不得泄露任何开放任务数量，避免把未授权任务事实下发给调用方。
fn blocked_purchase_creation_access(message: &str) -> PurchaseCreationAccessView {
    PurchaseCreationAccessView { blocker: Some(message.to_string()), ..Default::default() }
}

impl SalesOrderReadService {
    /// 在分页前经拥有领域解析客户名称、合同号；空关键词不读取关联表。
    ///
    /// 返回销售域中性的搜索条件，关联读取失败时整次查询失败。
    pub(crate) async fn keyword_search(
        &self,
        q: Option<&str>,
    ) -> Result<erp_sales::repository::sales_order::SalesOrderSearch> {
        use erp_contract::ContractExt;
        use erp_customer::CustomerExt;
        use erp_party::PartyExt;
        let Some(q) = application_core::normalized_text(q) else {
            return Ok(Default::default());
        };
        let party_ids = self
            .db
            .party()
            .matching_current_party_ids_by_name(&q, &mut NoTransaction)
            .await?
            .into_iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>();
        let customer_ids =
            self.db.customer_accounts().matching_ids_by_parties(&party_ids, &mut NoTransaction).await?;
        let contract_ids = self.db.contracts().matching_ids_by_number(&q, &mut NoTransaction).await?;
        Ok(erp_sales::repository::sales_order::SalesOrderSearch { q: Some(q), customer_ids, contract_ids })
    }
}

#[cfg(test)]
mod tests {
    use erp_sales::entity::sales_order::ReviewStatus;

    use super::blocked_purchase_creation_access;

    #[test]
    fn unified_and_legacy_pending_reviews_require_open_tasks() {
        for status in [
            ReviewStatus::InApproval,
            ReviewStatus::PendingProcurementConfirmation,
            ReviewStatus::PendingLowMarginSuperior,
            ReviewStatus::PendingSalesLeader,
            ReviewStatus::PendingOperations,
        ] {
            assert!(status.has_active_review_task());
        }
    }

    #[test]
    fn terminal_review_states_do_not_require_open_tasks() {
        for status in [ReviewStatus::NotSubmitted, ReviewStatus::Approved, ReviewStatus::Rejected] {
            assert!(!status.has_active_review_task());
        }
    }

    /// 禁止访问投影必须返回明确 blocker 且隐藏任务数量。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无；禁止投影仍允许创建、泄露任务数或缺少说明时测试失败。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 未授权账号不能从销售详情推断其名下或他人的采购任务数量。
    #[test]
    fn blocked_purchase_creation_access_is_explicit_and_hides_tasks() {
        let view = blocked_purchase_creation_access("当前账号缺少 purchase_order:create 权限");

        assert!(!view.allowed);
        assert_eq!(view.task_count, 0);
        assert_eq!(view.blocker.as_deref(), Some("当前账号缺少 purchase_order:create 权限"));
    }
}
