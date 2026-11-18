use std::collections::BTreeSet;

use application_core::{EmptyScopeVersion, ScopeVersion};
use erp_core::ids::SupplierAccountId;
use erp_permissions::supplier_settlement;
use persistence_core::Executor;
use validator::Validate;

use super::{SettlementActionAccess, SupplierSettlementService, dto, zero_amount};
use crate::authorization::CompiledScope;
use crate::dto::supplier_fulfillment::SortDir;
use crate::dto::supplier_settlement::{
    StatementListQuery, SupplierSettlementItemView, SupplierSettlementStatementListParams,
    SupplierSettlementStatementListView, SupplierSettlementStatementView,
};
use crate::entity::supplier_settlement::{
    SettlementStatus, SupplierSettlementDifference, SupplierSettlementItem, SupplierSettlementStatement,
};
use crate::repository::SupplierSettlementExt;
use crate::repository::prelude::*;
use crate::repository::supplier_settlement::{
    SupplierSettlementStatementFilter, SupplierSettlementStatementRow, SupplierSettlementStatementStatsRow,
};
use crate::{Error, Result};

impl SupplierSettlementService {
    /// 用原执行器校验固定列表动作和范围版本，将授权、组织及工作项条件用于同一分页和统计。后续页缺版本或
    /// 版本失效时拒绝；交付前由身份域复验凭证。范围为空返回空集，组织展开或数据库异常不当作空结果。
    pub async fn statement_list_scoped(
        &self,
        params: &SupplierSettlementStatementListParams,
        keyword_supplier_ids: Vec<SupplierAccountId>,
        context: &CompiledScope,
        handler_open_statement_ids: Vec<String>,
        executor: &mut dyn Executor,
    ) -> Result<SupplierSettlementStatementListView> {
        params.validate()?;
        let query = params.normalized()?;
        ensure_page(query.paging.page, params.scope_version.as_deref())?;

        context.ensure_permission(&supplier_settlement::STATEMENT_LIST, executor)?;
        ensure_scope_version(params.scope_version.as_deref(), &context.scope_version)?;

        let org_ids = expand_org_filter(
            &self.access(),
            query.org_unit_ids.as_ref().map(|ids| ids.as_slice()),
            query.include_descendants,
            executor,
        )
        .await?;
        let mut filter = statement_filter(&query);
        filter.keyword_supplier_ids = keyword_supplier_ids;
        filter.authorized_scope = Some(context.clone());
        filter.handler_open_statement_ids = handler_open_statement_ids;
        filter.business_org_unit_ids = org_ids.map(|ids| ids.into_iter().collect());

        let mut view = self.load_statement_page(&filter, context, executor).await?;
        context.validate(executor).await?;
        view.empty_reason = None;
        Ok(view)
    }
}

// 后续页必须携带当前范围版本。
fn ensure_page(page: u64, version: Option<&str>) -> Result<()> {
    ScopeVersion::new(version, EmptyScopeVersion::Missing)
        .require_for_page(page)
        .map_err(|error| Error::ConflictError(format!("DATA_SCOPE_CHANGED：{}", error.detail())))
}

// 客户端回传的范围版本必须与当前快照一致。
fn ensure_scope_version(expected: Option<&str>, actual: &str) -> Result<()> {
    ScopeVersion::new(expected, EmptyScopeVersion::Provided)
        .verify(actual)
        .map_err(|error| Error::ConflictError(format!("DATA_SCOPE_CHANGED：{}", error.detail())))
}

async fn expand_org_filter(
    access: &super::SettlementAccess,
    org_unit_ids: Option<&[String]>,
    include_descendants: bool,
    executor: &mut dyn Executor,
) -> Result<Option<BTreeSet<String>>> {
    let Some(ids) = org_unit_ids.filter(|ids| !ids.is_empty()) else {
        return Ok(None);
    };
    Ok(Some(access.expand_org_units(ids, include_descendants, executor).await?))
}

impl SupplierSettlementService {
    async fn load_statement_page(
        &self,
        filter: &SupplierSettlementStatementFilter,
        context: &CompiledScope,
        executor: &mut dyn Executor,
    ) -> Result<SupplierSettlementStatementListView> {
        let page = self
            .db
            .supplier_settlement_statements()
            .search_supplier_settlement_statements(filter, executor)
            .await?;
        let stats = self
            .db
            .supplier_settlement_statements()
            .aggregate_supplier_settlement_statement_stats(filter, executor)
            .await?;

        let items = page.items.into_iter().map(statement_row_view).collect::<Vec<_>>();
        Ok(list_view(items, page.total, filter, stats, context))
    }
}

// 消费列表投影构造视图，仅缺省差异处理人时复制对账负责人。
fn statement_row_view(row: SupplierSettlementStatementRow) -> SupplierSettlementStatementView {
    let difference_handler_user_id = if row.difference_handler_user_id.is_empty() {
        row.prepared_by.clone()
    } else {
        row.difference_handler_user_id
    };
    SupplierSettlementStatementView {
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
        business_org_unit_id: row.business_org_unit_id,
        difference_handler_user_id,
        reviewed_by: row.reviewed_by,
        review_result: row.review_result,
        review_reason_code: row.review_reason_code,
        review_comment: row.review_comment,
        reviewed_at: row.reviewed_at.map(|time| time.unix_secs()),
        confirmed_at: row.confirmed_at.map(|t| t.unix_secs()),
        payable_account_id: row.payable_account_id.map(|id| id.to_string()),
        version: row.version,
        created_at: row.created_at,
    }
}

