use crate::entity::supplier_settlement::{
    SettlementStatus, SupplierSettlementDifference, SupplierSettlementItem, SupplierSettlementStatement,
};
use crate::repository::SupplierSettlementExt;
use persistence_core::NoTransaction;
use validator::Validate;

use super::{dto, zero_amount, SupplierSettlementService};
use crate::dto::supplier_fulfillment::SortDir;
use crate::dto::supplier_settlement::{
    StatementListQuery, SupplierSettlementItemView, SupplierSettlementStatementListParams,
    SupplierSettlementStatementListView, SupplierSettlementStatementView,
};
use crate::Result;

/// 结算单列表筛选条件类型（经 `SupplierSettlementExt` 关联类型跨 crate 可达）。
type StatementFilter = <mongodb::Database as SupplierSettlementExt>::SupplierSettlementStatementFilter;

impl SupplierSettlementService {
    /// 分页查询供应商结算单列表。
    ///
    /// 排序字段白名单在 Service 层校验（api-contract §4），禁止任意字段透传。
    ///
    /// # 参数
    /// * `params` - 查询参数（`statement_no`/`supplier_id`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn supplier_settlement_statement_list(
        &self,
        params: &SupplierSettlementStatementListParams,
    ) -> Result<SupplierSettlementStatementListView> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = statement_filter(&query);
        let page = self
            .db
            .supplier_settlement_statements()
            .search_supplier_settlement_statements(&filter, &mut NoTransaction)
            .await?;
        let stats = self
            .db
            .supplier_settlement_statements()
            .aggregate_supplier_settlement_statement_stats(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| SupplierSettlementStatementView {
                id: row.id,
                statement_no: row.statement_no,
                supplier_id: row.supplier_id.to_string(),
                period_start: row.period_start.to_string(),
                period_end: row.period_end.to_string(),
                period_policy_id: row.period_policy_id,
                period_policy_version: row.period_policy_version,
                period_timezone: row.period_timezone,
                external_bill_no: row.external_bill_no,
                external_bill_version: row.external_bill_version,
                erp_amount: row.erp_amount,
                supplier_amount: row.supplier_amount,
                difference_amount: row.difference_amount,
                status: row.status,
                subject_hash: row.subject_hash,
                source_as_of: row.source_as_of.unix_secs(),
                source_snapshot_at: row.source_snapshot_at.unix_secs(),
                source_snapshot_hash: row.source_snapshot_hash,
                refresh_cutoff_policy_id: row.refresh_cutoff_policy_id,
                refresh_cutoff_policy_version: row.refresh_cutoff_policy_version,
                prepared_by: row.prepared_by,
                reviewed_by: row.reviewed_by,
                review_result: row.review_result,
                review_reason_code: row.review_reason_code,
                review_comment: row.review_comment,
                reviewed_at: row.reviewed_at.map(|time| time.unix_secs()),
                confirmed_at: row.confirmed_at.map(|t| t.unix_secs()),
                payable_account_id: row.payable_account_id.map(|id| id.to_string()),
                version: row.version,
                created_at: row.created_at,
            })
            .collect();

        let stats = stats.map_or_else(
            || dto::SettlementStatementListStatsView {
                pending_reconciliation_count: 0,
                has_difference_count: 0,
                pending_review_count: 0,
                confirmed_amount: zero_amount(),
            },
            |stats| dto::SettlementStatementListStatsView {
                pending_reconciliation_count: stats.pending_reconciliation_count,
                has_difference_count: stats.has_difference_count,
                pending_review_count: stats.pending_review_count,
                confirmed_amount: stats.confirmed_amount,
            },
        );
        Ok(SupplierSettlementStatementListView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
            stats,
            processing_state: if page.total == 0 { "EMPTY" } else { "READY" }.to_string(),
        })
    }
}

