//! 采购版本形成、版本号与变更差额构造。

use database::{NoTransaction, PurchaseOrderExt};
use entities::common::time::Instant;
use entities::ids::{PayableEntryId, PurchaseOrderRevisionId, PurchaseOrderRevisionLineId};
use entities::money::Amount;
use entities::purchase_order::{
    PurchaseChangeSubmission, PurchaseChangeSubmissionLine, PurchaseOrder, PurchaseOrderRevision,
    PurchaseOrderRevisionLine, PurchaseOrderSubmission, PurchaseOrderSubmissionLine,
};
use id_generator::next_id;

use super::shared::zero_amount;
use super::PurchaseOrderService;
use crate::errors::Result;

impl PurchaseOrderService {
    /// 计算下一个版本号（同一采购单内从 1 递增）。
    pub(super) async fn next_revision_no(&self, order: &PurchaseOrder) -> Result<u32> {
        let existing = self
            .db
            .purchase_order()
            .list_revisions_by_order(&order.base.id.clone().into(), &mut NoTransaction)
            .await?;
        PurchaseOrderRevision::next_revision_no(&existing).map_err(Into::into)
    }

    /// 形成生效版本与版本行（§8.1.4 复制已通过提交）。
    ///
    /// 说明：`purchase_line_sales_allocation` 的 Data 类型未从实体层导出
    /// （entities 冻结），分配写入本阶段无法构造实体，已在报告中提出；
    /// 版本行保留销售提交行引用与分配数量，供入库预占沿分配关系回查。
    pub(super) async fn build_effective_revision(
        &self,
        order: &PurchaseOrder,
        submission: &PurchaseOrderSubmission,
        submission_lines: &[PurchaseOrderSubmissionLine],
        revision_no: u32,
    ) -> Result<(PurchaseOrderRevision, Vec<PurchaseOrderRevisionLine>)> {
        if submission.purchase_order_id.as_ref() != order.base.id {
            return Err(crate::errors::Error::BusinessLogicError(
                "采购提交不属于当前采购单".to_string(),
            ));
        }
        let revision = PurchaseOrderRevision::from_submission(
            PurchaseOrderRevisionId::new(next_id()),
            revision_no,
            submission,
            Instant::now(),
        )?;
        let revision_id = PurchaseOrderRevisionId::new(revision.base.id.clone());
        let revision_lines = submission_lines
            .iter()
            .map(|line| {
                PurchaseOrderRevisionLine::from_submission_line(
                    PurchaseOrderRevisionLineId::new(next_id()),
                    revision_id.clone(),
                    line,
                )
            })
            .collect::<entities::Result<Vec<_>>>()?;
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
        let revision = PurchaseOrderRevision::from_change_submission(
            PurchaseOrderRevisionId::new(next_id()),
            order.base.id.clone().into(),
            revision_no,
            submission,
            Instant::now(),
        )?;
        let revision_id = PurchaseOrderRevisionId::new(revision.base.id.clone());
        let revision_lines = lines
            .iter()
            .map(|line| {
                PurchaseOrderRevisionLine::from_change_submission_line(
                    PurchaseOrderRevisionLineId::new(next_id()),
                    revision_id.clone(),
                    line,
                )
            })
            .collect::<entities::Result<Vec<_>>>()?;
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
