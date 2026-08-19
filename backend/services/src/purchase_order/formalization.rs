//! 采购版本形成、版本号与变更差额构造。

use database::{NoTransaction, PurchaseOrderExt, SalesReviewExt};
use entities::common::time::Instant;
use entities::ids::{PayableEntryId, PurchaseOrderRevisionId, PurchaseOrderRevisionLineId};
use entities::money::Amount;
use entities::purchase_order::{
    PurchaseChangeSubmission, PurchaseChangeSubmissionLine, PurchaseLineType, PurchaseOrder,
    PurchaseOrderRevision, PurchaseOrderRevisionData, PurchaseOrderRevisionLine,
    PurchaseOrderRevisionLineData, PurchaseOrderSubmission, PurchaseOrderSubmissionLine,
};
use id_generator::next_id;

use super::shared::zero_amount;
use super::PurchaseOrderService;
use crate::errors::{Error, Result};

impl PurchaseOrderService {
    /// 计算下一个版本号（同一采购单内从 1 递增）。
    pub(super) async fn next_revision_no(&self, order: &PurchaseOrder) -> Result<u32> {
        let existing = self
            .db
            .purchase_order_revisions()
            .find_many(
                mongodb::bson::doc! { "purchase_order_id": order.base.id.clone() },
                &mut NoTransaction,
            )
            .await?;
        Ok(existing
            .iter()
            .map(|revision| revision.revision.revision_no)
            .max()
            .unwrap_or(0)
            + 1)
    }

    /// 形成生效版本与版本行（§8.1.4 复制已通过提交）。
    ///
    /// 说明：`purchase_line_sales_allocation` 的 Data 类型未从实体层导出
    /// （entities 冻结），分配写入本阶段无法构造实体，已在报告中提出；
    /// 版本行保留销售提交行引用与分配数量，供入库预占沿分配关系回查。
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn build_effective_revision(
        &self,
        order: &PurchaseOrder,
        submission: &PurchaseOrderSubmission,
        submission_lines: &[PurchaseOrderSubmissionLine],
        revision_no: u32,
    ) -> Result<(PurchaseOrderRevision, Vec<PurchaseOrderRevisionLine>)> {
        let _ = submission_lines;
        let revision = PurchaseOrderRevision::new(
            PurchaseOrderRevisionId::new(next_id()),
            PurchaseOrderRevisionData {
                purchase_order_id: order.base.id.clone().into(),
                revision_no,
                supplier_revision_id: submission.supplier_revision_id.clone(),
                supplier_snapshot: submission.supplier_snapshot.clone(),
                payment_term_snapshot: submission.payment_term_snapshot.clone(),
                gross_amount: submission.gross_amount,
                net_amount: submission.net_amount,
                tax_amount: submission.tax_amount,
                effective_at: Instant::now(),
            },
        )?;
        let mut revision_lines = Vec::with_capacity(submission_lines.len());
        for line in submission_lines {
            revision_lines.push(PurchaseOrderRevisionLine::new(
                PurchaseOrderRevisionLineId::new(next_id()),
                PurchaseOrderRevisionLineData {
                    purchase_order_revision_id: revision.base.id.clone().into(),
                    line_no: line.line_no,
                    line_type: line.line_type,
                    procurement_confirmation_line_id: line.procurement_confirmation_line_id.clone(),
                    sku_id: line.sku_id.clone(),
                    sku_revision_id: line.sku_revision_id.clone(),
                    product_name_snapshot: line.product_name_snapshot.clone(),
                    specification_snapshot: line.specification_snapshot.clone(),
                    quantity: line.quantity,
                    base_unit_code: line.base_unit_code.clone(),
                    unit_cost_gross: line.unit_cost_gross,
                    gross_amount: line.gross_amount,
                    net_amount: line.net_amount,
                    tax_amount: line.tax_amount,
                    input_tax_rate: line.input_tax_rate,
                    expected_delivery_date: line.expected_delivery_date,
                },
            )?);
        }
        Ok((revision, revision_lines))
    }

