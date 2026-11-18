//! 销售详情在原授权执行器中读取事实并组装冻结提交。
use std::collections::HashMap;

use erp_core::ids::{SalesOrderId, SalesOrderSubmissionId};
use erp_finance::entity::receivable::SalesOrderReceivableAmountSummary;
use erp_finance::repository::ReceivableExt;
use erp_finance::repository::prelude::*;
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::repository::prelude::*;
use erp_sales::dto::sales_order::{RevisionView, SalesOrderLineView, SubmissionView, WorkingCopyView};
use erp_sales::entity::sales_order::{
    SalesOrder, SalesOrderLine, SalesOrderSubmission, SalesOrderSubmissionLine, WorkingPurpose,
};
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;
use erp_sales::service::sales_order::mapper::submission_view;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::service::document_registry::find_approval_binding;
use persistence_core::Executor;

use super::SalesOrderReadService;
use crate::Result;

impl SalesOrderReadService {
    /// 在原执行器依次读取稳定行、提交、版本和采购数量。
    async fn load_detail_batch_a(
        &self,
        order_id: &SalesOrderId,
        executor: &mut dyn Executor,
    ) -> Result<(Vec<SalesOrderLine>, Vec<SalesOrderSubmission>, Vec<RevisionView>, u64)> {
        let lines = self.db.sales_order_lines().list_lines_by_order(order_id, executor).await?;
        let submissions =
            self.db.sales_order_submissions().list_by_order_newest_first(order_id, executor).await?;
        let revisions = self.sales().load_revision_views(order_id, executor).await?;
        let count = self.db.purchase_orders().count_active_by_sales_order(order_id, executor).await?;
        Ok((lines, submissions, revisions, count))
    }

