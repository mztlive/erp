//! 询价费用编排使用的窄持久化步骤，全部复用调用方的执行器。

use application_core::CommandIdentity;
use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_procurement::entity::purchase_order::{InquiryExpenseSource, PurchaseOrder};
use erp_sales::entity::sales_inquiry::SalesInquiry;
use erp_sales::service::sales_inquiry::SalesInquiryService;
use erp_supply::inquiry::{InquiryFeeBasis, InquiryFeeBasisData, InquirySupplyService};
use mongodb::Database;
use persistence_core::Executor;

use super::payee_basis_matches;
use crate::{Error, Result};

#[async_trait]
pub(super) trait ExpensePersistence: Sync {
    async fn inquiry(&self, id: &str, executor: &mut dyn Executor) -> Result<SalesInquiry>;
    async fn save(&self, inquiry: &mut SalesInquiry, executor: &mut dyn Executor) -> Result<()>;
    async fn fee_bases(
        &self,
        inquiry_id: &str,
        requirement_version: u32,
        round: u32,
        executor: &mut dyn Executor,
    ) -> Result<Vec<InquiryFeeBasis>>;
    async fn append_basis(&self, basis: &InquiryFeeBasis, executor: &mut dyn Executor) -> Result<()>;
}

pub(super) struct MongoExpensePersistence<'a> {
    db: &'a Database,
}

impl<'a> MongoExpensePersistence<'a> {
    pub(super) fn new(db: &'a Database) -> Self {
        Self { db }
    }
}

#[async_trait]
impl ExpensePersistence for MongoExpensePersistence<'_> {
    async fn inquiry(&self, id: &str, executor: &mut dyn Executor) -> Result<SalesInquiry> {
        Ok(SalesInquiryService::new(self.db.clone()).require(id, executor).await?)
    }

    async fn save(&self, inquiry: &mut SalesInquiry, executor: &mut dyn Executor) -> Result<()> {
        Ok(SalesInquiryService::new(self.db.clone()).save(inquiry, executor).await?)
    }

    async fn fee_bases(
        &self,
        inquiry_id: &str,
        requirement_version: u32,
        round: u32,
        executor: &mut dyn Executor,
    ) -> Result<Vec<InquiryFeeBasis>> {
        Ok(InquirySupplyService::new(self.db.clone())
            .fee_bases(inquiry_id, requirement_version, round, executor)
            .await?)
    }

    async fn append_basis(&self, basis: &InquiryFeeBasis, executor: &mut dyn Executor) -> Result<()> {
        Ok(InquirySupplyService::new(self.db.clone()).append_fee_basis(basis, executor).await?)
    }
}

pub(super) async fn validate_payee(
    persistence: &impl ExpensePersistence,
    inquiry: &SalesInquiry,
    source: &InquiryExpenseSource,
    order: &PurchaseOrder,
    executor: &mut dyn Executor,
) -> Result<()> {
    let feedback = inquiry.feedback_snapshot(source.feedback_round)?;
    let bases = persistence
        .fee_bases(&inquiry.base.id, feedback.requirement_version, feedback.round, executor)
        .await?;
    if !payee_basis_matches(&bases, &source.fee_id, order.supplier_id.as_ref(), &order.payment_term_code)? {
        return Err(Error::BusinessLogicError("本轮询价费用缺少已确认的收款依据".into()));
    }
    Ok(())
}

pub(super) async fn confirm_payee(
    persistence: &impl ExpensePersistence,
    inquiry: &SalesInquiry,
    source: &InquiryExpenseSource,
    order: &PurchaseOrder,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let feedback = inquiry.accepted_feedback(source.feedback_round, Instant::now())?;
    let bases = persistence
        .fee_bases(&inquiry.base.id, feedback.requirement_version, feedback.round, executor)
        .await?;
    if payee_basis_matches(&bases, &source.fee_id, order.supplier_id.as_ref(), &order.payment_term_code)? {
        return Ok(());
    }
    let basis = conversion_basis(inquiry, source, order, feedback.requirement_version, actor_id)?;
    persistence.append_basis(&basis, executor).await?;
    // 发生写入时保留原重读验证，确认唯一收款依据未发生冲突；无写入分支直接复用已核对结果。
    validate_payee(persistence, inquiry, source, order, executor).await
}

fn conversion_basis(
    inquiry: &SalesInquiry,
    source: &InquiryExpenseSource,
    order: &PurchaseOrder,
    requirement_version: u32,
    actor_id: &str,
) -> Result<InquiryFeeBasis> {
    let identity = CommandIdentity::new(
        "inquiry-fee-basis:",
        [
            inquiry.base.id.clone(),
            requirement_version.to_string(),
            source.feedback_round.to_string(),
            source.fee_id.clone(),
        ],
    )?;
    Ok(InquiryFeeBasis::new(
        identity.current_id().to_string(),
        InquiryFeeBasisData {
            inquiry_id: inquiry.base.id.clone(),
            requirement_version,
            feedback_round: source.feedback_round,
            fee_id: source.fee_id.clone(),
            payee_supplier_id: order.supplier_id.clone(),
            payment_term_code: order.payment_term_code.clone(),
            internal_note: "销售生效后的费用采购转换确认".into(),
            attachment_ids: vec![],
            recorded_by: actor_id.to_string(),
        },
    )?)
}