    /// 形成变更生效版本与版本行。
    pub(super) async fn build_change_revision(
        &self,
        order: &PurchaseOrder,
        submission: &PurchaseChangeSubmission,
        lines: &[PurchaseChangeSubmissionLine],
        revision_no: u32,
    ) -> Result<(PurchaseOrderRevision, Vec<PurchaseOrderRevisionLine>)> {
        let revision = PurchaseOrderRevision::new(
            PurchaseOrderRevisionId::new(next_id()),
            PurchaseOrderRevisionData {
                purchase_order_id: order.base.id.clone().into(),
                revision_no,
                supplier_revision_id: submission.supplier_revision_id.clone(),
                supplier_snapshot: submission.supplier_snapshot.clone(),
                payment_term_snapshot: submission.payment_term_snapshot.clone(),
                gross_amount: submission.gross_amount,
                net_amount: submission.net_amount,
                tax_amount: submission.tax_amount,
                effective_at: Instant::now(),
            },
        )?;
        let mut revision_lines = Vec::with_capacity(lines.len());
        for line in lines {
            revision_lines.push(PurchaseOrderRevisionLine::new(
                PurchaseOrderRevisionLineId::new(next_id()),
                PurchaseOrderRevisionLineData {
                    purchase_order_revision_id: revision.base.id.clone().into(),
                    line_no: line.line_no,
                    line_type: line.line_type,
                    procurement_confirmation_line_id: line.procurement_confirmation_line_id.clone(),
                    sku_id: line.sku_id.clone(),
                    sku_revision_id: line.sku_revision_id.clone(),
                    product_name_snapshot: line.product_name_snapshot.clone(),
                    specification_snapshot: line.specification_snapshot.clone(),
                    quantity: line.quantity,
                    base_unit_code: line.base_unit_code.clone(),
                    unit_cost_gross: line.unit_cost_gross,
                    gross_amount: line.gross_amount,
                    net_amount: line.net_amount,
                    tax_amount: line.tax_amount,
                    input_tax_rate: line.input_tax_rate,
                    expected_delivery_date: line.expected_delivery_date,
                },
            )?);
        }
        Ok((revision, revision_lines))
    }

    /// 构建变更差额（应付差额分录 + `CONFIRMED` 差额成本事实）。
    pub(super) async fn build_change_deltas(
        &self,
        order: &PurchaseOrder,
        base_revision: &PurchaseOrderRevision,
        new_revision: &PurchaseOrderRevision,
    ) -> Result<(
        Option<(entities::payable::PayableAccount, entities::payable::PayableEntry)>,
        Vec<entities::cost::CostEntry>,
    )> {
        let delta_amount = Amount::try_from(
            new_revision.gross_amount.to_decimal() - base_revision.gross_amount.to_decimal(),
        )
        .expect("金额差值小数位不超过 2 位");
        let payable_delta = if delta_amount.to_decimal() != zero_amount().to_decimal() {
            let account = entities::payable::PayableAccount::new(
                entities::ids::PayableAccountId::new(next_id()),
                entities::payable::PayableAccountData {
                    source_document_id: order.base.id.clone(),
                    supplier_id: order.supplier_id.clone(),
                    source_type: entities::payable::PayableSourceType::PurchaseOrder,
                    gross_total: delta_amount,
                    settled_total: zero_amount(),
                    invoiceable_total: delta_amount,
                    invoiced_total: zero_amount(),
                },
                "system",
            )?;
            let entry = entities::payable::PayableEntry::new(
                PayableEntryId::new(next_id()),
                entities::payable::PayableEntryData {
                    payable_account_id: account.base.id.clone().into(),
                    entry_type: entities::payable::PayableEntryType::ChangeDelta,
                    direction: if delta_amount.to_decimal() > zero_amount().to_decimal() {
                        entities::payable::EntryDirection::Increase
                    } else {
                        entities::payable::EntryDirection::Decrease
                    },
                    amount: Amount::try_from(delta_amount.to_decimal().abs())
                        .expect("差额绝对值小数位不超过 2 位"),
                    due_date: entities::common::time::BusinessDate::today(),
                    source_fact_type: "purchase_change_order".to_string(),
                    source_document_id: order.base.id.clone(),
                    source_revision_id: new_revision.base.id.clone(),
                    source_sequence: 1,
                    posted_at: Instant::now(),
                },
            )?;
            Some((account, entry))
        } else {
            None
        };
        Ok((payable_delta, Vec::new()))
    }
}