/// 构建结算单列表筛选条件。
///
/// # 参数
/// * `query` - 归一化查询参数
///
/// # 返回
/// 返回仓储筛选条件。
fn statement_filter(query: &StatementListQuery) -> StatementFilter {
    StatementFilter {
        statement_no: query.statement_no.clone(),
        supplier_id: query.supplier_id.clone(),
        status: query.status,
        period_from: query.period_from,
        period_to: query.period_to,
        page: query.paging.page,
        page_size: query.paging.page_size,
        sort_by: Some(query.paging.sort_by.to_string()),
        sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
    }
}

/// 从结算明细实体构造响应视图。
///
/// # 参数
/// * `item` - 结算明细实体
///
/// # 返回
/// 返回响应视图。
pub fn settlement_item_view(item: SupplierSettlementItem) -> SupplierSettlementItemView {
    SupplierSettlementItemView {
        id: item.base.id,
        statement_id: item.statement_id.to_string(),
        supplier_fulfillment_order_id: item.supplier_fulfillment_order_id.to_string(),
        supplier_fulfillment_item_id: item.supplier_fulfillment_item_id.to_string(),
        quantity: item.quantity,
        order_amount: item.order_amount,
        freight_amount: item.freight_amount,
        service_fee_amount: item.service_fee_amount,
        refund_amount: item.refund_amount,
        erp_calculated_amount: item.erp_calculated_amount,
        erp_calculated_net_amount: item.erp_calculated_net_amount,
        erp_calculated_tax_amount: item.erp_calculated_tax_amount,
        supplier_billed_amount: item.supplier_billed_amount,
        supplier_billed_net_amount: item.supplier_billed_net_amount,
        supplier_billed_tax_amount: item.supplier_billed_tax_amount,
        created_at: item.base.created_at,
    }
}

/// 由服务端状态、责任人与差异事实投影当前对象动作和处理态。
pub fn settlement_object_actions(
    statement: &SupplierSettlementStatement,
    differences: &[SupplierSettlementDifference],
    actor_id: &str,
) -> (Vec<String>, Vec<dto::SettlementReviewActionBlockerView>, String) {
    let editable = statement.is_editable();
    let pending = differences
        .iter()
        .filter(|difference| difference.is_pending())
        .count();
    let processing_state = if editable && pending == 0 {
        "READY_FOR_REVIEW"
    } else {
        match statement.status {
            SettlementStatus::Draft
            | SettlementStatus::PendingReconciliation
            | SettlementStatus::HasDifference => "EVIDENCE_OR_DECISION_REQUIRED",
            SettlementStatus::PendingReview => "REVIEW_PENDING",
            SettlementStatus::Confirmed => "COMPLETED",
            SettlementStatus::Voided => "VOIDED",
        }
    }
    .to_string();
    let mut actions = Vec::new();
    let mut blockers = Vec::new();
    if editable {
        actions.push("APPEND_EVIDENCE".to_string());
    }
    if editable && statement.is_prepared_by(actor_id) {
        actions.extend([
            "REFRESH_TRIAL".to_string(),
            "RESOLVE_DIFFERENCE".to_string(),
            "VOID_DRAFT".to_string(),
        ]);
        if pending == 0 {
            actions.push("SUBMIT_REVIEW".to_string());
        } else {
            blockers.push(super::review::review_blocker(
                "SUBMIT_REVIEW",
                "PENDING_DIFFERENCES",
                "存在未处理差异，禁止提交财务复核",
            ));
        }
    } else if editable {
        for action in [
            "REFRESH_TRIAL",
            "RESOLVE_DIFFERENCE",
            "VOID_DRAFT",
            "SUBMIT_REVIEW",
        ] {
            blockers.push(super::review::review_blocker(
                action,
                "PREPARER_REQUIRED",
                "该动作仅允许当前结算经办人执行",
            ));
        }
    } else {
        blockers.push(super::review::review_blocker(
            "EDIT_DRAFT",
            "STATEMENT_NOT_EDITABLE",
            "当前结算状态不允许刷新、补证或处理差异",
        ));
    }
    (actions, blockers, processing_state)
}