fn list_view(
    items: Vec<SupplierSettlementStatementView>,
    total: i64,
    filter: &SupplierSettlementStatementFilter,
    stats: Option<SupplierSettlementStatementStatsRow>,
    context: &CompiledScope,
) -> SupplierSettlementStatementListView {
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
    SupplierSettlementStatementListView {
        items,
        total,
        page: filter.page,
        page_size: filter.page_size,
        stats,
        processing_state: if total == 0 { "EMPTY" } else { "READY" }.to_string(),
        scope_version: context.scope_version.clone(),
        policy_version: context.policy_version(),
        organization_version: context.organization_version(),
        as_of: context.as_of().as_utc().to_rfc3339(),
        empty_reason: None,
        scope_summary: "结算单按岗位权限共享；负责人筛选为额外收窄",
        ownership_basis: "settlement_prepared_by",
    }
}

// 构建结算单列表筛选条件。
fn statement_filter(query: &StatementListQuery) -> SupplierSettlementStatementFilter {
    SupplierSettlementStatementFilter {
        q: query.q.clone(),
        keyword_supplier_ids: Vec::new(),
        statement_no: query.statement_no.clone(),
        supplier_id: query.supplier_id.clone(),
        status: query.status,
        period_from: query.period_from,
        period_to: query.period_to,
        authorized_scope: None,
        owner_user_ids: query.owner_user_ids.as_ref().map(|ids| ids.as_slice().to_vec()),
        operator_user_ids: query.operator_user_ids.as_ref().map(|ids| ids.as_slice().to_vec()),
        handler_user_ids: query.handler_user_ids.as_ref().map(|ids| ids.as_slice().to_vec()),
        handler_open_statement_ids: Vec::new(),
        business_org_unit_ids: None,
        page: query.paging.page,
        page_size: query.paging.page_size,
        sort_by: Some(query.paging.sort_by.to_string()),
        sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
    }
}

/// 从结算明细实体构造响应视图。
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
/// 返回允许的动作名、阻塞原因，以及处理态文本。
pub fn settlement_object_actions(
    statement: &SupplierSettlementStatement,
    differences: &[SupplierSettlementDifference],
    access: &SettlementActionAccess,
) -> (Vec<String>, Vec<dto::SettlementReviewActionBlockerView>, String) {
    let pending = differences.iter().filter(|difference| difference.is_pending()).count();
    let mut actions = Vec::new();
    let mut blockers = Vec::new();
    extend_edit_actions(statement, access, pending, &mut actions, &mut blockers);
    (actions, blockers, settlement_processing_state(statement, pending))
}

fn settlement_processing_state(statement: &SupplierSettlementStatement, pending: usize) -> String {
    if statement.is_editable() && pending == 0 {
        return "READY_FOR_REVIEW".into();
    }
    match statement.status {
        SettlementStatus::Draft
        | SettlementStatus::PendingReconciliation
        | SettlementStatus::HasDifference => "EVIDENCE_OR_DECISION_REQUIRED",
        SettlementStatus::PendingReview => "REVIEW_PENDING",
        SettlementStatus::Confirmed => "COMPLETED",
        SettlementStatus::Voided => "VOIDED",
    }
    .into()
}

fn extend_edit_actions(
    statement: &SupplierSettlementStatement,
    access: &SettlementActionAccess,
    pending: usize,
    actions: &mut Vec<String>,
    blockers: &mut Vec<dto::SettlementReviewActionBlockerView>,
) {
    if !statement.is_editable() {
        blockers.push(super::review::review_blocker(
            "EDIT_DRAFT",
            "STATEMENT_NOT_EDITABLE",
            "当前结算状态不允许刷新、补证或处理差异",
        ));
        return;
    }

    if access.append_evidence {
        actions.push("APPEND_EVIDENCE".into());
    } else {
        blockers.push(super::review::review_blocker(
            "APPEND_EVIDENCE",
            "ACTION_NOT_AUTHORIZED",
            "当前账号无权追加证据",
        ));
    }
    extend_preparer_actions(access, pending, actions, blockers);

    if access.resolve_difference {
        actions.push("RESOLVE_DIFFERENCE".into());
    } else {
        blockers.push(super::review::review_blocker(
            "RESOLVE_DIFFERENCE",
            "DIFFERENCE_HANDLER_REQUIRED",
            "该动作仅允许当前差异处理人执行",
        ));
    }
}

fn extend_preparer_actions(
    access: &SettlementActionAccess,
    pending: usize,
    actions: &mut Vec<String>,
    blockers: &mut Vec<dto::SettlementReviewActionBlockerView>,
) {
    for (action, allowed) in [("REFRESH_TRIAL", access.refresh), ("VOID_DRAFT", access.void)] {
        if allowed {
            actions.push(action.into());
        } else {
            blockers.push(super::review::review_blocker(
                action,
                "ACTION_NOT_AUTHORIZED",
                "当前账号无权执行该动作",
            ));
        }
    }

    if !access.submit {
        blockers.push(super::review::review_blocker(
            "SUBMIT_REVIEW",
            "ACTION_NOT_AUTHORIZED",
            "当前账号无权提交财务复核",
        ));
    } else if pending == 0 {
        actions.push("SUBMIT_REVIEW".into());
    } else {
        blockers.push(super::review::review_blocker(
            "SUBMIT_REVIEW",
            "PENDING_DIFFERENCES",
            "存在未处理差异，禁止提交财务复核",
        ));
    }
}