    /// 在原执行器读取应收摘要、审批绑定和当前工作副本。
    async fn load_detail_batch_b(
        &self,
        order_id: &SalesOrderId,
        document_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<(SalesOrderReceivableAmountSummary, Option<ApprovalDefinitionBinding>, Option<WorkingCopyView>)>
    {
        let summary = self.db.receivable_accounts().sales_order_amount_summary(order_id, executor).await?;
        let binding = find_approval_binding(&self.db, document_id, executor).await.ok().flatten();

        let copy = self
            .db
            .sales_order_working_copies()
            .find_active_by_order_and_purpose(order_id, WorkingPurpose::FirstSubmission, executor)
            .await?;
        let view = match copy {
            Some(copy) => {
                let mut view = self.sales().working_copy_view(&copy, executor).await?;
                self.enrich_working_copy_prices(&mut view, executor).await?;
                Some(view)
            },
            None => None,
        };
        Ok((summary, binding, view))
    }

    /// 在原执行器加载详情事实，再按提交身份批量读取提交行。
    ///
    /// # 参数
    /// * `order` - 已授权的销售稳定单
    /// * `executor` - 同一授权事务的执行器。
    ///
    /// # 返回
    /// 返回稳定行、草稿视图、提交、版本视图、采购数、应收摘要与审批绑定。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    ///
    /// # 关键业务约束
    /// 全部事实使用同一快照，按顺序借用原执行器。
    pub(super) async fn load_detail_facts(
        &self,
        order: &SalesOrder,
        executor: &mut dyn Executor,
    ) -> Result<DetailFacts> {
        let order_id = SalesOrderId::new(order.base.id.clone());
        let (stable_lines, submissions, revisions, purchase_order_count) =
            self.load_detail_batch_a(&order_id, executor).await?;
        let (receivable_summary, binding, working_copy_view) =
            self.load_detail_batch_b(&order_id, &order.base.id, executor).await?;

        let submission_ids = submissions
            .iter()
            .map(|item| SalesOrderSubmissionId::new(item.base.id.clone()))
            .collect::<Vec<_>>();
        let submission_lines = self
            .db
            .sales_order_submission_lines()
            .list_lines_by_submissions(&submission_ids, executor)
            .await?;

        Ok(DetailFacts {
            stable_lines: stable_lines
                .into_iter()
                .map(|line| SalesOrderLineView {
                    id: line.base.id,
                    line_no: line.line_no,
                    line_status: line.line_status,
                })
                .collect(),
            working_copy_view,
            submissions: assemble_submission_views(submissions, submission_lines),
            revisions,
            purchase_order_count,
            receivable_summary,
            binding,
        })
    }
}

/// 已加载的详情事实；提交行已按提交身份归组并按行号排序。
pub(super) struct DetailFacts {
    pub(super) stable_lines: Vec<SalesOrderLineView>,
    pub(super) working_copy_view: Option<WorkingCopyView>,
    pub(super) submissions: Vec<SubmissionView>,
    pub(super) revisions: Vec<RevisionView>,
    pub(super) purchase_order_count: u64,
    pub(super) receivable_summary: SalesOrderReceivableAmountSummary,
    pub(super) binding: Option<ApprovalDefinitionBinding>,
}

/// 组装提交视图，纯内存分组排序不触库。
///
/// # 参数
/// * `submissions` - 已并行加载的提交头，新提交在前
/// * `lines` - 已按提交批量加载的提交行
///
/// # 返回
/// 返回按提交分组、行号排序的提交视图。
///
/// # 错误
/// 无。
fn assemble_submission_views(
    submissions: Vec<SalesOrderSubmission>,
    lines: Vec<SalesOrderSubmissionLine>,
) -> Vec<SubmissionView> {
    let mut lines_by_submission: HashMap<String, Vec<SalesOrderSubmissionLine>> = HashMap::new();
    for line in lines {
        lines_by_submission.entry(line.submission_id.to_string()).or_default().push(line);
    }

    submissions
        .into_iter()
        .map(|submission| {
            let mut lines = lines_by_submission.remove(&submission.base.id).unwrap_or_default();
            lines.sort_by_key(|line| line.line_no);
            submission_view(submission, lines)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use erp_core::common::stable::StableBase;
    use erp_core::common::time::Instant;
    use erp_core::ids::{CustomerAccountId, PartyId, SalesOrderLineId, SalesOrderWorkingCopyId};
    use erp_sales::entity::sales_order::{
        BusinessType, CustomerSnapshot, InvoiceRequirementSnapshot, LineType, PaymentTermSnapshot,
        SalesPricingMode, SubmissionStatus,
    };

    use super::*;

    fn submission(id: &str, submission_no: u32) -> SalesOrderSubmission {
        SalesOrderSubmission {
            base: BaseModel { id: id.into(), ..BaseModel::fake() },
            stable: StableBase::new(SubmissionStatus::Approved, "sales"),
            sales_order_id: SalesOrderId::new("order"),
            submission_no,
            working_copy_id: SalesOrderWorkingCopyId::new(format!("copy-{id}")),
            working_copy_version: 1,
            business_type: BusinessType::GoodsService,
            customer_id: CustomerAccountId::new("customer"),
            contract_revision_id: None,
            settlement_party_id: PartyId::new("party"),
            customer_snapshot: CustomerSnapshot { customer_name: format!("客户-{id}") },
            contract_snapshot: None,
            settlement_party_snapshot: None,
            payment_term_snapshot: PaymentTermSnapshot {
                payment_term_code: "NET30".into(),
                payment_term_name: "月结 30 天".into(),
            },
            invoice_requirement_snapshot: InvoiceRequirementSnapshot {
                invoice_type: "增值税专用发票".into(),
                tax_point: "6".into(),
            },
            project_name: None,
            business_remark: None,
            voucher_category_sku_id: None,
            voucher_expiry_at: None,
            receivable_due_date: None,
            gross_amount: "12.34".parse().unwrap(),
            net_amount: "11.64".parse().unwrap(),
            tax_amount: "0.70".parse().unwrap(),
            submitted_at: Instant::from_unix_secs(1_700_000_000),
            submitted_by: "sales".into(),
        }
    }

    fn line(submission_id: &str, id: &str, line_no: u32) -> SalesOrderSubmissionLine {
        SalesOrderSubmissionLine {
            base: BaseModel { id: id.into(), ..BaseModel::fake() },
            submission_id: SalesOrderSubmissionId::new(submission_id),
            sales_order_line_id: SalesOrderLineId::new(format!("stable-{id}")),
            line_no,
            line_type: LineType::GoodsService,
            gross_amount: "12.34".parse().unwrap(),
            net_amount: "11.64".parse().unwrap(),
            tax_amount: "0.70".parse().unwrap(),
            sales_tax_rate: "0.06".parse().unwrap(),
            item_name_snapshot: format!("商品-{id}"),
            spec_snapshot: None,
            unit_snapshot: None,
            sku_id: None,
            sku_revision_id: None,
            welfare_scenario: None,
            procurement_supply_price_gross: None,
            procurement_price_basis: None,
            fulfillment_due_at: None,
            quantity: None,
            base_unit_code: None,
            unit_price_gross: None,
            pricing_mode: SalesPricingMode::Manual,
            delivery_allocations: Vec::new(),
            face_value: None,
            card_count: None,
            face_value_total: None,
            transaction_amount: None,
            gift_amount: None,
            gift_rate: None,
            card_form: None,
        }
    }

    #[test]
    fn submission_views_preserve_header_order_and_sort_only_their_own_lines() {
        let views = assemble_submission_views(
            vec![submission("new", 3), submission("old", 2), submission("empty", 1)],
            vec![
                line("old", "old-2", 2),
                line("new", "new-2a", 2),
                line("unrelated", "unrelated-1", 1),
                line("new", "new-1", 1),
                line("old", "old-1", 1),
                line("new", "new-2b", 2),
            ],
        );

        assert_eq!(views.iter().map(|view| view.id.as_str()).collect::<Vec<_>>(), ["new", "old", "empty"]);
        assert_eq!(
            views[0].lines.iter().map(|line| line.id.as_str()).collect::<Vec<_>>(),
            ["new-1", "new-2a", "new-2b"]
        );
        assert_eq!(
            views[1].lines.iter().map(|line| line.id.as_str()).collect::<Vec<_>>(),
            ["old-1", "old-2"]
        );
        assert!(views[2].lines.is_empty());
        assert_eq!(views[0].customer_name, "客户-new");
        assert_eq!(views[0].lines[0].item_name_snapshot, "商品-new-1");
        assert_eq!(views[0].lines[0].gross_amount.to_string(), "12.34");
    }

    #[test]
    fn submission_views_are_empty_without_headers() {
        assert!(assemble_submission_views(Vec::new(), Vec::new()).is_empty());
        assert!(assemble_submission_views(Vec::new(), vec![line("unrelated", "line", 1)]).is_empty());
    }
}
