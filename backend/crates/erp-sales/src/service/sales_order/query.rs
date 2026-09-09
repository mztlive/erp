//! Sales-owned frozen snapshot queries.
use super::{
    mapper::{revision_view, working_copy_line_view},
    SalesOrderService,
};
use crate::dto::sales_order::{RevisionView, WorkingCopyView};
use crate::entity::sales_order::{SalesOrderRevision, SalesOrderRevisionLine, SalesOrderWorkingCopy};
use crate::repository::SalesOrderExt;
use crate::Result;
use erp_core::ids::{SalesOrderId, SalesOrderRevisionId};
use persistence_core::NoTransaction;
use std::collections::HashMap;
/// Group already loaded frozen rows once and retain line-number order within every revision.
fn group_revision_lines(lines: Vec<SalesOrderRevisionLine>) -> HashMap<String, Vec<SalesOrderRevisionLine>> {
    let mut grouped: HashMap<String, Vec<SalesOrderRevisionLine>> = HashMap::new();
    for line in lines {
        grouped
            .entry(line.sales_order_revision_id.to_string())
            .or_default()
            .push(line);
    }
    for group in grouped.values_mut() {
        group.sort_by_key(|line| line.line_no);
    }
    grouped
}

/// Resolve predecessor numbers solely from the loaded historical revisions, without current-data lookup.
fn previous_revision_numbers(revisions: &[SalesOrderRevision]) -> HashMap<String, u32> {
    let by_id: HashMap<&str, u32> = revisions
        .iter()
        .map(|row| (row.base.id.as_str(), row.revision.revision_no))
        .collect();
    let mut out = HashMap::new();
    for row in revisions {
        let Some(previous_id) = row.previous_revision_id.as_ref() else {
            continue;
        };
        let Some(previous_no) = by_id.get(previous_id.as_ref()) else {
            continue;
        };
        out.insert(row.base.id.clone(), *previous_no);
    }
    out
}

impl SalesOrderService {
    /// 组装销售单正式版本历史视图（含当时表头快照与明细摘要）。
    ///
    /// # 参数
    /// * `order_id` - 稳定销售单
    ///
    /// # 返回
    /// 返回按版本号倒序的版本视图。
    ///
    /// # 错误
    /// * `RepositoryError` - 查询正式版本或版本行失败
    ///
    /// # 关键业务约束
    /// 版本行按版本 ID 一次批量取出，禁止按版本循环查询。
    pub async fn load_revision_views(&self, order_id: &SalesOrderId) -> Result<Vec<RevisionView>> {
        let revisions = self
            .db
            .sales_order_revisions()
            .list_by_order(order_id, &mut NoTransaction)
            .await?;
        let revision_ids = revisions
            .iter()
            .map(|row| SalesOrderRevisionId::new(row.base.id.clone()))
            .collect::<Vec<_>>();
        let lines = self
            .db
            .sales_order_revision_lines()
            .list_lines_by_revisions(&revision_ids, &mut NoTransaction)
            .await?;
        let mut lines_by_revision = group_revision_lines(lines);
        let previous_nos = previous_revision_numbers(&revisions);
        Ok(revisions
            .into_iter()
            .map(|revision| {
                let lines = lines_by_revision.remove(&revision.base.id).unwrap_or_default();
                let previous_revision_no = previous_nos.get(&revision.base.id).copied();
                revision_view(revision, lines, previous_revision_no)
            })
            .collect())
    }

    /// 构建工作副本行视图。
    ///
    /// # 参数
    /// * `copy` - 工作副本实体
    ///
    /// # 返回
    /// 返回行视图集合。
    ///
    /// # 错误
    /// 数据库读取失败时返回错误。
    pub async fn working_copy_view(&self, copy: &SalesOrderWorkingCopy) -> Result<WorkingCopyView> {
        let lines = self
            .db
            .sales_order_working_copy_lines()
            .list_lines_by_working_copy(&copy.base.id.clone().into(), &mut NoTransaction)
            .await?;
        Ok(WorkingCopyView {
            id: copy.base.id.clone(),
            version: copy.base.version,
            working_purpose: copy.working_purpose,
            status: copy.stable.status,
            draft_version: copy.draft_version,
            content_hash: copy.content_hash.clone(),
            editor_user_id: copy.editor_user_id.clone(),
            business_type: copy.business_type,
            customer_name: copy.customer_snapshot.customer_name.clone(),
            contract_no: copy.contract_snapshot.as_ref().map(|s| s.contract_no.clone()),
            contract_revision_id: copy.contract_revision_id.as_ref().map(ToString::to_string),
            settlement_party_name: copy
                .settlement_party_snapshot
                .as_ref()
                .map(|s| s.settlement_party_name.clone()),
            payment_term_code: copy.payment_term_snapshot.payment_term_code.clone(),
            payment_term_name: copy.payment_term_snapshot.payment_term_name.clone(),
            invoice_type: copy.invoice_requirement_snapshot.invoice_type.clone(),
            tax_point: copy.invoice_requirement_snapshot.tax_point.clone(),
            project_name: copy.project_name.clone(),
            business_remark: copy.business_remark.clone(),
            voucher_category_sku_id: copy.voucher_category_sku_id.as_ref().map(ToString::to_string),
            voucher_expiry_at: copy.voucher_expiry_at.map(|instant| instant.unix_secs() as u64),
            receivable_due_date: copy.receivable_due_date,
            gross_amount: copy.gross_amount,
            net_amount: copy.net_amount,
            tax_amount: copy.tax_amount,
            lines: lines.into_iter().map(working_copy_line_view).collect(),
        })
    }
}

impl SalesOrderService {
    /// Search sales rows with the original validated filters and pagination.
    ///
    /// Cross-domain stage owners and account names are composed by the read model.
    pub async fn list_rows(
        &self,
        params: &crate::dto::sales_order::SalesOrderListParams,
        search: crate::repository::sales_order::SalesOrderSearch,
    ) -> crate::Result<application_core::PageView<crate::repository::sales_order::SalesOrderRow>> {
        use validator::Validate;
        type SalesOrderFilter = <mongodb::Database as crate::repository::SalesOrderExt>::SalesOrderFilter;
        params.validate()?;
        let query = params.normalized()?;
        let filter = SalesOrderFilter {
            search,
            order_no: query.order_no,
            customer_id: query.customer_id,
            contract_id: query.contract_id,
            origin_system: query.origin_system,
            commercial_status: query.commercial_status,
            review_status: query.review_status,
            business_type: query.business_type,
            fulfillment_progress: query.fulfillment_progress,
            collection_progress: query.collection_progress,
            invoice_progress: query.invoice_progress,
            close_status: query.close_status,
            created_from: query.created_from,
            created_to: query.created_to,
            created_by: query.created_by,
            my_todo: query.my_todo,
            exception_only: query.exception_only,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, crate::dto::sales_order::SortDir::Asc),
        };
        let page = self
            .db
            .sales_orders()
            .search_sales_orders(&filter, &mut NoTransaction)
            .await?;

        Ok(application_core::PageView {
            items: page.items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }
}
