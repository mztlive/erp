//! 销售单跨域查询：组合销售历史快照、采购覆盖、财务与审批投影。

mod customer_names;
mod detail;

use std::collections::{HashMap, HashSet};

use application_core::AuditActor;
use erp_contract::repository::prelude::*;
use erp_core::ids::{SalesOrderId, SalesOrderSubmissionId};
use erp_core::money::Amount;
use erp_customer::repository::prelude::*;
use erp_identity::AccessControlExt;
use erp_identity::repository::prelude::*;
use erp_sales::dto::sales_order::{PageView, SubmissionView};
use erp_sales::entity::sales_order::{BusinessType, ReviewStatus};
use erp_sales::repository::sales_order::SalesOrderRow;
use erp_sales::repository::{SalesOrderExt, SalesOrderRevisionRepositoryExt};
use erp_workflow::WorkItemExt;
use erp_workflow::repository::prelude::*;
use persistence_core::Executor;

use super::dto::{
    ActiveCardSalesApprovalView, PurchaseCreationAccessView, SalesOrderView, SalesProcurementCoverageView,
};
use super::status::stage_code_label_tone;
use super::{SalesOrderReadService, dto};
use crate::purchase_center::creation_authorization::{CreationTaskFilter, query_tasks};
use crate::purchase_center::freight_follow_up::{FreightFollowUpView, freight_follow_ups};
use crate::{Error, Result};

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
        self.list_snapshot(params, actor).await
    }

    /// 在原授权事务补齐当前页金额、责任人与客户展示，不改变分页成员。
    ///
    /// # 参数
    /// page 为已授权分页；executor 为签发该页凭证的原事务执行器。
    /// # 返回
    /// 完整列表页；总数、页码、行序及页大小保持原值。
    /// # 错误
    /// 任一金额、任务、账号或客户事实读取失败时拒绝交付整页。
    pub(super) async fn list_view(
        &self,
        page: PageView<SalesOrderRow>,
        executor: &mut dyn Executor,
    ) -> Result<PageView<SalesOrderView>> {
        let revision_amounts = self.revision_amounts(&page.items, executor).await?;
        let mut items =
            page.items.into_iter().map(|row| map_sales_row(row, &revision_amounts)).collect::<Vec<_>>();
        self.fill_stage_owners(&mut items, executor).await?;
        self.fill_owner_names(&mut items, executor).await?;
        self.fill_customer_names(&mut items, executor).await?;
        Ok(PageView { items, total: page.total, page: page.page, page_size: page.page_size })
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
        executor: &mut dyn Executor,
    ) -> Result<PurchaseCreationAccessView> {
        if let Some(message) = order.procurement_creation_blocker(coverage.remaining_quantity) {
            return Ok(blocked_purchase_creation_access(message));
        }
        let Some(actor) = actor else {
            return Ok(blocked_purchase_creation_access("当前调用缺少供给分配责任上下文"));
        };
        let task_count = match query_tasks(
            &self.db,
            self.require_rbac()?,
            actor,
            CreationTaskFilter { sales_order_id: Some(order.base.id.clone()), work_item_id: None },
            executor,
        )
        .await
        {
            Ok(tasks) => tasks.len(),
            Err(Error::Forbidden(_) | Error::Unauthenticated(_)) => {
                return Ok(blocked_purchase_creation_access(
                    "当前账号不能执行供给分配，请检查账号状态和 purchase_order:create 权限",
                ));
            },
            Err(error) => return Err(error),
        };
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
        executor: &mut dyn Executor,
    ) -> Result<Vec<FreightFollowUpView>> {
        let Some(actor) = actor else {
            return Ok(Vec::new());
        };
        let authorization = self.require_rbac()?;
        if !authorization.has_permission(actor, &erp_permissions::purchase_order::CREATE, executor).await? {
            return Ok(Vec::new());
        }
        let mut grouped =
            freight_follow_ups(&self.db, std::slice::from_ref(&order.base.id), executor).await?;
        Ok(grouped.remove(&order.base.id).unwrap_or_default())
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
        executor: &mut dyn Executor,
    ) -> Result<(Option<String>, Option<String>, Option<u64>)> {
        if !review_status.has_active_review_task() {
            return Ok((None, None, None));
        }
        let object_type = super::document_type_of_sales_business(business_type).as_str().to_string();
        let tasks = self
            .db
            .work_items()
            .list_active_approval_by_objects(&[(object_type, sales_order_id.to_string())], executor)
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
        _executor: &mut dyn Executor,
    ) -> Result<Option<ActiveCardSalesApprovalView>> {
        Ok(None)
    }

    /// 批量补齐当前页的阶段责任人和时限，不改变列表顺序与成员。
    ///
    /// 无开放审核任务的单据保持空阶段责任，仓储失败时返回错误。
    async fn fill_stage_owners(
        &self,
        items: &mut [SalesOrderView],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let business_objects = items
            .iter()
            .filter(|item| item.review_status.has_active_review_task())
            .map(|item| {
                (
                    super::document_type_of_sales_business(item.business_type).as_str().to_string(),
                    item.id.clone(),
                )
            })
            .collect::<Vec<_>>();
        let tasks = self.db.work_items().list_active_approval_by_objects(&business_objects, executor).await?;
        let owner_names = self
            .account_names(
                &tasks.iter().filter_map(|task| task.owner_user_id.clone()).collect::<Vec<_>>(),
                executor,
            )
            .await?;
        let owners = tasks
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
            .collect::<HashMap<_, _>>();
        for item in items {
            (
                item.stage.owner_role,
                item.stage.owner_user_id,
                item.stage.owner_user_name,
                item.stage.due_at,
            ) = owners.get(&item.id).cloned().unwrap_or_default();
        }
        Ok(())
    }

    /// 批量补齐当前页负责销售姓名；缺失账号保持空名称，查询失败时返回错误。
    async fn fill_owner_names(
        &self,
        items: &mut [SalesOrderView],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = items.iter().map(|item| item.owner_user_id.clone()).collect::<Vec<_>>();
        let names = self.account_names(&ids, executor).await?;
        for item in items {
            item.owner_user_name = names.get(&item.owner_user_id).cloned();
        }
        Ok(())
    }

    /// 批量读取列表当前正式版本的金额。
    ///
    /// # 参数
    /// * `rows` - 当前页销售单投影行
    /// * `executor` - 原授权事务执行器。
    ///
    /// # 返回
    /// 返回按正式版本身份索引的金额快照；无正式版本的单据不在结果中。
    ///
    /// # 错误
    /// 版本仓储查询失败时返回仓储错误。
    async fn revision_amounts(
        &self,
        rows: &[SalesOrderRow],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, SalesRevisionAmounts>> {
        let revision_ids = rows
            .iter()
            .filter_map(|row| row.current_revision_id.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let revisions =
            self.db.sales_order_revisions().find_revisions_by_ids(&revision_ids, executor).await?;
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
    /// * `executor` - 原授权事务执行器。
    ///
    /// # 返回
    /// 返回按账号 ID 索引的展示姓名；已删除或不存在的账号不会进入结果。
    ///
    /// # 错误
    /// 账号仓储查询失败时返回仓储错误。
    async fn account_names(
        &self,
        account_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        let unique_ids = account_ids.iter().cloned().collect::<HashSet<_>>().into_iter().collect::<Vec<_>>();
        let accounts = self.db.accounts().list_by_ids(&unique_ids, executor).await?;
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

/// 映射销售列表行与正式版本金额，名称和阶段责任由批量查询补齐。
fn map_sales_row(
    row: SalesOrderRow,
    revision_amounts: &HashMap<String, SalesRevisionAmounts>,
) -> SalesOrderView {
    let (code, label, tone) = stage_code_label_tone(
        row.commercial_status,
        row.review_status,
        row.close_status,
        row.fulfillment_progress,
    );
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
        owner_user_id: row.sales_owner_user_id,
        owner_user_name: None,
        gross_amount: amounts.map_or_else(Amount::zero, |value| value.gross),
        net_amount: amounts.map_or_else(Amount::zero, |value| value.net),
        tax_amount: amounts.map_or_else(Amount::zero, |value| value.tax),
        stage: dto::SalesOrderStageSummary {
            code,
            label,
            tone,
            owner_role: None,
            owner_user_id: None,
            owner_user_name: None,
            due_at: None,
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
    /// # 参数
    /// q 为可选关键词，executor 为当前读取用例的执行器。
    /// # 返回
    /// 返回销售域中性的搜索条件；空关键词得到默认条件。
    /// # 错误
    /// 客户、主体或合同关联读取失败时整次查询失败。
    pub(crate) async fn keyword_search(
        &self,
        q: Option<&str>,
        executor: &mut dyn Executor,
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
            .matching_current_party_ids_by_name(&q, executor)
            .await?
            .into_iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>();
        let customer_ids = self.db.customer_accounts().matching_ids_by_parties(&party_ids, executor).await?;
        let contract_ids = self.db.contracts().matching_ids_by_number(&q, executor).await?;
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
